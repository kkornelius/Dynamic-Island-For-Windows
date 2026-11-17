mod artwork;
pub mod canvas;
mod demo;
mod face;
mod graphics;
mod icons;
mod notes;
mod preview;
mod utilities;
mod verification;
mod weather;
use crate::{
    app_state::*,
    media::lyrics::Status as LyricsStatus,
    platform::LocalTime,
    storage::DockMode,
    timer::{Status, format_duration},
};
use canvas::*;
pub use demo::render_demo;
use icons::Icon;
pub use preview::render_design;
pub use verification::{verify_color_reuse, verify_partial, verify_reuse};
pub fn release_graphics() {
    graphics::Graphics::release_thread();
}
pub const WIDTH: i32 = 456;
pub const MEDIA_WIDTH: i32 = 748;
pub const HEIGHT: i32 = 576;
pub const HALO: i32 = 28;
pub fn top_inset(s: &State) -> i32 {
    if s.config.dock_mode == DockMode::Anchored {
        0
    } else {
        HALO
    }
}
pub fn surface_inset(s: &State) -> i32 {
    gutter(s) + HALO
}
pub const COMPACT_WIDTH: i32 = 336;
pub const COMPACT_HEIGHT: i32 = 58;
pub fn gutter(s: &State) -> i32 {
    if s.config.dock_mode == DockMode::Anchored {
        12
    } else {
        0
    }
}
pub fn expanded_width(s: &State) -> i32 {
    if s.tab == Tab::Media && s.media.available && s.show_lyrics {
        MEDIA_WIDTH
    } else {
        WIDTH
    }
}
/// The only region animated on the audio clock. All other content keeps its original cadence.
pub fn meter_rect(s: &State) -> Option<[i32; 4]> {
    if !s.notices.is_empty() {
        return None;
    }
    if s.expanded {
        Some([expanded_width(s) - 112, 18, 38, 18])
    } else if s.media.available && !s.idle() {
        s.media.playing.then_some([290, 19, 30, 20])
    } else if s.idle() {
        None
    } else {
        Some([280, 17, 40, 22])
    }
}
pub fn expanded_height(s: &State) -> i32 {
    match s.tab {
        Tab::Media => 336,
        Tab::Weather => 420,
        Tab::Timer => 500,
        Tab::Water => 520,
        Tab::Notes => 500,
        Tab::Settings => HEIGHT,
    }
}
pub fn dock_rect(s: &State, i: usize) -> [i32; 4] {
    [
        expanded_width(s) / 2 - 157 + i as i32 * 54,
        expanded_height(s) - 61,
        44,
        40,
    ]
}
#[derive(Clone, Copy)]
pub struct Hit {
    pub rect: [i32; 4],
    pub action: Action,
}
impl Hit {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.rect[0]
            && y >= self.rect[1]
            && x < self.rect[0] + self.rect[2]
            && y < self.rect[1] + self.rect[3]
    }
}
struct View<'a> {
    c: &'a mut Canvas,
    s: &'a State,
    hits: Vec<Hit>,
}
impl View<'_> {
    fn button(&mut self, label: &str, r: [i32; 4], a: Action, primary: bool, enabled: bool) {
        let [x, y, w, h] = r;
        let hover = self.s.hover == Some(a);
        self.c.round(
            x,
            y,
            w,
            h,
            11,
            if primary && enabled {
                if hover { 0xe3e3e3 } else { WHITE }
            } else if hover && enabled {
                0x303030
            } else {
                CARD
            },
        );
        self.c.text(
            label,
            x + 4,
            y,
            w - 8,
            h,
            13,
            primary,
            if !enabled {
                DIM
            } else if primary {
                BG
            } else {
                WHITE
            },
            true,
        );
        if enabled {
            self.hits.push(Hit { rect: r, action: a });
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        t: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        size: i32,
        bold: bool,
        color: u32,
    ) {
        self.c.text(t, x, y, w, h, size, bold, color, false);
    }
    fn center(&mut self, t: &str, y: i32, h: i32, size: i32, bold: bool, color: u32) {
        self.c
            .text(t, 24, y, WIDTH - 48, h, size, bold, color, true);
    }
    fn toggle(&mut self, title: &str, subtitle: &str, y: i32, setting: Setting, on: bool) {
        self.label(title, 28, y, 336, 23, 14, true, WHITE);
        self.label(subtitle, 28, y + 23, 336, 20, 11, false, MUTED);
        self.c
            .round(384, y + 9, 42, 23, 12, if on { 0xe5e5e5 } else { LINE });
        self.c.circle(
            if on { 406 } else { 387 },
            y + 12,
            17,
            if on { BG } else { MUTED },
        );
        self.hits.push(Hit {
            rect: [24, y, 408, 46],
            action: Action::Toggle(setting),
        });
    }
    fn stepper(&mut self, title: &str, value: &str, y: i32, setting: Setting, step: i32) {
        self.label(title, 28, y, 240, 35, 14, false, WHITE);
        self.button(
            "−",
            [290, y, 32, 34],
            Action::Adjust(setting, -step),
            false,
            true,
        );
        self.c.text(value, 324, y, 62, 34, 13, true, WHITE, true);
        self.button(
            "+",
            [390, y, 32, 34],
            Action::Adjust(setting, step),
            false,
            true,
        );
    }
    fn icon_button(
        &mut self,
        icon: Icon,
        r: [i32; 4],
        action: Action,
        selected: bool,
        enabled: bool,
    ) {
        let [x, y, w, h] = r;
        if selected || self.s.hover == Some(action) {
            self.c
                .round(x, y, w, h, 12, if selected { 0xe6e6e6 } else { CARD });
        }
        icons::draw(
            self.c,
            icon,
            x + (w - 22) / 2,
            y + (h - 22) / 2,
            22,
            if !enabled {
                DIM
            } else if selected {
                0x141414
            } else {
                WHITE
            },
        );
        if enabled {
            self.hits.push(Hit { rect: r, action });
        }
    }
    fn meter(&mut self, x: i32, y: i32, w: i32, h: i32, now: u64) {
        let bars = crate::media::pulse::segments(
            &self.s.bars,
            self.s.audio_at_ms,
            now,
            self.s.config.visualizer,
            [x, y, w, h],
        );
        self.c.rounded_lines(
            &bars,
            if self.s.media.spotify() {
                SPOTIFY
            } else {
                WHITE
            },
            2.2,
        );
    }
}
pub fn draw(c: &mut Canvas, s: &State, now: u64, local: &LocalTime) -> Vec<Hit> {
    c.begin(surface_inset(s), top_inset(s));
    c.set_border(
        s.config.stroke_enabled,
        s.config.stroke_color,
        s.config.stroke_width,
        s.config.stroke_glow,
    );
    let w = if !s.notices.is_empty() {
        384
    } else if s.expanded {
        expanded_width(s)
    } else {
        COMPACT_WIDTH
    };
    let h = if !s.notices.is_empty() {
        82
    } else if s.expanded {
        expanded_height(s)
    } else {
        COMPACT_HEIGHT
    };
    if s.media.available
        && !s.idle()
        && s.media.spotify()
        && (!s.expanded || s.tab == Tab::Media)
        && s.notices.is_empty()
    {
        c.spotify_gradient(-gutter(s), 0, w + 2 * gutter(s), h);
    } else {
        c.round(-gutter(s), 0, w + 2 * gutter(s), h, 0, BG);
    }
    let mut v = View {
        c,
        s,
        hits: Vec::new(),
    };
    if let Some(notice) = s.notices.front() {
        v.c.round(17, 20, 42, 42, 14, 0x232323);
        let icon = if let Some(d) = notice.device {
            icons::device(d)
        } else {
            match notice.tab {
                Tab::Timer => Icon::Timer,
                Tab::Water => Icon::Water,
                _ => Icon::Bluetooth,
            }
        };
        icons::draw(v.c, icon, 26, 29, 24, WHITE);
        v.label(&notice.title, 73, 16, 264, 25, 14, true, WHITE);
        v.label(&notice.detail, 73, 43, 264, 21, 12, false, MUTED);
        v.icon_button(
            Icon::Close,
            [345, 24, 28, 28],
            Action::Collapse,
            false,
            true,
        );
        v.hits.push(Hit {
            rect: [10, 5, 330, 72],
            action: Action::Select(notice.tab),
        });
        return v.hits;
    }
    if !s.expanded {
        if s.media.available && !s.idle() {
            if s.media.spotify() {
                v.c.spotify(18, 18, 22);
            } else if let Some(art) = &s.media.artwork {
                v.c.artwork(art, 16, 14, 30);
            } else {
                icons::draw(v.c, Icon::Music, 18, 17, 24, WHITE);
            }
            let line = s
                .lyrics
                .current_line(&s.media, now, s.config.lyrics_offset_ms)
                .filter(|l| !l.is_empty());
            v.label(
                &s.media.title,
                54,
                9,
                224,
                20,
                if line.is_some() { 11 } else { 14 },
                line.is_none(),
                if line.is_some() { MUTED } else { WHITE },
            );
            v.label(
                line.unwrap_or(&s.media.artist),
                54,
                29,
                224,
                20,
                if line.is_some() { 13 } else { 11 },
                line.is_some(),
                if line.is_some() { WHITE } else { MUTED },
            );
            if s.media.playing {
                v.meter(290, 19, 30, 20, now);
            } else {
                icons::draw(v.c, Icon::Pause, 294, 21, 17, MUTED);
            }
        } else if s.idle() {
            v.label(
                &local.clock(s.config.clock_24h),
                22,
                14,
                105,
                30,
                if s.config.clock_24h { 22 } else { 17 },
                true,
                WHITE,
            );
            face::draw(v.c, s.face.pose(now), 168.0, 26.0);
            if s.config.show_date {
                let text = local.date();
                let (day, date) = text.split_once(", ").unwrap_or((&text, ""));
                v.label(day, 232, 10, 84, 20, 11, false, MUTED);
                v.label(date, 232, 29, 84, 20, 12, true, WHITE);
            }
        } else {
            v.label(
                &local.clock(s.config.clock_24h),
                22,
                9,
                150,
                26,
                20,
                true,
                WHITE,
            );
            let hint = if matches!(s.timer.status, Status::Running | Status::Paused) {
                format!("Timer {}", format_duration(s.timer.remaining_ms(now)))
            } else if s.config.show_date {
                local.date()
            } else {
                String::new()
            };
            v.label(&hint, 23, 34, 245, 17, 11, false, MUTED);
            v.meter(280, 17, 40, 22, now);
        }
        v.hits.push(Hit {
            rect: [-gutter(s), 0, COMPACT_WIDTH + 2 * gutter(s), COMPACT_HEIGHT],
            action: Action::Expand,
        });
        return v.hits;
    }
    v.label(
        &local.clock(s.config.clock_24h),
        27,
        13,
        110,
        27,
        12,
        true,
        WHITE,
    );
    if s.config.show_date {
        v.label(&local.date(), 126, 14, w - 252, 25, 10, false, DIM);
    }
    v.meter(w - 112, 18, 38, 18, now);
    v.button("−", [w - 50, 14, 28, 27], Action::Collapse, false, true);
    match s.tab {
        Tab::Media => media(&mut v, now),
        Tab::Weather => weather::draw(&mut v, now),
        Tab::Timer => utilities::timer(&mut v, now),
        Tab::Water => utilities::water(&mut v, now),
        Tab::Notes => notes::draw(&mut v, now),
        Tab::Settings => settings(&mut v),
    }
    v.c.gradient(w / 2 - 173, h - 65, 346, 48, 20, 0x202020, 0x161616);
    for (i, (tab, icon, label)) in [
        (Tab::Media, Icon::Music, "Music"),
        (Tab::Weather, Icon::Weather, "Weather"),
        (Tab::Timer, Icon::Timer, "Timer"),
        (Tab::Water, Icon::Water, "Water"),
        (Tab::Notes, Icon::Notes, "Notes"),
        (Tab::Settings, Icon::Settings, "Settings"),
    ]
    .into_iter()
    .enumerate()
    {
        let r = dock_rect(s, i);
        v.icon_button(icon, r, Action::Select(tab), s.tab == tab, true);
        if s.hover == Some(Action::Select(tab)) {
            v.c.round(r[0] - 15, r[1] - 29, 74, 23, 8, 0x303030);
            v.c.text(label, r[0] - 15, r[1] - 29, 74, 23, 10, false, WHITE, true);
        }
    }
    if s.tab == Tab::Settings && !s.status.is_empty() {
        let status_y = match s.settings_page {
            3 => 175,
            0 => 442,
            _ => h - 97,
        };
        v.label(
            &s.status,
            28,
            status_y,
            w - 56,
            if s.settings_page == 0 { 16 } else { 22 },
            10,
            false,
            MUTED,
        );
    }
    v.hits
}
fn media(v: &mut View<'_>, now: u64) {
    let m = &v.s.media;
    if !m.available {
        icons::draw(v.c, Icon::Music, 205, 81, 46, MUTED);
        v.center("A moment of quiet", 137, 35, 23, true, WHITE);
        v.center("Play something you love.", 176, 24, 13, false, MUTED);
        return;
    }
    let split = v.s.show_lyrics;
    let pw = if split { 280 } else { 400 };
    if let Some(art) = &m.artwork {
        v.c.artwork(art, 28, 66, 68);
    } else {
        v.c.gradient(28, 66, 68, 68, 16, 0x242424, 0x121212);
        icons::draw(v.c, Icon::Music, 49, 87, 26, WHITE);
    }
    v.label(
        if m.title.is_empty() {
            "Untitled media"
        } else {
            &m.title
        },
        112,
        69,
        pw - 89,
        27,
        if split { 16 } else { 18 },
        true,
        WHITE,
    );
    v.label(
        if m.artist.is_empty() {
            "Artist unavailable"
        } else {
            &m.artist
        },
        112,
        99,
        pw - 89,
        23,
        12,
        false,
        MUTED,
    );
    if m.spotify() {
        v.c.spotify(30, 143, 12);
        v.label("Spotify", 48, 138, 100, 23, 10, false, 0xa2c2b2);
    } else {
        v.label(&m.source, 28, 138, pw, 23, 10, false, MUTED);
    }
    if m.duration_ms > 0 {
        let position = m.position(now);
        let progress = (pw as f64 * position as f64 / m.duration_ms as f64) as i32;
        v.c.round(28, 172, pw, 4, 2, 0x363636);
        v.c.round(
            28,
            172,
            progress.max(2),
            4,
            2,
            if m.spotify() { SPOTIFY } else { WHITE },
        );
        v.c.circle(24 + progress, 169, 10, WHITE);
        v.label(
            &format_duration(position),
            28,
            181,
            80,
            18,
            10,
            false,
            MUTED,
        );
        v.c.text(
            &format!(
                "−{}",
                format_duration(m.duration_ms.saturating_sub(position))
            ),
            pw - 46,
            181,
            74,
            18,
            10,
            false,
            MUTED,
            true,
        );
    }
    let mid = 28 + pw / 2;
    v.icon_button(
        Icon::Lyrics,
        [mid - 108, 201, 36, 32],
        Action::LyricsToggle,
        split,
        true,
    );
    v.icon_button(
        Icon::Previous,
        [mid - 60, 201, 36, 32],
        Action::Previous,
        false,
        m.can_previous,
    );
    v.icon_button(
        if m.playing { Icon::Pause } else { Icon::Play },
        [mid - 16, 198, 40, 36],
        Action::Play,
        false,
        if m.playing { m.can_pause } else { m.can_play },
    );
    v.icon_button(
        Icon::Next,
        [mid + 32, 201, 36, 32],
        Action::Next,
        false,
        m.can_next,
    );
    if split {
        v.c.line(329, 72, 329, 220, 0x303030, 1);
        lyrics(v, now);
    }
}
fn lyrics(v: &mut View<'_>, now: u64) {
    let x = 354;
    let width = 362;
    if let Some(d) = v.s.lyrics.document() {
        if d.instrumental {
            v.c.text_block("Just the music.", x, 120, width, 60, 24, true, WHITE);
            return;
        }
        if !d.lines.is_empty() && v.s.media.timeline_available {
            let active = d.active(v.s.lyrics.position(
                &v.s.media,
                now,
                v.s.config.lyrics_offset_ms,
            ));
            let index = active.unwrap_or(0);
            if let Some(p) = index.checked_sub(1).and_then(|i| d.lines.get(i)) {
                v.c.text_block(&p.text, x, 62, width, 40, 15, false, DIM);
            }
            if let Some(p) = d.lines.get(index) {
                v.c.text_block(
                    if p.text.is_empty() {
                        "•••"
                    } else {
                        &p.text
                    },
                    x,
                    110,
                    width,
                    66,
                    23,
                    true,
                    if active.is_some() { WHITE } else { MUTED },
                );
            }
            if let Some(p) = d.lines.get(index + 1) {
                v.c.text_block(&p.text, x, 182, width, 43, 16, false, MUTED);
            }
            return;
        }
        let plain: Vec<&str> = if d.plain.is_empty() {
            d.lines.iter().map(|l| l.text.as_str()).collect()
        } else {
            d.plain.iter().map(String::as_str).collect()
        };
        for (i, text) in plain.iter().skip(v.s.lyrics.scroll).take(3).enumerate() {
            v.c.text_block(
                text,
                x,
                67 + i as i32 * 49,
                width - 22,
                45,
                17,
                false,
                WHITE,
            );
        }
        v.button(
            "↑",
            [694, 195, 26, 26],
            Action::LyricsScroll(-1),
            false,
            v.s.lyrics.scroll > 0,
        );
        v.button(
            "↓",
            [694, 224, 26, 26],
            Action::LyricsScroll(1),
            false,
            v.s.lyrics.scroll + 3 < plain.len(),
        );
        return;
    }
    let (title, detail) = match v.s.lyrics.status {
        LyricsStatus::Loading => ("Finding the words…", "Looking up this song."),
        LyricsStatus::Offline => ("Lyrics are offline.", "Enable online lyrics in Settings."),
        LyricsStatus::NetworkError => (
            "Couldn't load lyrics.",
            "Check your connection and try again.",
        ),
        LyricsStatus::RateLimited => ("A little pause.", "Try again later."),
        _ => (
            "No lyrics just yet.",
            "Try another song, or check back later.",
        ),
    };
    v.c.text_block(title, x, 82, width, 45, 22, true, WHITE);
    v.c.text_block(detail, x, 133, width, 42, 13, false, MUTED);
    v.button(
        if matches!(v.s.lyrics.status, LyricsStatus::Offline) {
            "Set up lyrics"
        } else {
            "Try again"
        },
        [x, 190, 148, 32],
        if matches!(v.s.lyrics.status, LyricsStatus::Offline) {
            Action::LyricsSettings
        } else {
            Action::LyricsRetry
        },
        false,
        !matches!(
            v.s.lyrics.status,
            LyricsStatus::Loading | LyricsStatus::RateLimited
        ),
    );
}
fn settings(v: &mut View<'_>) {
    for (i, label) in ["Appearance", "Features", "Lyrics", "Stroke"]
        .into_iter()
        .enumerate()
    {
        v.button(
            label,
            [24 + i as i32 * 103, 68, 98, 33],
            Action::SettingsPage(i as u8),
            v.s.settings_page == i as u8,
            true,
        );
    }
    let c = &v.s.config;
    match v.s.settings_page {
        0 => {
            v.label("ISLAND POSITION", 28, 119, 390, 20, 10, true, MUTED);
            for (i, (label, mode)) in [
                ("Floating", DockMode::Floating),
                ("Anchored", DockMode::Anchored),
            ]
            .into_iter()
            .enumerate()
            {
                v.button(
                    label,
                    [28 + i as i32 * 204, 148, 194, 48],
                    Action::Dock(mode),
                    c.dock_mode == mode,
                    true,
                );
            }
            v.label(
                if c.dock_mode == DockMode::Anchored {
                    "Attached to the top edge"
                } else {
                    "A floating capsule you can move"
                },
                28,
                202,
                252,
                23,
                11,
                false,
                MUTED,
            );
            v.c.text("Reset position", 292, 202, 132, 23, 11, false, WHITE, true);
            v.hits.push(Hit {
                rect: [288, 200, 140, 29],
                action: Action::ResetPosition,
            });
            v.toggle(
                "Always on top",
                "Keep the island above other windows",
                241,
                Setting::Topmost,
                c.always_on_top,
            );
            v.toggle(
                "24-hour clock",
                "Use your local Windows time",
                295,
                Setting::Clock24,
                c.clock_24h,
            );
            v.toggle(
                "Show date",
                "A quiet date beside the clock",
                349,
                Setting::Date,
                c.show_date,
            );
            v.stepper(
                "Opacity",
                &format!("{}%", c.opacity),
                407,
                Setting::Opacity,
                1,
            );
            v.toggle(
                "Run at startup",
                v.s.startup.detail(),
                461,
                Setting::Startup,
                v.s.startup.enabled(),
            );
        }
        1 => {
            v.toggle(
                "Real audio visualizer",
                "Reacts to the Windows output mix",
                125,
                Setting::Visualizer,
                c.visualizer,
            );
            v.stepper(
                "Sensitivity",
                &format!("{:.1}×", c.sensitivity),
                182,
                Setting::Sensitivity,
                1,
            );
            v.label(
                "Higher = taller bars for quiet sounds",
                28,
                217,
                392,
                20,
                11,
                false,
                MUTED,
            );
            v.stepper(
                "Smoothing",
                &format!("{}%", (c.smoothing * 100.).round()),
                247,
                Setting::Smoothing,
                5,
            );
            v.label(
                "Higher = a softer, slower fade when sound drops",
                28,
                282,
                400,
                20,
                11,
                false,
                MUTED,
            );
            v.toggle(
                "Bluetooth notifications",
                &v.s.bluetooth_status,
                313,
                Setting::Bluetooth,
                c.bluetooth,
            );
            v.toggle(
                "Timer sound",
                "Uses the Windows system volume",
                375,
                Setting::Alarm,
                c.alarm_sound,
            );
            v.stepper(
                "Notification duration",
                &format!("{}s", c.notification_seconds),
                443,
                Setting::NoticeDuration,
                1,
            );
        }
        2 => {
            v.toggle(
                "Online lyrics",
                "Find lyrics using title, artist and album",
                125,
                Setting::OnlineLyrics,
                c.online_lyrics,
            );
            v.c.text_block("Only title, artist, album and duration are sent to LRCLIB. Your audio and Spotify login stay on this PC. Saved lyrics work offline.",28,197,396,76,13,false,MUTED);
            v.stepper(
                "Timing offset",
                &format!("{:+}ms", c.lyrics_offset_ms),
                300,
                Setting::LyricsOffset,
                250,
            );
            v.label(
                "− delays the lyrics · + brings them forward",
                28,
                342,
                400,
                23,
                11,
                false,
                DIM,
            );
            v.button(
                "Reset timing",
                [28, 397, 130, 36],
                Action::Adjust(Setting::LyricsOffset, -c.lyrics_offset_ms),
                false,
                true,
            );
            v.button(
                "Reload current lyrics",
                [172, 397, 254, 36],
                Action::LyricsRetry,
                false,
                v.s.lyrics.key.is_some(),
            );
            v.label(
                if v.s.lyrics.document().is_some_and(|d| !d.lines.is_empty()) {
                    "Timed lyrics ready"
                } else {
                    "Untimed tracks use scroll controls until timing is found."
                },
                28,
                449,
                400,
                24,
                11,
                false,
                MUTED,
            );
        }
        _ => {
            v.toggle(
                "Capsule stroke",
                "A soft outline around your island",
                124,
                Setting::Stroke,
                c.stroke_enabled,
            );
            let hsv = if v.s.stroke_hsv.rgb() == c.stroke_color {
                v.s.stroke_hsv
            } else {
                crate::color::Hsv::from_rgb(c.stroke_color)
            };
            v.c.color_wheel(hsv);
            v.label("COLOR", 248, 201, 174, 21, 10, true, MUTED);
            v.c.round(248, 235, 174, 57, 14, c.stroke_color);
            v.c.outline(248, 235, 174, 57, 14, 0x525252, 1.0);
            v.button(
                &format!("#{:06X}", c.stroke_color),
                [248, 305, 174, 36],
                Action::CustomStroke,
                false,
                true,
            );
            v.label("Brightness", 248, 345, 115, 22, 12, false, MUTED);
            v.c.text(
                &format!("{}%", (hsv.v * 100.0).round()),
                361,
                345,
                61,
                22,
                12,
                false,
                WHITE,
                true,
            );
            v.c.horizontal_gradient(
                crate::color::VALUE,
                0x000000,
                crate::color::Hsv { v: 1.0, ..hsv }.rgb(),
            );
            v.c.outline(248, 374, 174, 18, 9, 0x525252, 1.0);
            let knob = 248 + (hsv.v * 174.0).round() as i32;
            v.c.circle(knob - 11, 372, 22, 0x151515);
            v.c.circle(knob - 9, 374, 18, WHITE);
            v.hits.push(Hit {
                rect: crate::color::WHEEL,
                action: Action::PickColor(crate::color::Part::Wheel),
            });
            v.hits.push(Hit {
                rect: [243, 367, 184, 32],
                action: Action::PickColor(crate::color::Part::Value),
            });
            v.stepper(
                "Stroke width",
                &format!("{} px", c.stroke_width),
                419,
                Setting::StrokeWidth,
                1,
            );
            v.stepper(
                "Glow",
                &format!("{}%", c.stroke_glow),
                466,
                Setting::StrokeGlow,
                10,
            );
            if v.s.editing == Some(EditField::Stroke) {
                v.c.round(235, 296, 195, 104, 13, CARD);
                if v.s.edit_select_all {
                    v.c.round(246, 304, 119, 28, 5, 0x505050);
                }
                v.label(
                    &format!("{}|", v.s.edit_text),
                    250,
                    303,
                    166,
                    31,
                    17,
                    true,
                    WHITE,
                );
                v.button(
                    "Save",
                    [248, 348, 78, 33],
                    Action::WeatherSubmit,
                    true,
                    true,
                );
                v.button(
                    "Cancel",
                    [335, 348, 81, 33],
                    Action::WeatherCancel,
                    false,
                    true,
                );
            }
        }
    }
}
