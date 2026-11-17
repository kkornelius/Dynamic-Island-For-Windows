//! Lightweight cursor-following expression and blink timing; no textures or input hooks.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub look: [f32; 2],
    pub crying: f32,
    pub openness: f32,
    pub tear_phase: f32,
}
pub struct Face {
    look: [f32; 2],
    crying: f32,
    started: u64,
    updated: u64,
}
impl Face {
    pub fn new(now: u64) -> Self {
        Self {
            look: [0.0; 2],
            crying: 0.0,
            started: now,
            updated: now,
        }
    }
    pub fn advance(&mut self, now: u64, target: [f32; 2], near: bool) -> bool {
        let before = self.pose(self.updated);
        let dt = now.saturating_sub(self.updated).min(100) as f32;
        let a = 1.0 - (-dt / 90.0).exp();
        for (current, target) in self.look.iter_mut().zip(target) {
            *current += (target.clamp(-1.0, 1.0) - *current) * a;
        }
        // Reach exact endpoints and repaint the final zero frame; no exponential tail.
        self.crying = if near {
            (self.crying + dt / 80.0).min(1.0)
        } else {
            (self.crying - dt / 96.0).max(0.0)
        };
        self.updated = now;
        let after = self.pose(now);
        after.crying > 0.0
            || before.crying > 0.0
            || (after.openness - before.openness).abs() > 0.005
            || after
                .look
                .iter()
                .zip(before.look)
                .any(|(a, b)| (*a - b).abs() > 0.005)
    }
    pub fn clear_tears(&mut self) {
        self.crying = 0.0;
    }
    pub fn pose(&self, now: u64) -> Pose {
        let phase = (now.saturating_sub(self.started) + 3000) % 4800;
        let smooth = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let openness = if phase < 70 {
            1.0 - smooth(phase as f32 / 70.0)
        } else if phase < 105 {
            0.0
        } else if phase < 230 {
            smooth((phase - 105) as f32 / 125.0)
        } else {
            1.0
        };
        Pose {
            look: self.look,
            crying: self.crying,
            openness,
            tear_phase: (now % 900) as f32 / 900.0,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eyes_follow_smoothly_and_react_to_proximity() {
        let mut f = Face::new(0);
        f.advance(33, [5.0, -5.0], true);
        let a = f.pose(33);
        assert!(a.look[0] > 0.0 && a.look[0] < 1.0);
        assert!(a.look[1] < 0.0 && a.look[1] > -1.0);
        for t in (66..1000).step_by(33) {
            f.advance(t, [1.0, -1.0], true);
        }
        assert!(f.pose(1000).crying > 0.99);
        for t in (1000..2000).step_by(33) {
            f.advance(t, [0.0, 0.0], false);
        }
        assert!(f.pose(2000).crying < 0.01);
    }
    #[test]
    fn blinking_closes_and_reopens() {
        let f = Face::new(0);
        assert_eq!(f.pose(1500).openness, 1.0);
        assert_eq!(f.pose(1880).openness, 0.0);
        assert_eq!(f.pose(2050).openness, 1.0);
    }
    #[test]
    fn leaving_proximity_reaches_zero_and_paints_the_cleanup_frame() {
        let mut f = Face::new(0);
        assert!(f.advance(100, [0.0, 0.0], true));
        assert_eq!(f.pose(100).crying, 1.0);
        assert!(f.advance(148, [0.0, 0.0], false));
        assert!(f.pose(148).crying > 0.0);
        assert!(f.advance(196, [0.0, 0.0], false));
        assert_eq!(f.pose(196).crying, 0.0);
        assert!(!f.advance(212, [0.0, 0.0], false));
    }
}
