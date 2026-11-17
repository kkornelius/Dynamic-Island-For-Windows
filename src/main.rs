#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod bluetooth;
mod browser_space;
mod color;
mod event;
mod face;
mod fullscreen;
mod interaction;
mod media;
mod network;
mod notes;
mod platform;
mod reminders;
mod sound;
mod startup;
mod storage;
mod timer;
mod tray;
mod ui;
mod visibility;
mod weather;
mod window;

fn main() -> windows::core::Result<()> {
    let result = run();
    ui::release_graphics();
    result
}
fn run() -> windows::core::Result<()> {
    if std::env::args().any(|a| a == "--licenses") {
        print!("{}", include_str!("../assets/licenses/NOTICES.txt"));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--render-demo") {
        return ui::render_demo();
    }
    if let Some(value) = platform::argument("--startup") {
        return startup::cli(&value);
    }
    if std::env::args().any(|s| s == "--verify-color-reuse") {
        return ui::verify_color_reuse();
    }
    if std::env::args().any(|s| s == "--verify-partial-render") {
        return ui::verify_partial();
    }
    if std::env::args().any(|a| a == "--probe-browser-space") {
        return browser_space::probe();
    }
    if std::env::args().any(|a| a == "--verify-canvas-reuse") {
        return ui::verify_reuse();
    }
    if std::env::args().any(|a| a == "--probe-reported-lyrics") {
        return media::lyrics::probe_reported_track();
    }
    if std::env::args().any(|a| a == "--probe-go-easy-lyrics") {
        return media::lyrics::probe_go_easy_track();
    }
    if std::env::args().any(|a| a == "--probe-weather") {
        return weather::probe();
    }
    if std::env::args().any(|a| a == "--probe-public-lyrics") {
        return media::lyrics::probe_public_example();
    }
    if std::env::args().any(|a| a == "--render-design") {
        return ui::render_design();
    }
    if std::env::args().any(|a| a == "--probe-lyrics") {
        return media::lyrics::probe();
    }
    if std::env::args().any(|a| a == "--probe-window") {
        return platform::probe_window();
    }
    if std::env::args().any(|a| a == "--test-media-controls") {
        return media::sessions::probe_controls();
    }
    if std::env::args().any(|a| a == "--probe") {
        return platform::probe();
    }
    window::run()
}
