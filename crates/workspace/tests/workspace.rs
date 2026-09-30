// SPDX-License-Identifier: GPL-3.0-or-later
//! The workspace on disk: creation, writing, locking, scanning, removal (testing strategy §3).

use std::fs;

use auroraw_format::sidecar::{Flag, PhotoSidecar, VersionSidecar};
use auroraw_format::state::{Collection, Loaded, Series, Sources, Vocabulary};
use auroraw_testkit::{rng, temp_dir};
use auroraw_types::{CollectionId, PhotoId, SeriesId, VersionId};
use auroraw_workspace::{Access, ForeignReason, Workspace, WorkspaceError, WriteOutcome};
use proptest::prelude::*;

fn new_workspace() -> (auroraw_testkit::TempDir, Workspace) {
    let dir = temp_dir();
    let ws = Workspace::create(&dir.path().join("Main"), "Main").unwrap();
    (dir, ws)
}

fn photo(rating: u8) -> PhotoSidecar {
    let mut p = PhotoSidecar::new(PhotoId::random());
    p.meta.rating = Some(rating);
    p
}

fn now() -> auroraw_types::Timestamp {
    "2026-09-21T14:02:11Z".parse().unwrap()
}

fn series_of(photo: &PhotoSidecar) -> Series {
    Series {
        id: SeriesId::random(),
        updated: now(),
        kind: "burst".into(),
        cover: photo.photo_id,
        resolved: false,
        kept: vec![],
        members: vec![photo.photo_id],
        extra: Default::default(),
    }
}

#[test]
fn a_new_workspace_has_its_marker_readme_and_folders() {
    let (_dir, ws) = new_workspace();
    assert_eq!(ws.access(), Access::ReadWrite);
    assert_eq!(ws.marker().catalogue_name, "Main");
    assert_eq!(ws.marker().layout, 1);
    for name in [
        "workspace.json",
        "README.txt",
        "photos",
        "versions",
        "state",
        ".auroraw/tmp",
        ".auroraw/lock",
    ] {
        assert!(ws.root().join(name).exists(), "{name}");
    }
    let marker = fs::read_to_string(ws.root().join("workspace.json")).unwrap();
    assert!(
        marker.contains("\"format\": \"auroraw/workspace\""),
        "{marker}"
    );
    assert!(Workspace::create(ws.root(), "again").is_err(), "not twice");
}

#[test]
fn a_folder_that_is_not_a_workspace_is_refused() {
    let dir = temp_dir();
    assert!(matches!(
        Workspace::open(dir.path()),
        Err(WorkspaceError::NotAWorkspace(_))
    ));
}

#[test]
fn a_newer_layout_or_marker_is_refused() {
    let (_dir, ws) = new_workspace();
    let root = ws.root().to_path_buf();
    let marker = fs::read_to_string(root.join("workspace.json")).unwrap();
    drop(ws);
    fs::write(
        root.join("workspace.json"),
        marker.replace("\"layout\": 1", "\"layout\": 2"),
    )
    .unwrap();
    assert!(matches!(
        Workspace::open(&root),
        Err(WorkspaceError::NewerLayout { found: 2, .. })
    ));
    fs::write(
        root.join("workspace.json"),
        marker.replace("\"schema\": 1", "\"schema\": 5"),
    )
    .unwrap();
    assert!(matches!(
        Workspace::open(&root),
        Err(WorkspaceError::NewerMarker)
    ));
}

#[test]
fn a_photo_sidecar_lands_in_its_shard_and_is_not_rewritten_when_unchanged() {
    let (_dir, ws) = new_workspace();
    let p = photo(3);
    assert_eq!(ws.write_photo(&p).unwrap(), WriteOutcome::Written);
    let path = ws
        .root()
        .join("photos")
        .join(p.photo_id.shard())
        .join(format!("{}.xmp", p.photo_id));
    assert!(path.exists(), "{}", path.display());
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(ws.write_photo(&p).unwrap(), WriteOutcome::Unchanged);
    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        modified,
        "no new modification time"
    );
    let mut changed = p.clone();
    changed.meta.flag = Some(Flag::Picked);
    assert_eq!(ws.write_photo(&changed).unwrap(), WriteOutcome::Written);
    let read = ws
        .read_photo(&p.photo_id)
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(read, changed);
    assert!(ws.read_photo(&PhotoId::random()).unwrap().is_none());
    // no temporary file is left behind by successful writes
    assert_eq!(
        fs::read_dir(ws.root().join(".auroraw/tmp"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn versions_and_every_state_file_round_trip_through_the_workspace() {
    let (_dir, ws) = new_workspace();
    let p = photo(4);
    ws.write_photo(&p).unwrap();
    let v = VersionSidecar::new(&p, VersionId::random());
    ws.write_version(&v).unwrap();
    assert_eq!(
        ws.read_version(&p.photo_id, &v.version_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap(),
        v
    );

    let vocabulary = Vocabulary {
        updated: now(),
        keywords: vec![],
        extra: Default::default(),
    };
    ws.write_vocabulary(&vocabulary).unwrap();
    assert_eq!(
        ws.read_vocabulary().unwrap().unwrap().current().unwrap(),
        vocabulary
    );

    let sources = Sources {
        updated: now(),
        sources: vec![],
        extra: Default::default(),
    };
    ws.write_sources(&sources).unwrap();
    assert!(matches!(
        ws.read_sources().unwrap().unwrap(),
        Loaded::Current(_)
    ));

    let collection = Collection {
        id: CollectionId::random(),
        updated: now(),
        name: "Picks".into(),
        kind: "manual".into(),
        parent: None,
        members: vec![auroraw_types::MemberRef::Photo(p.photo_id)],
        query: None,
        query_schema: None,
        extra: Default::default(),
    };
    ws.write_collection(&collection).unwrap();
    assert_eq!(
        ws.read_collection(&collection.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap(),
        collection
    );

    let series = series_of(&p);
    ws.write_series(&series).unwrap();
    assert_eq!(
        ws.read_series(&series.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap(),
        series
    );
    assert!(
        ws.series_path(&series.id)
            .parent()
            .unwrap()
            .ends_with(&series.id.to_string()[..2])
    );
}

#[test]
fn a_second_opener_gets_a_read_only_workspace_until_the_first_lets_go() {
    let (_dir, ws) = new_workspace();
    let root = ws.root().to_path_buf();
    let second = Workspace::open(&root).unwrap();
    assert_eq!(second.access(), Access::ReadOnly);
    assert!(matches!(
        second.write_photo(&photo(1)),
        Err(WorkspaceError::ReadOnly)
    ));
    assert!(second.scan().is_ok(), "reading works");
    drop(second);
    assert_eq!(
        Workspace::open(&root).unwrap().access(),
        Access::ReadOnly,
        "the first still holds the lock"
    );
    drop(ws);
    assert_eq!(Workspace::open(&root).unwrap().access(), Access::ReadWrite);
}

#[test]
fn the_scan_classifies_what_it_knows_and_leaves_the_rest_alone() {
    let (_dir, ws) = new_workspace();
    let p = photo(5);
    ws.write_photo(&p).unwrap();
    let v = VersionSidecar::new(&p, VersionId::random());
    ws.write_version(&v).unwrap();
    ws.write_series(&series_of(&p)).unwrap();
    ws.write_sources(&Sources {
        updated: now(),
        sources: vec![],
        extra: Default::default(),
    })
    .unwrap();

    // things that are not ours
    let photo_dir = ws.root().join("photos").join(p.photo_id.shard());
    fs::write(
        photo_dir.join(format!("{} (conflicted copy).xmp", p.photo_id)),
        "x",
    )
    .unwrap();
    fs::write(photo_dir.join("notes.txt"), "x").unwrap();
    let other = PhotoId::from_bytes([0xee; 16]);
    let wrong = ws.root().join("photos").join("00");
    fs::create_dir_all(&wrong).unwrap();
    fs::write(wrong.join(format!("{other}.xmp")), "x").unwrap(); // right name, wrong shard
    let vdir = ws.root().join("versions").join(p.photo_id.shard());
    fs::write(
        vdir.join(format!("{}.{}.history.json", p.photo_id, v.version_id)),
        "{}",
    )
    .unwrap(); // a later companion
    fs::create_dir_all(ws.root().join("annotations")).unwrap(); // a newer version's folder
    fs::write(ws.root().join("stray.txt"), "x").unwrap();
    fs::create_dir_all(ws.exports_dir()).unwrap();
    fs::write(ws.exports_dir().join("a.jpg"), "x").unwrap(); // exports are never scanned

    let scan = ws.scan().unwrap();
    assert_eq!(scan.photos.len(), 1);
    assert_eq!(scan.photos[0].key, p.photo_id);
    assert!(scan.photos[0].stat.size > 100);
    assert!(scan.photos[0].stat.modified.is_some());
    assert_eq!(scan.versions.len(), 1);
    assert_eq!(scan.versions[0].key, (p.photo_id, v.version_id));
    assert_eq!(scan.series.len(), 1);
    assert!(scan.sources.is_some() && scan.vocabulary.is_none());
    let reasons: Vec<ForeignReason> = scan.foreign.iter().map(|f| f.reason).collect();
    assert_eq!(scan.foreign.len(), 6, "{:?}", scan.foreign);
    for wanted in [
        ForeignReason::WrongShard,
        ForeignReason::Companion,
        ForeignReason::UnknownFolder,
    ] {
        assert!(reasons.contains(&wanted), "{wanted:?} in {reasons:?}");
    }
    assert_eq!(
        reasons
            .iter()
            .filter(|r| **r == ForeignReason::Unrecognised)
            .count(),
        3
    );
    // and nothing was touched
    assert!(photo_dir.join("notes.txt").exists() && ws.root().join("stray.txt").exists());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_never_followed() {
    let (_dir, ws) = new_workspace();
    let p = photo(1);
    ws.write_photo(&p).unwrap();
    let outside = temp_dir();
    fs::write(outside.path().join("secret.xmp"), "x").unwrap();
    let mut bytes = *p.photo_id.as_bytes();
    bytes[15] ^= 1;
    let sibling = PhotoId::from_bytes(bytes); // same shard
    let link = ws
        .root()
        .join("photos")
        .join(p.photo_id.shard())
        .join(format!("{sibling}.xmp"));
    std::os::unix::fs::symlink(outside.path().join("secret.xmp"), &link).unwrap();
    let scan = ws.scan().unwrap();
    assert_eq!(scan.photos.len(), 1);
    assert!(
        scan.foreign
            .iter()
            .any(|f| f.reason == ForeignReason::SymbolicLink)
    );
}

#[test]
fn removal_moves_to_removed_and_never_collides() {
    let (_dir, ws) = new_workspace();
    let p = photo(2);
    ws.write_photo(&p).unwrap();
    let original = fs::read(ws.photo_path(&p.photo_id)).unwrap();
    let moved = ws.remove_recoverably(&ws.photo_path(&p.photo_id)).unwrap();
    assert!(!ws.photo_path(&p.photo_id).exists());
    assert_eq!(fs::read(&moved).unwrap(), original);
    assert!(
        moved.starts_with(ws.root().join("removed").join("photos")),
        "{}",
        moved.display()
    );
    // the same path removed again the same second gets another name
    ws.write_photo(&p).unwrap();
    let again = ws.remove_recoverably(&ws.photo_path(&p.photo_id)).unwrap();
    assert_ne!(moved, again);
    // the removed folder is not scanned as a source of photos
    assert!(ws.scan().unwrap().photos.is_empty());
}

#[test]
fn a_removed_collection_is_recoverable_and_removing_none_is_fine() {
    let (_dir, ws) = new_workspace();
    let collection = Collection {
        id: CollectionId::random(),
        updated: now(),
        name: "Picks".into(),
        kind: "manual".into(),
        parent: None,
        members: Vec::new(),
        query: None,
        query_schema: None,
        extra: Default::default(),
    };
    ws.write_collection(&collection).unwrap();
    assert_eq!(ws.scan().unwrap().collections.len(), 1);
    ws.remove_collection(&collection.id).unwrap();
    assert!(ws.read_collection(&collection.id).unwrap().is_none());
    assert!(ws.scan().unwrap().collections.is_empty());
    assert!(
        ws.root()
            .join("removed")
            .join("state")
            .join("collections")
            .is_dir(),
        "the file is kept under removed/"
    );
    // Nothing there any more: not an error.
    ws.remove_collection(&collection.id).unwrap();
}

#[test]
fn removal_refuses_paths_outside_the_workspace() {
    let (dir, ws) = new_workspace();
    let outside = dir.path().join("elsewhere.txt");
    fs::write(&outside, "x").unwrap();
    assert!(matches!(
        ws.remove_recoverably(&outside),
        Err(WorkspaceError::Outside(_))
    ));
    assert!(matches!(
        ws.remove_recoverably(std::path::Path::new("../elsewhere.txt")),
        Err(WorkspaceError::Outside(_))
    ));
    assert!(outside.exists());
}

#[test]
fn an_unreadable_sidecar_is_an_error_naming_the_file_and_is_not_overwritten_by_reading() {
    let (_dir, ws) = new_workspace();
    let p = photo(1);
    ws.write_photo(&p).unwrap();
    let path = ws.photo_path(&p.photo_id);
    fs::write(&path, "this is not xml").unwrap();
    match ws.read_photo(&p.photo_id) {
        Err(WorkspaceError::Sidecar { path: named, .. }) => assert_eq!(named, path),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), "this is not xml");
}

#[test]
fn a_hundred_random_photos_are_all_found_by_the_scan() {
    let (_dir, ws) = new_workspace();
    let mut r = rng(1);
    let mut ids = Vec::new();
    for _ in 0..100 {
        let mut bytes = [0u8; 16];
        bytes.iter_mut().for_each(|b| *b = r.u8(..));
        let mut p = PhotoSidecar::new(PhotoId::from_bytes(bytes));
        p.meta.rating = Some(r.u8(0..=5));
        ws.write_photo(&p).unwrap();
        ids.push(p.photo_id);
    }
    let scan = ws.scan().unwrap();
    assert_eq!(scan.photos.len(), 100);
    assert!(scan.foreign.is_empty(), "{:?}", scan.foreign);
    for id in ids {
        assert!(scan.photos.iter().any(|e| e.key == id));
    }
}

proptest! {
    #[test]
    fn the_shard_is_always_the_first_two_characters(bytes in any::<[u8; 16]>()) {
        let id = PhotoId::from_bytes(bytes);
        let (_dir, ws) = new_workspace();
        let path = ws.photo_path(&id);
        let shard = path.parent().unwrap().file_name().unwrap().to_str().unwrap().to_string();
        prop_assert_eq!(&shard, &id.to_string()[..2]);
        prop_assert_eq!(path.file_name().unwrap().to_str().unwrap(), format!("{id}.xmp"));
    }
}

#[cfg(windows)]
mod windows_sharing {
    //! Antivirus scanners and indexers hold files open without allowing deletion; a rename over
    //! such a file fails until they let go. The write retries for a short while (note 001 §5.4).
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    fn hold(path: &std::path::Path) -> fs::File {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    #[test]
    fn a_write_succeeds_when_the_other_program_lets_go_in_time() {
        let (_dir, ws) = new_workspace();
        let mut p = photo(1);
        ws.write_photo(&p).unwrap();
        let held = hold(&ws.photo_path(&p.photo_id));
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(held);
        });
        p.meta.rating = Some(5);
        ws.write_photo(&p).expect("the retry outlasts a short hold");
        releaser.join().unwrap();
        assert_eq!(
            ws.read_photo(&p.photo_id)
                .unwrap()
                .unwrap()
                .current()
                .unwrap()
                .meta
                .rating,
            Some(5)
        );
    }

    #[test]
    fn a_write_that_cannot_get_through_fails_cleanly() {
        let (_dir, ws) = new_workspace();
        let mut p = photo(1);
        ws.write_photo(&p).unwrap();
        let held = hold(&ws.photo_path(&p.photo_id));
        p.meta.rating = Some(5);
        assert!(ws.write_photo(&p).is_err());
        drop(held);
        assert_eq!(
            ws.read_photo(&p.photo_id)
                .unwrap()
                .unwrap()
                .current()
                .unwrap()
                .meta
                .rating,
            Some(1),
            "the old file is intact"
        );
        assert_eq!(
            fs::read_dir(ws.root().join(".auroraw/tmp"))
                .unwrap()
                .count(),
            0,
            "no temporary file left"
        );
    }
}

#[test]
fn a_file_from_outside_is_kept_recoverably_under_removed_and_never_leaves_its_place() {
    let (_dir, ws) = new_workspace();
    let relative = std::path::Path::new("external-xmp")
        .join("source")
        .join("d/a.xmp");
    let first = ws.keep_recoverably(&relative, b"one").unwrap();
    let second = ws.keep_recoverably(&relative, b"two").unwrap();
    assert_ne!(
        first, second,
        "the same name in the same second is another file"
    );
    assert_eq!(fs::read(&first).unwrap(), b"one");
    assert_eq!(fs::read(&second).unwrap(), b"two");
    assert!(first.starts_with(ws.root().join("removed").join("external-xmp")));
    assert!(matches!(
        ws.keep_recoverably(std::path::Path::new("../escape.xmp"), b"x"),
        Err(WorkspaceError::Outside(_))
    ));
}

#[test]
fn a_file_written_beside_its_target_replaces_it_whole_and_leaves_no_temporary_file() {
    let dir = auroraw_testkit::temp_dir();
    let target = dir.path().join("a.xmp");
    auroraw_workspace::write_beside(&target, b"first").unwrap();
    auroraw_workspace::write_beside(&target, b"second and longer").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second and longer");
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.xmp"]);
    // A folder that does not exist is an error here, not created.
    let missing = dir.path().join("nope/b.xmp");
    assert!(auroraw_workspace::write_beside(&missing, b"x").is_err());
    assert!(!dir.path().join("nope").exists());
}

#[test]
fn a_new_file_never_replaces_one_that_is_there_however_late_it_appeared() {
    let dir = auroraw_testkit::temp_dir();
    let target = dir.path().join("IMG_0042.xmp");
    // Nothing there: written whole, no temporary file left.
    auroraw_workspace::write_new_beside(&target, b"ours").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"ours");
    // Another application's file, made after whatever listing the caller went by: untouched.
    let theirs = dir.path().join("IMG_0043.xmp");
    fs::write(&theirs, "LIGHTROOM'S FILE").unwrap();
    let e = auroraw_workspace::write_new_beside(&theirs, b"ours").unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&theirs).unwrap(), b"LIGHTROOM'S FILE");
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["IMG_0042.xmp", "IMG_0043.xmp"],
        "and no temporary file"
    );
    // A folder that does not exist is an error, not created.
    assert!(auroraw_workspace::write_new_beside(&dir.path().join("nope/c.xmp"), b"x").is_err());
    assert!(!dir.path().join("nope").exists());
}

#[test]
fn a_temporary_file_left_by_a_killed_export_does_not_stop_the_next_one() {
    let dir = auroraw_testkit::temp_dir();
    let target = dir.path().join("a.xmp");
    fs::write(dir.path().join("a.xmp.part"), "half a file").unwrap();
    auroraw_workspace::write_beside(&target, b"whole").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"whole");
    fs::write(dir.path().join("a.xmp.part"), "half a file").unwrap();
    let other = dir.path().join("b.xmp");
    auroraw_workspace::write_new_beside(&other, b"whole").unwrap();
    assert_eq!(fs::read(&other).unwrap(), b"whole");
    assert!(
        !dir.path().join("b.xmp.part").exists(),
        "the stale name went with the write"
    );
}
