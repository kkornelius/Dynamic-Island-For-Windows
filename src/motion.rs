//! Analytic damped springs: elapsed-time based, with velocity preserved on reversal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Resize,
    Collapse,
    Notice,
}
#[derive(Clone, Copy)]
pub struct Spring {
    start: u64,
    from: [f32; 2],
    velocity: [f32; 2],
    target: [f32; 2],
    kind: Kind,
}
#[derive(Clone, Copy)]
pub struct CollapseVisual {
    pub panel_alpha: f32,
    pub capsule_alpha: f32,
}
#[derive(Clone, Copy)]
pub struct DockPose {
    pub anchor: [f32; 2],
    pub size: [f32; 2],
    pub attached: f32,
}
impl DockPose {
    fn values(self) -> [f32; 5] {
        [
            self.anchor[0],
            self.anchor[1],
            self.size[0],
            self.size[1],
            self.attached,
        ]
    }
}
/// Position, size and top-edge attachment share one smooth endpoint. Hermite
/// interpolation carries current velocity when the user reverses the switch.
#[derive(Clone, Copy)]
pub struct Dock {
    start: u64,
    from: [f32; 5],
    velocity: [f32; 5],
    target: [f32; 5],
}
impl Dock {
    pub fn new(now: u64, from: DockPose, target: DockPose, previous: Option<Self>) -> Self {
        let (from, velocity) = previous
            .map(|d| d.sample(now))
            .unwrap_or((from.values(), [0.; 5]));
        Self {
            start: now,
            from,
            velocity,
            target: target.values(),
        }
    }
    fn sample(self, now: u64) -> ([f32; 5], [f32; 5]) {
        let t = (now.saturating_sub(self.start) as f32 / 380.0).min(1.0);
        let h00 = 2.0 * t * t * t - 3.0 * t * t + 1.0;
        let h10 = t * t * t - 2.0 * t * t + t;
        let h01 = -2.0 * t * t * t + 3.0 * t * t;
        let p = std::array::from_fn(|i| {
            h00 * self.from[i] + h10 * self.velocity[i] * 0.38 + h01 * self.target[i]
        });
        let v = std::array::from_fn(|i| {
            ((6.0 * t * t - 6.0 * t) * self.from[i]
                + (3.0 * t * t - 4.0 * t + 1.0) * self.velocity[i] * 0.38
                + (-6.0 * t * t + 6.0 * t) * self.target[i])
                / 0.38
        });
        (p, v)
    }
    pub fn pose(self, now: u64) -> DockPose {
        let (p, _) = self.sample(now);
        DockPose {
            anchor: [p[0], p[1]],
            size: [p[2].max(1.0), p[3].max(1.0)],
            attached: p[4].clamp(0.0, 1.0),
        }
    }
    pub fn done(self, now: u64) -> bool {
        now.saturating_sub(self.start) >= 380
    }
}
impl Spring {
    pub fn new(
        now: u64,
        from: (i32, i32),
        target: (i32, i32),
        previous: Option<Self>,
        kind: Kind,
    ) -> Self {
        let (position, mut velocity) = previous
            .map(|s| s.sample(now))
            .unwrap_or(([from.0 as f32, from.1 as f32], [0.; 2]));
        if kind == Kind::Notice && previous.is_none() && from.1 <= target.1 {
            velocity = [180., 130.];
        }
        Self {
            start: now,
            from: position,
            velocity,
            target: [target.0 as f32, target.1 as f32],
            kind,
        }
    }
    pub fn sample(self, now: u64) -> ([f32; 2], [f32; 2]) {
        let t = now.saturating_sub(self.start) as f32 / 1000.;
        // A tall panel needs more travel time on collapse. Opening keeps its
        // original response; closing spreads the contraction over ~210ms and
        // uses a softer rebound instead of rushing into the compact endpoint.
        let (omega, damping) = match self.kind {
            Kind::Collapse => (20.0, 0.80),
            Kind::Notice => (29.0, 0.60),
            Kind::Resize => (26.0, 0.73),
        };
        let decay = omega * damping;
        let frequency = omega * (1.0_f32 - damping * damping).sqrt();
        let (sin, cos) = (frequency * t).sin_cos();
        let envelope = (-decay * t).exp();
        let mut position = self.target;
        let mut velocity = [0.; 2];
        for axis in 0..2 {
            let a = self.from[axis] - self.target[axis];
            let b = (self.velocity[axis] + decay * a) / frequency;
            let offset = a * cos + b * sin;
            position[axis] += envelope * offset;
            velocity[axis] =
                envelope * (-decay * offset - a * frequency * sin + b * frequency * cos);
            // Soft-limit the rebound instead of hard clipping it: a tall panel
            // should not squash the 58px capsule by a third of its height.
            let low = self.from[axis].min(self.target[axis]);
            let high = self.from[axis].max(self.target[axis]);
            let boundary = position[axis].clamp(low, high);
            let excess = position[axis] - boundary;
            if excess != 0.0 {
                let cap = if axis == 0 { 14.0 } else { 9.0 };
                let attenuation = (-excess.abs() / cap).exp();
                position[axis] = boundary + excess.signum() * cap * (1.0 - attenuation);
                velocity[axis] *= attenuation;
            }
        }
        (position, velocity)
    }
    pub fn collapse_visual(self, now: u64) -> Option<CollapseVisual> {
        (self.kind == Kind::Collapse).then(|| {
            let elapsed = now.saturating_sub(self.start) as f32;
            let smooth = |t: f32| {
                let t = t.clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            CollapseVisual {
                panel_alpha: 1.0 - smooth(elapsed / 90.0),
                capsule_alpha: smooth((elapsed - 60.0) / 120.0),
            }
        })
    }
    pub fn size(self, now: u64) -> ((i32, i32), bool) {
        let (p, v) = self.sample(now);
        let done = now.saturating_sub(self.start) >= 620
            || (0..2).all(|i| (p[i] - self.target[i]).abs() < 0.08 && v[i].abs() < 0.8);
        let p = if done { self.target } else { p };
        ((p[0].round() as i32, p[1].round() as i32), done)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn docking_moves_size_position_and_shape_together_and_reverses_continuously() {
        let floating = DockPose {
            anchor: [900., 40.],
            size: [456., 576.],
            attached: 0.,
        };
        let anchored = DockPose {
            anchor: [1000., 0.],
            size: [480., 576.],
            attached: 1.,
        };
        let attach = Dock::new(0, floating, anchored, None);
        assert_eq!(attach.pose(0).values(), floating.values());
        assert_eq!(attach.pose(380).values(), anchored.values());
        let mid = attach.pose(190);
        assert!((mid.attached - 0.5).abs() < 0.001);
        assert_eq!(mid.anchor, [950., 20.]);
        let reverse = Dock::new(150, mid, floating, Some(attach));
        let a = attach.sample(150);
        let b = reverse.sample(150);
        for i in 0..5 {
            assert!((a.0[i] - b.0[i]).abs() < 0.001);
            assert!((a.1[i] - b.1[i]).abs() < 0.01);
        }
        assert_eq!(reverse.pose(530).values(), floating.values());
        assert!(reverse.done(530));
    }
    #[test]
    fn closes_with_a_small_bounce_and_exact_endpoint() {
        let s = Spring::new(0, (456, 576), (336, 58), None, Kind::Collapse);
        assert_eq!(s.size(0).0, (456, 576));
        let min = (0..=620).map(|t| s.size(t).0.1).min().unwrap();
        assert!((49..58).contains(&min));
        assert_eq!(s.size(620), ((336, 58), true));
        assert!(s.size(80).0.1 > 280);
        assert!(s.size(160).0.1 > 70);
    }
    #[test]
    fn retargeting_preserves_position_and_velocity() {
        let closing = Spring::new(0, (456, 500), (336, 58), None, Kind::Collapse);
        let opening = Spring::new(75, (0, 0), (456, 500), Some(closing), Kind::Resize);
        let a = closing.sample(75);
        let b = opening.sample(75);
        for i in 0..2 {
            assert!((a.0[i] - b.0[i]).abs() < 0.001);
            assert!((a.1[i] - b.1[i]).abs() < 0.01);
        }
        assert_eq!(opening.size(695), ((456, 500), true));
    }
    #[test]
    fn notice_pop_is_finite_and_overshoots_then_settles() {
        let s = Spring::new(100, (336, 58), (384, 82), None, Kind::Notice);
        assert!((100..600).any(|t| s.size(t).0.0 > 384));
        assert_eq!(s.size(720), ((384, 82), true));
    }
    #[test]
    fn closing_content_fades_without_changing_opening() {
        let close = Spring::new(100, (456, 576), (336, 58), None, Kind::Collapse);
        let start = close.collapse_visual(100).unwrap();
        assert_eq!((start.panel_alpha, start.capsule_alpha), (1.0, 0.0));
        let end = close.collapse_visual(280).unwrap();
        assert_eq!((end.panel_alpha, end.capsule_alpha), (0.0, 1.0));
        let open = Spring::new(100, (336, 58), (456, 576), None, Kind::Resize);
        assert!(open.collapse_visual(150).is_none());
        assert!(open.size(180).0.1 > 300);
    }
    #[test]
    fn dismissing_a_notification_never_draws_outgoing_panel_content() {
        let notice = Spring::new(0, (336, 58), (384, 82), None, Kind::Notice);
        let dismiss = Spring::new(620, (384, 82), (336, 58), Some(notice), Kind::Resize);
        assert!(dismiss.collapse_visual(650).is_none());
    }
}
