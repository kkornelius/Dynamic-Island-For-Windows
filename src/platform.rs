use windows::{
    Media::Control::*,
    Win32::{
        Media::Audio::*,
        System::{Com::*, WinRT::*},
    },
    core::*,
};

pub struct Apartment;
impl Apartment {
    pub fn new() -> Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

pub fn now_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}
pub fn utc_ticks() -> u64 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime() };
    (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64
}
pub fn argument(name: &str) -> Option<String> {
    let args: Vec<_> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
pub struct LocalTime {
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub weekday: u16,
}
impl LocalTime {
    pub fn get() -> Self {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        Self {
            hour: t.wHour,
            minute: t.wMinute,
            second: t.wSecond,
            milliseconds: t.wMilliseconds,
            year: t.wYear,
            month: t.wMonth,
            day: t.wDay,
            weekday: t.wDayOfWeek,
        }
    }
    pub fn key(&self) -> u32 {
        self.year as u32 * 10000 + self.month as u32 * 100 + self.day as u32
    }
    pub fn clock(&self, h24: bool) -> String {
        if h24 {
            format!("{:02}:{:02}", self.hour, self.minute)
        } else {
            format!(
                "{}:{:02} {}",
                if self.hour.is_multiple_of(12) {
                    12
                } else {
                    self.hour % 12
                },
                self.minute,
                if self.hour < 12 { "AM" } else { "PM" }
            )
        }
    }
    pub fn date(&self) -> String {
        let days = [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ];
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        format!(
            "{}, {} {}",
            days[self.weekday as usize % 7],
            self.day,
            months[self.month.saturating_sub(1) as usize % 12]
        )
    }
}

/// Bounded WinRT waits occur only on workers; cancellation also bounds shutdown.
#[macro_export]
macro_rules! wait_operation {
    ($expression:expr,$stop:expr) => {{
        let operation = $expression?;
        let start = std::time::Instant::now();
        while operation.Status()?.0 == 0 {
            if $stop.load(std::sync::atomic::Ordering::Relaxed)
                || start.elapsed() > std::time::Duration::from_secs(3)
            {
                let _ = operation.Cancel();
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x800705b4u32 as i32),
                    "Windows operation timed out or cancelled",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        operation.GetResults()
    }};
}

pub fn probe() -> Result<()> {
    let _apartment = Apartment::new()?;
    let media = (|| -> Result<serde_json::Value> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
        Ok(serde_json::json!({"available":true,"sessions":manager.GetSessions()?.Size()?}))
    })()
    .unwrap_or_else(|e| serde_json::json!({"available":false,"error":e.to_string()}));
    let audio = (|| -> Result<serde_json::Value> { unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = client.GetMixFormat()?;
        let channels = (*format).nChannels;
        let rate = (*format).nSamplesPerSec;
        let initialized = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 1_000_000, 0, format, None);
        CoTaskMemFree(Some(format.cast()));
        initialized?;
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        let packets = capture.GetNextPacketSize()?;
        client.Stop()?;
        Ok(serde_json::json!({"available":true,"channels":channels,"sample_rate":rate,"first_packet_frames":packets}))
    } })().unwrap_or_else(|e| serde_json::json!({"available":false,"error":e.to_string()}));
    let metadata=crate::media::sessions::probe_snapshot().map(|s|serde_json::json!({"session":s.available,"title_available":!s.title.is_empty(),"artist_available":!s.artist.is_empty(),"artwork":s.artwork.is_some(),"playing":s.playing,"play_control":s.can_play,"pause_control":s.can_pause,"next_control":s.can_next,"previous_control":s.can_previous})).unwrap_or_else(|e|serde_json::json!({"error":e.code().0}));
    let report = serde_json::json!({"media":media,"metadata":metadata,"audio":audio});
    let _ = std::fs::create_dir_all("artifacts");
    std::fs::write(
        "artifacts/platform-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    println!("{report}");
    Ok(())
}

pub fn probe_window() -> Result<()> {
    unsafe {
        use windows::Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*};
        unsafe extern "system" fn procedure(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("DynamicIslandForWindows.ZOrderProbe");
        RegisterClassW(&WNDCLASSW {
            hInstance: instance,
            lpszClassName: class,
            lpfnWndProc: Some(procedure),
            ..Default::default()
        });
        let mut rows = Vec::new();
        for (label, extra) in [
            ("regular", WINDOW_EX_STYLE(0)),
            ("tool", WS_EX_TOOLWINDOW),
            ("layered", WS_EX_TOOLWINDOW | WS_EX_LAYERED),
            (
                "noactivate",
                WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            ),
        ] {
            let hwnd = CreateWindowExW(
                extra | WS_EX_TOPMOST,
                class,
                w!("Window order probe"),
                WS_POPUP,
                0,
                0,
                50,
                50,
                None,
                None,
                Some(instance),
                None,
            )?;
            let mut steps = Vec::new();
            steps.push(
                serde_json::json!({"operation":"create","style":GetWindowLongW(hwnd,GWL_EXSTYLE)}),
            );
            for (name, after, flags) in [
                (
                    "off",
                    HWND_NOTOPMOST,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                ),
                ("on", HWND_TOPMOST, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE),
                (
                    "on-nosend",
                    HWND_TOPMOST,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                ),
                (
                    "on-frame",
                    HWND_TOPMOST,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_NOACTIVATE,
                ),
                (
                    "on-show",
                    HWND_TOPMOST,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ),
            ] {
                let result = SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, flags);
                let mut info = WINDOWINFO {
                    cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
                    ..Default::default()
                };
                let _ = GetWindowInfo(hwnd, &mut info);
                steps.push(serde_json::json!({"operation":name,"success":result.is_ok(),"style":GetWindowLongW(hwnd,GWL_EXSTYLE),"window_info_style":info.dwExStyle.0,"hwnd_after":after.0 as isize}));
            }
            let anchor = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Order anchor"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )?;
            let result = SetWindowPos(
                hwnd,
                Some(anchor),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            steps.push(serde_json::json!({"operation":"after-topmost-anchor","success":result.is_ok(),"style":GetWindowLongW(hwnd,GWL_EXSTYLE)}));
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let defer = BeginDeferWindowPos(1)?;
            let defer = DeferWindowPos(
                defer,
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
            let result = EndDeferWindowPos(defer);
            steps.push(serde_json::json!({"operation":"defer-topmost","success":result.is_ok(),"style":GetWindowLongW(hwnd,GWL_EXSTYLE)}));
            let _ = DestroyWindow(anchor);
            let _ = DestroyWindow(hwnd);
            rows.push(serde_json::json!({"kind":label,"steps":steps}));
        }
        let _ = std::fs::create_dir_all("artifacts");
        std::fs::write(
            "artifacts/window-probe.json",
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
        Ok(())
    }
}
