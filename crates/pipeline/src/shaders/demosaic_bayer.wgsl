// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stage `demosaic`, for a 2x2 Bayer mosaic (design note 006 §3.3): gradient-corrected bilinear
// interpolation (Malvar, He and Cutler, 2004) of `mosaic-linear` into `camera-linear` RGB, the method
// spike 1 measured (20 to 60 MP in 24 to 63 ms). Edges are mirrored. A negative result is clamped to
// zero (an interpolation artefact, not a signal); there is no upper bound.
//
// `flip` says which of the four phases the pattern has at the image's origin, relative to RGGB: bit 0
// swaps the column parity, bit 1 the row parity (RGGB 0, GRBG 1, GBRG 2, BGGR 3).
//
// Portable subset: the interpolated site is chosen by comparisons, the colour is built as a whole vec3
// and never written one component at a time.

struct Params {
    width: u32,
    height: u32,
    flip: u32,
    pad0: u32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mosaic: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<vec4<f32>>;

fn mirror(i: i32, n: i32) -> i32 {
    var r = i;
    if (r < 0) {
        r = -r;
    }
    if (r >= n) {
        r = 2 * n - 2 - r;
    }
    return clamp(r, 0, n - 1);
}

fn at(x: i32, y: i32) -> f32 {
    let ix = mirror(x, i32(p.width));
    let iy = mirror(y, i32(p.height));
    return mosaic[u32(iy) * p.width + u32(ix)];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) {
        return;
    }
    let x = i32(gid.x);
    let y = i32(gid.y);
    let c = at(x, y);
    let n = at(x, y - 1);
    let s = at(x, y + 1);
    let w = at(x - 1, y);
    let e = at(x + 1, y);
    let nn = at(x, y - 2);
    let ss = at(x, y + 2);
    let ww = at(x - 2, y);
    let ee = at(x + 2, y);
    let nw = at(x - 1, y - 1);
    let ne = at(x + 1, y - 1);
    let sw = at(x - 1, y + 1);
    let se = at(x + 1, y + 1);

    let px = (gid.x ^ p.flip) & 1u;
    let py = (gid.y ^ (p.flip >> 1u)) & 1u;
    var rgb = vec3<f32>(0.0);
    // Green interpolated at a red or blue site.
    let g_at_rb = (4.0 * c + 2.0 * (n + s + e + w) - (nn + ss + ee + ww)) / 8.0;
    if (px == 0u && py == 0u) {
        // A red site.
        let b_here = (6.0 * c + 2.0 * (nw + ne + sw + se) - 1.5 * (nn + ss + ee + ww)) / 8.0;
        rgb = vec3<f32>(c, g_at_rb, b_here);
    } else if (px == 1u && py == 1u) {
        // A blue site.
        let r_here = (6.0 * c + 2.0 * (nw + ne + sw + se) - 1.5 * (nn + ss + ee + ww)) / 8.0;
        rgb = vec3<f32>(r_here, g_at_rb, c);
    } else if (px == 1u && py == 0u) {
        // A green site in a red row.
        let r_here = (5.0 * c + 4.0 * (w + e) - (nw + ne + sw + se) - (ww + ee) + 0.5 * (nn + ss)) / 8.0;
        let b_here = (5.0 * c + 4.0 * (n + s) - (nw + ne + sw + se) - (nn + ss) + 0.5 * (ww + ee)) / 8.0;
        rgb = vec3<f32>(r_here, c, b_here);
    } else {
        // A green site in a blue row.
        let r_here = (5.0 * c + 4.0 * (n + s) - (nw + ne + sw + se) - (nn + ss) + 0.5 * (ww + ee)) / 8.0;
        let b_here = (5.0 * c + 4.0 * (w + e) - (nw + ne + sw + se) - (ww + ee) + 0.5 * (nn + ss)) / 8.0;
        rgb = vec3<f32>(r_here, c, b_here);
    }
    out[gid.y * p.width + gid.x] = vec4<f32>(max(rgb, vec3<f32>(0.0)), 1.0);
}
