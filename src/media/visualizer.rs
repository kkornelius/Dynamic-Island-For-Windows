use crate::{
    event::{Bus, Event},
    media::levels::{self, Levels, Sample},
    platform::Apartment,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Media::{Audio::*, Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT},
        System::{Com::*, Threading::*},
    },
    core::*,
};

/// Kernel event handles support concurrent signaling/waiting; closed after all Arc owners leave.
pub struct Wake(HANDLE);
unsafe impl Send for Wake {}
unsafe impl Sync for Wake {}
impl Wake {
    pub fn new() -> Result<Self> {
        unsafe { Ok(Self(CreateEventW(None, false, false, None)?)) }
    }
    pub fn signal(&self) {
        unsafe {
            let _ = SetEvent(self.0);
        }
    }
}
impl Drop for Wake {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub struct AudioWorker {
    pub enabled: Arc<AtomicBool>,
    pub settings: Arc<Mutex<(f32, f32)>>,
    wake: Arc<Wake>,
    join: Option<JoinHandle<()>>,
}
impl AudioWorker {
    pub fn start(
        bus: Bus,
        stop: Arc<AtomicBool>,
        enabled: bool,
        sensitivity: f32,
        smoothing: f32,
    ) -> Result<Self> {
        let wake = Arc::new(Wake::new()?);
        let flag = Arc::new(AtomicBool::new(enabled));
        let settings = Arc::new(Mutex::new((sensitivity, smoothing)));
        let w = wake.clone();
        let f = flag.clone();
        let s = settings.clone();
        let join = thread::spawn(move || {
            if let Err(e) = run(&bus, &stop, &f, &s, &w)
                && !stop.load(Ordering::Relaxed)
            {
                bus.send(Event::AudioStatus(format!(
                    "Audio unavailable ({})",
                    e.code()
                )));
            }
        });
        Ok(Self {
            enabled: flag,
            settings,
            wake,
            join: Some(join),
        })
    }
    pub fn configure(&self, enabled: bool, sensitivity: f32, smoothing: f32) {
        self.enabled.store(enabled, Ordering::Relaxed);
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = (sensitivity, smoothing);
        self.wake.signal();
    }
    pub fn shutdown(&mut self) {
        self.wake.signal();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
#[implement(IMMNotificationClient)]
struct EndpointEvents {
    wake: Arc<Wake>,
    dirty: Arc<AtomicBool>,
}
impl EndpointEvents_Impl {
    fn changed(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.wake.signal();
    }
}
impl IMMNotificationClient_Impl for EndpointEvents_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eMultimedia {
            self.changed();
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        self.changed();
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        self.changed();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        self.changed();
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}
struct Registration {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
}
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.callback);
        }
    }
}
struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    sample: Sample,
    block: usize,
}
impl Stream {
    fn open(enumerator: &IMMDeviceEnumerator, wake: &Wake) -> Result<Self> {
        unsafe {
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let opened = (|| -> Result<(Sample, usize)> {
                let fmt = *format;
                let subtype = if fmt.wFormatTag == 0xfffe && fmt.cbSize >= 22 {
                    (*(format as *const WAVEFORMATEXTENSIBLE)).SubFormat
                } else {
                    GUID::zeroed()
                };
                let float = fmt.wFormatTag == 3 || subtype == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
                let pcm = fmt.wFormatTag == 1
                    || subtype == GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
                let sample = match (float, pcm, fmt.wBitsPerSample) {
                    (true, _, 32) => Sample::Float32,
                    (_, true, 8) => Sample::Pcm8,
                    (_, true, 16) => Sample::Pcm16,
                    (_, true, 24) => Sample::Pcm24,
                    (_, true, 32) => Sample::Pcm32,
                    _ => {
                        return Err(Error::new(
                            HRESULT(0x80004001u32 as i32),
                            "Unsupported audio sample format",
                        ));
                    }
                };
                if fmt.nBlockAlign as usize
                    != fmt.nChannels as usize * fmt.wBitsPerSample as usize / 8
                {
                    return Err(Error::empty());
                }
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    0,
                    0,
                    format,
                    None,
                )?;
                Ok((sample, fmt.nBlockAlign as usize))
            })();
            CoTaskMemFree(Some(format.cast()));
            let (sample, block) = opened?;
            client.SetEventHandle(wake.0)?;
            let capture = client.GetService()?;
            client.Start()?;
            Ok(Self {
                client,
                capture,
                sample,
                block,
            })
        }
    }
    fn drain(&self) -> Result<(f64, u64)> {
        unsafe {
            let mut energy = 0.0;
            let mut frames_total = 0;
            while self.capture.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let rms = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    0.0
                } else {
                    levels::rms(
                        std::slice::from_raw_parts(data, frames as usize * self.block),
                        self.sample,
                    )
                };
                energy += rms as f64 * rms as f64 * frames as f64;
                frames_total += frames as u64;
                self.capture.ReleaseBuffer(frames)?;
            }
            Ok((energy, frames_total))
        }
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
fn run(
    bus: &Bus,
    stop: &AtomicBool,
    enabled: &AtomicBool,
    settings: &Mutex<(f32, f32)>,
    wake: &Arc<Wake>,
) -> Result<()> {
    let _apartment = Apartment::new()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let dirty = Arc::new(AtomicBool::new(false));
    let callback: IMMNotificationClient = EndpointEvents {
        wake: wake.clone(),
        dirty: dirty.clone(),
    }
    .into();
    unsafe {
        enumerator.RegisterEndpointNotificationCallback(&callback)?;
    }
    let _registration = Registration {
        enumerator: enumerator.clone(),
        callback,
    };
    let mut stream: Option<Stream> = None;
    let mut levels = Levels::default();
    let mut last_emit = Instant::now();
    let mut last_status = String::new();
    let mut retry = Instant::now();
    let mut energy = 0.0;
    let mut frames = 0;
    while !stop.load(Ordering::Relaxed) {
        if dirty.swap(false, Ordering::Relaxed) {
            stream = None;
            retry = Instant::now();
        }
        let on = enabled.load(Ordering::Relaxed);
        if !on {
            stream = None;
            levels = Levels::default();
            bus.levels(levels.bars);
            if last_status != "Visualizer off" {
                last_status = "Visualizer off".into();
                bus.send(Event::AudioStatus(last_status.clone()));
            }
            unsafe {
                WaitForSingleObject(wake.0, INFINITE);
            }
            continue;
        }
        if stream.is_none() && Instant::now() >= retry {
            match Stream::open(&enumerator, wake) {
                Ok(s) => {
                    stream = Some(s);
                    last_status = "System output · WASAPI".into();
                    bus.send(Event::AudioStatus(last_status.clone()));
                }
                Err(_) => {
                    let status = "No output available · retrying";
                    if last_status != status {
                        last_status = status.into();
                        bus.send(Event::AudioStatus(last_status.clone()));
                    }
                    retry = Instant::now() + Duration::from_secs(5);
                }
            }
        }
        let timeout = if stream.is_none() || levels.bars.iter().all(|v| *v == 0.0) {
            1000
        } else {
            50
        };
        unsafe {
            WaitForSingleObject(wake.0, timeout);
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(s) = stream.as_ref() {
            match s.drain() {
                Ok((e, n)) => {
                    energy += e;
                    frames += n;
                }
                Err(_) => {
                    stream = None;
                    retry = Instant::now() + Duration::from_millis(500);
                    bus.send(Event::AudioStatus("Output changed · reconnecting".into()));
                }
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(50) {
            let rms = if frames > 0 {
                (energy / frames as f64).sqrt() as f32
            } else {
                0.0
            };
            energy = 0.0;
            frames = 0;
            let (sensitivity, smoothing) = *settings.lock().unwrap_or_else(|e| e.into_inner());
            let old = levels.bars;
            levels.push(rms, sensitivity, smoothing);
            if old != levels.bars || levels.bars.iter().any(|v| *v > 0.003) {
                bus.levels(levels.bars);
            }
            last_emit = Instant::now();
        }
    }
    drop(stream);
    bus.levels([0.0; levels::BARS]);
    Ok(())
}
