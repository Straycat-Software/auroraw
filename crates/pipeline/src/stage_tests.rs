// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests of the first stages: each against its CPU reference, the chain against **golden renders**
//! (testing strategy §4, items 1, 8 and 9), the edges of the input (tiny sizes, black and white frames,
//! extreme parameters), and the properties the stages must keep whatever the data.
//!
//! **Golden renders** are small 8-bit PNG files in `tests/golden/`, each with the parameters that
//! produced it in a `.params` text file next to it. They are produced by the CPU reference and checked by
//! eye once. They change only by an explicit command that prints the difference:
//!
//! ```text
//! AUR_UPDATE_GOLDEN=1 cargo test -p auroraw-pipeline golden
//! ```
//!
//! An update is a decision, not a side effect: the difference is printed, and the changed files show in
//! the review.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::OnceLock;

use proptest::prelude::*;

use crate::adapter::{AdapterChoice, list_adapters};
use crate::colour;
use crate::gpu::GpuError;
use crate::reference;
use crate::scenes::{self, BLACK, Scene, WHITE};
use crate::stages::{self, BayerPattern, Develop, Kernels, Levels};
use crate::thread::{Config, OpenError, Pipeline};

const PATTERNS: [BayerPattern; 4] = [
    BayerPattern::Rggb,
    BayerPattern::Grbg,
    BayerPattern::Gbrg,
    BayerPattern::Bggr,
];

// ---- running the chain on a device ----

/// Runs the whole chain on `engine`'s device and returns the 8-bit RGBA words.
fn gpu_develop(
    engine: &Pipeline,
    mosaic: &[u16],
    width: u32,
    height: u32,
    p: &Develop,
) -> Result<Vec<u32>, String> {
    let (mosaic, p) = (mosaic.to_vec(), p.clone());
    engine
        .run(move |gpu| -> Result<Vec<u32>, String> {
            let kernels = Kernels::compile(gpu).map_err(|e| e.to_string())?;
            stages::develop(gpu, &kernels, &mosaic, width, height, &p).map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())?
}

/// Every engine of the machine, one per adapter, or none (a skip locally, a failure under
/// `AUR_REQUIRE_GPU`), **shared by all the tests of the process**.
///
/// A device costs about 165 MiB of GPU memory just by being open (measured with `nvidia-smi` on the
/// development machine's GTX 1650 SUPER, issue #69: six devices took 983 and 986 MiB in two runs), so one
/// engine per test, with eight tests running at once, took about four fifths of what the desktop left free
/// of its 4 GiB. The tests only run stages on the device (a job at a time, queued on its thread); the ones
/// that must create or lose a device themselves (`tests.rs`, `tests/smoke.rs`) open their own.
///
/// This shares nothing under `cargo nextest`, which runs every test in a process of its own: each test then
/// opens its engines, as before. The CI has no discrete GPU, so it does not matter there.
fn engines() -> &'static [Pipeline] {
    static ENGINES: OnceLock<Vec<Pipeline>> = OnceLock::new();
    ENGINES.get_or_init(|| {
        let adapters = list_adapters();
        if adapters.is_empty() {
            assert!(
                std::env::var_os("AUR_REQUIRE_GPU").is_none(),
                "AUR_REQUIRE_GPU is set but the machine has no graphics adapter"
            );
            eprintln!("skipped: no graphics adapter");
        }
        adapters
            .iter()
            .map(|info| {
                Pipeline::open(Config {
                    adapter: AdapterChoice::Named(format!("{} {}", info.backend, info.name)),
                })
                .unwrap_or_else(|e| panic!("cannot open {}: {e}{}", info.describe(), explained(&e)))
            })
            .collect()
    })
}

/// What a refusal to give a device means when it is about memory, said where the test fails, so that nobody
/// reads it as a result about the change under test.
fn explained(error: &OpenError) -> &'static str {
    match error {
        OpenError::Device { reason, .. } if reason.to_lowercase().contains("memory") => {
            "\nthe GPU is out of memory: other processes hold it (see nvidia-smi), and a device costs about \
             165 MiB on the development machine; this is not a result about the change under test"
        }
        _ => "",
    }
}

#[test]
fn a_device_refused_for_memory_is_explained_and_other_refusals_are_left_alone() {
    let device = |reason: &str| OpenError::Device {
        adapter: "an adapter".into(),
        reason: reason.into(),
    };
    let memory = explained(&device("Not enough memory left."));
    assert!(memory.contains("out of memory"), "{memory}");
    assert!(memory.contains("nvidia-smi"), "{memory}");
    assert!(memory.contains("165 MiB"), "{memory}");
    // Another reason, or another kind of refusal, gets no explanation it did not earn.
    assert_eq!(explained(&device("The feature is not supported.")), "");
    assert_eq!(explained(&OpenError::Thread("memory".into())), "");
}

/// The difference between two 8-bit images: the largest, how many channels are over one level, and how
/// many were compared.
fn compare_words(a: &[u32], b: &[u32]) -> (u32, usize, usize) {
    assert_eq!(a.len(), b.len());
    let (mut max, mut over, mut total) = (0_u32, 0_usize, 0_usize);
    for (x, y) in a.iter().zip(b) {
        for shift in [0, 8, 16, 24] {
            let d = (((x >> shift) & 255) as i32 - ((y >> shift) & 255) as i32).unsigned_abs();
            max = max.max(d);
            over += usize::from(d > 1);
            total += 1;
        }
    }
    (max, over, total)
}

/// How many channels differ at all between two 8-bit images, as a fraction.
fn fraction_differing(a: &[u32], b: &[u32]) -> f64 {
    let differing: usize = a
        .iter()
        .zip(b)
        .map(|(x, y)| {
            [0, 8, 16, 24]
                .iter()
                .filter(|&&s| ((x >> s) & 255) != ((y >> s) & 255))
                .count()
        })
        .sum();
    differing as f64 / (a.len() * 4) as f64
}

/// The rule of testing strategy §4.2 for a final 8-bit output: at most 0.1 % of the channels over one level.
fn assert_within_output_tolerance(what: &str, got: &[u32], want: &[u32]) {
    let (max, over, total) = compare_words(got, want);
    assert!(
        over as f64 / total as f64 <= 0.001,
        "{what}: {over} of {total} channels are more than one level off (largest difference {max})"
    );
}

// ---- golden renders ----

struct GoldenCase {
    name: &'static str,
    scene: fn(u32, u32) -> Scene,
    scene_name: &'static str,
    pattern: BayerPattern,
    /// A daylight-like colour step: a camera matrix with a white balance folded in, and a gain.
    colour: bool,
}

const GOLDEN: [GoldenCase; 5] = [
    GoldenCase {
        name: "zone_plate_rggb",
        scene: scenes::zone_plate,
        scene_name: "zone_plate",
        pattern: BayerPattern::Rggb,
        colour: false,
    },
    GoldenCase {
        name: "edges_grbg",
        scene: scenes::edges,
        scene_name: "edges",
        pattern: BayerPattern::Grbg,
        colour: false,
    },
    GoldenCase {
        name: "clipped_gbrg",
        scene: scenes::clipped,
        scene_name: "clipped",
        pattern: BayerPattern::Gbrg,
        colour: false,
    },
    GoldenCase {
        name: "ramp_bggr",
        scene: scenes::ramp,
        scene_name: "ramp",
        pattern: BayerPattern::Bggr,
        colour: false,
    },
    GoldenCase {
        name: "daylight_zone_plate_rggb",
        scene: scenes::zone_plate,
        scene_name: "zone_plate",
        pattern: BayerPattern::Rggb,
        colour: true,
    },
];

const GOLDEN_WIDTH: u32 = 96;
const GOLDEN_HEIGHT: u32 = 64;

impl GoldenCase {
    /// The mosaic and the chain's parameters this case renders.
    fn inputs(&self) -> (Vec<u16>, Develop) {
        let scene = (self.scene)(GOLDEN_WIDTH, GOLDEN_HEIGHT);
        let mosaic = scenes::mosaic_of(&scene, self.pattern, BLACK, WHITE);
        let mut p = scenes::neutral(self.pattern);
        if self.colour {
            // A camera matrix invented for the test (not a real camera's), the multipliers of a daylight
            // shot folded in, and half a stop of exposure.
            let xyz_to_cam = [
                [1.1285, -0.5003, -0.0897],
                [-0.5039, 1.2559, 0.1346],
                [-0.0432, 0.1655, 0.5774],
            ];
            let m = colour::camera_to_working(&xyz_to_cam).expect("a usable matrix");
            p.to_working = colour::fold_white_balance(&m, [1.9, 1.0, 1.38]);
            p.exposure_ev = 0.5;
        }
        (mosaic, p)
    }

    /// The parameters that produced the render, as text.
    fn params(&self) -> String {
        let (_, p) = self.inputs();
        format!(
            "scene = {}\nsize = {}x{}\npattern = {:?}\nblack = {}\nwhite = {}\nto_working = {:?}\nexposure_ev = {}\nto_display = {:?}\n",
            self.scene_name,
            GOLDEN_WIDTH,
            GOLDEN_HEIGHT,
            self.pattern,
            BLACK,
            WHITE,
            p.to_working,
            p.exposure_ev,
            p.to_display
        )
    }

    fn png_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{}.png", self.name))
    }

    fn params_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{}.params", self.name))
    }
}

fn write_png(path: &PathBuf, width: u32, height: u32, words: &[u32]) {
    let file = BufWriter::new(File::create(path).expect("the golden file can be created"));
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("a PNG header");
    let bytes: Vec<u8> = words
        .iter()
        .flat_map(|w| {
            [
                (w & 255) as u8,
                ((w >> 8) & 255) as u8,
                ((w >> 16) & 255) as u8,
            ]
        })
        .collect();
    writer.write_image_data(&bytes).expect("PNG data");
}

fn read_png(path: &PathBuf) -> (u32, u32, Vec<u32>) {
    let file = File::open(path).unwrap_or_else(|e| {
        panic!(
            "the golden render {} is missing ({e}); create it with AUR_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .expect("a PNG header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a PNG of known size")];
    let info = reader.next_frame(&mut buffer).expect("PNG data");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgb,
        "a golden render is 8-bit RGB"
    );
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let words = buffer[..info.buffer_size()]
        .chunks_exact(3)
        .map(|c| u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16) | (255 << 24))
        .collect();
    (info.width, info.height, words)
}

/// The golden render of `case`, checked against its stored parameters; created or replaced, with the
/// difference printed, when `AUR_UPDATE_GOLDEN` is set.
fn golden_of(case: &GoldenCase, reference_render: &[u32]) -> Vec<u32> {
    let updating = std::env::var_os("AUR_UPDATE_GOLDEN").is_some();
    let (png, params) = (case.png_path(), case.params_path());
    if updating {
        match File::open(&png).ok().map(|_| read_png(&png)) {
            Some((_, _, old)) if old.len() == reference_render.len() => {
                let (max, over, total) = compare_words(reference_render, &old);
                let changed = reference_render
                    .iter()
                    .zip(&old)
                    .filter(|(a, b)| a != b)
                    .count();
                println!(
                    "golden {}: {changed} of {} pixels change, largest difference {max} level(s), {over} of {total} channels over one level",
                    case.name,
                    old.len()
                );
            }
            Some(_) => println!("golden {}: the size changed", case.name),
            None => println!("golden {}: created", case.name),
        }
        std::fs::create_dir_all(png.parent().expect("a directory")).expect("the golden directory");
        write_png(&png, GOLDEN_WIDTH, GOLDEN_HEIGHT, reference_render);
        std::fs::write(&params, case.params()).expect("the parameters file");
    }
    let stored = std::fs::read_to_string(&params).unwrap_or_else(|e| {
        panic!(
            "the parameters of golden {} are missing ({e}); create them with AUR_UPDATE_GOLDEN=1",
            case.name
        )
    });
    assert_eq!(
        stored,
        case.params(),
        "the parameters of golden render {} changed: its picture was produced by other ones. \\
         Regenerate it with AUR_UPDATE_GOLDEN=1 and review the difference it prints",
        case.name
    );
    let (width, height, words) = read_png(&png);
    assert_eq!(
        (width, height),
        (GOLDEN_WIDTH, GOLDEN_HEIGHT),
        "golden {} has the wrong size",
        case.name
    );
    words
}

#[test]
fn the_cpu_reference_reproduces_the_golden_renders() {
    for case in &GOLDEN {
        let (mosaic, p) = case.inputs();
        let render = reference::develop(&mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p);
        let golden = golden_of(case, &render);
        // The reference is plain `f32` Rust: the same on a given machine, and within one level of
        // another machine's (a C library's `pow` differs in its last digit).
        let (max, _, _) = compare_words(&render, &golden);
        assert!(
            max <= 1,
            "{}: the reference differs from its golden render by {max} levels",
            case.name
        );
    }
}

#[test]
fn the_gpu_matches_the_golden_renders_on_every_adapter() {
    for engine in engines() {
        for case in &GOLDEN {
            let (mosaic, p) = case.inputs();
            let reference_render = reference::develop(&mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p);
            let golden = golden_of(case, &reference_render);
            let got =
                gpu_develop(engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p).unwrap_or_else(|e| {
                    panic!("{} on {}: {e}", case.name, engine.adapter().describe())
                });
            assert_within_output_tolerance(
                &format!("{} on {}", case.name, engine.adapter().describe()),
                &got,
                &golden,
            );
            // The golden render is the reference's, and the stages are the same arithmetic as it: a
            // systematic bias of one level (a truncation for a rounding) would pass the rule of one
            // level and is caught here, as the output stage's own tolerance catches it.
            let differing = fraction_differing(&got, &golden);
            assert!(
                differing <= 0.05,
                "{} on {}: {:.1} % of the channels differ from the golden render",
                case.name,
                engine.adapter().describe(),
                differing * 100.0
            );
        }
    }
}

// ---- determinism, edges and extreme parameters ----

#[test]
fn the_same_input_gives_byte_identical_output_twice() {
    for engine in engines() {
        let (mosaic, p) = GOLDEN[4].inputs();
        let first =
            gpu_develop(engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p).expect("a render");
        let second =
            gpu_develop(engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p).expect("a render");
        assert_eq!(
            first,
            second,
            "not deterministic on {}",
            engine.adapter().describe()
        );
    }
}

#[test]
fn tiny_and_odd_sizes_agree_with_the_reference_in_every_pattern() {
    // A single pixel, a single row and column, sizes that are not multiples of the 16 x 16 workgroup, and
    // multiples of it.
    let sizes = [
        (1, 1),
        (2, 2),
        (1, 17),
        (17, 1),
        (3, 3),
        (31, 17),
        (16, 16),
        (33, 33),
    ];
    for engine in engines() {
        for (width, height) in sizes {
            for pattern in PATTERNS {
                let scene = scenes::zone_plate(width, height);
                let mosaic = scenes::mosaic_of(&scene, pattern, BLACK, WHITE);
                let p = scenes::neutral(pattern);
                let got = gpu_develop(engine, &mosaic, width, height, &p)
                    .unwrap_or_else(|e| panic!("{width}x{height} {pattern:?}: {e}"));
                let want = reference::develop(&mosaic, width, height, &p);
                let (max, over, total) = compare_words(&got, &want);
                // On a tiny image one wrong channel is a large fraction: here no channel may be over one level.
                assert!(
                    over == 0,
                    "{width}x{height} {pattern:?} on {}: {over} of {total} channels over one level (largest {max})",
                    engine.adapter().describe()
                );
            }
        }
    }
}

#[test]
fn a_black_frame_is_black_and_a_white_frame_is_white() {
    for engine in engines() {
        for pattern in PATTERNS {
            let black = scenes::mosaic_of(&scenes::flat(20, 12, [0.0; 3]), pattern, BLACK, WHITE);
            let white = scenes::mosaic_of(&scenes::flat(20, 12, [1.0; 3]), pattern, BLACK, WHITE);
            let p = scenes::neutral(pattern);
            let dark = gpu_develop(engine, &black, 20, 12, &p).expect("a render");
            assert!(
                dark.iter().all(|&w| w & 0x00ff_ffff == 0),
                "a black frame must be black, in {pattern:?}"
            );
            let light = gpu_develop(engine, &white, 20, 12, &p).expect("a render");
            for w in light {
                for shift in [0, 8, 16] {
                    let c = (w >> shift) & 255;
                    assert!(
                        c >= 254,
                        "a white frame must be white: channel {c} in {pattern:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn extreme_exposures_agree_with_the_reference_and_clamp() {
    for engine in engines() {
        for ev in [-24.0_f32, -8.0, 8.0, 24.0] {
            let scene = scenes::clipped(40, 24);
            let mosaic = scenes::mosaic_of(&scene, BayerPattern::Rggb, BLACK, WHITE);
            let mut p = scenes::neutral(BayerPattern::Rggb);
            p.exposure_ev = ev;
            let got = gpu_develop(engine, &mosaic, 40, 24, &p).expect("a render");
            let want = reference::develop(&mosaic, 40, 24, &p);
            assert_within_output_tolerance(
                &format!("{ev} EV on {}", engine.adapter().describe()),
                &got,
                &want,
            );
        }
    }
}

#[test]
fn an_image_too_large_for_the_binding_limit_is_refused_not_rendered_wrongly() {
    // 2900 x 2900 pixels of camera RGB is 16 bytes each: 134,560,000, over the 128 MiB floor binding.
    let (width, height) = (2900_u32, 2900_u32);
    let mosaic = vec![0.5_f32; (width * height) as usize];
    for engine in engines() {
        let mosaic = mosaic.clone();
        let outcome = engine
            .run(move |gpu| {
                let kernels = Kernels::compile(gpu).expect("the stages compile");
                stages::run_demosaic(gpu, &kernels, &mosaic, width, height, BayerPattern::Rggb)
                    .map(|_| ())
            })
            .expect("the job ran");
        assert!(
            matches!(outcome, Err(GpuError::TooLarge { .. })),
            "an image over the binding limit must be refused with TooLarge on {}, got {outcome:?}",
            engine.adapter().describe()
        );
    }
}

// ---- properties ----

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The interpolation's weights sum to one at every site, so a flat mosaic gives a flat image,
    /// whatever its size (edges mirrored) and phase.
    #[test]
    fn a_flat_mosaic_demosaics_to_a_flat_image(
        width in 1_u32..40,
        height in 1_u32..40,
        pattern in 0_usize..4,
        value in 0.0_f32..1.0,
    ) {
        let mosaic = vec![value; (width * height) as usize];
        let image = reference::demosaic_bayer(&mosaic, width, height, PATTERNS[pattern]);
        for pixel in image {
            for c in pixel {
                prop_assert!((c - value).abs() < 1e-5, "{pixel:?} against {value}");
            }
        }
    }

    /// The black level maps to 0 and the white level to 1, exactly, for any pair.
    #[test]
    fn the_black_level_is_zero_and_the_white_level_is_one(black in 0_u16..4000, headroom in 100_u16..20000) {
        let white = black + headroom;
        let levels = Levels::uniform(f32::from(black), f32::from(white));
        let out = reference::levels(&[black, white], 2, 1, &levels);
        prop_assert_eq!(out, vec![0.0, 1.0]);
    }

    /// A brighter grey is never darker on screen: the output stage is monotonic on greys.
    #[test]
    fn the_output_is_monotonic_on_greys(a in 0.0_f32..2.0, b in 0.0_f32..2.0) {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let out = reference::output(&[[low; 3], [high; 3]], &colour::working_to_display());
        for shift in [0, 8, 16] {
            prop_assert!((out[0] >> shift) & 255 <= (out[1] >> shift) & 255);
        }
    }

    /// The exposure stage composes: two gains in a row are one gain, up to rounding.
    #[test]
    fn exposures_compose(v in 0.0_f32..2.0, a in -4.0_f32..4.0, b in -4.0_f32..4.0) {
        let twice = reference::exposure(&reference::exposure(&[[v; 3]], a), b);
        let once = reference::exposure(&[[v; 3]], a + b);
        prop_assert!((twice[0][0] - once[0][0]).abs() <= 1e-4 * once[0][0].max(1.0));
    }
}

// ---- the demosaic keeps the whole distribution of a dark, noisy area ----

/// Gaussian noise from a fixed generator (xorshift and Box-Muller), so that the test is the same every run.
struct Noise(u64);

impl Noise {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1_u64 << 53) as f64
    }

    fn gauss(&mut self) -> f64 {
        let (a, b) = (self.uniform(), self.uniform());
        (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
    }
}

/// The mean of the interior of a demosaiced image, per channel, with the edges left out.
fn interior_mean(image: &[[f32; 3]], width: usize, height: usize) -> [f64; 3] {
    let mut sum = [0.0_f64; 3];
    let mut n = 0.0;
    for y in 4..height - 4 {
        for x in 4..width - 4 {
            for c in 0..3 {
                sum[c] += f64::from(image[y * width + x][c]);
            }
            n += 1.0;
        }
    }
    sum.map(|s| s / n)
}

/// A flat dark patch with noise, as a mosaic of `mosaic-linear` values: the signal `signal` plus Gaussian
/// noise of standard deviation `sigma`, **negative where the noise takes it below the black level**.
fn dark_noisy_mosaic(width: u32, height: u32, signal: f64, sigma: f64) -> Vec<f32> {
    let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
    (0..width * height)
        .map(|_| (signal + sigma * noise.gauss()) as f32)
        .collect()
}

#[test]
fn the_demosaic_does_not_lift_a_dark_noisy_area() {
    // The signal is a fifth of the noise: about 40 % of the samples are below the black level. A noise-model
    // denoiser needs that whole distribution (note 006 §4: the transform sees `y / a + 3/8 + b / a²`), and a
    // demosaic that cuts it at zero raises the local mean, a lifted black and a colour cast in the shadows.
    let (width, height) = (96_u32, 96_u32);
    let (signal, sigma) = (0.002_f64, 0.01_f64);
    for pattern in PATTERNS {
        let mosaic = dark_noisy_mosaic(width, height, signal, sigma);
        let image = reference::demosaic_bayer(&mosaic, width, height, pattern);
        let kept = interior_mean(&image, width as usize, height as usize);
        // What a demosaic that clamped at zero would give: the same image with its negatives cut.
        let cut: Vec<[f32; 3]> = image.iter().map(|p| p.map(|v| v.max(0.0))).collect();
        let clamped = interior_mean(&cut, width as usize, height as usize);
        for c in 0..3 {
            // The interpolation weights sum to one, so the mean of the output is the mean of the input.
            assert!(
                (kept[c] - signal).abs() < 6e-4,
                "{pattern:?}: channel {c} has a mean of {} for a signal of {signal}",
                kept[c]
            );
            // And the clamp would have lifted it by a large fraction of the signal itself.
            assert!(
                clamped[c] > signal * 2.0,
                "{pattern:?}: the clamped mean {} is not above twice the signal {signal}: the test no longer shows the bias",
                clamped[c]
            );
        }
        println!(
            "{pattern:?}: signal {signal}, mean kept {:.5} / {:.5} / {:.5}, mean if clamped {:.5} / {:.5} / {:.5}",
            kept[0], kept[1], kept[2], clamped[0], clamped[1], clamped[2]
        );
    }
}

// ---- a known colour through the demosaic: the one check whose truth the code does not define ----

#[test]
fn a_constant_colour_comes_back_in_every_phase_and_at_every_size() {
    // The golden renders come from the reference, and a flat mosaic gives a flat image even if red and
    // blue were swapped (every site has the same value), so neither compares the demosaic with a truth the
    // code does not itself define. A constant colour sampled through each pattern does: red, green and
    // blue must come back as they went in. Sizes down to 2 x 2, where the edges do all the work; an image
    // one pixel wide cannot be tested this way, as it holds no photosite of some colours at all.
    let colour = [0.8_f32, 0.4, 0.2];
    for pattern in PATTERNS {
        for (w, h) in [(20_u32, 12_u32), (3, 3), (4, 3), (2, 2), (2, 5), (5, 2)] {
            let mosaic = scenes::mosaic_of(&scenes::flat(w, h, colour), pattern, BLACK, WHITE);
            let linear = reference::levels(&mosaic, w, h, &Levels::uniform(BLACK, WHITE));
            for px in reference::demosaic_bayer(&linear, w, h, pattern) {
                for c in 0..3 {
                    assert!(
                        (px[c] - colour[c]).abs() < 2e-4,
                        "{pattern:?} {w}x{h}: {px:?} for {colour:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_gpu_returns_a_constant_colour_too_including_on_two_pixel_images() {
    let colour = [0.8_f32, 0.4, 0.2];
    for engine in engines() {
        for pattern in PATTERNS {
            for (w, h) in [(20_u32, 12_u32), (3, 3), (2, 2), (2, 5), (5, 2)] {
                let mosaic16 =
                    scenes::mosaic_of(&scenes::flat(w, h, colour), pattern, BLACK, WHITE);
                let mosaic = reference::levels(&mosaic16, w, h, &Levels::uniform(BLACK, WHITE));
                let image = engine
                    .run(move |gpu| {
                        let kernels = Kernels::compile(gpu).expect("the stages compile");
                        stages::run_demosaic(gpu, &kernels, &mosaic, w, h, pattern)
                    })
                    .expect("the job ran")
                    .expect("a demosaic");
                for px in image {
                    for c in 0..3 {
                        assert!(
                            (px[c] - colour[c]).abs() < 2e-4,
                            "{pattern:?} {w}x{h} on {}: {px:?} for {colour:?}",
                            engine.adapter().describe()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_two_pixel_image_reflects_at_the_edges_as_often_as_it_must() {
    // A single reflection sent x + 3 on a two-pixel image to the wrong parity: this is the case that
    // the first version got wrong (the colour came back as 0.75, 0.40, 0.225 for 0.8, 0.4, 0.2). The
    // mirror repeats every 2 (n - 1) pixels.
    let colour = [0.8_f32, 0.4, 0.2];
    let mosaic = scenes::mosaic_of(
        &scenes::flat(2, 2, colour),
        BayerPattern::Rggb,
        BLACK,
        WHITE,
    );
    let linear = reference::levels(&mosaic, 2, 2, &Levels::uniform(BLACK, WHITE));
    let image = reference::demosaic_bayer(&linear, 2, 2, BayerPattern::Rggb);
    for px in image {
        assert!(
            (px[0] - 0.8).abs() < 2e-4 && (px[1] - 0.4).abs() < 2e-4 && (px[2] - 0.2).abs() < 2e-4,
            "{px:?}"
        );
    }
}

// ---- levels a decoder gave that cannot be applied are refused, not a panic of the GPU thread ----

#[test]
fn levels_that_cannot_be_applied_are_refused_with_the_reason() {
    use crate::stages::StageError;
    let ok = Levels::uniform(512.0, 15360.0);
    assert_eq!(ok.validate(), Ok(()));
    let shape = |rows, cols, n| Levels {
        rows,
        cols,
        black: vec![0.0; n],
        white: 100.0,
    };
    assert!(matches!(
        shape(2, 2, 3).validate(),
        Err(StageError::LevelsShape {
            rows: 2,
            cols: 2,
            values: 3
        })
    ));
    assert!(
        matches!(
            shape(0, 2, 0).validate(),
            Err(StageError::LevelsShape { .. })
        ),
        "no rows"
    );
    assert!(
        matches!(
            shape(2, 0, 0).validate(),
            Err(StageError::LevelsShape { .. })
        ),
        "no columns"
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let white = Levels {
            white: bad,
            ..ok.clone()
        };
        assert_eq!(
            white.validate(),
            Err(StageError::LevelsNotFinite),
            "white {bad}"
        );
        let black = Levels {
            black: vec![bad],
            ..ok.clone()
        };
        assert_eq!(
            black.validate(),
            Err(StageError::LevelsNotFinite),
            "black {bad}"
        );
    }
    // White at or below the black: the division would be by zero or by a negative number.
    let equal = Levels::uniform(512.0, 512.0);
    assert_eq!(
        equal.validate(),
        Err(StageError::WhiteNotAboveBlack {
            white: 512.0,
            black: 512.0
        })
    );
    let inverted = Levels::uniform(2000.0, 512.0);
    assert!(matches!(
        inverted.validate(),
        Err(StageError::WhiteNotAboveBlack { .. })
    ));
    // White above one black value of the pattern but not another: refused, whichever it is.
    let one_too_high = Levels {
        rows: 1,
        cols: 2,
        black: vec![100.0, 20000.0],
        white: 15360.0,
    };
    assert_eq!(
        one_too_high.validate(),
        Err(StageError::WhiteNotAboveBlack {
            white: 15360.0,
            black: 20000.0
        })
    );
}

#[test]
fn the_stages_return_the_refusal_instead_of_panicking_on_the_gpu_thread() {
    use crate::stages::StageError;
    for engine in engines() {
        let bad = Levels::uniform(2000.0, 512.0);
        let (mosaic, p) = GOLDEN[0].inputs();
        let outcome = engine
            .run({
                let (bad, mosaic, p) = (bad.clone(), mosaic.clone(), p.clone());
                move |gpu| {
                    let kernels = Kernels::compile(gpu).expect("the stages compile");
                    let alone = stages::run_levels(
                        gpu,
                        &kernels,
                        &mosaic,
                        GOLDEN_WIDTH,
                        GOLDEN_HEIGHT,
                        &bad,
                    );
                    let chain = stages::develop(
                        gpu,
                        &kernels,
                        &mosaic,
                        GOLDEN_WIDTH,
                        GOLDEN_HEIGHT,
                        &Develop { levels: bad, ..p },
                    );
                    (alone.map(|_| ()), chain.map(|_| ()))
                }
            })
            .expect("the job ran, and the GPU thread did not panic");
        assert!(
            matches!(outcome.0, Err(StageError::WhiteNotAboveBlack { .. })),
            "{:?}",
            outcome.0
        );
        assert!(
            matches!(outcome.1, Err(StageError::WhiteNotAboveBlack { .. })),
            "{:?}",
            outcome.1
        );
        // The engine is still there afterwards.
        engine.smoke_test().expect("the smoke test still runs");
        assert_eq!(
            engine.generation(),
            0,
            "a refusal is not a panic: the device was not re-created"
        );
    }
}
