use crate::media::{MediaSnapshot, levels::BARS};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicIsize, Ordering},
    mpsc::Sender,
};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

#[derive(Clone, Copy)]
pub struct AudioFrame {
    pub levels: [f32; BARS],
    pub at_ms: u64,
}
impl Default for AudioFrame {
    fn default() -> Self {
        Self {
            levels: [0.0; BARS],
            at_ms: 0,
        }
    }
}
pub const WORKER_MESSAGE: u32 = WM_APP + 1;
pub enum Event {
    NoteSaved(u64, bool),
    BrowserSpace(u64, bool),
    Media(MediaSnapshot),
    Lyrics(crate::media::lyrics::TrackKey, crate::media::lyrics::Status),
    AudioStatus(String),
    BluetoothStatus(String),
    Bluetooth {
        name: String,
        connected: bool,
        kind: crate::bluetooth::DeviceKind,
    },
    Weather(
        crate::weather::City,
        std::result::Result<crate::weather::Forecast, String>,
    ),
    Cities(u64, std::result::Result<Vec<crate::weather::City>, String>),
    Error(String),
}
#[derive(Clone)]
pub struct Bus {
    pub tx: Sender<Event>,
    pub hwnd: Arc<AtomicIsize>,
    pub audio: Arc<Mutex<AudioFrame>>,
    pub audio_pending: Arc<AtomicBool>,
}
impl Bus {
    pub fn send(&self, event: Event) {
        if self.tx.send(event).is_ok() {
            self.wake();
        }
    }
    pub fn wake(&self) {
        let handle = self.hwnd.load(Ordering::Acquire);
        if handle == 0 {
            return;
        }
        unsafe {
            let _ = PostMessageW(
                Some(HWND(handle as _)),
                WORKER_MESSAGE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    pub fn levels(&self, levels: [f32; BARS]) {
        *self.audio.lock().unwrap_or_else(|e| e.into_inner()) = AudioFrame {
            levels,
            at_ms: crate::platform::now_ms(),
        };
        if !self.audio_pending.swap(true, Ordering::AcqRel) {
            self.wake();
        }
    }
}
