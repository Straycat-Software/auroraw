// SPDX-License-Identifier: GPL-3.0-or-later
//! Interoperability: ExifTool reads back what Auroraw writes (M1 exit criterion 8, design note 003).
//!
//! Runs when `exiftool` is on the path. Continuous integration installs it and sets
//! `AUR_REQUIRE_EXIFTOOL=1`, so that a missing tool there is a failure, not a silent skip.

use std::path::{Path, PathBuf};
use std::process::Command;

use auroraw_format::sidecar::export::{ExportView, build, merge_into};
use auroraw_format::sidecar::{Flag, Metadata, Original};
use auroraw_format::xmp::Xmp;

fn exiftool(args: &[&str], file: &Path) -> Option<String> {
    match Command::new("exiftool").args(args).arg(file).output() {
        Ok(out) => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        Err(_) if std::env::var_os("AUR_REQUIRE_EXIFTOOL").is_none() => {
            eprintln!("exiftool not found: skipping");
            None
        }
        Err(e) => panic!("exiftool is required here but could not be run: {e}"),
    }
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sidecars")
        .join(name)
}

/// ExifTool prints a one-item list as a plain value.
fn list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(items) => items
            .iter()
            .map(|i| i.as_str().unwrap_or_default().to_string())
            .collect(),
        other => vec![other.as_str().unwrap_or_default().to_string()],
    }
}

#[test]
fn a_photo_sidecar_validates_and_is_read_where_other_software_looks() {
    let file = fixture("photo-full.xmp");
    let Some(check) = exiftool(&["-validate", "-warning", "-error", "-a", "-G1"], &file) else {
        return;
    };
    assert!(
        check.contains("Validate") && check.contains(": OK"),
        "{check}"
    );
    assert!(
        !check.contains("Warning") && !check.contains("Error"),
        "{check}"
    );

    let json = exiftool(&["-json", "-G1"], &file).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tags = &value[0];
    assert_eq!(tags["XMP-xmp:Rating"], 4);
    assert_eq!(tags["XMP-xmp:Label"], "Green");
    assert_eq!(tags["XMP-dc:Title"], "Heron at dawn");
    assert_eq!(tags["XMP-dc:Description"], "A grey heron & its reflection");
    assert_eq!(list(&tags["XMP-dc:Subject"]), ["Heron", "Quebec"]);
    assert_eq!(
        list(&tags["XMP-lr:HierarchicalSubject"]),
        ["Fauna|Birds|Heron", "Places|Canada|Quebec"]
    );
    assert_eq!(list(&tags["XMP-dc:Creator"]), ["Marie Tremblay"]);
    assert_eq!(tags["XMP-photoshop:City"], "Roberval");
    assert_eq!(tags["XMP-tiff:Make"], "SONY");
    assert_eq!(tags["XMP-aur:PhotoId"], "3f2a91c0d77e4b5a8c1e0f9d2b6a4c31");
}

#[test]
fn a_version_sidecar_shows_its_own_values_to_other_software() {
    let file = fixture("version-overrides.xmp");
    let Some(json) = exiftool(&["-json", "-G1"], &file) else {
        return;
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value[0]["XMP-xmp:Rating"], 1);
    assert_eq!(value[0]["XMP-dc:Title"], "Its own title");
    assert_eq!(
        value[0]["XMP-dc:Description"],
        "A grey heron & its reflection"
    );
}

/// A file of the export's tests, in the target folder of the tests.
fn written(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("exiftool-export");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::write(&file, bytes).unwrap();
    file
}

fn exported_photo() -> Metadata {
    Metadata {
        rating: Some(4),
        flag: Some(Flag::Picked),
        label: Some("Green".into()),
        title: Some("Heron at dawn".into()),
        caption: Some("A grey heron & its reflection".into()),
        creator: vec!["Marie Tremblay".into()],
        city: Some("Roberval".into()),
        original: Original {
            make: Some("SONY".into()),
            ..Original::default()
        },
        ..Metadata::default()
    }
}

fn valid(file: &Path) -> bool {
    let Some(check) = exiftool(&["-validate", "-warning", "-error", "-a", "-G1"], file) else {
        return false;
    };
    assert!(
        check.contains("Validate") && check.contains(": OK"),
        "{check}"
    );
    assert!(
        !check.contains("Warning") && !check.contains("Error"),
        "{check}"
    );
    true
}

fn tags(file: &Path) -> serde_json::Value {
    let json = exiftool(&["-json", "-G1"], file).unwrap();
    serde_json::from_str::<serde_json::Value>(&json).unwrap()[0].clone()
}

#[test]
fn an_export_validates_and_is_read_where_other_software_looks() {
    let keywords = vec![
        "Fauna|Birds|Heron".to_string(),
        "Places|Canada|Quebec".into(),
    ];
    let meta = exported_photo();
    let bytes = build(&ExportView {
        meta: &meta,
        keywords: &keywords,
        rejected_as_minus_one: true,
    })
    .to_bytes();
    let file = written("new.xmp", &bytes);
    if !valid(&file) {
        return;
    }
    let tags = tags(&file);
    assert_eq!(tags["XMP-xmp:Rating"], 4);
    assert_eq!(tags["XMP-xmp:Label"], "Green");
    assert_eq!(tags["XMP-dc:Title"], "Heron at dawn");
    assert_eq!(tags["XMP-dc:Description"], "A grey heron & its reflection");
    assert_eq!(list(&tags["XMP-dc:Subject"]), ["Heron", "Quebec"]);
    assert_eq!(
        list(&tags["XMP-lr:HierarchicalSubject"]),
        ["Fauna|Birds|Heron", "Places|Canada|Quebec"]
    );
    assert_eq!(list(&tags["XMP-dc:Creator"]), ["Marie Tremblay"]);
    assert_eq!(tags["XMP-photoshop:City"], "Roberval");
    assert_eq!(tags["XMP-tiff:Make"], "SONY");
    assert_eq!(tags["XMP-aur:Export"], 1);
    assert!(
        tags.get("XMP-aur:PhotoId").is_none(),
        "no identifier of Auroraw's leaves"
    );
}

#[test]
fn an_export_from_below_sea_level_reads_as_below_sea_level_to_other_software() {
    let meta = Metadata {
        original: Original {
            gps_latitude: Some("31,30.0000N".into()),
            gps_longitude: Some("35,28.0000E".into()),
            gps_altitude: Some("4300/10".into()),
            gps_altitude_ref: Some("1".into()),
            ..exported_photo().original
        },
        ..exported_photo()
    };
    let bytes = build(&ExportView {
        meta: &meta,
        keywords: &[],
        rejected_as_minus_one: true,
    })
    .to_bytes();
    let file = written("dead-sea.xmp", &bytes);
    if !valid(&file) {
        return;
    }
    let tags = tags(&file);
    assert_eq!(tags["XMP-exif:GPSAltitudeRef"], "Below Sea Level");
    assert!(tags["XMP-exif:GPSAltitude"].to_string().contains("430"));
    // (ExifTool's own composite says both together: "430 m Below Sea Level".)
    if let Some(composite) = tags.get("Composite:GPSAltitude") {
        assert!(
            composite.to_string().contains("Below Sea Level"),
            "{composite}"
        );
    }
}

#[test]
fn a_rejected_export_reads_as_rejected_to_other_software() {
    let meta = Metadata {
        flag: Some(Flag::Rejected),
        rating: Some(3),
        ..exported_photo()
    };
    let bytes = build(&ExportView {
        meta: &meta,
        keywords: &[],
        rejected_as_minus_one: true,
    })
    .to_bytes();
    let file = written("rejected.xmp", &bytes);
    if !valid(&file) {
        return;
    }
    let tags = tags(&file);
    assert_eq!(tags["XMP-xmp:Rating"], -1);
    assert_eq!(tags["XMP-aur:Flag"], "rejected");
    assert_eq!(tags["XMP-aur:Stars"], 3);
}

#[test]
fn a_merge_leaves_the_other_applications_develop_settings_as_exiftool_reads_them() {
    for name in ["lightroom.xmp", "darktable.xmp", "digikam.xmp"] {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/xmp")
            .join(name);
        let mut xmp = Xmp::from_bytes(&std::fs::read(&original).unwrap()).unwrap();
        let meta = exported_photo();
        merge_into(
            &mut xmp,
            &ExportView {
                meta: &meta,
                keywords: &["Fauna|Birds|Heron".to_string()],
                rejected_as_minus_one: true,
            },
        );
        let merged = written(&format!("merged-{name}"), &xmp.to_bytes());
        if !valid(&merged) {
            return;
        }
        let (before, after) = (tags(&original), tags(&merged));
        let foreign: Vec<&String> = before
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with("XMP-crs:") || k.starts_with("XMP-darktable:"))
            .collect();
        for key in &foreign {
            assert_eq!(before[key.as_str()], after[key.as_str()], "{name}: {key}");
        }
        if name != "digikam.xmp" {
            assert!(!foreign.is_empty(), "{name} has develop settings to keep");
        }
        assert_eq!(after["XMP-xmp:Rating"], 4, "{name}");
        assert_eq!(after["XMP-dc:Title"], "Heron at dawn", "{name}");
    }
}
