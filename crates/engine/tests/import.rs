// SPDX-License-Identifier: GPL-3.0-or-later
//! Import as a copy that the catalogue may or may not care about (M1 plan workflow revision D-090,
//! D-093): a plain copy writes no sidecar and skips nothing the catalogue knows; a destination inside
//! a source (or added as one) also registers the photos; a card's own folders can be kept and merged;
//! the case of a file name is never changed; importing twice adds nothing.

use std::path::Path;
use std::time::{Duration, Instant};

use auroraw_engine::{
    AddSourceRequest, DestinationKind, Engine, EngineError, Event, EventReceiver, ImportRequest,
    JobId, MetadataTemplate, PairRule, Profile,
};
use auroraw_testkit::{TempDir, temp_dir};

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

fn profile(template: &str) -> Profile {
    Profile {
        name: "Test".into(),
        destination_template: template.into(),
        backup_templates: Vec::new(),
        pair_rule: PairRule::Both,
        metadata_template: MetadataTemplate::default(),
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn request(s: &Setup, card: &Path, destination: &Path, template: &str) -> ImportRequest {
    ImportRequest {
        source_root: card.to_path_buf(),
        destination_root: destination.to_path_buf(),
        profile: profile(template),
        backup_root: None,
        state_dir: s.dir.path().join("state"),
        add_destination_as_source: false,
    }
}

fn wait_finished(events: &EventReceiver, job: JobId) -> (usize, usize, usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::ImportFinished {
                job: j,
                copied,
                skipped,
                failed,
            }) if j == job => return (copied, skipped, failed),
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the import never finished"),
        }
    }
}

fn run(s: &Setup, request: ImportRequest) -> (usize, usize, usize) {
    let started = s.engine.import(request).unwrap();
    wait_finished(&s.events, started.job)
}

fn sidecar_count(s: &Setup) -> usize {
    s.engine.read_catalogue().unwrap().count_all().unwrap() as usize
}

#[test]
fn a_plain_copy_writes_no_sidecar_registers_nothing_and_is_not_stopped_by_what_the_catalogue_knows()
{
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("a.CR2"), b"photo a");
    write(&card.join("b.CR2"), b"photo b");
    write(&card.join("notes.txt"), b"not a photo");
    write(&card.join("a.xmp"), b"<x/>");

    let plain = s.dir.path().join("Plain");
    let started = s
        .engine
        .import(request(&s, &card, &plain, "{name}"))
        .unwrap();
    assert!(!started.registered && started.added_source.is_none());
    assert_eq!(wait_finished(&s.events, started.job), (2, 0, 0));
    assert_eq!(std::fs::read(plain.join("a.CR2")).unwrap(), b"photo a");
    assert!(!plain.join("notes.txt").exists(), "only photos are copied");
    assert!(!plain.join("a.xmp").exists());
    assert_eq!(sidecar_count(&s), 0, "nothing entered the catalogue");
    assert!(
        s.engine.sources().unwrap().is_empty(),
        "the card is no source either"
    );

    // The catalogue learns the same photos through a registered import; a plain copy of the same
    // card to yet another place still copies everything: that is what was asked.
    let archive = s.dir.path().join("Archive");
    let mut registered = request(&s, &card, &archive, "{name}");
    registered.add_destination_as_source = true;
    assert_eq!(run(&s, registered), (2, 0, 0));
    assert_eq!(sidecar_count(&s), 2);
    let elsewhere = s.dir.path().join("Elsewhere");
    assert_eq!(run(&s, request(&s, &card, &elsewhere, "{name}")), (2, 0, 0));
}

#[test]
fn importing_twice_into_the_same_folders_adds_nothing_and_makes_no_numbered_twins() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("DCIM/100CANON/IMG_0001.CR2"), b"one");
    write(&card.join("DCIM/100CANON/IMG_0002.CR2"), b"two");
    let plain = s.dir.path().join("Plain");

    assert_eq!(run(&s, request(&s, &card, &plain, "{path}")), (2, 0, 0));
    assert_eq!(run(&s, request(&s, &card, &plain, "{path}")), (0, 2, 0));
    let mut names: Vec<String> = std::fs::read_dir(plain.join("100CANON"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["IMG_0001.CR2", "IMG_0002.CR2"]);
}

#[test]
fn keeping_the_cards_folders_merges_into_what_is_there_and_keeps_every_name_as_it_was() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("DCIM/100CANON/IMG_0001.CR2"), b"new number one");
    write(&card.join("DCIM/100CANON/IMG_0002.CR2"), b"two");
    write(&card.join("DCIM/101CANON/IMG_0001.CR2"), b"another folder");
    write(&card.join("DCIM/101CANON/img_0003.jpg"), b"lower case");
    let archive = s.dir.path().join("Archive");
    // An earlier card already put a different IMG_0001.CR2 in 100CANON.
    write(&archive.join("100CANON/IMG_0001.CR2"), b"old number one");

    assert_eq!(run(&s, request(&s, &card, &archive, "{path}")), (4, 0, 0));
    let read = |name: &str| std::fs::read(archive.join(name)).unwrap();
    assert_eq!(
        read("100CANON/IMG_0001.CR2"),
        b"old number one",
        "never overwritten"
    );
    assert_eq!(
        read("100CANON/IMG_0001_2.CR2"),
        b"new number one",
        "numbered in place"
    );
    assert_eq!(read("100CANON/IMG_0002.CR2"), b"two");
    assert_eq!(read("101CANON/IMG_0001.CR2"), b"another folder");
    assert_eq!(
        read("101CANON/img_0003.jpg"),
        b"lower case",
        "the case is kept, extension included"
    );
}

#[test]
fn a_template_can_name_the_cards_folder_and_keeps_the_extension_case() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("DCIM/100CANON/IMG_0001.CR2"), b"one");
    let archive = s.dir.path().join("Archive");
    assert_eq!(
        run(
            &s,
            request(&s, &card, &archive, "{folder}/{original}.{ext}")
        ),
        (1, 0, 0)
    );
    assert!(archive.join("100CANON/IMG_0001.CR2").is_file());
}

#[test]
fn a_destination_inside_a_source_registers_the_photos_with_their_place_in_it() {
    let s = setup();
    let library = s.dir.path().join("Library");
    std::fs::create_dir_all(&library).unwrap();
    let added = s
        .engine
        .add_source(AddSourceRequest {
            root: library.clone(),
            name: Some("Library".into()),
            merge: false,
        })
        .unwrap();
    // Wait for the (empty) scan.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match s.events.recv_timeout(Duration::from_millis(100)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }

    let card = s.dir.path().join("Card");
    write(&card.join("a.CR2"), b"photo a");
    let destination = library.join("2026/September");
    let started = s
        .engine
        .import(request(&s, &card, &destination, "{name}"))
        .unwrap();
    assert!(started.registered);
    assert!(started.added_source.is_none(), "it was already covered");
    assert_eq!(wait_finished(&s.events, started.job), (1, 0, 0));

    let catalogue = s.engine.read_catalogue().unwrap();
    let row = catalogue.list_recent(None, 10).unwrap().remove(0);
    assert_eq!(row.source_id, Some(added.source_id));
    assert_eq!(row.path.as_deref(), Some("2026/September/a.CR2"));
    assert!(s.engine.workspace().read_photo(&row.id).unwrap().is_some());
    assert_eq!(
        s.engine.sources().unwrap().len(),
        1,
        "no second source was added"
    );
}

#[test]
fn a_destination_can_be_added_as_a_source_and_one_that_contains_sources_cannot() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("a.CR2"), b"photo a");

    let archive = s.dir.path().join("Archive");
    let mut req = request(&s, &card, &archive, "{name}");
    req.add_destination_as_source = true;
    assert!(matches!(
        s.engine.import_destination(&archive).unwrap(),
        DestinationKind::NotCovered
    ));
    let started = s.engine.import(req).unwrap();
    assert!(started.registered);
    let source = started
        .added_source
        .expect("the destination became a source");
    wait_finished(&s.events, started.job);
    assert_eq!(s.engine.sources().unwrap()[0].id, source);
    assert_eq!(s.engine.sources().unwrap()[0].photos, 1);

    // A folder that contains that source: a plain copy is fine, adding it as a source is not.
    let parent = s.dir.path();
    let mut into_parent = request(&s, &card, &parent.join("Above/Deeper"), "{name}");
    into_parent.destination_root = parent.to_path_buf();
    into_parent.add_destination_as_source = true;
    assert!(matches!(
        s.engine.import(into_parent),
        Err(EngineError::ContainsSources(_))
    ));
}

#[test]
fn an_import_that_cannot_make_sense_is_refused_and_creates_nothing() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("a.CR2"), b"photo a");
    let nowhere = s.dir.path().join("Nowhere");
    assert!(
        s.engine
            .import(request(&s, &nowhere, &s.dir.path().join("Out"), "{name}"))
            .is_err()
    );
    assert!(
        s.engine
            .import(request(&s, &card, &card.join("inside"), "{name}"))
            .is_err(),
        "a destination inside the card"
    );
    let mut same_backup = request(&s, &card, &s.dir.path().join("Out"), "{name}");
    same_backup.backup_root = Some(s.dir.path().join("Out"));
    assert!(
        s.engine.import(same_backup).is_err(),
        "the backup is the destination"
    );
    assert!(!s.dir.path().join("Out").exists());
    assert!(!card.join("inside").exists());
}

#[test]
fn a_card_with_a_dcim_folder_says_which_camera_folders_it_has() {
    let s = setup();
    let card = s.dir.path().join("Card");
    write(&card.join("dcim/101NIKON/D1.NEF"), b"x");
    write(&card.join("dcim/100NIKON/D0.NEF"), b"x");
    let info = Engine::inspect_import_source(&card);
    assert!(info.has_dcim);
    assert_eq!(info.camera_folders, ["100NIKON", "101NIKON"]);

    // The DCIM folder itself, whatever its case, is a camera card's layout too.
    let inside = Engine::inspect_import_source(&card.join("dcim"));
    assert!(inside.has_dcim);
    assert_eq!(inside.camera_folders, ["100NIKON", "101NIKON"]);

    let plain = s.dir.path().join("Plain");
    write(&plain.join("a.jpg"), b"x");
    assert!(!Engine::inspect_import_source(&plain).has_dcim);
    assert!(!Engine::inspect_import_source(&s.dir.path().join("Nowhere/DCIM")).has_dcim);
}
