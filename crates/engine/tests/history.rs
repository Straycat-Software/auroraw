// SPDX-License-Identifier: GPL-3.0-or-later
//! The command journal (D-096): what a person did to their photos is undone and redone as the person
//! meant it. Rating, flag and keywords go back exactly (a never-rated photo to *unset*), an action is one
//! step however many photos it touched, a same-value edit is not a step, a new edit discards what was
//! undone, a step about a photo that has gone is dropped without harming the others, and any sequence
//! of edits, undos and redos ends where a model that replays the surviving edits ends.

use std::collections::HashMap;
use std::path::Path;

use auroraw_catalogue::Catalogue;
use auroraw_engine::{Command, Engine, EngineError, Event, EventReceiver, LabelKind, Outcome};
use auroraw_format::sidecar::{Flag, Metadata, PhotoSidecar};
use auroraw_format::state::{KeywordEntry, Vocabulary};
use auroraw_testkit::{rng, temp_dir};
use auroraw_types::{KeywordId, PhotoId, Timestamp};
use auroraw_workspace::Workspace;

struct Fixture {
    _dir: auroraw_testkit::TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
    keywords: Vec<KeywordId>,
}

fn fixture(photos: usize, keywords: usize) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let photo_ids: Vec<PhotoId> = (0..photos).map(|_| PhotoId::random()).collect();
    let keyword_ids: Vec<KeywordId> = (0..keywords).map(|_| KeywordId::random()).collect();
    seed(&root, &catalogue, &photo_ids, &keyword_ids);
    let (engine, events) = Engine::open(&root, &catalogue).unwrap();
    engine.submit_and_wait(Command::Rebuild).unwrap();
    events.drain();
    Fixture {
        _dir: dir,
        engine,
        events,
        photos: photo_ids,
        keywords: keyword_ids,
    }
}

fn seed(root: &Path, catalogue: &Path, photos: &[PhotoId], keywords: &[KeywordId]) {
    let ws = Workspace::create(root, "Main").unwrap();
    ws.write_vocabulary(&Vocabulary {
        updated: Timestamp::now(),
        keywords: keywords
            .iter()
            .enumerate()
            .map(|(i, id)| KeywordEntry {
                id: *id,
                name: format!("K{i}"),
                parent: None,
                synonyms: Vec::new(),
                export: true,
                extra: Default::default(),
            })
            .collect(),
        extra: Default::default(),
    })
    .unwrap();
    for id in photos {
        ws.write_photo(&PhotoSidecar::new(*id)).unwrap();
    }
    let workspace_id = ws.workspace_id();
    drop(ws);
    drop(Catalogue::create(catalogue, workspace_id).unwrap());
}

impl Fixture {
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

    fn rate(&self, photo: PhotoId, rating: u8) {
        self.engine
            .submit_and_wait(Command::SetRating {
                photo_id: photo,
                rating,
            })
            .unwrap();
    }
}

#[test]
fn a_rating_is_undone_and_redone_and_a_never_rated_photo_goes_back_to_unset() {
    let f = fixture(1, 0);
    let photo = f.photos[0];
    let untouched = f.meta(photo);
    assert_eq!(untouched.rating, None);

    f.rate(photo, 4);
    assert_eq!(f.meta(photo).rating, Some(4));
    f.engine.undo().unwrap();
    assert_eq!(
        f.meta(photo),
        untouched,
        "unset, not 0, and nothing else changed"
    );
    f.engine.redo().unwrap();
    assert_eq!(f.meta(photo).rating, Some(4));
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo), untouched);
    assert_eq!(
        f.engine.undo().unwrap(),
        Outcome::Nothing,
        "nothing left to undo"
    );
}

#[test]
fn the_catalogue_follows_an_undo() {
    let f = fixture(1, 0);
    f.rate(f.photos[0], 5);
    f.engine.undo().unwrap();
    let row = f
        .engine
        .read_catalogue()
        .unwrap()
        .photo(&f.photos[0])
        .unwrap()
        .unwrap();
    assert_eq!(row.effective_rating, 0);
}

#[test]
fn an_edit_that_changes_nothing_is_not_a_step() {
    let f = fixture(1, 0);
    let photo = f.photos[0];
    f.rate(photo, 4);
    f.rate(photo, 4);
    f.engine.undo().unwrap();
    assert_eq!(
        f.meta(photo).rating,
        None,
        "the key pressed twice was one step"
    );
    assert_eq!(f.engine.undo().unwrap(), Outcome::Nothing);
}

#[test]
fn steps_are_undone_in_the_order_they_were_made_and_a_new_edit_discards_what_was_undone() {
    let f = fixture(1, 0);
    let photo = f.photos[0];
    f.rate(photo, 1);
    f.rate(photo, 2);
    f.rate(photo, 3);
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo).rating, Some(2));
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo).rating, Some(1));
    f.engine.redo().unwrap();
    assert_eq!(f.meta(photo).rating, Some(2));

    f.rate(photo, 5);
    assert_eq!(
        f.engine.redo().unwrap(),
        Outcome::Nothing,
        "editing after an undo"
    );
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo).rating, Some(2));
}

#[test]
fn flags_and_keywords_are_undone_too() {
    let f = fixture(1, 2);
    let photo = f.photos[0];
    let start = f.meta(photo);
    f.engine
        .submit_and_wait(Command::SetFlag {
            photo_id: photo,
            flag: Some(Flag::Rejected),
        })
        .unwrap();
    for keyword_id in &f.keywords {
        f.engine
            .submit_and_wait(Command::AddKeyword {
                photo_id: photo,
                keyword_id: *keyword_id,
            })
            .unwrap();
    }
    f.engine
        .submit_and_wait(Command::RemoveKeyword {
            photo_id: photo,
            keyword_id: f.keywords[0],
        })
        .unwrap();
    assert_eq!(f.meta(photo).keyword_ids, vec![f.keywords[1]]);
    f.engine.undo().unwrap();
    assert_eq!(
        f.meta(photo).keyword_ids,
        f.keywords,
        "the removed keyword is back, in its place"
    );
    assert_eq!(f.meta(photo).keyword_paths.len(), 2, "with its path");
    f.engine.undo().unwrap();
    f.engine.undo().unwrap();
    assert!(f.meta(photo).keyword_ids.is_empty());
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo), start);
}

#[test]
fn a_batch_is_one_step_that_undoes_every_photo_it_touched() {
    let f = fixture(3, 0);
    f.rate(f.photos[0], 2);
    f.events.drain();
    let batch = Command::Batch {
        commands: f
            .photos
            .iter()
            .map(|p| Command::SetRating {
                photo_id: *p,
                rating: 5,
            })
            .collect(),
    };
    f.engine.submit_and_wait(batch).unwrap();
    let steps: Vec<_> = f
        .events
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            Event::HistoryChanged(state) => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(steps.len(), 1, "one step for the whole batch");
    let label = steps[0].undo.unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::Rating, 3));

    f.engine.undo().unwrap();
    assert_eq!(
        f.meta(f.photos[0]).rating,
        Some(2),
        "back to what it was before the batch"
    );
    assert_eq!(f.meta(f.photos[1]).rating, None);
    assert_eq!(f.meta(f.photos[2]).rating, None);
    f.engine.redo().unwrap();
    assert!(f.photos.iter().all(|p| f.meta(*p).rating == Some(5)));
}

#[test]
fn a_batch_of_several_kinds_is_labelled_a_batch() {
    let f = fixture(2, 0);
    f.engine
        .submit_and_wait(Command::Batch {
            commands: vec![
                Command::SetRating {
                    photo_id: f.photos[0],
                    rating: 1,
                },
                Command::SetFlag {
                    photo_id: f.photos[1],
                    flag: Some(Flag::Picked),
                },
            ],
        })
        .unwrap();
    let events = f.events.drain();
    let state = events
        .iter()
        .rev()
        .find_map(|e| match e {
            Event::HistoryChanged(s) => Some(*s),
            _ => None,
        })
        .unwrap();
    assert_eq!(state.undo.unwrap().kind, LabelKind::Batch);
}

#[test]
fn a_batch_is_all_or_nothing() {
    let f = fixture(2, 0);
    let gone = PhotoId::random();
    let failed = f.engine.submit_and_wait(Command::Batch {
        commands: vec![
            Command::SetRating {
                photo_id: f.photos[0],
                rating: 3,
            },
            Command::SetRating {
                photo_id: gone,
                rating: 3,
            },
        ],
    });
    assert!(matches!(failed, Err(EngineError::NotFound { .. })));
    assert_eq!(
        f.meta(f.photos[0]).rating,
        None,
        "the first edit was taken back"
    );
    assert_eq!(
        f.engine.undo().unwrap(),
        Outcome::Nothing,
        "and left no step"
    );

    let refused = f.engine.submit_and_wait(Command::Batch {
        commands: vec![
            Command::SetRating {
                photo_id: f.photos[0],
                rating: 3,
            },
            Command::Rebuild,
        ],
    });
    assert!(matches!(refused, Err(EngineError::InvalidCommand(_))));
    assert_eq!(f.meta(f.photos[0]).rating, None);
}

#[test]
fn a_step_about_a_photo_that_has_gone_is_dropped_and_the_rest_of_the_history_stays() {
    let f = fixture(2, 0);
    f.rate(f.photos[0], 1);
    f.rate(f.photos[1], 2);
    std::fs::remove_file(f.engine.workspace().photo_path(&f.photos[0])).unwrap();

    f.engine.undo().unwrap(); // photo 1's rating
    assert_eq!(f.meta(f.photos[1]).rating, None);
    let dropped = f.engine.undo();
    assert!(matches!(dropped, Err(EngineError::NotFound { .. })));
    assert_eq!(
        f.engine.undo().unwrap(),
        Outcome::Nothing,
        "the step is gone"
    );
    f.engine.redo().unwrap();
    assert_eq!(
        f.meta(f.photos[1]).rating,
        Some(2),
        "what was undone can still be redone"
    );
}

#[test]
fn the_engine_says_what_undo_and_redo_would_do_and_what_was_touched() {
    let f = fixture(1, 0);
    let photo = f.photos[0];
    f.rate(photo, 3);
    f.engine.undo().unwrap();
    let events = f.events.drain();
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::HistoryChanged(s) => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(states.len(), 2);
    assert!(states[0].undo.is_some() && states[0].redo.is_none());
    assert!(states[1].undo.is_none() && states[1].redo.is_some());
    assert!(events.contains(&Event::HistoryApplied {
        redo: false,
        photos: vec![photo]
    }));
}

/// The whole state the history is about, for one photo.
#[derive(Clone, PartialEq, Debug, Default)]
struct Model {
    rating: Option<u8>,
    flag: Option<Flag>,
    keywords: Vec<KeywordId>,
}

type World = HashMap<PhotoId, Model>;

fn edit(world: &mut World, command: &Command) {
    match command {
        Command::SetRating { photo_id, rating } => {
            world.get_mut(photo_id).unwrap().rating = Some(*rating)
        }
        Command::SetFlag { photo_id, flag } => world.get_mut(photo_id).unwrap().flag = *flag,
        Command::AddKeyword {
            photo_id,
            keyword_id,
        } => {
            let keywords = &mut world.get_mut(photo_id).unwrap().keywords;
            if !keywords.contains(keyword_id) {
                keywords.push(*keyword_id);
            }
        }
        Command::RemoveKeyword {
            photo_id,
            keyword_id,
        } => world
            .get_mut(photo_id)
            .unwrap()
            .keywords
            .retain(|k| k != keyword_id),
        _ => unreachable!("an edit"),
    }
}

/// Any sequence of edits, batches, undos and redos ends in the state that replaying the surviving edits
/// gives: a model keeps the whole world before every action and does what a person would expect.
#[test]
fn any_sequence_of_edits_undos_and_redos_agrees_with_a_model() {
    for seed in 0..12u64 {
        let f = fixture(4, 3);
        let mut random = rng(seed);
        let mut world: World = f.photos.iter().map(|p| (*p, Model::default())).collect();
        let (mut undo, mut redo): (Vec<World>, Vec<World>) = (Vec::new(), Vec::new());
        for step in 0..60 {
            let photo = f.photos[random.usize(..f.photos.len())];
            let keyword = f.keywords[random.usize(..f.keywords.len())];
            let one = |random: &mut fastrand::Rng, photo, keyword| match random.usize(..4) {
                0 => Command::SetRating {
                    photo_id: photo,
                    rating: random.u8(0..=5),
                },
                1 => Command::SetFlag {
                    photo_id: photo,
                    flag: [None, Some(Flag::Picked), Some(Flag::Rejected)][random.usize(..3)],
                },
                2 => Command::AddKeyword {
                    photo_id: photo,
                    keyword_id: keyword,
                },
                _ => Command::RemoveKeyword {
                    photo_id: photo,
                    keyword_id: keyword,
                },
            };
            match random.usize(..10) {
                0..=1 => {
                    f.engine.undo().unwrap();
                    if let Some(before) = undo.pop() {
                        redo.push(std::mem::replace(&mut world, before));
                    }
                }
                2 => {
                    f.engine.redo().unwrap();
                    if let Some(after) = redo.pop() {
                        undo.push(std::mem::replace(&mut world, after));
                    }
                }
                3 => {
                    let commands: Vec<Command> = (0..random.usize(1..5))
                        .map(|_| {
                            let p = f.photos[random.usize(..f.photos.len())];
                            let k = f.keywords[random.usize(..f.keywords.len())];
                            one(&mut random, p, k)
                        })
                        .collect();
                    let before = world.clone();
                    for command in &commands {
                        edit(&mut world, command);
                    }
                    f.engine
                        .submit_and_wait(Command::Batch { commands })
                        .unwrap();
                    if world != before {
                        undo.push(before);
                        redo.clear();
                    }
                }
                _ => {
                    let command = one(&mut random, photo, keyword);
                    let before = world.clone();
                    edit(&mut world, &command);
                    f.engine.submit_and_wait(command).unwrap();
                    if world != before {
                        undo.push(before);
                        redo.clear();
                    }
                }
            }
            for (id, expected) in &world {
                let meta = f.meta(*id);
                // A photo that was rated once and then undone is unset: the model does not tell 0 from
                // unset, and neither does what a person sees.
                assert_eq!(
                    meta.rating.unwrap_or(0),
                    expected.rating.unwrap_or(0),
                    "seed {seed}, step {step}"
                );
                assert_eq!(meta.flag, expected.flag, "seed {seed}, step {step}");
                let mut have = meta.keyword_ids.clone();
                let mut want = expected.keywords.clone();
                have.sort_by_key(|k| k.to_string());
                want.sort_by_key(|k| k.to_string());
                assert_eq!(have, want, "seed {seed}, step {step}");
            }
        }
    }
}

#[test]
fn a_colour_label_is_undone_and_redone_and_a_selection_is_one_step() {
    use auroraw_engine::ColourLabel;
    let f = fixture(3, 0);
    let label_of = |photo: PhotoId| f.meta(photo).label;
    let row_label = |photo: PhotoId| {
        f.engine
            .read_catalogue()
            .unwrap()
            .photo(&photo)
            .unwrap()
            .unwrap()
            .label
    };
    let set = |photo: PhotoId, label: Option<ColourLabel>| {
        f.engine
            .submit_and_wait(Command::SetLabel {
                photo_id: photo,
                label,
            })
            .unwrap();
    };

    set(f.photos[0], Some(ColourLabel::Red));
    assert_eq!(label_of(f.photos[0]).as_deref(), Some("Red"));
    assert_eq!(
        row_label(f.photos[0]).as_deref(),
        Some("Red"),
        "the catalogue follows"
    );
    f.engine.undo().unwrap();
    assert_eq!(label_of(f.photos[0]), None);
    assert_eq!(row_label(f.photos[0]), None);
    f.engine.redo().unwrap();
    assert_eq!(label_of(f.photos[0]).as_deref(), Some("Red"));

    // The same colour again is not a step; taking it off is.
    f.events.drain();
    set(f.photos[0], Some(ColourLabel::Red));
    assert!(
        f.events
            .drain()
            .into_iter()
            .all(|e| !matches!(e, Event::HistoryChanged(_))),
        "a same-value edit is not a step"
    );
    set(f.photos[0], None);
    assert_eq!(label_of(f.photos[0]), None);
    f.engine.undo().unwrap();
    assert_eq!(label_of(f.photos[0]).as_deref(), Some("Red"));

    // A selection: one step, all three go back together (one of them had another colour).
    set(f.photos[1], Some(ColourLabel::Blue));
    f.events.drain();
    f.engine
        .submit_and_wait(Command::Batch {
            commands: f
                .photos
                .iter()
                .map(|p| Command::SetLabel {
                    photo_id: *p,
                    label: Some(ColourLabel::Green),
                })
                .collect(),
        })
        .unwrap();
    let steps: Vec<_> = f
        .events
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            Event::HistoryChanged(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(steps.len(), 1);
    let label = steps[0].undo.unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::ColourLabel, 3));
    f.engine.undo().unwrap();
    assert_eq!(label_of(f.photos[0]).as_deref(), Some("Red"));
    assert_eq!(label_of(f.photos[1]).as_deref(), Some("Blue"));
    assert_eq!(label_of(f.photos[2]), None);

    // A rebuild from the sidecars gives the catalogue the same labels.
    f.engine.submit_and_wait(Command::Rebuild).unwrap();
    assert_eq!(row_label(f.photos[1]).as_deref(), Some("Blue"));
}

#[test]
fn a_label_another_program_wrote_goes_back_exactly() {
    use auroraw_engine::ColourLabel;
    let f = fixture(1, 0);
    // A label of our own making is a colour; one from elsewhere is kept as it was written.
    let mut sidecar = f
        .engine
        .workspace()
        .read_photo(&f.photos[0])
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    sidecar.meta.label = Some("Approved".into());
    f.engine.workspace().write_photo(&sidecar).unwrap();
    f.engine
        .submit_and_wait(Command::SetLabel {
            photo_id: f.photos[0],
            label: Some(ColourLabel::Purple),
        })
        .unwrap();
    assert_eq!(f.meta(f.photos[0]).label.as_deref(), Some("Purple"));
    f.engine.undo().unwrap();
    assert_eq!(f.meta(f.photos[0]).label.as_deref(), Some("Approved"));
}

// WP10, slice 1: the metadata panel (spec §5.7). `MetadataField` covers plain fields (a title,
// a copyright notice, ...), the two list fields (creator, persons, one name a line), and a
// custom field (no UI makes one yet, but the command and its undo already work for any name).

#[test]
fn a_metadata_field_is_undone_and_redone_with_its_own_label_and_a_same_value_edit_is_not_a_step() {
    use auroraw_engine::MetadataField;
    let f = fixture(1, 0);
    let photo = f.photos[0];
    let set = |value: &str| {
        f.engine
            .submit_and_wait(Command::SetMetadataField {
                photo_id: photo,
                field: MetadataField::Title,
                value: value.to_string(),
            })
            .unwrap();
    };
    assert_eq!(f.meta(photo).title, None);
    f.events.drain();
    set("Heron at dawn");
    assert_eq!(f.meta(photo).title.as_deref(), Some("Heron at dawn"));
    let changed = f
        .events
        .drain()
        .into_iter()
        .find_map(|e| match e {
            Event::HistoryChanged(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(changed.undo.unwrap().kind, LabelKind::MetaTitle);
    f.engine.undo().unwrap();
    assert_eq!(f.meta(photo).title, None, "unset, not an empty string");
    f.engine.redo().unwrap();
    assert_eq!(f.meta(photo).title.as_deref(), Some("Heron at dawn"));

    f.events.drain();
    set("Heron at dawn");
    assert!(
        f.events
            .drain()
            .into_iter()
            .all(|e| !matches!(e, Event::HistoryChanged(_))),
        "a same-value edit is not a step"
    );
    set("");
    assert_eq!(f.meta(photo).title, None, "an empty value clears it");
}

#[test]
fn the_two_list_fields_take_one_name_a_line() {
    use auroraw_engine::MetadataField;
    let f = fixture(1, 0);
    let photo = f.photos[0];
    f.engine
        .submit_and_wait(Command::SetMetadataField {
            photo_id: photo,
            field: MetadataField::Creator,
            value: "Marie Tremblay\nJean Roy\n".into(),
        })
        .unwrap();
    assert_eq!(
        f.meta(photo).creator,
        vec!["Marie Tremblay".to_string(), "Jean Roy".to_string()]
    );
    f.engine.undo().unwrap();
    assert!(f.meta(photo).creator.is_empty());
}

#[test]
fn a_custom_field_is_set_cleared_and_undone_under_one_shared_label() {
    use auroraw_engine::MetadataField;
    let f = fixture(1, 0);
    let photo = f.photos[0];
    let field = MetadataField::Custom("Model release".into());
    let set = |value: &str| {
        f.engine
            .submit_and_wait(Command::SetMetadataField {
                photo_id: photo,
                field: field.clone(),
                value: value.to_string(),
            })
            .unwrap();
    };
    assert!(f.meta(photo).custom.is_empty());
    f.events.drain();
    set("on file");
    assert_eq!(f.meta(photo).custom[0].name, "Model release");
    assert_eq!(f.meta(photo).custom[0].value, "on file");
    let changed = f
        .events
        .drain()
        .into_iter()
        .find_map(|e| match e {
            Event::HistoryChanged(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(changed.undo.unwrap().kind, LabelKind::MetaCustom);
    f.engine.undo().unwrap();
    assert!(f.meta(photo).custom.is_empty());
    f.engine.redo().unwrap();
    set("");
    assert!(
        f.meta(photo).custom.is_empty(),
        "an empty value removes it, not an empty entry"
    );
}

#[test]
fn a_batch_of_metadata_edits_across_a_selection_with_mixed_values_is_one_step() {
    use auroraw_engine::MetadataField;
    let f = fixture(3, 0);
    f.engine
        .submit_and_wait(Command::SetMetadataField {
            photo_id: f.photos[1],
            field: MetadataField::Caption,
            value: "An old caption".into(),
        })
        .unwrap();
    f.events.drain();

    f.engine
        .submit_and_wait(Command::Batch {
            commands: f
                .photos
                .iter()
                .map(|p| Command::SetMetadataField {
                    photo_id: *p,
                    field: MetadataField::Caption,
                    value: "A grey heron & its reflection".into(),
                })
                .collect(),
        })
        .unwrap();
    let steps: Vec<_> = f
        .events
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            Event::HistoryChanged(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(steps.len(), 1, "one step, whatever each photo started from");
    let label = steps[0].undo.unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::MetaCaption, 3));
    for photo in &f.photos {
        assert_eq!(
            f.meta(*photo).caption.as_deref(),
            Some("A grey heron & its reflection")
        );
    }

    f.engine.undo().unwrap();
    assert_eq!(f.meta(f.photos[0]).caption, None, "back to unset");
    assert_eq!(
        f.meta(f.photos[1]).caption.as_deref(),
        Some("An old caption"),
        "back to what it was, not to unset"
    );
    assert_eq!(f.meta(f.photos[2]).caption, None);
}
