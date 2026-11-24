//! Original vector illustrations: minimal monochrome glass hourglass and a closed reusable bottle.
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
    let round = |a: i32, b: i32, w: i32, h: i32, r: i32, top, bottom| {
        c.gradient(
            x + (a as f32 * k) as i32,
            y + (b as f32 * k) as i32,
            (w as f32 * k) as i32,
            (h as f32 * k) as i32,
            (r as f32 * k) as i32,
            top,
            bottom,
        );
    };
    let glass = [
        [29., 34., 35., 39., 46., 48.],
        [49., 50., 49., 52., 46., 55.],
        [35., 64., 29., 70., 29., 86.],
        [40., 88., 60., 88., 71., 86.],
        [71., 70., 65., 64., 54., 55.],
        [51., 52., 51., 50., 54., 48.],
        [65., 39., 71., 34., 71., 16.],
    ];
    curve((29., 16.), &glass, 0x191b1e, 1., true, true);
    curve((29., 16.), &glass, 0xb9bdc4, 1.4, false, true);
    curve(
        (35., 29.),
        &[
            [44., 31., 56., 31., 65., 29.],
            [63., 37., 55., 42., 50., 46.],
            [45., 42., 37., 37., 35., 29.],
        ],
        0xd5d8de,
        1.,
        true,
        true,
    );
    curve(
        (34., 80.),
        &[
            [41., 75., 45., 71., 50., 68.],
            [55., 71., 60., 75., 66., 80.],
            [56., 83., 44., 83., 34., 80.],
        ],
        0xaeb3bc,
        1.,
        true,
        true,
    );
    curve(
        (50., 50.),
        &[[50., 55., 50., 60., 50., 64.]],
        0xe8eaee,
        1.,
        false,
        false,
    );
    curve(
        (33., 21.),
        &[[33., 31., 36., 36., 41., 41.]],
        0xf0f1f3,
        1.,
        false,
        false,
    );
    curve(
        (59., 63.),
        &[[64., 69., 67., 73., 67., 81.]],
        0x737983,
        1.,
        false,
        false,
    );
    round(24, 10, 52, 6, 3, 0xf1f2f4, 0x8c919a);
    round(24, 86, 52, 6, 3, 0xd8dce2, 0x7f858e);
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
