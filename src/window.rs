use crate::{
    app_state::*,
    bluetooth::BluetoothWorker,
    event::{self, Bus, Event},
    media::{
        Playback,
        levels::BARS,
        lyrics::{LyricsWorker, Request as LyricsRequest, Status as LyricsStatus, TrackKey},
        sessions::{Command, MediaWorker},
        visualizer::AudioWorker,
    },
    platform::{LocalTime, now_ms},
    timer::Status,
    tray::{TRAY_MESSAGE, Tray},
    ui::{self, Hit, canvas::Canvas},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc,
    },
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Media::Audio::*,
        System::{LibraryLoader::*, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::NIN_SELECT, WindowsAndMessaging::*},
    },
    core::*,
};

#[derive(serde::Serialize)]
struct FrameTiming {
    width: i32,
    height: i32,
    animating: bool,
    surface_ms: f64,
    mask_build_ms: f64,
    mask_apply_ms: f64,
    upload_ms: f64,
    draw_ms: f64,
    present_ms: f64,
    total_ms: f64,
    interval_ms: f64,
}
struct App {
    hwnd: HWND,
    state: State,
    rx: mpsc::Receiver<Event>,
    bus: Bus,
    stop: Arc<AtomicBool>,
    media: MediaWorker,
    lyrics: LyricsWorker,
    weather: crate::weather::WeatherWorker,
    edit_previous: Option<HWND>,
    note_writer: Option<crate::notes::Writer>,
    note_drag: bool,
    note_surrogate: Option<u16>,
    color_drag: Option<crate::color::Part>,
    color_dirty: bool,
    design_checks: std::collections::BTreeMap<&'static str, bool>,
    water_frame: Option<u64>,
    audio: Option<AudioWorker>,
    bluetooth: BluetoothWorker,
    tray: Tray,
    canvas: Option<Canvas>,
    hits: Vec<Hit>,
    scale: f32,
    size: (i32, i32),
    target: (i32, i32),
    animation: Option<crate::motion::Spring>,
    dock_animation: Option<crate::motion::Dock>,
    shown_notice: Option<(String, String)>,
    frame_clock: crate::frame_clock::FrameClock,
    frame_batch: bool,
    frame_requested: bool,
    frame_full: bool,
    wake_due: Option<u64>,
    pulse_timer: bool,
    pulse_only: bool,
    content_epoch: Option<[u64; 5]>,
    pointer_timer: bool,
    peek_through: bool,
    fullscreen_hidden: bool,
    fullscreen_blocker: Option<isize>,
    browser_hidden: bool,
    browser_fade: crate::visibility::Fade,
    browser_fade_timer: bool,
    browser_missing: crate::visibility::MissingTarget,
    browser_fade_samples: Vec<(bool, u64, u8, f64)>,
    browser_fade_hash: Option<u64>,
    browser_worker: crate::browser_space::BrowserWorker,
    browser_query: Option<crate::browser_space::Query>,
    browser_generation: u64,
    fullscreen_timer: bool,
    dragging: bool,
    move_start: Option<((i32, i32), (i32, i32))>,
    anchor: (i32, i32),
    taskbar_message: u32,
    self_test: bool,
    transition_profile: bool,
    profile_stage: usize,
    frame_timings: Vec<FrameTiming>,
    last_frame: Option<Instant>,
    test_stage: u32,
    test_start: u64,
    alarms: u32,
    timer_finished_compact: bool,
    test_clicks_ok: bool,
    test_audio_disabled: bool,
    nonzero_audio: bool,
    bluetooth_events: Vec<(String, bool, u64)>,
    z_order_attempts: std::cell::Cell<u32>,
    run_for_ms: Option<u64>,
    test_topmost_off: bool,
    test_water_notified: bool,
    test_anchored: bool,
    test_floating_restored: bool,
    test_floating_anchor: (i32, i32),
    test_pulse_source: Option<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>,
    test_lyrics_media: Option<crate::media::MediaSnapshot>,
    test_pulse_frames: [u32; 6],
    test_water_seconds: std::collections::BTreeSet<u64>,
    test_moved_anchor: Option<(i32, i32)>,
}
impl App {
    fn new(hwnd: HWND, taskbar_message: u32) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let bus = Bus {
            tx,
            hwnd: Arc::new(AtomicIsize::new(hwnd.0 as isize)),
            audio: Arc::new(Mutex::new(event::AudioFrame::default())),
            audio_pending: Arc::new(AtomicBool::new(false)),
        };
        let mut state = State::new(now_ms(), LocalTime::get().key());
        if crate::platform::argument("--data-dir").is_none() {
            state.startup = crate::startup::query();
        }
        if std::env::args().any(|a| a == "--self-test") {
            state.tab = Tab::Settings;
        }
        if std::env::args()
            .any(|a| a == "--self-test" || a == "--offline" || a == "--profile-transitions")
            || crate::platform::argument("--benchmark-scenario").is_some()
        {
            state.config.online_lyrics = false;
        }
        if let Some(scenario) = crate::platform::argument("--benchmark-scenario") {
            state.config.visualizer =
                matches!(scenario.as_str(), "audio" | "audio-expanded" | "all");
            state.expanded = matches!(scenario.as_str(), "audio-expanded" | "all" | "water");
            if scenario == "water" {
                state.tab = Tab::Water;
            }
            if matches!(scenario.as_str(), "timer" | "all") {
                state.timer.set_seconds(180);
                state.timer.start(now_ms());
            }
            if scenario == "all" {
                state.config.water.enabled = true;
                state.config.water.active_start = 0;
                state.config.water.active_end = 0;
                state.config.water.quiet_enabled = false;
            }
        }
        let transition_profile =
            std::env::args().any(|a| a == "--profile-transitions" || a == "--profile-docking");
        if transition_profile {
            state.config.stroke_enabled = !std::env::args().any(|a| a == "--no-stroke");
            state.config.stroke_color = 0xa69bff;
            state.config.stroke_width = 3;
            state.config.stroke_glow = 100;
            state.config.visualizer = false;
            state.config.water.enabled = false;
            if std::env::args().any(|a| a == "--profile-docking") {
                state.expanded = true;
                state.tab = Tab::Settings;
            }
            state.media = crate::media::MediaSnapshot {
                available: true,
                title: "Animation fixture".into(),
                artist: "Local diagnostic".into(),
                source: "Spotify".into(),
                ..Default::default()
            };
        }
        let tray = Tray::new(hwnd)?;
        let media = MediaWorker::start(bus.clone(), stop.clone());
        let lyrics = LyricsWorker::start(bus.clone(), stop.clone(), state.store.directory.clone());
        let audio = AudioWorker::start(
            bus.clone(),
            stop.clone(),
            state.config.visualizer,
            state.config.sensitivity,
            state.config.smoothing,
        )
        .ok();
        let bluetooth = BluetoothWorker::start(bus.clone(), stop.clone(), state.config.bluetooth);
        let offline = std::env::args()
            .any(|a| a == "--offline" || a == "--self-test" || a == "--profile-transitions")
            || crate::platform::argument("--benchmark-scenario").is_some();
        let weather = crate::weather::WeatherWorker::start(
            bus.clone(),
            stop.clone(),
            state.store.directory.clone(),
            state.config.weather_city.clone(),
            offline,
        );
        let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
        let scale = scale.max(1.0);
        let floating = state
            .config
            .position
            .map(|p| (p[0], p[1]))
            .unwrap_or_else(|| default_anchor(scale));
        let anchor = if state.config.dock_mode == crate::storage::DockMode::Anchored {
            state
                .config
                .anchored_position
                .map(|p| (p[0], p[1]))
                .unwrap_or_else(|| anchored_anchor(floating))
        } else {
            floating
        };
        let browser_worker = crate::browser_space::BrowserWorker::start(bus.clone());
        let frame_clock = crate::frame_clock::FrameClock::new(bus.hwnd.clone());
        let mut app = Self {
            hwnd,
            state,
            rx,
            bus,
            stop,
            media,
            lyrics,
            weather,
            edit_previous: None,
            note_writer: None,
            note_drag: false,
            note_surrogate: None,
            color_drag: None,
            color_dirty: false,
            design_checks: Default::default(),
            water_frame: None,
            audio,
            bluetooth,
            tray,
            canvas: None,
            hits: Vec::new(),
            scale,
            size: (ui::COMPACT_WIDTH, ui::COMPACT_HEIGHT),
            target: (ui::COMPACT_WIDTH, ui::COMPACT_HEIGHT),
            animation: None,
            dock_animation: None,
            shown_notice: None,
            frame_clock,
            frame_batch: false,
            frame_requested: false,
            frame_full: false,
            wake_due: None,
            pulse_timer: false,
            pulse_only: false,
            content_epoch: None,
            pointer_timer: false,
            peek_through: false,
            fullscreen_hidden: false,
            fullscreen_blocker: None,
            browser_hidden: false,
            browser_fade: Default::default(),
            browser_fade_timer: false,
            browser_missing: Default::default(),
            browser_fade_samples: Vec::new(),
            browser_fade_hash: None,
            browser_worker,
            browser_query: None,
            browser_generation: 0,
            fullscreen_timer: false,
            dragging: false,
            move_start: None,
            anchor,
            taskbar_message,
            self_test: std::env::args().any(|a| a == "--self-test"),
            transition_profile,
            profile_stage: 0,
            frame_timings: Vec::new(),
            last_frame: None,
            test_stage: 0,
            test_start: now_ms(),
            alarms: 0,
            timer_finished_compact: false,
            test_clicks_ok: true,
            test_audio_disabled: false,
            nonzero_audio: false,
            bluetooth_events: Vec::new(),
            z_order_attempts: std::cell::Cell::new(0),
            run_for_ms: crate::platform::argument("--run-for")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|s| s.clamp(1, 86400) * 1000),
            test_topmost_off: false,
            test_water_notified: false,
            test_anchored: false,
            test_floating_restored: false,
            test_floating_anchor: anchor,
            test_pulse_source: None,
            test_lyrics_media: None,
            test_pulse_frames: [0; 6],
            test_water_seconds: Default::default(),
            test_moved_anchor: None,
        };
        if app.audio.is_none() {
            app.state.audio_status = "Audio worker could not start".into();
        }
        if app.self_test
            && let Some(stage) = crate::platform::argument("--self-test-start-stage")
                .and_then(|s| s.parse::<u32>().ok())
        {
            let stage = stage.min(56);
            app.test_start = now_ms().saturating_sub(stage as u64 * 1000);
            app.test_stage = stage;
        }
        app.layout(false);
        Ok(app)
    }
    fn desired_size(&self) -> (i32, i32) {
        let size = if !self.state.notices.is_empty() {
            (384, 82)
        } else if self.state.expanded {
            (
                ui::expanded_width(&self.state),
                ui::expanded_height(&self.state),
            )
        } else {
            (ui::COMPACT_WIDTH, ui::COMPACT_HEIGHT)
        };
        (size.0 + 2 * ui::gutter(&self.state), size.1)
    }
    fn layout(&mut self, animate: bool) {
        let animate = animate && self.surface_visible();
        if self.dragging || (self.browser_hidden && self.browser_fade.active(now_ms())) {
            return;
        }
        let target = self.desired_size();
        let notice = self
            .state
            .notices
            .front()
            .map(|n| (n.title.clone(), n.detail.clone()));
        let notice_changed = notice.is_some() && notice != self.shown_notice;
        self.shown_notice = notice;
        if !animate {
            self.target = target;
            self.size = target;
            self.animation = None;
            self.dock_animation = None;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), 2);
            }
        }
        if animate && let Some(previous) = self.dock_animation {
            if target != self.target {
                self.clamp_position();
                let to = self.dock_target(target);
                self.dock_animation = Some(crate::motion::Dock::new(
                    now_ms(),
                    self.dock_pose(now_ms()),
                    to,
                    Some(previous),
                ));
                self.target = target;
            }
            return;
        }
        if target != self.target {
            let kind = if !self.state.notices.is_empty() {
                crate::motion::Kind::Notice
            } else if !self.state.expanded && self.target.1 > 82 {
                crate::motion::Kind::Collapse
            } else {
                crate::motion::Kind::Resize
            };
            self.target = target;
            if animate {
                self.animation = Some(crate::motion::Spring::new(
                    now_ms(),
                    self.size,
                    target,
                    self.animation,
                    kind,
                ));
                self.frame_clock.set_active(true);
            } else {
                self.size = target;
                self.animation = None;
            }
        }
        if animate && target == self.target && notice_changed && self.animation.is_none() {
            self.animation = Some(crate::motion::Spring::new(
                now_ms(),
                self.size,
                target,
                None,
                crate::motion::Kind::Notice,
            ));
            self.frame_clock.set_active(true);
        }
        self.clamp_position();
    }
    // Store a stable capsule anchor. Expanded panels fit the screen without changing it.
    fn clamp_position(&mut self) {
        self.anchor = self.fitted_anchor((
            ui::COMPACT_WIDTH + 2 * ui::gutter(&self.state),
            ui::COMPACT_HEIGHT,
        ));
    }
    fn fitted_anchor(&self, size: (i32, i32)) -> (i32, i32) {
        unsafe {
            let monitor = MonitorFromPoint(
                POINT {
                    x: self.anchor.0,
                    y: self.anchor.1,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                let anchored = self.state.config.dock_mode == crate::storage::DockMode::Anchored;
                let r = if anchored {
                    info.rcMonitor
                } else {
                    info.rcWork
                };
                let half = ((size.0 + 2 * ui::HALO) as f32 * self.scale / 2.0).round() as i32;
                let h = ((size.1 + ui::HALO) as f32 * self.scale).round() as i32;
                (
                    self.anchor
                        .0
                        .clamp(r.left + half, (r.right - half).max(r.left + half)),
                    if anchored {
                        r.top
                    } else {
                        self.anchor.1.clamp(
                            r.top + (ui::HALO as f32 * self.scale).round() as i32,
                            (r.bottom - h)
                                .max(r.top + (ui::HALO as f32 * self.scale).round() as i32),
                        )
                    },
                )
            } else {
                self.anchor
            }
        }
    }
    fn body_rect(&self) -> [f32; 4] {
        let pose = self.dock_pose(now_ms());
        [
            pose.anchor[0] - self.size.0 as f32 * self.scale / 2.0,
            pose.anchor[1],
            self.size.0 as f32 * self.scale,
            self.size.1 as f32 * self.scale,
        ]
    }
    fn dock_target(&self, size: (i32, i32)) -> crate::motion::DockPose {
        let anchor = self.fitted_anchor(size);
        crate::motion::DockPose {
            anchor: [anchor.0 as f32, anchor.1 as f32],
            size: [size.0 as f32, size.1 as f32],
            attached: if self.state.config.dock_mode == crate::storage::DockMode::Anchored {
                1.0
            } else {
                0.0
            },
        }
    }
    fn dock_pose(&self, now: u64) -> crate::motion::DockPose {
        self.dock_animation
            .map(|d| d.pose(now))
            .unwrap_or_else(|| self.dock_target(self.size))
    }
    fn content_origin(&self) -> (f32, f32) {
        let q = self.dock_pose(now_ms()).attached;
        (ui::HALO as f32 + 12.0 * q, ui::HALO as f32 * (1.0 - q))
    }
    fn pointer_at(&mut self, now: u64, point: [f32; 2], own_window: bool) {
        if self.dragging || !self.surface_visible() || !self.state.notices.is_empty() {
            return;
        }
        let body = self.body_rect();
        let activation = crate::interaction::activation_rect(body, self.scale);
        let inside = own_window
            && crate::interaction::contains(
                if self.state.expanded {
                    body
                } else {
                    activation
                },
                point,
                0.0,
            );
        let near = crate::interaction::near_capsule(body, point, self.scale);
        let idle = self.state.idle();
        // Keep hidden expression state current so collapsing never flashes stale tears.
        let updated = self.state.face.advance(
            now,
            [
                (point[0] - (body[0] + body[2] / 2.0)) / (400.0 * self.scale),
                (point[1] - (body[1] + 29.0 * self.scale)) / (300.0 * self.scale),
            ],
            idle && near,
        );
        let face_changed = idle && updated;
        let transition = self
            .state
            .interaction
            .update(now, inside, self.state.expanded);
        match transition {
            Some(crate::interaction::Transition::Open) => {
                self.state.expanded = true;
                if self.state.media.available {
                    self.state.tab = Tab::Media;
                }
                self.layout(true);
                self.schedule();
                self.paint();
            }
            Some(crate::interaction::Transition::Close) => {
                self.state.expanded = false;
                self.state.hover = None;
                self.layout(true);
                self.schedule();
                self.paint();
            }
            None => {
                if face_changed {
                    self.paint();
                }
            }
        }
    }
    fn set_pass_through(&self, enabled: bool) {
        unsafe {
            let old = GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32;
            let new = if enabled {
                old | WS_EX_TRANSPARENT.0
            } else {
                old & !WS_EX_TRANSPARENT.0
            };
            if old != new {
                SetWindowLongW(self.hwnd, GWL_EXSTYLE, new as i32);
            }
        }
    }
    fn surface_visible(&self) -> bool {
        self.state.visible && !self.peek_through && !self.fullscreen_hidden && !self.browser_hidden
    }
    fn base_visible(&self) -> bool {
        self.state.visible && !self.peek_through && !self.fullscreen_hidden
    }
    fn set_native_visible(&self, show: bool) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() != show {
                let _ = ShowWindow(self.hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    }
    fn sync_visibility(&mut self) {
        let now = now_ms();
        if !self.base_visible() {
            self.browser_fade.snap(!self.browser_hidden);
            self.stop_browser_fade_timer();
            self.set_pass_through(true);
            self.set_native_visible(false);
            return;
        }
        self.set_pass_through(self.browser_hidden);
        if self.browser_fade.active(now) {
            // Fade-in uploads its initial (possibly zero-alpha) frame before showing the HWND.
            if !self.browser_hidden {
                self.paint_immediate();
                self.set_native_visible(true);
            }
            self.browser_fade_timer = true;
            self.frame_clock.set_active(true);
        } else {
            self.browser_fade.snap(!self.browser_hidden);
            self.stop_browser_fade_timer();
            if !self.browser_hidden {
                self.paint_immediate();
            }
            self.set_native_visible(!self.browser_hidden);
        }
    }
    fn stop_browser_fade_timer(&mut self) {
        if self.browser_fade_timer {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), 9);
            }
            self.browser_fade_timer = false;
        }
    }
    fn browser_fade_tick(&mut self, now: u64) {
        if !self.base_visible() {
            self.sync_visibility();
            return;
        }
        let alpha = self.browser_fade.alpha(now, self.state.config.opacity);
        let started = Instant::now();
        if let Some(canvas) = &self.canvas
            && let Err(e) = canvas.fade_alpha(self.hwnd, alpha)
        {
            self.state
                .store
                .log(&format!("Opacity upload failed: {}", e.code()));
        }
        if self.self_test && self.test_stage >= 54 {
            self.browser_fade_samples.push((
                self.browser_hidden,
                now,
                alpha,
                started.elapsed().as_secs_f64() * 1000.0,
            ));
        }
        if !self.browser_fade.active(now) {
            self.browser_fade.snap(!self.browser_hidden);
            self.stop_browser_fade_timer();
            self.set_native_visible(!self.browser_hidden);
            self.layout(false);
            if !self.browser_hidden {
                self.paint_now();
            }
            self.schedule();
        }
    }
    fn set_fullscreen_hidden(&mut self, hidden: bool) {
        if hidden == self.fullscreen_hidden {
            return;
        }
        self.fullscreen_hidden = hidden;
        self.state.hover = None;
        if hidden {
            self.finish_color_drag();
            self.state.face.clear_tears();
            if self.state.interaction.preview {
                self.state.expanded = false;
            }
            if !self.state.expanded {
                self.state.interaction.minimize();
            }
            self.layout(false);
        }
        self.sync_visibility();
        self.schedule();
    }
    fn fullscreen_tick(&mut self) {
        if self.self_test || self.transition_profile {
            return;
        }
        let (hidden, blocker) =
            crate::fullscreen::active(self.hwnd, self.anchor, self.fullscreen_blocker);
        self.fullscreen_blocker = blocker;
        self.set_fullscreen_hidden(hidden);
        if !hidden && self.base_visible() && !unsafe { IsWindowVisible(self.hwnd).as_bool() } {
            self.sync_visibility();
        }
        self.browser_space_tick();
    }
    fn set_browser_hidden(&mut self, hidden: bool) {
        if hidden == self.browser_hidden {
            return;
        }
        self.browser_hidden = hidden;
        self.state.hover = None;
        if hidden {
            // Keep the current pixels/size throughout fade-out, including the glow.
            self.animation = None;
            self.dock_animation = None;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), 2);
            }
            self.state.face.clear_tears();
            if self.state.interaction.preview {
                self.state.expanded = false;
            }
            if !self.state.expanded {
                self.state.interaction.minimize();
            }
        }
        if self.base_visible()
            && self.canvas.is_some()
            && (!hidden || unsafe { IsWindowVisible(self.hwnd).as_bool() })
        {
            self.browser_fade.retarget(!hidden, now_ms());
        } else {
            self.browser_fade.snap(!hidden);
        }
        if !hidden || !self.browser_fade.active(now_ms()) {
            self.layout(false);
        }
        self.sync_visibility();
        self.schedule();
    }
    fn browser_eligible(&self) -> bool {
        self.state.visible
            && !self.fullscreen_hidden
            && !self.dragging
            && (!self.state.expanded || self.state.interaction.preview)
            && self.state.editing.is_none()
    }
    fn browser_space_tick(&mut self) {
        let allow = self.browser_eligible();
        let a = self.fitted_anchor((ui::COMPACT_WIDTH, ui::COMPACT_HEIGHT));
        let half = (ui::COMPACT_WIDTH + 2 * ui::gutter(&self.state)) as f32 * self.scale / 2.0;
        let area = crate::browser_space::Rect(
            (a.0 as f32 - half).floor() as i32,
            a.1,
            (a.0 as f32 + half).ceil() as i32,
            a.1 + (ui::COMPACT_HEIGHT as f32 * self.scale).ceil() as i32,
        );
        let window = if allow {
            crate::browser_space::target(self.hwnd, area)
        } else {
            None
        };
        let returning = self.browser_missing.pending() && window.is_some();
        if allow && window.is_none() && self.browser_query.is_some() {
            if !self.browser_missing.update(true, now_ms()) {
                return;
            }
        } else {
            self.browser_missing.update(false, now_ms());
        }
        if returning
            || self.browser_query.as_ref().map(|q| (q.window, q.area)) != window.map(|w| (w, area))
        {
            self.browser_generation = self.browser_generation.wrapping_add(1);
            self.browser_query = window.map(|window| crate::browser_space::Query {
                generation: self.browser_generation,
                window,
                area,
            });
            self.browser_worker.watch(self.browser_query);
        }
        if window.is_none() {
            self.set_browser_hidden(false);
        }
    }
    fn set_alt_bypass(&mut self, active: bool) {
        if active == self.peek_through {
            return;
        }
        self.peek_through = active;
        self.state.hover = None;
        if active {
            self.finish_color_drag();
            self.state.face.clear_tears();
            if self.state.interaction.preview {
                self.state.expanded = false;
            }
            if !self.state.expanded {
                self.state.interaction.minimize();
            }
            self.layout(false);
        }
        self.sync_visibility();
        self.schedule();
    }
    fn pointer_tick(&mut self) {
        if self.dragging || !self.state.visible || self.fullscreen_hidden || self.browser_hidden {
            return;
        }
        unsafe {
            let alt = GetAsyncKeyState(VK_MENU.0 as i32) < 0;
            self.set_alt_bypass(alt);
            if alt {
                return;
            }
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                let point = [p.x as f32, p.y as f32];
                let pass = crate::interaction::pass_through(
                    self.body_rect(),
                    point,
                    self.scale,
                    self.state.expanded,
                    !self.state.notices.is_empty(),
                    false,
                );
                self.set_pass_through(pass);
                let own = WindowFromPoint(p) == self.hwnd;
                self.pointer_at(now_ms(), point, own);
            }
            if self.color_dirty {
                self.paint();
            }
        }
    }
    fn begin_move(&mut self) {
        unsafe {
            let mut r = RECT::default();
            if GetWindowRect(self.hwnd, &mut r).is_ok() {
                self.move_start = Some((self.anchor, (r.left, r.top)));
            }
        }
        self.dragging = true;
        self.animation = None;
        self.state.interaction.pin();
        unsafe {
            let _ = KillTimer(Some(self.hwnd), 2);
        }
    }
    fn end_move(&mut self) {
        unsafe {
            let mut r = RECT::default();
            if GetWindowRect(self.hwnd, &mut r).is_ok() {
                self.anchor = if let Some((anchor, start)) = self.move_start.take()
                    && start == (r.left, r.top)
                {
                    anchor
                } else {
                    (
                        (r.left + r.right) / 2,
                        r.top + (ui::top_inset(&self.state) as f32 * self.scale).round() as i32,
                    )
                };
                self.dragging = false;
                self.clamp_position();
                if self.state.config.dock_mode == crate::storage::DockMode::Anchored {
                    self.state.config.anchored_position = Some([self.anchor.0, self.anchor.1]);
                } else {
                    self.state.config.position = Some([self.anchor.0, self.anchor.1]);
                }
                self.state.save();
            } else {
                self.dragging = false;
            }
        }
        self.layout(false);
        self.schedule();
        self.paint();
    }
    fn content_epoch(&self, now: u64) -> [u64; 5] {
        let s = &self.state;
        let media = (!s.expanded || s.tab == Tab::Media) && s.media.available;
        let p = if media { s.media.position(now) } else { 0 };
        let width = if s.expanded && s.show_lyrics {
            280
        } else {
            400
        };
        [
            p.div_ceil(1000),
            if media && s.media.duration_ms > 0 {
                p * width / s.media.duration_ms
            } else {
                0
            },
            if media {
                s.media.duration_ms.saturating_sub(p).div_ceil(1000)
            } else {
                0
            },
            if media {
                s.lyrics
                    .document()
                    .and_then(|d| {
                        d.active(s.lyrics.position(&s.media, now, s.config.lyrics_offset_ms))
                    })
                    .map(|i| i as u64 + 1)
                    .unwrap_or(0)
            } else {
                0
            },
            if s.tab == Tab::Timer || !s.expanded {
                s.timer.remaining_ms(now).div_ceil(1000)
            } else if s.tab == Tab::Water {
                s.water.next_due.saturating_sub(now).div_ceil(1000)
            } else {
                0
            },
        ]
    }
    fn render(&mut self) -> Result<()> {
        if !self.surface_visible() || self.dragging {
            return Ok(());
        }
        let started = Instant::now();
        let epoch = self.content_epoch(now_ms());
        let partial = if self.pulse_only
            && self.animation.is_none()
            && self.dock_animation.is_none()
            && self.content_epoch == Some(epoch)
        {
            ui::meter_rect(&self.state).map(|[x, y, w, h]| [x - 3, y - 3, w + 6, h + 6])
        } else {
            None
        };
        let pose = self.dock_pose(now_ms());
        let top = (ui::HALO as f32 * (1.0 - pose.attached)).round() as i32;
        let width = self.size.0 + 2 * ui::HALO;
        let height = self.size.1 + top + ui::HALO;
        let reusable = self
            .canvas
            .as_mut()
            .is_some_and(|c| (c.scale - self.scale).abs() < 0.001 && c.resize(width, height));
        if !reusable {
            self.canvas = Some(Canvas::with_capacity(
                width,
                height,
                ui::MEDIA_WIDTH + 24 + 2 * ui::HALO,
                ui::HEIGHT + 24 + 2 * ui::HALO,
                self.scale,
            )?);
        }
        let allocated = started.elapsed().as_secs_f64() * 1000.0;
        let alpha = self.browser_fade.alpha(now_ms(), self.state.config.opacity);
        let c = self.canvas.as_mut().unwrap();
        c.set_dock_mix(self.dock_animation.map(|_| pose.attached));
        c.set_partial(if reusable { partial } else { None });
        let testing_pulse = self.test_pulse_source.is_some();
        c.profiling = self.transition_profile || testing_pulse;
        let now = now_ms();
        let local = LocalTime::get();
        self.hits = if !self.state.expanded
            && self.state.notices.is_empty()
            && let Some(visual) = self.animation.and_then(|a| a.collapse_visual(now))
        {
            ui::draw_collapsing(c, &self.state, now, &local, self.size, visual)
        } else {
            ui::draw(c, &self.state, now, &local)
        };
        c.finish()?;
        let drawn = started.elapsed().as_secs_f64() * 1000.0;
        c.present(
            self.hwnd,
            pose.anchor[0].round() as i32 - c.width / 2,
            pose.anchor[1].round() as i32 - (top as f32 * self.scale).round() as i32,
            alpha,
            if self.state.expanded && self.state.notices.is_empty() {
                28
            } else {
                if self.state.notices.is_empty() {
                    ui::COMPACT_HEIGHT / 2
                } else {
                    41
                }
            },
            self.state.config.dock_mode,
        )?;
        let phases = c.timings.get();
        self.content_epoch = Some(epoch);
        self.apply_z_order();
        if testing_pulse {
            let index = match self.state.tab {
                Tab::Media => 0,
                Tab::Weather => 1,
                Tab::Timer => 2,
                Tab::Water => 3,
                Tab::Notes => 5,
                Tab::Settings => 4,
            };
            if self.pulse_visible(now_ms()) {
                self.test_pulse_frames[index] += 1;
            }
        }
        if self.self_test
            && (29..33).contains(&self.test_stage)
            && self.state.tab == Tab::Water
            && self.state.expanded
        {
            self.test_water_seconds.insert(
                self.state
                    .water
                    .next_due
                    .saturating_sub(now_ms())
                    .div_ceil(1000),
            );
        }
        if self.transition_profile || testing_pulse {
            let total = started.elapsed().as_secs_f64() * 1000.0;
            let interval = self
                .last_frame
                .map(|last| started.duration_since(last).as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            self.last_frame = Some(started);
            self.frame_timings.push(FrameTiming {
                width: self.size.0,
                height: self.size.1,
                animating: self.animation.is_some() || self.dock_animation.is_some(),
                surface_ms: allocated,
                mask_build_ms: phases[0],
                mask_apply_ms: phases[1],
                upload_ms: phases[2],
                draw_ms: drawn - allocated,
                present_ms: total - drawn,
                total_ms: total,
                interval_ms: interval,
            });
        }
        Ok(())
    }
    fn pulse_visible(&self, now: u64) -> bool {
        self.surface_visible()
            && ui::meter_rect(&self.state).is_some()
            && !self.browser_fade.active(now)
            && self.state.notices.is_empty()
            && crate::media::pulse::active(
                &self.state.bars,
                self.state.audio_at_ms,
                now,
                self.state.config.visualizer,
            )
    }
    fn pulse_tick(&mut self) {
        if !self.pulse_visible(now_ms()) {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), 6);
            }
            self.pulse_timer = false;
        }
        self.pulse_only = true;
        self.paint();
        self.pulse_only = false;
    }
    fn schedule(&mut self) {
        let now = now_ms();
        let local = LocalTime::get();
        let mut delay = 60_000 - local.second as u64 * 1000 - local.milliseconds as u64;
        if let Some(due) = self.state.note.due {
            delay = delay.min(due.saturating_sub(now));
        }
        if self.state.expanded && self.state.tab == Tab::Notes && self.surface_visible() {
            if self.state.editing == Some(EditField::Note) {
                delay = delay.min(500 - now.saturating_sub(self.state.note.caret_at) % 500);
            }
            if self.state.note.copied_until > now {
                delay = delay.min(self.state.note.copied_until - now);
            }
        }
        if self.state.timer.status == Status::Running {
            let remaining = self.state.timer.remaining_ms(now);
            delay = delay.min(crate::timer::next_display_change(remaining));
        }
        if self.state.visible
            && self.state.expanded
            && self.state.tab == Tab::Weather
            && self.state.weather.refresh_after_ms > now
        {
            delay = delay.min(self.state.weather.refresh_after_ms - now);
        }
        if self.state.config.water.enabled
            && !self.state.water.pending
            && self.state.water.next_due > now
        {
            let remaining = self.state.water.next_due - now;
            delay = delay.min(remaining);
            if self.state.visible
                && self.state.expanded
                && self.state.tab == Tab::Water
                && !self.state.water_schedule
                && self.state.notices.is_empty()
            {
                delay = delay.min(crate::timer::next_display_change(remaining));
            }
        }
        if !self.state.notices.is_empty() {
            delay = delay.min(self.state.notice_until.saturating_sub(now));
        }
        if self.state.visible && self.state.notices.is_empty() && self.state.media.playing {
            let position = self.state.lyrics.position(
                &self.state.media,
                now,
                self.state.config.lyrics_offset_ms,
            );
            if let Some(document) = self.state.lyrics.document()
                && self.state.media.timeline_available
                && let Some(next) = document.next_delay(position)
            {
                delay = delay
                    .min((next as f64 / self.state.media.playback_rate.max(0.01)).ceil() as u64);
            }
            if self.state.expanded && self.state.tab == Tab::Media {
                delay = delay.min(1000 - self.state.media.position(now) % 1000);
            }
        }
        let pulse = self.pulse_visible(now);
        let pointer = self.state.visible
            && !self.fullscreen_hidden
            && !self.browser_hidden
            && !self.self_test
            && !self.transition_profile;
        unsafe {
            if !self.fullscreen_timer && !self.self_test && !self.transition_profile {
                SetTimer(Some(self.hwnd), 8, 200, None);
                self.fullscreen_timer = true;
            }
            self.pointer_timer = pointer;
            self.pulse_timer = pulse;
            self.frame_clock.set_active(
                pointer
                    || pulse
                    || self.animation.is_some()
                    || self.dock_animation.is_some()
                    || self.browser_fade_timer,
            );
            let delay = delay.clamp(16, 60_000);
            let due = now.saturating_add(delay);
            // Frequent media/status wakes must not replace an already-due
            // WM_TIMER with the following second's deadline before it paints.
            if self
                .wake_due
                .is_none_or(|previous| due.saturating_add(1) < previous)
                && SetTimer(Some(self.hwnd), 1, delay as u32, None) != 0
            {
                self.wake_due = Some(due);
            }
        }
    }
    fn notify(&mut self, title: &str, detail: &str, tab: Tab) {
        self.state.notify(
            Notice {
                title: title.into(),
                detail: detail.into(),
                tab,
                device: None,
            },
            now_ms(),
        );
    }
    fn tick(&mut self) {
        let now = now_ms();
        self.queue_note(false);
        let local = LocalTime::get();
        if self.state.timer.tick(now) {
            self.alarms += 1;
            self.timer_finished_compact = !self.state.expanded;
            if self.state.config.alarm_sound {
                unsafe {
                    let _ = PlaySoundW(
                        w!("SystemExclamation"),
                        None,
                        SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
                    );
                }
            }
            self.notify(
                "Your timer is complete",
                "A little break is yours.",
                Tab::Timer,
            );
        }
        let old_day = self.state.water.day;
        if self
            .state
            .water
            .tick(now, local.hour, local.key(), &self.state.config.water)
        {
            self.test_water_notified |= !self.state.expanded;
            self.notify(
                "Time for a sip",
                "Click to check in or snooze for 10 minutes.",
                Tab::Water,
            );
        }
        if old_day != self.state.water.day {
            self.state.save();
        }
        if !self.state.notices.is_empty() && now >= self.state.notice_until {
            self.state.dismiss_notice(now);
        }
        self.layout(true);
        self.schedule();
    }
    fn events(&mut self) {
        let mut redraw = false;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::NoteSaved(revision, success) => {
                    self.state.note.completed(revision, success, now_ms());
                    redraw |= self.state.tab == Tab::Notes && self.state.expanded;
                }
                Event::BrowserSpace(generation, hidden) => {
                    if generation == self.browser_generation
                        && self.browser_query.is_some()
                        && self.browser_eligible()
                        && !self.browser_missing.pending()
                    {
                        self.set_browser_hidden(hidden);
                    }
                }
                Event::Media(media) => {
                    if self.test_lyrics_media.is_some() {
                        self.test_lyrics_media = Some(media);
                        continue;
                    }
                    if self.transition_profile
                        || (self.self_test && (29..33).contains(&self.test_stage))
                    {
                        continue;
                    }
                    self.state.media = media;
                    self.refresh_lyrics(false);
                    self.layout(true);
                    redraw = true;
                }
                Event::Lyrics(key, status) => {
                    if self.state.lyrics.key.as_ref() == Some(&key) {
                        self.state.lyrics.status = status;
                        redraw = true;
                    }
                }
                Event::AudioStatus(status) => {
                    self.state.audio_status = status;
                    redraw = true;
                }
                Event::BluetoothStatus(status) => {
                    self.state.bluetooth_status = status;
                    redraw = true;
                }
                Event::Bluetooth {
                    name,
                    connected,
                    kind,
                } => {
                    if self.state.config.bluetooth {
                        let name = if name.is_empty() {
                            "Bluetooth device".into()
                        } else {
                            name
                        };
                        let now = now_ms();
                        self.bluetooth_events
                            .retain(|e| now.saturating_sub(e.2) < 2000);
                        if !self
                            .bluetooth_events
                            .iter()
                            .any(|e| e.0 == name && e.1 == connected)
                        {
                            self.bluetooth_events.push((name.clone(), connected, now));
                            self.notify(
                                if connected {
                                    "Bluetooth connected"
                                } else {
                                    "Bluetooth disconnected"
                                },
                                &name,
                                Tab::Settings,
                            );
                            if let Some(n) = self.state.notices.back_mut() {
                                n.device = Some(kind);
                                n.title = format!(
                                    "{} {}",
                                    kind.label(),
                                    if connected {
                                        "connected"
                                    } else {
                                        "disconnected"
                                    }
                                );
                            }
                            self.layout(true);
                            redraw = true;
                        }
                    }
                }
                Event::Weather(city, result) => {
                    if self.state.config.weather_city.as_ref() == Some(&city) {
                        self.state.weather.refreshing = false;
                        match result {
                            Ok(f) => {
                                self.state.weather.forecast = Some(f);
                                self.state.weather.error.clear();
                            }
                            Err(e) => self.state.weather.error = e,
                        }
                        redraw = true;
                    }
                }
                Event::Cities(generation, result) => {
                    if generation == self.state.weather.generation {
                        self.state.weather.searching = false;
                        match result {
                            Ok(cities) => {
                                self.state.weather.search_error = if cities.is_empty() {
                                    "No matching cities. Try a nearby city.".into()
                                } else {
                                    String::new()
                                };
                                self.state.weather.cities = cities;
                            }
                            Err(e) => self.state.weather.search_error = e,
                        }
                        redraw = true;
                    }
                }
                Event::Error(message) => {
                    self.state.store.log(&message);
                    self.state.status = message;
                    redraw = true;
                }
            }
        }
        // Clear the wake flag while holding the sample lock so a new frame cannot be lost.
        let frame = {
            let frame = self.bus.audio.lock().unwrap_or_else(|e| e.into_inner());
            self.bus.audio_pending.store(false, Ordering::Release);
            *frame
        };
        self.state.bars = frame.levels;
        self.state.audio_at_ms = frame.at_ms;
        if self.state.bars.iter().any(|v| *v > 0.01) {
            self.nonzero_audio = true;
        }
        // The pulse clock paints every tab. Coalesce audio wakes rather than drawing twice.
        if redraw || (self.pulse_timer && !self.pulse_visible(now_ms())) {
            self.paint();
        }
        self.schedule();
    }
    fn paint(&mut self) {
        if self.animation.is_none() {
            if self.frame_clock.is_active() && !self.frame_batch {
                self.frame_requested = true;
                self.frame_full |= !self.pulse_only;
            } else {
                self.paint_now();
            }
        }
    }
    fn paint_now(&mut self) {
        if self.frame_batch {
            self.frame_requested = true;
            self.frame_full |= !self.pulse_only;
            return;
        }
        self.paint_immediate();
    }
    fn paint_immediate(&mut self) {
        self.color_dirty = false;
        if let Err(e) = self.render() {
            self.canvas = None;
            self.state
                .store
                .log(&format!("Window rendering failed: {}", e.code()));
        }
    }
    fn refresh_lyrics(&mut self, force: bool) {
        let key = TrackKey::from_media(&self.state.media);
        if !force && key == self.state.lyrics.key {
            return;
        }
        self.state.lyrics.key = key.clone();
        self.state.lyrics.scroll = 0;
        self.state.lyrics.status = if key.is_some() {
            LyricsStatus::Loading
        } else {
            LyricsStatus::Empty
        };
        if let Some(key) = key {
            let _ = self.lyrics.tx.send(LyricsRequest::Lookup {
                key,
                online: self.state.config.online_lyrics,
                force,
            });
        }
    }
    fn color_point(&self, lp: LPARAM) -> (f32, f32) {
        let (origin, top) = self.content_origin();
        (
            lp.0 as i16 as f32 / self.scale - origin,
            (lp.0 >> 16) as i16 as f32 / self.scale - top,
        )
    }
    fn begin_color_drag(&mut self, x: f32, y: f32) -> bool {
        if !self.state.expanded
            || self.state.tab != Tab::Settings
            || self.state.settings_page != 3
            || self.state.editing.is_some()
        {
            return false;
        }
        let Some(part) = crate::color::hit(x, y) else {
            return false;
        };
        if self.state.stroke_hsv.rgb() != self.state.config.stroke_color {
            self.state.stroke_hsv = crate::color::Hsv::from_rgb(self.state.config.stroke_color);
        }
        self.state.interaction.pin();
        self.color_drag = Some(part);
        unsafe {
            SetCapture(self.hwnd);
        }
        self.drag_color(x, y);
        true
    }
    fn drag_color(&mut self, x: f32, y: f32) {
        if let Some(part) = self.color_drag {
            let before = self.state.stroke_hsv;
            self.state.stroke_hsv.pick(part, x, y);
            let color = self.state.stroke_hsv.rgb();
            if self.state.stroke_hsv != before {
                self.state.config.stroke_color = color;
                self.state.status.clear();
                self.color_dirty = true;
                if self.self_test {
                    self.paint();
                }
            }
        }
    }
    fn finish_color_drag(&mut self) {
        if self.color_drag.take().is_some() {
            // One atomic settings write per gesture; every intermediate color is previewed in memory.
            self.state.save();
            if self.color_dirty {
                self.paint();
            }
            unsafe {
                if GetCapture() == self.hwnd {
                    let _ = ReleaseCapture();
                }
            }
        }
    }
    fn queue_note(&mut self, force: bool) {
        let n = &self.state.note;
        if !n.readable
            || n.editor.revision == n.saved
            || n.editor.revision == n.queued
            || (!force && n.due.is_none_or(|due| now_ms() < due))
        {
            return;
        }
        if self.note_writer.is_none() {
            match crate::notes::Writer::new(self.state.store.directory.clone(), self.bus.clone()) {
                Ok(w) => self.note_writer = Some(w),
                Err(_) => {
                    self.state.note.error = true;
                    self.state.note.due = Some(now_ms() + 5_000);
                    return;
                }
            }
        }
        let n = &mut self.state.note;
        self.note_writer
            .as_ref()
            .unwrap()
            .submit(n.editor.revision, n.editor.text.clone());
        n.queued = n.editor.revision;
        n.due = None;
    }
    fn note_refresh(&mut self, previous: u64) {
        self.state.note.changed(previous, now_ms());
        if let Some(c) = &mut self.canvas {
            let (_, y) = c.note_caret(&self.state.note.editor);
            let height = c.note_height(&self.state.note.editor.text);
            let e = &mut self.state.note.editor;
            let visible = crate::notes::TEXT[3] as f32;
            if y < e.scroll {
                e.scroll = y;
            }
            if y + 22. > e.scroll + visible {
                e.scroll = y + 22. - visible;
            }
            e.scroll = e.scroll.clamp(0., (height - visible).max(0.));
        }
        self.schedule();
        self.paint();
    }
    fn note_point(&mut self, x: f32, y: f32, extend: bool) {
        if !self.state.note.readable {
            return;
        }
        if self.state.editing != Some(EditField::Note) {
            self.begin_edit(EditField::Note);
        }
        if let Some(c) = &mut self.canvas {
            let [tx, ty, _, h] = crate::notes::TEXT;
            if extend && (y < ty as f32 || y > (ty + h) as f32) {
                let height = c.note_height(&self.state.note.editor.text);
                self.state.note.editor.scroll = (self.state.note.editor.scroll
                    + if y < ty as f32 { -22. } else { 22. })
                .clamp(0., (height - h as f32).max(0.));
            }
            let e = &mut self.state.note.editor;
            let p = c.note_hit(e, x - tx as f32, y - ty as f32 + e.scroll);
            e.select(p, extend);
        }
        let revision = self.state.note.editor.revision;
        self.note_refresh(revision);
    }
    fn note_copy(&mut self, all: bool) -> bool {
        let e = &self.state.note.editor;
        let r = if all { 0..e.text.len() } else { e.selection() };
        if r.is_empty() {
            return false;
        }
        let ok = crate::notes::clipboard::copy(self.hwnd, &e.text[r]).is_ok();
        if ok {
            self.state.note.copied_until = now_ms() + 1500;
        }
        ok
    }
    fn note_key(&mut self, key: u16) {
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 };
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 };
        let previous = self.state.note.editor.revision;
        match key {
            0x41 if ctrl => self.state.note.editor.select_all(),
            0x43 if ctrl => {
                self.note_copy(false);
            }
            0x58 if ctrl => {
                if self.note_copy(false) {
                    self.state.note.editor.insert("");
                }
            }
            0x56 if ctrl => {
                if let Ok(text) = crate::notes::clipboard::paste(self.hwnd) {
                    self.state.note.editor.insert(&text);
                }
            }
            0x5a if ctrl => self.state.note.editor.undo(shift),
            0x59 if ctrl => self.state.note.editor.undo(true),
            0x53 if ctrl => self.queue_note(true),
            0x08 if ctrl => self.state.note.editor.delete_word(true),
            0x2e if ctrl => self.state.note.editor.delete_word(false),
            0x2e => self.state.note.editor.delete(false),
            0x25 | 0x27 => self.state.note.editor.horizontal(key == 0x27, shift, ctrl),
            0x24 | 0x23 if ctrl => {
                let len = self.state.note.editor.text.len();
                self.state
                    .note
                    .editor
                    .select(if key == 0x24 { 0 } else { len }, shift);
            }
            0x26 | 0x28 | 0x24 | 0x23 | 0x21 | 0x22 => {
                if let Some(c) = &mut self.canvas {
                    let e = &mut self.state.note.editor;
                    let (x, y) = c.note_caret(e);
                    let (x, y) = match key {
                        0x24 => (0., y + 10.),
                        0x23 => (10_000., y + 10.),
                        0x26 => (x, y - 12.),
                        0x28 => (x, y + 32.),
                        0x21 => (x, y - 210.),
                        _ => (x, y + 230.),
                    };
                    let p = c.note_hit(e, x, y);
                    e.select(p, shift);
                }
            }
            0x1b => self.end_edit(),
            0x0d if ctrl => self.end_edit(),
            _ => {}
        }
        self.note_refresh(previous);
    }
    fn note_char(&mut self, unit: u32) {
        if unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 } {
            return;
        }
        let previous = self.state.note.editor.revision;
        match unit {
            8 => self.state.note.editor.delete(true),
            13 => self.state.note.editor.insert("\n"),
            9 => self.state.note.editor.insert("    "),
            0xd800..=0xdbff => self.note_surrogate = Some(unit as u16),
            0xdc00..=0xdfff => {
                if let Some(high) = self.note_surrogate.take() {
                    let s = String::from_utf16_lossy(&[high, unit as u16]);
                    self.state.note.editor.insert(&s);
                }
            }
            n if n >= 32 => {
                self.note_surrogate = None;
                if let Some(c) = char::from_u32(n) {
                    self.state.note.editor.insert(&c.to_string());
                }
            }
            _ => {}
        }
        self.note_refresh(previous);
    }
    fn begin_edit(&mut self, field: EditField) {
        if self.state.editing == Some(field) {
            return;
        }
        if field == EditField::Note && !self.state.note.readable {
            return;
        }
        self.note_surrogate = None;
        if field == EditField::Note {
            self.state.note.caret_at = now_ms();
            self.state.interaction.pin();
        }
        self.state.editing = Some(field);
        self.state.edit_select_all =
            field == EditField::Stroke || matches!(field, EditField::Timer(_));
        unsafe {
            self.edit_previous = Some(GetForegroundWindow());
            let style = GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32 & !WS_EX_NOACTIVATE.0;
            SetWindowLongW(self.hwnd, GWL_EXSTYLE, style as i32);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
    }
    fn end_edit(&mut self) {
        if let Some(EditField::Timer(part)) = self.state.editing {
            self.commit_timer_input(part);
        }
        if self.state.editing == Some(EditField::Note) {
            self.queue_note(true);
            self.note_surrogate = None;
        }
        if self.state.editing.take().is_some() {
            unsafe {
                SetWindowLongW(
                    self.hwnd,
                    GWL_EXSTYLE,
                    (GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32 | WS_EX_NOACTIVATE.0) as i32,
                );
                if GetForegroundWindow() == self.hwnd
                    && let Some(previous) = self.edit_previous.take()
                {
                    let _ = SetForegroundWindow(previous);
                }
            }
        }
    }
    fn submit_edit(&mut self) {
        match self.state.editing {
            Some(EditField::Timer(_)) => self.end_edit(),
            Some(EditField::Note) => self.end_edit(),
            Some(EditField::City) => {
                let query = self.state.edit_text.trim().to_string();
                if query.chars().count() < 2 {
                    self.state.weather.search_error = "Enter at least two letters.".into();
                    return;
                }
                self.state.weather.generation += 1;
                self.state.weather.searching = true;
                self.state.weather.search_error.clear();
                let _ = self.weather.tx.send(crate::weather::Request::Search(
                    query,
                    self.state.weather.generation,
                ));
                self.end_edit();
            }
            Some(EditField::Stroke) => {
                let hex = self.state.edit_text.trim().trim_start_matches('#');
                if hex.len() == 6
                    && let Ok(color) = u32::from_str_radix(hex, 16)
                {
                    self.state.config.stroke_color = color;
                    self.state.stroke_hsv = crate::color::Hsv::from_rgb(color);
                    self.state.status.clear();
                    self.state.save();
                    self.end_edit();
                } else {
                    self.state.status = "Use six hex digits, for example #AEA0D9".into();
                }
            }
            None => {}
        }
    }
    fn commit_timer_input(&mut self, part: crate::timer::Part) {
        if self.state.timer.status != Status::Running
            && let Some(seconds) =
                part.replace(self.state.timer.duration_ms / 1000, &self.state.edit_text)
            && seconds != self.state.timer.duration_ms / 1000
        {
            self.state.timer.set_seconds(seconds);
            self.state.config.timer_seconds = seconds;
            self.state.save();
        }
    }
    fn edit_timer(&mut self, part: crate::timer::Part) {
        if self.state.timer.status == Status::Running {
            return;
        }
        if let Some(EditField::Timer(previous)) = self.state.editing {
            self.commit_timer_input(previous);
            self.state.editing = Some(EditField::Timer(part));
        } else {
            self.end_edit();
            self.begin_edit(EditField::Timer(part));
        }
        self.state.edit_text = part.value(self.state.timer.duration_ms / 1000).to_string();
        self.state.edit_select_all = true;
        self.paint();
    }
    fn timer_char(&mut self, character: u32) {
        let Some(EditField::Timer(part)) = self.state.editing else {
            return;
        };
        match character {
            9 => self.edit_timer(part.other()),
            13 => self.end_edit(),
            27 => {
                self.state.edit_text.clear(); // Empty input cancels instead of changing the duration.
                self.end_edit();
            }
            1 => self.state.edit_select_all = true,
            8 => {
                if self.state.edit_select_all {
                    self.state.edit_text.clear();
                } else {
                    self.state.edit_text.pop();
                }
                self.state.edit_select_all = false;
            }
            22 => {
                if let Ok(text) = crate::notes::clipboard::paste(self.hwnd)
                    && part
                        .replace(self.state.timer.duration_ms / 1000, &text)
                        .is_some()
                {
                    self.state.edit_text = text.trim().to_string();
                    self.state.edit_select_all = false;
                }
            }
            48..=57 => {
                if self.state.edit_select_all {
                    self.state.edit_text.clear();
                }
                self.state.edit_select_all = false;
                if self.state.edit_text.len() < 5 {
                    self.state
                        .edit_text
                        .push(char::from_u32(character).unwrap());
                }
            }
            _ => {}
        }
        self.schedule();
        self.paint();
    }
    fn action(&mut self, action: Action) {
        self.finish_color_drag();
        if matches!(
            action,
            Action::Select(_)
                | Action::SettingsPage(_)
                | Action::Collapse
                | Action::Hide
                | Action::Preset(_)
                | Action::Duration(_)
                | Action::TimerToggle
                | Action::TimerReset
        ) {
            self.end_edit();
        }
        match action {
            Action::Collapse => self.state.interaction.minimize(),
            Action::Expand => {
                if self.state.expanded {
                    self.state.interaction.minimize()
                } else {
                    self.state.interaction.pin()
                }
            }
            Action::Hide => self.state.interaction.pin(),
            _ => self.state.interaction.pin(),
        }
        self.tick();
        let now = now_ms();
        let mut persist = false;
        match action {
            Action::TimerEdit(part) => self.edit_timer(part),
            Action::NoteEdit => self.begin_edit(EditField::Note),
            Action::IconAttribution => unsafe {
                windows::Win32::UI::Shell::ShellExecuteW(
                    Some(self.hwnd),
                    w!("open"),
                    w!("https://icons8.com/"),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            },
            Action::WeatherAttribution => unsafe {
                windows::Win32::UI::Shell::ShellExecuteW(
                    None,
                    w!("open"),
                    w!("https://open-meteo.com/en/licence"),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            },
            Action::NoteCopy => {
                self.note_copy(true);
            }
            Action::LyricsToggle => self.state.show_lyrics = !self.state.show_lyrics,
            Action::PickColor(_) => {}
            Action::CustomStroke => {
                self.state.edit_text = format!("#{:06X}", self.state.config.stroke_color);
                self.begin_edit(EditField::Stroke);
            }
            Action::WeatherSearch => {
                self.state.weather.choosing = true;
                self.state.weather.generation += 1;
                self.state.weather.cities.clear();
                self.state.weather.search_error.clear();
                self.state.edit_text.clear();
                self.begin_edit(EditField::City);
            }
            Action::WeatherSubmit => self.submit_edit(),
            Action::WeatherCancel => {
                self.end_edit();
                self.state.weather.choosing = false;
                self.state.weather.generation += 1;
            }
            Action::WeatherChoose(index) => {
                if let Some(city) = self.state.weather.cities.get(index).cloned() {
                    self.state.config.weather_city = Some(city.clone());
                    self.state.weather.forecast = None;
                    self.state.weather.error.clear();
                    self.state.weather.refreshing = false;
                    self.state.weather.refresh_after_ms = 0;
                    self.state.weather.choosing = false;
                    let _ = self
                        .weather
                        .tx
                        .send(crate::weather::Request::Select(Some(city)));
                    persist = true;
                }
            }
            Action::WeatherRefresh => {
                if !self.state.weather.refreshing
                    && now >= self.state.weather.refresh_after_ms
                    && self.state.config.weather_city.is_some()
                    && self
                        .weather
                        .tx
                        .send(crate::weather::Request::Refresh)
                        .is_ok()
                {
                    self.state.weather.refreshing = true;
                    self.state.weather.refresh_after_ms = now + 60_000;
                    self.state.weather.error.clear();
                }
            }
            Action::LyricsSettings => {
                self.state.expanded = true;
                self.state.tab = Tab::Settings;
                self.state.settings_page = 2;
            }
            Action::Dock(mode) => {
                if mode != self.state.config.dock_mode {
                    let from = self.dock_pose(now);
                    let previous = self.dock_animation;
                    if self.state.config.dock_mode == crate::storage::DockMode::Floating {
                        self.state.config.position = Some([self.anchor.0, self.anchor.1]);
                    } else {
                        self.state.config.anchored_position = Some([self.anchor.0, self.anchor.1]);
                    }
                    self.anchor = if mode == crate::storage::DockMode::Anchored {
                        self.state
                            .config
                            .anchored_position
                            .map(|p| (p[0], p[1]))
                            .unwrap_or_else(|| anchored_anchor(self.anchor))
                    } else {
                        self.state
                            .config
                            .position
                            .map(|p| (p[0], p[1]))
                            .unwrap_or_else(|| default_anchor(self.scale))
                    };
                    self.state.config.dock_mode = mode;
                    self.clamp_position();
                    if self.surface_visible() {
                        self.target = self.desired_size();
                        self.animation = None;
                        self.dock_animation = Some(crate::motion::Dock::new(
                            now,
                            from,
                            self.dock_target(self.target),
                            previous,
                        ));
                        self.frame_clock.set_active(true);
                    }
                    persist = true;
                }
            }
            Action::LyricsRetry => self.refresh_lyrics(true),
            Action::LyricsScroll(delta) => {
                let count = self
                    .state
                    .lyrics
                    .document()
                    .map(|d| {
                        if d.plain.is_empty() {
                            d.lines.len()
                        } else {
                            d.plain.len()
                        }
                    })
                    .unwrap_or(0);
                self.state.lyrics.scroll = self
                    .state
                    .lyrics
                    .scroll
                    .saturating_add_signed(delta as isize)
                    .min(count.saturating_sub(3));
            }
            Action::Expand => {
                let opening = !self.state.expanded;
                if opening && self.state.media.available {
                    self.state.tab = Tab::Media;
                }
                self.state.expanded = opening;
            }
            Action::Collapse => {
                self.state.face.clear_tears();
                if !self.state.notices.is_empty() {
                    self.state.dismiss_notice(now);
                } else {
                    self.state.expanded = false;
                }
            }
            Action::Select(tab) => {
                if !self.state.notices.is_empty() {
                    self.state.dismiss_notice(now);
                }
                self.state.expanded = true;
                self.state.tab = tab;
                if tab == Tab::Settings && crate::platform::argument("--data-dir").is_none() {
                    self.state.startup = crate::startup::query();
                }
                if tab == Tab::Notes {
                    self.begin_edit(EditField::Note);
                }
            }
            Action::Play | Action::Next | Action::Previous => {
                let p = match action {
                    Action::Next => Playback::Next,
                    Action::Previous => Playback::Previous,
                    _ => Playback::Toggle,
                };
                let _ = self.media.tx.try_send(Command::Playback(p));
            }
            Action::TimerToggle => {
                if self.state.timer.status == Status::Running {
                    self.state.timer.pause(now);
                } else {
                    self.state.timer.start(now);
                }
            }
            Action::TimerReset => self.state.timer.reset(),
            Action::Preset(seconds) => {
                if self.state.timer.status != Status::Running {
                    self.state.timer.set_seconds(seconds);
                    self.state.config.timer_seconds = seconds;
                    persist = true;
                }
            }
            Action::Duration(delta) => {
                if self.state.timer.status != Status::Running {
                    let seconds = (self.state.timer.duration_ms as i64 / 1000 + delta as i64)
                        .clamp(1, 86400) as u64;
                    self.state.timer.set_seconds(seconds);
                    self.state.config.timer_seconds = seconds;
                    persist = true;
                }
            }
            Action::WaterConfirm => {
                self.state.water.confirm(now, &self.state.config.water);
                self.state.notices.retain(|n| n.tab != Tab::Water);
                let played = crate::sound::water_check_in();
                if self.self_test {
                    self.design_checks
                        .insert("water_check_in_sound_async_started", played);
                }
                persist = true;
            }
            Action::WaterSnooze => {
                self.state.water.snooze(now);
                self.state.notices.retain(|n| n.tab != Tab::Water);
            }
            Action::WaterToggle => {
                self.state.config.water.enabled = !self.state.config.water.enabled;
                self.state.water.reschedule(now, &self.state.config.water);
                if !self.state.config.water.enabled {
                    self.state.notices.retain(|n| n.tab != Tab::Water);
                }
                persist = true;
            }
            Action::WaterSchedule => self.state.water_schedule = !self.state.water_schedule,
            Action::SettingsPage(page) => {
                self.state.settings_page = page;
                if page == 0 && crate::platform::argument("--data-dir").is_none() {
                    self.state.startup = crate::startup::query();
                }
            }
            Action::Toggle(Setting::Startup) => {
                if crate::platform::argument("--data-dir").is_some() {
                    self.state.startup.registered = !self.state.startup.registered;
                } else {
                    let status = crate::startup::query();
                    if status.registered && status.blocked_by_windows {
                        crate::startup::manage_in_windows();
                        self.state.startup = status;
                    } else {
                        self.state.startup = match crate::startup::set_enabled(!status.registered) {
                            Ok(s) => s,
                            Err(_) => crate::startup::Status {
                                error: true,
                                ..crate::startup::query()
                            },
                        };
                    }
                }
            }
            Action::Toggle(setting) => {
                let c = &mut self.state.config;
                match setting {
                    Setting::Topmost => c.always_on_top = !c.always_on_top,
                    Setting::Clock24 => c.clock_24h = !c.clock_24h,
                    Setting::Date => c.show_date = !c.show_date,
                    Setting::Visualizer => c.visualizer = !c.visualizer,
                    Setting::Bluetooth => c.bluetooth = !c.bluetooth,
                    Setting::Alarm => c.alarm_sound = !c.alarm_sound,
                    Setting::Quiet => c.water.quiet_enabled = !c.water.quiet_enabled,
                    Setting::OnlineLyrics => c.online_lyrics = !c.online_lyrics,
                    Setting::Stroke => c.stroke_enabled = !c.stroke_enabled,
                    _ => {}
                }
                persist = true;
                if setting == Setting::OnlineLyrics {
                    self.refresh_lyrics(true);
                }
            }
            Action::Adjust(setting, delta) => {
                let c = &mut self.state.config;
                match setting {
                    Setting::StrokeGlow => {
                        c.stroke_glow = (c.stroke_glow as i32 + delta).clamp(0, 100) as u8
                    }
                    Setting::StrokeWidth => {
                        c.stroke_width = (c.stroke_width as i32 + delta).clamp(1, 3) as u8
                    }
                    Setting::Opacity => c.opacity = (c.opacity as i32 + delta).clamp(75, 100) as u8,
                    Setting::NoticeDuration => {
                        c.notification_seconds =
                            (c.notification_seconds as i32 + delta).clamp(3, 30) as u16
                    }
                    Setting::Sensitivity => {
                        c.sensitivity = (c.sensitivity + delta as f32 / 10.0).clamp(0.5, 8.0)
                    }
                    Setting::Smoothing => {
                        c.smoothing = (c.smoothing + delta as f32 / 100.0).clamp(0.1, 0.95)
                    }
                    Setting::WaterInterval => {
                        c.water.interval_minutes =
                            (c.water.interval_minutes as i32 + delta).clamp(1, 240) as u16;
                        self.state.water.reschedule(now, &c.water);
                    }
                    Setting::ActiveStart => {
                        c.water.active_start =
                            (c.water.active_start as i32 + delta).rem_euclid(24) as u16
                    }
                    Setting::ActiveEnd => {
                        c.water.active_end =
                            (c.water.active_end as i32 + delta).rem_euclid(24) as u16
                    }
                    Setting::QuietStart => {
                        c.water.quiet_start =
                            (c.water.quiet_start as i32 + delta).rem_euclid(24) as u16
                    }
                    Setting::QuietEnd => {
                        c.water.quiet_end = (c.water.quiet_end as i32 + delta).rem_euclid(24) as u16
                    }
                    Setting::LyricsOffset => {
                        c.lyrics_offset_ms = (c.lyrics_offset_ms + delta).clamp(-10000, 10000)
                    }
                    _ => {}
                }
                persist = true;
            }
            Action::ResetPosition => {
                self.dock_animation = None;
                if self.state.config.dock_mode == crate::storage::DockMode::Anchored {
                    self.anchor = anchored_anchor(self.anchor);
                    self.state.config.anchored_position = Some([self.anchor.0, self.anchor.1]);
                } else {
                    self.anchor = default_anchor(self.scale);
                    self.state.config.position = None;
                }
                persist = true;
            }
            Action::Hide => {
                self.peek_through = false;
                self.state.visible = !self.state.visible;
                self.sync_visibility();
            }
            Action::Quit => {
                unsafe {
                    let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
        if persist {
            self.state.save();
            self.configure();
            if matches!(action, Action::Toggle(Setting::Topmost))
                && unsafe {
                    WINDOW_EX_STYLE(GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32)
                        .contains(WS_EX_TOPMOST)
                } != self.state.config.always_on_top
                && let Err(e) = self.recreate_window()
            {
                self.state.status = "Could not change window order".into();
                self.state
                    .store
                    .log(&format!("Window replacement failed: {}", e.code()));
            }
        }
        if !self.self_test && !self.transition_profile {
            self.browser_space_tick();
        }
        self.layout(true);
        self.schedule();
        if self.state.tab == Tab::Notes
            && matches!(action, Action::Select(Tab::Notes) | Action::NoteEdit)
        {
            self.note_refresh(self.state.note.editor.revision);
        } else {
            self.paint();
        }
    }
    fn configure(&self) {
        if let Some(audio) = self.audio.as_ref() {
            audio.configure(
                self.state.config.visualizer,
                self.state.config.sensitivity,
                self.state.config.smoothing,
            );
        }
        let _ = self.bluetooth.tx.send(self.state.config.bluetooth);
        self.apply_z_order();
    }
    /// Some Windows environments acknowledge SetWindowPos without applying its
    /// topmost change. Recreate only the native surface with the requested style;
    /// domain state, subscriptions and capture workers continue uninterrupted.
    fn recreate_window(&mut self) -> Result<()> {
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            let inspect = std::env::args().any(|a| a == "--inspect-ui");
            let ex = WS_EX_LAYERED
                | WS_EX_NOACTIVATE
                | if inspect {
                    WS_EX_APPWINDOW
                } else {
                    WS_EX_TOOLWINDOW
                }
                | if self.state.config.always_on_top {
                    WS_EX_TOPMOST
                } else {
                    WINDOW_EX_STYLE(0)
                };
            let old = self.hwnd;
            let pointer = GetWindowLongPtrW(old, GWLP_USERDATA);
            let width = ((self.size.0 + 2 * ui::HALO) as f32 * self.scale) as i32;
            let height =
                ((self.size.1 + ui::top_inset(&self.state) + ui::HALO) as f32 * self.scale) as i32;
            let new = CreateWindowExW(
                ex,
                w!("DynamicIslandForWindows"),
                w!("Dynamic Island For Windows"),
                WS_POPUP,
                self.anchor.0 - width / 2,
                self.anchor.1 - (ui::top_inset(&self.state) as f32 * self.scale).round() as i32,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            )?;
            let tray = match Tray::new(new) {
                Ok(t) => t,
                Err(e) => {
                    let _ = DestroyWindow(new);
                    return Err(e);
                }
            };
            SetWindowLongPtrW(new, GWLP_USERDATA, pointer);
            self.tray.remove();
            self.tray = tray;
            self.hwnd = new;
            self.bus.hwnd.store(new.0 as isize, Ordering::Release);
            self.frame_clock.acknowledge();
            SetWindowLongPtrW(old, GWLP_USERDATA, 0);
            let _ = DestroyWindow(old);
            if self.surface_visible() {
                let _ = ShowWindow(new, SW_SHOWNOACTIVATE);
                SetWindowPos(
                    new,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
            }
            self.animation = None;
            self.dock_animation = None;
            self.wake_due = None;
            self.pulse_timer = false;
            self.pointer_timer = false;
            self.fullscreen_timer = false;
            self.browser_fade_timer = false;
            self.browser_generation = self.browser_generation.wrapping_add(1);
            self.browser_query = None;
            self.browser_worker.watch(None);
            self.size = self.target;
            self.canvas = None;
            self.sync_visibility();
            self.schedule();
            if self.self_test {
                SetTimer(Some(new), 3, 1000, None);
            }
            if self.run_for_ms.is_some() {
                SetTimer(Some(new), 4, 1000, None);
            }
            self.bus.wake();
            Ok(())
        }
    }
    fn apply_z_order(&self) {
        unsafe {
            let topmost = WINDOW_EX_STYLE(GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32)
                .contains(WS_EX_TOPMOST);
            if topmost == self.state.config.always_on_top {
                return;
            }
            if let Err(e) = SetWindowPos(
                self.hwnd,
                Some(if self.state.config.always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                }),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            ) {
                self.state
                    .store
                    .log(&format!("Could not set window order: {}", e.code()));
            }
            if self.self_test && self.z_order_attempts.get() < 3 {
                self.z_order_attempts.set(self.z_order_attempts.get() + 1);
                let mut info = WINDOWINFO {
                    cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
                    ..Default::default()
                };
                let _ = GetWindowInfo(self.hwnd, &mut info);
                self.state.store.log(&format!(
                    "Window order probe: requested={}, extended_style={:#x}, window_info={:#x}",
                    self.state.config.always_on_top,
                    GetWindowLongW(self.hwnd, GWL_EXSTYLE),
                    info.dwExStyle.0
                ));
            }
        }
    }
    fn frame_tick(&mut self) {
        self.frame_batch = true;
        if self.browser_fade_timer {
            self.browser_fade_tick(now_ms());
        } else {
            if self.pointer_timer {
                self.pointer_tick();
            }
            if let Some(dock) = self.dock_animation {
                let now = now_ms();
                let pose = dock.pose(now);
                self.size = (pose.size[0].round() as i32, pose.size[1].round() as i32);
                if dock.done(now) {
                    self.size = self.target;
                    self.dock_animation = None;
                }
                self.paint_now();
            } else if self.animation.is_some() {
                self.animate();
            } else if self.pulse_timer {
                self.pulse_tick();
            }
        }
        self.frame_batch = false;
        if self.frame_requested {
            self.pulse_only = !self.frame_full;
            self.paint_immediate();
            self.pulse_only = false;
        }
        self.frame_requested = false;
        self.frame_full = false;
        self.frame_clock.set_active(
            self.pointer_timer
                || self.pulse_timer
                || self.animation.is_some()
                || self.dock_animation.is_some()
                || self.browser_fade_timer,
        );
    }
    fn animate(&mut self) {
        if let Some(spring) = self.animation {
            let (size, done) = spring.size(now_ms());
            self.size = (
                size.0.clamp(ui::COMPACT_WIDTH - 24, ui::MEDIA_WIDTH + 24),
                size.1.clamp(ui::COMPACT_HEIGHT - 24, ui::HEIGHT + 24),
            );
            if done {
                self.size = self.target;
                self.animation = None;
            }
            self.paint_now();
        }
    }
    fn profile_transition(&mut self) {
        if std::env::args().any(|a| a == "--profile-docking") {
            if self.profile_stage >= 8 {
                unsafe {
                    let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else {
                let mode = if self.profile_stage.is_multiple_of(2) {
                    crate::storage::DockMode::Anchored
                } else {
                    crate::storage::DockMode::Floating
                };
                self.profile_stage += 1;
                self.action(Action::Dock(mode));
            }
            return;
        }
        let stages = [
            (Tab::Media, true, false),
            (Tab::Media, true, true),
            (Tab::Weather, true, false),
            (Tab::Timer, true, false),
            (Tab::Water, true, false),
            (Tab::Settings, true, false),
            (Tab::Media, false, false),
        ];
        if self.profile_stage >= stages.len() * 2 {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        let (tab, expanded, lyrics) = stages[self.profile_stage % stages.len()];
        self.profile_stage += 1;
        self.state.tab = tab;
        self.state.expanded = expanded;
        self.state.show_lyrics = lyrics;
        self.layout(true);
        self.schedule();
        self.paint();
    }
    fn menu(&mut self) {
        let action = match self.tray.menu(self.state.visible) {
            1 => Some(Action::Expand),
            2 => Some(Action::Select(Tab::Settings)),
            3 => Some(Action::Hide),
            4 => Some(Action::ResetPosition),
            6 => Some(Action::WeatherAttribution),
            7 => Some(Action::IconAttribution),
            9 => Some(Action::Quit),
            _ => None,
        };
        if let Some(action) = action {
            if !self.state.visible && action != Action::Hide && action != Action::Quit {
                self.action(Action::Hide);
            }
            self.action(action);
        }
    }
    fn stop_test_pulse(&mut self) {
        if let Some((stop, join)) = self.test_pulse_source.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = join.join();
        }
    }
    fn shutdown(&mut self) {
        self.queue_note(true);
        if let Some(writer) = &mut self.note_writer {
            writer.shutdown();
        }
        self.finish_color_drag();
        self.stop_test_pulse();
        self.trace("shutdown: begin");
        let started = Instant::now();
        self.browser_worker.shutdown();
        self.trace("shutdown: browser geometry stopped");
        self.stop.store(true, Ordering::Relaxed);
        if let Some(audio) = self.audio.as_mut() {
            audio.shutdown();
        }
        self.trace("shutdown: audio stopped");
        self.media.shutdown();
        self.trace("shutdown: media stopped");
        self.lyrics.shutdown();
        self.weather.shutdown();
        self.trace("shutdown: lyrics and weather stopped");
        self.bluetooth.shutdown();
        self.trace("shutdown: bluetooth stopped");
        self.tray.remove();
        self.state.save();
        unsafe {
            let _ = PlaySoundW(None, None, SND_PURGE);
        }
        if self.self_test {
            let path = self.state.store.directory.join("self-test.json");
            if let Ok(bytes) = std::fs::read(&path)
                && let Ok(mut report) = serde_json::from_slice::<serde_json::Value>(&bytes)
            {
                report["worker_shutdown_ms"] = serde_json::json!(started.elapsed().as_millis());
                let _ = std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap());
            }
        }
        if self.transition_profile {
            let animated: Vec<_> = self.frame_timings.iter().filter(|f| f.animating).collect();
            let mean = |field: fn(&FrameTiming) -> f64| {
                animated.iter().map(|f| field(f)).sum::<f64>() / (animated.len().max(1) as f64)
            };
            let mut times: Vec<f64> = animated.iter().map(|f| f.total_ms).collect();
            times.sort_by(f64::total_cmp);
            let p95 = times
                .get((times.len().saturating_sub(1) * 95) / 100)
                .copied()
                .unwrap_or(0.0);
            let report = serde_json::json!({"fixture":true,"online":false,"stroke":self.state.config.stroke_enabled,"dpi_scale":self.scale,"animated_frames":animated.len(),"mean_render_ms":mean(|f|f.total_ms),"p95_render_ms":p95,"mean_surface_ms":mean(|f|f.surface_ms),"mean_mask_build_ms":mean(|f|f.mask_build_ms),"mean_mask_apply_ms":mean(|f|f.mask_apply_ms),"mean_upload_ms":mean(|f|f.upload_ms),"mean_draw_ms":mean(|f|f.draw_ms),"mean_present_ms":mean(|f|f.present_ms),"frames_over_16_7_ms":animated.iter().filter(|f|f.total_ms>16.7).count(),"frames":self.frame_timings});
            let _ = std::fs::write(
                self.state.store.directory.join("transition-profile.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            );
        }
        self.trace("shutdown: complete");
    }
    fn trace(&self, message: &str) {
        if std::env::args().any(|a| a == "--trace-shutdown") {
            self.state.store.log(message);
        }
    }
    fn test_click(&self, x: i32, y: i32) {
        unsafe {
            let (origin, top) = self.content_origin();
            let px = ((x as f32 + origin) * self.scale).round() as u32;
            let py = ((y as f32 + top) * self.scale).round() as u32;
            let _ = PostMessageW(
                Some(self.hwnd),
                WM_LBUTTONUP,
                WPARAM(0),
                LPARAM((px | (py << 16)) as isize),
            );
        }
    }
    fn test_type(&self, text: &str) {
        for c in text.encode_utf16() {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CHAR, WPARAM(c as usize), LPARAM(0));
            }
        }
    }
    fn surface_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(c) = &self.canvas {
            for y in 0..c.height {
                unsafe {
                    std::slice::from_raw_parts(
                        c.bits.add((y * c.stride * 4) as usize),
                        (c.width * 4) as usize,
                    )
                    .hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }
    fn frame_hash(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let c = self.canvas.as_ref()?;
        if ((254.0 + ui::top_inset(&self.state) as f32) * c.scale) as i32 > c.height
            || ((135.0 + ui::surface_inset(&self.state) as f32) * c.scale) as i32 > c.width
        {
            return None;
        }
        {
            unsafe {
                let pixels = std::slice::from_raw_parts(c.bits, (c.stride * c.height * 4) as usize);
                for y in ((112.0 + ui::top_inset(&self.state) as f32) * c.scale) as usize
                    ..((254.0 + ui::top_inset(&self.state) as f32) * c.scale) as usize
                {
                    let start = (y * c.stride as usize
                        + ((43.0 + ui::surface_inset(&self.state) as f32) * c.scale) as usize)
                        * 4;
                    let end = (y * c.stride as usize
                        + ((135.0 + ui::surface_inset(&self.state) as f32) * c.scale) as usize)
                        * 4;
                    pixels[start..end].hash(&mut hasher);
                }
            }
        }
        Some(hasher.finish())
    }
    fn test_tick(&mut self) {
        if !self.self_test {
            return;
        }
        let elapsed = now_ms() - self.test_start;
        let stage = (elapsed / 1000) as u32;
        if stage <= self.test_stage {
            return;
        }
        // A delayed/coalesced WM_TIMER must not skip an interaction step.
        let stage = self.test_stage + 1;
        self.test_stage = stage;
        self.trace(&format!(
            "self-test stage {stage}: visible={} peek={} fullscreen={} browser={} expanded={}",
            self.state.visible,
            self.peek_through,
            self.fullscreen_hidden,
            self.browser_hidden,
            self.state.expanded,
        ));
        let folder = self.state.store.directory.join("renders");
        let _ = std::fs::create_dir_all(&folder);
        match stage {
            1 => {
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("compact.bmp"));
                }
                self.test_click(126, 29);
            }
            2 => {
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("media.bmp"));
                }
                self.test_clicks_ok &= self.state.expanded
                    && (!self.state.media.available || self.state.tab == Tab::Media);
                self.test_click(
                    ui::dock_rect(&self.state, 2)[0] + 20,
                    ui::dock_rect(&self.state, 2)[1] + 20,
                );
            }
            3 => {
                self.test_clicks_ok &= self.state.tab == Tab::Timer;
                self.state.config.water.enabled = true;
                self.state.config.water.active_start = 0;
                self.state.config.water.active_end = 0;
                self.state.config.water.quiet_enabled = false;
                self.state.water.next_due = now_ms() + 500;
                self.state.timer.set_seconds(2);
                self.action(Action::TimerToggle);
                self.test_click(420, 30);
            }
            6 => {
                self.state.notices.clear();
                self.state.expanded = true;
                self.state.tab = Tab::Timer;
                self.layout(false);
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("timer.bmp"));
                }
            }
            7 => {
                self.state.tab = Tab::Water;
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("water.bmp"));
                }
                self.test_click(120, 309);
            }
            8 => {
                self.state.tab = Tab::Settings;
                self.state.settings_page = 0;
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("settings.bmp"));
                }
            }
            9 => {
                self.state.settings_page = 1;
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("features.bmp"));
                }
                if self.state.config.visualizer {
                    self.action(Action::Toggle(Setting::Visualizer));
                }
            }
            10 => {
                self.test_audio_disabled = self.state.audio_status == "Visualizer off";
                if !self.state.config.visualizer {
                    self.action(Action::Toggle(Setting::Visualizer));
                }
            }
            11 => {
                self.action(Action::Toggle(Setting::Topmost));
                self.test_topmost_off = unsafe {
                    !WINDOW_EX_STYLE(GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32)
                        .contains(WS_EX_TOPMOST)
                };
            }
            12 => {
                self.action(Action::Toggle(Setting::Topmost));
            }
            13 => {
                self.test_floating_anchor = self.anchor;
                self.action(Action::Dock(crate::storage::DockMode::Anchored));
            }
            14 => {
                self.test_anchored = self.anchor.1 == anchored_anchor(self.anchor).1
                    && self.state.config.dock_mode == crate::storage::DockMode::Anchored
                    && self.size.0 == ui::WIDTH + 24;
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("anchored-settings.bmp"));
                }
                self.action(Action::Dock(crate::storage::DockMode::Floating));
            }
            15 => {
                self.test_floating_restored = self.anchor == self.test_floating_anchor
                    && self.state.config.dock_mode == crate::storage::DockMode::Floating;
                unsafe {
                    let _ = PostMessageW(Some(self.hwnd), WM_EXITSIZEMOVE, WPARAM(0), LPARAM(0));
                }
            }
            16 => {
                self.design_checks.insert(
                    "drag_end_preserves_body_anchor",
                    self.anchor == self.test_floating_anchor,
                );
                self.state.show_lyrics = false;
                // Exercise the real lyrics button without depending on a
                // song being open in another application during this test.
                self.test_lyrics_media = Some(std::mem::replace(
                    &mut self.state.media,
                    crate::media::MediaSnapshot {
                        available: true,
                        title: "Local interaction fixture".into(),
                        artist: "Diagnostic".into(),
                        source: "Spotify".into(),
                        ..Default::default()
                    },
                ));
                self.action(Action::Select(Tab::Media));
                self.layout(false);
                self.paint_now();
                if self.state.media.available {
                    self.test_click(138, 218);
                }
            }
            17 => {
                self.design_checks.insert(
                    "lyrics_open_click",
                    self.state.show_lyrics && self.size.0 == ui::MEDIA_WIDTH,
                );
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("lyrics-open.bmp"));
                }
                if self.state.media.available {
                    self.test_click(78, 218);
                }
            }
            18 => {
                self.design_checks.insert(
                    "lyrics_close_click",
                    !self.state.show_lyrics && self.size.0 == ui::WIDTH,
                );
                let r = ui::dock_rect(&self.state, 1);
                self.test_click(r[0] + 20, r[1] + 20);
            }
            19 => {
                if let Some(media) = self.test_lyrics_media.take() {
                    self.state.media = media;
                }
                self.design_checks
                    .insert("weather_dock_click", self.state.tab == Tab::Weather);
                self.action(Action::WeatherSearch);
                self.test_type("Jakarta");
                self.test_type("\r");
            }
            20 => {
                self.design_checks.insert(
                    "city_keyboard_submit",
                    self.state.edit_text == "Jakarta"
                        && self.state.editing.is_none()
                        && self.state.weather.choosing,
                );
                self.action(Action::WeatherCancel);
                self.action(Action::Select(Tab::Settings));
                self.action(Action::SettingsPage(3));
                self.action(Action::CustomStroke);
                self.test_type("\x08\x08\x08\x08\x08\x08\x08#89ABCD\r");
            }
            21 => {
                self.design_checks.insert(
                    "custom_stroke_keyboard",
                    self.state.config.stroke_color == 0x89abcd && self.state.editing.is_none(),
                );
                self.action(Action::Toggle(Setting::Stroke));
                self.design_checks
                    .insert("stroke_toggle", !self.state.config.stroke_enabled);
                self.action(Action::Toggle(Setting::Stroke));
                self.action(Action::Adjust(Setting::StrokeWidth, 1));
                self.design_checks
                    .insert("stroke_width", self.state.config.stroke_width == 2);
                self.action(Action::Adjust(Setting::StrokeGlow, -20));
                self.design_checks
                    .insert("glow_setting", self.state.config.stroke_glow == 50);
                if let Some(c) = &self.canvas {
                    let px = c.width / 2;
                    let py = ((ui::top_inset(&self.state) - 4) as f32 * c.scale) as i32;
                    let alpha = unsafe { *c.bits.add(((py * c.stride + px) * 4 + 3) as usize) };
                    self.design_checks
                        .insert("external_glow_rendered", alpha > 0);
                    self.design_checks
                        .insert("glow_outside_hit_area", !c.hit_test(px, py));
                }
                let (saved, _) = self.state.store.load();
                self.design_checks.insert(
                    "style_persisted",
                    saved.stroke_color == 0x89abcd
                        && saved.stroke_width == 2
                        && saved.stroke_glow == 50,
                );
                self.bus.send(Event::Bluetooth {
                    name: "Test keyboard · fixture".into(),
                    connected: true,
                    kind: crate::bluetooth::DeviceKind::Keyboard,
                });
            }
            22 => {
                self.design_checks.insert(
                    "bluetooth_icon_dispatch",
                    self.state
                        .notices
                        .iter()
                        .any(|n| n.device == Some(crate::bluetooth::DeviceKind::Keyboard)),
                );
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("bluetooth-keyboard.bmp"));
                }
                self.state.notices.clear();
                self.action(Action::Select(Tab::Water));
                self.layout(false);
                self.paint_now();
                self.water_frame = self.frame_hash();
            }
            23 => {
                self.paint_now();
                self.design_checks.insert(
                    "tumbler_static",
                    self.water_frame.is_some() && self.water_frame == self.frame_hash(),
                );
                self.test_click(120, 410);
            }
            24 => {
                self.design_checks
                    .insert("reminder_switch_click", !self.state.config.water.enabled);
                self.state.notices.clear();
                self.state.config.water.enabled = false;
                self.state.config.stroke_enabled = true;
                self.state.config.stroke_width = 3;
                self.state.config.stroke_glow = 100;
                self.state.config.visualizer = true;
                if let Some(audio) = &self.audio {
                    audio.configure(
                        false,
                        self.state.config.sensitivity,
                        self.state.config.smoothing,
                    );
                }
                let bus = self.bus.clone();
                let global_stop = self.stop.clone();
                let stop = Arc::new(AtomicBool::new(false));
                let producer_stop = stop.clone();
                let join = std::thread::spawn(move || {
                    while !producer_stop.load(Ordering::Relaxed)
                        && !global_stop.load(Ordering::Relaxed)
                    {
                        bus.levels([0.65; BARS]);
                        std::thread::sleep(std::time::Duration::from_millis(
                            crate::media::pulse::FRAME_MS,
                        ));
                    }
                });
                self.test_pulse_source = Some((stop, join));
                self.last_frame = None;
                self.action(Action::Select(Tab::Media));
            }
            25 => self.action(Action::Select(Tab::Weather)),
            26 => self.action(Action::Select(Tab::Timer)),
            27 => self.action(Action::Select(Tab::Water)),
            28 => {
                self.action(Action::Select(Tab::Settings));
                self.action(Action::SettingsPage(1));
            }
            29 => {
                self.stop_test_pulse();
                self.state.config.visualizer = false;
                self.state.media.playing = false;
                self.bus.levels([0.0; BARS]);
                self.state.config.water.enabled = true;
                self.state.water.pending = false;
                self.state.water.next_due = now_ms() + 9000;
                self.action(Action::Select(Tab::Water));
            }
            33 => {
                self.design_checks.insert(
                    "pulse_live_in_all_five_tabs_with_glow",
                    self.test_pulse_frames[..5]
                        .iter()
                        .all(|frames| *frames > 10),
                );
                self.design_checks.insert(
                    "water_seconds_advance_without_audio_or_media_updates",
                    self.test_water_seconds.len() >= 4,
                );
                self.action(Action::Select(Tab::Weather));
                self.action(Action::WeatherRefresh);
            }
            34 => {
                self.design_checks.insert(
                    "weather_refresh_finishes_offline",
                    !self.state.weather.refreshing
                        && !self.state.weather.error.is_empty()
                        && self.state.weather.refresh_after_ms > now_ms(),
                );
                self.state.config.visualizer = true;
                if let Some(audio) = &self.audio {
                    audio.configure(
                        true,
                        self.state.config.sensitivity,
                        self.state.config.smoothing,
                    );
                }
            }
            36 => {
                self.state.notices.clear();
                self.state.config.water.enabled = false;
                self.state.expanded = false;
                self.state.interaction = Default::default();
                self.layout(false);
                self.paint_now();
                let b = self.body_rect();
                let p = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                let now = now_ms();
                self.pointer_at(now, p, true);
                self.pointer_at(now + 400, p, true);
                self.design_checks.insert(
                    "hover_opens_without_click",
                    self.state.expanded && self.state.interaction.preview,
                );
            }
            37 => {
                let now = now_ms();
                self.pointer_at(now, [-20000.0, -20000.0], false);
                self.pointer_at(now + 300, [-20000.0, -20000.0], false);
                self.design_checks
                    .insert("hover_leave_collapses_preview", !self.state.expanded);
            }
            38 => {
                let b = self.body_rect();
                let p = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                let now = now_ms();
                self.pointer_at(now, p, true);
                self.pointer_at(now + 400, p, true);
            }
            39 => {
                let r = ui::dock_rect(&self.state, 2);
                self.test_click(r[0] + 20, r[1] + 20);
            }
            40 => {
                let now = now_ms();
                self.pointer_at(now, [-20000.0, -20000.0], false);
                self.pointer_at(now + 500, [-20000.0, -20000.0], false);
                self.design_checks.insert(
                    "feature_click_pins_preview",
                    self.state.expanded
                        && !self.state.interaction.preview
                        && self.state.tab == Tab::Timer,
                );
                self.action(Action::Collapse);
                let b = self.body_rect();
                let p = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                self.pointer_at(now + 600, p, true);
                self.pointer_at(now + 1600, p, true);
                self.design_checks
                    .insert("minimize_blocks_reopen_until_exit", !self.state.expanded);
            }
            41 => {
                self.action(Action::Dock(crate::storage::DockMode::Anchored));
                self.action(Action::Select(Tab::Settings));
                self.state.settings_page = 0;
                self.layout(false);
                self.paint_now();
                self.begin_move();
                unsafe {
                    let mut r = RECT::default();
                    if GetWindowRect(self.hwnd, &mut r).is_ok() {
                        let _ = SetWindowPos(
                            self.hwnd,
                            None,
                            r.left + 120,
                            r.top,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                self.paint_now();
                self.end_move();
                self.test_moved_anchor = Some(self.anchor);
            }
            42 => {
                let saved = self.state.store.load().0;
                self.design_checks.insert(
                    "manual_drag_position_saved",
                    self.test_moved_anchor
                        .is_some_and(|a| saved.anchored_position == Some([a.0, a.1])),
                );
            }
            44 => {
                let mut rect = RECT::default();
                let positioned = unsafe { GetWindowRect(self.hwnd, &mut rect).is_ok() };
                self.design_checks.insert(
                    "manual_drag_survives_audio_redraws",
                    positioned
                        && self.test_moved_anchor == Some(self.anchor)
                        && (rect.left + rect.right) / 2 == self.fitted_anchor(self.size).0,
                );
                if let Some(hit) = self.hits.iter().find(|h| h.action == Action::ResetPosition) {
                    let r = hit.rect;
                    self.test_click(r[0] + r[2] / 2, r[1] + r[3] / 2);
                }
            }
            45 => {
                self.design_checks.insert(
                    "reset_position_requires_explicit_click",
                    self.anchor == anchored_anchor(self.anchor)
                        && Some(self.anchor) != self.test_moved_anchor,
                );
            }
            46 => {
                self.state.notices.clear();
                self.action(Action::Collapse);
                self.state.interaction = Default::default();
                self.layout(false);
                self.paint_now();
                self.state.media.playing = false;
                self.state.timer.reset();
                let b = self.body_rect();
                let outside = [b[0] + b[2] / 2.0, b[1] + b[3] + 80.0 * self.scale];
                let approach_time = now_ms();
                self.pointer_at(approach_time, outside, false);
                self.pointer_at(approach_time + 100, outside, false);
                self.design_checks.insert(
                    "cry_starts_outside_capsule_without_opening",
                    self.state.face.pose(approach_time + 100).crying > 0.9 && !self.state.expanded,
                );
                let side = [b[0] + 30.0 * self.scale, b[1] + 25.0 * self.scale];
                let now = now_ms();
                let pass =
                    crate::interaction::pass_through(b, side, self.scale, false, false, false);
                self.set_pass_through(pass);
                self.pointer_at(now, side, true);
                self.pointer_at(now + 500, side, true);
                let style = unsafe { GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32 };
                self.design_checks.insert(
                    "clock_area_opens_preview",
                    self.state.expanded
                        && self.state.interaction.preview
                        && style & WS_EX_TRANSPARENT.0 == 0,
                );
                self.set_pass_through(false);
                self.action(Action::Collapse);
                self.state.interaction = Default::default();
                self.layout(false);
                let b = self.body_rect();
                let date = [b[0] + b[2] - 30.0 * self.scale, b[1] + 25.0 * self.scale];
                self.pointer_at(now + 600, date, true);
                self.pointer_at(now + 800, date, true);
                self.design_checks.insert(
                    "date_area_opens_preview",
                    self.state.expanded && self.state.interaction.preview,
                );
            }
            47 => {
                self.action(Action::Select(Tab::Settings));
                self.set_alt_bypass(true);
                let hidden = unsafe { !IsWindowVisible(self.hwnd).as_bool() };
                let transparent = unsafe {
                    GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0 != 0
                };
                self.set_alt_bypass(false);
                self.design_checks.insert(
                    "alt_temporarily_hides_and_restores_pinned_panel",
                    hidden
                        && transparent
                        && self.state.expanded
                        && !self.peek_through
                        && unsafe { IsWindowVisible(self.hwnd).as_bool() },
                );
                self.action(Action::Collapse);
                self.state.interaction = Default::default();
                self.set_alt_bypass(true);
                self.set_alt_bypass(false);
                let b = self.body_rect();
                let point = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                let now = now_ms();
                self.pointer_at(now, point, true);
                self.pointer_at(now + 500, point, true);
                self.design_checks.insert(
                    "alt_release_does_not_reopen_until_cursor_leaves",
                    !self.state.expanded,
                );
            }
            48 => {
                match crate::fullscreen::verify_geometry(self.anchor) {
                    Ok(checks) => self.design_checks.extend(checks),
                    Err(_) => {
                        self.design_checks
                            .insert("fullscreen_native_client_geometry", false);
                    }
                }
                self.action(Action::Select(Tab::Settings));
                let anchor = self.anchor;
                let config = serde_json::to_vec(&self.state.config).unwrap();
                self.set_fullscreen_hidden(true);
                let hidden = unsafe { !IsWindowVisible(self.hwnd).as_bool() };
                self.set_alt_bypass(true);
                self.set_alt_bypass(false);
                self.design_checks.insert(
                    "fullscreen_remains_hidden_after_alt_release",
                    hidden && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.set_fullscreen_hidden(false);
                self.design_checks.insert(
                    "fullscreen_exit_restores_pinned_panel_and_position",
                    self.state.expanded
                        && !self.state.interaction.preview
                        && self.anchor == anchor
                        && unsafe { IsWindowVisible(self.hwnd).as_bool() },
                );
                self.design_checks.insert(
                    "fullscreen_does_not_change_preferences",
                    serde_json::to_vec(&self.state.config).unwrap() == config,
                );
            }
            49 => {
                self.set_fullscreen_hidden(true);
                self.action(Action::Hide);
                self.set_fullscreen_hidden(false);
                self.design_checks.insert(
                    "fullscreen_exit_respects_manual_hide",
                    !self.state.visible && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.action(Action::Hide);
            }
            50 => {
                self.action(Action::Select(Tab::Settings));
                self.action(Action::SettingsPage(3));
                self.animation = None;
                self.paint_now();
                let before =
                    std::fs::read(self.state.store.directory.join("settings.json")).unwrap();
                self.begin_color_drag(422.0, 383.0);
                self.drag_color(248.0, 383.0);
                self.drag_color(422.0, 383.0);
                self.design_checks.insert(
                    "color_preview_defers_settings_write",
                    std::fs::read(self.state.store.directory.join("settings.json")).unwrap()
                        == before,
                );
                self.finish_color_drag();
                for (message, x, y) in [
                    (WM_LBUTTONDOWN, 216, 294),
                    (WM_MOUSEMOVE, 124, 150),
                    (WM_LBUTTONUP, 124, 150),
                ] {
                    let px =
                        ((x + ui::surface_inset(&self.state)) as f32 * self.scale).round() as u32;
                    let py = ((y + ui::top_inset(&self.state)) as f32 * self.scale).round() as u32;
                    unsafe {
                        let _ = PostMessageW(
                            Some(self.hwnd),
                            message,
                            WPARAM(0),
                            LPARAM((px | (py << 16)) as isize),
                        );
                    }
                }
            }
            51 => {
                self.paint_now();
                let (saved, _) = self.state.store.load();
                self.design_checks.insert(
                    "wheel_drag_native_messages_and_clamped_edge",
                    self.state.config.stroke_color == 0x8000ff && self.color_drag.is_none(),
                );
                self.design_checks.insert(
                    "wheel_color_persists_on_release",
                    saved.stroke_color == 0x8000ff,
                );
                if let Some(c) = self.canvas.as_ref() {
                    let _ = c.save_bmp(&folder.join("color-wheel.bmp"));
                }
            }
            52 => {
                self.action(Action::Collapse);
                let anchor = self.anchor;
                let config = serde_json::to_vec(&self.state.config).unwrap();
                self.set_browser_hidden(true);
                self.browser_fade_tick(now_ms() + 500);
                self.design_checks
                    .insert("browser_collision_hides_native_window", unsafe {
                        !IsWindowVisible(self.hwnd).as_bool()
                    });
                self.set_alt_bypass(true);
                self.set_alt_bypass(false);
                self.design_checks
                    .insert("browser_collision_survives_alt_release", unsafe {
                        !IsWindowVisible(self.hwnd).as_bool()
                    });
                self.set_fullscreen_hidden(true);
                self.set_fullscreen_hidden(false);
                self.design_checks
                    .insert("browser_collision_survives_fullscreen_exit", unsafe {
                        !IsWindowVisible(self.hwnd).as_bool()
                    });
                self.set_browser_hidden(false);
                self.browser_fade_tick(now_ms() + 500);
                self.design_checks.insert(
                    "browser_clear_space_restores_window_and_position",
                    unsafe { IsWindowVisible(self.hwnd).as_bool() } && self.anchor == anchor,
                );
                self.design_checks.insert(
                    "browser_avoidance_preserves_preferences",
                    serde_json::to_vec(&self.state.config).unwrap() == config,
                );
                self.set_browser_hidden(true);
                self.browser_fade_tick(now_ms() + 500);
                self.action(Action::Hide);
                self.set_browser_hidden(false);
                self.browser_fade_tick(now_ms() + 500);
                self.design_checks.insert(
                    "browser_clear_space_respects_manual_hide",
                    !self.state.visible && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.action(Action::Hide);
                self.browser_generation = 77;
                self.bus.send(Event::BrowserSpace(76, true));
            }
            53 => {
                self.design_checks.insert(
                    "stale_browser_geometry_cannot_hide_window",
                    !self.browser_hidden && unsafe { IsWindowVisible(self.hwnd).as_bool() },
                );
            }
            54 => {
                self.action(Action::Collapse);
                self.animation = None;
                self.state.config.stroke_enabled = true;
                self.state.config.stroke_glow = 100;
                self.state.config.opacity = 100;
                self.paint_now();
                self.browser_fade_hash = Some(self.surface_hash());
                self.browser_fade_samples.clear();
                self.set_browser_hidden(true);
                self.design_checks.insert(
                    "browser_fade_keeps_window_until_transparent",
                    unsafe { IsWindowVisible(self.hwnd).as_bool() } && self.browser_fade_timer,
                );
                self.design_checks.insert(
                    "browser_fade_immediately_releases_mouse_input",
                    unsafe {
                        GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0 != 0
                    },
                );
            }
            55 => {
                let down: Vec<_> = self
                    .browser_fade_samples
                    .iter()
                    .filter(|s| s.0)
                    .map(|s| s.2)
                    .collect();
                self.design_checks.insert(
                    "browser_fade_out_has_intermediate_monotonic_frames",
                    down.iter().filter(|a| **a > 0 && **a < 255).count() >= 3
                        && down.windows(2).all(|p| p[0] >= p[1])
                        && down.last() == Some(&0)
                        && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.design_checks.insert(
                    "browser_fade_reuses_unchanged_masked_pixels",
                    self.browser_fade_hash == Some(self.surface_hash()),
                );
                self.set_browser_hidden(false);
                self.design_checks.insert(
                    "browser_fade_in_starts_transparent",
                    unsafe { IsWindowVisible(self.hwnd).as_bool() }
                        && self.browser_fade.alpha(now_ms(), 100) < 10,
                );
            }
            56 => {
                let up: Vec<_> = self
                    .browser_fade_samples
                    .iter()
                    .filter(|s| !s.0)
                    .map(|s| s.2)
                    .collect();
                self.design_checks.insert(
                    "browser_fade_in_has_intermediate_monotonic_frames",
                    up.iter().filter(|a| **a > 0 && **a < 255).count() >= 3
                        && up.windows(2).all(|p| p[0] <= p[1])
                        && up.last() == Some(&255)
                        && unsafe { IsWindowVisible(self.hwnd).as_bool() },
                );
                self.set_browser_hidden(true);
                self.set_alt_bypass(true);
                self.design_checks.insert(
                    "alt_cancels_in_progress_browser_fade",
                    !self.browser_fade_timer && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.set_alt_bypass(false);
                self.set_browser_hidden(false);
                self.set_fullscreen_hidden(true);
                self.design_checks.insert(
                    "fullscreen_cancels_in_progress_browser_fade",
                    !self.browser_fade_timer && unsafe { !IsWindowVisible(self.hwnd).as_bool() },
                );
                self.set_fullscreen_hidden(false);
                let _ = std::fs::write(
                    self.state.store.directory.join("browser-fade.json"),
                    serde_json::to_vec_pretty(&self.browser_fade_samples).unwrap(),
                );
            }
            57 => {
                self.action(Action::Select(Tab::Notes));
                self.state.note.editor.select_all();
                self.test_type("Besok:\rIde kecil… 🥤");
            }
            58 => {
                self.design_checks.insert(
                    "notes_dock_and_focus",
                    self.state.tab == Tab::Notes
                        && self.state.editing == Some(EditField::Note)
                        && self
                            .hits
                            .iter()
                            .any(|h| h.action == Action::Select(Tab::Notes)),
                );
                self.design_checks.insert(
                    "notes_multiline_utf16_input",
                    self.state.note.editor.text == "Besok:\nIde kecil… 🥤",
                );
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("notes.bmp"));
                }
                self.test_type("\x08");
            }
            59 => {
                self.design_checks.insert(
                    "notes_unicode_backspace",
                    self.state.note.editor.text == "Besok:\nIde kecil… ",
                );
                self.design_checks.insert(
                    "notes_debounced_save",
                    self.note_writer.is_some()
                        && self.state.note.saved == self.state.note.editor.revision
                        && std::fs::read_to_string(
                            self.state.store.directory.join(crate::notes::FILE),
                        )
                        .ok()
                        .as_deref()
                            == Some(self.state.note.editor.text.as_str()),
                );
                let previous = self.state.note.editor.revision;
                self.state.note.editor.undo(false);
                self.note_refresh(previous);
            }
            60 => {
                self.design_checks.insert(
                    "notes_undo_restores_unicode",
                    self.state.note.editor.text == "Besok:\nIde kecil… 🥤",
                );
                let previous = self.state.note.editor.revision;
                self.state.note.editor.insert("\nFinal line");
                self.state.note.changed(previous, now_ms());
                self.action(Action::Select(Tab::Settings));
                self.design_checks.insert(
                    "notes_switch_flushes_without_wait",
                    self.state.note.queued == self.state.note.editor.revision
                        && self.state.editing.is_none(),
                );
            }
            61 => {
                let restored = crate::notes::Note::load(&self.state.store.directory);
                self.design_checks.insert(
                    "notes_persist_after_reopen",
                    restored.readable
                        && restored.editor.text == "Besok:\nIde kecil… 🥤\nFinal line",
                );
                self.action(Action::Select(Tab::Notes));
                let previous = self.state.note.editor.revision;
                self.state.note.editor.select_all();
                self.state.note.editor.insert(
                    &(1..=80)
                        .map(|i| format!("Line {i} — original test text\n"))
                        .collect::<String>(),
                );
                self.note_refresh(previous);
            }
            62 => {
                self.design_checks.insert(
                    "notes_caret_scrolls_long_document",
                    self.state.note.editor.scroll > 0.,
                );
                let last = self.state.note.editor.cursor;
                self.note_point(60., 150., false);
                self.design_checks.insert(
                    "notes_mouse_positions_caret",
                    self.state.note.editor.cursor < last
                        && self
                            .state
                            .note
                            .editor
                            .text
                            .is_char_boundary(self.state.note.editor.cursor),
                );
                self.note_key(VK_HOME.0);
                self.note_char('!' as u32);
            }
            63 => {
                self.action(Action::Collapse);
                self.design_checks.insert(
                    "notes_minimize_ends_edit",
                    !self.state.expanded
                        && self.state.editing.is_none()
                        && self.state.note.queued == self.state.note.editor.revision,
                );
            }
            64 => {
                let restored = crate::notes::Note::load(&self.state.store.directory);
                self.design_checks.insert(
                    "notes_latest_text_preserved",
                    restored.editor.text == self.state.note.editor.text,
                );
                self.action(Action::Select(Tab::Settings));
                self.action(Action::SettingsPage(0));
                self.layout(false);
                self.paint_now();
                self.test_click(405, 480);
            }
            65 => {
                self.design_checks.insert(
                    "startup_toggle_on_in_isolated_test",
                    self.state.startup.enabled(),
                );
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("startup-enabled.bmp"));
                }
                self.test_click(405, 480);
            }
            66 => {
                self.design_checks.insert(
                    "startup_toggle_off_in_isolated_test",
                    !self.state.startup.registered,
                );
            }
            67 => {
                self.action(Action::Select(Tab::Notes));
                self.state.note.editor.select_all();
                self.state.note.editor.insert("hapus satu kata 🥤");
                unsafe {
                    let mut original = [0u8; 256];
                    if GetKeyboardState(&mut original).is_ok() {
                        let mut ctrl = original;
                        ctrl[VK_CONTROL.0 as usize] = 0x80;
                        if SetKeyboardState(&ctrl).is_ok() {
                            self.note_key(VK_BACK.0);
                            self.note_key(VK_BACK.0);
                            let words = self.state.note.editor.text == "hapus satu ";
                            self.note_char(127);
                            self.design_checks.insert(
                                "notes_ctrl_backspace_deletes_words_once",
                                words && self.state.note.editor.text == "hapus satu ",
                            );
                            self.state.note.editor.select(0, false);
                            self.note_key(VK_DELETE.0);
                            self.design_checks.insert(
                                "notes_ctrl_delete_uses_next_word",
                                self.state.note.editor.text == "satu ",
                            );
                            self.note_key(0x5a);
                            self.design_checks.insert(
                                "notes_word_delete_undo_keeps_text",
                                self.state.note.editor.text == "hapus satu ",
                            );
                            let _ = SetKeyboardState(&original);
                        }
                    }
                }
            }
            68 => {
                self.action(Action::Select(Tab::Timer));
                self.state.timer.set_seconds(60);
                self.layout(false);
                self.paint_now();
                self.test_click(115, 353);
                self.test_type("2\t30\r");
            }
            69 => {
                self.design_checks.insert(
                    "timer_typed_minutes_seconds_and_tab",
                    self.state.timer.duration_ms == 150_000 && self.state.editing.is_none(),
                );
                self.paint_now();
                if let Some(c) = &self.canvas {
                    let _ = c.save_bmp(&folder.join("timer-input.bmp"));
                }
                self.action(Action::TimerToggle);
                self.action(Action::TimerEdit(crate::timer::Part::Minutes));
                self.design_checks.insert(
                    "running_timer_rejects_duration_edit",
                    self.state.timer.status == Status::Running && self.state.editing.is_none(),
                );
                self.action(Action::TimerToggle);
                self.action(Action::TimerEdit(crate::timer::Part::Seconds));
                self.timer_char('9' as u32);
                self.timer_char(27);
                self.design_checks.insert(
                    "timer_escape_preserves_duration",
                    self.state.timer.duration_ms == 150_000,
                );
                self.action(Action::TimerEdit(crate::timer::Part::Minutes));
                self.timer_char('3' as u32);
                self.end_edit();
                self.action(Action::TimerEdit(crate::timer::Part::Seconds));
                self.test_type("75");
                unsafe {
                    let _ = PostMessageW(
                        Some(self.hwnd),
                        WM_ACTIVATE,
                        WPARAM(WA_INACTIVE as usize),
                        LPARAM(0),
                    );
                }
            }
            70 => {
                self.design_checks.insert(
                    "timer_blur_commits_and_normalizes_seconds",
                    self.state.timer.duration_ms == 255_000 && self.state.editing.is_none(),
                );
                self.design_checks.insert(
                    "timer_typed_duration_persists",
                    self.state.store.load().0.timer_seconds == 255,
                );
                self.action(Action::Select(Tab::Settings));
                self.action(Action::SettingsPage(0));
                self.action(Action::Dock(crate::storage::DockMode::Floating));
            }
            71 => {
                let before = self.dock_pose(now_ms());
                let start = now_ms();
                self.action(Action::Dock(crate::storage::DockMode::Anchored));
                if let Some(dock) = self.dock_animation {
                    let first = dock.pose(start);
                    let middle = dock.pose(start + 190);
                    self.design_checks.insert(
                        "dock_starts_at_current_position",
                        (first.anchor[0] - before.anchor[0]).abs() < 1.0
                            && (first.anchor[1] - before.anchor[1]).abs() < 1.0,
                    );
                    self.design_checks.insert(
                        "dock_has_intermediate_shape_and_width",
                        middle.attached > 0.2
                            && middle.attached < 0.8
                            && middle.size[0] > ui::WIDTH as f32
                            && middle.size[0] < (ui::WIDTH + 24) as f32,
                    );
                    let before_reverse = self.dock_pose(now_ms());
                    self.action(Action::Dock(crate::storage::DockMode::Floating));
                    let reversed = self.dock_pose(now_ms());
                    self.design_checks.insert(
                        "dock_reverses_without_position_jump",
                        (before_reverse.anchor[0] - reversed.anchor[0]).abs() < 2.0
                            && (before_reverse.anchor[1] - reversed.anchor[1]).abs() < 2.0,
                    );
                }
            }
            72 => {
                self.design_checks.insert(
                    "dock_finishes_with_correct_floating_geometry",
                    self.dock_animation.is_none()
                        && self.size == (ui::WIDTH, ui::HEIGHT)
                        && self.state.config.dock_mode == crate::storage::DockMode::Floating,
                );
            }
            73.. => unsafe {
                let ex = WINDOW_EX_STYLE(GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32);
                let report = serde_json::json!({
                    "layered":ex.contains(WS_EX_LAYERED),
                    "no_activate":ex.contains(WS_EX_NOACTIVATE),
                    "topmost":ex.contains(WS_EX_TOPMOST),
                    "topmost_disabled":self.test_topmost_off,
                    "window_visible":IsWindowVisible(self.hwnd).as_bool(),
                    "tray_registered":self.tray.registered,
                    "timer_finished_while_compact":self.timer_finished_compact,
                    "alarm_count":self.alarms,
                    "native_clicks":self.test_clicks_ok,
                    "water_confirmation":self.state.water.count>0,
                    "water_notification_while_compact":self.test_water_notified,
                    "media_session":self.state.media.available,
                    "media_artwork":self.state.media.artwork.is_some(),
                    "audio_received_nonzero_samples":self.nonzero_audio,
                    "capture_disabled":self.test_audio_disabled,
                    "capture_restarted":self.state.audio_status=="System output · WASAPI",
                    "worker_bus_rebound":self.bus.hwnd.load(Ordering::Acquire)==self.hwnd.0 as isize,
                    "anchored_at_monitor_edge":self.test_anchored,
                    "floating_position_restored":self.test_floating_restored,
                    "online_lyrics_disabled_for_test":!self.state.config.online_lyrics,
                    "native_font_rendering":self.canvas.is_some(),
                    "design_checks":self.design_checks,
                    "audio_status":self.state.audio_status,
                    "bluetooth_status":self.state.bluetooth_status,
                    "renders":folder.to_string_lossy(),
                    "pulse_fixture":true,
                    "pulse_frames_by_tab": self.test_pulse_frames,
                    "water_countdown_seconds_without_audio": self.test_water_seconds,
                    "pulse_frame_timings": self.frame_timings
                });
                let _ = std::fs::write(
                    self.state.store.directory.join("self-test.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                );
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            _ => {}
        }
        self.schedule();
    }
}
fn default_anchor(scale: f32) -> (i32, i32) {
    unsafe {
        let mut r = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut r as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        ((r.left + r.right) / 2, r.top + (24.0 * scale) as i32)
    }
}
fn anchored_anchor(point: (i32, i32)) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point.0,
                y: point.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            (
                (info.rcMonitor.left + info.rcMonitor.right) / 2,
                info.rcMonitor.top,
            )
        } else {
            point
        }
    }
}
pub fn run() -> Result<()> {
    unsafe {
        if (std::env::args().any(|a| a == "--self-test" || a == "--profile-transitions")
            || crate::platform::argument("--benchmark-scenario").is_some())
            && crate::platform::argument("--data-dir").is_none()
        {
            return Err(Error::new(
                HRESULT(0x80070057u32 as i32),
                "Diagnostics require a separate --data-dir",
            ));
        }
        if std::env::args().any(|a| a == "--quit") {
            if let Ok(hwnd) = FindWindowW(w!("DynamicIslandForWindows"), None) {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex = CreateMutexW(None, false, w!("Local\\DynamicIslandForWindows.Instance"))?;
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        if already {
            if !std::env::args().any(|a| a == "--autostart")
                && let Ok(hwnd) = FindWindowW(w!("DynamicIslandForWindows"), None)
            {
                let _ = PostMessageW(Some(hwnd), WM_APP + 3, WPARAM(0), LPARAM(0));
            }
            let _ = CloseHandle(mutex);
            return Ok(());
        }
        crate::startup::refresh_copy();
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("DynamicIslandForWindows");
        let wc = WNDCLASSW {
            hInstance: instance,
            lpszClassName: class,
            lpfnWndProc: Some(proc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let inspect = std::env::args().any(|a| a == "--inspect-ui");
        let taskbar = if inspect {
            WS_EX_APPWINDOW
        } else {
            WS_EX_TOOLWINDOW
        };
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | taskbar | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            w!("Dynamic Island For Windows"),
            WS_POPUP,
            0,
            0,
            ui::COMPACT_WIDTH,
            ui::COMPACT_HEIGHT,
            None,
            None,
            Some(instance),
            None,
        )?;
        let taskbar_message = RegisterWindowMessageW(w!("TaskbarCreated"));
        let app = Box::new(std::cell::RefCell::new(App::new(hwnd, taskbar_message)?));
        let ptr = app.as_ref() as *const std::cell::RefCell<App>;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        app.borrow().configure();
        app.borrow_mut().fullscreen_tick();
        app.borrow_mut().paint();
        let show_initial = app.borrow().surface_visible();
        if show_initial {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
        }
        app.borrow().configure();
        app.borrow_mut().schedule();
        if app.borrow().transition_profile {
            SetTimer(Some(hwnd), 5, 650, None);
        }
        if app.borrow().self_test {
            SetTimer(Some(hwnd), 3, 1000, None);
        }
        if app.borrow().run_for_ms.is_some() {
            SetTimer(Some(hwnd), 4, 1000, None);
        }
        let mut msg = MSG::default();
        loop {
            let code = GetMessageW(&mut msg, None, 0, 0).0;
            if code <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        app.borrow_mut().shutdown();
        let _ = CloseHandle(mutex);
        Ok(())
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        // Nested message loops must release the wake slot even if App cannot
        // be borrowed. The next frame retries instead of freezing the clock.
        if msg == crate::frame_clock::FRAME_MESSAGE {
            crate::frame_clock::acknowledge_message();
        }
        if msg == WM_DESTROY {
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 {
                PostQuitMessage(0);
            }
            return LRESULT(0);
        }
        if msg == WM_CLOSE {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const std::cell::RefCell<App>;
            if !pointer.is_null()
                && let Ok(mut app) = (&*pointer).try_borrow_mut()
            {
                app.stop.store(true, Ordering::Relaxed);
                app.bus.hwnd.store(0, Ordering::Release);
                app.tray.remove();
            }
            let _ = DestroyWindow(hwnd);
            return LRESULT(0);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        // Native caption dragging enters a nested message loop. Do not hold App's
        // RefCell borrow across it, or moving/end-move messages cannot update the anchor.
        if msg == WM_NCLBUTTONDOWN || msg == WM_SYSCOMMAND {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const std::cell::RefCell<App>;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        // Win32 can synchronously re-enter a window procedure during painting or
        // placement. A RefCell prevents aliased mutable App references in that case.
        let Ok(mut borrowed) = (&*ptr).try_borrow_mut() else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        let app = &mut *borrowed;
        match msg {
            crate::frame_clock::FRAME_MESSAGE => {
                app.frame_tick();
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(if app.state.editing.is_some() {
                MA_ACTIVATE
            } else {
                MA_NOACTIVATE
            } as isize),
            WM_KEYDOWN if app.state.editing == Some(EditField::Note) => {
                app.note_key(wp.0 as u16);
                LRESULT(0)
            }
            WM_UNICHAR if app.state.editing == Some(EditField::Note) => {
                if wp.0 != 0xffff {
                    app.note_char(wp.0 as u32);
                }
                LRESULT(1)
            }
            WM_CHAR if app.state.editing == Some(EditField::Note) => {
                app.note_char(wp.0 as u32);
                LRESULT(0)
            }
            WM_CHAR if matches!(app.state.editing, Some(EditField::Timer(_))) => {
                app.timer_char(wp.0 as u32);
                LRESULT(0)
            }
            WM_KEYDOWN if matches!(app.state.editing, Some(EditField::Timer(_))) => {
                if wp.0 as u16 == VK_DELETE.0 && app.state.edit_select_all {
                    app.state.edit_text.clear();
                    app.state.edit_select_all = false;
                    app.paint();
                }
                LRESULT(0)
            }
            WM_CHAR if app.state.editing.is_some() => {
                match wp.0 as u32 {
                    13 => app.action(Action::WeatherSubmit),
                    27 => app.action(Action::WeatherCancel),
                    8 => {
                        if app.state.edit_select_all {
                            app.state.edit_text.clear();
                            app.state.edit_select_all = false;
                        } else {
                            app.state.edit_text.pop();
                        }
                        app.paint();
                    }
                    1 => {
                        app.state.edit_select_all = true;
                        app.paint();
                    }
                    c if c >= 32 && app.state.edit_text.len() < 160 => {
                        if let Some(c) = char::from_u32(c) {
                            if app.state.edit_select_all {
                                app.state.edit_text.clear();
                                app.state.edit_select_all = false;
                            }
                            app.state.edit_text.push(c);
                            app.paint();
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_ACTIVATE if wp.0 as u16 == WA_INACTIVE as u16 && app.state.editing.is_some() => {
                app.end_edit();
                app.paint();
                LRESULT(0)
            }
            event::WORKER_MESSAGE => {
                app.events();
                LRESULT(0)
            }
            WM_APP_PLUS_THREE => {
                if !app.state.visible {
                    app.action(Action::Hide);
                }
                app.action(Action::Select(Tab::Media));
                LRESULT(0)
            }
            WM_TIMER => {
                match wp.0 {
                    1 => {
                        app.wake_due = None;
                        app.tick();
                        app.paint();
                    }
                    2 => app.animate(),
                    3 => app.test_tick(),
                    5 => app.profile_transition(),
                    6 => app.pulse_tick(),
                    7 => app.pointer_tick(),
                    8 => app.fullscreen_tick(),
                    9 => app.browser_fade_tick(now_ms()),
                    4 if app
                        .run_for_ms
                        .is_some_and(|duration| now_ms() - app.test_start >= duration) =>
                    {
                        app.trace("run-for: deadline reached");
                        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    4 => {
                        app.trace(&format!(
                            "run-for: elapsed={}, duration={:?}",
                            now_ms() - app.test_start,
                            app.run_for_ms
                        ));
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = app.color_point(lp);
                if app.state.expanded && app.state.tab == Tab::Notes && app.state.notices.is_empty()
                {
                    let [nx, ny, nw, nh] = crate::notes::BODY;
                    if x >= nx as f32
                        && x < (nx + nw) as f32
                        && y >= ny as f32
                        && y < (ny + nh) as f32
                        && app.state.note.readable
                    {
                        let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                        app.note_point(x, y, shift);
                        app.note_drag = true;
                        SetCapture(app.hwnd);
                        return LRESULT(0);
                    }
                }
                app.begin_color_drag(x, y);
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                app.note_drag = false;
                app.finish_color_drag();
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if app.note_drag {
                    let (x, y) = app.color_point(lp);
                    app.note_point(x, y, true);
                    app.note_drag = false;
                    let _ = ReleaseCapture();
                    return LRESULT(0);
                }
                if app.color_drag.is_some() {
                    let (x, y) = app.color_point(lp);
                    app.drag_color(x, y);
                    app.finish_color_drag();
                    app.paint();
                    return LRESULT(0);
                }
                if app.animation.is_none() || app.state.expanded {
                    let (x, y) = app.color_point(lp);
                    let (x, y) = (x as i32, y as i32);
                    if let Some(action) = app
                        .hits
                        .iter()
                        .rev()
                        .find(|h| h.contains(x, y))
                        .map(|h| h.action)
                    {
                        app.action(action);
                    }
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                // Diagnostic clicks are posted directly. Ignore the real
                // desktop pointer/Alt state while deterministic fixtures run.
                if app.self_test || app.transition_profile {
                    return LRESULT(HTTRANSPARENT as isize);
                }
                let mut p = POINT {
                    x: lp.0 as i16 as i32,
                    y: (lp.0 >> 16) as i16 as i32,
                };
                let alt = GetAsyncKeyState(VK_MENU.0 as i32) < 0;
                if alt {
                    app.set_alt_bypass(true);
                    return LRESULT(HTTRANSPARENT as isize);
                }
                let pass = crate::interaction::pass_through(
                    app.body_rect(),
                    [p.x as f32, p.y as f32],
                    app.scale,
                    app.state.expanded,
                    !app.state.notices.is_empty(),
                    false,
                );
                app.set_pass_through(pass);
                if pass || app.peek_through || app.fullscreen_hidden || app.browser_hidden {
                    return LRESULT(HTTRANSPARENT as isize);
                }
                let _ = ScreenToClient(hwnd, &mut p);
                if app.canvas.as_ref().is_none_or(|c| !c.hit_test(p.x, p.y)) {
                    return LRESULT(HTTRANSPARENT as isize);
                }
                let (origin, top) = app.content_origin();
                let x = (p.x as f32 / app.scale - origin) as i32;
                let y = (p.y as f32 / app.scale - top) as i32;
                if app.state.expanded
                    && app.dock_animation.is_none()
                    && app.state.notices.is_empty()
                    && y < 48
                    && x < ui::expanded_width(&app.state) - 65
                {
                    LRESULT(HTCAPTION as isize)
                } else {
                    LRESULT(HTCLIENT as isize)
                }
            }
            WM_MOUSEMOVE => {
                if app.note_drag {
                    let (x, y) = app.color_point(lp);
                    app.note_point(x, y, true);
                    return LRESULT(0);
                }
                if app.color_drag.is_some() {
                    let (x, y) = app.color_point(lp);
                    app.drag_color(x, y);
                    return LRESULT(0);
                }
                let (x, y) = app.color_point(lp);
                let (x, y) = (x as i32, y as i32);
                let action = app
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.contains(x, y))
                    .map(|h| h.action);
                if action != app.state.hover {
                    app.state.hover = action;
                    app.paint();
                }
                if action.is_some()
                    && let Ok(cursor) = LoadCursorW(
                        None,
                        if matches!(action, Some(Action::NoteEdit | Action::TimerEdit(_))) {
                            IDC_IBEAM
                        } else {
                            IDC_HAND
                        },
                    )
                {
                    SetCursor(Some(cursor));
                }
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    ..Default::default()
                };
                let _ = TrackMouseEvent(&mut track);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                if app.state.expanded && app.state.tab == Tab::Notes && app.state.notices.is_empty()
                {
                    if let Some(c) = &mut app.canvas {
                        let height = c.note_height(&app.state.note.editor.text);
                        let e = &mut app.state.note.editor;
                        e.scroll = (e.scroll - ((wp.0 >> 16) as i16 as f32 / 120.) * 66.)
                            .clamp(0., (height - crate::notes::TEXT[3] as f32).max(0.));
                    }
                    app.paint();
                    return LRESULT(0);
                }
                if app.state.expanded && app.state.tab == Tab::Media && app.state.show_lyrics {
                    let mut point = POINT {
                        x: lp.0 as i16 as i32,
                        y: (lp.0 >> 16) as i16 as i32,
                    };
                    let _ = ScreenToClient(hwnd, &mut point);
                    if point.x as f32 / app.scale - ui::surface_inset(&app.state) as f32 > 320.0 {
                        app.action(Action::LyricsScroll(-((wp.0 >> 16) as i16 as i32).signum()));
                    }
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                app.state.hover = None;
                app.paint();
                LRESULT(0)
            }
            WM_ENTERSIZEMOVE => {
                app.begin_move();
                LRESULT(0)
            }
            WM_MOVING => {
                let r = &mut *(lp.0 as *mut RECT);
                app.anchor = (
                    (r.left + r.right) / 2,
                    r.top + (ui::top_inset(&app.state) as f32 * app.scale).round() as i32,
                );
                if app.state.config.dock_mode == crate::storage::DockMode::Anchored {
                    let top = app.fitted_anchor(app.size).1;
                    let height = r.bottom - r.top;
                    r.top = top;
                    r.bottom = top + height;
                    app.anchor.1 = top;
                }
                LRESULT(1)
            }
            WM_EXITSIZEMOVE => {
                app.end_move();
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let rect = *(lp.0 as *const RECT);
                app.scale = (wp.0 & 0xffff) as f32 / 96.0;
                app.anchor = (
                    (rect.left + rect.right) / 2,
                    rect.top + (ui::top_inset(&app.state) as f32 * app.scale).round() as i32,
                );
                app.layout(false);
                app.paint();
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                app.clamp_position();
                app.fullscreen_tick();
                app.schedule();
                app.paint();
                LRESULT(0)
            }
            WM_POWERBROADCAST | WM_TIMECHANGE => {
                app.fullscreen_tick();
                app.tick();
                app.paint();
                LRESULT(1)
            }
            TRAY_MESSAGE => {
                match lp.0 as u32 & 0xffff {
                    WM_CONTEXTMENU | WM_RBUTTONUP => app.menu(),
                    WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => {
                        if !app.state.visible {
                            app.action(Action::Hide);
                        }
                        app.action(Action::Expand);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            _ if msg == app.taskbar_message => {
                app.tray.add();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
const WM_APP_PLUS_THREE: u32 = WM_APP + 3;
const WM_MOUSELEAVE: u32 = 0x02a3;
const NIN_KEYSELECT: u32 = WM_USER + 1;
