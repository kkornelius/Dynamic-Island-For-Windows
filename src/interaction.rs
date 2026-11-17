//! Hover previews are transient until the user explicitly interacts with a feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Open,
    Close,
}
#[derive(Default)]
pub struct Interaction {
    pub preview: bool,
    entered: Option<u64>,
    left: Option<u64>,
    suppressed: bool,
}
impl Interaction {
    pub fn update(&mut self, now: u64, inside: bool, expanded: bool) -> Option<Transition> {
        if inside {
            self.left = None;
            if !expanded && !self.suppressed {
                let start = *self.entered.get_or_insert(now);
                if now.saturating_sub(start) >= 100 {
                    self.preview = true;
                    self.entered = None;
                    return Some(Transition::Open);
                }
            }
        } else {
            self.entered = None;
            self.suppressed = false;
            if expanded && self.preview {
                let start = *self.left.get_or_insert(now);
                if now.saturating_sub(start) >= 160 {
                    self.preview = false;
                    self.left = None;
                    return Some(Transition::Close);
                }
            } else {
                self.left = None;
            }
        }
        None
    }
    pub fn pin(&mut self) {
        self.preview = false;
        self.entered = None;
        self.left = None;
    }
    pub fn minimize(&mut self) {
        self.pin();
        self.suppressed = true;
    }
}
pub fn contains(rect: [f32; 4], point: [f32; 2], padding: f32) -> bool {
    let [x, y, w, h] = rect;
    point[0] >= x - padding
        && point[0] <= x + w + padding
        && point[1] >= y - padding
        && point[1] <= y + h + padding
}
/// The entire visible capsule body accepts hover and click; glow stays outside the target.
pub fn activation_rect(body: [f32; 4], _scale: f32) -> [f32; 4] {
    body
}
/// Emotion has a wider radius than the opening target and never activates the panel.
pub fn near_capsule(body: [f32; 4], point: [f32; 2], scale: f32) -> bool {
    let dx = (body[0] - point[0])
        .max(point[0] - body[0] - body[2])
        .max(0.0);
    let dy = (body[1] - point[1])
        .max(point[1] - body[1] - body[3])
        .max(0.0);
    dx.hypot(dy) <= 96.0 * scale
}
pub fn pass_through(
    body: [f32; 4],
    point: [f32; 2],
    scale: f32,
    expanded: bool,
    notice: bool,
    alt: bool,
) -> bool {
    alt || (!expanded && !notice && !contains(activation_rect(body, scale), point, 0.0))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_opens_and_leaving_closes_after_grace() {
        let mut h = Interaction::default();
        assert_eq!(h.update(0, true, false), None);
        assert_eq!(h.update(99, true, false), None);
        assert_eq!(h.update(100, true, false), Some(Transition::Open));
        assert_eq!(h.update(120, false, true), None);
        assert_eq!(h.update(200, true, true), None);
        assert_eq!(h.update(300, false, true), None);
        assert_eq!(h.update(460, false, true), Some(Transition::Close));
    }
    #[test]
    fn feature_click_pins_and_minimize_requires_exit_before_reopening() {
        let mut h = Interaction::default();
        h.update(0, true, false);
        h.update(100, true, false);
        h.pin();
        assert_eq!(h.update(1000, false, true), None);
        assert_eq!(h.update(5000, false, true), None);
        h.minimize();
        assert_eq!(h.update(6000, true, false), None);
        assert_eq!(h.update(9000, true, false), None);
        h.update(9100, false, false);
        h.update(9200, true, false);
        assert_eq!(h.update(9300, true, false), Some(Transition::Open));
    }
    #[test]
    fn whole_capsule_is_interactive_and_alt_still_passes_through() {
        let body = [100.0, 20.0, 336.0, 58.0];
        assert!(!pass_through(body, [130.0, 45.0], 1.0, false, false, false));
        assert!(!pass_through(body, [410.0, 45.0], 1.0, false, false, false));
        assert!(!pass_through(body, [268.0, 45.0], 1.0, false, false, false));
        assert!(!pass_through(body, [130.0, 45.0], 1.0, true, false, false));
        assert!(pass_through(body, [268.0, 45.0], 1.0, true, false, true));
        assert!(!pass_through(body, [130.0, 45.0], 1.0, false, true, false));
        assert!(!pass_through(body, [101.0, 49.0], 1.0, false, false, false));
        assert!(!pass_through(body, [435.0, 49.0], 1.0, false, false, false));
        assert!(!pass_through(body, [268.0, 21.0], 1.0, false, false, false));
        assert!(!pass_through(body, [268.0, 77.0], 1.0, false, false, false));
        assert!(pass_through(body, [90.0, 49.0], 1.0, false, false, false));
    }
    #[test]
    fn crying_starts_outside_capsule_independently_of_activation_zone() {
        let body = [100.0, 20.0, 336.0, 58.0];
        let point = [268.0, 158.0];
        assert!(near_capsule(body, point, 1.0));
        assert!(!contains(activation_rect(body, 1.0), point, 0.0));
        assert!(near_capsule(body, [532.0, 49.0], 1.0));
        assert!(!near_capsule(body, [533.0, 49.0], 1.0));
        assert!(!near_capsule(body, [516.0, 158.0], 1.0));
        assert!(near_capsule(
            [200.0, 40.0, 672.0, 116.0],
            [536.0, 316.0],
            2.0
        ));
    }
}
