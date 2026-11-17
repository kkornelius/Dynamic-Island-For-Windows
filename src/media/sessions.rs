use crate::{
    event::{Bus, Event},
    media::{Artwork, MediaSnapshot, Playback},
    platform::Apartment,
    wait_operation,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::*,
    Storage::Streams::*,
    Win32::{Graphics::Imaging::*, System::Com::*, UI::Shell::SHCreateMemStream},
    core::*,
};

pub enum Command {
    Refresh,
    StateChanged,
    Playback(Playback),
    Stop,
}
pub struct MediaWorker {
    pub tx: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
}
impl MediaWorker {
    pub fn start(bus: Bus, stop: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::sync_channel(16);
        let callback_tx = tx.clone();
        let join = thread::spawn(move || {
            let result = (|| -> Result<()> {
                let _apartment = Apartment::new()?;
                let manager = wait_operation!(
                    GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
                    stop
                )?;
                let tx1 = callback_tx.clone();
                let current_token =
                    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
                        let _ = tx1.try_send(Command::Refresh);
                        Ok(())
                    }))?;
                let tx2 = callback_tx.clone();
                let sessions_token =
                    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
                        let _ = tx2.try_send(Command::Refresh);
                        Ok(())
                    }))?;
                let mut watched: Option<Watched> = None;
                let mut latest = MediaSnapshot::default();
                let _ = callback_tx.try_send(Command::Refresh);
                while !stop.load(Ordering::Relaxed) {
                    let command = match rx.recv() {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    if matches!(command, Command::Stop) {
                        break;
                    }
                    let current = if matches!(command, Command::StateChanged) {
                        watched.as_ref().map(|w| w.session.clone())
                    } else {
                        manager.GetCurrentSession().ok()
                    };
                    let changed = current.as_ref().map(Interface::as_raw)
                        != watched.as_ref().map(|s| s.session.as_raw());
                    if changed {
                        latest = MediaSnapshot::default();
                        watched = None;
                        if let Some(session) = current {
                            match Watched::new(session, callback_tx.clone()) {
                                Ok(w) => watched = Some(w),
                                Err(_) => bus.send(Event::Error(
                                    "Could not subscribe to this media session.".into(),
                                )),
                            }
                        }
                    }
                    if let Some(w) = watched.as_ref() {
                        if let Command::Playback(action) = command
                            && let Err(_) = control(&w.session, action, &stop)
                        {
                            bus.send(Event::Error(
                                "The media source did not accept that control.".into(),
                            ));
                        }
                        // Timeline/playback events never re-open or decode artwork.
                        let update = if !latest.available || matches!(command, Command::Refresh) {
                            snapshot(&w.session, &stop)
                        } else {
                            refresh_state(&w.session, &mut latest).map(|_| latest.clone())
                        };
                        match update {
                            Ok(snapshot) => {
                                latest = snapshot;
                                bus.send(Event::Media(latest.clone()));
                            }
                            Err(_) => {
                                latest = MediaSnapshot::default();
                                bus.send(Event::Media(latest.clone()));
                            }
                        }
                    } else {
                        latest = MediaSnapshot::default();
                        bus.send(Event::Media(MediaSnapshot::default()));
                    }
                }
                drop(watched);
                let _ = manager.RemoveCurrentSessionChanged(current_token);
                let _ = manager.RemoveSessionsChanged(sessions_token);
                Ok(())
            })();
            if let Err(e) = result
                && !stop.load(Ordering::Relaxed)
            {
                bus.send(Event::Error(format!(
                    "Windows media unavailable: {}",
                    e.code()
                )));
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.try_send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
struct Watched {
    session: GlobalSystemMediaTransportControlsSession,
    tokens: [i64; 3],
}
impl Watched {
    fn new(
        session: GlobalSystemMediaTransportControlsSession,
        tx: SyncSender<Command>,
    ) -> Result<Self> {
        let mut watched = Self {
            session,
            tokens: [0; 3],
        };
        let tx1 = tx.clone();
        watched.tokens[0] = watched
            .session
            .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = tx1.try_send(Command::Refresh);
                Ok(())
            }))?;
        let tx2 = tx.clone();
        watched.tokens[1] = watched
            .session
            .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = tx2.try_send(Command::StateChanged);
                Ok(())
            }))?;
        watched.tokens[2] = watched
            .session
            .TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = tx.try_send(Command::StateChanged);
                Ok(())
            }))?;
        Ok(watched)
    }
}
impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.tokens[0]);
        let _ = self.session.RemovePlaybackInfoChanged(self.tokens[1]);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.tokens[2]);
    }
}
fn control(
    session: &GlobalSystemMediaTransportControlsSession,
    action: Playback,
    stop: &AtomicBool,
) -> Result<()> {
    let info = session.GetPlaybackInfo()?;
    let c = info.Controls()?;
    let op = match action {
        Playback::Toggle
            if info.PlaybackStatus()?
                == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                && c.IsPauseEnabled()? =>
        {
            session.TryPauseAsync()
        }
        Playback::Toggle if c.IsPlayEnabled()? => session.TryPlayAsync(),
        Playback::Next if c.IsNextEnabled()? => session.TrySkipNextAsync(),
        Playback::Previous if c.IsPreviousEnabled()? => session.TrySkipPreviousAsync(),
        _ => {
            return Err(Error::new(
                HRESULT(0x80004001u32 as i32),
                "Control unavailable",
            ));
        }
    };
    if wait_operation!(op, stop)? {
        Ok(())
    } else {
        Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "Source rejected control",
        ))
    }
}
fn snapshot(
    session: &GlobalSystemMediaTransportControlsSession,
    stop: &AtomicBool,
) -> Result<MediaSnapshot> {
    let properties = wait_operation!(session.TryGetMediaPropertiesAsync(), stop)?;
    let source = session.SourceAppUserModelId()?.to_string();
    let source = if source.to_lowercase().contains("spotify") {
        "Spotify".into()
    } else if source.contains("Chrome") {
        "Chrome".into()
    } else if source.contains("MSEdge") {
        "Microsoft Edge".into()
    } else if source.to_lowercase().contains("brave") {
        "Brave".into()
    } else {
        source
    };
    let artwork = read_artwork(&properties, stop).ok().map(Arc::new);
    let mut result = MediaSnapshot {
        available: true,
        title: properties.Title()?.to_string(),
        artist: properties.Artist()?.to_string(),
        album: properties
            .AlbumTitle()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        source,
        artwork,
        ..Default::default()
    };
    refresh_state(session, &mut result)?;
    Ok(result)
}
fn refresh_state(
    session: &GlobalSystemMediaTransportControlsSession,
    result: &mut MediaSnapshot,
) -> Result<()> {
    let info = session.GetPlaybackInfo()?;
    let controls = info.Controls()?;
    let timeline = session.GetTimelineProperties().ok();
    let position = timeline
        .as_ref()
        .and_then(|t| {
            Some((t.Position().ok()?.Duration - t.StartTime().ok()?.Duration).max(0) as u64 / 10000)
        })
        .unwrap_or(0);
    let duration = timeline
        .as_ref()
        .and_then(|t| {
            Some((t.EndTime().ok()?.Duration - t.StartTime().ok()?.Duration).max(0) as u64 / 10000)
        })
        .unwrap_or(0);
    result.playing =
        info.PlaybackStatus()? == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    result.can_play = controls.IsPlayEnabled()?;
    result.can_pause = controls.IsPauseEnabled()?;
    result.can_next = controls.IsNextEnabled()?;
    result.can_previous = controls.IsPreviousEnabled()?;
    result.position_ms = position;
    result.duration_ms = duration;
    result.playback_rate = info
        .PlaybackRate()
        .and_then(|r| r.Value())
        .ok()
        .filter(|r| r.is_finite() && *r > 0.0 && *r <= 16.0)
        .unwrap_or(1.0);
    result.sampled_at_ms = crate::platform::now_ms();
    let updated = timeline
        .as_ref()
        .and_then(|t| t.LastUpdatedTime().ok())
        .map(|t| t.UniversalTime)
        .unwrap_or(0);
    result.timeline_available = timeline.is_some() && updated > 0;
    if result.playing && result.timeline_available {
        let age = crate::platform::utc_ticks().saturating_sub(updated as u64) / 10000;
        result.position_ms = result
            .position_ms
            .saturating_add((age as f64 * result.playback_rate) as u64);
        if duration > 0 {
            result.position_ms = result.position_ms.min(duration);
        }
    }
    Ok(())
}
fn read_artwork(
    properties: &GlobalSystemMediaTransportControlsSessionMediaProperties,
    stop: &AtomicBool,
) -> Result<Artwork> {
    let stream = wait_operation!(properties.Thumbnail()?.OpenReadAsync(), stop)?;
    let size = stream.Size()?;
    if size == 0 || size > 4 * 1024 * 1024 {
        return Err(Error::empty());
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    let loaded = wait_operation!(reader.LoadAsync(size as u32), stop)?;
    let mut bytes = vec![0; loaded as usize];
    reader.ReadBytes(&mut bytes)?;
    decode_artwork(&bytes)
}
pub fn decode_artwork(bytes: &[u8]) -> Result<Artwork> {
    unsafe {
        let stream = SHCreateMemStream(Some(bytes)).ok_or_else(Error::empty)?;
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnLoad,
        )?;
        let frame = decoder.GetFrame(0)?;
        let mut width = 0;
        let mut height = 0;
        frame.GetSize(&mut width, &mut height)?;
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(Error::empty());
        }
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(&frame, 160, 160, WICBitmapInterpolationModeFant)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let mut pixels = vec![0; 160 * 160 * 4];
        converter.CopyPixels(std::ptr::null(), 160 * 4, &mut pixels)?;
        Ok(Artwork {
            width: 160,
            height: 160,
            pixels,
        })
    }
}

pub fn probe_snapshot() -> Result<MediaSnapshot> {
    let _a = Apartment::new()?;
    let stop = AtomicBool::new(false);
    let manager = wait_operation!(
        GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
        stop
    )?;
    match manager.GetCurrentSession() {
        Ok(s) => snapshot(&s, &stop),
        Err(_) => Ok(MediaSnapshot::default()),
    }
}

/// Exercise play/pause on the same session and restore its initial playback state.
pub fn probe_controls() -> Result<()> {
    use std::time::{Duration, Instant};
    let _a = Apartment::new()?;
    let stop = AtomicBool::new(false);
    let manager = wait_operation!(
        GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
        stop
    )?;
    let session = manager.GetCurrentSession()?;
    let playing = || -> Result<bool> {
        Ok(session.GetPlaybackInfo()?.PlaybackStatus()?
            == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
    };
    let original = playing()?;
    let accepted = control(&session, Playback::Toggle, &stop).is_ok();
    let start = Instant::now();
    while playing()? == original && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let changed = playing()? != original;
    let restored = if changed {
        control(&session, Playback::Toggle, &stop).is_ok()
    } else {
        true
    };
    let start = Instant::now();
    while playing()? != original && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let report = serde_json::json!({"initially_playing":original,"command_accepted":accepted,"playback_state_changed":changed,"restore_accepted":restored,"initial_state_restored":playing()?==original});
    let _ = std::fs::create_dir_all("artifacts");
    std::fs::write(
        "artifacts/media-controls.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}
