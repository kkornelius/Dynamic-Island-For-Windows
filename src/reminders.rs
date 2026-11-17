use crate::storage::WaterConfig;

/// Half-open hour range; equal endpoints mean all day. Overnight ranges work.
pub fn in_hours(hour: u16, start: u16, end: u16) -> bool {
    if start == end {
        true
    } else if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}
pub fn allowed(config: &WaterConfig, hour: u16) -> bool {
    config.enabled
        && in_hours(hour, config.active_start, config.active_end)
        && !(config.quiet_enabled && in_hours(hour, config.quiet_start, config.quiet_end))
}
pub struct WaterReminder {
    pub next_due: u64,
    pub pending: bool,
    pub count: u32,
    pub day: u32,
}
impl WaterReminder {
    pub fn new(now: u64, config: &WaterConfig, day: u32, count: u32) -> Self {
        Self {
            next_due: now + config.interval_minutes as u64 * 60_000,
            pending: false,
            day,
            count,
        }
    }
    pub fn reschedule(&mut self, now: u64, config: &WaterConfig) {
        self.next_due = now + config.interval_minutes as u64 * 60_000;
        self.pending = false;
    }
    pub fn tick(&mut self, now: u64, hour: u16, day: u32, config: &WaterConfig) -> bool {
        if self.day != day {
            self.day = day;
            self.count = 0;
        }
        if now >= self.next_due && !self.pending && allowed(config, hour) {
            self.pending = true;
            true
        } else {
            false
        }
    }
    pub fn confirm(&mut self, now: u64, config: &WaterConfig) {
        self.count = self.count.saturating_add(1);
        self.reschedule(now, config);
    }
    pub fn snooze(&mut self, now: u64) {
        self.pending = false;
        self.next_due = now + 10 * 60_000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overnight_quiet_hours() {
        let c = WaterConfig {
            enabled: true,
            active_start: 0,
            active_end: 0,
            ..Default::default()
        };
        assert!(!allowed(&c, 23));
        assert!(!allowed(&c, 6));
        assert!(allowed(&c, 12));
    }
    #[test]
    fn schedule_deduplicates_and_snoozes() {
        let c = WaterConfig {
            enabled: true,
            ..Default::default()
        };
        let mut w = WaterReminder::new(0, &c, 1, 0);
        let due = w.next_due;
        assert!(w.tick(due, 12, 1, &c));
        assert!(!w.tick(due + 1, 12, 1, &c));
        w.snooze(due);
        assert!(!w.tick(due + 599999, 12, 1, &c));
        assert!(w.tick(due + 600000, 12, 1, &c));
        w.confirm(due + 600000, &c);
        assert_eq!(w.count, 1);
        w.tick(due + 600001, 12, 2, &c);
        assert_eq!(w.count, 0);
    }
    #[test]
    fn suppressed_reminder_can_fire_after_quiet_hours() {
        let c = WaterConfig {
            enabled: true,
            ..Default::default()
        };
        let mut w = WaterReminder::new(0, &c, 1, 0);
        assert!(!w.tick(w.next_due, 23, 1, &c));
        assert!(w.tick(w.next_due + 3600000, 10, 1, &c));
    }
}
