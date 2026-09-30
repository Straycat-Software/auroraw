// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP export to the source folders (spec §5.7, D-024; design note 003 §8.1), through the real
//! production path (`Command::ExportXmp`, `xmp_export_job`, the coordinator): a file is written beside
//! the original, merged into one that exists (develop settings kept), held back for the review when
//! another application changed it, never reported as an external change afterwards, and the source is
//! touched by nothing else.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use auroraw_engine::{
    AddSourceRequest, ColourLabel, Command, Engine, Event, EventReceiver, Flag, JobId,
    MetadataField, Outcome, XmpExisting, XmpExportOptions, XmpExportReport, XmpNaming, XmpScope,
};
use auroraw_format::sidecar::Metadata;
use auroraw_format::sidecar::external::read;
use auroraw_format::xmp::{Xmp, ns};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::{KeywordId, PhotoId, SourceId};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Setup {
    dir: TempDir,
    engine: Engine,
    events: EventReceiver,
}

fn setup() -> Setup {
    let dir = temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    Setup {
        dir,
        engine,
        events,
    }
}

fn jpeg(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(32, 24, |x, y| {
        Rgb([(x * 8) as u8 ^ seed, (y * 9) as u8, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

/// A second kind of original for a name that is not a JPEG's: the picture is a JPEG all the same (the
/// engine's `image` has no other codec), which the scan takes for a photo by its name.
fn png(path: &Path, seed: u8) {
    jpeg(path, seed);
}

fn foreign(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../format/tests/fixtures/xmp/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

static CLOCK: AtomicU64 = AtomicU64::new(1_700_000_000);

/// Writes a file the way another application would, with a modification time of its own (never the same
/// as the last one written), so that nothing depends on the file system's granularity.
fn other_app_writes(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    let at = UNIX_EPOCH + Duration::from_secs(CLOCK.fetch_add(10, Ordering::SeqCst));
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

fn scanned(events: &EventReceiver, source: SourceId) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::ExternalChanges {
                source_id: Some(id),
                ..
            }) if id == source => return,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
}

fn add(s: &Setup, root: &Path) -> SourceId {
    let added = s
        .engine
        .add_source(AddSourceRequest {
            root: root.to_path_buf(),
            name: None,
            merge: false,
        })
        .unwrap();
    scanned(&s.events, added.source_id);
    added.source_id
}

fn rescan(s: &Setup, source: SourceId) {
    s.engine
        .submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: Vec::new(),
        })
        .unwrap();
    scanned(&s.events, source);
}

fn photo_named(s: &Setup, filename: &str) -> PhotoId {
    s.engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 1000)
        .unwrap()
        .into_iter()
        .find(|row| row.filename == filename)
        .unwrap_or_else(|| panic!("no photo {filename}"))
        .id
}

fn meta(s: &Setup, photo: PhotoId) -> Metadata {
    s.engine
        .workspace()
        .read_photo(&photo)
        .unwrap()
        .unwrap()
        .current()
        .unwrap()
        .meta
}

fn run(s: &Setup, command: Command) {
    s.engine.submit_and_wait(command).unwrap();
}

fn set_title(s: &Setup, photo: PhotoId, title: &str) {
    run(
        s,
        Command::SetMetadataField {
            photo_id: photo,
            field: MetadataField::Title,
            value: title.into(),
        },
    );
}

fn set_rating(s: &Setup, photo: PhotoId, rating: u8) {
    run(
        s,
        Command::SetRating {
            photo_id: photo,
            rating,
        },
    );
}

fn create_keyword(s: &Setup, name: &str, parent: Option<KeywordId>) -> KeywordId {
    let Outcome::KeywordCreated(id) = s
        .engine
        .submit_and_wait(Command::CreateKeyword {
            name: name.into(),
            parent,
            id: None,
        })
        .unwrap()
    else {
        panic!("expected KeywordCreated");
    };
    id
}

struct Exported {
    report: XmpExportReport,
    cancelled: bool,
    /// The number the banner would show, when the export made the coordinator report one.
    waiting: Option<usize>,
}

fn wait_export(s: &Setup, job: JobId) -> Exported {
    let deadline = Instant::now() + Duration::from_secs(60);
    let (mut waiting, mut finished) = (None, None);
    loop {
        match s.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::ExternalChanges {
                source_id: None,
                photos,
                ..
            }) => waiting = Some(photos),
            Some(Event::XmpExportFinished {
                job: j,
                report,
                cancelled,
            }) if j == job => finished = Some((report, cancelled)),
            Some(Event::JobFinished(j) | Event::JobCancelled(j)) if j == job => {
                let (report, cancelled) = finished.expect("the report comes before the end");
                return Exported {
                    report,
                    cancelled,
                    waiting,
                };
            }
            _ => assert!(Instant::now() < deadline, "the export never finished"),
        }
    }
}

fn export_with(s: &Setup, scope: XmpScope, options: XmpExportOptions) -> Exported {
    let Outcome::XmpExportStarted { job, .. } = s
        .engine
        .submit_and_wait(Command::ExportXmp { scope, options })
        .unwrap()
    else {
        panic!("expected XmpExportStarted");
    };
    wait_export(s, job)
}

fn export(s: &Setup, photos: &[PhotoId]) -> Exported {
    export_with(
        s,
        XmpScope::Photos(photos.to_vec()),
        XmpExportOptions::default(),
    )
}

fn listing(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let mut all = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                all.push((
                    path.strip_prefix(folder).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    all.sort();
    all
}

fn names(folder: &Path) -> Vec<String> {
    listing(folder).into_iter().map(|(n, _)| n).collect()
}

/// A property the export owns or stamps: what a merge is allowed to change (`xmp:MetadataDate` is said anew
/// whenever the metadata of a file changes).
fn owned_or_not(p: &auroraw_format::xmp::Property) -> bool {
    p.is(ns::XMP, "MetadataDate")
        || auroraw_format::sidecar::export::OWNED
            .iter()
            .any(|(n, name)| p.is(n, name))
}

fn has_date(file: &Path) -> bool {
    Xmp::from_bytes(&std::fs::read(file).unwrap())
        .unwrap()
        .get(ns::XMP, "MetadataDate")
        .is_some()
}

/// The copies of other applications' files under the workspace's `removed/external-xmp/`.
fn kept_copies(s: &Setup) -> Vec<(String, Vec<u8>)> {
    let removed = s.dir.path().join("Main").join("removed");
    if !removed.exists() {
        return Vec::new();
    }
    listing(&removed)
        .into_iter()
        .filter(|(name, _)| name.contains("external-xmp") && name.ends_with(".xmp"))
        .collect()
}

fn mtime(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn a_photo_with_no_file_gets_one_beside_its_original_and_nothing_else_is_written() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 1);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    set_rating(&s, photo, 4);
    set_title(&s, photo, "Heron at dawn");
    run(
        &s,
        Command::SetLabel {
            photo_id: photo,
            label: Some(ColourLabel::Green),
        },
    );
    let heron = create_keyword(&s, "Heron", None);
    run(
        &s,
        Command::AddKeyword {
            photo_id: photo,
            keyword_id: heron,
        },
    );
    let before = names(&folder);

    let done = export(&s, &[photo]);
    assert_eq!(done.report.written, 1, "{:?}", done.report);
    assert!(!done.cancelled);
    assert_eq!(done.waiting, None);

    let after = names(&folder);
    assert_eq!(
        after,
        [before.clone(), vec!["a.xmp".to_string()]].concat(),
        "one new file, no temporary file left behind"
    );
    let bytes = std::fs::read(folder.join("a.xmp")).unwrap();
    let fields = read(&bytes).unwrap();
    assert_eq!(fields.rating, Some(4));
    assert_eq!(fields.label.as_deref(), Some("Green"));
    assert_eq!(fields.title.as_deref(), Some("Heron at dawn"));
    assert_eq!(fields.keywords, ["Heron"]);
    let xmp = Xmp::from_bytes(&bytes).unwrap();
    assert!(xmp.get(ns::AUR, "Export").is_some(), "the marker");
    assert!(xmp.get(ns::AUR, "PhotoId").is_none(), "no identifier");
}

#[test]
fn an_export_is_never_reported_as_an_external_change_and_a_second_one_writes_nothing() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 2);
    let source = add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    set_rating(&s, photo, 3);
    assert_eq!(export(&s, &[photo]).report.written, 1);
    let file = folder.join("a.xmp");
    let (bytes, at) = (std::fs::read(&file).unwrap(), mtime(&file));

    // The base is the file as written: a scan finds nothing to report and reads nothing again.
    let row = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(row.path, "a.xmp");
    assert_eq!(row.base.rating, Some(3));
    assert_eq!(row.stat.size, bytes.len() as u64);
    rescan(&s, source);
    assert_eq!(s.engine.external_pending(), 0, "no feedback loop");
    assert_eq!(meta(&s, photo).rating, Some(3), "the photo is as it was");

    let again = export(&s, &[photo]);
    assert_eq!(
        (again.report.written, again.report.up_to_date),
        (0, 1),
        "{:?}",
        again.report
    );
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(mtime(&file), at, "not even touched");
}

#[test]
fn a_file_another_application_wrote_is_merged_and_keeps_its_develop_settings() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 3);
    let original = foreign("lightroom.xmp");
    other_app_writes(&folder.join("a.xmp"), &original);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    assert_eq!(meta(&s, photo).rating, Some(4), "read at import");
    set_title(&s, photo, "A new title");

    let done = export(&s, &[photo]);
    assert_eq!(done.report.written, 1, "{:?}", done.report);
    let merged = Xmp::from_bytes(&std::fs::read(folder.join("a.xmp")).unwrap()).unwrap();
    let before = Xmp::from_bytes(&original).unwrap();
    let rest = |x: &Xmp| {
        Xmp {
            properties: x
                .properties
                .iter()
                .filter(|p| !owned_or_not(p))
                .cloned()
                .collect(),
            prefixes: x.prefixes.clone(),
        }
        .to_bytes()
    };
    assert_eq!(rest(&merged), rest(&before), "everything else as it was");
    assert!(
        merged
            .properties
            .iter()
            .any(|p| p.ns == "http://ns.adobe.com/camera-raw-settings/1.0/"),
        "the develop settings are there"
    );
    assert_eq!(
        read(&std::fs::read(folder.join("a.xmp")).unwrap())
            .unwrap()
            .title
            .as_deref(),
        Some("A new title")
    );
    // ... and it is now the base: a scan reports nothing.
    rescan(&s, photo_source(&s, photo));
    assert_eq!(s.engine.external_pending(), 0);
}

fn photo_source(s: &Setup, photo: PhotoId) -> SourceId {
    s.engine
        .read_catalogue()
        .unwrap()
        .photo(&photo)
        .unwrap()
        .unwrap()
        .source_id
        .unwrap()
}

#[test]
fn a_file_another_application_changed_is_held_back_reviewed_and_then_exported() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 4);
    other_app_writes(&folder.join("a.xmp"), &foreign("lightroom.xmp"));
    let source = add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    // The other application changes the rating in its file; nobody has told Auroraw (no scan yet).
    let changed = String::from_utf8(foreign("lightroom.xmp"))
        .unwrap()
        .replace(r#"xmp:Rating="4""#, r#"xmp:Rating="2""#);
    other_app_writes(&folder.join("a.xmp"), changed.as_bytes());
    set_title(&s, photo, "Mine");

    let held = export(&s, &[photo]);
    assert_eq!(
        (held.report.held_back, held.report.written),
        (1, 0),
        "{:?}",
        held.report
    );
    assert_eq!(held.waiting, Some(1), "the banner's number");
    assert_eq!(
        std::fs::read(folder.join("a.xmp")).unwrap(),
        changed.as_bytes(),
        "the file is untouched"
    );
    let waiting = s.engine.external_changes().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].photo_id, photo);

    // Asking again does not answer it either.
    assert_eq!(export(&s, &[photo]).report.held_back, 1);
    assert_eq!(
        std::fs::read(folder.join("a.xmp")).unwrap(),
        changed.as_bytes()
    );

    // The photographer accepts; the photo has the file's rating; the next export writes.
    run(
        &s,
        Command::AcceptExternalChanges {
            photos: vec![photo],
            use_file: Vec::new(),
        },
    );
    assert_eq!(meta(&s, photo).rating, Some(2));
    let written = export(&s, &[photo]);
    assert_eq!(written.report.written, 1, "{:?}", written.report);
    let fields = read(&std::fs::read(folder.join("a.xmp")).unwrap()).unwrap();
    assert_eq!(
        (fields.rating, fields.title.as_deref()),
        (Some(2), Some("Mine"))
    );
    rescan(&s, source);
    assert_eq!(s.engine.external_pending(), 0);
}

#[test]
fn a_field_changed_on_both_sides_is_held_back_as_a_conflict() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 5);
    other_app_writes(&folder.join("a.xmp"), &foreign("lightroom.xmp"));
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    set_rating(&s, photo, 5);
    let changed = String::from_utf8(foreign("lightroom.xmp"))
        .unwrap()
        .replace(r#"xmp:Rating="4""#, r#"xmp:Rating="1""#);
    other_app_writes(&folder.join("a.xmp"), changed.as_bytes());

    let held = export(&s, &[photo]);
    assert_eq!(held.report.held_back, 1);
    let waiting = s.engine.external_changes().unwrap();
    assert!(waiting[0].changes.iter().any(|c| c.conflict));
    assert_eq!(
        meta(&s, photo).rating,
        Some(5),
        "Auroraw's value stands until answered"
    );
}

#[test]
fn a_file_that_appeared_after_the_last_scan_has_no_base_and_is_offered_not_overwritten() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 6);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    // Another application writes its file now; Auroraw has never seen it.
    other_app_writes(&folder.join("a.xmp"), &foreign("lightroom.xmp"));

    let held = export(&s, &[photo]);
    assert_eq!(held.report.held_back, 1, "{:?}", held.report);
    assert_eq!(
        std::fs::read(folder.join("a.xmp")).unwrap(),
        foreign("lightroom.xmp")
    );
    let waiting = s.engine.external_changes().unwrap();
    assert_eq!(waiting.len(), 1);
    assert!(
        waiting[0].changes.iter().all(|c| !c.conflict),
        "what the file has and the photo lacks is the file's to give, not a conflict"
    );
    run(
        &s,
        Command::AcceptExternalChanges {
            photos: vec![photo],
            use_file: Vec::new(),
        },
    );
    assert_eq!(meta(&s, photo).rating, Some(4));
    assert_eq!(export(&s, &[photo]).report.written, 1);
}

#[test]
fn replace_keeps_the_old_file_under_removed_and_writes_a_new_one() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 7);
    let original = foreign("darktable.xmp");
    other_app_writes(&folder.join("a.xmp"), &original);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    let done = export_with(
        &s,
        XmpScope::Photos(vec![photo]),
        XmpExportOptions {
            existing: XmpExisting::Replace,
            ..XmpExportOptions::default()
        },
    );
    assert_eq!(done.report.written, 1, "{:?}", done.report);
    let now = Xmp::from_bytes(&std::fs::read(folder.join("a.xmp")).unwrap()).unwrap();
    assert!(
        now.properties
            .iter()
            .all(|p| p.ns != "http://darktable.sf.net/"),
        "a new file, not a merge"
    );
    let kept = listing(&s.dir.path().join("Main").join("removed"));
    let old: Vec<_> = kept
        .iter()
        .filter(|(name, _)| name.contains("external-xmp") && name.ends_with(".xmp"))
        .collect();
    assert_eq!(old.len(), 1, "{kept:?}");
    assert_eq!(old[0].1, original, "the old file, byte for byte");
}

#[test]
fn skip_existing_writes_only_the_missing_files() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 8);
    jpeg(&folder.join("b.jpg"), 9);
    let original = foreign("lightroom.xmp");
    other_app_writes(&folder.join("a.xmp"), &original);
    add(&s, &folder);
    let (a, b) = (photo_named(&s, "a.jpg"), photo_named(&s, "b.jpg"));
    let done = export_with(
        &s,
        XmpScope::Photos(vec![a, b]),
        XmpExportOptions {
            existing: XmpExisting::SkipExisting,
            ..XmpExportOptions::default()
        },
    );
    assert_eq!(
        (done.report.written, done.report.skipped_existing),
        (1, 1),
        "{:?}",
        done.report
    );
    assert_eq!(std::fs::read(folder.join("a.xmp")).unwrap(), original);
    assert!(folder.join("b.xmp").exists());
}

#[test]
fn a_file_that_is_not_xmp_is_never_touched_unless_replace_is_chosen() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 10);
    other_app_writes(&folder.join("a.xmp"), b"this is not xmp <at all");
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    let done = export(&s, &[photo]);
    assert_eq!(
        (done.report.unreadable, done.report.written),
        (1, 0),
        "{:?}",
        done.report
    );
    assert_eq!(
        std::fs::read(folder.join("a.xmp")).unwrap(),
        b"this is not xmp <at all"
    );
    let replaced = export_with(
        &s,
        XmpScope::Photos(vec![photo]),
        XmpExportOptions {
            existing: XmpExisting::Replace,
            ..XmpExportOptions::default()
        },
    );
    assert_eq!(replaced.report.written, 1, "{:?}", replaced.report);
    assert!(read(&std::fs::read(folder.join("a.xmp")).unwrap()).is_ok());
}

#[test]
fn photos_sharing_a_stem_get_full_names_and_the_report_says_so() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    // Two originals of one folder with one stem (neither is a JPEG, so they are not one pair).
    png(&folder.join("A.png"), 12);
    jpeg(&folder.join("A.tif"), 11);
    add(&s, &folder);
    let done = export_with(
        &s,
        XmpScope::Photos(vec![photo_named(&s, "A.png"), photo_named(&s, "A.tif")]),
        XmpExportOptions::default(),
    );
    assert_eq!(
        (done.report.written, done.report.name_shared),
        (2, 2),
        "{:?}",
        done.report
    );
    assert!(folder.join("A.png.xmp").exists() && folder.join("A.tif.xmp").exists());
    assert!(!folder.join("A.xmp").exists());
    let full = export_with(
        &s,
        XmpScope::Photos(vec![photo_named(&s, "A.png")]),
        XmpExportOptions {
            naming: XmpNaming::FullName,
            ..XmpExportOptions::default()
        },
    );
    assert_eq!(full.report.up_to_date, 1, "an existing file keeps its name");
}

#[test]
fn a_photo_and_its_jpeg_are_one_photo_with_one_file() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    png(&folder.join("B.png"), 13);
    jpeg(&folder.join("B.jpg"), 14);
    add(&s, &folder);
    let count = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 100)
        .unwrap()
        .len();
    assert_eq!(count, 1, "a pair");
    let done = export_with(
        &s,
        XmpScope::Source(photo_source(&s, photo_named(&s, "B.png"))),
        XmpExportOptions::default(),
    );
    assert_eq!(
        (done.report.written, done.report.name_shared),
        (1, 0),
        "{:?}",
        done.report
    );
    assert!(folder.join("B.xmp").exists(), "named for the pair");
}

#[test]
fn a_keyword_marked_do_not_export_stays_out_of_the_file_and_is_no_change_afterwards() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 13);
    let source = add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    let places = create_keyword(&s, "Places", None);
    let secret = create_keyword(&s, "Secret spot", Some(places));
    let cabin = create_keyword(&s, "Cabin", Some(secret));
    let quebec = create_keyword(&s, "Quebec", Some(places));
    for keyword in [cabin, quebec] {
        run(
            &s,
            Command::AddKeyword {
                photo_id: photo,
                keyword_id: keyword,
            },
        );
    }
    run(
        &s,
        Command::SetKeywordProperties {
            keyword_id: secret,
            synonyms: Vec::new(),
            export: false,
        },
    );
    assert_eq!(export(&s, &[photo]).report.written, 1);
    let fields = read(&std::fs::read(folder.join("a.xmp")).unwrap()).unwrap();
    assert_eq!(
        fields.keywords,
        ["Places|Quebec"],
        "nothing below a hidden keyword"
    );
    assert_eq!(meta(&s, photo).keyword_ids.len(), 2, "the photo keeps them");

    // Auroraw's side has the hidden keyword, the file does not, and that is no difference.
    rescan(&s, source);
    assert_eq!(s.engine.external_pending(), 0);
    assert_eq!(export(&s, &[photo]).report.up_to_date, 1);
}

#[test]
fn a_rejected_photo_is_written_as_minus_one_or_with_its_stars_and_comes_back_whole() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 14);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    set_rating(&s, photo, 3);
    run(
        &s,
        Command::SetFlag {
            photo_id: photo,
            flag: Some(Flag::Rejected),
        },
    );
    export(&s, &[photo]);
    let text = std::fs::read_to_string(folder.join("a.xmp")).unwrap();
    assert!(text.contains(r#"<xmp:Rating>-1</xmp:Rating>"#), "{text}");
    assert!(text.contains("<aur:Stars>3</aur:Stars>"), "{text}");

    // Another workspace adds the folder: the photo arrives rejected, with its three stars.
    let other = setup();
    add(&other, &folder);
    let arrived = meta(&other, photo_named(&other, "a.jpg"));
    assert_eq!(
        (arrived.flag, arrived.rating),
        (Some(Flag::Rejected), Some(3))
    );

    // Without the -1 option the stars stay in xmp:Rating; the round trip is as exact.
    let again = export_with(
        &s,
        XmpScope::Photos(vec![photo]),
        XmpExportOptions {
            rejected_as_minus_one: false,
            ..XmpExportOptions::default()
        },
    );
    // (The file says something else now than the base: what Auroraw wrote itself is not a change.)
    assert_eq!(again.report.written, 1, "{:?}", again.report);
    let text = std::fs::read_to_string(folder.join("a.xmp")).unwrap();
    assert!(text.contains(r#"<xmp:Rating>3</xmp:Rating>"#), "{text}");
    assert!(!text.contains("aur:Stars"), "{text}");
}

#[test]
fn a_source_that_cannot_be_reached_is_counted_and_nothing_is_written() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 15);
    let source = add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    let moved = s.dir.path().join("Elsewhere");
    std::fs::rename(&folder, &moved).unwrap();
    let done = export_with(&s, XmpScope::Source(source), XmpExportOptions::default());
    assert_eq!(
        (done.report.unreachable, done.report.written),
        (1, 0),
        "{:?}",
        done.report
    );
    assert!(!moved.join("a.xmp").exists());
    let _ = photo;
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_written_fails_that_photo_and_the_batch_goes_on() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a/one.jpg"), 16);
    jpeg(&folder.join("b/two.jpg"), 17);
    add(&s, &folder);
    let locked = folder.join("a");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(locked.join("probe"), b"x").is_ok() {
        // (Running as root: a mode does not stop it, and there is nothing to test.)
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let done = export_with(
        &s,
        XmpScope::Source(photo_source(&s, photo_named(&s, "one.jpg"))),
        XmpExportOptions::default(),
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        (done.report.failed, done.report.written),
        (1, 1),
        "{:?}",
        done.report
    );
    assert!(done.report.first_error.is_some());
    assert!(folder.join("b/two.xmp").exists());
}

#[test]
fn a_whole_source_is_exported_as_one_job_with_progress() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    for i in 0..250u32 {
        jpeg(&folder.join(format!("d{}/img_{i:04}.jpg", i % 5)), i as u8);
    }
    let source = add(&s, &folder);
    let done = export_with(&s, XmpScope::Source(source), XmpExportOptions::default());
    assert_eq!(done.report.written, 250, "{:?}", done.report);
    let xmp_files = listing(&folder)
        .into_iter()
        .filter(|(n, _)| n.ends_with(".xmp"))
        .count();
    assert_eq!(xmp_files, 250);
    rescan(&s, source);
    assert_eq!(
        s.engine.external_pending(),
        0,
        "and none of them is a change"
    );
    let again = export_with(&s, XmpScope::Source(source), XmpExportOptions::default());
    assert_eq!(again.report.up_to_date, 250, "{:?}", again.report);
}

#[test]
fn a_cancelled_export_keeps_what_it_wrote_and_records_a_base_for_each() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    for i in 0..120u32 {
        jpeg(&folder.join(format!("img_{i:04}.jpg")), i as u8);
    }
    let source = add(&s, &folder);
    let Outcome::XmpExportStarted { job, .. } = s
        .engine
        .submit_and_wait(Command::ExportXmp {
            scope: XmpScope::Source(source),
            options: XmpExportOptions::default(),
        })
        .unwrap()
    else {
        panic!("expected XmpExportStarted");
    };
    run(&s, Command::CancelJob { job_id: job });
    let done = wait_export(&s, job);
    let files = listing(&folder)
        .into_iter()
        .filter(|(n, _)| n.ends_with(".xmp"))
        .count();
    // Whichever way the race went, what is on disk is what the report says, and every file is a base.
    assert_eq!(files, done.report.written, "{:?}", done.report);
    assert!(done.cancelled || done.report.written == 120);
    let catalogue = s.engine.read_catalogue().unwrap();
    let with_base = catalogue.external_stats(&source).unwrap().len();
    assert_eq!(with_base, done.report.written);
}

#[test]
fn a_file_another_application_wrote_is_kept_once_before_it_is_first_rewritten() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 3);
    let original = foreign("darktable.xmp");
    other_app_writes(&folder.join("a.xmp"), &original);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    set_title(&s, photo, "First");
    assert_eq!(export(&s, &[photo]).report.written, 1);
    let kept = kept_copies(&s);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        kept[0].1, original,
        "the other application's file, byte for byte"
    );
    // The file is now one Auroraw wrote (it carries the marker): later changes cost no copy.
    set_title(&s, photo, "Second");
    assert_eq!(export(&s, &[photo]).report.written, 1);
    assert_eq!(kept_copies(&s).len(), 1, "still the one copy");
}

#[test]
fn a_merge_that_changes_nothing_of_ours_leaves_another_applications_file_as_it_is() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 4);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    assert_eq!(export(&s, &[photo]).report.written, 1);
    let file = folder.join("a.xmp");
    // Another application rewrites it in its own layout (same content, padded), and Auroraw has seen it.
    let ours = std::fs::read_to_string(&file).unwrap();
    let theirs =
        format!("<?xpacket begin='' id='W5M0MpCehiHzreSzNTczkc9d'?>\n{ours}\n<?xpacket end='w'?>");
    other_app_writes(&file, theirs.as_bytes());
    rescan(&s, photo_source(&s, photo));
    assert_eq!(s.engine.external_pending(), 0);
    let done = export(&s, &[photo]);
    assert_eq!(
        (done.report.written, done.report.up_to_date),
        (0, 1),
        "{:?}",
        done.report
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        theirs,
        "not rewritten in our layout"
    );
}

#[test]
fn the_metadata_date_is_said_when_a_file_is_made_and_when_it_changes() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 5);
    add(&s, &folder);
    let photo = photo_named(&s, "a.jpg");
    assert_eq!(export(&s, &[photo]).report.written, 1);
    assert!(
        has_date(&folder.join("a.xmp")),
        "a new file says when its metadata was made"
    );
    set_title(&s, photo, "Changed");
    assert_eq!(export(&s, &[photo]).report.written, 1);
    assert!(has_date(&folder.join("a.xmp")));
    let at = mtime(&folder.join("a.xmp"));
    let again = export(&s, &[photo]);
    assert_eq!((again.report.written, again.report.up_to_date), (0, 1));
    assert_eq!(
        mtime(&folder.join("a.xmp")),
        at,
        "nothing changed, nothing touched"
    );
}

#[test]
fn a_folder_that_cannot_be_listed_fails_its_photos_and_writes_nothing_there() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("sub/a.jpg"), 6);
    jpeg(&folder.join("b.jpg"), 7); // (Not the same picture as a.jpg: two files with the same bytes are one photo with two locations.)
    other_app_writes(&folder.join("sub/a.xmp"), &foreign("lightroom.xmp"));
    add(&s, &folder);
    let (a, b) = (photo_named(&s, "a.jpg"), photo_named(&s, "b.jpg"));
    // The folder goes away between the scan and the export (a mount that drops, a permission): it is not
    // a folder with no files in it.
    let lightroom = std::fs::read(folder.join("sub/a.xmp")).unwrap();
    std::fs::remove_dir_all(folder.join("sub")).unwrap();
    let done = export(&s, &[a, b]);
    assert_eq!(done.report.failed, 1, "{:?}", done.report);
    assert_eq!(
        done.report.written, 1,
        "the other folder is fine: {:?}",
        done.report
    );
    assert!(
        done.report
            .first_error
            .as_deref()
            .is_some_and(|e| e.contains("cannot be listed")),
        "{:?}",
        done.report
    );
    assert!(
        !folder.join("sub").exists(),
        "the folder was not made again"
    );
    assert!(!lightroom.is_empty());
}
