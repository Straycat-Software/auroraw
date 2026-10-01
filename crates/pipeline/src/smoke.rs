// SPDX-License-Identifier: GPL-3.0-or-later
//! The smoke test (testing strategy §4, item 4): compile **every** shader of the engine and run
//! each on a known input against its CPU reference, on the adapter the engine is running on.
//!
//! It is the test that caught a DirectX-only failure in spike 1 (FXC rejected a construct that
//! Vulkan and Metal accept), so it is blocking in continuous integration on all three platforms,
//! on the software adapters the runners offer (lavapipe, WARP, the runner's Metal adapter), and
//! it is what `cargo xtask gpu-check` will run on each real adapter of a reference machine.
//!
//! Every shader in `src/shaders/` must be in [`SHADERS`] with a case that runs it; a test enforces
//! it, so that a new shader cannot skip the smoke test.

use crate::adapter::AdapterInfo;
use crate::gpu::{Gpu, GpuError, KernelError};
use crate::reference;
use crate::scenes::{self, BLACK, WHITE};
use crate::stages::{self, BayerPattern, Kernels, Levels};
use crate::thread::{Pipeline, RunError};

/// How far a shader's output may be from its CPU reference (testing strategy §4, item 2).
///
/// It is set **per shader, from measurement**, as §4.2 asks, not once for all: a neighbourhood
/// operation with 0.05 % of its channels two levels off must not fail a rule written for a probe, and a
/// stage whose output is linear `f32` is held to a bound on the value, not on 8-bit levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tolerance {
    /// An 8-bit output, compared in levels.
    Levels {
        /// The largest difference allowed in any channel, in 8-bit levels; `None` only reports it.
        max_level_difference: Option<u32>,
        /// The largest fraction of channels allowed to differ by more than one level.
        max_fraction_over_one_level: f64,
        /// The largest fraction of channels allowed to differ **at all**; `None` does not gate it. A
        /// rule of "one level" cannot see a bias of exactly one level (a truncation where the
        /// reference rounds differs on half the channels and never by more than one), so a stage whose
        /// output is computed in the same arithmetic as its reference is held to this as well.
        max_fraction_differing: Option<f64>,
    },
    /// A linear `f32` intermediate, compared by absolute difference (testing strategy §4.2: "a tighter
    /// bound on the linear f32 intermediates of each stage, set per stage from measurement").
    Absolute {
        /// The largest absolute difference allowed.
        max_difference: f32,
    },
}

impl Tolerance {
    /// The rule of testing strategy §4.2 for a stage's final output: one level on at least 99.9 %
    /// of the channels, and the largest difference reported, not gated.
    pub const OUTPUT: Tolerance = Tolerance::Levels {
        max_level_difference: None,
        max_fraction_over_one_level: 0.001,
        max_fraction_differing: None,
    };
    /// A stage that is the same arithmetic as its reference up to the precision of the graphics API
    /// (the output transform): no channel more than one level off, and no more than 5 % off at all. On
    /// the adapters measured, none is off by even one level; the margin is for a `pow` that rounds the
    /// other way on a boundary, a handful of channels in a thousand and not a twentieth of them.
    pub const SAME_ARITHMETIC: Tolerance = Tolerance::Levels {
        max_level_difference: Some(1),
        max_fraction_over_one_level: 0.001,
        max_fraction_differing: Some(0.05),
    };
    /// A linear `f32` stage held to `max_difference`.
    pub const fn absolute(max_difference: f32) -> Tolerance {
        Tolerance::Absolute { max_difference }
    }

    /// The probe's: no channel more than one level off, as on every adapter measured so far.
    /// (With a maximum of one level nothing can be over one level, so only that bound can fail.)
    pub const PROBE: Tolerance = Tolerance::Levels {
        max_level_difference: Some(1),
        max_fraction_over_one_level: 0.001,
        max_fraction_differing: None,
    };
}

/// What was measured between a shader and its reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measured {
    /// An 8-bit output: the largest difference and the fraction of channels over one level.
    Levels {
        /// The largest difference from the CPU reference, in 8-bit levels.
        max_level_difference: u32,
        /// The fraction of channels that differ by more than one level.
        fraction_over_one_level: f64,
        /// The fraction of channels that differ at all.
        fraction_differing: f64,
    },
    /// A linear `f32` output: the largest absolute difference.
    Absolute {
        /// The largest absolute difference from the CPU reference.
        max_difference: f32,
    },
}

/// A shader of the engine, as WGSL text, with the case that runs it against its reference.
pub(crate) struct Shader {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
    /// What it may differ from its reference by, from measurement.
    pub(crate) tolerance: Tolerance,
    /// Runs it on known inputs and measures it against the reference.
    pub(crate) case: fn(&Gpu, &Kernels, &Shader) -> Result<Measurement, SmokeError>,
}

/// The outcome of a shader's case, before it is held to its tolerance.
pub(crate) struct Measurement {
    pub(crate) cases: usize,
    pub(crate) compared: usize,
    pub(crate) measured: Measured,
}

// The bounds of the linear `f32` stages, from measurement (testing strategy §4.2). Measured on the GTX
// 1650 SUPER (Vulkan) and llvmpipe: levels at most 6e-8, the demosaic 1.2e-7, the colour step 4.8e-7,
// the exposure exactly 0. Each bound is **about ten times the largest value measured**, a margin for
// adapters not yet seen (fused multiply-adds, a fast-math divide); the continuous-integration runs add WARP
// and the macOS Metal device, and a bound that one of them exceeds is a finding to read, not to widen.
const LEVELS: Tolerance = Tolerance::absolute(1e-6);
const DEMOSAIC: Tolerance = Tolerance::absolute(2e-6);
const COLOUR: Tolerance = Tolerance::absolute(5e-6);
const EXPOSURE: Tolerance = Tolerance::absolute(1e-6);

/// Every shader of the engine. A `.wgsl` file in `src/shaders/` that is not here fails a test.
pub(crate) const SHADERS: &[Shader] = &[
    Shader {
        name: "probe",
        source: include_str!("shaders/probe.wgsl"),
        tolerance: Tolerance::PROBE,
        case: probe::run,
    },
    Shader {
        name: "levels",
        source: stages::LEVELS_WGSL,
        tolerance: LEVELS,
        case: cases::levels,
    },
    Shader {
        name: "demosaic_bayer",
        source: stages::DEMOSAIC_BAYER_WGSL,
        tolerance: DEMOSAIC,
        case: cases::demosaic,
    },
    Shader {
        name: "input_colour",
        source: stages::INPUT_COLOUR_WGSL,
        tolerance: COLOUR,
        case: cases::input_colour,
    },
    Shader {
        name: "exposure",
        source: stages::EXPOSURE_WGSL,
        tolerance: EXPOSURE,
        case: cases::exposure,
    },
    Shader {
        name: "output",
        source: stages::OUTPUT_WGSL,
        tolerance: Tolerance::SAME_ARITHMETIC,
        case: cases::output,
    },
];

/// What running one shader against its reference showed.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderReport {
    /// The shader's name (its file name without `.wgsl`).
    pub shader: &'static str,
    /// How many inputs it ran on (sizes that are and are not multiples of the workgroup, patterns).
    pub cases: usize,
    /// How many values (8-bit channels, or `f32` components) were compared.
    pub compared: usize,
    /// What was measured.
    pub measured: Measured,
    /// The tolerance it was held to.
    pub tolerance: Tolerance,
}

impl ShaderReport {
    /// Whether the shader agrees with its reference within the stated tolerance.
    pub fn passed(&self) -> bool {
        match (self.measured, self.tolerance) {
            (
                Measured::Levels {
                    max_level_difference,
                    fraction_over_one_level,
                    fraction_differing,
                },
                Tolerance::Levels {
                    max_level_difference: max,
                    max_fraction_over_one_level,
                    max_fraction_differing,
                },
            ) => {
                max.is_none_or(|max| max_level_difference <= max)
                    && fraction_over_one_level <= max_fraction_over_one_level
                    && max_fraction_differing.is_none_or(|f| fraction_differing <= f)
            }
            (
                Measured::Absolute { max_difference },
                Tolerance::Absolute {
                    max_difference: bound,
                },
            ) => max_difference <= bound,
            // A measure in one unit held to a tolerance in another is a bug of the registry.
            _ => false,
        }
    }

    /// A short line for logs, such as `max 0 level(s), 0.0000 % over one level` or `max 3.0e-7`.
    pub fn describe(&self) -> String {
        match self.measured {
            Measured::Levels {
                max_level_difference,
                fraction_over_one_level,
                fraction_differing,
            } => format!(
                "max {max_level_difference} level(s), {:.4} % over one level, {:.4} % differing",
                fraction_over_one_level * 100.0,
                fraction_differing * 100.0
            ),
            Measured::Absolute { max_difference } => format!("max difference {max_difference:.1e}"),
        }
    }
}

/// The result of the smoke test on one adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct SmokeReport {
    /// The adapter it ran on.
    pub adapter: AdapterInfo,
    /// The device's generation when it ran: `0` unless the device was re-created before.
    pub generation: u64,
    /// One entry per shader.
    pub shaders: Vec<ShaderReport>,
}

impl SmokeReport {
    /// Whether every shader compiled and agreed with its reference.
    pub fn passed(&self) -> bool {
        !self.shaders.is_empty() && self.shaders.iter().all(ShaderReport::passed)
    }
}

/// Why the smoke test could not produce a report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SmokeError {
    /// A shader did not compile on this adapter. The message is the graphics API's own.
    #[error("the shader {shader:?} does not compile on this adapter: {message}")]
    Shader {
        /// The shader that failed.
        shader: &'static str,
        /// What the compiler said.
        message: String,
    },
    /// The graphics API failed while running a shader (memory, a lost device).
    #[error("the graphics API failed while running {shader:?}: {message}")]
    Gpu {
        /// The shader being run.
        shader: &'static str,
        /// What went wrong.
        message: String,
    },
    /// The GPU thread could not run the test at all.
    #[error("the smoke test could not run: {0}")]
    Engine(String),
}

impl SmokeError {
    fn from_kernel(error: KernelError) -> SmokeError {
        match error {
            KernelError::Compile { shader, message } => SmokeError::Shader { shader, message },
            KernelError::Gpu(e) => SmokeError::Engine(e.to_string()),
        }
    }

    /// A graphics-API failure while running `shader`.
    pub(crate) fn gpu(shader: &'static str, error: GpuError) -> SmokeError {
        SmokeError::Gpu {
            shader,
            message: error.to_string(),
        }
    }
}

impl Pipeline {
    /// Compiles every shader and runs each against its CPU reference on the engine's adapter.
    pub fn smoke_test(&self) -> Result<SmokeReport, SmokeError> {
        let shaders = self
            .run(|gpu| -> Result<Vec<ShaderReport>, SmokeError> {
                // The stages' shaders compile together; a refusal names the shader that failed.
                let kernels = Kernels::compile(gpu).map_err(SmokeError::from_kernel)?;
                let mut reports = Vec::with_capacity(SHADERS.len());
                for shader in SHADERS {
                    let measurement = (shader.case)(gpu, &kernels, shader)?;
                    reports.push(ShaderReport {
                        shader: shader.name,
                        cases: measurement.cases,
                        compared: measurement.compared,
                        measured: measurement.measured,
                        tolerance: shader.tolerance,
                    });
                }
                Ok(reports)
            })
            .map_err(|e: RunError| SmokeError::Engine(e.to_string()))??;
        // Read after the job: a device lost before it was re-created for it.
        Ok(SmokeReport {
            adapter: self.adapter(),
            generation: self.generation(),
            shaders,
        })
    }
}

/// Folds one comparison of linear values into a running maximum.
fn max_abs_difference(got: &[f32], want: &[f32]) -> f32 {
    assert_eq!(
        got.len(),
        want.len(),
        "a stage returned the wrong number of values"
    );
    got.iter()
        .zip(want)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max)
}

fn flatten(pixels: &[[f32; 3]]) -> Vec<f32> {
    pixels.iter().flatten().copied().collect()
}

/// The cases of the stages: each runs on inputs that expose edges, and is fed **the reference's
/// output of the stage before it**, so that a difference is this stage's and not a stage upstream's.
mod cases {
    use super::*;
    use crate::colour;

    /// Sizes that are not multiples of the 16 x 16 workgroup, a tiny one, and one that is a multiple.
    const SIZES: [(u32, u32); 3] = [(37, 23), (3, 5), (64, 48)];

    const PATTERNS: [BayerPattern; 4] = [
        BayerPattern::Rggb,
        BayerPattern::Grbg,
        BayerPattern::Gbrg,
        BayerPattern::Bggr,
    ];

    fn scene_for(i: usize, width: u32, height: u32) -> scenes::Scene {
        match i % 4 {
            0 => scenes::zone_plate(width, height),
            1 => scenes::edges(width, height),
            2 => scenes::clipped(width, height),
            _ => scenes::ramp(width, height),
        }
    }

    fn absolute(cases: usize, compared: usize, max: f32) -> Measurement {
        Measurement {
            cases,
            compared,
            measured: Measured::Absolute {
                max_difference: max,
            },
        }
    }

    pub(super) fn levels(
        gpu: &Gpu,
        kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let fail = |e| SmokeError::gpu(shader.name, e);
        let (mut cases, mut compared, mut max) = (0, 0, 0.0_f32);
        for (i, (width, height)) in SIZES.into_iter().enumerate() {
            let mosaic = scenes::mosaic_of(
                &scene_for(i, width, height),
                BayerPattern::Rggb,
                BLACK,
                WHITE,
            );
            // A uniform black level, and a 2 x 2 pattern whose four values differ.
            for levels in [
                Levels::uniform(BLACK, WHITE),
                Levels {
                    rows: 2,
                    cols: 2,
                    black: vec![510.0, 512.0, 512.0, 514.0],
                    white: WHITE,
                },
            ] {
                let got = stages::run_levels(gpu, kernels, &mosaic, width, height, &levels)
                    .map_err(fail)?;
                let want = reference::levels(&mosaic, width, height, &levels);
                max = max.max(max_abs_difference(&got, &want));
                compared += got.len();
                cases += 1;
            }
        }
        Ok(absolute(cases, compared, max))
    }

    pub(super) fn demosaic(
        gpu: &Gpu,
        kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let fail = |e| SmokeError::gpu(shader.name, e);
        let (mut cases, mut compared, mut max) = (0, 0, 0.0_f32);
        for (i, (width, height)) in SIZES.into_iter().enumerate() {
            for pattern in PATTERNS {
                let mosaic16 =
                    scenes::mosaic_of(&scene_for(i, width, height), pattern, BLACK, WHITE);
                let mosaic =
                    reference::levels(&mosaic16, width, height, &Levels::uniform(BLACK, WHITE));
                let got = stages::run_demosaic(gpu, kernels, &mosaic, width, height, pattern)
                    .map_err(fail)?;
                let want = reference::demosaic_bayer(&mosaic, width, height, pattern);
                max = max.max(max_abs_difference(&flatten(&got), &flatten(&want)));
                compared += got.len() * 3;
                cases += 1;
            }
        }
        Ok(absolute(cases, compared, max))
    }

    /// A camera matrix with the white balance of a daylight shot folded in: not the identity, so the
    /// stage's arithmetic is exercised.
    fn colour_matrix() -> colour::Matrix3 {
        let xyz_to_cam = [
            [1.1285, -0.5003, -0.0897],
            [-0.5039, 1.2559, 0.1346],
            [-0.0432, 0.1655, 0.5774],
        ];
        let m = colour::camera_to_working(&xyz_to_cam).expect("a usable matrix");
        colour::fold_white_balance(&m, [1.9, 1.0, 1.38])
    }

    pub(super) fn input_colour(
        gpu: &Gpu,
        kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let fail = |e| SmokeError::gpu(shader.name, e);
        let m = colour_matrix();
        let (mut cases, mut compared, mut max) = (0, 0, 0.0_f32);
        for (i, (width, height)) in SIZES.into_iter().enumerate() {
            let camera = scene_for(i, width, height).rgb;
            let got =
                stages::run_input_colour(gpu, kernels, &camera, width, height, &m).map_err(fail)?;
            let want = reference::apply_matrix(&camera, &m);
            max = max.max(max_abs_difference(&flatten(&got), &flatten(&want)));
            compared += got.len() * 3;
            cases += 1;
        }
        Ok(absolute(cases, compared, max))
    }

    pub(super) fn exposure(
        gpu: &Gpu,
        kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let fail = |e| SmokeError::gpu(shader.name, e);
        let (mut cases, mut compared, mut max) = (0, 0, 0.0_f32);
        for (i, (width, height)) in SIZES.into_iter().enumerate() {
            let working = scene_for(i, width, height).rgb;
            for ev in [-2.5_f32, 0.0, 1.25] {
                let got = stages::run_exposure(gpu, kernels, &working, width, height, ev)
                    .map_err(fail)?;
                let want = reference::exposure(&working, ev);
                max = max.max(max_abs_difference(&flatten(&got), &flatten(&want)));
                compared += got.len() * 3;
                cases += 1;
            }
        }
        Ok(absolute(cases, compared, max))
    }

    pub(super) fn output(
        gpu: &Gpu,
        kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let fail = |e| SmokeError::gpu(shader.name, e);
        let to_display = colour::working_to_display();
        let (mut cases, mut compared, mut max, mut over, mut differing) =
            (0, 0_usize, 0_u32, 0_usize, 0_usize);
        for (i, (width, height)) in SIZES.into_iter().enumerate() {
            // Values below zero and above one are in the scenes after the exposure: they must clamp.
            let working = reference::exposure(&scene_for(i, width, height).rgb, 0.5);
            let got = stages::run_output(gpu, kernels, &working, width, height, &to_display)
                .map_err(fail)?;
            let want = reference::output(&working, &to_display);
            for (a, b) in got.iter().zip(&want) {
                for shift in [0, 8, 16, 24] {
                    let d =
                        (((a >> shift) & 255) as i32 - ((b >> shift) & 255) as i32).unsigned_abs();
                    max = max.max(d);
                    over += usize::from(d > 1);
                    differing += usize::from(d > 0);
                    compared += 1;
                }
            }
            cases += 1;
        }
        Ok(Measurement {
            cases,
            compared,
            measured: Measured::Levels {
                max_level_difference: max,
                fraction_over_one_level: over as f64 / compared as f64,
                fraction_differing: differing as f64 / compared as f64,
            },
        })
    }
}

/// The probe shader's case.
mod probe {
    use super::*;

    /// Sizes that expose edges: a single pixel, sizes that are not multiples of the 16 x 16
    /// workgroup, and one that is.
    const SIZES: [(u32, u32); 4] = [(1, 1), (37, 23), (300, 200), (256, 256)];
    const SCALE: f32 = 1.25;
    const BIAS: f32 = -0.05;

    /// Deterministic input in `[0, 1)`: a linear congruential generator, so a run is reproducible
    /// (testing strategy §1, item 3).
    fn input(width: u32, height: u32) -> Vec<f32> {
        let mut state = 0x2545_f491_u32 ^ (width << 16) ^ height;
        (0..width * height)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / 16_777_216.0
            })
            .collect()
    }

    /// The reference: the same maths, written for clarity. A 3 x 3 average with the edges
    /// clamped, scaled and offset, clamped to `[0, 1]` and packed as an opaque grey RGBA8 word.
    fn reference(width: u32, height: u32, src: &[f32]) -> Vec<u32> {
        let (w, h) = (width as i64, height as i64);
        let mut out = Vec::with_capacity(src.len());
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0_f32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let xx = (x + dx).clamp(0, w - 1);
                        let yy = (y + dy).clamp(0, h - 1);
                        sum += src[(yy * w + xx) as usize];
                    }
                }
                let v = (sum / 9.0 * SCALE + BIAS).clamp(0.0, 1.0);
                let q = (v * 255.0 + 0.5) as u32;
                out.push(q | (q << 8) | (q << 16) | (255 << 24));
            }
        }
        out
    }

    pub(super) fn run(
        gpu: &Gpu,
        _kernels: &Kernels,
        shader: &Shader,
    ) -> Result<Measurement, SmokeError> {
        let kernel = gpu
            .compile(shader.name, shader.source)
            .map_err(SmokeError::from_kernel)?;
        let fail = |e| SmokeError::gpu(shader.name, e);
        let (mut compared, mut max, mut over, mut differing) = (0_usize, 0_u32, 0_usize, 0_usize);
        for (width, height) in SIZES {
            let src = input(width, height);
            let params = stages::Block::new()
                .u32(width)
                .u32(height)
                .f32(SCALE)
                .f32(BIAS)
                .finish();
            let input = gpu
                .upload(
                    &src.iter()
                        .flat_map(|v| v.to_le_bytes())
                        .collect::<Vec<u8>>(),
                    wgpu::BufferUsages::STORAGE,
                )
                .map_err(fail)?;
            let len = u64::from(width) * u64::from(height) * 4;
            let out = gpu
                .create_buffer(
                    len,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                )
                .map_err(fail)?;
            gpu.dispatch(&kernel, &params, &[&input, &out], width, height)
                .map_err(fail)?;
            gpu.wait().map_err(fail)?;
            let got: Vec<u32> = gpu
                .read_back(&out, len)
                .map_err(fail)?
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let want = reference(width, height, &src);
            for (a, b) in got.iter().zip(&want) {
                for shift in [0, 8, 16, 24] {
                    let d =
                        (((a >> shift) & 255) as i32 - ((b >> shift) & 255) as i32).unsigned_abs();
                    max = max.max(d);
                    over += usize::from(d > 1);
                    differing += usize::from(d > 0);
                    compared += 1;
                }
            }
        }
        Ok(Measurement {
            cases: SIZES.len(),
            compared,
            measured: Measured::Levels {
                max_level_difference: max,
                fraction_over_one_level: over as f64 / compared as f64,
                fraction_differing: differing as f64 / compared as f64,
            },
        })
    }
}
