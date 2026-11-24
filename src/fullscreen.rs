//! Foreground full-screen detection uses window geometry, never titles or video content.
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::*,
};
fn covers(client: RECT, monitor: RECT, style: u32) -> bool {
    let framed = style & (WS_DLGFRAME.0 | WS_THICKFRAME.0) != 0;
    !framed
        && monitor.right > monitor.left
        && monitor.bottom > monitor.top
        && client.left <= monitor.left + 2
        && client.top <= monitor.top + 2
        && client.right >= monitor.right - 2
        && client.bottom >= monitor.bottom - 2
}
fn geometry(window: HWND, monitor: RECT) -> Option<bool> {
    unsafe {
        let mut client = RECT::default();
        GetClientRect(window, &mut client).ok()?;
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        let mut end = POINT {
            x: client.right,
            y: client.bottom,
        };
        if !ClientToScreen(window, &mut origin).as_bool()
            || !ClientToScreen(window, &mut end).as_bool()
        {
            return None;
        }
        Some(covers(
            RECT {
                left: origin.x,
                top: origin.y,
                right: end.x,
                bottom: end.y,
            },
            monitor,
            GetWindowLongW(window, GWL_STYLE) as u32,
        ))
    }
}
fn usable(window: HWND) -> bool {
    unsafe {
        if !IsWindow(Some(window)).as_bool()
            || !IsWindowVisible(window).as_bool()
            || IsIconic(window).as_bool()
        {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            window,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            4,
        );
        cloaked == 0
    }
}
/// A missing foreground during a game teardown may only retain a blocker that
/// still exists, is visible, and still covers this monitor. Never latch a stale
/// fullscreen bool when the game window has already gone away.
pub fn active(own: HWND, anchor: (i32, i32), previous: Option<isize>) -> (bool, Option<isize>) {
    unsafe {
        let foreground = GetForegroundWindow();
        let monitor = MonitorFromPoint(
            POINT {
                x: anchor.0,
                y: anchor.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return (false, None);
        }
        let candidate = if foreground.0.is_null() {
            previous.map(|h| HWND(h as _)).filter(|w| usable(*w))
        } else {
            Some(foreground).filter(|w| *w != own && usable(*w))
        };
        let Some(foreground) = candidate else {
            return (false, None);
        };
        let mut name = [0u16; 128];
        let length = GetClassNameW(foreground, &mut name);
        let class = String::from_utf16_lossy(&name[..length.max(0) as usize]);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return (false, None);
        }
        let hidden = geometry(foreground, info.rcMonitor) == Some(true);
        (hidden, hidden.then_some(foreground.0 as isize))
    }
}
/// Hidden native fixtures exercise actual client-to-screen coordinates without taking focus.
pub fn verify_geometry(
    anchor: (i32, i32),
) -> Result<std::collections::BTreeMap<&'static str, bool>> {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: anchor.0,
                y: anchor.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err(Error::from_thread());
        }
        let r = info.rcMonitor;
        let window = CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!("Geometry fixture"),
            WS_POPUP,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            None,
            None,
        )?;
        let mut checks = std::collections::BTreeMap::new();
        checks.insert(
            "fullscreen_native_client_geometry",
            geometry(window, r) == Some(true),
        );
        SetWindowLongW(window, GWL_STYLE, WS_OVERLAPPEDWINDOW.0 as i32);
        let _ = SetWindowPos(
            window,
            None,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        checks.insert(
            "maximized_size_with_frame_is_not_fullscreen",
            geometry(window, r) == Some(false),
        );
        SetWindowLongW(window, GWL_STYLE, WS_POPUP.0 as i32);
        let _ = SetWindowPos(
            window,
            None,
            r.left + 40,
            r.top + 40,
            (r.right - r.left - 80).max(1),
            (r.bottom - r.top - 80).max(1),
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        checks.insert(
            "windowed_popup_is_not_fullscreen",
            geometry(window, r) == Some(false),
        );
        let _ = DestroyWindow(window);
        checks.insert(
            "destroyed_fullscreen_blocker_is_not_retained",
            !usable(window),
        );
        Ok(checks)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
        RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }
    #[test]
    fn borderless_monitor_coverage_is_fullscreen_but_normal_maximized_is_not() {
        let m = rect(0, 0, 1920, 1080);
        assert!(covers(m, m, WS_POPUP.0));
        assert!(!covers(m, m, WS_OVERLAPPEDWINDOW.0));
        assert!(!covers(rect(0, 0, 1920, 1040), m, WS_POPUP.0));
        assert!(!covers(rect(100, 100, 1600, 900), m, WS_POPUP.0));
    }
    #[test]
    fn uses_capsule_monitor_and_accepts_negative_coordinates_and_spanning_windows() {
        let m = rect(-1920, 0, 1920, 1080);
        assert!(covers(m, m, WS_POPUP.0));
        assert!(!covers(rect(0, 0, 1920, 1080), m, WS_POPUP.0));
        assert!(covers(rect(-1920, 0, 3840, 1080), m, WS_POPUP.0));
        assert!(covers(rect(-1919, 1, 1918, 1078), m, WS_POPUP.0));
        assert!(!covers(rect(-1917, 0, 1917, 1080), m, WS_POPUP.0));
    }
}
