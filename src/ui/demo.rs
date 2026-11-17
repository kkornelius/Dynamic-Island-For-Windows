//! Reproducible native-renderer media using isolated, original sample data.
use super::*;
use std::path::Path;
use windows::{
    Win32::Foundation::{E_FAIL, E_INVALIDARG},
    core::{Error, Result},
};

pub fn render_demo() -> Result<()> {
    if crate::platform::argument("--data-dir").is_none() {
        return Err(Error::new(
            E_INVALIDARG,
            "Demo rendering requires an isolated --data-dir",
        ));
    }
    let output = crate::platform::argument("--demo-output")
        .unwrap_or_else(|| "artifacts/public-demo/frames".into());
    let output = Path::new(&output);
    std::fs::create_dir_all(output.join("screenshots")).map_err(io_error)?;
    let (mut s, _) = preview::fixture_state();
    let now = crate::platform::now_ms();
    s.config = crate::storage::Config {
        opacity: 100,
        stroke_enabled: true,
        stroke_color: 0xe6e6e6,
        stroke_width: 2,
        stroke_glow: 70,
        ..Default::default()
    };
    s.config.water.enabled = true;
    s.media.title = "An evening, unhurried".into();
    s.media.artist = "Demo artist".into();
    s.media.album = "Sample session".into();
    s.media.position_ms = 0;
    s.media.sampled_at_ms = now;
    if let Some(f) = &mut s.weather.forecast {
        f.fetched = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
    }
    if let LyricsStatus::Ready(ref mut doc) = s.lyrics.status {
        for (i, line) in doc.lines.iter_mut().enumerate() {
            line.at_ms = i as u64 * 1500;
        }
    }
    s.note.editor = Default::default();
    s.note.saved = 0;
    s.note.error = false;
    s.startup = crate::startup::Status {
        registered: true,
        ..Default::default()
    };
    s.status.clear();
    s.water.next_due = now + 45 * 60_000;
    s.water.count = 3;
    s.timer.set_seconds(25 * 60);
    let local = LocalTime {
        hour: 9,
        minute: 41,
        second: 0,
        milliseconds: 0,
        year: 2026,
        month: 11,
        day: 18,
        weekday: 3,
    };
    let scenes = [
        ("idle", 26),
        ("compact", 26),
        ("media", 28),
        ("lyrics", 36),
        ("weather", 32),
        ("timer", 26),
        ("water", 28),
        ("notes", 40),
        ("settings", 28),
        ("idle", 10),
    ];
    let mut canvas = Canvas::with_capacity(
        COMPACT_WIDTH + 2 * HALO,
        COMPACT_HEIGHT + 2 * HALO,
        MEDIA_WIDTH + 2 * HALO,
        HEIGHT + 2 * HALO,
        1.,
    )?;
    let mut previous = (COMPACT_WIDTH, COMPACT_HEIGHT);
    let mut frames = Vec::new();
    let mut index = 0u64;
    for (scene, count) in scenes {
        s.expanded = !matches!(scene, "idle" | "compact");
        s.tab = match scene {
            "weather" => Tab::Weather,
            "timer" => Tab::Timer,
            "water" => Tab::Water,
            "notes" => Tab::Notes,
            "settings" => Tab::Settings,
            _ => Tab::Media,
        };
        s.show_lyrics = scene == "lyrics";
        s.media.playing = scene != "idle";
        s.settings_page = 0;
        s.editing = if scene == "notes" {
            Some(EditField::Note)
        } else {
            None
        };
        if scene == "timer" {
            s.timer.start(now + index * 50);
        } else {
            s.timer.reset();
        }
        let target = if s.expanded {
            (expanded_width(&s), expanded_height(&s))
        } else {
            (COMPACT_WIDTH, COMPACT_HEIGHT)
        };
        for n in 0..count {
            let time = now + index * 50;
            let t = (n as f32 * 0.05 / 0.12).min(1.);
            let ease = 1. - (1. - t).powi(3);
            let width = previous.0 + ((target.0 - previous.0) as f32 * ease) as i32;
            let height = previous.1 + ((target.1 - previous.1) as f32 * ease) as i32;
            if !canvas.resize(width + 2 * HALO, height + 2 * HALO) {
                return Err(Error::new(
                    E_INVALIDARG,
                    "Demo surface exceeded fixed capacity",
                ));
            }
            let phase = index as f32 * 0.19;
            s.bars = std::array::from_fn(|i| 0.1 + 0.75 * (phase + i as f32 * 0.65).sin().abs());
            s.audio_at_ms = time;
            s.face.advance(
                time,
                [(phase * 0.25).sin() * 0.7, (phase * 0.2).cos() * 0.45],
                scene == "idle" && n > 14 && count > 10,
            );
            if scene == "notes" {
                let note = "Ideas for tomorrow\n\nShip one small thing.\nTake a short walk.\nLeave room for a break.";
                let text: String = note.chars().take((n * 4) as usize).collect();
                if s.note.editor.text != text {
                    s.note.editor.select_all();
                    s.note.editor.insert(&text);
                    s.note.caret_at = time;
                }
                if n > 25 {
                    s.note.saved = s.note.editor.revision;
                }
            }
            draw(&mut canvas, &s, time, &local);
            canvas.finish()?;
            canvas.mask(if s.expanded { 28 } else { 29 }, DockMode::Floating);
            let filename = format!("{index:04}.bmp");
            canvas.save_bmp(&output.join(&filename)).map_err(io_error)?;
            frames.push(serde_json::json!({"file":filename,"scene":scene,"width":canvas.width,"height":canvas.height}));
            if n == count - 1 {
                canvas
                    .save_bmp(&output.join("screenshots").join(format!("{scene}.bmp")))
                    .map_err(io_error)?;
            }
            index += 1;
        }
        previous = target;
    }
    let manifest = serde_json::json!({"fps":20,"frames":frames,"provenance":"Actual Direct2D app renderer; fictional offline media, lyrics, weather and notes; not a desktop recording."});
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(io_error)?;
    Ok(())
}
fn io_error(e: std::io::Error) -> Error {
    Error::new(E_FAIL, e.to_string())
}
