//! Allocation-free envelope of actual samples. These are amplitude-history bars, not fake FFT bands.
pub const BARS: usize = 28;
pub struct Levels {
    pub bars: [f32; BARS],
    envelope: f32,
}
impl Default for Levels {
    fn default() -> Self {
        Self {
            bars: [0.0; BARS],
            envelope: 0.0,
        }
    }
}
impl Levels {
    pub fn push(&mut self, rms: f32, sensitivity: f32, smoothing: f32) {
        let target = if rms.is_finite() && rms > 0.0003 {
            (rms * sensitivity).sqrt().clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.envelope = if target > self.envelope {
            target
        } else {
            self.envelope * smoothing + target * (1.0 - smoothing)
        };
        if self.envelope < 0.003 {
            self.envelope = 0.0;
        }
        self.bars.copy_within(1.., 0);
        self.bars[BARS - 1] = self.envelope;
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Sample {
    Float32,
    Pcm16,
    Pcm24,
    Pcm32,
    Pcm8,
}
pub fn rms(bytes: &[u8], format: Sample) -> f32 {
    let width = match format {
        Sample::Float32 | Sample::Pcm32 => 4,
        Sample::Pcm24 => 3,
        Sample::Pcm16 => 2,
        Sample::Pcm8 => 1,
    };
    let mut sum = 0.0f64;
    let mut count = 0;
    for b in bytes.chunks_exact(width) {
        let x = match format {
            Sample::Float32 => f32::from_le_bytes(b.try_into().unwrap()),
            Sample::Pcm32 => i32::from_le_bytes(b.try_into().unwrap()) as f32 / 2147483648.0,
            Sample::Pcm24 => {
                ((b[0] as i32 | (b[1] as i32) << 8 | (b[2] as i32) << 16) << 8 >> 8) as f32
                    / 8388608.0
            }
            Sample::Pcm16 => i16::from_le_bytes(b.try_into().unwrap()) as f32 / 32768.0,
            Sample::Pcm8 => (b[0] as f32 - 128.0) / 128.0,
        };
        if x.is_finite() {
            let x = x.clamp(-1.0, 1.0) as f64;
            sum += x * x;
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64).sqrt() as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_signal_and_silence() {
        let bytes: Vec<u8> = [0.5f32, -0.5, 0.5, -0.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert!((rms(&bytes, Sample::Float32) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[0; 16], Sample::Float32), 0.0);
        assert_eq!(rms(&[128; 4], Sample::Pcm8), 0.0);
    }
    #[test]
    fn integer_decoding() {
        assert!((rms(&[0, 64], Sample::Pcm16) - 0.5).abs() < 1e-6);
        assert!((rms(&[0, 0, 64], Sample::Pcm24) - 0.5).abs() < 1e-6);
        assert!((rms(&[0, 0, 192], Sample::Pcm24) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&f32::NAN.to_le_bytes(), Sample::Float32), 0.0);
    }
    #[test]
    fn envelope_eventually_settles_at_zero() {
        let mut l = Levels::default();
        l.push(0.5, 2.5, 0.72);
        assert!(l.bars[BARS - 1] > 0.0);
        for _ in 0..100 {
            l.push(0.0, 2.5, 0.72);
        }
        assert_eq!(l.bars, [0.0; BARS]);
    }
}
