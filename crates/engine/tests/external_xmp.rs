// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP files other applications keep next to originals (spec §5.7, D-047; design note 003 §8),
//! through the real production path (`add_source` / `IndexSource`, `index_job`): a NEW photo takes what
//! the file says when it is first seen (rating, label, title, IPTC fields, keywords by identifier,
//! matched to the vocabulary whatever the case and created where missing); a photo the catalogue
//! already knew is baselined silently, its sidecar untouched; a file that cannot be read never stops a
//! photo from being added; and nothing in the source is ever written (D-018).

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use auroraw_engine::{AddSourceRequest, Command, Engine, Event, EventReceiver, JobId, Outcome};
use auroraw_format::sidecar::{Flag, Metadata};
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

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../format/tests/fixtures/xmp/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Waits for the `ExternalChanges` event of `job`'s source (it follows `IndexFinished` and every
/// external file of the scan the coordinator was sent) and returns how many files were unreadable.
fn scanned(events: &EventReceiver, source: SourceId) -> usize {
    scanned_with_count(events, source).1
}

/// The same, with how many photos have a change waiting for an answer: `(waiting, unreadable)`.
fn scanned_with_count(events: &EventReceiver, source: SourceId) -> (usize, usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::ExternalChanges {
                source_id: Some(id),
                photos,
                unreadable,
            }) if id == source => return (photos, unreadable),
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
}

/// Adds `root` as a source and waits for its scan to be over.
fn add(s: &Setup, root: &Path) -> (SourceId, usize) {
    let added = s
        .engine
        .add_source(AddSourceRequest {
            root: root.to_path_buf(),
            name: None,
            merge: false,
        })
        .unwrap();
    let unreadable = scanned(&s.events, added.source_id);
    (added.source_id, unreadable)
}

/// Scans an already-added source again; how many photos have a change waiting afterwards.
fn rescan_waiting(s: &Setup, source: SourceId) -> usize {
    let Outcome::IndexStarted { job: _ } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected IndexStarted");
    };
    scanned_with_count(&s.events, source).0
}

/// Scans an already-added source again.
fn rescan(s: &Setup, source: SourceId) -> usize {
    let Outcome::IndexStarted { job: _ } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected IndexStarted");
    };
    scanned(&s.events, source)
}

fn only_photo(s: &Setup) -> PhotoId {
    let rows = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10)
        .unwrap();
    assert_eq!(rows.len(), 1, "one photo");
    rows[0].id
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

fn vocabulary_paths(s: &Setup) -> Vec<String> {
    let mut paths: Vec<String> = s
        .engine
        .read_catalogue()
        .unwrap()
        .keywords_with_counts()
        .unwrap()
        .into_iter()
        .map(|k| k.path)
        .collect();
    paths.sort();
    paths
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

#[test]
fn a_new_photo_takes_the_fields_of_the_xmp_beside_it_and_its_keywords_go_by_identifier() {
    let s = setup();
    // The vocabulary already has "Fauna|Birds|heron", in another case than the file spells it.
    let fauna = create_keyword(&s, "Fauna", None);
    let birds = create_keyword(&s, "Birds", Some(fauna));
    let heron = create_keyword(&s, "heron", Some(birds));
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 1);
    std::fs::write(folder.join("a.xmp"), fixture("lightroom.xmp")).unwrap();
    let before = listing(&folder);

    let (source, unreadable) = add(&s, &folder);
    assert_eq!(unreadable, 0);
    let photo = only_photo(&s);
    let m = meta(&s, photo);
    assert_eq!(m.rating, Some(4));
    assert_eq!(m.label.as_deref(), Some("Green"));
    assert_eq!(m.title.as_deref(), Some("Heron at dawn"));
    assert_eq!(m.city.as_deref(), Some("Roberval"));
    assert!(
        m.keyword_ids.contains(&heron),
        "the keyword that was there, whatever its case"
    );
    assert_eq!(m.keyword_ids.len(), 2);
    assert_eq!(
        vocabulary_paths(&s),
        [
            "Fauna",
            "Fauna|Birds",
            "Fauna|Birds|heron",
            "Places",
            "Places|Canada",
            "Places|Canada|Quebec"
        ],
        "no second Heron; the missing path was created, parents first"
    );
    assert_eq!(m.keyword_paths.len(), 2);
    assert!(
        m.keyword_paths.contains(&"Fauna|Birds|heron".to_string()),
        "the vocabulary's spelling"
    );

    // The catalogue agrees with the sidecar, and the base is what the file said.
    let row = s
        .engine
        .read_catalogue()
        .unwrap()
        .photo(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(row.rating, 4);
    let base = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(base.path, "a.xmp");
    assert_eq!(base.base.rating, Some(4));
    assert_eq!(
        base.base.keywords,
        ["Fauna|Birds|Heron", "Places|Canada|Quebec"]
    );
    assert_eq!(
        base.stat.size,
        before.iter().find(|(n, _)| n == "a.xmp").unwrap().1.len() as u64
    );
    let _ = source;
    assert_eq!(listing(&folder), before, "the source is untouched (D-018)");
}

#[test]
fn a_photo_another_application_rejected_arrives_rejected() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 2);
    std::fs::write(folder.join("a.xmp"), fixture("darktable.xmp")).unwrap();
    add(&s, &folder);
    let m = meta(&s, only_photo(&s));
    assert_eq!((m.flag, m.rating), (Some(Flag::Rejected), None));
    assert_eq!(m.keyword_paths, ["Fauna|Birds|Heron"]);
}

#[test]
fn a_photo_the_catalogue_already_knew_gets_a_silent_base_and_its_sidecar_is_not_touched() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 3);
    let (source, _) = add(&s, &folder);
    let photo = only_photo(&s);
    let sidecar = std::fs::read(s.engine.workspace().photo_path(&photo)).unwrap();
    assert_eq!(
        s.engine
            .read_catalogue()
            .unwrap()
            .external_of(&photo)
            .unwrap(),
        None
    );

    // Another application writes its XMP later.
    std::fs::write(folder.join("a.xmp"), fixture("lightroom.xmp")).unwrap();
    assert_eq!(rescan(&s, source), 0);

    let m = meta(&s, photo);
    assert_eq!((m.rating, m.title), (None, None), "nothing applied");
    assert_eq!(
        std::fs::read(s.engine.workspace().photo_path(&photo)).unwrap(),
        sidecar,
        "the sidecar is not even rewritten"
    );
    let base = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(
        base.base.rating,
        Some(4),
        "what it says is now the base, for later"
    );
    assert!(
        vocabulary_paths(&s).is_empty(),
        "and no keyword was created for it"
    );

    // Scanning again reads nothing again.
    let again = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(rescan(&s, source), 0);
    assert_eq!(
        s.engine
            .read_catalogue()
            .unwrap()
            .external_of(&photo)
            .unwrap()
            .unwrap(),
        again
    );
}

#[test]
fn a_file_that_is_not_xmp_is_counted_and_never_stops_the_photo_from_being_added() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 4);
    jpeg(&folder.join("b.jpg"), 5);
    std::fs::write(folder.join("a.xmp"), b"this is not xml at all").unwrap();
    std::fs::write(folder.join("b.xmp"), fixture("flat-subject.xmp")).unwrap();
    let (_, unreadable) = add(&s, &folder);
    assert_eq!(unreadable, 1);
    let catalogue = s.engine.read_catalogue().unwrap();
    assert_eq!(catalogue.count_all().unwrap(), 2, "both photos are there");
    let with_rows = catalogue
        .list_recent(None, 10)
        .unwrap()
        .into_iter()
        .filter(|r| catalogue.external_of(&r.id).unwrap().is_some())
        .count();
    assert_eq!(with_rows, 1, "only the readable file is tracked");
}

#[test]
fn a_raw_and_its_jpeg_are_one_photo_that_owns_the_xmp_of_their_stem() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("IMG_1.CR2"), b"raw bytes, not a real raw").unwrap();
    jpeg(&folder.join("IMG_1.JPG"), 6);
    std::fs::write(folder.join("IMG_1.xmp"), fixture("flat-subject.xmp")).unwrap();
    add(&s, &folder);
    let photo = only_photo(&s);
    assert_eq!(
        s.engine
            .read_catalogue()
            .unwrap()
            .external_of(&photo)
            .unwrap()
            .unwrap()
            .path,
        "IMG_1.xmp"
    );
    assert_eq!(meta(&s, photo).rating, Some(2));
}

#[test]
fn a_photo_that_is_restored_keeps_its_own_sidecar_and_the_file_is_only_baselined() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 7);
    std::fs::write(folder.join("a.xmp"), fixture("flat-subject.xmp")).unwrap();
    let (source, _) = add(&s, &folder);
    let photo = only_photo(&s);
    s.engine
        .submit_and_wait(Command::SetRating {
            photo_id: photo,
            rating: 5,
        })
        .unwrap();
    let Outcome::RemoveStarted { job } = s
        .engine
        .submit_and_wait(Command::RemoveSource { source_id: source })
        .unwrap()
    else {
        panic!("expected RemoveStarted");
    };
    wait_job(&s.events, job);

    // Another application changes the file while the source is out of the catalogue.
    std::fs::write(folder.join("a.xmp"), fixture("lightroom.xmp")).unwrap();
    let added = s
        .engine
        .add_source(AddSourceRequest {
            root: folder.clone(),
            name: None,
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match s.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexPlanned { .. }) => break,
            _ => assert!(Instant::now() < deadline, "no plan"),
        }
    }
    s.engine
        .submit_and_wait(Command::ContinueIndex {
            job_id: added.job,
            restore: true,
        })
        .unwrap();
    scanned(&s.events, added.source_id);

    let m = meta(&s, photo);
    assert_eq!(m.rating, Some(5), "its own work, not the file's 4");
    let base = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(base.base.rating, Some(4), "the file, as it says now");
}

fn wait_job(events: &EventReceiver, job: JobId) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::JobFinished(j)) if j == job => return,
            _ => assert!(Instant::now() < deadline, "the job never finished"),
        }
    }
}

// ---- slice 2: later changes are noticed, compared three ways, and applied only on an answer ----

static CLOCK: AtomicU64 = AtomicU64::new(1_700_000_000);

/// Writes an XMP file the way another application would, and gives it a modification time of its own
/// (never the same as the last one written), so that nothing depends on the file system's granularity.
fn write_xmp(path: &Path, rating: Option<i8>, title: &str, keywords: &[&str], develop: &str) {
    let rating = rating.map_or(String::new(), |r| format!(r#" xmp:Rating="{r}""#));
    let subjects: String = keywords
        .iter()
        .map(|k| format!("<rdf:li>{k}</rdf:li>"))
        .collect();
    let text = format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:lr="http://ns.adobe.com/lightroom/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:Exposure2012="{develop}"{rating}>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
   <lr:hierarchicalSubject><rdf:Bag>{subjects}</rdf:Bag></lr:hierarchicalSubject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#
    );
    std::fs::write(path, text).unwrap();
    let at = UNIX_EPOCH + Duration::from_secs(CLOCK.fetch_add(10, Ordering::SeqCst));
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

fn accept(
    s: &Setup,
    photos: &[PhotoId],
    use_file: &[(PhotoId, auroraw_engine::ExternalField)],
) -> usize {
    let Outcome::ExternalResolved { photos } = s
        .engine
        .submit_and_wait(Command::AcceptExternalChanges {
            photos: photos.to_vec(),
            use_file: use_file.to_vec(),
        })
        .unwrap()
    else {
        panic!("expected ExternalResolved");
    };
    photos
}

fn undo_kind(s: &Setup) -> Option<(auroraw_engine::LabelKind, usize)> {
    s.events
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            Event::HistoryChanged(state) => Some(state.undo),
            _ => None,
        })
        .next_back()
        .flatten()
        .map(|l| (l.kind, l.count))
}

/// A source with one photo, `a.jpg`, whose `a.xmp` says `rating`, `title` and `keywords` when it is added.
fn one_tracked(
    s: &Setup,
    rating: i8,
    title: &str,
    keywords: &[&str],
) -> (SourceId, PhotoId, std::path::PathBuf) {
    let folder = s.dir.path().join("Card");
    jpeg(&folder.join("a.jpg"), 9);
    write_xmp(
        &folder.join("a.xmp"),
        Some(rating),
        title,
        keywords,
        "+0.10",
    );
    let (source, _) = add(s, &folder);
    (source, only_photo(s), folder)
}

#[test]
fn a_later_change_is_noticed_shown_and_applied_only_on_acceptance_then_never_reported_again() {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 4, "Heron at dawn", &["Fauna|Birds|Heron"]);
    let xmp = folder.join("a.xmp");
    let sidecar_path = s.engine.workspace().photo_path(&photo);

    // Another application edits its file: a new title, five stars, one more keyword.
    write_xmp(
        &xmp,
        Some(5),
        "Heron at dusk",
        &["Fauna|Birds|Heron", "Sunrise"],
        "+0.10",
    );
    let bytes_of_the_file = std::fs::read(&xmp).unwrap();
    assert_eq!(rescan_waiting(&s, source), 1);
    assert_eq!(s.engine.external_pending(), 1);

    // Shown, nothing applied.
    let listed = s.engine.external_changes().unwrap();
    assert_eq!(listed.len(), 1);
    let one = &listed[0];
    assert_eq!((one.photo_id, one.xmp_path.as_str()), (photo, "a.xmp"));
    let fields: Vec<(&str, &str, &str, bool)> = one
        .changes
        .iter()
        .map(|c| (c.field.key(), c.mine.as_str(), c.file.as_str(), c.conflict))
        .collect();
    assert_eq!(
        fields,
        [
            ("rating", "4", "5", false),
            ("title", "Heron at dawn", "Heron at dusk", false)
        ]
    );
    assert_eq!(one.keywords_added, ["Sunrise"]);
    assert!(one.keywords_removed.is_empty());
    let before = meta(&s, photo);
    assert_eq!(
        (before.rating, before.title.as_deref()),
        (Some(4), Some("Heron at dawn"))
    );
    assert_eq!(
        std::fs::read(&xmp).unwrap(),
        bytes_of_the_file,
        "Auroraw never writes it (D-018)"
    );

    // Accepted: one step, named for what it is.
    s.events.drain();
    assert_eq!(accept(&s, &[photo], &[]), 1);
    let after = meta(&s, photo);
    assert_eq!(
        (after.rating, after.title.as_deref()),
        (Some(5), Some("Heron at dusk"))
    );
    assert!(after.keyword_paths.contains(&"Sunrise".to_string()));
    assert!(
        vocabulary_paths(&s).contains(&"Sunrise".to_string()),
        "the keyword was created"
    );
    assert_eq!(
        undo_kind(&s),
        Some((auroraw_engine::LabelKind::ExternalChanges, 1))
    );
    assert_eq!(s.engine.external_pending(), 0);

    // No feedback loop: scanning again reports nothing and does not rewrite the sidecar.
    let written = std::fs::read(&sidecar_path).unwrap();
    let modified = std::fs::metadata(&sidecar_path)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(rescan_waiting(&s, source), 0);
    assert_eq!(std::fs::read(&sidecar_path).unwrap(), written);
    assert_eq!(
        std::fs::metadata(&sidecar_path)
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );

    // One undo takes the whole acceptance back, including the keyword it created; redo does it again.
    s.engine.undo().unwrap();
    let undone = meta(&s, photo);
    assert_eq!(
        (undone.rating, undone.title.as_deref()),
        (Some(4), Some("Heron at dawn"))
    );
    assert!(
        !vocabulary_paths(&s).contains(&"Sunrise".to_string()),
        "and the keyword it made"
    );
    assert_eq!(
        rescan_waiting(&s, source),
        0,
        "undoing an acceptance does not re-report the file"
    );
    s.engine.redo().unwrap();
    assert_eq!(meta(&s, photo).title.as_deref(), Some("Heron at dusk"));
    assert!(vocabulary_paths(&s).contains(&"Sunrise".to_string()));
}

#[test]
fn an_ignored_change_is_not_reported_again_until_the_file_changes_again() {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 3, "One", &[]);
    let xmp = folder.join("a.xmp");
    write_xmp(&xmp, Some(3), "Two", &[], "+0.10");
    assert_eq!(rescan_waiting(&s, source), 1);
    let unchanged = std::fs::read(s.engine.workspace().photo_path(&photo)).unwrap();

    let Outcome::ExternalResolved { photos } = s
        .engine
        .submit_and_wait(Command::IgnoreExternalChanges {
            photos: vec![photo],
        })
        .unwrap()
    else {
        panic!("expected ExternalResolved");
    };
    assert_eq!(photos, 1);
    assert_eq!(s.engine.external_pending(), 0);
    assert_eq!(
        std::fs::read(s.engine.workspace().photo_path(&photo)).unwrap(),
        unchanged
    );
    assert_eq!(rescan_waiting(&s, source), 0);
    assert_eq!(rescan_waiting(&s, source), 0);

    // A later edit of the same file is a new change, against what was declined.
    write_xmp(&xmp, Some(3), "Three", &[], "+0.10");
    assert_eq!(rescan_waiting(&s, source), 1);
}

#[test]
fn a_file_that_changed_to_what_the_photo_already_says_needs_no_answer() {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 3, "Dawn", &[]);
    s.engine
        .submit_and_wait(Command::SetMetadataField {
            photo_id: photo,
            field: auroraw_engine::MetadataField::Title,
            value: "Dusk".into(),
        })
        .unwrap();
    write_xmp(&folder.join("a.xmp"), Some(3), "Dusk", &[], "+0.10");
    assert_eq!(
        rescan_waiting(&s, source),
        0,
        "converged: absorbed silently"
    );
    let row = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_eq!(row.base.title.as_deref(), Some("Dusk"));
    assert_eq!(row.pending, None);
}

#[test]
fn a_change_on_both_sides_is_a_conflict_that_keeps_the_photos_value_unless_told_otherwise() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    for (name, seed) in [("a", 1), ("b", 2)] {
        jpeg(&folder.join(format!("{name}.jpg")), seed);
        write_xmp(
            &folder.join(format!("{name}.xmp")),
            Some(2),
            "Original",
            &[],
            "+0.10",
        );
    }
    let (source, _) = add(&s, &folder);
    let rows = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10)
        .unwrap();
    let (a, b) = {
        let by_name = |n: &str| rows.iter().find(|r| r.filename == n).unwrap().id;
        (by_name("a.jpg"), by_name("b.jpg"))
    };
    for photo in [a, b] {
        s.engine
            .submit_and_wait(Command::SetMetadataField {
                photo_id: photo,
                field: auroraw_engine::MetadataField::Title,
                value: "Mine".into(),
            })
            .unwrap();
        s.engine
            .submit_and_wait(Command::SetRating {
                photo_id: photo,
                rating: 5,
            })
            .unwrap();
    }
    for name in ["a", "b"] {
        write_xmp(
            &folder.join(format!("{name}.xmp")),
            Some(4),
            "Theirs",
            &[],
            "+0.10",
        );
    }
    assert_eq!(rescan_waiting(&s, source), 2);
    let listed = s.engine.external_changes().unwrap();
    assert!(
        listed.iter().all(|p| p.changes.iter().all(|c| c.conflict)),
        "both fields changed on both sides"
    );

    // Accept both: `a`'s title goes the file's way, nothing else does.
    accept(&s, &[a, b], &[(a, auroraw_engine::ExternalField::Title)]);
    assert_eq!(meta(&s, a).title.as_deref(), Some("Theirs"));
    assert_eq!(meta(&s, a).rating, Some(5), "the photo's own rating stays");
    assert_eq!(meta(&s, b).title.as_deref(), Some("Mine"));
    assert_eq!(meta(&s, b).rating, Some(5));
    // The base advanced either way: a rescan asks nothing.
    assert_eq!(s.engine.external_pending(), 0);
    assert_eq!(rescan_waiting(&s, source), 0);
}

#[test]
fn keywords_are_merged_as_sets_and_a_lost_keyword_is_removed_from_the_photo() {
    let s = setup();
    let (source, photo, folder) =
        one_tracked(&s, 3, "T", &["Fauna|Birds|Heron", "Places|Canada|Quebec"]);
    // Auroraw adds a keyword of its own; the file drops Quebec and gains Sunrise.
    let own = create_keyword(&s, "Mine", None);
    s.engine
        .submit_and_wait(Command::AddKeyword {
            photo_id: photo,
            keyword_id: own,
        })
        .unwrap();
    write_xmp(
        &folder.join("a.xmp"),
        Some(3),
        "T",
        &["Fauna|Birds|Heron", "Sunrise"],
        "+0.10",
    );
    assert_eq!(rescan_waiting(&s, source), 1);
    let listed = s.engine.external_changes().unwrap();
    assert_eq!(listed[0].keywords_added, ["Sunrise"]);
    assert_eq!(listed[0].keywords_removed, ["Places|Canada|Quebec"]);

    accept(&s, &[photo], &[]);
    let paths = meta(&s, photo).keyword_paths;
    assert!(paths.contains(&"Sunrise".to_string()));
    assert!(
        paths.contains(&"Mine".to_string()),
        "a keyword only Auroraw gave is never taken away"
    );
    assert!(paths.contains(&"Fauna|Birds|Heron".to_string()));
    assert!(!paths.contains(&"Places|Canada|Quebec".to_string()));
}

#[test]
fn a_file_that_only_changed_in_ways_that_are_not_fields_moves_its_stat_and_reports_nothing() {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 3, "T", &[]);
    let before = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    write_xmp(&folder.join("a.xmp"), Some(3), "T", &[], "+1.75");
    assert_eq!(
        rescan_waiting(&s, source),
        0,
        "develop settings are not fields"
    );
    let after = s
        .engine
        .read_catalogue()
        .unwrap()
        .external_of(&photo)
        .unwrap()
        .unwrap();
    assert_ne!(
        after.stat, before.stat,
        "but it was read, and its stat is now the base's"
    );
    assert_eq!(after.base, before.base);
}

#[test]
fn after_a_rebuild_there_is_no_base_so_the_first_scan_baselines_silently_and_later_edits_are_reported()
 {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 3, "One", &[]);
    write_xmp(&folder.join("a.xmp"), Some(3), "Two", &[], "+0.10");
    s.engine.submit_and_wait(Command::Rebuild).unwrap();
    assert_eq!(
        s.engine
            .read_catalogue()
            .unwrap()
            .external_of(&photo)
            .unwrap(),
        None
    );
    assert_eq!(
        rescan_waiting(&s, source),
        0,
        "nothing to compare with: baselined"
    );
    write_xmp(&folder.join("a.xmp"), Some(3), "Three", &[], "+0.10");
    assert_eq!(rescan_waiting(&s, source), 1, "and from there on, watched");
}

#[test]
fn an_answer_for_a_photo_with_nothing_waiting_is_skipped() {
    let s = setup();
    let (_, photo, _) = one_tracked(&s, 3, "T", &[]);
    assert_eq!(accept(&s, &[photo, PhotoId::random()], &[]), 0);
    let Outcome::ExternalResolved { photos } = s
        .engine
        .submit_and_wait(Command::IgnoreExternalChanges {
            photos: vec![photo],
        })
        .unwrap()
    else {
        panic!("expected ExternalResolved");
    };
    assert_eq!(photos, 0);
}

#[test]
fn a_photo_the_file_rejects_becomes_rejected_and_keeps_its_stars() {
    let s = setup();
    let (source, photo, folder) = one_tracked(&s, 3, "T", &[]);
    write_xmp(&folder.join("a.xmp"), Some(-1), "T", &[], "+0.10");
    assert_eq!(rescan_waiting(&s, source), 1);
    accept(&s, &[photo], &[]);
    let m = meta(&s, photo);
    assert_eq!((m.flag, m.rating), (Some(Flag::Rejected), Some(3)));
    // And un-rejecting it (four stars) lifts the mark.
    write_xmp(&folder.join("a.xmp"), Some(4), "T", &[], "+0.10");
    assert_eq!(rescan_waiting(&s, source), 1);
    accept(&s, &[photo], &[]);
    let m = meta(&s, photo);
    assert_eq!((m.flag, m.rating), (None, Some(4)));
}

#[test]
fn hundreds_of_photos_are_accepted_as_one_step_and_undone_as_one() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    let n = 250;
    for i in 0..n {
        jpeg(&folder.join(format!("p{i:03}.jpg")), i as u8);
        write_xmp(
            &folder.join(format!("p{i:03}.xmp")),
            Some(1),
            "Old",
            &[],
            "+0.10",
        );
    }
    let (source, _) = add(&s, &folder);
    for i in 0..n {
        write_xmp(
            &folder.join(format!("p{i:03}.xmp")),
            Some(2),
            "New",
            &[],
            "+0.10",
        );
    }
    assert_eq!(rescan_waiting(&s, source), n);
    let ids: Vec<PhotoId> = s
        .engine
        .external_changes()
        .unwrap()
        .iter()
        .map(|p| p.photo_id)
        .collect();
    assert_eq!(ids.len(), n);
    s.events.drain();
    assert_eq!(accept(&s, &ids, &[]), n);
    assert_eq!(
        undo_kind(&s),
        Some((auroraw_engine::LabelKind::ExternalChanges, n))
    );
    assert!(
        ids.iter()
            .all(|id| meta(&s, *id).title.as_deref() == Some("New"))
    );
    s.engine.undo().unwrap();
    assert!(
        ids.iter()
            .all(|id| meta(&s, *id).title.as_deref() == Some("Old"))
    );
}
