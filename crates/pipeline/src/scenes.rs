// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic inputs that expose specific faults (testing strategy §4, item 3): a ramp for banding, a
//! zone plate for the demosaic, hard edges for haloes, clipped highlights, a black frame and a white
//! frame, at sizes that are not multiples of the workgroup.
//!
//! They are **generated, never stored** (testing strategy §5), and built in **integer arithmetic** up to
//! the last division, so that a scene is the same bit for bit on every platform: a `sin` or a `cos` would
//! differ in its last digit between a C library and another, and a mosaic that differs by one count is
//! a golden render that differs by a level on one machine.

use crate::stages::{BayerPattern, Develop, Levels};

/// A scene as the camera would see it: linear RGB per pixel, where 1.0 is the sensor's white.
pub(crate) struct Scene {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<[f32; 3]>,
}

fn build(width: u32, height: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> Scene {
    let mut rgb = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            rgb.push(f(x, y));
        }
    }
    Scene { width, height, rgb }
}

/// A triangle wave of `phase` over `period`, from 0 to 1 and back.
fn triangle(phase: u32, period: u32) -> f32 {
    let p = phase % period;
    let half = period / 2;
    let v = if p < half { p } else { period - p };
    v as f32 / half as f32
}

/// A grey ramp from black to white along `x`: the scene for banding and for the transfer function.
pub(crate) fn ramp(width: u32, height: u32) -> Scene {
    let last = (width - 1).max(1) as f32;
    build(width, height, |x, _| {
        let v = x as f32 / last;
        [v, v, v]
    })
}

/// A zone plate: a pattern whose spatial frequency grows with the distance from the centre, a stress
/// test for demosaic aliasing. The phase is an integer quadratic form, each channel offset from the
/// others, so that the colour fringes of a bad interpolation show.
pub(crate) fn zone_plate(width: u32, height: u32) -> Scene {
    let (cx, cy) = (width / 2, height / 2);
    build(width, height, |x, y| {
        let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
        let phase = dx * dx + dy * dy;
        let wave = |offset: u32| 0.1 + 0.8 * triangle(phase * 3 + offset, 512);
        [wave(0), wave(97), wave(211)]
    })
}

/// Hard edges between flat colours, vertical and diagonal: where a bad interpolation makes haloes.
pub(crate) fn edges(width: u32, height: u32) -> Scene {
    const COLOURS: [[f32; 3]; 6] = [
        [0.9, 0.05, 0.05],
        [0.05, 0.8, 0.05],
        [0.05, 0.05, 0.9],
        [0.9, 0.9, 0.9],
        [0.02, 0.02, 0.02],
        [0.8, 0.6, 0.1],
    ];
    build(width, height, |x, y| {
        let band = ((x * 6) / width.max(1)) as usize;
        // A diagonal edge across the lower half.
        let below = y > height / 2 && x + (y - height / 2) > width / 2;
        COLOURS[(band + usize::from(below) * 3) % COLOURS.len()]
    })
}

/// A smooth gradient that goes past the sensor's white (1.0, 1.7 at the right): clipped highlights.
pub(crate) fn clipped(width: u32, height: u32) -> Scene {
    let last = (width - 1).max(1) as f32;
    build(width, height, |x, y| {
        let t = x as f32 / last;
        let tint = (y % 3) as f32 * 0.04;
        [0.2 + 1.5 * t, 0.18 + 1.45 * t + tint, 0.15 + 1.4 * t]
    })
}

/// One colour everywhere: a black frame, a white frame, a mid grey.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn flat(width: u32, height: u32, rgb: [f32; 3]) -> Scene {
    build(width, height, |_, _| rgb)
}

/// The sensor's mosaic of a scene: each photosite keeps the channel its pattern gives it, mapped into
/// counts between `black` and `white`, clipped at the sensor's limits.
pub(crate) fn mosaic_of(scene: &Scene, pattern: BayerPattern, black: f32, white: f32) -> Vec<u16> {
    let mut out = Vec::with_capacity(scene.rgb.len());
    for y in 0..scene.height {
        for x in 0..scene.width {
            let v = scene.rgb[(y * scene.width + x) as usize][pattern.colour_at(x, y)];
            let counts = black + v.clamp(0.0, 1.0) * (white - black);
            out.push(counts.round().clamp(0.0, 65535.0) as u16);
        }
    }
    out
}

/// The black and white levels of the synthetic sensor: a 14-bit sensor with a pedestal.
pub(crate) const BLACK: f32 = 512.0;
/// See [`BLACK`].
pub(crate) const WHITE: f32 = 15360.0;

/// The parameters of the chain for the scenes: a neutral camera (nothing to interpret), no white balance,
/// no exposure change, and the working-to-display matrix of the Rec.2020 working space.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn neutral(pattern: BayerPattern) -> Develop {
    Develop {
        levels: Levels::uniform(BLACK, WHITE),
        pattern,
        to_working: crate::colour::IDENTITY,
        exposure_ev: 0.0,
        to_display: crate::colour::working_to_display(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_is_the_same_every_time() {
        // The scenes are integer arithmetic up to the last division: two builds are bit for bit equal.
        let a = zone_plate(37, 23);
        let b = zone_plate(37, 23);
        assert_eq!(a.rgb, b.rgb);
    }

    #[test]
    fn the_ramp_goes_from_black_to_white_and_the_clipped_scene_goes_past_white() {
        let r = ramp(64, 4);
        assert_eq!(r.rgb[0], [0.0; 3]);
        assert_eq!(r.rgb[63], [1.0; 3]);
        let c = clipped(64, 4);
        assert!(c.rgb[63].iter().all(|&v| v > 1.0), "{:?}", c.rgb[63]);
    }

    #[test]
    fn the_mosaic_clips_at_the_sensor_limits() {
        let m = mosaic_of(&clipped(32, 4), BayerPattern::Rggb, BLACK, WHITE);
        assert!(m.iter().all(|&c| f32::from(c) <= WHITE));
        assert!(
            m.iter().any(|&c| f32::from(c) == WHITE),
            "some highlight reaches the white level"
        );
        let black = mosaic_of(&flat(8, 8, [0.0; 3]), BayerPattern::Rggb, BLACK, WHITE);
        assert!(black.iter().all(|&c| f32::from(c) == BLACK));
    }

    #[test]
    fn the_mosaic_keeps_the_channel_the_pattern_gives_each_photosite() {
        // Red 1.0, green 0.5, blue 0.0, in each of the four phases.
        let scene = flat(4, 4, [1.0, 0.5, 0.0]);
        for pattern in [
            BayerPattern::Rggb,
            BayerPattern::Grbg,
            BayerPattern::Gbrg,
            BayerPattern::Bggr,
        ] {
            let m = mosaic_of(&scene, pattern, 0.0, 1000.0);
            for y in 0..4 {
                for x in 0..4 {
                    let want = [1000, 500, 0][pattern.colour_at(x, y)];
                    assert_eq!(m[(y * 4 + x) as usize], want, "{pattern:?} at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn the_four_patterns_start_as_their_names_say() {
        use BayerPattern::*;
        // Row 0, then row 1, as colours 0 red, 1 green, 2 blue.
        let first_cell = |p: BayerPattern| {
            [
                [p.colour_at(0, 0), p.colour_at(1, 0)],
                [p.colour_at(0, 1), p.colour_at(1, 1)],
            ]
        };
        assert_eq!(first_cell(Rggb), [[0, 1], [1, 2]]);
        assert_eq!(first_cell(Grbg), [[1, 0], [2, 1]]);
        assert_eq!(first_cell(Gbrg), [[1, 2], [0, 1]]);
        assert_eq!(first_cell(Bggr), [[2, 1], [1, 0]]);
    }
}
