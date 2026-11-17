use crate::{
    media::{MediaSnapshot, levels::BARS},
    reminders::WaterReminder,
    storage::{Config, Store},
    timer::Countdown,
};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Media,
    Weather,
    Timer,
    Water,
    Notes,
    Settings,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Startup,
    Topmost,
    Clock24,
    Date,
    Opacity,
    Visualizer,
    Sensitivity,
    Smoothing,
    Bluetooth,
    Alarm,
    NoticeDuration,
    WaterInterval,
    ActiveStart,
    ActiveEnd,
    Quiet,
    QuietStart,
    QuietEnd,
    OnlineLyrics,
    LyricsOffset,
    Stroke,
    StrokeWidth,
    StrokeGlow,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Expand,
    Collapse,
    Select(Tab),
    Play,
    Previous,
    Next,
    TimerToggle,
    TimerReset,
    Preset(u64),
    Duration(i32),
    WaterConfirm,
    WaterSnooze,
    WaterToggle,
    WaterSchedule,
    NoteEdit,
    NoteCopy,
    SettingsPage(u8),
    Toggle(Setting),
    Adjust(Setting, i32),
    ResetPosition,
    Hide,
    Quit,
    Dock(crate::storage::DockMode),
    LyricsRetry,
    LyricsSettings,
    LyricsScroll(i32),
    LyricsToggle,
    PickColor(crate::color::Part),
    CustomStroke,
    WeatherSearch,
    WeatherSubmit,
    WeatherCancel,
    WeatherChoose(usize),
    WeatherRefresh,
    WeatherAttribution,
}
#[derive(Clone)]
pub struct Notice {
    pub title: String,
    pub detail: String,
    pub tab: Tab,
    pub device: Option<crate::bluetooth::DeviceKind>,
}
pub struct State {
    pub startup: crate::startup::Status,
    pub config: Config,
    pub store: Store,
    pub timer: Countdown,
    pub water: WaterReminder,
    pub note: crate::notes::Note,
    pub media: MediaSnapshot,
    pub lyrics: crate::media::lyrics::LyricsState,
    pub show_lyrics: bool,
    pub weather: crate::weather::WeatherState,
    pub editing: Option<crate::app_state::EditField>,
    pub edit_text: String,
    pub edit_select_all: bool,
    pub stroke_hsv: crate::color::Hsv,
    pub bars: [f32; BARS],
    pub audio_at_ms: u64,
    pub audio_status: String,
    pub bluetooth_status: String,
    pub expanded: bool,
    pub tab: Tab,
    pub settings_page: u8,
    pub water_schedule: bool,
    pub notices: VecDeque<Notice>,
    pub notice_until: u64,
    pub visible: bool,
    pub hover: Option<Action>,
    pub interaction: crate::interaction::Interaction,
    pub face: crate::face::Face,
    pub status: String,
}
impl State {
    pub fn new(now: u64, day: u32) -> Self {
        let store = Store::new();
        let (config, warning) = store.load();
        let timer = Countdown::new(config.timer_seconds);
        let count = if config.water_day == day {
            config.water_count
        } else {
            0
        };
        let water = WaterReminder::new(now, &config.water, day, count);
        Self {
            startup: Default::default(),
            note: crate::notes::Note::load(&store.directory),
            stroke_hsv: crate::color::Hsv::from_rgb(config.stroke_color),
            config,
            store,
            timer,
            water,
            media: MediaSnapshot::default(),
            lyrics: Default::default(),
            show_lyrics: false,
            weather: Default::default(),
            editing: None,
            edit_text: String::new(),
            edit_select_all: false,
            bars: [0.0; BARS],
            audio_at_ms: 0,
            audio_status: "Connecting to system output…".into(),
            bluetooth_status: "Starting device watcher…".into(),
            expanded: false,
            tab: Tab::Media,
            settings_page: 0,
            water_schedule: false,
            notices: VecDeque::new(),
            notice_until: 0,
            visible: true,
            hover: None,
            interaction: Default::default(),
            face: crate::face::Face::new(now),
            status: warning.unwrap_or_default(),
        }
    }
    pub fn idle(&self) -> bool {
        !self.expanded
            && self.notices.is_empty()
            && !self.media.playing
            && !matches!(
                self.timer.status,
                crate::timer::Status::Running | crate::timer::Status::Paused
            )
    }
    pub fn save(&mut self) {
        self.config.water_day = self.water.day;
        self.config.water_count = self.water.count;
        if let Err(e) = self.store.save(&self.config) {
            self.status = "Could not save settings. Check the data folder.".into();
            self.store
                .log(&format!("Settings write failed: {}", e.kind()));
        }
    }
    pub fn notify(&mut self, notice: Notice, now: u64) {
        if self
            .notices
            .iter()
            .any(|n| n.title == notice.title && n.detail == notice.detail)
        {
            return;
        }
        if self.notices.len() >= 6 {
            self.notices.pop_back();
        }
        if self.notices.is_empty() {
            self.notice_until = now + self.config.notification_seconds as u64 * 1000;
        }
        self.notices.push_back(notice);
    }
    pub fn dismiss_notice(&mut self, now: u64) {
        self.notices.pop_front();
        self.notice_until = now + self.config.notification_seconds as u64 * 1000;
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Note,
    City,
    Stroke,
}
