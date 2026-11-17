//! Original vector illustrations: smooth glass hourglass and a closed reusable bottle.
use super::canvas::Canvas;
pub fn hourglass(c: &Canvas, x: i32, y: i32, size: i32) {
    if !c.visible([x - 2, y - 2, size + 4, size + 4]) {
        return;
    }
    let k = size as f32 / 100.;
    let curve = |start: (f32, f32), segments: &[[f32; 6]], color, width, fill, closed| {
        let points: Vec<_> = segments
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
            &points,
            color,
            width * k,
            fill,
            closed,
        );
    };
    let round = |a, b, w, h, r, top, bottom| {
        c.gradient(
            x + (a as f32 * k) as i32,
            y + (b as f32 * k) as i32,
            (w as f32 * k) as i32,
            (h as f32 * k) as i32,
            (r as f32 * k) as i32,
            top,
            bottom,
        )
    };
    round(17, 92, 68, 7, 4, 0x282828, 0x1a1a1a);
    curve(
        (26., 19.),
        &[
            [26., 40., 35., 42., 44., 51.],
            [48., 55., 48., 60., 44., 64.],
            [35., 72., 26., 75., 26., 91.],
            [37., 95., 63., 95., 74., 91.],
            [74., 75., 65., 72., 56., 64.],
            [52., 60., 52., 55., 56., 51.],
            [65., 42., 74., 40., 74., 19.],
        ],
        0x343434,
        1.,
        true,
        true,
    );
    curve(
        (26., 19.),
        &[
            [26., 40., 35., 42., 44., 51.],
            [48., 55., 48., 60., 44., 64.],
            [35., 72., 26., 75., 26., 91.],
            [37., 95., 63., 95., 74., 91.],
            [74., 75., 65., 72., 56., 64.],
            [52., 60., 52., 55., 56., 51.],
            [65., 42., 74., 40., 74., 19.],
        ],
        0xc7c7c7,
        1.8,
        false,
        true,
    );
    curve(
        (32., 31.),
        &[
            [41., 34., 59., 34., 68., 31.],
            [66., 41., 57., 46., 50., 52.],
            [43., 46., 34., 41., 32., 31.],
        ],
        0xd8bea3,
        1.,
        true,
        true,
    );
    curve(
        (32., 86.),
        &[
            [39., 80., 44., 75., 50., 73.],
            [56., 75., 62., 80., 68., 86.],
            [59., 90., 41., 90., 32., 86.],
        ],
        0xe5c8a5,
        1.,
        true,
        true,
    );
    curve(
        (34., 84.),
        &[
            [42., 81., 45., 77., 50., 76.],
            [54., 78., 60., 83., 65., 85.],
        ],
        0xf3ddbe,
        1.5,
        false,
        false,
    );
    curve(
        (50., 55.),
        &[[50., 60., 50., 63., 50., 67.]],
        0xe8d2b5,
        1.3,
        false,
        false,
    );
    curve(
        (31., 24.),
        &[[31., 36., 35., 42., 40., 46.]],
        0xe3e3e3,
        1.3,
        false,
        false,
    );
    curve(
        (59., 69.),
        &[[65., 75., 69., 78., 69., 86.]],
        0x929292,
        1.3,
        false,
        false,
    );
    round(17, 9, 66, 13, 7, 0xd1d1d1, 0x818181);
    round(17, 90, 66, 12, 6, 0xb5b5b5, 0x707070);
    round(23, 11, 54, 3, 2, 0xf0f0f0, 0xb9b9b9);
}
pub fn bottle(c: &Canvas, x: i32, y: i32, size: i32) {
    if !c.visible([x - 2, y - 2, size + 4, size + 4]) {
        return;
    }
    let k = size as f32 / 100.;
    let curve = |start: (f32, f32), segments: &[[f32; 6]], color, width, fill, closed| {
        let points: Vec<_> = segments
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
            &points,
            color,
            width * k,
            fill,
            closed,
        );
    };
    curve(
        (68., 21.),
        &[[89., 13., 97., 37., 79., 46.]],
        0x8a8a8a,
        5.3,
        false,
        false,
    );
    curve(
        (68., 21.),
        &[[87., 15., 92., 35., 79., 42.]],
        0xc2c2c2,
        1.4,
        false,
        false,
    );
    curve(
        (29., 28.),
        &[
            [29., 42., 12., 35., 12., 58.],
            [12., 77., 12., 106., 12., 116.],
            [12., 139., 82., 139., 82., 116.],
            [82., 106., 82., 77., 82., 58.],
            [82., 35., 65., 42., 65., 28.],
        ],
        0x343434,
        1.,
        true,
        true,
    );
    c.gradient(
        x + (13. * k) as i32,
        y + (51. * k) as i32,
        (68. * k) as i32,
        (80. * k) as i32,
        (20. * k) as i32,
        0x484848,
        0x252525,
    );
    curve(
        (29., 28.),
        &[
            [29., 42., 12., 35., 12., 58.],
            [12., 77., 12., 106., 12., 116.],
            [12., 139., 82., 139., 82., 116.],
            [82., 106., 82., 77., 82., 58.],
            [82., 35., 65., 42., 65., 28.],
        ],
        0xaeaeae,
        1.7,
        false,
        true,
    );
    curve(
        (24., 52.),
        &[[20., 58., 20., 80., 20., 106.]],
        0x808080,
        3.6,
        false,
        false,
    );
    c.gradient(
        x + (24. * k) as i32,
        y + (10. * k) as i32,
        (46. * k) as i32,
        (21. * k) as i32,
        (8. * k) as i32,
        0xc8c8c8,
        0x8a8a8a,
    );
    c.round(
        x + (29. * k) as i32,
        y + (13. * k) as i32,
        (36. * k) as i32,
        (3. * k) as i32,
        (2. * k) as i32,
        0xe1e1e1,
    );
    c.line(
        x + (27. * k) as i32,
        y + (28. * k) as i32,
        x + (67. * k) as i32,
        y + (28. * k) as i32,
        0x696969,
        2,
    );
    c.droplet(
        x + (35. * k) as i32,
        y + (71. * k) as i32,
        (25. * k) as i32,
        (33. * k) as i32,
        0xd0d0d0,
    );
    curve(
        (24., 123.),
        &[[40., 131., 60., 131., 73., 122.]],
        0x828282,
        1.4,
        false,
        false,
    );
}
