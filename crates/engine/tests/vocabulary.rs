// SPDX-License-Identifier: GPL-3.0-or-later
//! Changes to the vocabulary are actions of the person's and are undone like any other (D-099): making a
//! keyword (alone, or with the photos that first get it, as one step), renaming, moving, deleting a branch
//! and setting its synonyms and export flag (WP10 slice 2). Each ends in the same place in the vocabulary
//! file, the catalogue and the sidecars, whether it is done, undone or redone; a move that would make a
//! cycle or a name clash is refused; two path refreshes never finish in the wrong order; and a refresh
//! never writes back a sidecar that a source removal moved away.

use std::time::{Duration, Instant};

use auroraw_catalogue::Catalogue;
use auroraw_engine::{
    AddSourceRequest, Command, Engine, EngineError, Event, EventReceiver, Label, LabelKind, Outcome,
};
use auroraw_format::sidecar::{Metadata, PhotoSidecar};
use auroraw_format::state::KeywordEntry;
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::{KeywordId, PhotoId};
use auroraw_workspace::Workspace;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
    /// Refresh jobs that ended while `undo_label` was draining the events (`wait_for_refreshes` counts them).
    refreshes_ended: std::cell::Cell<usize>,
}

/// A workspace with `photos` photos and an empty vocabulary.
fn fixture(photos: usize) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let photo_ids: Vec<PhotoId> = (0..photos).map(|_| PhotoId::random()).collect();
    let ws = Workspace::create(&root, "Main").unwrap();
    for id in &photo_ids {
        ws.write_photo(&PhotoSidecar::new(*id)).unwrap();
    }
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
        photos: photo_ids,
        refreshes_ended: std::cell::Cell::new(0),
    }
}

impl Fixture {
    fn create(&self, name: &str, parent: Option<KeywordId>) -> KeywordId {
        let Outcome::KeywordCreated(id) = self
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

    fn give(&self, photo: PhotoId, keyword: KeywordId) {
        self.engine
            .submit_and_wait(Command::AddKeyword {
                photo_id: photo,
                keyword_id: keyword,
            })
            .unwrap();
    }

    fn meta(&self, photo: PhotoId) -> Metadata {
        self.engine
            .workspace()
            .read_photo(&photo)
            .unwrap()
            .unwrap()
            .current()
            .unwrap()
            .meta
    }

    /// The vocabulary file: (name, parent) by identifier.
    fn vocabulary(&self) -> Vec<KeywordEntry> {
        self.engine
            .workspace()
            .read_vocabulary()
            .unwrap()
            .and_then(|l| l.current())
            .map(|v| v.keywords)
            .unwrap_or_default()
    }

    /// The catalogue's keyword rows: (path, photos), sorted by path.
    fn rows(&self) -> Vec<(String, u64)> {
        self.engine
            .read_catalogue()
            .unwrap()
            .keywords_with_counts()
            .unwrap()
            .into_iter()
            .map(|k| (k.path, k.photos))
            .collect()
    }

    /// The catalogue's own synonyms and export flag for one keyword (by path).
    fn properties(&self, path: &str) -> (Vec<String>, bool) {
        let row = self
            .engine
            .read_catalogue()
            .unwrap()
            .keywords_with_counts()
            .unwrap()
            .into_iter()
            .find(|k| k.path == path)
            .unwrap();
        (row.synonyms, row.export)
    }

    /// What Undo would undo now, from the last `HistoryChanged` since the events were last drained.
    fn undo_label(&self) -> Option<Label> {
        self.events
            .drain()
            .into_iter()
            .filter_map(|e| match e {
                Event::HistoryChanged(state) => Some(state.undo),
                // A quick job may be over already: it is not lost to the wait that follows.
                Event::JobFinished(_) | Event::JobCancelled(_) => {
                    self.refreshes_ended.set(self.refreshes_ended.get() + 1);
                    None
                }
                _ => None,
            })
            .next_back()
            .flatten()
    }

    /// Waits until every path-refresh job asked for so far is over (the events it sends).
    fn wait_for_refreshes(&self, jobs: usize) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = self.refreshes_ended.replace(0).min(jobs);
        while seen < jobs {
            match self.events.recv_timeout(Duration::from_millis(100)) {
                Some(Event::JobFinished(_) | Event::JobCancelled(_)) => seen += 1,
                Some(_) => {}
                None => assert!(Instant::now() < deadline, "the refresh never ended"),
            }
        }
    }
}

fn paths_of(meta: &Metadata) -> Vec<String> {
    meta.keyword_paths.clone()
}

#[test]
fn a_keyword_made_with_its_first_photos_is_one_step_and_undo_takes_all_of_it_back() {
    let f = fixture(3);
    let peru = KeywordId::random();
    f.engine
        .submit_and_wait(Command::Batch {
            commands: vec![
                Command::CreateKeyword {
                    name: "Peru".into(),
                    parent: None,
                    id: Some(peru),
                },
                Command::AddKeyword {
                    photo_id: f.photos[0],
                    keyword_id: peru,
                },
                Command::AddKeyword {
                    photo_id: f.photos[1],
                    keyword_id: peru,
                },
            ],
        })
        .unwrap();
    assert_eq!(f.rows(), vec![("Peru".to_string(), 2u64)]);
    assert_eq!(paths_of(&f.meta(f.photos[0])), vec!["Peru"]);
    let label = f.undo_label().unwrap();
    assert_eq!(label.kind, LabelKind::KeywordCreate);

    f.engine.undo().unwrap();
    assert!(
        f.vocabulary().is_empty(),
        "the keyword is gone from the file"
    );
    assert!(f.rows().is_empty(), "and from the catalogue");
    assert!(f.meta(f.photos[0]).keyword_ids.is_empty());
    assert!(f.meta(f.photos[1]).keyword_ids.is_empty());
    assert_eq!(f.engine.undo().unwrap(), Outcome::Nothing, "one step");

    f.engine.redo().unwrap();
    assert_eq!(f.rows(), vec![("Peru".to_string(), 2u64)]);
    assert_eq!(
        f.meta(f.photos[1]).keyword_ids,
        vec![peru],
        "the same keyword"
    );
}

#[test]
fn a_keyword_made_alone_can_be_undone_and_a_name_that_is_taken_is_refused() {
    let f = fixture(1);
    let birds = f.create("Birds", None);
    assert_eq!(f.undo_label().unwrap().kind, LabelKind::KeywordCreate);
    let again = f.engine.submit_and_wait(Command::CreateKeyword {
        name: "birds".into(),
        parent: None,
        id: None,
    });
    assert!(
        matches!(again, Err(EngineError::KeywordNameTaken(_))),
        "a sibling has that name, whatever the case: {again:?}"
    );
    // Under another parent the same name is fine.
    f.create("Birds", Some(birds));
    for bad in ["", "  ", "a|b"] {
        assert!(matches!(
            f.engine.submit_and_wait(Command::CreateKeyword {
                name: bad.into(),
                parent: None,
                id: None
            }),
            Err(EngineError::KeywordName)
        ));
    }
    f.engine.undo().unwrap(); // the child
    f.engine.undo().unwrap(); // Birds
    assert!(f.vocabulary().is_empty() && f.rows().is_empty());
}

#[test]
fn setting_a_keywords_synonyms_and_export_flag_is_one_step_and_undo_brings_back_the_old_ones() {
    let f = fixture(1);
    let redwood = f.create("Redwood", None);
    f.events.drain();
    assert_eq!(f.properties("Redwood"), (vec![], true), "the defaults");

    f.engine
        .submit_and_wait(Command::SetKeywordProperties {
            keyword_id: redwood,
            synonyms: vec!["Sequoia".to_string(), "Coast redwood".to_string()],
            export: false,
        })
        .unwrap();
    assert_eq!(
        f.properties("Redwood"),
        (
            vec!["Sequoia".to_string(), "Coast redwood".to_string()],
            false
        )
    );
    assert_eq!(f.vocabulary()[0].synonyms, vec!["Sequoia", "Coast redwood"]);
    assert!(!f.vocabulary()[0].export);
    let label = f.undo_label().unwrap();
    assert_eq!(label.kind, LabelKind::KeywordProperties);

    f.engine.undo().unwrap();
    assert_eq!(
        f.properties("Redwood"),
        (vec![], true),
        "back to the defaults"
    );

    f.engine.redo().unwrap();
    assert_eq!(
        f.properties("Redwood"),
        (
            vec!["Sequoia".to_string(), "Coast redwood".to_string()],
            false
        )
    );
}

#[test]
fn setting_the_same_properties_again_changes_nothing_and_is_not_a_step() {
    let f = fixture(1);
    let redwood = f.create("Redwood", None);
    f.engine
        .submit_and_wait(Command::SetKeywordProperties {
            keyword_id: redwood,
            synonyms: vec!["Sequoia".to_string()],
            export: false,
        })
        .unwrap();
    f.events.drain();

    let outcome = f
        .engine
        .submit_and_wait(Command::SetKeywordProperties {
            keyword_id: redwood,
            synonyms: vec!["Sequoia".to_string()],
            export: false,
        })
        .unwrap();
    assert_eq!(outcome, Outcome::Applied);
    assert!(f.undo_label().is_none(), "no new step to undo");
}

#[test]
fn moving_a_branch_updates_the_catalogue_and_the_sidecars_and_undo_brings_it_back() {
    let f = fixture(4);
    let places = f.create("Places", None);
    let peru = f.create("Peru", Some(places));
    let cusco = f.create("Cusco", Some(peru));
    let trips = f.create("Trips", None);
    f.give(f.photos[0], cusco);
    f.give(f.photos[1], peru);
    f.events.drain();

    let outcome = f
        .engine
        .submit_and_wait(Command::MoveKeyword {
            keyword_id: peru,
            new_parent: Some(trips),
        })
        .unwrap();
    assert_eq!(
        outcome,
        Outcome::KeywordsChanged {
            keywords: 2,
            photos: 2
        }
    );
    assert_eq!(f.undo_label().unwrap().kind, LabelKind::KeywordMove);
    f.wait_for_refreshes(1);
    let rows: Vec<String> = f.rows().into_iter().map(|r| r.0).collect();
    assert_eq!(
        rows,
        ["Places", "Trips", "Trips|Peru", "Trips|Peru|Cusco"],
        "the catalogue's paths follow the whole branch"
    );
    assert_eq!(paths_of(&f.meta(f.photos[0])), vec!["Trips|Peru|Cusco"]);
    assert_eq!(paths_of(&f.meta(f.photos[1])), vec!["Trips|Peru"]);

    f.engine.undo().unwrap();
    f.wait_for_refreshes(1);
    let rows: Vec<String> = f.rows().into_iter().map(|r| r.0).collect();
    assert_eq!(
        rows,
        ["Places", "Places|Peru", "Places|Peru|Cusco", "Trips"]
    );
    assert_eq!(paths_of(&f.meta(f.photos[0])), vec!["Places|Peru|Cusco"]);

    f.engine.redo().unwrap();
    f.wait_for_refreshes(1);
    assert_eq!(paths_of(&f.meta(f.photos[1])), vec!["Trips|Peru"]);

    // To the top level, and to where it is (nothing to do, no step).
    f.engine
        .submit_and_wait(Command::MoveKeyword {
            keyword_id: peru,
            new_parent: None,
        })
        .unwrap();
    f.wait_for_refreshes(1);
    assert_eq!(paths_of(&f.meta(f.photos[1])), vec!["Peru"]);
    f.events.drain();
    f.engine
        .submit_and_wait(Command::MoveKeyword {
            keyword_id: peru,
            new_parent: None,
        })
        .unwrap();
    assert_eq!(
        f.undo_label(),
        None,
        "no new step (no HistoryChanged at all)"
    );
}

#[test]
fn a_move_that_makes_a_cycle_or_a_name_clash_is_refused_and_changes_nothing() {
    let f = fixture(1);
    let a = f.create("A", None);
    let b = f.create("B", Some(a));
    let c = f.create("C", Some(b));
    let other_b = f.create("b", None);
    let before = f.vocabulary().len();
    for (id, parent) in [(a, Some(a)), (a, Some(c)), (b, Some(c))] {
        let refused = f.engine.submit_and_wait(Command::MoveKeyword {
            keyword_id: id,
            new_parent: parent,
        });
        assert!(
            matches!(refused, Err(EngineError::KeywordCycle)),
            "{id} under {parent:?}: {refused:?}"
        );
    }
    let clash = f.engine.submit_and_wait(Command::MoveKeyword {
        keyword_id: other_b,
        new_parent: Some(a),
    });
    assert!(
        matches!(clash, Err(EngineError::KeywordNameTaken(_))),
        "{clash:?}"
    );
    assert_eq!(f.vocabulary().len(), before);
    let rows: Vec<String> = f.rows().into_iter().map(|r| r.0).collect();
    assert_eq!(rows, ["A", "A|B", "A|B|C", "b"]);
    let missing = f.engine.submit_and_wait(Command::MoveKeyword {
        keyword_id: KeywordId::random(),
        new_parent: None,
    });
    assert!(matches!(missing, Err(EngineError::NotFound { .. })));
}

#[test]
fn deleting_a_branch_takes_it_off_the_photos_and_undo_gives_everything_back() {
    let f = fixture(5);
    let animals = f.create("Animals", None);
    let birds = f.create("Birds", Some(animals));
    let heron = f.create("Heron", Some(birds));
    let other = f.create("Other", None);
    f.give(f.photos[0], heron);
    f.give(f.photos[0], other);
    f.give(f.photos[1], birds);
    f.give(f.photos[2], animals);
    f.give(f.photos[3], other);
    let meta_before: Vec<Metadata> = f.photos.iter().map(|p| f.meta(*p)).collect();
    let vocabulary_before = f.vocabulary();
    let rows_before = f.rows();
    f.events.drain();

    let outcome = f
        .engine
        .submit_and_wait(Command::DeleteKeyword {
            keyword_id: animals,
        })
        .unwrap();
    assert_eq!(
        outcome,
        Outcome::KeywordsChanged {
            keywords: 3,
            photos: 3
        }
    );
    let label = f.undo_label().unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::KeywordDelete, 3));
    assert_eq!(f.vocabulary().len(), 1, "only Other is left");
    assert_eq!(f.rows(), vec![("Other".to_string(), 2u64)]);
    assert_eq!(f.meta(f.photos[0]).keyword_ids, vec![other]);
    assert!(f.meta(f.photos[1]).keyword_ids.is_empty());
    assert!(f.meta(f.photos[2]).keyword_ids.is_empty());
    assert_eq!(f.meta(f.photos[3]).keyword_ids, vec![other], "untouched");

    let Outcome::History(_) = f.engine.undo().unwrap() else {
        panic!("expected History");
    };
    let events = f.events.drain();
    assert!(
        events.iter().any(
            |e| matches!(e, Event::HistoryApplied { photos, redo: false } if photos.is_empty())
        ),
        "a keyword coming back does not select photos"
    );
    let mut restored = f.vocabulary();
    let mut expected = vocabulary_before.clone();
    restored.sort_by_key(|k| k.id);
    expected.sort_by_key(|k| k.id);
    assert_eq!(restored, expected, "the same keywords, same identifiers");
    assert_eq!(f.rows(), rows_before);
    for (photo, before) in f.photos.iter().zip(&meta_before) {
        assert_eq!(&f.meta(*photo), before, "sidecars exactly as they were");
    }

    f.engine.redo().unwrap();
    assert_eq!(f.rows(), vec![("Other".to_string(), 2u64)]);
    assert!(f.meta(f.photos[1]).keyword_ids.is_empty());

    // A rebuild from the workspace agrees with the live catalogue (no keyword is invented from a sidecar).
    f.engine.submit_and_wait(Command::Rebuild).unwrap();
    assert_eq!(f.rows(), vec![("Other".to_string(), 2u64)]);
    f.engine.undo().unwrap();
    f.engine.submit_and_wait(Command::Rebuild).unwrap();
    assert_eq!(f.rows(), rows_before);
}

#[test]
fn a_rename_followed_at_once_by_its_undo_ends_with_the_old_paths() {
    let f = fixture(60);
    let animals = f.create("Animals", None);
    let birds = f.create("Birds", Some(animals));
    for photo in &f.photos {
        f.give(*photo, birds);
    }
    f.events.drain();
    let renamed = f
        .engine
        .submit_and_wait(Command::RenameKeyword {
            keyword_id: animals,
            new_name: "Fauna".into(),
        })
        .unwrap();
    assert!(matches!(
        renamed,
        Outcome::RenameStarted { affected: 60, .. }
    ));
    assert_eq!(f.undo_label().unwrap().kind, LabelKind::KeywordRename);
    // No waiting: the second refresh is asked while the first runs.
    f.engine.undo().unwrap();
    f.wait_for_refreshes(2);
    for photo in &f.photos {
        assert_eq!(
            paths_of(&f.meta(*photo)),
            vec!["Animals|Birds"],
            "the last vocabulary wins"
        );
    }
    f.engine.redo().unwrap();
    f.wait_for_refreshes(1);
    assert_eq!(paths_of(&f.meta(f.photos[59])), vec!["Fauna|Birds"]);
}

#[test]
fn a_rename_refresh_and_a_source_removal_together_leave_no_sidecar_behind() {
    // A source of photos that all carry a keyword; the keyword is renamed and the source removed at the same
    // time. However the two background jobs interleave, nothing that was removed may come back into
    // `photos/`, and what stays has its final path.
    let dir = temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let folder = dir.path().join("Trip");
    std::fs::create_dir_all(&folder).unwrap();
    for i in 0..40u8 {
        // A pattern that never collides after JPEG's lossy quantization, unlike a plainer one tried here once
        // (two of forty ended up bit-identical files, wrongly joined as one photo once this test's job-wide
        // duplicate check, D-108, could see across the whole catalogue rather than only within its own batch).
        let img = ImageBuffer::from_fn(32, 24, |x, y| Rgb([(x * 8) as u8 ^ i, (y * 9) as u8, i]));
        DynamicImage::ImageRgb8(img)
            .save_with_format(folder.join(format!("p{i}.jpg")), ImageFormat::Jpeg)
            .unwrap();
    }
    let added = engine
        .add_source(AddSourceRequest {
            root: folder,
            name: None,
            merge: false,
        })
        .unwrap();
    wait_for(
        &events,
        |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
    );
    let Outcome::KeywordCreated(animals) = engine
        .submit_and_wait(Command::CreateKeyword {
            name: "Animals".into(),
            parent: None,
            id: None,
        })
        .unwrap()
    else {
        panic!()
    };
    let photos: Vec<PhotoId> = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 100)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(photos.len(), 40);
    engine
        .submit_and_wait(Command::Batch {
            commands: photos
                .iter()
                .map(|p| Command::AddKeyword {
                    photo_id: *p,
                    keyword_id: animals,
                })
                .collect(),
        })
        .unwrap();
    events.drain();

    engine
        .submit_and_wait(Command::RenameKeyword {
            keyword_id: animals,
            new_name: "Fauna".into(),
        })
        .unwrap();
    let Outcome::RemoveStarted { job } = engine
        .submit_and_wait(Command::RemoveSource {
            source_id: added.source_id,
        })
        .unwrap()
    else {
        panic!()
    };
    wait_for(
        &events,
        |e| matches!(e, Event::SourceRemoved { job: j, .. } if *j == job),
    );
    // Let the refresh finish too (a rename's job ends with JobFinished, the removal's already came).
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut quiet = 0;
    while quiet < 5 && Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(100)) {
            Some(_) => quiet = 0,
            None => quiet += 1,
        }
    }

    let catalogue_photos = engine.read_catalogue().unwrap().count_all().unwrap();
    let on_disk = engine.workspace().scan().unwrap().photos.len() as u64;
    assert_eq!(
        on_disk, catalogue_photos,
        "every sidecar in photos/ is a photo of the catalogue (nothing was written back after it left)"
    );
    assert_eq!(
        catalogue_photos, 0,
        "the source was removed with all its photos"
    );
}

fn wait_for(events: &EventReceiver, wanted: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(event) if wanted(&event) => return event,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}
