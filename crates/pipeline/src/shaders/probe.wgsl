// SPDX-License-Identifier: GPL-3.0-or-later
//
// The probe: the smallest shader that uses what every stage will use, so that the smoke test says
// whether an adapter can run this engine at all. A uniform block, a read-only and a read-write
// storage buffer, a 16 x 16 workgroup, a bounded loop over a clamped neighbourhood (the shape of a
// halo), f32 arithmetic, and an 8-bit RGBA word packed into a u32 (the shape of the display stage).
//
// It stays inside the portable subset of WGSL (design note 005 §5, layer 2): no write into a
// vector through a dynamic index, which DirectX's FXC compiler rejected in spike 1.

struct Params {
    width: u32,
    height: u32,
    scale: f32,
    bias: f32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) {
        return;
    }
    var sum = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let x = clamp(i32(gid.x) + dx, 0, i32(p.width) - 1);
            let y = clamp(i32(gid.y) + dy, 0, i32(p.height) - 1);
            sum += src[u32(y) * p.width + u32(x)];
        }
    }
    let v = clamp(sum / 9.0 * p.scale + p.bias, 0.0, 1.0);
    let q = u32(v * 255.0 + 0.5);
    dst[gid.y * p.width + gid.x] = q | (q << 8u) | (q << 16u) | (255u << 24u);
}
