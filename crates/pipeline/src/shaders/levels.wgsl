// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stage `raw-linear`, the spine's first step (design note 006 §3.3): the decoder's samples (`sensor-raw`,
// u16 counts, two per word) become one linear value per photosite (`mosaic-linear`), the black level
// subtracted and the white level mapped to 1.0. The black level is a repeating pattern of `rows` x `cols`
// values, as the decoder gives it (D-141); the white level is one value. Nothing is clamped: a sample
// below its black is a negative value, which the demosaic and the denoiser may use.
//
// Portable subset: no write into a vector through a dynamic index (FXC rejected one in spike 1); the
// black pattern is read from a storage buffer by a scalar index.

struct Params {
    width: u32,
    height: u32,
    rows: u32,
    cols: u32,
    white: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> raw: array<u32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<storage, read> black: array<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) {
        return;
    }
    let idx = gid.y * p.width + gid.x;
    let word = raw[idx >> 1u];
    let count = (word >> ((idx & 1u) * 16u)) & 0xffffu;
    let b = black[(gid.y % p.rows) * p.cols + (gid.x % p.cols)];
    out[idx] = (f32(count) - b) / (p.white - b);
}
