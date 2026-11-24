//! Compare a reused, padded surface with independently allocated native renders.
use super::*;
pub fn verify_reuse() -> windows::core::Result<()> {
    let (mut s, now) = preview::fixture_state();
    let local = LocalTime::get();
    s.config.stroke_color = 0xa69bff;
    s.config.stroke_width = 3;
    s.config.stroke_glow = 100;
    s.status.clear();
    let mut cases = 0;
    let mut maximum = 0u8;
    let mut stable = true;
    for scale in [1.0, 1.5, 2.5] {
        for mode in [DockMode::Floating, DockMode::Anchored] {
            for stroke in [true, false] {
                s.config.dock_mode = mode;
                s.config.stroke_enabled = stroke;
                let mut reused = Canvas::with_capacity(
                    392,
                    114,
                    MEDIA_WIDTH + 24 + 2 * HALO,
                    HEIGHT + 2 * HALO,
                    scale,
                )?;
                let pointer = reused.bits;
                for (tab, expanded, lyrics, page) in [
                    (Tab::Media, false, false, 0),
                    (Tab::Media, true, false, 0),
                    (Tab::Media, true, true, 0),
                    (Tab::Settings, true, false, 0),
                    (Tab::Settings, true, false, 3),
                    (Tab::Weather, true, false, 0),
                    (Tab::Notes, true, false, 0),
                    (Tab::Media, false, false, 0),
                ] {
                    s.tab = tab;
                    s.settings_page = page;
                    s.expanded = expanded;
                    s.show_lyrics = lyrics;
                    let w = if expanded {
                        expanded_width(&s)
                    } else {
                        COMPACT_WIDTH
                    } + 2 * surface_inset(&s);
                    let h = if expanded {
                        expanded_height(&s)
                    } else {
                        COMPACT_HEIGHT
                    } + top_inset(&s)
                        + HALO;
                    assert!(reused.resize(w, h));
                    stable &= pointer == reused.bits;
                    let mut reference = Canvas::new(w, h, scale)?;
                    draw(&mut reused, &s, now, &local);
                    reused.finish()?;
                    draw(&mut reference, &s, now, &local);
                    reference.finish()?;
                    let radius = if expanded { 28 } else { 29 };
                    reused.mask(radius, mode);
                    reference.mask(radius, mode);
                    for y in 0..reused.height {
                        unsafe {
                            let a = std::slice::from_raw_parts(
                                reused.bits.add((y * reused.stride * 4) as usize),
                                reused.width as usize * 4,
                            );
                            let b = std::slice::from_raw_parts(
                                reference.bits.add((y * reference.stride * 4) as usize),
                                reference.width as usize * 4,
                            );
                            maximum = maximum.max(
                                a.iter()
                                    .zip(b)
                                    .map(|(a, b)| a.abs_diff(*b))
                                    .max()
                                    .unwrap_or(0),
                            );
                        }
                    }
                    cases += 1;
                }
            }
        }
    }
    let report = serde_json::json!({"cases":cases,"surface_pointer_stable":stable,"maximum_channel_difference":maximum,"network":false});
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/canvas-reuse-verification.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| {
        windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
    })?;
    if !stable || maximum > 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Retained surface differs from a fresh render",
        ));
    }
    verify_idle_face()?;
    verify_collapse()?;
    verify_dock()?;
    Ok(())
}

fn verify_dock() -> windows::core::Result<()> {
    let (mut s, now) = preview::fixture_state();
    let local = LocalTime::get();
    s.expanded = true;
    s.tab = Tab::Settings;
    s.settings_page = 0;
    s.status.clear();
    let output = std::path::Path::new("artifacts/dock-verification");
    std::fs::create_dir_all(output).ok();
    let mut maximum = 0u8;
    let mut cases = 0;
    for scale in [1.0, 1.5, 2.5] {
        for mode in [DockMode::Floating, DockMode::Anchored] {
            for stroke in [false, true] {
                s.config.dock_mode = mode;
                s.config.stroke_enabled = stroke;
                let w = WIDTH + 2 * surface_inset(&s);
                let h = HEIGHT + top_inset(&s) + HALO;
                let mut reference = Canvas::new(w, h, scale)?;
                let mut animated = Canvas::new(w, h, scale)?;
                animated.set_dock_mix(Some(if mode == DockMode::Anchored { 1.0 } else { 0.0 }));
                draw(&mut reference, &s, now, &local);
                draw(&mut animated, &s, now, &local);
                reference.finish()?;
                animated.finish()?;
                reference.mask(28, mode);
                animated.mask(28, mode);
                for y in 0..reference.height {
                    unsafe {
                        let a = std::slice::from_raw_parts(
                            reference.bits.add((y * reference.stride * 4) as usize),
                            reference.width as usize * 4,
                        );
                        let b = std::slice::from_raw_parts(
                            animated.bits.add((y * animated.stride * 4) as usize),
                            animated.width as usize * 4,
                        );
                        maximum = maximum.max(
                            a.iter()
                                .zip(b)
                                .map(|(a, b)| a.abs_diff(*b))
                                .max()
                                .unwrap_or(0),
                        );
                    }
                }
                cases += 1;
            }
        }
    }
    let mut hits = true;
    s.config.dock_mode = DockMode::Anchored;
    s.config.stroke_enabled = true;
    s.config.stroke_glow = 100;
    for q in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let top = (HALO as f32 * (1.0 - q)).round() as i32;
        let width = WIDTH + (24.0 * q).round() as i32;
        let mut c = Canvas::new(width + 2 * HALO, HEIGHT + top + HALO, 1.0)?;
        c.set_dock_mix(Some(q));
        draw(&mut c, &s, now, &local);
        c.finish()?;
        c.mask(28, DockMode::Anchored);
        hits &= c.hit_test(c.width / 2, top + 100) && !c.hit_test(0, 0);
        c.save_bmp(&output.join(format!("dock-{:03}.bmp", (q * 100.0) as u32)))
            .ok();
    }
    let report = serde_json::json!({"endpoint_cases":cases,"maximum_channel_difference":maximum,"intermediate_hit_regions_valid":hits});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .ok();
    if maximum > 1 || !hits {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Animated dock geometry differs at its endpoints or hit regions",
        ));
    }
    Ok(())
}

fn verify_collapse() -> windows::core::Result<()> {
    let (mut s, now) = preview::fixture_state();
    let local = LocalTime::get();
    s.status.clear();
    let output = std::path::Path::new("artifacts/collapse-verification");
    std::fs::create_dir_all(output).ok();
    let mut maximum = 0u8;
    let mut cases = 0;
    for scale in [1.0, 1.5, 2.5] {
        for mode in [DockMode::Floating, DockMode::Anchored] {
            for tab in [Tab::Media, Tab::Weather, Tab::Notes, Tab::Settings] {
                s.config.dock_mode = mode;
                s.config.stroke_enabled = true;
                s.config.stroke_glow = 100;
                s.tab = tab;
                s.show_lyrics = true;
                let from = (expanded_width(&s) + 2 * gutter(&s), expanded_height(&s));
                let target = (COMPACT_WIDTH + 2 * gutter(&s), COMPACT_HEIGHT);
                let spring = crate::motion::Spring::new(
                    now,
                    from,
                    target,
                    None,
                    crate::motion::Kind::Collapse,
                );
                for (elapsed, expanded, size) in [(0, true, from), (620, false, target)] {
                    let w = size.0 + 2 * HALO;
                    let h = size.1 + top_inset(&s) + HALO;
                    let mut reference = Canvas::new(w, h, scale)?;
                    let mut animated = Canvas::new(w, h, scale)?;
                    s.expanded = expanded;
                    draw(&mut reference, &s, now, &local);
                    reference.finish()?;
                    s.expanded = false;
                    draw_collapsing(
                        &mut animated,
                        &s,
                        now,
                        &local,
                        size,
                        spring.collapse_visual(now + elapsed).unwrap(),
                    );
                    animated.finish()?;
                    let radius = if expanded { 28 } else { 29 };
                    reference.mask(radius, mode);
                    animated.mask(radius, mode);
                    for y in 0..reference.height {
                        unsafe {
                            let a = std::slice::from_raw_parts(
                                reference.bits.add((y * reference.stride * 4) as usize),
                                reference.width as usize * 4,
                            );
                            let b = std::slice::from_raw_parts(
                                animated.bits.add((y * animated.stride * 4) as usize),
                                animated.width as usize * 4,
                            );
                            maximum = maximum.max(
                                a.iter()
                                    .zip(b)
                                    .map(|(a, b)| a.abs_diff(*b))
                                    .max()
                                    .unwrap_or(0),
                            );
                        }
                    }
                    cases += 1;
                }
                if scale == 1.0 && mode == DockMode::Floating {
                    let name = match tab {
                        Tab::Media => "media",
                        Tab::Weather => "weather",
                        Tab::Notes => "notes",
                        _ => "settings",
                    };
                    for elapsed in [0, 33, 67, 100, 133, 167, 210, 267, 400, 620] {
                        let size = spring.size(now + elapsed).0;
                        let mut c = Canvas::new(size.0 + 2 * HALO, size.1 + 2 * HALO, scale)?;
                        draw_collapsing(
                            &mut c,
                            &s,
                            now,
                            &local,
                            size,
                            spring.collapse_visual(now + elapsed).unwrap(),
                        );
                        c.finish()?;
                        c.mask(29, mode);
                        c.save_bmp(&output.join(format!("{name}-{elapsed:03}.bmp")))
                            .ok();
                    }
                }
            }
        }
    }
    let report = serde_json::json!({"endpoint_cases":cases,"maximum_channel_difference":maximum,"network":false});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .ok();
    if maximum > 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Collapse endpoints differ from their steady layouts",
        ));
    }
    Ok(())
}

fn verify_idle_face() -> windows::core::Result<()> {
    let (mut s, _) = preview::fixture_state();
    let local = LocalTime::get();
    s.expanded = false;
    s.notices.clear();
    s.media.playing = false;
    s.timer.reset();
    s.config.stroke_enabled = false;
    s.config.dock_mode = DockMode::Floating;
    let pixels = |s: &State| -> windows::core::Result<Vec<u8>> {
        let mut c = Canvas::new(COMPACT_WIDTH + 2 * HALO, COMPACT_HEIGHT + 2 * HALO, 1.0)?;
        draw(&mut c, s, 1200, &local);
        c.finish()?;
        c.mask(29, DockMode::Floating);
        Ok(
            unsafe { std::slice::from_raw_parts(c.bits, (c.stride * c.height * 4) as usize) }
                .to_vec(),
        )
    };
    s.bars = [0.0; crate::media::levels::BARS];
    s.face = crate::face::Face::new(0);
    let baseline = pixels(&s)?;
    s.face.advance(1000, [0.0, 0.0], true);
    let mut reused = Canvas::new(COMPACT_WIDTH + 2 * HALO, COMPACT_HEIGHT + 2 * HALO, 1.0)?;
    draw(&mut reused, &s, 1000, &local);
    reused.finish()?;
    reused.mask(29, DockMode::Floating);
    s.face.advance(1096, [0.0, 0.0], false);
    draw(&mut reused, &s, 1200, &local);
    reused.finish()?;
    reused.mask(29, DockMode::Floating);
    let recovered = unsafe {
        std::slice::from_raw_parts(reused.bits, (reused.stride * reused.height * 4) as usize)
    }
    .to_vec();
    s.bars = [0.95; crate::media::levels::BARS];
    s.audio_at_ms = 1200;
    let audio = pixels(&s)?;
    let maximum = |a: &[u8], b: &[u8]| {
        a.iter()
            .zip(b)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0)
    };
    let cleanup = maximum(&baseline, &recovered);
    let idle_audio = maximum(&baseline, &audio);
    s.face = crate::face::Face::new(0);
    s.face.advance(1000, [-1.0, 0.0], false);
    let left = pixels(&s)?;
    s.face = crate::face::Face::new(0);
    s.face.advance(1000, [1.0, 0.0], false);
    let right = pixels(&s)?;
    let gaze = left.iter().zip(&right).filter(|(a, b)| a != b).count();
    // Inspect the bright little ring inside each iris, not movement of the whole eye.
    let marker_offsets = |image: &[u8], look: [f32; 2]| -> [[f64; 2]; 2] {
        [-1.0, 1.0].map(|side| {
            let cx = HALO as f32 + 168.0 + side * 17.0 + look[0] * 3.7;
            let cy = HALO as f32 + 26.0 + look[1] * 3.7;
            let (mut weight, mut sx, mut sy) = (0.0, 0.0, 0.0);
            for y in (cy - 6.0).floor() as i32..=(cy + 6.0).ceil() as i32 {
                for x in (cx - 6.0).floor() as i32..=(cx + 6.0).ceil() as i32 {
                    let dx = x as f32 + 0.5 - cx;
                    let dy = y as f32 + 0.5 - cy;
                    if dx * dx + dy * dy > 4.9 * 4.9 {
                        continue;
                    }
                    let index = ((y * (COMPACT_WIDTH + 2 * HALO) + x) * 4) as usize;
                    let peak = *image[index..index + 3].iter().max().unwrap();
                    if peak > 80 {
                        let w = (peak - 50) as f64;
                        weight += w;
                        sx += dx as f64 * w;
                        sy += dy as f64 * w;
                    }
                }
            }
            if weight > 0.0 {
                [sx / weight, sy / weight]
            } else {
                [0.0, 0.0]
            }
        })
    };
    s.face = crate::face::Face::new(0);
    s.face.advance(1000, [1.0, 1.0], false);
    let down_right = marker_offsets(&pixels(&s)?, s.face.pose(1200).look);
    s.face = crate::face::Face::new(0);
    s.face.advance(1000, [-1.0, -1.0], false);
    let up_left = marker_offsets(&pixels(&s)?, s.face.pose(1200).look);
    let pupils_follow = down_right.iter().all(|p| p[0] > 0.5 && p[1] > 0.5)
        && up_left.iter().all(|p| p[0] < -0.5 && p[1] < -0.5);
    let report = serde_json::json!({"tear_return_pixel_difference":cleanup,"idle_audio_pixel_difference":idle_audio,"left_vs_right_gaze_changed_channels":gaze,"small_pupil_down_right_offsets":down_right,"small_pupil_up_left_offsets":up_left,"small_pupils_follow_cursor":pupils_follow,"passed":cleanup==0&&idle_audio==0&&gaze>0&&pupils_follow});
    std::fs::write(
        "artifacts/idle-face-verification.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| {
        windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
    })?;
    if cleanup != 0 || idle_audio != 0 || gaze == 0 || !pupils_follow {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Idle face cleanup, gaze or hidden-meter render differs",
        ));
    }
    Ok(())
}

pub fn verify_partial() -> windows::core::Result<()> {
    let (mut s, now) = preview::fixture_state();
    let local = LocalTime::get();
    s.status.clear();
    let mut maximum = 0u8;
    let mut cases = 0;
    let mut failures = Vec::new();
    for scale in [1.0, 1.5, 2.5] {
        for mode in [DockMode::Floating, DockMode::Anchored] {
            for stroke in [false, true] {
                for (tab, expanded, lyrics, page) in [
                    (Tab::Media, false, false, 0),
                    (Tab::Media, true, false, 0),
                    (Tab::Media, true, true, 0),
                    (Tab::Weather, true, false, 0),
                    (Tab::Timer, true, false, 0),
                    (Tab::Water, true, false, 0),
                    (Tab::Notes, true, false, 0),
                    (Tab::Settings, true, false, 0),
                    (Tab::Settings, true, false, 1),
                    (Tab::Settings, true, false, 2),
                    (Tab::Settings, true, false, 3),
                ] {
                    s.tab = tab;
                    s.expanded = expanded;
                    s.show_lyrics = lyrics;
                    s.settings_page = page;
                    s.config.dock_mode = mode;
                    s.config.stroke_enabled = stroke;
                    s.config.stroke_width = 3;
                    s.config.stroke_glow = 100;
                    s.config.stroke_color = 0xa69bff;
                    let w = if expanded {
                        expanded_width(&s)
                    } else {
                        COMPACT_WIDTH
                    } + 2 * surface_inset(&s);
                    let h = if expanded {
                        expanded_height(&s)
                    } else {
                        COMPACT_HEIGHT
                    } + top_inset(&s)
                        + HALO;
                    let mut reused = Canvas::new(w, h, scale)?;
                    draw(&mut reused, &s, now + 150, &local);
                    reused.finish()?;
                    reused.mask(if expanded { 28 } else { 29 }, mode);
                    for i in 1..=3 {
                        let stamp = now + 150 + i * 40;
                        s.audio_at_ms = stamp;
                        s.bars = std::array::from_fn(|j| {
                            0.1 + 0.8 * (i as f32 * 0.6 + j as f32).sin().abs()
                        });
                        let [x, y, rw, rh] = meter_rect(&s).unwrap();
                        reused.set_partial(Some([x - 3, y - 3, rw + 6, rh + 6]));
                        draw(&mut reused, &s, stamp, &local);
                        reused.finish()?;
                        reused.mask(if expanded { 28 } else { 29 }, mode);
                        let mut reference = Canvas::new(w, h, scale)?;
                        draw(&mut reference, &s, stamp, &local);
                        reference.finish()?;
                        reference.mask(if expanded { 28 } else { 29 }, mode);
                        let mut case_max = 0u8;
                        for y in 0..reused.height {
                            unsafe {
                                let a = std::slice::from_raw_parts(
                                    reused.bits.add((y * reused.stride * 4) as usize),
                                    (reused.width * 4) as usize,
                                );
                                let b = std::slice::from_raw_parts(
                                    reference.bits.add((y * reference.stride * 4) as usize),
                                    (reference.width * 4) as usize,
                                );
                                case_max = case_max.max(
                                    a.iter()
                                        .zip(b)
                                        .map(|(a, b)| a.abs_diff(*b))
                                        .max()
                                        .unwrap_or(0),
                                );
                            }
                        }
                        maximum = maximum.max(case_max);
                        if case_max > 1 {
                            failures.push(serde_json::json!({"tab":match tab{Tab::Media=>"media",Tab::Weather=>"weather",Tab::Timer=>"timer",Tab::Water=>"water",Tab::Notes=>"notes",Tab::Settings=>"settings"},"page":page,"expanded":expanded,"lyrics":lyrics,"scale":scale,"anchored":mode==DockMode::Anchored,"stroke":stroke,"frame":i,"maximum":case_max}));
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    let report = serde_json::json!({"cases":cases,"maximum_channel_difference":maximum,"passed":maximum<=1,"failures":failures});
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/partial-render-verification.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .ok();
    if maximum > 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Partial/full pixels differ",
        ));
    }
    Ok(())
}

pub fn verify_color_reuse() -> windows::core::Result<()> {
    let (mut s, now) = preview::fixture_state();
    s.status.clear();
    s.config.stroke_enabled = true;
    let local = LocalTime::get();
    let mut maximum = 0u8;
    let mut cases = 0;
    for scale in [1.0, 1.5, 2.5] {
        for mode in [DockMode::Floating, DockMode::Anchored] {
            for (width, glow) in [(1, 0), (3, 100)] {
                for (tab, expanded, lyrics, page) in [
                    (Tab::Media, false, false, 0),
                    (Tab::Media, true, true, 0),
                    (Tab::Settings, true, false, 3),
                ] {
                    s.tab = tab;
                    s.expanded = expanded;
                    s.show_lyrics = lyrics;
                    s.settings_page = page;
                    s.config.dock_mode = mode;
                    s.config.stroke_width = width;
                    s.config.stroke_glow = glow;
                    s.config.stroke_color = 0xffffff;
                    let w = if expanded {
                        expanded_width(&s)
                    } else {
                        COMPACT_WIDTH
                    } + 2 * surface_inset(&s);
                    let h = if expanded {
                        expanded_height(&s)
                    } else {
                        COMPACT_HEIGHT
                    } + top_inset(&s)
                        + HALO;
                    let mut reused = Canvas::new(w, h, scale)?;
                    draw(&mut reused, &s, now, &local);
                    reused.finish()?;
                    reused.mask(if expanded { 28 } else { 29 }, mode);
                    for color in [0x2377dd, 0x1ed760, 0xaea0d9, 0, 0xffffff] {
                        s.config.stroke_color = color;
                        draw(&mut reused, &s, now, &local);
                        reused.finish()?;
                        reused.mask(if expanded { 28 } else { 29 }, mode);
                        let mut reference = Canvas::new(w, h, scale)?;
                        draw(&mut reference, &s, now, &local);
                        reference.finish()?;
                        reference.mask(if expanded { 28 } else { 29 }, mode);
                        for y in 0..reused.height {
                            unsafe {
                                let a = std::slice::from_raw_parts(
                                    reused.bits.add((y * reused.stride * 4) as usize),
                                    (reused.width * 4) as usize,
                                );
                                let b = std::slice::from_raw_parts(
                                    reference.bits.add((y * reference.stride * 4) as usize),
                                    (reference.width * 4) as usize,
                                );
                                maximum = maximum.max(
                                    a.iter()
                                        .zip(b)
                                        .map(|(a, b)| a.abs_diff(*b))
                                        .max()
                                        .unwrap_or(0),
                                );
                            }
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    let report =
        serde_json::json!({"cases":cases,"maximum_channel_difference":maximum,"passed":maximum<=1});
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/color-reuse-verification.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .ok();
    if maximum > 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Cached color pixels differ",
        ));
    }
    Ok(())
}
