//! One coalesced frame source, sleeping completely when the overlay is hidden.
//! High-resolution one-shot timers avoid WM_TIMER's low priority and coarse cadence.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*},
    core::*,
};
pub const FRAME_MESSAGE: u32 = WM_APP + 4;
static PENDING: AtomicBool = AtomicBool::new(false);
pub fn acknowledge_message() {
    PENDING.store(false, Ordering::Release);
}
enum Command {
    Active(bool),
    Stop,
}
pub struct FrameClock {
    tx: mpsc::Sender<Command>,
    active: bool,
    join: Option<JoinHandle<()>>,
}
impl FrameClock {
    pub fn new(hwnd: Arc<AtomicIsize>) -> Self {
        let (tx, rx) = mpsc::channel();
        acknowledge_message();
        let join = thread::spawn(move || unsafe {
            let timer = CreateWaitableTimerExW(
                None,
                PCWSTR::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
            .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, TIMER_ALL_ACCESS.0))
            .ok();
            let interval = Duration::from_nanos(16_666_667);
            let mut active = false;
            let mut next = Instant::now();
            'frames: loop {
                if !active {
                    match rx.recv() {
                        Ok(Command::Active(true)) => {
                            active = true;
                            next = Instant::now();
                        }
                        Ok(Command::Active(false)) => continue,
                        _ => break,
                    }
                }
                while let Ok(command) = rx.try_recv() {
                    match command {
                        Command::Stop => break 'frames,
                        Command::Active(value) => active = value,
                    }
                }
                if !active {
                    continue;
                }
                let now = Instant::now();
                if now < next {
                    let delay = next - now;
                    if let Some(timer) = timer {
                        let due = -((delay.as_nanos() / 100).max(1) as i64);
                        if SetWaitableTimer(timer, &due, 0, None, None, false).is_ok() {
                            WaitForSingleObject(timer, 50);
                        } else {
                            thread::sleep(delay);
                        }
                    } else {
                        thread::sleep(delay);
                    }
                    continue;
                }
                next += interval;
                if next < now {
                    next = now + interval;
                }
                let handle = hwnd.load(Ordering::Acquire);
                if handle != 0
                    && !PENDING.swap(true, Ordering::AcqRel)
                    && PostMessageW(Some(HWND(handle as _)), FRAME_MESSAGE, WPARAM(0), LPARAM(0))
                        .is_err()
                {
                    PENDING.store(false, Ordering::Release);
                }
            }
            if let Some(timer) = timer {
                let _ = CancelWaitableTimer(timer);
                let _ = CloseHandle(timer);
            }
        });
        Self {
            tx,
            active: false,
            join: Some(join),
        }
    }
    pub fn set_active(&mut self, value: bool) {
        if value != self.active {
            self.active = value;
            let _ = self.tx.send(Command::Active(value));
        }
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn acknowledge(&self) {
        acknowledge_message();
    }
}
impl Drop for FrameClock {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
