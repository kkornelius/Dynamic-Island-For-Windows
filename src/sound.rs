use windows::{
    Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCSTR,
};

// The asynchronous Win32 player retains this pointer until playback finishes.
// Embedded static storage keeps it valid without file I/O or a decoding worker.
const WATER_GULP: &[u8] = include_bytes!("../assets/sounds/water-gulp.wav");

pub fn water_check_in() -> bool {
    unsafe {
        PlaySoundA(
            PCSTR(WATER_GULP.as_ptr()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        )
        .as_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_drinking_clip_is_short_pcm_with_silent_boundaries() {
        let wav = WATER_GULP;
        let u16_at = |i| u16::from_le_bytes(wav[i..i + 2].try_into().unwrap());
        let u32_at = |i| u32::from_le_bytes(wav[i..i + 4].try_into().unwrap());
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32_at(4) as usize + 8, wav.len());
        assert_eq!(u32_at(16), 16);
        assert_eq!(u16_at(20), 1); // PCM, no runtime codec needed.
        assert_eq!(u16_at(22), 1);
        assert_eq!(u32_at(24), 24_000);
        assert_eq!(u16_at(32), 2);
        assert_eq!(u16_at(34), 16);
        assert_eq!(&wav[36..40], b"data");
        let bytes = u32_at(40) as usize;
        assert_eq!(bytes + 44, wav.len());
        assert!((1.5..2.5).contains(&(bytes as f64 / 48_000.0)));
        let samples: Vec<_> = wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| i16::from_le_bytes([s[0], s[1]]))
            .collect();
        assert_eq!(samples[0], 0);
        assert_eq!(samples.last(), Some(&0));
        assert!(samples.iter().any(|s| s.unsigned_abs() > 1_000));
        assert!(samples.iter().all(|s| s.unsigned_abs() < 16_000));
    }
}
