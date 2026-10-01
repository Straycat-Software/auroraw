// SPDX-License-Identifier: GPL-3.0-or-later
//! Developer commands, run as `cargo xtask <command>` (continuous integration §4).
//!
//! - `spdx`: every source file starts with its SPDX licence identifier.
//! - `layers`: the crates depend on each other only as the architecture allows (§3.2), and a
//!   permissively licensed crate depends on nothing under the GPL (decision D-080). A
//!   dev-dependency (test-only, never linked into a shipped artifact) is unrestricted: a crate's
//!   tests may set up fixtures with any sibling crate.
//! - `check`: both of the above.
//! - `manual-images`: redraws the pictures of the user manual (`docs/manual/images/`) from the interface's own tests.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const GPL: &str = "// SPDX-License-Identifier: GPL-3.0-or-later";
const PERMISSIVE: &str = "// SPDX-License-Identifier: MIT OR Apache-2.0";

/// The crates of the workspace and the workspace crates each one may depend on **as a normal or
/// build dependency** (architecture §3.1). Dev-dependencies are not restricted by this table
/// (see `layers` below).
const ALLOWED: &[(&str, &[&str])] = &[
    ("auroraw-types", &[]),
    ("auroraw-format", &["auroraw-types"]),
    ("auroraw-catalogue", &["auroraw-format", "auroraw-types"]),
    ("auroraw-workspace", &["auroraw-format", "auroraw-types"]),
    ("auroraw-plugin-api", &[]),
    ("auroraw-plugin-host", &["auroraw-plugin-api"]),
    (
        "auroraw-sources",
        &["auroraw-plugin-api", "auroraw-types", "auroraw-format"],
    ),
    ("auroraw-imaging", &["auroraw-types"]),
    // Offline place names (design note 008): a lookup in a file, nothing of the application.
    ("auroraw-places", &[]),
    // The image engine. Its edges are added when the code first needs them (D-140 writes the ones
    // design note 005 §2.2 names: `plugin-api` for `RawImage`, `imaging`): an allowed edge that is
    // unused is a hole in this check, which only refuses what is not in the table.
    ("auroraw-pipeline", &[]),
    (
        "auroraw-import",
        &[
            "auroraw-format",
            "auroraw-plugin-api",
            "auroraw-sources",
            "auroraw-types",
        ],
    ),
    (
        "auroraw-engine",
        &[
            "auroraw-catalogue",
            "auroraw-workspace",
            "auroraw-format",
            "auroraw-sources",
            "auroraw-plugin-api",
            "auroraw-import",
            "auroraw-imaging",
            "auroraw-places",
            "auroraw-plugin-host",
            "auroraw-types",
        ],
    ),
    (
        "auroraw-ui",
        &[
            "auroraw-catalogue",
            "auroraw-engine",
            "auroraw-imaging",
            "auroraw-types",
        ],
    ),
    (
        "auroraw-cli",
        &[
            "auroraw-engine",
            "auroraw-catalogue",
            "auroraw-format",
            "auroraw-types",
            "auroraw-sources",
        ],
    ),
    (
        "auroraw-app",
        &["auroraw-ui", "auroraw-engine", "auroraw-types"],
    ),
    ("auroraw-testkit", &[]),
    ("xtask", &[]),
];

fn main() -> ExitCode {
    let ok = match std::env::args().nth(1).as_deref() {
        Some("spdx") => spdx(),
        Some("layers") => layers(),
        Some("check") => {
            let a = spdx();
            let b = layers();
            a && b
        }
        Some("manual-images") => manual_images(),
        _ => {
            eprintln!("usage: cargo xtask <spdx|layers|check|manual-images>");
            return ExitCode::from(2);
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The pictures of the user manual: the name the interface's tests draw a view under (in English, under
/// `views/` of the folder `AUR_SNAPSHOT_DIR` names), and the name it has in `docs/manual/images/`.
const MANUAL_IMAGES: &[(&str, &str)] = &[
    ("welcome-empty-en", "welcome"),
    ("new-workspace-dialog-en", "new-workspace"),
    ("menu-file-en", "menu-file"),
    ("settings-dialog-en", "settings"),
    ("catalogue-with-a-source", "catalogue"),
    ("add-source-dialog", "add-source"),
    ("remove-source-dialog-en", "remove-source"),
    ("import-dialog", "import"),
    ("grid-en", "grid"),
    ("grid-menu-en", "grid-menu"),
    ("viewer-en", "viewer"),
    ("viewer-state-en", "viewer-state"),
    ("series-collapsed-en", "series-collapsed"),
    ("series-open-en", "series-open"),
    ("similar-en", "similar"),
    ("compare-en", "compare"),
    ("compare-aids-en", "compare-aids"),
    ("grid-large-en", "grid-large"),
    ("keywords-add-en", "keywords-add"),
    ("keywords-drag-en", "keywords-drag"),
    ("duplicates-en", "duplicates"),
    ("xmp-export", "xmp-export"),
    ("xmp-export-held-back", "xmp-export-held-back"),
    ("place-names", "place-names"),
    ("place-names-preview", "place-names-preview"),
    ("place-names-done", "place-names-done"),
    ("metadata-en", "metadata"),
    ("info-en", "info"),
    ("collections-en", "collections"),
];

/// Runs the interface's QML suites with `AUR_SNAPSHOT_DIR` set (they draw every view to a PNG) and copies the
/// pictures the manual shows to `docs/manual/images/`. The pictures are the current interface, drawn with the
/// tests' generated photographs.
fn manual_images() -> bool {
    let root = root();
    let scratch = root.join("target").join("manual-images");
    let _ = std::fs::remove_dir_all(&scratch);
    if std::fs::create_dir_all(&scratch).is_err() {
        eprintln!("cannot create {}", scratch.display());
        return false;
    }
    let status = Command::new("cargo")
        .args(["test", "-p", "auroraw-ui", "--test", "qml"])
        .env("AUR_SNAPSHOT_DIR", &scratch)
        .current_dir(&root)
        .status();
    if !status.is_ok_and(|s| s.success()) {
        eprintln!("the interface's tests failed: no pictures were copied");
        return false;
    }
    let target = root.join("docs").join("manual").join("images");
    let _ = std::fs::create_dir_all(&target);
    let mut ok = true;
    for (from, to) in MANUAL_IMAGES {
        let source = scratch.join("views").join(format!("{from}.png"));
        match std::fs::copy(&source, target.join(format!("{to}.png"))) {
            Ok(_) => println!("{from} -> docs/manual/images/{to}.png"),
            Err(e) => {
                eprintln!("{}: {e}", source.display());
                ok = false;
            }
        }
    }
    ok
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn spdx() -> bool {
    let root = root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("xtask"), &mut files);
    // `plugins/` is a separate Cargo workspace (WP6: it targets wasm32-wasip1, not the host), so
    // `layers()`'s `cargo metadata` never sees it; its licence headers are still checked here.
    rust_files(&root.join("plugins"), &mut files);
    files.sort();
    let mut ok = true;
    for file in &files {
        let permissive = file.starts_with(root.join("crates").join("plugin-api"));
        let expected = if permissive { PERMISSIVE } else { GPL };
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if text.lines().next() != Some(expected) {
            eprintln!(
                "{}: the first line must be `{expected}`",
                file.strip_prefix(&root).unwrap_or(file).display()
            );
            ok = false;
        }
    }
    println!(
        "spdx: {} files checked{}",
        files.len(),
        if ok { "" } else { ", with errors" }
    );
    ok
}

fn layers() -> bool {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .expect("cargo metadata");
    if !out.status.success() {
        eprintln!("cargo metadata failed");
        return false;
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata is JSON");
    let packages = meta["packages"].as_array().expect("packages");
    let names: Vec<&str> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut ok = true;
    for name in &names {
        if !ALLOWED.iter().any(|(n, _)| n == name) {
            eprintln!(
                "{name}: not in the table of allowed dependencies (xtask/src/main.rs); add it"
            );
            ok = false;
        }
    }
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        let license = package["license"].as_str().unwrap_or_default();
        let allowed = ALLOWED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, a)| *a)
            .unwrap_or(&[]);
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if !names.contains(&dep_name) {
                continue; // an external crate: `cargo deny` checks those
            }
            // A dev-dependency is test-only: cargo never links it into a shipped library or
            // binary, so it cannot leak a GPL crate into a permissive one or blur the runtime
            // layering. Tests may freely set up fixtures with any sibling crate.
            let dev = dep["kind"].as_str() == Some("dev");
            if !dev && !allowed.contains(&dep_name) {
                eprintln!("{name} must not depend on {dep_name} (architecture §3.2)");
                ok = false;
            }
            let dep_license = packages
                .iter()
                .find(|p| p["name"].as_str() == Some(dep_name))
                .and_then(|p| p["license"].as_str())
                .unwrap_or_default();
            if license.contains("MIT") && !dev && !dep_license.contains("MIT") {
                eprintln!(
                    "{name} is permissively licensed but depends on {dep_name} ({dep_license}) (D-080)"
                );
                ok = false;
            }
        }
    }
    println!(
        "layers: {} crates checked{}",
        names.len(),
        if ok { "" } else { ", with errors" }
    );
    ok
}
