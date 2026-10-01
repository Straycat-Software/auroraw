// SPDX-License-Identifier: GPL-3.0-or-later
//! The QML suites (`tests/qml/tst_*.qml`), run by QtQuickTest's runner in a process of their own, off
//! screen, with real key and mouse events. Each test here is one suite on a machine of its own;
//! the results are written to a file and must end in a clean totals line.

mod support;

use std::path::Path;
use std::process::Command;

use auroraw_testkit::temp_dir;

/// Runs `tests/qml/tst_<suite>.qml` with `home` as the machine, and fails with the suite's own
/// report when a test in it fails.
fn run_suite(suite: &str, home: &Path, extra: Option<&Path>) {
    let report = home.join(format!("{suite}.txt"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_qml-test-runner"));
    command
        // The module is built into the runner (its QML is in the executable's resources).
        .arg("-import")
        .arg("qrc:/qt/qml")
        .arg("-input")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/qml")
                .join(format!("tst_{suite}.qml")),
        )
        .arg("-o")
        .arg(format!("{},txt", report.display()))
        // The report goes to the console too, since a run that dies leaves the file empty.
        .arg("-o")
        .arg("-,txt")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("QT_QUICK_CONTROLS_STYLE", "Fusion")
        // The machine's own language must not change what the suites read.
        .env("LC_ALL", "C.UTF-8")
        .env("AURORAW_TEST_HOME", home)
        // The places file of the suites about place names (design note 008): where `support::write_places_pack` puts it.
        // A machine without one has no place names, which is what the other suites expect.
        .env("AURORAW_PLACES", home.join("places.sqlite"));
    if let Some(extra) = extra {
        command.env("AURORAW_TEST_EXTRA", extra);
    }
    let output = command.output().expect("the runner starts");
    let text = std::fs::read_to_string(&report).unwrap_or_default();
    let clean = text
        .lines()
        .any(|line| line.starts_with("Totals: ") && line.contains(" 0 failed"));
    assert!(
        output.status.success() && clean,
        "suite {suite} failed ({}):\n{text}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The suites that make their own machines (folders under the one they are given).
#[test]
fn opening_and_creating_workspaces() {
    let home = temp_dir();
    support::write_photos(&home.path().join("Template2"), "IMG", 2);
    run_suite("launch", home.path(), None);
}

/// The interface's own controls, each on its own, with no application around them (D-136).
#[test]
fn the_controls_of_the_interface() {
    let home = temp_dir();
    run_suite("controls", home.path(), None);
}

#[test]
fn the_hamburger_menu_and_its_commands() {
    let home = temp_dir();
    run_suite("menus", home.path(), None);
}

#[test]
fn modal_dialogs_settings_and_the_language() {
    let home = temp_dir();
    run_suite("dialogs", home.path(), None);
}

#[test]
fn the_grid_on_a_machine_with_photos() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 80);
    let extra = home.path().join("Extra");
    support::write_photos(&extra, "EXTRA", 5);
    run_suite("grid", home.path(), Some(&extra));
}

/// A photo whose file has gone (an unplugged card, a deleted picture) is listed but has no thumbnail
/// to make: its cell says so, and the others show theirs.
#[test]
fn a_photo_that_no_thumbnail_can_be_made_for_says_so() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 12);
    std::fs::remove_file(home.path().join("Card").join("IMG_0005.jpg")).unwrap();
    run_suite("thumbnails", home.path(), None);
}

#[test]
fn a_scan_reaches_the_grid_and_makes_thumbnails_ahead() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 20);
    let extra = home.path().join("Extra");
    support::write_photos(&extra, "EXTRA", 5);
    run_suite("scan", home.path(), Some(&extra));
}

/// The catalogue task on machines that start empty: folders of generated photos to copy from.
#[test]
fn the_catalogue_task_adds_scans_removes_and_restores_sources() {
    let home = temp_dir();
    support::write_photos(&home.path().join("Template2"), "IMG", 2);
    support::write_photos(&home.path().join("Template3"), "IMG", 3);
    support::write_photos(&home.path().join("One"), "NEW", 1);
    run_suite("catalogue", home.path(), None);
}

/// Import, on machines that start empty: folders of generated photos to copy from.
#[test]
fn importing_photos_from_a_card_or_a_folder() {
    let home = temp_dir();
    support::write_photos(&home.path().join("Template2"), "IMG", 2);
    support::write_photos(&home.path().join("Template3"), "IMG", 3);
    support::write_photos(&home.path().join("Big"), "IMG", 40);
    support::write_photos(&home.path().join("Cam100"), "IMG", 1);
    support::write_photos(&home.path().join("Cam101"), "CAM", 1);
    // Two photos taken in the West of Aland (the places file of design note 008's suites), for the option of finding
    // their place names.
    support::write_photos(&home.path().join("TemplateGps"), "IMG", 2);
    for n in 0..2 {
        support::put_gps_in_jpeg(
            &home
                .path()
                .join("TemplateGps")
                .join(format!("IMG_000{n}.jpg")),
            5.0,
            2.1,
        );
    }
    support::write_places_pack(&home.path().join("places.sqlite"));
    run_suite("import", home.path(), None);
}

/// Offline place names: the dialog, the selection and a source, what a person typed, one undo, the language, the
/// refresh, the credit in About. Positions in the sidecars and a small places file (design note 008).
#[test]
fn place_names_are_found_for_the_selection_or_a_source_and_undone_as_one_step() {
    let home = temp_dir();
    support::machine_with_places(home.path(), 12);
    run_suite("places", home.path(), None);
}

/// The place menu of the filter bar, on its own against a stand-in for the engine's facets (design note 008 §5).
#[test]
fn the_place_menu_shows_the_tree_of_places_and_sets_the_filter() {
    let home = temp_dir();
    run_suite("placemenu", home.path(), None);
}

/// Without the places file the feature says so and the rest is unaffected.
#[test]
fn place_names_say_they_are_not_installed_when_the_file_is_missing() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 6);
    run_suite("placesabsent", home.path(), None);
}

/// The application itself, started the way `auroraw-app` does (not through QtQuickTest, which sets
/// the QML import path for it), says nothing about its QML: no unresolved theme, no failed binding.
#[test]
fn the_application_starts_without_a_word_about_its_qml() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 6);
    let output = Command::new(env!("CARGO_BIN_EXE_qml-test-runner"))
        .arg("--app")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("LC_ALL", "C.UTF-8")
        .env("AURORAW_TEST_HOME", home.path())
        .env("AURORAW_TEST_QUIT_MS", "3000")
        .output()
        .expect("the application starts");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the application failed:\n{said}");
    // What the platform says of itself (fonts, plug-ins) is not the QML's.
    let about_qml: Vec<&str> = said
        .lines()
        .filter(|line| line.contains("qrc:") || line.contains("QML") || line.contains("TypeError"))
        .collect();
    assert!(
        about_qml.is_empty(),
        "the application said something about its QML:\n{}",
        about_qml.join("\n")
    );
}

/// Every view drawn (to PNG when `AUR_SNAPSHOT_DIR` says where), in English and in French.
#[test]
fn every_view_can_be_shown_and_drawn_in_both_languages() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 60);
    support::write_photos(&home.path().join("Template3"), "IMG", 3);
    support::write_photos(&home.path().join("Cam100"), "IMG", 1);
    support::write_photos(&home.path().join("Cam101"), "CAM", 1);
    run_suite("views", home.path(), None);
}

/// Selecting several photos of the grid: clicks and keys with Shift and Ctrl, the rubber band, and rating a
/// selection as one step.
#[test]
fn selecting_several_photos_in_the_grid() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 80);
    run_suite("selection", home.path(), None);
}

/// Flags: P, X and U, the same key taking the flag off, the flag filter, undo, and rejected photos.
#[test]
fn flagging_photos_in_the_grid() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("flags", home.path(), None);
}

/// Keywords: the panel's field, the tri-state check, undo, the hierarchy, type-ahead, rename.
#[test]
fn keywords_in_the_panel() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("keywords", home.path(), None);
}

/// The metadata panel (WP10, slice 1): a field applied to one photo or a selection, "Multiple
/// values", undo and redo, the two list fields, a custom field through the engine directly.
#[test]
fn the_metadata_panel_edits_a_photos_or_a_selections_fields() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("metadata", home.path(), None);
}

/// The Info panel: the active photo's own technical metadata, read-only, a field left out when the
/// photo does not carry it, following the grid's cursor, French.
#[test]
fn the_info_panel_shows_a_photos_technical_metadata() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("info", home.path(), None);
}

/// The collections tab (WP10, slice 3): made from the field with the selection in it, the tri-state check,
/// collections inside collections, the list filtered by one, rename, move, drag and drop, delete, undo, French.
#[test]
fn collections_in_the_tab() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("collections", home.path(), None);
}

/// Reorganising the vocabulary: drag and drop, the Move dialog, deleting a branch, undo.
#[test]
fn reorganising_the_keyword_tree() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("keyword_tree", home.path(), None);
}

/// The image view: opening, walking, rating with keys, 100 %, full screen, auto-advance.
#[test]
fn the_image_view_and_cull_mode() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 40);
    run_suite("viewer", home.path(), None);
}

/// Series: a burst and a bracket collapsed in the grid, opened in place, acted on as one, resolved, made by hand.
#[test]
fn series_in_the_grid() {
    let home = temp_dir();
    support::machine_with_series(home.path(), 40, 5);
    run_suite("series", home.path(), None);
}

/// Similar photos: the panel that suggests the photos that look alike, and groups or compares them.
#[test]
fn similar_photos_are_suggested_and_grouped() {
    let home = temp_dir();
    support::machine_with_similar(home.path(), 30);
    run_suite("similar", home.path(), None);
}

/// The duplicates report: a photo found at two locations, listed with both, "Show in file manager" per location.
#[test]
fn exact_duplicates_are_reported_with_every_location() {
    let home = temp_dir();
    support::machine_with_duplicate(home.path(), 20);
    run_suite("duplicates", home.path(), None);
}

/// Changes other applications make to the XMP files beside originals: the banner, the review window, accepting,
/// declining and choosing where both sides changed.
///
/// (It was ignored on Windows while issue #27 was open: the first click on "Review…" was lost intermittently there. A
/// click sent before the frame that gives a banner or a window its contents is drawn is lost; the suite now waits
/// for that frame, `drawn()` of `AppTestCase.qml`.)
#[test]
fn external_xmp_changes_are_announced_reviewed_and_answered() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 20);
    run_suite("external", home.path(), None);
}

/// The XMP export to the source folders: the command, the form, the files written beside the originals, a file another
/// application changed held back, Replace asking first, the choices remembered.
#[test]
fn the_xmp_export_writes_files_beside_the_originals() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 20);
    run_suite("xmpexport", home.path(), None);
}

/// Comparing frames: pages, marks, resolving from the comparison, linked zoom, the aids, the thumbnail size.
#[test]
fn comparing_frames_and_the_quality_aids() {
    let home = temp_dir();
    support::machine_with_series(home.path(), 40, 5);
    run_suite("compare", home.path(), None);
}
