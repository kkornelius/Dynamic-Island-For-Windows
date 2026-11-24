//! Read-only YouTube player cross-check for stale Chromium SMTC metadata.
//! Reads the window caption and known player controls only, never URLs, search
//! values, comments, page text, cookies, history, audio, or account information.
use super::MediaSnapshot;
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Variant::*},
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub fn video_title(caption: &str) -> Option<String> {
    let mut caption = caption.trim();
    for suffix in [
        " - Brave",
        " - Google Chrome",
        " - Chromium",
        " - Microsoft Edge",
        " — Mozilla Firefox",
        " - Mozilla Firefox",
        " - Opera",
        " - Vivaldi",
    ] {
        if let Some(value) = caption.strip_suffix(suffix) {
            caption = value;
            break;
        }
    }
    caption = caption
        .strip_suffix(" - YouTube")
        .or_else(|| caption.strip_suffix(" - YouTube Music"))?;
    if let Some((prefix, tail)) = caption.strip_prefix('(').and_then(|v| v.split_once(") "))
        && !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_digit())
    {
        caption = tail;
    }
    let title = caption.trim();
    (!title.is_empty()).then(|| title.chars().take(256).collect())
}
fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn correct_identity(media: &mut MediaSnapshot, title: &str) {
    let native_title = normalized(&media.title);
    let matches = !native_title.is_empty() && normalized(title).contains(&native_title);
    if !matches {
        media.title = title.into();
        media.artist = [" - ", " – ", " — "]
            .iter()
            .find_map(|s| title.split_once(s).map(|v| v.0.trim().to_owned()))
            .unwrap_or_default();
        media.album.clear();
        media.artwork = None;
        // Never apply the old advertisement/video's clock to the corrected song.
        media.timeline_available = false;
        media.position_ms = 0;
        media.duration_ms = 0;
    }
}
#[derive(Default)]
pub struct PlayerClock {
    raw: Option<u64>,
    at: u64,
    playing: bool,
}
impl PlayerClock {
    pub fn sample(&mut self, raw: u64, playing: bool, now: u64) -> Option<u64> {
        if self.raw != Some(raw) || self.playing != playing {
            self.raw = Some(raw);
            self.at = now;
            self.playing = playing;
        }
        if playing && now.saturating_sub(self.at) > 2500 {
            return None;
        }
        Some(raw.saturating_add(if playing {
            now.saturating_sub(self.at)
        } else {
            0
        }))
    }
}
struct Player {
    window: HWND,
    title: String,
    document: IUIAutomationElement,
    slider: IUIAutomationRangeValuePattern,
    play: Option<IUIAutomationElement>,
    clock: PlayerClock,
}
pub struct Reader {
    automation: IUIAutomation,
    player: Option<Player>,
    next_search: Instant,
    source: String,
}
impl Reader {
    pub fn new() -> Result<Self> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let options: IUIAutomation2 = automation.cast()?;
            options.SetConnectionTimeout(120)?;
            options.SetTransactionTimeout(180)?;
            options.SetAutoSetFocus(false)?;
            Ok(Self {
                automation,
                player: None,
                next_search: Instant::now(),
                source: String::new(),
            })
        }
    }
    fn caption(window: HWND) -> Option<String> {
        unsafe {
            let mut text = [0u16; 1024];
            let n = GetWindowTextW(window, &mut text);
            (n > 0).then(|| String::from_utf16_lossy(&text[..n as usize]))
        }
    }
    fn source_matches(caption: &str, source: &str) -> bool {
        let source = source.to_ascii_lowercase();
        let caption = caption.to_ascii_lowercase();
        [
            ("brave", "brave"),
            ("chrome", "chrome"),
            ("edge", "edge"),
            ("firefox", "firefox"),
            ("opera", "opera"),
            ("vivaldi", "vivaldi"),
            ("chromium", "chromium"),
        ]
        .iter()
        .any(|(s, c)| source.contains(s) && caption.ends_with(c))
    }
    fn window(media: &MediaSnapshot) -> Option<(HWND, String)> {
        unsafe {
            let mut next = GetTopWindow(None).ok();
            let mut first = None;
            for _ in 0..120 {
                let Some(window) = next else {
                    break;
                };
                next = GetWindow(window, GW_HWNDNEXT).ok();
                let Some(caption) = Self::caption(window) else {
                    continue;
                };
                if !Self::source_matches(&caption, &media.source) {
                    continue;
                }
                let Some(title) = video_title(&caption) else {
                    continue;
                };
                if first.is_none() {
                    first = Some((window, title.clone()));
                }
                if !media.title.is_empty() && normalized(&title).contains(&normalized(&media.title))
                {
                    return Some((window, title));
                }
            }
            first
        }
    }
    fn property(&self, id: UIA_PROPERTY_ID, value: &VARIANT) -> Result<IUIAutomationCondition> {
        unsafe { self.automation.CreatePropertyCondition(id, value) }
    }
    fn player(&self, window: HWND, title: String) -> Result<Player> {
        unsafe {
            let root = self.automation.ElementFromHandle(window)?;
            let condition = self.property(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_DocumentControlTypeId.0),
            )?;
            let doc = root.FindFirst(TreeScope_Descendants, &condition)?;
            // Match the currently selected YouTube document to its browser caption.
            if !normalized(&doc.CurrentName()?.to_string()).contains(&normalized(&title)) {
                return Err(Error::empty());
            }
            let slider_kind = self.property(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_SliderControlTypeId.0),
            )?;
            let sliders = doc.FindAll(TreeScope_Descendants, &slider_kind)?;
            let mut slider = None;
            for index in 0..sliders.Length()?.min(12) {
                let element = sliders.GetElement(index)?;
                let name = element.CurrentName()?.to_string().to_lowercase();
                if !(name.contains("seek")
                    || name.contains("penggeser")
                    || name.contains("progress")
                    || name.contains("position")
                    || name.contains("pencarian"))
                    || name.contains("volume")
                {
                    continue;
                }
                let Ok(pattern) = element
                    .GetCurrentPattern(UIA_RangeValuePatternId)
                    .and_then(|p| p.cast::<IUIAutomationRangeValuePattern>())
                else {
                    continue;
                };
                let maximum = pattern.CurrentMaximum()?;
                let value = pattern.CurrentValue()?;
                if maximum.is_finite()
                    && value.is_finite()
                    && maximum > 0.0
                    && maximum < 86400.0
                    && value >= 0.0
                    && value <= maximum
                {
                    slider = Some(pattern);
                    break;
                }
            }
            let slider = slider.ok_or_else(Error::empty)?;
            let button_kind = self.property(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_ButtonControlTypeId.0),
            )?;
            let mut names = self.property(UIA_NamePropertyId, &VARIANT::from("Pause (k)"))?;
            for name in [
                "Play (k)",
                "Jeda (k)",
                "Putar (k)",
                "Putar ulang (k)",
                "Replay (k)",
            ] {
                let name = self.property(UIA_NamePropertyId, &VARIANT::from(name))?;
                names = self.automation.CreateOrCondition(&names, &name)?;
            }
            let controls = self.automation.CreateAndCondition(&button_kind, &names)?;
            let play = doc.FindFirst(TreeScope_Descendants, &controls).ok();
            Ok(Player {
                window,
                title,
                document: doc,
                slider,
                play,
                clock: PlayerClock::default(),
            })
        }
    }
    pub fn refresh(&mut self, media: &mut MediaSnapshot) {
        if !media.browser() {
            self.player = None;
            return;
        }
        if self.source != media.source {
            self.player = None;
            self.source = media.source.clone();
        }
        let visible = Self::window(media);
        // A selected non-music tab must not discard the established YouTube
        // player. Its cached accessibility controls can still supply position.
        let cached = self.player.as_ref().and_then(|p| unsafe {
            if !IsWindow(Some(p.window)).as_bool() {
                return None;
            }
            let title = video_title(&p.document.CurrentName().ok()?.to_string())?;
            Some((p.window, title))
        });
        let background = visible.is_none() && cached.is_some();
        let Some((window, title)) = visible.or(cached) else {
            self.player = None;
            return;
        };
        correct_identity(media, &title);
        if background
            && let Some(player) = &mut self.player
            && player.title != title
        {
            player.title = title.clone();
            player.clock = PlayerClock::default();
        }
        let changed = self
            .player
            .as_ref()
            .is_none_or(|p| p.window != window || p.title != title);
        if changed {
            self.player = None;
            if Instant::now() < self.next_search {
                return;
            }
            self.next_search = Instant::now() + Duration::from_millis(900);
            self.player = self.player(window, title).ok();
        }
        let Some(player) = &mut self.player else {
            return;
        };
        let value = (|| -> Result<(f64, f64, Option<bool>)> {
            unsafe {
                let value = player.slider.CurrentValue()?;
                let maximum = player.slider.CurrentMaximum()?;
                let playing = player
                    .play
                    .as_ref()
                    .and_then(|e| e.CurrentName().ok())
                    .and_then(|s| {
                        let s = s.to_string().to_lowercase();
                        if s.starts_with("pause") || s.starts_with("jeda") {
                            Some(true)
                        } else if s.starts_with("play")
                            || s.starts_with("putar")
                            || s.starts_with("replay")
                        {
                            Some(false)
                        } else {
                            None
                        }
                    });
                Ok((value, maximum, playing))
            }
        })();
        let Ok((position, duration, playing)) = value else {
            self.player = None;
            return;
        };
        if !position.is_finite()
            || !duration.is_finite()
            || position < 0.0
            || position > duration
            || duration <= 0.0
            || duration > 86400.0
        {
            self.player = None;
            return;
        }
        let now = crate::platform::now_ms();
        media.playing = playing.unwrap_or(media.playing);
        media.duration_ms = (duration * 1000.0).round() as u64;
        media.sampled_at_ms = now;
        if !media.playback_rate.is_finite() || media.playback_rate <= 0.0 {
            media.playback_rate = 1.0;
        }
        if let Some(position) =
            player
                .clock
                .sample((position * 1000.0).round() as u64, media.playing, now)
        {
            media.position_ms = position.min(media.duration_ms);
            media.timeline_available = true;
        } else {
            media.timeline_available = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_tracks_the_actual_youtube_tab_instead_of_a_blocked_ad() {
        let title =
            video_title("(5) Bernadya - Terlintas (Official Music Video) - YouTube - Brave")
                .unwrap();
        let mut media = MediaSnapshot {
            title: "Unrelated advertisement".into(),
            artist: "Advertisement uploader".into(),
            timeline_available: true,
            position_ms: 95_000,
            duration_ms: 245_000,
            ..Default::default()
        };
        correct_identity(&mut media, &title);
        assert_eq!(media.title, title);
        assert_eq!(media.artist, "Bernadya");
        assert_eq!(media.position_ms, 0);
        assert!(!media.timeline_available);
        assert!(video_title("Some text - Brave").is_none());
    }
    #[test]
    fn player_clock_reanchors_after_seek_and_freezes_on_pause() {
        let mut c = PlayerClock::default();
        assert_eq!(c.sample(24_000, true, 0), Some(24_000));
        assert_eq!(c.sample(24_000, true, 300), Some(24_300));
        assert_eq!(c.sample(100_000, true, 500), Some(100_000));
        assert_eq!(c.sample(100_000, false, 700), Some(100_000));
        assert_eq!(c.sample(100_000, false, 5000), Some(100_000));
        assert_eq!(c.sample(10_000, true, 5100), Some(10_000));
        assert_eq!(c.sample(10_000, true, 8000), None);
    }
}
