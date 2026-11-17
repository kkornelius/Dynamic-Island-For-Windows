//! Small rounded audio bars, with the shared live-signal clock retained for every menu.
use super::levels::BARS;
pub const FRAME_MS: u64 = 50;
pub const BAR_COUNT: usize = 6;
const STALE_MS: u64 = 450;
pub fn active(history: &[f32; BARS], sample_at: u64, now: u64, enabled: bool) -> bool {
    enabled
        && sample_at > 0
        && now.saturating_sub(sample_at) < STALE_MS
        && history.iter().any(|v| v.is_finite() && *v > 0.003)
}
pub fn segments(
    history: &[f32; BARS],
    sample_at: u64,
    now: u64,
    enabled: bool,
    rect: [i32; 4],
) -> [[(f32, f32); 2]; BAR_COUNT] {
    let [x, y, w, h] = rect;
    let center = y as f32 + h as f32 / 2.0;
    let live = active(history, sample_at, now, enabled);
    let fraction = (now.saturating_sub(sample_at) as f32 / FRAME_MS as f32).min(1.0);
    let fade =
        (1.0 - now.saturating_sub(sample_at).saturating_sub(150) as f32 / 300.0).clamp(0.0, 1.0);
    std::array::from_fn(|i| {
        let left = BARS - 2 - i * 3;
        let value = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let level =
            value(history[left]) + (value(history[left + 1]) - value(history[left])) * fraction;
        let height = if live {
            level * (h as f32 - 7.0).max(0.0) * fade
        } else {
            0.0
        };
        let at = x as f32 + (i as f32 + 0.5) * w as f32 / BAR_COUNT as f32;
        [
            (at, center - height.max(0.05) / 2.0),
            (at, center + height.max(0.05) / 2.0),
        ]
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_disabled_or_stale_input_returns_small_dots() {
        for (history, stamp, enabled) in [
            ([0.0; BARS], 5000, true),
            ([0.8; BARS], 5000, false),
            ([0.8; BARS], 4000, true),
        ] {
            assert!(
                segments(&history, stamp, 5000, enabled, [0, 0, 30, 20])
                    .iter()
                    .all(|p| p[1].1 - p[0].1 < 0.1)
            );
        }
    }
    #[test]
    fn bars_interpolate_between_measured_packets_and_follow_loudness() {
        let mut history = [0.1; BARS];
        history[BARS - 1] = 0.9;
        let a = segments(&history, 5000, 5000, true, [0, 0, 30, 20]);
        let b = segments(&history, 5000, 5025, true, [0, 0, 30, 20]);
        let c = segments(&history, 5000, 5050, true, [0, 0, 30, 20]);
        let height = |p: [(f32, f32); 2]| p[1].1 - p[0].1;
        assert!(height(a[0]) < height(b[0]) && height(b[0]) < height(c[0]));
        assert!((height(b[0]) - (height(a[0]) + height(c[0])) / 2.0).abs() < 0.001);
        assert!(height(c[0]) > height(c[1]));
    }
}
