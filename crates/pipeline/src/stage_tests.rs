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

use proptest::prelude::*;

use crate::adapter::{AdapterChoice, list_adapters};
use crate::colour;
use crate::gpu::GpuError;
use crate::reference;
use crate::scenes::{self, BLACK, Scene, WHITE};
use crate::stages::{self, BayerPattern, Develop, Kernels, Levels};
use crate::thread::{Config, Pipeline};

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
/// `AUR_REQUIRE_GPU`).
fn engines() -> Vec<Pipeline> {
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
            .unwrap_or_else(|e| panic!("cannot open {}: {e}", info.describe()))
        })
        .collect()
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
            let got = gpu_develop(&engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p)
                .unwrap_or_else(|e| {
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
            gpu_develop(&engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p).expect("a render");
        let second =
            gpu_develop(&engine, &mosaic, GOLDEN_WIDTH, GOLDEN_HEIGHT, &p).expect("a render");
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
                let got = gpu_develop(&engine, &mosaic, width, height, &p)
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
            let dark = gpu_develop(&engine, &black, 20, 12, &p).expect("a render");
            assert!(
                dark.iter().all(|&w| w & 0x00ff_ffff == 0),
                "a black frame must be black, in {pattern:?}"
            );
            let light = gpu_develop(&engine, &white, 20, 12, &p).expect("a render");
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
            let got = gpu_develop(&engine, &mosaic, 40, 24, &p).expect("a render");
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
