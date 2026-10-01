// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stage `display`, the spine's output transform for a screen (design note 006 §3.3, §6): the
// `working-linear` image to linear sRGB by one matrix, clamped to 0..1, the sRGB transfer function, and
// 8 bits per channel packed as RGBA (alpha opaque) in one u32. This is **the flat linear look**: the
// working-space data under the output transform and nothing else. The base look's `tone map` is an
// operation that comes before it, and its constants are not chosen yet (note 006 §6).

struct Params {
    width: u32,
    height: u32,
    pad0: u32,
    pad1: u32,
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

fn srgb_oetf(l: vec3<f32>) -> vec3<f32> {
    let lo = l * 12.92;
    let hi = 1.055 * pow(max(l, vec3<f32>(0.0031308)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, l <= vec3<f32>(0.0031308));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) {
        return;
    }
    let i = gid.y * p.width + gid.x;
    let w = src[i].xyz;
    let d = vec3<f32>(dot(p.m0.xyz, w), dot(p.m1.xyz, w), dot(p.m2.xyz, w));
    let enc = srgb_oetf(clamp(d, vec3<f32>(0.0), vec3<f32>(1.0)));
    let q = vec3<u32>(clamp(enc, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5);
    dst[i] = q.x | (q.y << 8u) | (q.z << 16u) | (255u << 24u);
}
