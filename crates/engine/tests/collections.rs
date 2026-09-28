// SPDX-License-Identifier: GPL-3.0-or-later
//! Manual collections (WP10, slice 3) are actions of the person's and are undone like any other: making one
//! (with its first photos, as one step), renaming, moving, deleting a branch, adding photos and taking them out.
//! Each ends in the same place in the collection's file and the catalogue's rows, whether it is done, undone or
//! redone; a name that a sibling has and a move that would make a cycle are refused; a collection of another
//! kind (smart, client selection) is left alone; and a photo that leaves with its source leaves its collections.

use std::time::{Duration, Instant};

use auroraw_catalogue::{Catalogue, CollectionRow, Filter, FlagFilter};
use auroraw_engine::{
    AddSourceRequest, Command, Engine, EngineError, Event, EventReceiver, Label, LabelKind, Outcome,
};
use auroraw_format::sidecar::PhotoSidecar;
use auroraw_format::state::{Collection, collection_kind};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::{CollectionId, MemberRef, PhotoId, SourceId};
use auroraw_workspace::Workspace;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
}

/// A workspace with `photos` photos and no collection.
fn fixture(photos: usize) -> Fixture {
    fixture_with(photos, |_| {})
}

/// The same, after `before` has written whatever else the workspace should hold when the engine opens it.
fn fixture_with(photos: usize, before: impl FnOnce(&Workspace)) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let ids: Vec<PhotoId> = (0..photos).map(|_| PhotoId::random()).collect();
    let ws = Workspace::create(&root, "Main").unwrap();
    for id in &ids {
        ws.write_photo(&PhotoSidecar::new(*id)).unwrap();
    }
    before(&ws);
    let workspace_id = ws.workspace_id();
    drop(ws);
    drop(Catalogue::create(&catalogue, workspace_id).unwrap());
    let (engine, events) = Engine::open(&root, &catalogue).unwrap();
    engine.submit_and_wait(Command::Rebuild).unwrap();
    events.drain();
    Fixture {
        _dir: dir,
        engine,
        events,
        photos: ids,
    }
}

impl Fixture {
    fn create(&self, name: &str, parent: Option<CollectionId>, photos: &[usize]) -> CollectionId {
        let Outcome::CollectionCreated(id) = self
            .engine
            .submit_and_wait(Command::CreateCollection {
                name: name.into(),
                parent,
                id: None,
                photos: photos.iter().map(|i| self.photos[*i]).collect(),
            })
            .unwrap()
        else {
            panic!("expected CollectionCreated");
        };
        id
    }

    fn add(&self, id: CollectionId, photos: &[usize]) -> Outcome {
        self.engine
            .submit_and_wait(Command::AddToCollection {
                collection_id: id,
                photos: photos.iter().map(|i| self.photos[*i]).collect(),
            })
            .unwrap()
    }

    fn take_out(&self, id: CollectionId, photos: &[usize]) {
        self.engine
            .submit_and_wait(Command::RemoveFromCollection {
                collection_id: id,
                photos: photos.iter().map(|i| self.photos[*i]).collect(),
            })
            .unwrap();
    }

    /// The collection's state file, `None` when there is none.
    fn file(&self, id: CollectionId) -> Option<Collection> {
        self.engine
            .workspace()
            .read_collection(&id)
            .unwrap()
            .and_then(|l| l.current())
    }

    /// The photos of the collection's file, by index into `self.photos`, in the file's order.
    fn members(&self, id: CollectionId) -> Vec<usize> {
        self.file(id)
            .expect("the file is there")
            .members
            .iter()
            .map(|m| {
                self.photos
                    .iter()
                    .position(|p| MemberRef::Photo(*p) == *m)
                    .expect("a member is one of the fixture's photos")
            })
            .collect()
    }

    fn rows(&self) -> Vec<CollectionRow> {
        self.engine
            .read_catalogue()
            .unwrap()
            .collections_with_counts()
            .unwrap()
    }

    /// The catalogue's rows as (name, depth, photos).
    fn shown(&self) -> Vec<(String, i32, u64)> {
        self.rows()
            .into_iter()
            .map(|r| (r.name, r.depth, r.photos))
            .collect()
    }

    /// What Undo would undo now, from the last `HistoryChanged` since the events were last drained.
    fn undo_label(&self) -> Option<Label> {
        self.events
            .drain()
            .into_iter()
            .filter_map(|e| match e {
                Event::HistoryChanged(state) => Some(state.undo),
                _ => None,
            })
            .next_back()
            .flatten()
    }
}

#[test]
fn a_collection_made_with_photos_is_one_step_and_undo_takes_all_of_it_back() {
    let f = fixture(4);
    let picks = f.create("Picks", None, &[2, 0, 3]);
    assert_eq!(
        f.members(picks),
        vec![2, 0, 3],
        "in the order they were given"
    );
    assert_eq!(f.shown(), vec![("Picks".to_string(), 0, 3)]);
    let label = f.undo_label().unwrap();
    assert_eq!(label.kind, LabelKind::CollectionCreate);
    assert_eq!(label.count, 3);

    f.engine.undo().unwrap();
    assert!(f.file(picks).is_none(), "the file is gone");
    assert!(f.rows().is_empty(), "and the rows");
    assert_eq!(f.engine.undo().unwrap(), Outcome::Nothing, "one step");

    f.engine.redo().unwrap();
    assert_eq!(f.members(picks), vec![2, 0, 3]);
    assert_eq!(f.shown(), vec![("Picks".to_string(), 0, 3)]);
}

#[test]
fn adding_and_taking_out_photos_are_steps_and_adding_twice_changes_nothing() {
    let f = fixture(4);
    let picks = f.create("Picks", None, &[]);
    f.add(picks, &[0, 1]);
    f.add(picks, &[1, 2]);
    assert_eq!(
        f.members(picks),
        vec![0, 1, 2],
        "photo 1 was in already: left where it was"
    );
    let label = f.undo_label().unwrap();
    assert_eq!(
        (label.kind, label.count),
        (LabelKind::CollectionAdd, 1),
        "one new photo"
    );

    // Nothing new: not a step.
    assert_eq!(f.add(picks, &[0, 2]), Outcome::Applied);
    assert!(
        f.undo_label().is_none(),
        "no HistoryChanged for a step that was not one"
    );

    f.take_out(picks, &[1, 3]);
    assert_eq!(
        f.members(picks),
        vec![0, 2],
        "photo 3 was not in it: ignored"
    );
    let label = f.undo_label().unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::CollectionRemove, 1));
    assert_eq!(f.shown(), vec![("Picks".to_string(), 0, 2)]);

    f.engine.undo().unwrap();
    assert_eq!(f.members(picks), vec![0, 1, 2]);
    f.engine.undo().unwrap();
    assert_eq!(f.members(picks), vec![0, 1]);
    f.engine.redo().unwrap();
    assert_eq!(f.members(picks), vec![0, 1, 2]);
    assert_eq!(f.shown(), vec![("Picks".to_string(), 0, 3)]);
}

#[test]
fn a_name_a_sibling_has_and_a_move_under_itself_are_refused_and_change_nothing() {
    let f = fixture(1);
    let weddings = f.create("Weddings", None, &[]);
    let marie = f.create("Marie", Some(weddings), &[]);
    let taken = f.engine.submit_and_wait(Command::CreateCollection {
        name: "marie".into(),
        parent: Some(weddings),
        id: None,
        photos: vec![],
    });
    assert!(
        matches!(taken, Err(EngineError::CollectionNameTaken(_))),
        "a sibling has that name, whatever the case: {taken:?}"
    );
    // Under another parent the same name is fine, and so is a top-level one.
    let elsewhere = f.create("Marie", None, &[]);
    for bad in ["", "   "] {
        assert!(matches!(
            f.engine.submit_and_wait(Command::CreateCollection {
                name: bad.into(),
                parent: None,
                id: None,
                photos: vec![]
            }),
            Err(EngineError::CollectionName)
        ));
    }
    let cycle = f.engine.submit_and_wait(Command::MoveCollection {
        collection_id: weddings,
        new_parent: Some(marie),
    });
    assert!(
        matches!(cycle, Err(EngineError::CollectionCycle)),
        "{cycle:?}"
    );
    let clash = f.engine.submit_and_wait(Command::MoveCollection {
        collection_id: marie,
        new_parent: None,
    });
    assert!(
        matches!(clash, Err(EngineError::CollectionNameTaken(_))),
        "the top level has a Marie: {clash:?}"
    );
    let rename_clash = f.engine.submit_and_wait(Command::RenameCollection {
        collection_id: elsewhere,
        new_name: "WEDDINGS".into(),
    });
    assert!(matches!(
        rename_clash,
        Err(EngineError::CollectionNameTaken(_))
    ));
    assert_eq!(
        f.file(marie).unwrap().parent,
        Some(weddings),
        "nothing moved"
    );
    assert_eq!(
        f.shown(),
        vec![
            ("Marie".to_string(), 0, 0),
            ("Weddings".to_string(), 0, 0),
            ("Marie".to_string(), 1, 0)
        ]
    );
}

#[test]
fn renaming_and_moving_follow_the_file_and_the_rows_and_are_undone() {
    let f = fixture(1);
    let folder = f.create("Folder", None, &[]);
    let old = f.create("Old", None, &[0]);
    f.events.drain();

    f.engine
        .submit_and_wait(Command::RenameCollection {
            collection_id: old,
            new_name: "  New  ".into(),
        })
        .unwrap();
    assert_eq!(f.file(old).unwrap().name, "New", "trimmed");
    assert_eq!(f.undo_label().unwrap().kind, LabelKind::CollectionRename);
    // The same name again is not a step.
    f.engine
        .submit_and_wait(Command::RenameCollection {
            collection_id: old,
            new_name: "New".into(),
        })
        .unwrap();
    assert!(f.undo_label().is_none());

    f.engine
        .submit_and_wait(Command::MoveCollection {
            collection_id: old,
            new_parent: Some(folder),
        })
        .unwrap();
    assert_eq!(f.undo_label().unwrap().kind, LabelKind::CollectionMove);
    assert_eq!(
        f.shown(),
        vec![("Folder".to_string(), 0, 0), ("New".to_string(), 1, 1)]
    );

    f.engine.undo().unwrap();
    assert_eq!(f.file(old).unwrap().parent, None);
    assert_eq!(
        f.shown(),
        vec![("Folder".to_string(), 0, 0), ("New".to_string(), 0, 1)]
    );
    f.engine.undo().unwrap();
    assert_eq!(f.file(old).unwrap().name, "Old");
    f.engine.redo().unwrap();
    assert_eq!(f.file(old).unwrap().name, "New");
}

#[test]
fn deleting_a_branch_takes_every_collection_in_it_and_undo_brings_them_all_back() {
    let f = fixture(4);
    let keep = f.create("Keep", None, &[3]);
    let folder = f.create("Folder", None, &[0]);
    let child = f.create("Child", Some(folder), &[0, 1]);
    let grandchild = f.create("Grandchild", Some(child), &[2]);
    f.events.drain();

    f.engine
        .submit_and_wait(Command::DeleteCollection {
            collection_id: folder,
        })
        .unwrap();
    for gone in [folder, child, grandchild] {
        assert!(f.file(gone).is_none());
    }
    assert_eq!(
        f.shown(),
        vec![("Keep".to_string(), 0, 1)],
        "the others stay"
    );
    assert!(f.file(keep).is_some());
    let label = f.undo_label().unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::CollectionDelete, 3));
    // The photos themselves are untouched.
    assert_eq!(f.engine.read_catalogue().unwrap().count_all().unwrap(), 4);

    f.engine.undo().unwrap();
    assert_eq!(f.members(folder), vec![0]);
    assert_eq!(f.members(child), vec![0, 1]);
    assert_eq!(f.members(grandchild), vec![2]);
    assert_eq!(
        f.shown(),
        vec![
            ("Folder".to_string(), 0, 1),
            ("Child".to_string(), 1, 2),
            ("Grandchild".to_string(), 2, 1),
            ("Keep".to_string(), 0, 1),
        ]
    );
    f.engine.redo().unwrap();
    assert_eq!(
        f.shown(),
        vec![("Keep".to_string(), 0, 1)],
        "and redo deletes it again"
    );
}

#[test]
fn the_grid_can_be_filtered_by_a_collection_and_the_ones_under_it() {
    let f = fixture(3);
    let folder = f.create("Folder", None, &[0]);
    let child = f.create("Child", Some(folder), &[0, 1]);
    let listed = |id: CollectionId| -> Vec<PhotoId> {
        let mut ids: Vec<PhotoId> = f
            .engine
            .read_catalogue()
            .unwrap()
            .list_filtered(
                &Filter {
                    flags: FlagFilter::All,
                    collection: Some(id),
                    ..Filter::default()
                },
                None,
                100,
            )
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        ids.sort_by_key(|p| p.to_string());
        ids
    };
    let mut expected = vec![f.photos[0], f.photos[1]];
    expected.sort_by_key(|p| p.to_string());
    assert_eq!(
        listed(folder),
        expected,
        "the folder lists its own and its child's, once each"
    );
    assert_eq!(listed(child), expected);
    // Taking a photo out of the child changes the folder's list too.
    f.take_out(child, &[1]);
    assert_eq!(listed(folder), vec![f.photos[0]]);
}

#[test]
fn a_collection_of_another_kind_is_left_alone_and_refused() {
    let smart = CollectionId::random();
    let f = fixture_with(2, |ws| {
        ws.write_collection(&Collection {
            id: smart,
            updated: "2026-09-28T12:00:00Z".parse().unwrap(),
            name: "Smart".into(),
            kind: collection_kind::SMART.into(),
            parent: None,
            members: vec![],
            query: None,
            query_schema: None,
            extra: Default::default(),
        })
        .unwrap();
    });
    assert!(f.rows().is_empty(), "a smart collection is not listed");
    let before = std::fs::read(f.engine.workspace().collection_path(&smart)).unwrap();
    let photos = vec![f.photos[0]];
    for command in [
        Command::AddToCollection {
            collection_id: smart,
            photos: photos.clone(),
        },
        Command::RenameCollection {
            collection_id: smart,
            new_name: "Other".into(),
        },
        Command::DeleteCollection {
            collection_id: smart,
        },
        // (A move to the place it is in is a no-op by design, so it is asked to a parent that is not there.)
        Command::MoveCollection {
            collection_id: smart,
            new_parent: Some(CollectionId::random()),
        },
    ] {
        let result = f.engine.submit_and_wait(command);
        assert!(
            matches!(result, Err(EngineError::NotFound { .. })),
            "refused: {result:?}"
        );
    }
    let after = std::fs::read(f.engine.workspace().collection_path(&smart)).unwrap();
    assert_eq!(before, after, "the file was not touched");
    // And a manual collection cannot be made under it.
    let under = f.engine.submit_and_wait(Command::CreateCollection {
        name: "Inside".into(),
        parent: Some(smart),
        id: None,
        photos: vec![],
    });
    assert!(
        matches!(under, Err(EngineError::NotFound { .. })),
        "{under:?}"
    );
}

/// Writes `count` small distinct JPEGs in `folder`; `salt` keeps two folders' pictures apart.
fn write_photos(folder: &std::path::Path, salt: u8, count: u8) {
    std::fs::create_dir_all(folder).unwrap();
    for i in 0..count {
        let img = ImageBuffer::from_fn(32, 24, |x, y| {
            Rgb([
                (x * 8) as u8 ^ i,
                (y * 9) as u8 ^ salt,
                i.wrapping_mul(37) ^ salt,
            ])
        });
        DynamicImage::ImageRgb8(img)
            .save_with_format(folder.join(format!("p{i}.jpg")), ImageFormat::Jpeg)
            .unwrap();
    }
}

fn wait_for(events: &EventReceiver, wanted: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(event) if wanted(&event) => return,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}

fn remove(engine: &Engine, events: &EventReceiver, source_id: SourceId) {
    let Outcome::RemoveStarted { job } = engine
        .submit_and_wait(Command::RemoveSource { source_id })
        .unwrap()
    else {
        panic!("expected RemoveStarted");
    };
    wait_for(
        events,
        |e| matches!(e, Event::SourceRemoved { job: j, .. } if *j == job),
    );
}

#[test]
fn a_photo_that_leaves_with_its_source_leaves_its_collections_and_cannot_come_back_by_undo() {
    let dir = temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let mut sources = Vec::new();
    for (name, salt) in [("A", 0u8), ("B", 90u8)] {
        let folder = dir.path().join(name);
        write_photos(&folder, salt, 2);
        let added = engine
            .add_source(AddSourceRequest {
                root: folder,
                name: Some(name.into()),
                merge: false,
            })
            .unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
        );
        sources.push(added.source_id);
    }
    let catalogue = engine.read_catalogue().unwrap();
    let all: Vec<_> = catalogue.list_recent(None, 100).unwrap();
    assert_eq!(all.len(), 4);
    let of = |source: SourceId| -> Vec<PhotoId> {
        all.iter()
            .filter(|p| p.source_id == Some(source))
            .map(|p| p.id)
            .collect()
    };
    let (from_a, from_b) = (of(sources[0]), of(sources[1]));
    assert_eq!((from_a.len(), from_b.len()), (2, 2));
    // A collection with a photo of each source, and one that only source A's photos are in.
    let Outcome::CollectionCreated(mixed) = engine
        .submit_and_wait(Command::CreateCollection {
            name: "Mixed".into(),
            parent: None,
            id: None,
            photos: vec![from_a[0], from_b[0]],
        })
        .unwrap()
    else {
        panic!()
    };
    let Outcome::CollectionCreated(only_a) = engine
        .submit_and_wait(Command::CreateCollection {
            name: "Only A".into(),
            parent: None,
            id: None,
            photos: from_a.clone(),
        })
        .unwrap()
    else {
        panic!()
    };
    events.drain();

    remove(&engine, &events, sources[0]);
    let members = |id: CollectionId| -> Vec<MemberRef> {
        engine
            .workspace()
            .read_collection(&id)
            .unwrap()
            .and_then(|l| l.current())
            .unwrap()
            .members
    };
    assert_eq!(
        members(mixed),
        vec![MemberRef::Photo(from_b[0])],
        "only the photo that stayed"
    );
    assert!(members(only_a).is_empty(), "the collection stays, empty");
    let rows = engine
        .read_catalogue()
        .unwrap()
        .collections_with_counts()
        .unwrap();
    let counts: Vec<(String, u64)> = rows.into_iter().map(|r| (r.name, r.photos)).collect();
    assert_eq!(
        counts,
        vec![("Mixed".to_string(), 1), ("Only A".to_string(), 0)]
    );

    // Undoing the making of "Mixed" or "Only A" must not put a removed photo back into a file.
    engine.undo().unwrap();
    engine.undo().unwrap();
    engine.redo().unwrap();
    engine.redo().unwrap();
    assert_eq!(members(mixed), vec![MemberRef::Photo(from_b[0])]);
    assert!(members(only_a).is_empty());
}
