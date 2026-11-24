use windows::{
    Win32::{
        Foundation::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};
pub const TRAY_MESSAGE: u32 = WM_APP + 2;
pub struct Tray {
    data: NOTIFYICONDATAW,
    pub registered: bool,
}
fn copy_utf16(target: &mut [u16], text: &str) {
    let count = target.len().saturating_sub(1);
    target.fill(0);
    for (out, ch) in target.iter_mut().take(count).zip(text.encode_utf16()) {
        *out = ch;
    }
}
impl Tray {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: LoadIconW(None, IDI_APPLICATION)?,
                ..Default::default()
            };
            copy_utf16(
                &mut data.szTip,
                "Dynamic Island For Windows — click to open · right-click for menu",
            );
            let mut tray = Self {
                data,
                registered: false,
            };
            tray.add();
            Ok(tray)
        }
    }
    pub fn add(&mut self) {
        unsafe {
            self.registered = Shell_NotifyIconW(NIM_ADD, &self.data).as_bool();
            self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &self.data);
        }
    }
    pub fn remove(&mut self) {
        if self.registered {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
            }
            self.registered = false;
        }
    }
    pub fn menu(&self, visible: bool) -> u32 {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return 0;
            };
            let _ = AppendMenuW(menu, MF_STRING, 1, w!("Open / collapse island"));
            let _ = AppendMenuW(menu, MF_STRING, 2, w!("Settings"));
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                3,
                if visible {
                    w!("Hide capsule")
                } else {
                    w!("Show capsule")
                },
            );
            let _ = AppendMenuW(menu, MF_STRING, 4, w!("Reset position"));
            if let Ok(about) = CreatePopupMenu() {
                let version: Vec<u16> =
                    format!("Dynamic Island For Windows {}\0", env!("CARGO_PKG_VERSION"))
                        .encode_utf16()
                        .collect();
                let _ = AppendMenuW(about, MF_STRING | MF_DISABLED, 0, PCWSTR(version.as_ptr()));
                let _ = AppendMenuW(about, MF_SEPARATOR, 0, None);
                let _ = AppendMenuW(about, MF_STRING, 6, w!("Weather: Open-Meteo / GeoNames"));
                let _ = AppendMenuW(about, MF_STRING, 7, w!("Icons: Icons8"));
                let _ = AppendMenuW(menu, MF_POPUP, about.0 as usize, w!("About / credits"));
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, 9, w!("Quit Dynamic Island For Windows"));
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(self.data.hWnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                point.x,
                point.y,
                None,
                self.data.hWnd,
                None,
            )
            .0 as u32;
            let _ = DestroyMenu(menu);
            let _ = PostMessageW(Some(self.data.hWnd), WM_NULL, WPARAM(0), LPARAM(0));
            command
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}
