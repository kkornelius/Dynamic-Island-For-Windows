//! Licensed Icons8 bitmaps, decoded once per render target.
use super::{canvas::Canvas, icons::Icon};
pub fn draw(c: &Canvas, icon: Icon, x: i32, y: i32, size: i32, color: u32) -> bool {
    let dark = color < 0x505050;
    let bytes: &[u8] = match (icon, dark) {
        (Icon::Music, false) => include_bytes!("../../assets/icons8/music-FFFFFF.png"),
        (Icon::Music, true) => include_bytes!("../../assets/icons8/music-000000.png"),
        (Icon::Weather, false) => include_bytes!("../../assets/icons8/weather-FFFFFF.png"),
        (Icon::Weather, true) => include_bytes!("../../assets/icons8/weather-000000.png"),
        (Icon::Timer, false) => include_bytes!("../../assets/icons8/timer-FFFFFF.png"),
        (Icon::Timer, true) => include_bytes!("../../assets/icons8/timer-000000.png"),
        (Icon::Water, false) => include_bytes!("../../assets/icons8/water-FFFFFF.png"),
        (Icon::Water, true) => include_bytes!("../../assets/icons8/water-000000.png"),
        (Icon::Notes, false) => include_bytes!("../../assets/icons8/notes-FFFFFF.png"),
        (Icon::Notes, true) => include_bytes!("../../assets/icons8/notes-000000.png"),
        (Icon::Settings, false) => include_bytes!("../../assets/icons8/settings-FFFFFF.png"),
        (Icon::Settings, true) => include_bytes!("../../assets/icons8/settings-000000.png"),
        (Icon::Lyrics, false) => include_bytes!("../../assets/icons8/lyrics-FFFFFF.png"),
        (Icon::Lyrics, true) => include_bytes!("../../assets/icons8/lyrics-000000.png"),
        (Icon::Play, false) => include_bytes!("../../assets/icons8/play-FFFFFF.png"),
        (Icon::Play, true) => include_bytes!("../../assets/icons8/play-000000.png"),
        (Icon::Pause, false) => include_bytes!("../../assets/icons8/pause-FFFFFF.png"),
        (Icon::Pause, true) => include_bytes!("../../assets/icons8/pause-000000.png"),
        (Icon::Previous, false) => include_bytes!("../../assets/icons8/previous-FFFFFF.png"),
        (Icon::Previous, true) => include_bytes!("../../assets/icons8/previous-000000.png"),
        (Icon::Next, false) => include_bytes!("../../assets/icons8/next-FFFFFF.png"),
        (Icon::Next, true) => include_bytes!("../../assets/icons8/next-000000.png"),
        (Icon::Earbuds, false) => include_bytes!("../../assets/icons8/earbuds-FFFFFF.png"),
        (Icon::Earbuds, true) => include_bytes!("../../assets/icons8/earbuds-000000.png"),
        (Icon::Keyboard, false) => include_bytes!("../../assets/icons8/keyboard-FFFFFF.png"),
        (Icon::Keyboard, true) => include_bytes!("../../assets/icons8/keyboard-000000.png"),
        (Icon::Mouse, false) => include_bytes!("../../assets/icons8/mouse-FFFFFF.png"),
        (Icon::Mouse, true) => include_bytes!("../../assets/icons8/mouse-000000.png"),
        (Icon::Speaker, false) => include_bytes!("../../assets/icons8/speaker-FFFFFF.png"),
        (Icon::Speaker, true) => include_bytes!("../../assets/icons8/speaker-000000.png"),
        (Icon::Phone, false) => include_bytes!("../../assets/icons8/phone-FFFFFF.png"),
        (Icon::Phone, true) => include_bytes!("../../assets/icons8/phone-000000.png"),
        (Icon::Controller, false) => include_bytes!("../../assets/icons8/controller-FFFFFF.png"),
        (Icon::Controller, true) => include_bytes!("../../assets/icons8/controller-000000.png"),
        (Icon::Bluetooth, false) => include_bytes!("../../assets/icons8/bluetooth-FFFFFF.png"),
        (Icon::Bluetooth, true) => include_bytes!("../../assets/icons8/bluetooth-000000.png"),
        (Icon::Close, false) => include_bytes!("../../assets/icons8/close-FFFFFF.png"),
        (Icon::Close, true) => include_bytes!("../../assets/icons8/close-000000.png"),
        (Icon::Moon, false) => include_bytes!("../../assets/icons8/moon-FFFFFF.png"),
        (Icon::Moon, true) => include_bytes!("../../assets/icons8/moon-000000.png"),
    };
    let opacity = if dark {
        1.0
    } else {
        (((color >> 16) & 255)
            .max((color >> 8) & 255)
            .max(color & 255)) as f32
            / 255.0
    };
    c.icon_bitmap(
        icon as u16 * 2 + u16::from(dark),
        bytes,
        [x, y, size, size],
        opacity,
    )
}
