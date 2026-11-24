//! Deadline arithmetic is independent of rendering and accepts a monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Running,
    Paused,
    Finished,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Minutes,
    Seconds,
}
impl Part {
    pub fn value(self, seconds: u64) -> u64 {
        match self {
            Self::Minutes => seconds / 60,
            Self::Seconds => seconds % 60,
        }
    }
    pub fn replace(self, seconds: u64, text: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value = text.parse::<u64>().ok()?;
        Some(
            match self {
                Self::Minutes => value * 60 + seconds % 60,
                Self::Seconds => seconds / 60 * 60 + value,
            }
            .clamp(1, 86_400),
        )
    }
    pub fn other(self) -> Self {
        match self {
            Self::Minutes => Self::Seconds,
            Self::Seconds => Self::Minutes,
        }
    }
}

pub struct Countdown {
    pub duration_ms: u64,
    pub status: Status,
    deadline_ms: u64,
    paused_ms: u64,
}
impl Countdown {
    pub fn new(seconds: u64) -> Self {
        Self {
            duration_ms: seconds * 1000,
            status: Status::Idle,
            deadline_ms: 0,
            paused_ms: 0,
        }
    }
    pub fn set_seconds(&mut self, seconds: u64) {
        self.duration_ms = seconds.clamp(1, 86_400) * 1000;
        self.reset();
    }
    pub fn start(&mut self, now: u64) {
        let remaining = if self.status == Status::Paused {
            self.paused_ms
        } else {
            self.duration_ms
        };
        self.deadline_ms = now.saturating_add(remaining);
        self.status = Status::Running;
    }
    pub fn pause(&mut self, now: u64) {
        if self.status == Status::Running && now < self.deadline_ms {
            self.paused_ms = self.remaining_ms(now);
            self.status = Status::Paused;
        }
    }
    pub fn reset(&mut self) {
        self.status = Status::Idle;
        self.paused_ms = 0;
    }
    pub fn remaining_ms(&self, now: u64) -> u64 {
        match self.status {
            Status::Running => self.deadline_ms.saturating_sub(now),
            Status::Paused => self.paused_ms,
            Status::Finished => 0,
            Status::Idle => self.duration_ms,
        }
    }
    pub fn tick(&mut self, now: u64) -> bool {
        if self.status == Status::Running && now >= self.deadline_ms {
            self.status = Status::Finished;
            true
        } else {
            false
        }
    }
}

/// Delay until the next visible ceil-rounded second, without accumulating timer drift.
pub fn next_display_change(remaining_ms: u64) -> u64 {
    if remaining_ms == 0 {
        1
    } else {
        (remaining_ms - 1) % 1000 + 1
    }
}

pub fn format_duration(ms: u64) -> String {
    let seconds = ms.div_ceil(1000);
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_parts_normalize_seconds_and_enforce_timer_limits() {
        assert_eq!(Part::Minutes.replace(2700, "2"), Some(120));
        assert_eq!(Part::Seconds.replace(120, "90"), Some(210));
        assert_eq!(Part::Minutes.replace(45, "0"), Some(45));
        assert_eq!(Part::Seconds.replace(1, "0"), Some(1));
        assert_eq!(Part::Minutes.replace(59, "99999"), Some(86400));
        for invalid in ["", "-5", "1.5", "12m", "１２", "100000"] {
            assert_eq!(Part::Seconds.replace(60, invalid), None);
        }
    }
    #[test]
    fn pause_resume_keeps_fractional_seconds() {
        let mut t = Countdown::new(60);
        t.start(1000);
        t.pause(2456);
        assert_eq!(t.remaining_ms(999999), 58544);
        t.start(100000);
        assert_eq!(t.remaining_ms(101000), 57544);
    }
    #[test]
    fn expiration_is_once_even_after_sleep() {
        let mut t = Countdown::new(5);
        t.start(100);
        assert!(!t.tick(5099));
        assert!(t.tick(3_600_100));
        assert!(!t.tick(3_600_101));
        assert_eq!(t.remaining_ms(3_600_102), 0);
    }
    #[test]
    fn reset_and_restart() {
        let mut t = Countdown::new(10);
        t.start(1);
        t.pause(100);
        t.reset();
        assert_eq!(t.remaining_ms(999), 10000);
        t.start(200);
        assert!(t.tick(10200));
        t.start(10300);
        assert_eq!(t.remaining_ms(10300), 10000);
    }
    #[test]
    fn display_deadlines_align_with_rounded_seconds() {
        for (remaining, expected) in [(0, 1), (1, 1), (1000, 1000), (1001, 1), (345678, 678)] {
            assert_eq!(next_display_change(remaining), expected);
            if remaining > 0 {
                let delay = next_display_change(remaining);
                assert_eq!(
                    format_duration(remaining),
                    format_duration(remaining - delay + 1)
                );
                assert_ne!(
                    format_duration(remaining),
                    format_duration(remaining - delay)
                );
            }
        }
    }
    #[test]
    fn display_rounds_up_and_supports_hours() {
        assert_eq!(format_duration(1), "00:01");
        assert_eq!(format_duration(3_661_000), "1:01:01");
    }
}
