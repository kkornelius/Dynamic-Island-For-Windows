use super::*;
pub(super) fn timer(v: &mut View<'_>, now: u64) {
    let t = &v.s.timer;
    v.center(
        match t.status {
            Status::Running => "MAKE ROOM TO FOCUS",
            Status::Paused => "TAKE YOUR TIME",
            Status::Finished => "TIME WELL SPENT",
            _ => "ONE THING AT A TIME",
        },
        57,
        23,
        10,
        true,
        MUTED,
    );
    artwork::hourglass(v.c, 174, 87, 108);
    v.center(
        &format_duration(t.remaining_ms(now)),
        198,
        67,
        52,
        true,
        WHITE,
    );
    v.center(
        match t.status {
            Status::Running => "Your timer is running",
            Status::Paused => "Paused. Ready when you are.",
            Status::Finished => "Timer complete",
            _ => "Set a little time aside",
        },
        260,
        22,
        12,
        false,
        MUTED,
    );
    for (i, (label, seconds)) in [("5 min", 300), ("25 min", 1500), ("45 min", 2700)]
        .into_iter()
        .enumerate()
    {
        v.button(
            label,
            [44 + i as i32 * 125, 294, 116, 32],
            Action::Preset(seconds),
            false,
            t.status != Status::Running,
        );
    }
    let enabled = t.status != Status::Running;
    v.button(
        "−",
        [44, 338, 30, 30],
        Action::Duration(-60),
        false,
        enabled,
    );
    v.c.text(
        &format!("{} min", t.duration_ms / 60000),
        79,
        338,
        74,
        30,
        13,
        false,
        WHITE,
        true,
    );
    v.button(
        "+",
        [158, 338, 30, 30],
        Action::Duration(60),
        false,
        enabled,
    );
    v.button(
        "−",
        [268, 338, 30, 30],
        Action::Duration(-1),
        false,
        enabled,
    );
    v.c.text(
        &format!("{} sec", t.duration_ms / 1000 % 60),
        303,
        338,
        74,
        30,
        13,
        false,
        WHITE,
        true,
    );
    v.button("+", [382, 338, 30, 30], Action::Duration(1), false, enabled);
    v.button(
        match t.status {
            Status::Running => "Pause timer",
            Status::Paused => "Resume timer",
            Status::Finished => "Start again",
            _ => "Start timer",
        },
        [44, 380, 260, 34],
        Action::TimerToggle,
        true,
        true,
    );
    v.button("Reset", [316, 380, 96, 34], Action::TimerReset, false, true);
}
pub(super) fn water(v: &mut View<'_>, now: u64) {
    if v.s.water_schedule {
        v.label("YOUR REMINDER SCHEDULE", 28, 63, 390, 25, 10, true, MUTED);
        let w = &v.s.config.water;
        v.stepper(
            "Active from",
            &format!("{:02}:00", w.active_start),
            110,
            Setting::ActiveStart,
            1,
        );
        v.stepper(
            "Active until",
            &format!("{:02}:00", w.active_end),
            156,
            Setting::ActiveEnd,
            1,
        );
        v.toggle(
            "Quiet hours",
            "Pause reminders during this time",
            205,
            Setting::Quiet,
            w.quiet_enabled,
        );
        v.stepper(
            "Quiet from",
            &format!("{:02}:00", w.quiet_start),
            267,
            Setting::QuietStart,
            1,
        );
        v.stepper(
            "Quiet until",
            &format!("{:02}:00", w.quiet_end),
            313,
            Setting::QuietEnd,
            1,
        );
        v.button(
            "Done",
            [28, 371, 400, 34],
            Action::WaterSchedule,
            true,
            true,
        );
        return;
    }
    v.label(
        "A LITTLE SIP, A LITTLE RESET",
        28,
        62,
        282,
        22,
        10,
        true,
        MUTED,
    );
    v.button(
        "Schedule",
        [327, 59, 101, 28],
        Action::WaterSchedule,
        false,
        true,
    );
    artwork::bottle(v.c, 40, 91, 112);
    v.label("Water break", 163, 106, 265, 29, 23, true, WHITE);
    v.label(
        &format!("{} check-ins today", v.s.water.count),
        164,
        143,
        260,
        26,
        15,
        true,
        WHITE,
    );
    let next = if !v.s.config.water.enabled {
        "Reminders are off".into()
    } else if v.s.water.pending {
        "Time for a sip".into()
    } else if now >= v.s.water.next_due {
        "Waiting for active hours".into()
    } else {
        format!("Next in {}", format_duration(v.s.water.next_due - now))
    };
    v.label(&next, 164, 183, 260, 25, 12, false, MUTED);
    v.c.text_block(
        "A small pause for yourself.",
        164,
        219,
        240,
        44,
        12,
        false,
        DIM,
    );
    v.button(
        "I had some water",
        [28, 289, 258, 40],
        Action::WaterConfirm,
        true,
        true,
    );
    v.button(
        "Snooze 10m",
        [298, 289, 130, 40],
        Action::WaterSnooze,
        false,
        v.s.config.water.enabled,
    );
    v.stepper(
        "Remind me every",
        &format!("{}m", v.s.config.water.interval_minutes),
        345,
        Setting::WaterInterval,
        1,
    );
    v.c.round(28, 386, 400, 50, 13, CARD);
    v.label("Water reminders", 43, 389, 250, 22, 13, true, WHITE);
    v.label(
        if v.s.config.water.enabled {
            "On · Tap to turn off"
        } else {
            "Off · Tap to enable"
        },
        43,
        412,
        250,
        16,
        10,
        false,
        MUTED,
    );
    let on = v.s.config.water.enabled;
    v.c.round(369, 397, 43, 23, 12, if on { 0xe5e5e5 } else { 0x454545 });
    v.c.circle(
        if on { 392 } else { 372 },
        400,
        17,
        if on { BG } else { 0xc6c6c6 },
    );
    v.hits.push(Hit {
        rect: [28, 386, 400, 50],
        action: Action::WaterToggle,
    });
}
