// SPDX-License-Identifier: GPL-3.0-or-later
//! The WebAssembly `rawler-decoder` plugin against the decoder it wraps, `rawler`, called directly: what comes back
//! across the sandbox must be what `rawler` says (architecture §8.1: "the first real WebAssembly plugin, loaded through
//! the host, and the tests prove it returns exactly what the native one does", spike 4's own check), and it must be
//! what the fixture of the file says (decision D-141: a fixture for each of the seventeen sample files).
//!
//! **One test for each sample file**, so that the slow ones (the 103 MP file, the Windows runner) do not share a
//! timeout and a failure names its file.
//!
//! **Two checks.** The comparison with `rawler` is independent on purpose: it maps `rawler`'s image to the expected
//! values a second time, here, so that a mistake in the plugin's mapping is not copied into the test. The fixture
//! (`tests/fixtures/decoder/<file>.txt`) is a readable dump of what the plugin returns, with the `blake3` of the
//! samples: it makes a change of the decoder's output a visible change of the repository. `AUR_UPDATE_GOLDEN=1` writes
//! the fixtures (and the block of each file as a seed of the fuzz target `raw_block`, in
//! `fuzz/corpus/raw_block/`); without it a missing or different fixture fails with the difference.
//!
//! **A file the decoder refuses is a routine outcome**, not a failure: three of the seventeen are, and the fixture
//! says `refused`. The caller handles [`DecoderError::Invalid`] as such.
//!
//! Skipped locally when the plugin has not been built (`tools/build-plugins.sh`) or the samples have not been
//! fetched (`tools/fetch-samples.sh`); both are required in CI (`AUR_REQUIRE_PLUGINS=1`, `AUR_REQUIRE_SAMPLES=1`),
//! matching `imaging`'s own pattern for `AUR_REQUIRE_EXIFTOOL`.

use auroraw_plugin_api::{Decoder, DecoderError, InputProfile, RawImage, Samples, SensorLayout};
use auroraw_plugin_host::{PluginHost, WasmDecoder};
use rawler::RawImageData;
use rawler::decoders::RawDecodeParams;
use rawler::rawimage::RawPhotometricInterpretation;
use rawler::rawsource::RawSource;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn samples_dir(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/samples");
    if dir.join(name).is_file() {
        return Some(dir);
    }
    if std::env::var_os("AUR_REQUIRE_SAMPLES").is_some() {
        panic!(
            "testdata/samples/{name} is required here but was not found; run tools/fetch-samples.sh"
        );
    }
    eprintln!("testdata/samples/{name} not found: skipping (run tools/fetch-samples.sh)");
    None
}

fn plugin_wasm() -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/target/wasm32-wasip1/release/rawler_decoder.wasm");
    if path.is_file() {
        return Some(std::fs::read(&path).expect("read the compiled plugin"));
    }
    if std::env::var_os("AUR_REQUIRE_PLUGINS").is_some() {
        panic!(
            "rawler_decoder.wasm is required here but was not built; run tools/build-plugins.sh"
        );
    }
    eprintln!("rawler_decoder.wasm not found: skipping (run tools/build-plugins.sh)");
    None
}

/// The bytes of the samples, little-endian, as the plugin holds them.
fn sample_bytes(samples: &Samples) -> Vec<u8> {
    match samples {
        Samples::U16(v) => v.iter().flat_map(|s| s.to_le_bytes()).collect(),
        Samples::F32(v) => v.iter().flat_map(|s| s.to_le_bytes()).collect(),
        _ => panic!("a kind of samples this test does not know"),
    }
}

/// A readable dump of an image, but its samples, of which it gives the type, the count and the `blake3`.
fn dump(name: &str, image: &RawImage) -> String {
    let mut out = String::new();
    let line = |out: &mut String, key: &str, value: String| {
        let _ = writeln!(out, "{key}: {value}");
    };
    line(&mut out, "file", name.into());
    line(
        &mut out,
        "readout",
        format!("{} x {}", image.width, image.height),
    );
    line(
        &mut out,
        "layout",
        match &image.layout {
            SensorLayout::Cfa {
                width,
                height,
                colours,
            } => format!("cfa {width}x{height} {colours:?}"),
            SensorLayout::LinearRgb {
                components,
                profile,
            } => format!("linear rgb, {components} components, profile {profile:?}"),
            SensorLayout::Mono => "mono".into(),
        },
    );
    line(
        &mut out,
        "samples",
        format!(
            "{:?} x {}, blake3 {}",
            image.samples.kind(),
            image.samples.len(),
            blake3::hash(&sample_bytes(&image.samples)).to_hex()
        ),
    );
    let b = &image.levels.black;
    line(
        &mut out,
        "black",
        format!(
            "{} rows x {} columns x {} components {:?}",
            b.rows, b.columns, b.components, b.values
        ),
    );
    line(&mut out, "white", format!("{:?}", image.levels.white));
    line(
        &mut out,
        "white balance",
        format!("{:?}", image.white_balance),
    );
    for m in &image.colour {
        line(
            &mut out,
            "colour matrix",
            format!(
                "illuminant {} ({} rows) {:?}",
                m.illuminant.0, m.rows, m.xyz_to_camera
            ),
        );
    }
    if image.colour.is_empty() {
        line(&mut out, "colour matrix", "none".into());
    }
    line(&mut out, "crop", format!("{:?}", image.crop));
    line(&mut out, "active area", format!("{:?}", image.active_area));
    line(&mut out, "orientation", format!("{:?}", image.orientation));
    line(
        &mut out,
        "camera",
        format!("{} {}", image.camera.make, image.camera.model),
    );
    line(&mut out, "iso", format!("{:?}", image.iso));
    line(
        &mut out,
        "noise profile",
        format!("{:?}", image.noise_profile),
    );
    out
}

/// The fixture of `name`, or its writing under `AUR_UPDATE_GOLDEN`.
fn check_fixture(name: &str, text: &str, block: Option<&[u8]>) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest
        .join("tests/fixtures/decoder")
        .join(format!("{name}.txt"));
    if std::env::var_os("AUR_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        if let Some(block) = block {
            let seed = manifest
                .join("../../fuzz/corpus/raw_block")
                .join(format!("{name}.block"));
            std::fs::create_dir_all(seed.parent().unwrap()).unwrap();
            std::fs::write(seed, block).unwrap();
        }
        return;
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "the fixture {} is missing: run this test with AUR_UPDATE_GOLDEN=1 to write it, and read it before committing",
            path.display()
        )
    });
    assert!(
        stored == text,
        "the decoder's output for {name} is not the fixture's ({}). If the change is meant, run with \
         AUR_UPDATE_GOLDEN=1 and read the difference in the diff.\n--- stored\n{stored}--- now\n{text}",
        path.display()
    );
}

/// `rawler`'s image mapped to the values the plugin must return: written here a second time, independently of the
/// plugin's own mapping.
fn expected(native: &rawler::rawimage::RawImage) -> Expected {
    let rational = |r: &rawler::formats::tiff::Rational| {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };
    let b = &native.blacklevel;
    let patterned =
        !b.levels.is_empty() && b.levels.len() == b.height.max(1) * b.width.max(1) * b.cpp.max(1);
    let [r, g, bl, _] = native.wb_coeffs;
    let mut matrices: Vec<(u8, Vec<f32>)> = native
        .color_matrix
        .iter()
        .filter(|(_, v)| (v.len() == 9 || v.len() == 12) && v.iter().all(|x| x.is_finite()))
        .map(|(k, v)| (*k as u8, v.clone()))
        .collect();
    matrices.sort_by_key(|(k, _)| *k);
    Expected {
        width: native.width as u32,
        height: native.height as u32,
        black_pattern: patterned.then(|| (b.height.max(1), b.width.max(1), b.cpp.max(1))),
        black: if patterned {
            b.levels.iter().map(rational).collect()
        } else {
            vec![b.levels.iter().map(rational).sum::<f32>() / b.levels.len().max(1) as f32]
        },
        white: if native.whitelevel.0.is_empty() {
            vec![65535.0]
        } else {
            native.whitelevel.0.iter().map(|w| *w as f32).collect()
        },
        white_balance: (r.is_finite()
            && g.is_finite()
            && bl.is_finite()
            && r > 0.0
            && g > 0.0
            && bl > 0.0)
            .then_some([r, g, bl]),
        matrices,
        crop: native
            .crop_area
            .as_ref()
            .map(|a| (a.p.x, a.p.y, a.d.w, a.d.h)),
        active_area: native
            .active_area
            .as_ref()
            .map(|a| (a.p.x, a.p.y, a.d.w, a.d.h)),
        orientation: native.orientation.to_u16() as u8,
        make: native.clean_make.clone(),
        model: native.clean_model.clone(),
    }
}

struct Expected {
    width: u32,
    height: u32,
    /// `(rows, columns, components)` when `rawler` has a pattern, `None` when the level is one averaged value.
    black_pattern: Option<(usize, usize, usize)>,
    black: Vec<f32>,
    white: Vec<f32>,
    white_balance: Option<[f32; 3]>,
    matrices: Vec<(u8, Vec<f32>)>,
    crop: Option<(usize, usize, usize, usize)>,
    active_area: Option<(usize, usize, usize, usize)>,
    orientation: u8,
    make: String,
    model: String,
}

fn check_against_rawler(name: &str, wasm: &RawImage, native: rawler::rawimage::RawImage) {
    let want = expected(&native);
    assert_eq!(
        (wasm.width, wasm.height),
        (want.width, want.height),
        "{name}: readout"
    );
    // The layout.
    match (&wasm.layout, &native.photometric) {
        (
            SensorLayout::Cfa {
                width,
                height,
                colours,
            },
            RawPhotometricInterpretation::Cfa(config),
        ) => {
            assert_eq!(
                (usize::from(*width), usize::from(*height)),
                (config.cfa.width, config.cfa.height),
                "{name}: pattern size"
            );
            assert_eq!(
                colours,
                &config.cfa.flat_pattern(),
                "{name}: pattern colours"
            );
        }
        (SensorLayout::Mono, _) => assert_eq!(native.cpp, 1, "{name}: mono"),
        (
            SensorLayout::LinearRgb {
                components,
                profile,
            },
            _,
        ) => {
            assert_eq!(usize::from(*components), native.cpp, "{name}: components");
            assert_eq!(profile, &InputProfile::Unspecified, "{name}: profile");
        }
        (layout, photometric) => panic!("{name}: layout {layout:?} for {photometric:?}"),
    }
    // The samples, bit for bit.
    match (&wasm.samples, native.data) {
        (Samples::U16(a), RawImageData::Integer(b)) => {
            assert!(*a == b, "{name}: the samples differ")
        }
        (Samples::F32(a), RawImageData::Float(b)) => assert!(
            a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()),
            "{name}: the float samples differ"
        ),
        (wasm, _) => panic!(
            "{name}: the samples are of another type than rawler's ({:?})",
            wasm.kind()
        ),
    }
    // The levels.
    let black = &wasm.levels.black;
    match want.black_pattern {
        Some((rows, columns, components)) => assert_eq!(
            (
                usize::from(black.rows),
                usize::from(black.columns),
                usize::from(black.components)
            ),
            (rows, columns, components),
            "{name}: black pattern"
        ),
        None => assert_eq!(
            (black.rows, black.columns, black.components),
            (1, 1, 1),
            "{name}: averaged black"
        ),
    }
    assert_eq!(black.values, want.black, "{name}: black values");
    assert_eq!(wasm.levels.white, want.white, "{name}: white levels");
    assert_eq!(
        wasm.white_balance, want.white_balance,
        "{name}: white balance"
    );
    let matrices: Vec<(u8, Vec<f32>)> = wasm
        .colour
        .iter()
        .map(|m| (m.illuminant.0, m.xyz_to_camera.clone()))
        .collect();
    assert_eq!(matrices, want.matrices, "{name}: colour matrices");
    let rect = |r: &auroraw_plugin_api::Rect| {
        (
            r.x as usize,
            r.y as usize,
            r.width as usize,
            r.height as usize,
        )
    };
    assert_eq!(wasm.crop.as_ref().map(rect), want.crop, "{name}: crop");
    assert_eq!(
        wasm.active_area.as_ref().map(rect),
        want.active_area,
        "{name}: active area"
    );
    assert_eq!(
        wasm.orientation.to_exif(),
        want.orientation,
        "{name}: orientation"
    );
    assert_eq!(wasm.camera.make, want.make, "{name}: make");
    assert_eq!(wasm.camera.model, want.model, "{name}: model");
}

/// One sample file: the plugin against `rawler` and against the fixture.
fn check_sample(name: &str, fixture: &str) {
    let (Some(dir), Some(wasm)) = (samples_dir(name), plugin_wasm()) else {
        return;
    };
    let host = Arc::new(PluginHost::new().unwrap());
    let decoder = WasmDecoder::new(host, &wasm).unwrap();
    let bytes = std::fs::read(Path::new(&dir).join(name)).unwrap();

    let native = rawler::decode(
        &RawSource::new_from_slice(&bytes),
        &RawDecodeParams::default(),
    );
    let plugin = decoder.decode(&bytes);
    match (native, plugin) {
        (Err(_), Err(DecoderError::Invalid(_))) => {
            // A refusal is a routine outcome, and the same on both sides.
            check_fixture(fixture, "refused\n", None);
        }
        (Err(_), Err(DecoderError::Failed(_))) => {
            // `rawler` refuses the file and the plugin cannot even say so: it is stopped by the sandbox. This is the
            // Parrot Bebop DNG, whose TIFF structure makes `rawler` 0.8.0 allocate a size it read from the file before
            // checking it, which natively costs nothing (the pages are never touched) and in the sandbox reaches the
            // memory ceiling and traps. The caller sees `Failed`, not `Invalid`; both mean "cannot decode this file".
            // The fixture records it, so that a change of `rawler` that turns it into a refusal shows in the diff.
            check_fixture(fixture, "failed: stopped by the sandbox\n", None);
        }
        (Err(e), other) => {
            panic!("{name}: rawler refuses the file ({e}), the plugin says {other:?}")
        }
        (Ok(_), Err(e)) => panic!("{name}: the plugin cannot decode what rawler decodes: {e}"),
        (Ok(native), Ok(image)) => {
            check_against_rawler(name, &image, native);
            // The block of the image is what a reader of the fuzz corpus starts from.
            let block = image
                .to_block()
                .expect("what the plugin returned is a valid image");
            check_fixture(fixture, &dump(name, &image), Some(&block));
        }
    }
}

macro_rules! samples {
    ($($test:ident: $file:expr, $fixture:expr;)*) => {
        $(
            #[test]
            fn $test() {
                check_sample($file, $fixture);
            }
        )*
    };
}

samples! {
    canon_5d_iv_sraw_is_the_same_through_the_sandbox: "B13A0732.CR2", "canon-5d-iv-sraw";
    canon_r5_ii_is_the_same_through_the_sandbox: "canon-r5m2-CRAW.CR3", "canon-r5-ii";
    nikon_d850_is_the_same_through_the_sandbox: "Nikon-D850-14bit-compressed.NEF", "nikon-d850";
    sony_a7r_iv_is_the_same_through_the_sandbox: "DSC00396.ARW", "sony-a7r-iv";
    fujifilm_x_t50_xtrans_is_the_same_through_the_sandbox: "DSCF0120.RAF", "fujifilm-x-t50";
    panasonic_s5_is_the_same_through_the_sandbox: "dc-s5_6k4k.RW2", "panasonic-s5";
    olympus_e_m5_iii_is_the_same_through_the_sandbox: "PB290154.ORF", "olympus-e-m5-iii";
    leica_m9_is_the_same_through_the_sandbox: "L1049390.DNG", "leica-m9";
    canon_5d_iii_float_dng_is_the_same_through_the_sandbox: "Canon_-_EOS_5D_Mark_III_-_32bit_32bit_RAW.dng", "canon-5d-iii-float";
    fujifilm_gfx100s_ii_103_megapixels_is_the_same_through_the_sandbox: "Fujifilm_-_GFX100S_II_-_16bit_compressed_(4x3).RAF", "fujifilm-gfx100s-ii";
    leica_m_monochrom_has_no_matrix_and_no_white_balance: "Leica_-_M_Monochrom_-_16bit_(3x2).DNG", "leica-m-monochrom";
    eyedeas_e1_is_grbg_and_rotated: "Eyedeas_-_E1_-_16bit_(4x3).DNG", "eyedeas-e1";
    sony_a450_is_the_same_through_the_sandbox: "Sony_-_DSLR-A450_-_12bit_12bit_compressed_(3x2).ARW", "sony-a450";
    sony_a58_is_the_same_through_the_sandbox: "Sony_-_SLT-A58_-_12bit_12bit_compressed_(3x2).ARW", "sony-a58";
    samsung_sm_g973u_is_refused_without_make_and_model: "Samsung_-_SM-G973U_-_16bit_16bit_(2.1132075471698).dng", "samsung-sm-g973u";
    parrot_bebop_is_refused_for_its_compression: "PARROT_-_Bebop_Drone_-_16bit_(4x3).dng", "parrot-bebop";
    gopro_hero6_gpr_is_refused_for_its_compression: "GoPro_-_HERO6_Black_-_16bit_(4x3).GPR", "gopro-hero6";
}
