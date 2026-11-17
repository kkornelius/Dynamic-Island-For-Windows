//! Mirrored outlined eyes with gaze-aligned irises and small pupils; crying adds liquid without changing the eye shape.
use super::canvas::Canvas;
use crate::face::Pose;
#[allow(clippy::too_many_arguments)]
fn ellipse(c: &Canvas, x: f32, y: f32, rx: f32, ry: f32, color: u32, width: f32, fill: bool) {
    let k = 0.5522848;
    c.curve(
        (x + rx, y),
        &[
            [x + rx, y + ry * k, x + rx * k, y + ry, x, y + ry],
            [x - rx * k, y + ry, x - rx, y + ry * k, x - rx, y],
            [x - rx, y - ry * k, x - rx * k, y - ry, x, y - ry],
            [x + rx * k, y - ry, x + rx, y - ry * k, x + rx, y],
        ],
        color,
        width,
        fill,
        true,
    );
}
pub fn draw(c: &Canvas, pose: Pose, cx: f32, cy: f32) {
    let open = pose.openness;
    for side in [-1.0, 1.0] {
        let ex = cx + side * 17.0;
        let iris_x = ex + pose.look[0] * 3.7;
        let iris_y = cy + pose.look[1] * 3.7 * open;
        ellipse(
            c,
            iris_x,
            iris_y,
            7.1,
            7.1 * open.max(0.02),
            0x17191b,
            0.7,
            true,
        );
        ellipse(
            c,
            iris_x,
            iris_y,
            7.4,
            7.4 * open.max(0.02),
            0xd0d3d6,
            0.85,
            false,
        );
        ellipse(
            c,
            iris_x,
            iris_y,
            6.0,
            6.0 * open.max(0.02),
            0x777d82,
            0.55,
            false,
        );
        if open > 0.2 {
            ellipse(
                c,
                iris_x + pose.look[0] * 3.2,
                iris_y + pose.look[1] * 3.2 * open,
                1.65,
                1.65 * open,
                0xe9edef,
                0.7,
                false,
            );
        }
        for (scale, color, width) in [(1.0, 0xe1e5e8, 0.9), (0.88, 0xaab0b5, 0.6)] {
            let transform = |u: f32, v: f32| (ex + side * u * scale, cy + v * scale * open);
            let start = transform(-10.0, -17.0);
            let segments = [
                [-5.0, -16.0, 11.0, -11.0, 14.0, -1.0],
                [20.0, 12.0, 6.0, 18.0, -3.0, 13.0],
                [-15.0, 8.0, -15.0, -1.0, -10.0, -17.0],
            ]
            .map(|s| {
                let a = transform(s[0], s[1]);
                let b = transform(s[2], s[3]);
                let d = transform(s[4], s[5]);
                [a.0, a.1, b.0, b.1, d.0, d.1]
            });
            c.curve(start, &segments, color, width, false, true);
        }
        if pose.crying > 0.0 && open > 0.15 {
            // Fixed geometry fades to exact zero, so retreat never folds a tear into the eyelid.
            let color = ((pose.crying * 235.0).round() as u32) * 0x010101;
            let wave = (pose.tear_phase * std::f32::consts::TAU + side * 0.5).sin() * 0.7;
            let y = cy + 6.4 * open;
            c.curve(
                (ex - 8.0, y),
                &[
                    [
                        ex - 5.0,
                        y - 1.8 + wave,
                        ex - 1.0,
                        y + 1.4 - wave,
                        ex + 2.0,
                        y,
                    ],
                    [
                        ex + 5.0,
                        y - 1.2 + wave,
                        ex + 7.0,
                        y - 1.1,
                        ex + 9.0,
                        y - 1.7,
                    ],
                ],
                color,
                0.9,
                false,
                false,
            );
            c.curve(
                (ex - 8.0, y),
                &[[ex - 7.0, y + 4.0, ex + 5.0, y + 4.5, ex + 9.0, y - 1.7]],
                color,
                0.65,
                false,
                false,
            );
            let phase = (pose.tear_phase + if side < 0.0 { 0.0 } else { 0.42 }).fract();
            let x = ex + side * 1.5;
            let drop_y = cy + 16.0 + phase * 7.0;
            let drop_color = ((pose.crying * (1.0 - phase) * 225.0).round() as u32) * 0x010101;
            c.curve(
                (x, drop_y - 2.0),
                &[
                    [
                        x - 0.4,
                        drop_y - 0.5,
                        x - 2.2,
                        drop_y + 1.2,
                        x - 1.4,
                        drop_y + 2.0,
                    ],
                    [
                        x,
                        drop_y + 3.8,
                        x + 2.0,
                        drop_y + 2.4,
                        x + 1.3,
                        drop_y + 1.0,
                    ],
                    [
                        x + 0.6,
                        drop_y - 0.4,
                        x + 0.3,
                        drop_y - 1.0,
                        x,
                        drop_y - 2.0,
                    ],
                ],
                drop_color,
                0.8,
                false,
                true,
            );
        }
    }
}
