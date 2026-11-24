//! Per-user Windows logon registration. No service or periodic polling is used.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::*, System::Registry::*, UI::Shell::ShellExecuteW,
        UI::WindowsAndMessaging::SW_SHOWNORMAL,
    },
    core::*,
};

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVAL: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const NAME: &str = "Dynamic Island For Windows";
const EXE: &str = "dynamic-island-for-windows.exe";

#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct Status {
    pub registered: bool,
    pub blocked_by_windows: bool,
    pub error: bool,
}
impl Status {
    pub fn enabled(self) -> bool {
        self.registered && !self.blocked_by_windows
    }
    pub fn detail(self) -> &'static str {
        if self.error {
            "Couldn't update startup · tap to retry"
        } else if self.blocked_by_windows && self.registered {
            "Disabled in Windows · tap to manage"
        } else {
            "Open automatically when you sign in"
        }
    }
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn read_at(path: &str, flags: REG_ROUTINE_FLAGS) -> Result<Option<Vec<u8>>> {
    unsafe {
        let mut size = 0;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(NAME),
            flags,
            None,
            None,
            Some(&mut size),
        );
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        status.ok()?;
        if size > 4096 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let mut data = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(NAME),
            flags,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()?;
        data.truncate(size as usize);
        Ok(Some(data))
    }
}
fn registered_at(path: &str) -> Result<bool> {
    Ok(read_at(path, RRF_RT_REG_SZ)?.is_some())
}
// Read-only compatibility hint for desktop Startup Apps. Unknown states defer
// to Windows; this app never writes or removes StartupApproved preferences.
fn approval_blocked(data: Option<&[u8]>) -> bool {
    match data {
        None => false,
        Some(v) if v.len() >= 4 => !matches!(u32::from_le_bytes(v[..4].try_into().unwrap()), 2 | 6),
        Some(_) => true,
    }
}
pub fn query() -> Status {
    match registered_at(RUN).and_then(|registered| {
        let approval = read_at(APPROVAL, RRF_RT_REG_BINARY)?;
        Ok(Status {
            registered,
            blocked_by_windows: approval_blocked(approval.as_deref()),
            error: false,
        })
    }) {
        Ok(s) => s,
        Err(_) => Status {
            error: true,
            ..Default::default()
        },
    }
}
fn command(path: &Path) -> Result<String> {
    let text = path.as_os_str().to_string_lossy();
    let result = format!("\"{text}\" --autostart");
    if !path.is_absolute()
        || text.contains(['"', '\0', '\r', '\n'])
        || result.encode_utf16().count() > 260
    {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    Ok(result)
}
fn write_at(path: &str, command: Option<&str>) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        if command.is_some() {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
            .ok()?;
        } else {
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                None,
                KEY_SET_VALUE,
                &mut key,
            );
            if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
                return Ok(());
            }
            status.ok()?;
        }
        let key = Key(key);
        if let Some(command) = command {
            let data: Vec<u8> = command
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            RegSetValueExW(key.0, &HSTRING::from(NAME), None, REG_SZ, Some(&data)).ok()?;
        } else {
            let status = RegDeleteValueW(key.0, &HSTRING::from(NAME));
            if status != ERROR_FILE_NOT_FOUND {
                status.ok()?;
            }
        }
        Ok(())
    }
}
pub fn installed_path() -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or_else(|| Error::from_hresult(E_FAIL))?;
    let path = PathBuf::from(root)
        .join("DynamicIslandForWindows")
        .join("app")
        .join(EXE);
    command(&path)?;
    Ok(path)
}
fn io(error: std::io::Error) -> Error {
    Error::new(E_FAIL, error.to_string())
}
fn install_copy() -> Result<PathBuf> {
    let path = installed_path()?;
    let current = std::env::current_exe().map_err(io)?;
    let same = std::fs::canonicalize(&current)
        .ok()
        .zip(std::fs::canonicalize(&path).ok())
        .is_some_and(|(a, b)| {
            a.to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy())
        });
    if !same {
        let parent = path
            .parent()
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        std::fs::create_dir_all(parent).map_err(io)?;
        let pending = parent.join("dynamic-island-for-windows.pending");
        std::fs::copy(current, &pending).map_err(io)?;
        let result = unsafe {
            use windows::Win32::Storage::FileSystem::*;
            MoveFileExW(
                &HSTRING::from(pending.as_os_str()),
                &HSTRING::from(path.as_os_str()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result.is_err() {
            let _ = std::fs::remove_file(pending);
        }
        result?;
    }
    Ok(path)
}
pub fn set_enabled(enabled: bool) -> Result<Status> {
    if enabled {
        let path = install_copy()?;
        write_at(RUN, Some(&command(&path)?))?;
    } else {
        write_at(RUN, None)?;
    }
    Ok(query())
}
pub fn manage_in_windows() {
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:startupapps"),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
/// A manually opened upgrade may refresh the stable copy after obtaining the
/// single-instance mutex. Logon launches never rewrite their own Run entry.
pub fn refresh_copy() {
    if env!("CARGO_PKG_VERSION").contains("preview")
        || std::env::args().any(|a| a == "--autostart" || a == "--no-startup-refresh")
        || crate::platform::argument("--data-dir").is_some()
    {
        return;
    }
    let Ok(Some(raw)) = read_at(RUN, RRF_RT_REG_SZ) else {
        return;
    };
    let utf: Vec<_> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| u16::from_le_bytes(*s))
        .take_while(|v| *v != 0)
        .collect();
    let Ok(expected) = installed_path().and_then(|p| command(&p)) else {
        return;
    };
    if String::from_utf16_lossy(&utf).eq_ignore_ascii_case(&expected) {
        let _ = install_copy();
    }
}
pub fn cli(value: &str) -> Result<()> {
    if crate::platform::argument("--data-dir").is_some() {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let status = match value {
        "on" => set_enabled(true)?,
        "off" => set_enabled(false)?,
        "status" => query(),
        _ => return Err(Error::from_hresult(E_INVALIDARG)),
    };
    println!("{}", serde_json::to_string(&status).unwrap());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_native_command_and_limit() {
        assert_eq!(
            command(Path::new("C:\\A folder\\island.exe")).unwrap(),
            "\"C:\\A folder\\island.exe\" --autostart"
        );
        assert!(command(Path::new("relative.exe")).is_err());
        assert!(command(Path::new("C:\\a\"\\island.exe")).is_err());
        assert!(command(Path::new(&format!("C:\\{}.exe", "a".repeat(260)))).is_err());
        assert!(command(Path::new("C:\\Catatan é\\island.exe")).is_ok());
    }
    #[test]
    fn external_windows_disable_is_not_overridden() {
        assert!(!approval_blocked(None));
        for state in [2u32, 6] {
            assert!(!approval_blocked(Some(&state.to_le_bytes())));
        }
        for state in [3u32, 7, 0, 99] {
            assert!(approval_blocked(Some(&state.to_le_bytes())));
        }
        assert!(approval_blocked(Some(&[])));
        assert!(
            !Status {
                registered: true,
                blocked_by_windows: true,
                error: false
            }
            .enabled()
        );
    }
    #[test]
    fn isolated_registry_roundtrip_only_touches_own_value() {
        let path = format!(
            "Software\\DynamicIslandForWindows\\TestStartup-{}",
            std::process::id()
        );
        write_at(&path, None).unwrap();
        assert!(!registered_at(&path).unwrap());
        let cmd = command(Path::new("C:\\Test folder\\island.exe")).unwrap();
        write_at(&path, Some(&cmd)).unwrap();
        assert!(registered_at(&path).unwrap());
        let raw = read_at(&path, RRF_RT_REG_SZ).unwrap().unwrap();
        let actual: Vec<_> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| u16::from_le_bytes(*x))
            .take_while(|x| *x != 0)
            .collect();
        assert_eq!(String::from_utf16(&actual).unwrap(), cmd);
        write_at(&path, None).unwrap();
        write_at(&path, None).unwrap();
        assert!(!registered_at(&path).unwrap());
        unsafe {
            RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(path))
                .ok()
                .unwrap();
        }
    }
}
