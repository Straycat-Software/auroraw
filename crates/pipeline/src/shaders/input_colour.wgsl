// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stage `input-colour`, the spine's camera-to-working step with the white balance folded in (design note
// 006 §3.3, §2.2): `camera-linear` RGB multiplied by one 3x3 matrix, `M · diag(multipliers)`, into
// `working-linear` RGB. White balance is a per-channel multiplication, so it folds exactly and this stage
// does the same work whatever the balance is. Nothing is clamped: the working space has no upper bound,
// and a result slightly below zero is kept.

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
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) {
        return;
    }
    let i = gid.y * p.width + gid.x;
    let c = src[i].xyz;
    dst[i] = vec4<f32>(dot(p.m0.xyz, c), dot(p.m1.xyz, c), dot(p.m2.xyz, c), 1.0);
}
