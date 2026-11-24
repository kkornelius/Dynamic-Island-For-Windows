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
        day: 24,
        weekday: 2,
    };
    let scenes = [
        ("idle", 32),
        ("compact", 32),
        ("media", 35),
        ("lyrics", 45),
        ("weather", 40),
        ("timer", 32),
        ("water", 35),
        ("notes", 50),
        ("settings", 35),
        ("anchored", 20),
        ("floating", 20),
        ("idle", 16),
    ];
    let mut canvas = Canvas::with_capacity(
        COMPACT_WIDTH + 2 * HALO,
        COMPACT_HEIGHT + 2 * HALO,
        MEDIA_WIDTH + 24 + 2 * HALO,
        HEIGHT + 24 + 2 * HALO,
        1.,
    )?;
    let mut previous = (COMPACT_WIDTH, COMPACT_HEIGHT);
    let mut previous_mode = DockMode::Floating;
    let mut frames = Vec::new();
    let mut index = 0u64;
    for (scene, count) in scenes {
        let scene_start = now + index * 40;
        let mode = if scene == "anchored" {
            DockMode::Anchored
        } else {
            DockMode::Floating
        };
        s.config.dock_mode = mode;
        s.expanded = !matches!(scene, "idle" | "compact");
        s.tab = match scene {
            "weather" => Tab::Weather,
            "timer" => Tab::Timer,
            "water" => Tab::Water,
            "notes" => Tab::Notes,
            "settings" | "anchored" | "floating" => Tab::Settings,
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
            s.timer.set_seconds(150);
        } else {
            s.timer.reset();
        }
        let mut target = if s.expanded {
            (expanded_width(&s), expanded_height(&s))
        } else {
            (COMPACT_WIDTH, COMPACT_HEIGHT)
        };
        target.0 += 2 * gutter(&s);
        let dock = (previous_mode != mode).then(|| {
            let pose = |size: (i32, i32), mode| crate::motion::DockPose {
                anchor: [0., if mode == DockMode::Floating { 28. } else { 0. }],
                size: [size.0 as f32, size.1 as f32],
                attached: if mode == DockMode::Anchored { 1. } else { 0. },
            };
            crate::motion::Dock::new(
                scene_start,
                pose(previous, previous_mode),
                pose(target, mode),
                None,
            )
        });
        let spring = (dock.is_none() && previous != target).then(|| {
            crate::motion::Spring::new(
                scene_start,
                previous,
                target,
                None,
                if !s.expanded {
                    crate::motion::Kind::Collapse
                } else {
                    crate::motion::Kind::Resize
                },
            )
        });
        for n in 0..count {
            let time = now + index * 40;
            let size = dock
                .map(|d| {
                    let p = d.pose(time);
                    (p.size[0].round() as i32, p.size[1].round() as i32)
                })
                .or_else(|| spring.map(|s| s.size(time).0))
                .unwrap_or(target);
            let attached = dock.map(|d| d.pose(time).attached);
            let top = attached
                .map(|q| (HALO as f32 * (1.0 - q)).round() as i32)
                .unwrap_or_else(|| top_inset(&s));
            canvas.set_dock_mix(attached);
            if !canvas.resize(size.0 + 2 * HALO, size.1 + top + HALO) {
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
            if scene == "timer" {
                s.editing = if n < 6 {
                    Some(EditField::Timer(crate::timer::Part::Minutes))
                } else if n < 13 {
                    Some(EditField::Timer(crate::timer::Part::Seconds))
                } else {
                    None
                };
                s.edit_text = if n < 6 { "2" } else { "30" }.into();
                s.edit_select_all = false;
                if n == 16 {
                    s.timer.start(time);
                }
            }
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
            if let Some(visual) = spring.and_then(|spring| spring.collapse_visual(time)) {
                draw_collapsing(&mut canvas, &s, time, &local, size, visual);
            } else {
                draw(&mut canvas, &s, time, &local);
            }
            canvas.finish()?;
            canvas.mask(if s.expanded { 28 } else { 29 }, mode);
            let filename = format!("{index:04}.bmp");
            canvas.save_bmp(&output.join(&filename)).map_err(io_error)?;
            frames.push(serde_json::json!({"file":filename,"scene":scene,"width":canvas.width,"height":canvas.height}));
            if n == if scene == "timer" { 14 } else { count - 1 }
                && !matches!(scene, "anchored" | "floating")
            {
                canvas
                    .save_bmp(&output.join("screenshots").join(format!("{scene}.bmp")))
                    .map_err(io_error)?;
            }
            index += 1;
        }
        previous = target;
        previous_mode = mode;
    }
    let manifest = serde_json::json!({"fps":25,"frames":frames,"provenance":"Actual Direct2D app renderer and runtime spring/dock motion; fictional offline media, lyrics, weather and notes; not a desktop recording."});
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
