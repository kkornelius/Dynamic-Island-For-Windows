//! Continuous opacity transitions, independent of logical position and tab detection.
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    from: f32,
    to: f32,
    start: u64,
    duration: u64,
}
impl Default for Fade {
    fn default() -> Self {
        Self {
            from: 1.0,
            to: 1.0,
            start: 0,
            duration: 0,
        }
    }
}
impl Fade {
    pub fn value(&self, now: u64) -> f32 {
        if self.duration == 0 {
            return self.to;
        }
        let t = (now.saturating_sub(self.start) as f32 / self.duration as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t * t * (3.0 - 2.0 * t)
    }
    pub fn active(&self, now: u64) -> bool {
        self.duration != 0 && now.saturating_sub(self.start) < self.duration
    }
    pub fn snap(&mut self, show: bool) {
        self.to = if show { 1.0 } else { 0.0 };
        self.from = self.to;
        self.duration = 0;
    }
    pub fn retarget(&mut self, show: bool, now: u64) {
        let to = if show { 1.0 } else { 0.0 };
        if to == self.to {
            return;
        }
        let from = self.value(now);
        let distance = (to - from).abs();
        self.from = from;
        self.to = to;
        self.start = now;
        self.duration = if distance < 0.001 {
            0
        } else {
            ((if show { 200.0 } else { 160.0 }) * distance)
                .round()
                .max(40.0) as u64
        };
    }
    pub fn alpha(&self, now: u64, opacity: u8) -> u8 {
        (self.value(now).clamp(0.0, 1.0) * opacity.min(100) as f32 * 2.55).round() as u8
    }
}
#[derive(Default)]
pub struct MissingTarget {
    since: Option<u64>,
}
impl MissingTarget {
    pub fn update(&mut self, missing: bool, now: u64) -> bool {
        if !missing {
            self.since = None;
            return false;
        }
        now.saturating_sub(*self.since.get_or_insert(now)) >= 450
    }
    pub fn pending(&self) -> bool {
        self.since.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opacity_is_continuous_monotonic_and_reaches_exact_endpoints() {
        let mut fade = Fade::default();
        fade.retarget(false, 1000);
        let samples: Vec<_> = (0..=10).map(|i| fade.alpha(1000 + i * 16, 100)).collect();
        assert_eq!(samples[0], 255);
        assert_eq!(*samples.last().unwrap(), 0);
        assert!(samples.windows(2).all(|p| p[0] >= p[1]));
        assert!(samples.iter().any(|a| *a > 0 && *a < 255));
        fade.retarget(true, 1200);
        assert_eq!(fade.alpha(1200, 80), 0);
        assert_eq!(fade.alpha(1400, 80), 204);
    }
    #[test]
    fn reversing_a_transition_keeps_its_current_alpha() {
        let mut fade = Fade::default();
        fade.retarget(false, 0);
        let halfway = fade.alpha(80, 100);
        fade.retarget(true, 80);
        assert_eq!(fade.alpha(80, 100), halfway);
        assert!(fade.active(80));
        assert_eq!(fade.alpha(300, 100), 255);
        fade.snap(false);
        assert!(!fade.active(300));
        assert_eq!(fade.alpha(300, 100), 0);
    }
    #[test]
    fn a_transient_missing_browser_does_not_release_suppression() {
        let mut target = MissingTarget::default();
        assert!(!target.update(true, 0));
        assert!(!target.update(true, 400));
        assert!(!target.update(false, 410));
        assert!(!target.pending());
        assert!(!target.update(true, 600));
        assert!(target.update(true, 1050));
    }
}
