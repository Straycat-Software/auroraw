// SPDX-License-Identifier: GPL-3.0-or-later
//! The first stages of the chain, run on the GPU (design note 006 §3.3): the levels, the Bayer
//! demosaic, the camera-to-working step with the white balance folded in, the exposure, and the output
//! transform to 8 bits for a screen.
//!
//! These are the stages' **kernels and their parameters**, run one after another on whole images: the
//! render API (recipes, views, bands, caches) is the next work package, and with it banding, since a
//! whole-image buffer of `camera-linear` data is 16 bytes a pixel and the engine's floor binding limit is
//! 128 MiB ([`crate::EngineLimits::FLOOR`]), which holds about 8 million pixels. A larger image is refused
//! here with [`GpuError::TooLarge`], not rendered wrongly.
//!
//! Every stage has a CPU reference in [`crate::reference`] and is checked against it by the smoke test
//! on every adapter (testing strategy §4, items 1 and 4).

use crate::colour::Matrix3;
use crate::gpu::{Gpu, GpuError, Kernel, KernelError};

/// The shaders of the stages, as WGSL text. Each is registered in [`crate::smoke::SHADERS`], and a test
/// fails if a `.wgsl` file is not.
pub(crate) const LEVELS_WGSL: &str = include_str!("shaders/levels.wgsl");
pub(crate) const DEMOSAIC_BAYER_WGSL: &str = include_str!("shaders/demosaic_bayer.wgsl");
pub(crate) const INPUT_COLOUR_WGSL: &str = include_str!("shaders/input_colour.wgsl");
pub(crate) const EXPOSURE_WGSL: &str = include_str!("shaders/exposure.wgsl");
pub(crate) const OUTPUT_WGSL: &str = include_str!("shaders/output.wgsl");

/// The black and white levels of a mosaic, as the decoder gives them (D-141): the black level is a
/// repeating pattern of `rows` by `cols` values, the white level one value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Levels {
    pub(crate) rows: u32,
    pub(crate) cols: u32,
    /// `rows * cols` values, row-major.
    pub(crate) black: Vec<f32>,
    pub(crate) white: f32,
}

impl Levels {
    /// One black level for the whole mosaic.
    pub(crate) fn uniform(black: f32, white: f32) -> Levels {
        Levels {
            rows: 1,
            cols: 1,
            black: vec![black],
            white,
        }
    }

    /// Whether the pattern is well formed: at least one value and as many as it says.
    pub(crate) fn is_valid(&self) -> bool {
        self.rows >= 1 && self.cols >= 1 && self.black.len() == (self.rows * self.cols) as usize
    }
}

/// Which of the four phases a 2x2 Bayer pattern has at the image's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BayerPattern {
    /// Red, green / green, blue.
    Rggb,
    /// Green, red / blue, green.
    Grbg,
    /// Green, blue / red, green.
    Gbrg,
    /// Blue, green / green, red.
    Bggr,
}

impl BayerPattern {
    /// The `flip` the demosaic shader takes: bit 0 swaps the column parity, bit 1 the row parity,
    /// relative to RGGB.
    pub(crate) fn flip(self) -> u32 {
        match self {
            BayerPattern::Rggb => 0,
            BayerPattern::Grbg => 1,
            BayerPattern::Gbrg => 2,
            BayerPattern::Bggr => 3,
        }
    }

    /// The colour (0 red, 1 green, 2 blue) of the photosite at `(x, y)`.
    pub(crate) fn colour_at(self, x: u32, y: u32) -> usize {
        let px = (x ^ self.flip()) & 1;
        let py = (y ^ (self.flip() >> 1)) & 1;
        match (px, py) {
            (0, 0) => 0,
            (1, 1) => 2,
            _ => 1,
        }
    }
}

/// The parameters of the whole chain of stages.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Develop {
    pub(crate) levels: Levels,
    pub(crate) pattern: BayerPattern,
    /// Camera RGB to working space, with the white balance already folded in
    /// ([`crate::colour::fold_white_balance`]).
    pub(crate) to_working: Matrix3,
    /// The exposure, in stops: the gain is `2^exposure_ev`.
    pub(crate) exposure_ev: f32,
    /// Working space to linear display RGB ([`crate::colour::working_to_display`]).
    pub(crate) to_display: Matrix3,
}

/// A uniform block under construction, laid out as WGSL lays out a struct of 32-bit scalars and `vec4`s
/// (a `vec4` starts on a multiple of 16 bytes; the block ends on one).
pub(crate) struct Block(Vec<u8>);

impl Block {
    pub(crate) fn new() -> Block {
        Block(Vec::new())
    }

    pub(crate) fn u32(mut self, v: u32) -> Block {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub(crate) fn f32(mut self, v: f32) -> Block {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// A `vec4<f32>`, which WGSL aligns to 16 bytes.
    pub(crate) fn vec4(mut self, v: [f32; 4]) -> Block {
        assert!(
            self.0.len().is_multiple_of(16),
            "a vec4 must start on a 16-byte boundary"
        );
        for c in v {
            self.0.extend_from_slice(&c.to_le_bytes());
        }
        self
    }

    /// A matrix as three `vec4` rows (the fourth component unused).
    pub(crate) fn matrix(self, m: &Matrix3) -> Block {
        self.vec4([m[0][0], m[0][1], m[0][2], 0.0])
            .vec4([m[1][0], m[1][1], m[1][2], 0.0])
            .vec4([m[2][0], m[2][1], m[2][2], 0.0])
    }

    /// The bytes, padded with zeros to a multiple of 16.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        while !self.0.len().is_multiple_of(16) {
            self.0.push(0);
        }
        self.0
    }
}

/// Two 16-bit samples per 32-bit word, the first in the low half: the layout of the `sensor-raw` buffer.
pub(crate) fn pack_samples(samples: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len().div_ceil(2) * 4);
    for pair in samples.chunks(2) {
        let word = u32::from(pair[0]) | (u32::from(*pair.get(1).unwrap_or(&0)) << 16);
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn f32_from(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// RGB triples as the `vec4` buffer the stages read (alpha 1).
fn rgb_bytes(pixels: &[[f32; 3]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 1.0])
        .flat_map(f32::to_le_bytes)
        .collect()
}

fn rgb_from(bytes: &[u8]) -> Vec<[f32; 3]> {
    f32_from(bytes)
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2]])
        .collect()
}

/// The compiled kernels of the stages.
pub(crate) struct Kernels {
    levels: Kernel,
    demosaic: Kernel,
    input_colour: Kernel,
    exposure: Kernel,
    output: Kernel,
}

/// A storage buffer the stages can bind: refused if it is larger than the engine's binding limit.
fn storage(gpu: &Gpu, len: u64, usage: wgpu::BufferUsages) -> Result<wgpu::Buffer, GpuError> {
    let max = gpu.limits.max_storage_binding_size;
    if len > max {
        return Err(GpuError::TooLarge { size: len, max });
    }
    gpu.create_buffer(len.max(4), wgpu::BufferUsages::STORAGE | usage)
}

/// A storage buffer holding `bytes`, for a stage to read.
fn storage_with(gpu: &Gpu, bytes: &[u8]) -> Result<wgpu::Buffer, GpuError> {
    let max = gpu.limits.max_storage_binding_size;
    if bytes.len() as u64 > max {
        return Err(GpuError::TooLarge {
            size: bytes.len() as u64,
            max,
        });
    }
    gpu.upload(bytes, wgpu::BufferUsages::STORAGE)
}

fn out_buffer(gpu: &Gpu, len: u64) -> Result<wgpu::Buffer, GpuError> {
    storage(gpu, len, wgpu::BufferUsages::COPY_SRC)
}

impl Kernels {
    /// Compiles every stage's shader on this device.
    pub(crate) fn compile(gpu: &Gpu) -> Result<Kernels, KernelError> {
        Ok(Kernels {
            levels: gpu.compile("levels", LEVELS_WGSL)?,
            demosaic: gpu.compile("demosaic_bayer", DEMOSAIC_BAYER_WGSL)?,
            input_colour: gpu.compile("input_colour", INPUT_COLOUR_WGSL)?,
            exposure: gpu.compile("exposure", EXPOSURE_WGSL)?,
            output: gpu.compile("output", OUTPUT_WGSL)?,
        })
    }
}

/// The number of samples a `width` by `height` image has, or an error if the sizes overflow.
fn pixels(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height)
}

// ---- the stages, one at a time: each uploads its input, runs, and reads its output back ----

/// Stage `raw-linear`: counts to linear values.
pub(crate) fn run_levels(
    gpu: &Gpu,
    kernels: &Kernels,
    samples: &[u16],
    width: u32,
    height: u32,
    levels: &Levels,
) -> Result<Vec<f32>, GpuError> {
    assert!(
        levels.is_valid(),
        "a black level pattern with the wrong number of values"
    );
    let raw = storage_with(gpu, &pack_samples(samples))?;
    let black = storage_with(gpu, &f32_bytes(&levels.black))?;
    let out = out_buffer(gpu, pixels(width, height) * 4)?;
    levels_pass(gpu, kernels, &raw, &black, &out, width, height, levels)?;
    gpu.wait()?;
    Ok(f32_from(&gpu.read_back(&out, pixels(width, height) * 4)?))
}

#[allow(clippy::too_many_arguments)]
fn levels_pass(
    gpu: &Gpu,
    kernels: &Kernels,
    raw: &wgpu::Buffer,
    black: &wgpu::Buffer,
    out: &wgpu::Buffer,
    width: u32,
    height: u32,
    levels: &Levels,
) -> Result<(), GpuError> {
    let params = Block::new()
        .u32(width)
        .u32(height)
        .u32(levels.rows)
        .u32(levels.cols)
        .f32(levels.white)
        .f32(0.0)
        .f32(0.0)
        .f32(0.0)
        .finish();
    gpu.dispatch(&kernels.levels, &params, &[raw, out, black], width, height)
}

/// Stage `demosaic`: a Bayer mosaic of linear values to camera RGB.
pub(crate) fn run_demosaic(
    gpu: &Gpu,
    kernels: &Kernels,
    mosaic: &[f32],
    width: u32,
    height: u32,
    pattern: BayerPattern,
) -> Result<Vec<[f32; 3]>, GpuError> {
    let input = storage_with(gpu, &f32_bytes(mosaic))?;
    let out = out_buffer(gpu, pixels(width, height) * 16)?;
    demosaic_pass(gpu, kernels, &input, &out, width, height, pattern)?;
    gpu.wait()?;
    Ok(rgb_from(&gpu.read_back(&out, pixels(width, height) * 16)?))
}

fn demosaic_pass(
    gpu: &Gpu,
    kernels: &Kernels,
    input: &wgpu::Buffer,
    out: &wgpu::Buffer,
    width: u32,
    height: u32,
    pattern: BayerPattern,
) -> Result<(), GpuError> {
    let params = Block::new()
        .u32(width)
        .u32(height)
        .u32(pattern.flip())
        .u32(0)
        .finish();
    gpu.dispatch(&kernels.demosaic, &params, &[input, out], width, height)
}

/// Stage `input-colour`: camera RGB to working-space RGB by one matrix (the white balance is in it).
pub(crate) fn run_input_colour(
    gpu: &Gpu,
    kernels: &Kernels,
    camera: &[[f32; 3]],
    width: u32,
    height: u32,
    matrix: &Matrix3,
) -> Result<Vec<[f32; 3]>, GpuError> {
    let input = storage_with(gpu, &rgb_bytes(camera))?;
    let out = out_buffer(gpu, pixels(width, height) * 16)?;
    matrix_pass(
        gpu,
        &kernels.input_colour,
        &input,
        &out,
        width,
        height,
        matrix,
    )?;
    gpu.wait()?;
    Ok(rgb_from(&gpu.read_back(&out, pixels(width, height) * 16)?))
}

fn matrix_pass(
    gpu: &Gpu,
    kernel: &Kernel,
    input: &wgpu::Buffer,
    out: &wgpu::Buffer,
    width: u32,
    height: u32,
    matrix: &Matrix3,
) -> Result<(), GpuError> {
    let params = Block::new()
        .u32(width)
        .u32(height)
        .u32(0)
        .u32(0)
        .matrix(matrix)
        .finish();
    gpu.dispatch(kernel, &params, &[input, out], width, height)
}

/// Operation `exposure`: a gain on working-space RGB.
pub(crate) fn run_exposure(
    gpu: &Gpu,
    kernels: &Kernels,
    working: &[[f32; 3]],
    width: u32,
    height: u32,
    exposure_ev: f32,
) -> Result<Vec<[f32; 3]>, GpuError> {
    let input = storage_with(gpu, &rgb_bytes(working))?;
    let out = out_buffer(gpu, pixels(width, height) * 16)?;
    exposure_pass(gpu, kernels, &input, &out, width, height, exposure_ev)?;
    gpu.wait()?;
    Ok(rgb_from(&gpu.read_back(&out, pixels(width, height) * 16)?))
}

fn exposure_pass(
    gpu: &Gpu,
    kernels: &Kernels,
    input: &wgpu::Buffer,
    out: &wgpu::Buffer,
    width: u32,
    height: u32,
    exposure_ev: f32,
) -> Result<(), GpuError> {
    let params = Block::new()
        .u32(width)
        .u32(height)
        .f32(exposure_ev.exp2())
        .f32(0.0)
        .finish();
    gpu.dispatch(&kernels.exposure, &params, &[input, out], width, height)
}

/// Stage `display`: working-space RGB to 8-bit sRGB, as RGBA words (red in the low byte).
pub(crate) fn run_output(
    gpu: &Gpu,
    kernels: &Kernels,
    working: &[[f32; 3]],
    width: u32,
    height: u32,
    to_display: &Matrix3,
) -> Result<Vec<u32>, GpuError> {
    let input = storage_with(gpu, &rgb_bytes(working))?;
    let out = out_buffer(gpu, pixels(width, height) * 4)?;
    matrix_pass(
        gpu,
        &kernels.output,
        &input,
        &out,
        width,
        height,
        to_display,
    )?;
    gpu.wait()?;
    Ok(words_from(&gpu.read_back(&out, pixels(width, height) * 4)?))
}

fn words_from(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// The whole chain on the GPU, the intermediate images staying there: counts to 8-bit sRGB.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn develop(
    gpu: &Gpu,
    kernels: &Kernels,
    samples: &[u16],
    width: u32,
    height: u32,
    p: &Develop,
) -> Result<Vec<u32>, GpuError> {
    assert!(
        p.levels.is_valid(),
        "a black level pattern with the wrong number of values"
    );
    let n = pixels(width, height);
    let raw = storage_with(gpu, &pack_samples(samples))?;
    let black = storage_with(gpu, &f32_bytes(&p.levels.black))?;
    let mosaic = storage(gpu, n * 4, wgpu::BufferUsages::empty())?;
    let camera = storage(gpu, n * 16, wgpu::BufferUsages::empty())?;
    let working = storage(gpu, n * 16, wgpu::BufferUsages::empty())?;
    let exposed = storage(gpu, n * 16, wgpu::BufferUsages::empty())?;
    let out = out_buffer(gpu, n * 4)?;
    levels_pass(
        gpu, kernels, &raw, &black, &mosaic, width, height, &p.levels,
    )?;
    demosaic_pass(gpu, kernels, &mosaic, &camera, width, height, p.pattern)?;
    matrix_pass(
        gpu,
        &kernels.input_colour,
        &camera,
        &working,
        width,
        height,
        &p.to_working,
    )?;
    exposure_pass(
        gpu,
        kernels,
        &working,
        &exposed,
        width,
        height,
        p.exposure_ev,
    )?;
    matrix_pass(
        gpu,
        &kernels.output,
        &exposed,
        &out,
        width,
        height,
        &p.to_display,
    )?;
    gpu.wait()?;
    Ok(words_from(&gpu.read_back(&out, n * 4)?))
}
