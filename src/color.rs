//! HSV state and hit geometry shared by the native color wheel and pointer input.
use std::f32::consts::TAU;

pub const WHEEL: [i32; 4] = [32, 202, 184, 184];
pub const VALUE: [i32; 4] = [248, 374, 174, 18];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Wheel,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}
impl Hsv {
    pub fn from_rgb(rgb: u32) -> Self {
        let [r, g, b] = [16, 8, 0].map(|shift| ((rgb >> shift) & 255) as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };
        Self {
            h,
            s: if max == 0.0 { 0.0 } else { d / max },
            v: max,
        }
    }
    pub fn rgb(self) -> u32 {
        let h = self.h.rem_euclid(1.0) * 6.0;
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
        let [r, g, b] = match h as u32 {
            0 => [c, x, 0.0],
            1 => [x, c, 0.0],
            2 => [0.0, c, x],
            3 => [0.0, x, c],
            4 => [x, 0.0, c],
            _ => [c, 0.0, x],
        };
        let [r, g, b] = [r, g, b].map(|n| ((n + v - c) * 255.0).round() as u32);
        (r << 16) | (g << 8) | b
    }
    pub fn pick(&mut self, part: Part, x: f32, y: f32) {
        match part {
            Part::Wheel => {
                let radius = WHEEL[2] as f32 / 2.0;
                let dx = (x - WHEEL[0] as f32 - radius) / radius;
                let dy = (y - WHEEL[1] as f32 - radius) / radius;
                let distance = dx.hypot(dy);
                if distance > 0.0001 {
                    self.h = (dy.atan2(dx) / TAU).rem_euclid(1.0);
                }
                self.s = distance.clamp(0.0, 1.0);
            }
            Part::Value => self.v = ((x - VALUE[0] as f32) / VALUE[2] as f32).clamp(0.0, 1.0),
        }
    }
    pub fn marker(self) -> (i32, i32) {
        let radius = WHEEL[2] as f32 / 2.0;
        // Keep the marker legible at the rim while input still spans full saturation.
        let r = self.s * (radius - 5.0);
        (
            (WHEEL[0] as f32 + radius + (self.h * TAU).cos() * r).round() as i32,
            (WHEEL[1] as f32 + radius + (self.h * TAU).sin() * r).round() as i32,
        )
    }
}
pub fn hit(x: f32, y: f32) -> Option<Part> {
    let radius = WHEEL[2] as f32 / 2.0;
    if (x - WHEEL[0] as f32 - radius).hypot(y - WHEEL[1] as f32 - radius) <= radius {
        Some(Part::Wheel)
    } else if x >= (VALUE[0] - 5) as f32
        && x <= (VALUE[0] + VALUE[2] + 5) as f32
        && y >= (VALUE[1] - 7) as f32
        && y <= (VALUE[1] + VALUE[3] + 7) as f32
    {
        Some(Part::Value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgb_round_trips_cover_hues_and_neutrals() {
        for color in [
            0, 0xffffff, 0x808080, 0xff0000, 0x00ff00, 0x0000ff, 0xaea0d9, 0x123456, 0x1ed760,
        ] {
            assert_eq!(Hsv::from_rgb(color).rgb(), color);
        }
    }
    #[test]
    fn wheel_and_value_clamp_drags_and_retain_hue_in_black() {
        let mut c = Hsv::from_rgb(0xffffff);
        c.pick(Part::Wheel, 300.0, 294.0);
        assert_eq!(c.rgb(), 0xff0000);
        c.pick(Part::Value, -500.0, 0.0);
        assert_eq!(c.rgb(), 0);
        c.pick(Part::Value, 900.0, 0.0);
        assert_eq!(c.rgb(), 0xff0000);
        c.pick(Part::Wheel, 124.0, 294.0);
        assert_eq!(c.rgb(), 0xffffff);
        assert_eq!(hit(32.0, 202.0), None);
        assert_eq!(hit(124.0, 294.0), Some(Part::Wheel));
        assert_eq!(hit(330.0, 382.0), Some(Part::Value));
    }
}
