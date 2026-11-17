//! Deterministic layout fixtures, never used as live song data.
use super::*;
use std::sync::Arc;
pub(super) fn fixture_state() -> (State, u64) {
    let now = crate::platform::now_ms();
    let mut state = State::new(now, LocalTime::get().key());
    state.config.online_lyrics = false;
    state.config.opacity = 100;
    state.status = "Design preview · original test text".into();
    let mut pixels = Vec::with_capacity(160 * 160 * 4);
    for y in 0..160 {
        for x in 0..160 {
            let r = (28 + x / 2) as u8;
            let g = (61 + y / 2) as u8;
            let b = (46 + (x + y) / 5) as u8;
            pixels.extend_from_slice(&[b, g, r, 255]);
        }
    }
    state.media = crate::media::MediaSnapshot {
        available: true,
        title: "An evening, unhurried".into(),
        artist: "Studio session · layout fixture".into(),
        album: "Design preview".into(),
        source: "Spotify".into(),
        playing: true,
        can_pause: true,
        can_next: true,
        can_previous: true,
        position_ms: 22000,
        duration_ms: 186000,
        sampled_at_ms: now,
        timeline_available: true,
        playback_rate: 1.0,
        artwork: Some(Arc::new(crate::media::Artwork {
            width: 160,
            height: 160,
            pixels,
        })),
        ..Default::default()
    };
    state.lyrics.status = LyricsStatus::Ready(crate::media::lyrics::Document {
        lines: [
            (0, "Let the day slow down"),
            (10000, "Stay for a little while"),
            (20000, "There is room to breathe"),
            (30000, "Let the evening linger"),
            (40000, "A quiet corner of the day"),
        ]
        .into_iter()
        .map(|(at_ms, text)| crate::media::lyrics::LyricLine {
            at_ms,
            text: text.into(),
        })
        .collect(),
        ..Default::default()
    });
    state.weather.forecast = Some(crate::weather::Forecast {
        temperature: 29.,
        feels_like: 32.,
        code: 2,
        is_day: true,
        wind: 8.,
        high: 31.,
        low: 24.,
        time: 1794358800,
        fetched: 1794358800,
        utc_offset: 25200,
        hours: (1..=5)
            .map(|i| crate::weather::Hour {
                time: 1794358800 + i * 3600,
                temperature: 29. - i as f64,
                code: if i < 3 { 2 } else { 61 },
                rain: Some(20. + i as f64 * 10.),
                is_day: true,
            })
            .collect(),
    });
    state.audio_at_ms = now;
    state.water.count = 3;
    state.note = crate::notes::Note::load(std::path::Path::new("artifacts/preview-empty-note"));
    state.note.editor.insert("A few things for tomorrow\n\nPick up where you left off.\nKeep the afternoon free for a walk.\n\nOne small thing at a time.");
    state.note.saved = state.note.editor.revision;
    state.bars = std::array::from_fn(|i| [0.2, 0.4, 0.62, 0.85, 0.6, 0.35, 0.48][i % 7]);
    (state, now)
}
pub fn render_design() -> windows::core::Result<()> {
    let (mut state, now) = fixture_state();
    let folder = std::path::Path::new("artifacts/design-0.7.0");
    std::fs::create_dir_all(folder).map_err(io_error)?;
    for (name, expanded, tab, mode, page) in [
        ("floating-compact", false, Tab::Media, DockMode::Floating, 0),
        ("anchored-compact", false, Tab::Media, DockMode::Anchored, 0),
        ("media-player", true, Tab::Media, DockMode::Floating, 0),
        ("media-lyrics", true, Tab::Media, DockMode::Floating, 0),
        ("water", true, Tab::Water, DockMode::Floating, 0),
        ("weather", true, Tab::Weather, DockMode::Floating, 0),
        ("notes", true, Tab::Notes, DockMode::Floating, 0),
        ("anchored-notes", true, Tab::Notes, DockMode::Anchored, 0),
        (
            "settings-stroke",
            true,
            Tab::Settings,
            DockMode::Floating,
            3,
        ),
        ("anchored-media", true, Tab::Media, DockMode::Anchored, 0),
        (
            "settings-appearance",
            true,
            Tab::Settings,
            DockMode::Floating,
            0,
        ),
        (
            "settings-lyrics",
            true,
            Tab::Settings,
            DockMode::Floating,
            2,
        ),
        (
            "settings-features",
            true,
            Tab::Settings,
            DockMode::Floating,
            1,
        ),
        ("timer", true, Tab::Timer, DockMode::Floating, 0),
    ] {
        state.status = if name == "settings-features" {
            String::new()
        } else {
            "Design preview · original test text".into()
        };
        state.show_lyrics = name.contains("lyrics") || name == "anchored-media";
        state.expanded = expanded;
        state.tab = tab;
        state.config.dock_mode = mode;
        state.settings_page = page;
        let width = if expanded {
            expanded_width(&state)
        } else {
            COMPACT_WIDTH
        };
        let height = if expanded {
            expanded_height(&state)
        } else {
            COMPACT_HEIGHT
        };
        let mut canvas = Canvas::new(
            width + 2 * surface_inset(&state),
            height + top_inset(&state) + HALO,
            1.0,
        )?;
        draw(&mut canvas, &state, now, &LocalTime::get());
        canvas.finish()?;
        canvas.mask(if expanded { 28 } else { 29 }, mode);
        canvas
            .save_bmp(&folder.join(format!("{name}.bmp")))
            .map_err(io_error)?;
    }
    // Native eye poses: original vectors, never pasted image assets.
    state.media.playing = false;
    state.timer.reset();
    state.expanded = false;
    state.notices.clear();
    state.config.dock_mode = DockMode::Floating;
    state.config.stroke_enabled = false;
    for (name, look, cry, time) in [
        ("eyes-idle", [0.0, 0.0], false, 1000),
        ("eyes-look-left", [-1.0, 0.5], false, 1000),
        ("eyes-look-down-right", [1.0, 1.0], false, 1000),
        ("eyes-look-up-left", [-1.0, -1.0], false, 1000),
        ("eyes-crying", [0.2, 0.0], true, 1000),
        ("eyes-blink", [0.0, 0.0], false, 1880),
    ] {
        state.face = crate::face::Face::new(0);
        for tick in (33..=time).step_by(33) {
            state.face.advance(tick, look, cry);
        }
        let mut canvas = Canvas::new(COMPACT_WIDTH + 2 * HALO, COMPACT_HEIGHT + 2 * HALO, 2.0)?;
        draw(&mut canvas, &state, time, &LocalTime::get());
        canvas.finish()?;
        canvas.mask(29, DockMode::Floating);
        canvas
            .save_bmp(&folder.join(format!("{name}.bmp")))
            .map_err(io_error)?;
    }
    // Bare border at the reference proportions, plus the actual compact scale.
    for (name, width, height, scale) in [
        ("border-reference", 308, 58, 2.5),
        ("border-compact", 336, 58, 1.0),
    ] {
        let c = Canvas::new(width + 2 * HALO, height + 2 * HALO, scale)?;
        c.begin(HALO, HALO);
        c.gradient(0, 0, width, height, 0, 0x06060a, 0x08080d);
        c.set_border(true, 0xa69bff, 3, 100);
        c.finish()?;
        c.mask(height / 2, DockMode::Floating);
        c.save_bmp(&folder.join(format!("{name}.bmp")))
            .map_err(io_error)?;
    }
    Ok(())
}
fn io_error(e: std::io::Error) -> windows::core::Error {
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
}
