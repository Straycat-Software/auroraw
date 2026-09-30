// SPDX-License-Identifier: GPL-3.0-or-later
//! The smoke test (testing strategy §4, item 4): compile **every** shader of the engine and run
//! each on a known input against its CPU reference, on the adapter the engine is running on.
//!
//! It is the test that caught a DirectX-only failure in spike 1 (FXC rejected a construct that
//! Vulkan and Metal accept), so it is blocking in continuous integration on all three platforms,
//! on the software adapters the runners offer (lavapipe, WARP, the runner's Metal adapter), and
//! it is what `cargo xtask gpu-check` will run on each real adapter of a reference machine.
//!
//! Every shader in `src/shaders/` must be in [`SHADERS`] and have a case here; a test enforces it,
//! so that a new shader cannot skip the smoke test.

use crate::adapter::AdapterInfo;
use crate::gpu::{Gpu, GpuError};
use crate::thread::{Pipeline, RunError};

/// How far a shader's 8-bit output may be from its CPU reference (testing strategy §4, item 2).
///
/// It is set **per shader, from measurement**, as §4.2 asks, not once for all: a neighbourhood
/// operation with 0.05 % of its channels two levels off must not fail a rule written for a probe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// The largest difference allowed in any channel, in 8-bit levels; `None` only reports it.
    pub max_level_difference: Option<u32>,
    /// The largest fraction of channels allowed to differ by more than one level.
    pub max_fraction_over_one_level: f64,
}

impl Tolerance {
    /// The rule of testing strategy §4.2 for a stage's final output: one level on at least 99.9 %
    /// of the channels, and the largest difference reported, not gated.
    pub const OUTPUT: Tolerance = Tolerance {
        max_level_difference: None,
        max_fraction_over_one_level: 0.001,
    };
    /// The probe's: no channel more than one level off, as on every adapter measured so far.
    /// (With a maximum of one level nothing can be over one level, so only that bound can fail.)
    pub const PROBE: Tolerance = Tolerance {
        max_level_difference: Some(1),
        max_fraction_over_one_level: 0.001,
    };
}

/// A shader of the engine, as WGSL text.
pub(crate) struct Shader {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
    /// What it may differ from its reference by, from measurement.
    pub(crate) tolerance: Tolerance,
}

/// Every shader of the engine. A `.wgsl` file in `src/shaders/` that is not here fails a test.
pub(crate) const SHADERS: &[Shader] = &[Shader {
    name: "probe",
    source: include_str!("shaders/probe.wgsl"),
    tolerance: Tolerance::PROBE,
}];

/// What running one shader against its reference showed.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderReport {
    /// The shader's name (its file name without `.wgsl`).
    pub shader: &'static str,
    /// How many inputs it ran on (sizes that are and are not multiples of the workgroup).
    pub cases: usize,
    /// How many 8-bit channels were compared.
    pub channels: usize,
    /// The largest difference from the CPU reference, in 8-bit levels.
    pub max_level_difference: u32,
    /// The fraction of channels that differ by more than one level.
    pub fraction_over_one_level: f64,
    /// The tolerance it was held to.
    pub tolerance: Tolerance,
}

impl ShaderReport {
    /// Whether the shader agrees with its reference within the stated tolerance.
    pub fn passed(&self) -> bool {
        self.tolerance
            .max_level_difference
            .is_none_or(|max| self.max_level_difference <= max)
            && self.fraction_over_one_level <= self.tolerance.max_fraction_over_one_level
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

impl Pipeline {
    /// Compiles every shader and runs each against its CPU reference on the engine's adapter.
    pub fn smoke_test(&self) -> Result<SmokeReport, SmokeError> {
        let shaders = self
            .run(|gpu| -> Result<Vec<ShaderReport>, SmokeError> {
                let mut reports = Vec::with_capacity(SHADERS.len());
                for shader in SHADERS {
                    reports.push(match shader.name {
                        "probe" => probe::run(gpu, shader)?,
                        other => {
                            return Err(SmokeError::Engine(format!(
                                "the shader {other:?} has no smoke case"
                            )));
                        }
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

/// Compiles `shader` into a compute pipeline, turning a compile error into [`SmokeError::Shader`].
fn compile(gpu: &Gpu, shader: &Shader) -> Result<wgpu::ComputePipeline, SmokeError> {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(shader.name),
            source: wgpu::ShaderSource::Wgsl(shader.source.into()),
        });
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(shader.name),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
    match pollster::block_on(scope.pop()) {
        Some(error) => Err(SmokeError::Shader {
            shader: shader.name,
            message: error.to_string(),
        }),
        None => Ok(pipeline),
    }
}

/// The probe shader's case.
mod probe {
    use super::{Gpu, GpuError, Shader, ShaderReport, SmokeError, compile};

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

    fn bytes_of_f32(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn words_from(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Runs one size on the GPU and returns its words.
    fn on_gpu(
        gpu: &Gpu,
        pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        src: &[f32],
    ) -> Result<Vec<u32>, GpuError> {
        let mut params = Vec::with_capacity(16);
        params.extend_from_slice(&width.to_le_bytes());
        params.extend_from_slice(&height.to_le_bytes());
        params.extend_from_slice(&SCALE.to_le_bytes());
        params.extend_from_slice(&BIAS.to_le_bytes());
        let params = gpu.upload(&params, wgpu::BufferUsages::UNIFORM)?;
        let src = gpu.upload(&bytes_of_f32(src), wgpu::BufferUsages::STORAGE)?;
        let len = u64::from(width) * u64::from(height) * 4;
        let dst = gpu.create_buffer(
            len,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        )?;
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst.as_entire_binding(),
                },
            ],
        });
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
        }
        gpu.queue.submit([encoder.finish()]);
        gpu.wait()?;
        Ok(words_from(&gpu.read_back(&dst, len)?))
    }

    pub(super) fn run(gpu: &Gpu, shader: &Shader) -> Result<ShaderReport, SmokeError> {
        let pipeline = compile(gpu, shader)?;
        let failed = |e: GpuError| SmokeError::Gpu {
            shader: shader.name,
            message: e.to_string(),
        };
        let (mut channels, mut max, mut over) = (0_usize, 0_u32, 0_usize);
        for (width, height) in SIZES {
            let src = input(width, height);
            let got = on_gpu(gpu, &pipeline, width, height, &src).map_err(failed)?;
            let want = reference(width, height, &src);
            for (a, b) in got.iter().zip(&want) {
                for shift in [0, 8, 16, 24] {
                    let d =
                        (((a >> shift) & 255) as i32 - ((b >> shift) & 255) as i32).unsigned_abs();
                    max = max.max(d);
                    over += usize::from(d > 1);
                    channels += 1;
                }
            }
        }
        Ok(ShaderReport {
            shader: shader.name,
            cases: SIZES.len(),
            channels,
            max_level_difference: max,
            fraction_over_one_level: over as f64 / channels as f64,
            tolerance: shader.tolerance,
        })
    }
}
