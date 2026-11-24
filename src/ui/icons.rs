//! Small vectors share one visual weight; no font-dependent symbol glyphs.
use super::canvas::Canvas;
use crate::bluetooth::DeviceKind;
#[derive(Clone, Copy)]
pub enum Icon {
    Music,
    Weather,
    Timer,
    Water,
    Notes,
    Settings,
    Lyrics,
    Play,
    Pause,
    Previous,
    Next,
    Earbuds,
    Keyboard,
    Mouse,
    Speaker,
    Phone,
    Controller,
    Bluetooth,
    Close,
    Moon,
}
pub fn device(kind: DeviceKind) -> Icon {
    match kind {
        DeviceKind::Earbuds => Icon::Earbuds,
        DeviceKind::Keyboard => Icon::Keyboard,
        DeviceKind::Mouse => Icon::Mouse,
        DeviceKind::Speaker => Icon::Speaker,
        DeviceKind::Phone => Icon::Phone,
        DeviceKind::Controller => Icon::Controller,
        DeviceKind::Unknown => Icon::Bluetooth,
    }
}
pub fn draw(c: &Canvas, icon: Icon, x: i32, y: i32, size: i32, color: u32) {
    if !c.visible([x - 2, y - 2, size + 4, size + 4]) {
        return;
    }
    if super::icon_assets::draw(c, icon, x, y, size, color) {
        return;
    }
    let k = size as f32 / 24.0;
    let path = |points: &[(f32, f32)], closed, fill| {
        let pts: Vec<_> = points
            .iter()
            .map(|(a, b)| (x as f32 + a * k, y as f32 + b * k))
            .collect();
        c.path(&pts, color, 1.65 * k, fill, closed);
    };
    let line = |a, b, d, e| path(&[(a, b), (d, e)], false, false);
    let round = |a: f32, b: f32, w: f32, h: f32, r: f32| {
        c.outline(
            x + (a * k) as i32,
            y + (b * k) as i32,
            (w * k) as i32,
            (h * k) as i32,
            (r * k) as i32,
            color,
            1.6 * k,
        )
    };
    let curve = |start: (f32, f32), segments: &[[f32; 6]], fill, closed| {
        let seg: Vec<_> = segments
            .iter()
            .map(|s| {
                [
                    x as f32 + s[0] * k,
                    y as f32 + s[1] * k,
                    x as f32 + s[2] * k,
                    y as f32 + s[3] * k,
                    x as f32 + s[4] * k,
                    y as f32 + s[5] * k,
                ]
            })
            .collect();
        c.curve(
            (x as f32 + start.0 * k, y as f32 + start.1 * k),
            &seg,
            color,
            1.6 * k,
            fill,
            closed,
        );
    };
    match icon {
        Icon::Notes => {
            round(4., 3., 16., 18., 3.);
            line(8., 8., 16., 8.);
            line(8., 12., 16., 12.);
            line(8., 16., 13., 16.);
        }
        Icon::Music => {
            path(&[(9., 17.), (9., 5.), (19., 3.), (19., 15.)], false, false);
            c.circle(
                x + (3. * k) as i32,
                y + (15. * k) as i32,
                (6. * k) as i32,
                color,
            );
            c.circle(
                x + (13. * k) as i32,
                y + (13. * k) as i32,
                (6. * k) as i32,
                color,
            );
            line(9., 8., 19., 6.);
        }
        Icon::Weather => {
            c.circle(
                x + (12. * k) as i32,
                y + (3. * k) as i32,
                (8. * k) as i32,
                color,
            );
            c.round(
                x + (3. * k) as i32,
                y + (12. * k) as i32,
                (18. * k) as i32,
                (8. * k) as i32,
                (4. * k) as i32,
                color,
            );
            c.circle(
                x + (7. * k) as i32,
                y + (8. * k) as i32,
                (10. * k) as i32,
                color,
            );
            line(16., 0., 16., 1.);
            line(22., 4., 24., 3.);
        }
        Icon::Timer => {
            round(5., 1., 14., 3., 1.5);
            round(5., 20., 14., 3., 1.5);
            curve(
                (7., 4.),
                &[
                    [7., 9., 10., 9., 11., 12.],
                    [10., 15., 7., 15., 7., 20.],
                    [10., 21., 14., 21., 17., 20.],
                    [17., 15., 14., 15., 13., 12.],
                    [14., 9., 17., 9., 17., 4.],
                ],
                false,
                true,
            );
            curve(
                (9., 7.),
                &[
                    [11., 8., 13., 8., 15., 7.],
                    [14., 9., 13., 10., 12., 11.],
                    [11., 10., 10., 9., 9., 7.],
                ],
                true,
                true,
            );
            curve(
                (9., 18.),
                &[
                    [10., 17., 11., 16., 12., 15.],
                    [13., 16., 14., 17., 15., 18.],
                    [13., 19., 11., 19., 9., 18.],
                ],
                true,
                true,
            );
        }
        Icon::Water => {
            round(7., 1., 10., 4., 2.);
            curve(
                (8., 5.),
                &[
                    [8., 8., 4., 7., 4., 11.],
                    [4., 15., 4., 18., 4., 20.],
                    [4., 24., 20., 24., 20., 20.],
                    [20., 18., 20., 15., 20., 11.],
                    [20., 7., 16., 8., 16., 5.],
                ],
                false,
                true,
            );
            curve((18., 3.), &[[23., 2., 25., 8., 21., 10.]], false, false);
            c.droplet(
                x + 9 * size / 24,
                y + 10 * size / 24,
                6 * size / 24,
                9 * size / 24,
                color,
            );
        }
        Icon::Settings => {
            let points: Vec<_> = (0..=80)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 80.;
                    let r = 8.1 + 2.0 * ((a * 8.).cos() + 1.) / 2.;
                    (12. + a.cos() * r, 12. + a.sin() * r)
                })
                .collect();
            path(&points, true, false);
            round(8.5, 8.5, 7., 7., 3.5);
        }
        Icon::Lyrics => {
            round(3., 4., 18., 14., 4.);
            path(&[(6., 18.), (6., 22.), (11., 18.)], false, false);
            line(7., 9., 17., 9.);
            line(7., 13., 14., 13.);
        }
        Icon::Play => path(&[(8., 4.), (20., 12.), (8., 20.)], true, true),
        Icon::Pause => {
            c.round(
                x + 6 * size / 24,
                y + 4 * size / 24,
                4 * size / 24,
                16 * size / 24,
                1,
                color,
            );
            c.round(
                x + 14 * size / 24,
                y + 4 * size / 24,
                4 * size / 24,
                16 * size / 24,
                1,
                color,
            );
        }
        Icon::Previous => {
            path(&[(17., 5.), (7., 12.), (17., 19.)], true, true);
            line(5., 5., 5., 19.);
        }
        Icon::Next => {
            path(&[(7., 5.), (17., 12.), (7., 19.)], true, true);
            line(19., 5., 19., 19.);
        }
        Icon::Earbuds => {
            round(3., 4., 7., 9., 3.);
            round(14., 4., 7., 9., 3.);
            round(6., 11., 3., 10., 1.);
            round(15., 11., 3., 10., 1.);
        }
        Icon::Keyboard => {
            round(1., 5., 22., 14., 3.);
            for row in 0..2 {
                for col in 0..5 {
                    c.circle(
                        x + (4 + col * 4) * size / 24,
                        y + (8 + row * 4) * size / 24,
                        (2. * k).max(1.) as i32,
                        color,
                    );
                }
            }
            line(7., 16., 17., 16.);
        }
        Icon::Mouse => {
            round(5., 1., 14., 22., 7.);
            line(12., 3., 12., 9.);
            line(6., 10., 18., 10.);
        }
        Icon::Speaker => {
            round(5., 1., 14., 22., 4.);
            round(8., 10., 8., 8., 4.);
            round(10., 4., 4., 4., 2.);
        }
        Icon::Phone => {
            round(6., 1., 12., 22., 3.);
            line(10., 19., 14., 19.);
        }
        Icon::Controller => {
            round(1., 7., 22., 13., 5.);
            line(5., 13., 11., 13.);
            line(8., 10., 8., 16.);
            c.circle(x + 16 * size / 24, y + 11 * size / 24, 2 * size / 24, color);
            c.circle(x + 19 * size / 24, y + 14 * size / 24, 2 * size / 24, color);
        }
        Icon::Bluetooth => {
            path(
                &[
                    (7., 6.),
                    (17., 16.),
                    (12., 22.),
                    (12., 2.),
                    (17., 7.),
                    (7., 18.),
                ],
                false,
                false,
            );
        }
        Icon::Close => {
            line(7., 7., 17., 17.);
            line(17., 7., 7., 17.);
        }
        Icon::Moon => {
            path(
                &[
                    (17., 3.),
                    (12., 4.),
                    (10., 9.),
                    (11., 14.),
                    (16., 17.),
                    (21., 16.),
                    (18., 21.),
                    (11., 22.),
                    (5., 18.),
                    (2., 12.),
                    (4., 6.),
                    (10., 2.),
                    (17., 3.),
                ],
                true,
                false,
            );
        }
    }
}
pub fn weather(c: &Canvas, code: u16, day: bool, x: i32, y: i32, size: i32) {
    if !c.visible([x - 2, y - 2, size + 4, size + 4]) {
        return;
    }
    let k = size as f32 / 64.;
    let (sun, cloud) = (matches!(code, 0..=2), code != 0);
    if sun {
        if day {
            c.circle(
                x + (28. * k) as i32,
                y + (6. * k) as i32,
                (29. * k) as i32,
                0xf1d38d,
            );
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::PI / 4.;
                let (s, t) = a.sin_cos();
                c.path(
                    &[
                        (
                            x as f32 + (42. + s * 20.) * k,
                            y as f32 + (20. + t * 20.) * k,
                        ),
                        (
                            x as f32 + (42. + s * 24.) * k,
                            y as f32 + (20. + t * 24.) * k,
                        ),
                    ],
                    0xe0bd78,
                    2. * k,
                    false,
                    false,
                );
            }
        } else {
            draw(c, Icon::Moon, x + size / 3, y, size / 2, 0xcac4ed);
        }
    }
    if cloud {
        c.round(
            x + (5. * k) as i32,
            y + (29. * k) as i32,
            (54. * k) as i32,
            (22. * k) as i32,
            (11. * k) as i32,
            0xc7d8eb,
        );
        c.circle(
            x + (15. * k) as i32,
            y + (18. * k) as i32,
            (30. * k) as i32,
            0xdce7f3,
        );
        c.circle(
            x + (32. * k) as i32,
            y + (24. * k) as i32,
            (23. * k) as i32,
            0xdce7f3,
        );
        c.circle(
            x + (25. * k) as i32,
            y + (39. * k) as i32,
            (3. * k).max(1.) as i32,
            0x677d9b,
        );
        c.circle(
            x + (38. * k) as i32,
            y + (39. * k) as i32,
            (3. * k).max(1.) as i32,
            0x677d9b,
        );
    }
    if matches!(code,51..=67|80..=82|95..=99) {
        for a in [18., 31., 44.] {
            c.path(
                &[
                    (x as f32 + a * k, y as f32 + 55. * k),
                    (x as f32 + (a - 3.) * k, y as f32 + 61. * k),
                ],
                0x83bfe0,
                2. * k,
                false,
                false,
            );
        }
    }
    if matches!(code, 71..=77 | 85 | 86) {
        for a in [20, 32, 44] {
            c.circle(
                x + a * size / 64,
                y + 56 * size / 64,
                (4 * size / 64).max(1),
                0xe2edfa,
            );
        }
    }
}
