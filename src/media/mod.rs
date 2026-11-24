pub mod browser;
pub mod levels;
pub mod lyrics;
pub mod pulse;
pub mod sessions;
pub mod visualizer;

#[derive(Clone)]
pub struct Artwork {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}
#[derive(Clone, Default)]
pub struct MediaSnapshot {
    pub available: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source: String,
    pub playing: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub sampled_at_ms: u64,
    pub timeline_available: bool,
    pub playback_rate: f64,
    pub artwork: Option<std::sync::Arc<Artwork>>,
}
impl MediaSnapshot {
    pub fn position(&self, now: u64) -> u64 {
        let elapsed = if self.playing && self.timeline_available {
            (now.saturating_sub(self.sampled_at_ms) as f64 * self.playback_rate) as u64
        } else {
            0
        };
        let position = self.position_ms.saturating_add(elapsed);
        if self.duration_ms > 0 {
            position.min(self.duration_ms)
        } else {
            position
        }
    }
    pub fn spotify(&self) -> bool {
        self.source.eq_ignore_ascii_case("Spotify")
    }
    pub fn browser(&self) -> bool {
        let source = self.source.to_ascii_lowercase();
        [
            "chrome",
            "brave",
            "edge",
            "firefox",
            "opera",
            "vivaldi",
            "youtube",
            "thorium",
            "librewolf",
            "waterfox",
            "zen",
        ]
        .iter()
        .any(|name| source.contains(name))
    }
}
#[derive(Clone, Copy)]
pub enum Playback {
    Toggle,
    Next,
    Previous,
}
