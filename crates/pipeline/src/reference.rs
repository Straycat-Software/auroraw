// SPDX-License-Identifier: GPL-3.0-or-later
//! The CPU reference of every stage (testing strategy §4, item 1): the same maths as the shader, written
//! for **clarity, not speed**. It is the oracle a shader is checked against on every adapter, **not a
//! fallback**: nothing renders with it (design note 005 §6, the CPU fallback is the same shaders on a
//! software adapter).
//!
//! The arithmetic is `f32` and the operations are in the same order as the WGSL, so that the differences
//! the smoke test sees are the graphics API's (fused multiply-adds, the precision of `pow`), not ours.

use crate::colour::Matrix3;
use crate::stages::{BayerPattern, Develop, Levels};

/// Stage `raw-linear`: counts to linear values, `(count - black) / (white - black)`, the black level a
/// repeating pattern.
pub(crate) fn levels(samples: &[u16], width: u32, height: u32, levels: &Levels) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len());
    for y in 0..height {
        for x in 0..width {
            let count = f32::from(samples[(y * width + x) as usize]);
            let black =
                levels.black[((y % levels.rows) * levels.cols + (x % levels.cols)) as usize];
            out.push((count - black) / (levels.white - black));
        }
    }
    out
}

/// A coordinate reflected into `0..n` as many times as it takes, as the shader does at the edges: the
/// pattern repeats every `2 (n - 1)` pixels. A single reflection would send `x + 3` on a two-pixel image
/// to the wrong parity, and so to the wrong colour.
fn mirror(i: i32, n: i32) -> i32 {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n - 1);
    let mut r = i % period;
    if r < 0 {
        r += period;
    }
    if r >= n {
        r = period - r;
    }
    r
}

/// Stage `demosaic` for a Bayer mosaic: gradient-corrected bilinear interpolation (Malvar, He and
/// Cutler), edges mirrored. **Nothing is clamped**: a noise-model denoiser needs the samples that noise
/// took below the black level, and cutting them here would raise the local mean of a dark area.
pub(crate) fn demosaic_bayer(
    mosaic: &[f32],
    width: u32,
    height: u32,
    pattern: BayerPattern,
) -> Vec<[f32; 3]> {
    let (w, h) = (width as i32, height as i32);
    let at = |x: i32, y: i32| mosaic[(mirror(y, h) * w + mirror(x, w)) as usize];
    let mut out = Vec::with_capacity(mosaic.len());
    for gy in 0..height {
        for gx in 0..width {
            let (x, y) = (gx as i32, gy as i32);
            let c = at(x, y);
            let (n, s, west, e) = (at(x, y - 1), at(x, y + 1), at(x - 1, y), at(x + 1, y));
            let (nn, ss, ww, ee) = (at(x, y - 2), at(x, y + 2), at(x - 2, y), at(x + 2, y));
            let (nw, ne, sw, se) = (
                at(x - 1, y - 1),
                at(x + 1, y - 1),
                at(x - 1, y + 1),
                at(x + 1, y + 1),
            );
            let g_at_rb = (4.0 * c + 2.0 * (n + s + e + west) - (nn + ss + ee + ww)) / 8.0;
            let diagonal = nw + ne + sw + se;
            let rgb = match pattern.colour_at(gx, gy) {
                // A red or a blue site: which one, and which green row, is the pattern's business.
                0 => {
                    let b = (6.0 * c + 2.0 * diagonal - 1.5 * (nn + ss + ee + ww)) / 8.0;
                    [c, g_at_rb, b]
                }
                2 => {
                    let r = (6.0 * c + 2.0 * diagonal - 1.5 * (nn + ss + ee + ww)) / 8.0;
                    [r, g_at_rb, c]
                }
                _ => {
                    // A green site: in a red row (the pattern's `px == 1, py == 0`) or a blue row.
                    let px = (gx ^ pattern.flip()) & 1;
                    let in_red_row = px == 1;
                    if in_red_row {
                        let r = (5.0 * c + 4.0 * (west + e) - diagonal - (ww + ee)
                            + 0.5 * (nn + ss))
                            / 8.0;
                        let b = (5.0 * c + 4.0 * (n + s) - diagonal - (nn + ss) + 0.5 * (ww + ee))
                            / 8.0;
                        [r, c, b]
                    } else {
                        let r = (5.0 * c + 4.0 * (n + s) - diagonal - (nn + ss) + 0.5 * (ww + ee))
                            / 8.0;
                        let b = (5.0 * c + 4.0 * (west + e) - diagonal - (ww + ee)
                            + 0.5 * (nn + ss))
                            / 8.0;
                        [r, c, b]
                    }
                }
            };
            out.push(rgb);
        }
    }
    out
}

/// A 3x3 matrix applied to each pixel: the `input-colour` stage, and the first half of the output.
pub(crate) fn apply_matrix(pixels: &[[f32; 3]], m: &Matrix3) -> Vec<[f32; 3]> {
    pixels
        .iter()
        .map(|c| std::array::from_fn(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2]))
        .collect()
}

/// Operation `exposure`: a gain of `2^ev`.
pub(crate) fn exposure(pixels: &[[f32; 3]], ev: f32) -> Vec<[f32; 3]> {
    let gain = ev.exp2();
    pixels.iter().map(|c| c.map(|v| v * gain)).collect()
}

/// The sRGB transfer function (IEC 61966-2-1) of a value in `0..=1`.
fn srgb_oetf(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Stage `display`: working-space RGB to linear display RGB by a matrix, clamped, encoded, and packed
/// as 8-bit RGBA words (red in the low byte, alpha opaque).
pub(crate) fn output(pixels: &[[f32; 3]], to_display: &Matrix3) -> Vec<u32> {
    apply_matrix(pixels, to_display)
        .iter()
        .map(|d| {
            let q = d.map(|v| (srgb_oetf(v.clamp(0.0, 1.0)).clamp(0.0, 1.0) * 255.0 + 0.5) as u32);
            q[0] | (q[1] << 8) | (q[2] << 16) | (255 << 24)
        })
        .collect()
}

/// The whole chain: counts to 8-bit sRGB.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn develop(samples: &[u16], width: u32, height: u32, p: &Develop) -> Vec<u32> {
    let mosaic = levels(samples, width, height, &p.levels);
    let camera = demosaic_bayer(&mosaic, width, height, p.pattern);
    let working = apply_matrix(&camera, &p.to_working);
    let exposed = exposure(&working, p.exposure_ev);
    output(&exposed, &p.to_display)
}
