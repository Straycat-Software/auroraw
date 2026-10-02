// SPDX-License-Identifier: GPL-3.0-or-later
//! Decoder tests against the real per-maker sample files (testing strategy §3, §5): metadata, a
//! preview, a thumbnail and a deterministic hash for each; a truncated copy fails cleanly, never
//! a panic. Skipped locally when `testdata/samples/` has not been fetched
//! (`tools/fetch-samples.sh`); required in CI via `AUR_REQUIRE_SAMPLES=1`, so a missing fixture
//! there is a failure, not a silent skip (matching `AUR_REQUIRE_EXIFTOOL`'s own pattern).

use std::path::PathBuf;

const SAMPLES: &[&str] = &[
    "B13A0732.CR2",
    "canon-r5m2-CRAW.CR3",
    "Nikon-D850-14bit-compressed.NEF",
    "DSC00396.ARW",
    "DSCF0120.RAF",
    "dc-s5_6k4k.RW2",
    "PB290154.ORF",
    "L1049390.DNG",
];

/// Two real gaps in `rawler` 0.8, not bugs of this crate to paper over: Canon's "CRAW" compressed
/// variant of CR3 stores its preview trak in a codec `rawler` does not decode (its own warning
/// says "is it PQ/HEIF?"), and its ORF (Olympus) decoder implements neither `preview_image` nor
/// `thumbnail_image` at all. Both correctly return `None` rather than garbage, so this crate
/// correctly returns [`auroraw_imaging::ImagingError::NoPreview`]; decoding HEIF previews, or
/// finding Olympus's embedded JPEG some other way, needs more than this work package's chosen
/// stack. These two samples are tested for that specific, clean failure instead of a thumbnail.
const NO_PREVIEW: &[&str] = &["canon-r5m2-CRAW.CR3", "PB290154.ORF"];

fn samples_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/samples");
    if dir.join(SAMPLES[0]).is_file() {
        return Some(dir);
    }
    if std::env::var_os("AUR_REQUIRE_SAMPLES").is_some() {
        panic!("testdata/samples is required here but was not found; run tools/fetch-samples.sh");
    }
    eprintln!("testdata/samples not found: skipping (run tools/fetch-samples.sh)");
    None
}

#[test]
fn every_sample_yields_metadata_a_thumbnail_and_a_stable_hash() {
    let Some(dir) = samples_dir() else { return };
    for name in SAMPLES {
        let path = dir.join(name);

        // Metadata (make, model, the container's own data) never needs the preview codec, so
        // every sample, including the one with no decodable preview, gives it.
        let metadata =
            auroraw_imaging::read_metadata(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            metadata.make.as_deref().is_some_and(|s| !s.is_empty()),
            "{name}: no make"
        );
        assert!(
            metadata.model.as_deref().is_some_and(|s| !s.is_empty()),
            "{name}: no model"
        );

        if NO_PREVIEW.contains(name) {
            assert!(
                matches!(
                    auroraw_imaging::embedded_preview(&path),
                    Err(auroraw_imaging::ImagingError::NoPreview { .. })
                ),
                "{name}"
            );
            continue;
        }

        let (metadata, thumbnail, hash) =
            auroraw_imaging::process(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            metadata.pixel_width.unwrap_or(0) > 0,
            "{name}: no preview width"
        );
        assert!(
            metadata.pixel_height.unwrap_or(0) > 0,
            "{name}: no preview height"
        );

        assert!(!thumbnail.jpeg.is_empty(), "{name}: empty thumbnail");
        assert!(
            thumbnail.width <= 256 && thumbnail.height <= 256,
            "{name}: thumbnail bigger than 256 px"
        );
        assert_eq!(
            thumbnail.width.max(thumbnail.height),
            256,
            "{name}: not scaled to the long edge"
        );

        let preview =
            auroraw_imaging::embedded_preview(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            hash,
            auroraw_imaging::perceptual_hash(&preview),
            "{name}: hash is not deterministic"
        );
    }
}

#[test]
fn a_truncated_sample_fails_cleanly_not_a_panic() {
    let Some(dir) = samples_dir() else { return };
    let temp = auroraw_testkit::temp_dir();
    for name in SAMPLES {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let path = temp.path().join(name);
        // Aggressive enough that no format's header, let alone its preview data, survives: a
        // gentler cut (a third of the file) left some formats' small, early metadata block
        // intact, which is a legitimate, separate robustness property (partial data still gives
        // what it safely can), not what this test is checking.
        std::fs::write(&path, &bytes[..bytes.len().min(8192)]).unwrap();
        assert!(
            auroraw_imaging::embedded_preview(&path).is_err(),
            "{name}: a truncated file should not yield a preview"
        );
    }
}

/// The two Sony files whose orientation `rawler`'s decode does not report (note 005 §3.3b, item 3; D-141): the EXIF
/// of the Sony DSLR-A450 says 6 (turn 90 degrees clockwise) and that of the SLT-A58 says 8 (270), and `rawler::decode`
/// says `Normal` for both, while it reports `Rotate90` for the Eyedeas DNG. This is why the orientation of an image comes
/// from the EXIF that this crate reads, with the decoder's own as the fallback. It is a finding written as a test, not a
/// wish: if it fails because `rawler` now reports the rotation, that is good news, and the rule of D-141 can be looked at
/// again.
#[test]
fn the_exif_orientation_of_two_sony_files_is_not_the_one_rawlers_decode_reports() {
    let Some(dir) = samples_dir() else { return };
    for (name, exif) in [
        ("Sony_-_DSLR-A450_-_12bit_12bit_compressed_(3x2).ARW", 6),
        ("Sony_-_SLT-A58_-_12bit_12bit_compressed_(3x2).ARW", 8),
    ] {
        let path = dir.join(name);
        if !path.is_file() {
            if std::env::var_os("AUR_REQUIRE_SAMPLES").is_some() {
                panic!("testdata/samples/{name} is required here; run tools/fetch-samples.sh");
            }
            eprintln!("testdata/samples/{name} not found: skipping");
            continue;
        }
        let metadata =
            auroraw_imaging::read_metadata(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            metadata.orientation,
            Some(exif),
            "{name}: the EXIF orientation this crate reads"
        );
        let bytes = std::fs::read(&path).unwrap();
        let decoded = rawler::decode(
            &rawler::rawsource::RawSource::new_from_slice(&bytes),
            &rawler::decoders::RawDecodeParams::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            decoded.orientation.to_u16(),
            1,
            "{name}: rawler's decode reports Normal (the finding of note 005 §3.3b)"
        );
    }
}
