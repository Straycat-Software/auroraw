// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stage `scene-linear`, the operation `exposure` (design note 006 §3.3): a gain on the scene-linear
// values, `2^EV` computed by the caller. It is a multiplication in the working space, so it commutes with
// the other linear operations and with none that is not linear, and it is applied exactly where the recipe
// puts it.

struct Params {
    width: u32,
    height: u32,
    gain: f32,
    pad0: f32,
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
    dst[i] = vec4<f32>(src[i].xyz * p.gain, 1.0);
}
