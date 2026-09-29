// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP files other applications keep next to originals (spec §5.7, D-047; design note 003 §8),
//! through the real production path (`add_source` / `IndexSource`, `index_job`): a NEW photo takes what
//! the file says when it is first seen (rating, label, title, IPTC fields, keywords by identifier,
//! matched to the vocabulary whatever the case and created where missing); a photo the catalogue
//! already knew is baselined silently, its sidecar untouched; a file that cannot be read never stops a
//! photo from being added; and nothing in the source is ever written (D-018).

use std::path::Path;
use std::time::{Duration, Instant};

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
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::ExternalChanges {
                source_id: Some(id),
                unreadable,
                ..
            }) if id == source => return unreadable,
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
