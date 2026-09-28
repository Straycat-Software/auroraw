// SPDX-License-Identifier: GPL-3.0-or-later
//! The application core: owns the workspace and the catalogue, is their only writer, and is the
//! only thing the interface talks to (architecture §3.2, §4). Runs without a window (`cli`
//! drives it directly; WP4 and later work packages add the sources, import and pipeline this
//! coordinates).
//!
//! [`Engine::create`] or [`Engine::open`] starts a coordinator thread that owns the workspace and
//! the catalogue's single writable connection (architecture §4.2), and hands back an [`Engine`]
//! handle (cheap to clone, safe to share across threads: [`Engine::submit`] and
//! [`Engine::submit_and_wait`] just send a [`Command`] down a channel) and an [`EventReceiver`]
//! that reports what happened, in order (architecture §4.3). Reads bypass the coordinator
//! entirely: [`Engine::read_catalogue`] opens a fresh connection (SQLite's WAL mode, many readers
//! and one writer, is exactly this).
//!
//! The coordinator thread stops when every [`Engine`] handle for it has been dropped.

mod command;
mod coordinator;
mod duplicates_api;
mod error;
mod event;
mod history;
mod import_flow;
mod import_job;
mod index_job;
mod job;
pub mod paths;
mod photo_paths;
mod reconcile_apply;
mod refresh;
mod remove_job;
mod series_detect;
mod similar_api;
mod sources_api;
mod technical_details;
mod thumbnails;
mod viewer;
mod workspaces;

pub use auroraw_catalogue::SourceCounts;
pub use auroraw_format::sidecar::{ColourLabel, Flag};
pub use auroraw_imaging::{Aids, MaskKind};
pub use auroraw_import::{ItemOutcome, MetadataTemplate, PairRule, Profile};
pub use auroraw_types::KeywordId;
pub use command::{Command, MetadataField};
pub use coordinator::Outcome;
pub use duplicates_api::{DuplicatePhoto, LocationRef, format_report as duplicates_report};
pub use error::{EngineError, Result};
pub use event::Event;
pub use history::{
    Change, HistoryState, KeywordDelta, KeywordSet, Label, LabelKind, SeriesAction,
    VocabularyAction,
};
pub use import_flow::{
    DestinationKind, ImportRequest, ImportSourceInfo, ImportStarted, VolumeInfo,
};
pub use import_job::Registration;
pub use job::JobId;
pub use similar_api::{SimilarPhoto, SimilarQuery};
pub use sources_api::{AddPlan, AddSourceRequest, AddedSource, SourceInfo, SourceKind};
pub use technical_details::TechnicalDetails;
pub use thumbnails::ThumbnailService;
pub use viewer::{DEFAULT_CAPACITY as PREVIEW_CACHE, PreviewService};
pub use workspaces::{KnownWorkspace, LocalDirs, OpenedWorkspace};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use auroraw_catalogue::Catalogue;
use auroraw_workspace::Workspace;

use coordinator::{Coordinator, Inbound};

/// A handle to a running engine. Cheap to clone; every clone can submit commands from its own
/// thread (testing strategy §8: the single-writer coordinator is what serializes them, not this
/// handle).
#[derive(Clone)]
pub struct Engine {
    inbound: mpsc::Sender<Inbound>,
    workspace: Arc<Workspace>,
    catalogue_path: PathBuf,
    /// Dropped with the last handle, which stops the coordinator.
    _stop: Arc<StopWhenDropped>,
}

/// Tells the coordinator to stop when the last [`Engine`] handle goes away.
struct StopWhenDropped(mpsc::Sender<Inbound>);

impl Drop for StopWhenDropped {
    fn drop(&mut self) {
        let _ = self.0.send(Inbound::Stop);
    }
}

/// The engine's event stream. Drain it rather than reacting one event at a time (architecture
/// §4.3: events are meant to be batched).
pub struct EventReceiver(mpsc::Receiver<Event>);

impl EventReceiver {
    /// Blocks for the next event, or `None` once the coordinator has stopped and every event is
    /// delivered.
    pub fn recv(&self) -> Option<Event> {
        self.0.recv().ok()
    }

    /// The next event if one is already waiting, without blocking.
    pub fn try_recv(&self) -> Option<Event> {
        self.0.try_recv().ok()
    }

    /// Blocks for up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.0.recv_timeout(timeout).ok()
    }

    /// Every event waiting right now, in order, without blocking: the batch-drain architecture
    /// §4.3 describes.
    pub fn drain(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.0.try_recv() {
            events.push(event);
        }
        events
    }
}

impl Engine {
    /// The application version.
    pub fn version() -> &'static str {
        auroraw_types::app_version()
    }

    /// Creates a new workspace and catalogue and starts the engine over them.
    pub fn create(
        workspace_path: &Path,
        catalogue_path: &Path,
        catalogue_name: &str,
    ) -> Result<(Self, EventReceiver)> {
        let workspace = Workspace::create(workspace_path, catalogue_name)?;
        let catalogue = Catalogue::create(catalogue_path, workspace.workspace_id())?;
        Ok(Self::spawn(
            workspace,
            catalogue,
            catalogue_path.to_path_buf(),
        ))
    }

    /// Opens an existing workspace and catalogue. Refuses the pair if the catalogue does not
    /// index this workspace (note 001 §5.3): submit [`Command::Rebuild`] after opening the
    /// catalogue on its own (`Catalogue::open`) to recover from that, or rebuild through the CLI.
    pub fn open(workspace_path: &Path, catalogue_path: &Path) -> Result<(Self, EventReceiver)> {
        let workspace = Workspace::open(workspace_path)?;
        let catalogue = Catalogue::open(catalogue_path)?;
        if catalogue.workspace_id()? != workspace.workspace_id() {
            return Err(EngineError::WrongCatalogue);
        }
        Ok(Self::spawn(
            workspace,
            catalogue,
            catalogue_path.to_path_buf(),
        ))
    }

    fn spawn(
        workspace: Workspace,
        catalogue: Catalogue,
        catalogue_path: PathBuf,
    ) -> (Self, EventReceiver) {
        let workspace = Arc::new(workspace);
        let (inbound_tx, inbound_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let coordinator =
            Coordinator::new(workspace.clone(), catalogue, event_tx, inbound_tx.clone());
        std::thread::spawn(move || coordinator.run(inbound_rx));
        (
            Self {
                _stop: Arc::new(StopWhenDropped(inbound_tx.clone())),
                inbound: inbound_tx,
                workspace,
                catalogue_path,
            },
            EventReceiver(event_rx),
        )
    }

    /// Sends a command without waiting for it to apply. What happened arrives on the
    /// [`EventReceiver`], in order, as [`Event::Applied`] or [`Event::Failed`].
    pub fn submit(&self, command: Command) -> Result<()> {
        self.inbound
            .send(Inbound::Command {
                command,
                reply: None,
            })
            .map_err(|_| EngineError::Stopped)
    }

    /// Undoes the last action of the person's (D-096), waiting for it.
    pub fn undo(&self) -> Result<Outcome> {
        self.submit_and_wait(Command::Undo)
    }

    /// Redoes the action that was undone last, waiting for it.
    pub fn redo(&self) -> Result<Outcome> {
        self.submit_and_wait(Command::Redo)
    }

    /// Sends a command and blocks until the coordinator has applied it (or failed to), returning
    /// its outcome directly. Safe to call from several threads at once: each call blocks only its
    /// own caller, and the coordinator still applies commands one at a time, in the order it
    /// receives them from all callers combined.
    pub fn submit_and_wait(&self, command: Command) -> Result<Outcome> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.inbound
            .send(Inbound::Command {
                command,
                reply: Some(reply_tx),
            })
            .map_err(|_| EngineError::Stopped)?;
        reply_rx.recv().map_err(|_| EngineError::Stopped)?
    }

    /// The open workspace, for reading directly (its read methods take `&self` and need no
    /// coordination; only writes go through commands).
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// A fresh, independent connection to the catalogue, for queries (`list_recent`, `search`,
    /// ...). Safe alongside the coordinator's own writes: SQLite's WAL mode is many readers, one
    /// writer (architecture §4.2). Never write through the result of this: the coordinator is the
    /// catalogue's only writer, by convention this crate does not enforce at the type level.
    pub fn read_catalogue(&self) -> Result<Catalogue> {
        Ok(Catalogue::open(&self.catalogue_path)?)
    }

    /// `field` of each of `photos`' own sidecars, in the same order; a photo whose sidecar cannot
    /// be read (or is of a newer schema) is left out. What the metadata panel reads to show the
    /// selection's current value and whether it is "Multiple values" (spec §5.7; WP10, slice 1) —
    /// most fields are not catalogue columns (nothing queries by them yet), so this goes straight
    /// to the sidecars, the same read `Workspace::read_photo`'s own doc comment already says needs
    /// no coordination.
    pub fn metadata_field_of(
        &self,
        photos: &[auroraw_types::PhotoId],
        field: &MetadataField,
    ) -> Vec<String> {
        photos
            .iter()
            .filter_map(|id| self.workspace.read_photo(id).ok().flatten()?.current())
            .map(|photo| field.get(&photo.meta))
            .collect()
    }

    /// Starts a [`ThumbnailService`] for this engine's workspace and catalogue, generating and
    /// caching thumbnails at `previews_path` (D-075) on `workers` background threads. This crate
    /// resolves no cache directory itself; the caller decides where `previews_path` lives.
    pub fn start_thumbnails(
        &self,
        previews_path: &Path,
        workers: usize,
    ) -> Result<ThumbnailService> {
        ThumbnailService::start(
            self.workspace.clone(),
            self.catalogue_path.clone(),
            previews_path.to_path_buf(),
            workers,
            self.inbound.clone(),
        )
    }

    /// Starts a [`PreviewService`] for the image view: pictures of the photos at a size worth looking at,
    /// kept in memory (`PREVIEW_CACHE` of them), made on `workers` background threads.
    pub fn start_previews(&self, workers: usize) -> Result<PreviewService> {
        PreviewService::start(
            self.workspace.clone(),
            self.catalogue_path.clone(),
            workers,
            PREVIEW_CACHE,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use auroraw_format::sidecar::PhotoSidecar;
    use auroraw_testkit::temp_dir;
    use auroraw_types::{KeywordId, PhotoId, SourceId};

    use super::*;
    use crate::command::Command;

    fn new_engine() -> (Engine, EventReceiver, auroraw_testkit::TempDir) {
        let dir = temp_dir();
        let (engine, events) = Engine::create(
            &dir.path().join("Main"),
            &dir.path().join("main.sqlite"),
            "Main",
        )
        .unwrap();
        (engine, events, dir)
    }

    fn wait_for(
        events: &EventReceiver,
        mut matches: impl FnMut(&Event) -> bool,
        timeout: Duration,
    ) -> Event {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let event = events
                .recv_timeout(remaining)
                .expect("event before timeout");
            if matches(&event) {
                return event;
            }
        }
    }

    #[test]
    fn version_is_not_empty() {
        assert!(!Engine::version().is_empty());
    }

    #[test]
    fn opening_a_catalogue_for_the_wrong_workspace_is_refused() {
        let dir = temp_dir();
        let (_engine, _events) =
            Engine::create(&dir.path().join("A"), &dir.path().join("a.sqlite"), "A").unwrap();
        let other = Workspace::create(&dir.path().join("B"), "B").unwrap();
        assert!(matches!(
            Engine::open(other.root(), &dir.path().join("a.sqlite")),
            Err(EngineError::WrongCatalogue)
        ));
    }

    #[test]
    fn set_rating_updates_the_sidecar_and_the_catalogue() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        engine
            .submit_and_wait(Command::SetRating {
                photo_id,
                rating: 4,
            })
            .unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::PhotoChanged(id) if *id == photo_id),
            Duration::from_secs(5),
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().expect("indexed");
        assert_eq!(row.rating, 4);
        assert_eq!(row.effective_rating, 4);

        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(sidecar.meta.rating, Some(4));
    }

    #[test]
    fn set_rating_on_an_unknown_photo_is_an_error() {
        let (engine, _events, _dir) = new_engine();
        let err = engine
            .submit_and_wait(Command::SetRating {
                photo_id: PhotoId::random(),
                rating: 3,
            })
            .unwrap_err();
        assert!(matches!(err, EngineError::NotFound { kind: "photo", .. }));
    }

    #[test]
    fn add_and_remove_keyword_round_trips_through_the_catalogue() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let Outcome::KeywordCreated(keyword_id) = engine
            .submit_and_wait(Command::CreateKeyword {
                name: "Heron".into(),
                parent: None,
                id: None,
            })
            .unwrap()
        else {
            panic!("expected KeywordCreated");
        };

        engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(sidecar.meta.keyword_ids, vec![keyword_id]);
        assert_eq!(sidecar.meta.keyword_paths, vec!["Heron".to_string()]);

        let catalogue = engine.read_catalogue().unwrap();
        assert_eq!(
            catalogue.photos_with_keywords(&[keyword_id]).unwrap(),
            vec![photo_id]
        );

        engine
            .submit_and_wait(Command::RemoveKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert!(sidecar.meta.keyword_ids.is_empty());
        let catalogue = engine.read_catalogue().unwrap();
        assert!(
            catalogue
                .photos_with_keywords(&[keyword_id])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn adding_an_unknown_keyword_is_an_error() {
        let (engine, _events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();

        let err = engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id: KeywordId::random(),
            })
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::NotFound {
                kind: "keyword",
                ..
            }
        ));
    }

    #[test]
    fn renaming_a_keyword_refreshes_every_sidecar_that_carries_it() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let Outcome::KeywordCreated(keyword_id) = engine
            .submit_and_wait(Command::CreateKeyword {
                name: "Heron".into(),
                parent: None,
                id: None,
            })
            .unwrap()
        else {
            panic!("expected KeywordCreated");
        };
        engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        events.drain();

        let outcome = engine
            .submit_and_wait(Command::RenameKeyword {
                keyword_id,
                new_name: "Great Blue Heron".into(),
            })
            .unwrap();
        let Outcome::RenameStarted { job, affected } = outcome else {
            panic!("expected RenameStarted")
        };
        assert_eq!(affected, 1);

        wait_for(
            &events,
            |e| matches!(e, Event::JobFinished(id) if *id == job),
            Duration::from_secs(5),
        );

        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(
            sidecar.meta.keyword_paths,
            vec!["Great Blue Heron".to_string()]
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().unwrap();
        assert!(row.title.is_none()); // sanity: rename did not disturb unrelated fields
    }

    #[test]
    fn reconcile_picks_up_a_sidecar_edited_outside_the_engine() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let mut sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        sidecar.meta.rating = Some(5);
        engine.workspace().write_photo(&sidecar).unwrap();

        engine.submit_and_wait(Command::Reconcile).unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::ReconcileFinished { .. }),
            Duration::from_secs(5),
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().unwrap();
        assert_eq!(row.rating, 5);
    }

    #[test]
    fn commands_from_several_threads_are_applied_in_a_single_recorded_order() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let threads: Vec<_> = (0..4u8)
            .map(|i| {
                let engine = engine.clone();
                std::thread::spawn(move || {
                    engine
                        .submit_and_wait(Command::SetRating {
                            photo_id,
                            rating: i % 6,
                        })
                        .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }

        let mut applied = Vec::new();
        for event in events.drain() {
            if let Event::Applied(Command::SetRating { rating, .. }) = event {
                applied.push(rating);
            }
        }
        assert_eq!(applied.len(), 4);

        // The coordinator applies commands one at a time (architecture §4.2): whichever
        // `SetRating` it recorded last is necessarily the one whose value survives.
        let catalogue = engine.read_catalogue().unwrap();
        let concurrent_rating = catalogue.photo(&photo_id).unwrap().unwrap().rating;
        assert_eq!(concurrent_rating, *applied.last().unwrap());
    }

    fn add_source(engine: &Engine, root: &std::path::Path) -> auroraw_types::SourceId {
        let Outcome::SourceAdded(id) = engine
            .submit_and_wait(Command::AddSource {
                name: "Test source".into(),
                root: root.to_path_buf(),
                kind: auroraw_sources::filesystem::LOCAL_FOLDER.into(),
            })
            .unwrap()
        else {
            panic!("expected SourceAdded");
        };
        id
    }

    #[test]
    fn add_source_registers_it_in_the_workspace_and_the_catalogue() {
        let (engine, _events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();

        let id = add_source(&engine, &source_root);

        let sources = engine
            .workspace()
            .read_sources()
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(sources.sources.len(), 1);
        assert_eq!(sources.sources[0].id, id);

        let row = engine
            .read_catalogue()
            .unwrap()
            .source(&id)
            .unwrap()
            .unwrap();
        assert_eq!(row.name, "Test source");
        assert_eq!(row.kind, auroraw_sources::filesystem::LOCAL_FOLDER);
    }

    #[test]
    fn scanning_a_source_offers_a_new_file_and_confirming_it_adds_a_photo() {
        let (engine, events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("a.raw"), b"first photo").unwrap();
        let source_id = add_source(&engine, &source_root);
        events.drain();

        let Outcome::Scanned {
            reachable,
            new,
            confirmed,
            ..
        } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!("expected Scanned");
        };
        assert!(reachable);
        assert_eq!(new, vec!["a.raw".to_string()]);
        assert_eq!(confirmed, 0);

        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!("expected PhotosAdded");
        };
        assert_eq!(added.len(), 1);
        let row = engine
            .read_catalogue()
            .unwrap()
            .photo(&added[0])
            .unwrap()
            .unwrap();
        assert_eq!(row.path.as_deref(), Some("a.raw"));
        assert_eq!(row.source_id, Some(source_id));

        // Rescanning now confirms the photo instead of offering it again.
        let Outcome::Scanned { confirmed, new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!("expected Scanned");
        };
        assert_eq!(confirmed, 1);
        assert!(new.is_empty());
    }

    #[test]
    fn scanning_detects_an_original_changed_at_its_known_path() {
        let (engine, _events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("a.raw"), b"original bytes").unwrap();
        let source_id = add_source(&engine, &source_root);
        let Outcome::Scanned { new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!()
        };

        std::fs::write(source_root.join("a.raw"), b"edited outside auroraw, still").unwrap();
        let Outcome::Scanned { changed, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(changed, 1);
        assert!(
            engine
                .read_catalogue()
                .unwrap()
                .photo(&added[0])
                .unwrap()
                .unwrap()
                .original_changed
        );
    }

    #[test]
    fn scanning_relinks_a_renamed_file_silently() {
        let (engine, _events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("old.raw"), b"same bytes").unwrap();
        let source_id = add_source(&engine, &source_root);
        let Outcome::Scanned { new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!()
        };

        std::fs::rename(source_root.join("old.raw"), source_root.join("new.raw")).unwrap();
        let Outcome::Scanned { relinked, new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(relinked, 1);
        assert!(
            new.is_empty(),
            "the renamed file is a relink, not a new file"
        );

        let row = engine
            .read_catalogue()
            .unwrap()
            .photo(&added[0])
            .unwrap()
            .unwrap();
        assert_eq!(row.path.as_deref(), Some("new.raw"));
    }

    #[test]
    fn a_deleted_file_is_reported_missing_and_the_photo_is_kept() {
        let (engine, _events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("a.raw"), b"bytes").unwrap();
        let source_id = add_source(&engine, &source_root);
        let Outcome::Scanned { new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!()
        };

        std::fs::remove_file(source_root.join("a.raw")).unwrap();
        let Outcome::Scanned { missing, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(missing, 1);

        let row = engine
            .read_catalogue()
            .unwrap()
            .photo(&added[0])
            .unwrap()
            .unwrap();
        assert!(row.original_missing, "kept, not removed (D-019, D-031)");
    }

    #[test]
    fn an_unplugged_source_scans_as_unreachable_and_replugging_finds_everything_intact() {
        let (engine, _events, dir) = new_engine();
        let source_root = dir.path().join("Card");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("a.raw"), b"bytes").unwrap();
        let source_id = add_source(&engine, &source_root);
        let Outcome::Scanned { new, .. } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        let Outcome::PhotosAdded(added) = engine
            .submit_and_wait(Command::AddNewPhotos {
                source_id,
                paths: new,
            })
            .unwrap()
        else {
            panic!()
        };

        // Unplugged: the mount point itself is gone.
        let elsewhere = dir.path().join("Card-unplugged");
        std::fs::rename(&source_root, &elsewhere).unwrap();
        let Outcome::Scanned {
            reachable, missing, ..
        } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        assert!(
            !reachable,
            "an unreachable source is not the same as every file in it going missing"
        );
        assert_eq!(missing, 0);
        assert!(
            !engine
                .read_catalogue()
                .unwrap()
                .photo(&added[0])
                .unwrap()
                .unwrap()
                .original_missing
        );

        // Replugged: back exactly as it was.
        std::fs::rename(&elsewhere, &source_root).unwrap();
        let Outcome::Scanned {
            reachable,
            confirmed,
            missing,
            ..
        } = engine
            .submit_and_wait(Command::ScanSource { source_id })
            .unwrap()
        else {
            panic!()
        };
        assert!(reachable);
        assert_eq!(confirmed, 1);
        assert_eq!(missing, 0);
    }

    fn simple_profile(template: &str) -> auroraw_import::Profile {
        auroraw_import::Profile {
            name: "Test".into(),
            destination_template: template.into(),
            backup_templates: Vec::new(),
            pair_rule: auroraw_import::PairRule::Both,
            metadata_template: auroraw_import::MetadataTemplate::default(),
        }
    }

    fn import_and_wait(
        engine: &Engine,
        events: &EventReceiver,
        card: &std::path::Path,
        archive: &std::path::Path,
        archive_id: SourceId,
        profile: auroraw_import::Profile,
        state_path: std::path::PathBuf,
    ) -> Event {
        let Outcome::ImportStarted { job } = engine
            .submit_and_wait(Command::Import {
                source_root: card.to_path_buf(),
                destination_root: archive.to_path_buf(),
                registration: Some(Registration {
                    source_id: archive_id,
                    prefix: String::new(),
                }),
                profile,
                backup_roots: Vec::new(),
                state_path,
            })
            .unwrap()
        else {
            panic!("expected ImportStarted");
        };
        wait_for(
            events,
            |e| matches!(e, Event::ImportFinished { job: j, .. } if *j == job),
            Duration::from_secs(10),
        )
    }

    #[test]
    fn importing_copies_verifies_and_registers_every_file_leaving_the_card_untouched() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"photo a").unwrap();
        std::fs::write(card.join("b.raw"), b"photo b").unwrap();
        let archive = dir.path().join("Archive");
        std::fs::create_dir_all(&archive).unwrap();

        let archive_id = add_source(&engine, &archive);
        events.drain();

        let finished = import_and_wait(
            &engine,
            &events,
            &card,
            &archive,
            archive_id,
            simple_profile("{original}.{ext}"),
            dir.path().join("job.json"),
        );
        assert!(matches!(
            finished,
            Event::ImportFinished {
                copied: 2,
                skipped: 0,
                failed: 0,
                ..
            }
        ));

        assert_eq!(
            std::fs::read(card.join("a.raw")).unwrap(),
            b"photo a",
            "the card is never modified (D-031)"
        );
        assert_eq!(std::fs::read(card.join("b.raw")).unwrap(), b"photo b");
        assert_eq!(std::fs::read(archive.join("a.raw")).unwrap(), b"photo a");
        assert_eq!(std::fs::read(archive.join("b.raw")).unwrap(), b"photo b");

        // Every imported photo is findable by its archived file's own fingerprint and points at
        // the archive, not the card, with its hash already known.
        let catalogue = engine.read_catalogue().unwrap();
        for name in ["a.raw", "b.raw"] {
            let bytes = std::fs::read(archive.join(name)).unwrap();
            let (_, fingerprint) =
                auroraw_format::fingerprint::fingerprint(&mut std::io::Cursor::new(bytes)).unwrap();
            let matches = catalogue.find_by_fingerprint(&fingerprint).unwrap();
            assert_eq!(matches.len(), 1, "{name} registered exactly once");
            assert_eq!(matches[0].source_id, Some(archive_id));
            assert!(
                matches[0].hash.is_some(),
                "the hash is known at import time"
            );
        }
    }

    #[test]
    fn a_second_import_of_the_same_card_copies_nothing() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"only photo").unwrap();
        let archive = dir.path().join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        let archive_id = add_source(&engine, &archive);
        events.drain();

        let first = import_and_wait(
            &engine,
            &events,
            &card,
            &archive,
            archive_id,
            simple_profile("{original}.{ext}"),
            dir.path().join("job1.json"),
        );
        assert!(matches!(first, Event::ImportFinished { copied: 1, .. }));

        // A second, independent import job (a fresh state file, as if the card were pulled and
        // reinserted): the file is skipped on the catalogue's own hash, not because this job
        // remembers doing it before.
        let second = import_and_wait(
            &engine,
            &events,
            &card,
            &archive,
            archive_id,
            simple_profile("{original}.{ext}"),
            dir.path().join("job2.json"),
        );
        assert!(matches!(
            second,
            Event::ImportFinished {
                copied: 0,
                skipped: 1,
                failed: 0,
                ..
            }
        ));
    }

    #[test]
    fn an_interrupted_import_resumes_only_the_unfinished_files() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"already done").unwrap();
        std::fs::write(card.join("b.raw"), b"not done yet").unwrap();
        let archive = dir.path().join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        let archive_id = add_source(&engine, &archive);
        events.drain();

        // Simulate an import that was interrupted right after `a.raw` landed: its own state file
        // already marks it done, as the job itself would have left it.
        let state_path = dir.path().join("job.json");
        let mut state = auroraw_import::ImportState::new();
        state.record("a.raw", auroraw_import::ItemOutcome::Copied);
        state.save(&state_path).unwrap();
        // `a.raw` is not actually in the archive or the catalogue yet in this test (the
        // interruption is simulated, not a real prior run): resuming must still leave it alone,
        // proving the decision is state-file-driven and not a fresh catalogue check.
        assert!(!archive.join("a.raw").exists());

        let finished = import_and_wait(
            &engine,
            &events,
            &card,
            &archive,
            archive_id,
            simple_profile("{original}.{ext}"),
            state_path,
        );
        assert!(
            matches!(
                finished,
                Event::ImportFinished {
                    copied: 2,
                    skipped: 0,
                    failed: 0,
                    ..
                }
            ),
            "the report covers the whole import, the file an earlier run settled included"
        );
        assert!(
            !archive.join("a.raw").exists(),
            "already-settled in the state file: not retried"
        );
        assert_eq!(
            std::fs::read(archive.join("b.raw")).unwrap(),
            b"not done yet"
        );
    }

    #[test]
    fn two_cameras_with_the_same_file_name_do_not_collide_at_the_destination() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(card.join("CameraA")).unwrap();
        std::fs::create_dir_all(card.join("CameraB")).unwrap();
        std::fs::write(card.join("CameraA/IMG_0001.raw"), b"from camera A").unwrap();
        std::fs::write(card.join("CameraB/IMG_0001.raw"), b"from camera B").unwrap();
        let archive = dir.path().join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        let archive_id = add_source(&engine, &archive);
        events.drain();

        let finished = import_and_wait(
            &engine,
            &events,
            &card,
            &archive,
            archive_id,
            simple_profile("{original}.{ext}"),
            dir.path().join("job.json"),
        );
        assert!(matches!(finished, Event::ImportFinished { copied: 2, .. }));

        assert_eq!(
            std::fs::read(archive.join("IMG_0001.raw")).unwrap(),
            b"from camera A"
        );
        assert_eq!(
            std::fs::read(archive.join("IMG_0001_2.raw")).unwrap(),
            b"from camera B",
            "the second file to land on the same name gets a unique suffix"
        );
    }

    #[test]
    fn importing_does_not_block_the_coordinators_command_queue() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        for i in 0..20 {
            std::fs::write(card.join(format!("{i}.raw")), format!("photo {i}")).unwrap();
        }
        let archive = dir.path().join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        let archive_id = add_source(&engine, &archive);
        events.drain();

        let Outcome::ImportStarted { job } = engine
            .submit_and_wait(Command::Import {
                source_root: card.clone(),
                destination_root: archive.clone(),
                registration: Some(Registration {
                    source_id: archive_id,
                    prefix: String::new(),
                }),
                profile: simple_profile("{original}.{ext}"),
                backup_roots: Vec::new(),
                state_path: dir.path().join("job.json"),
            })
            .unwrap()
        else {
            panic!("expected ImportStarted");
        };
        // The import is still running (or, at worst, racing to finish) in the background: an
        // unrelated command submitted right after still gets a prompt answer from the
        // coordinator, which never waited for the import job itself.
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        // (Not a rebuild: that replaces the database file, which Windows refuses while the import
        // job has its own connection to it open.)
        engine.submit_and_wait(Command::Reconcile).unwrap();

        wait_for(
            &events,
            |e| matches!(e, Event::ImportFinished { job: j, .. } if *j == job),
            Duration::from_secs(10),
        );
    }

    fn request(dir: &std::path::Path, card: &std::path::Path) -> ImportRequest {
        ImportRequest {
            source_root: card.to_path_buf(),
            destination_root: dir.join("Archive"),
            profile: simple_profile("{original}.{ext}"),
            backup_root: None,
            state_dir: dir.join("state"),
            add_destination_as_source: true,
        }
    }

    fn finished(events: &EventReceiver, job: JobId) -> Event {
        wait_for(
            events,
            |e| matches!(e, Event::ImportFinished { job: j, .. } if *j == job),
            Duration::from_secs(10),
        )
    }

    #[test]
    fn importing_by_folder_registers_the_archive_once_and_never_the_card() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"photo a").unwrap();

        let job = engine.import(request(dir.path(), &card)).unwrap().job;
        assert!(matches!(
            finished(&events, job),
            Event::ImportFinished { copied: 1, .. }
        ));
        assert_eq!(
            std::fs::read(dir.path().join("Archive/a.raw")).unwrap(),
            b"photo a"
        );
        let sources = engine.read_catalogue().unwrap().list_sources().unwrap();
        assert_eq!(
            sources.len(),
            1,
            "the archive: the card is never registered"
        );

        std::fs::write(card.join("b.raw"), b"photo b").unwrap();
        let job = engine.import(request(dir.path(), &card)).unwrap().job;
        assert!(matches!(
            finished(&events, job),
            Event::ImportFinished {
                copied: 1,
                skipped: 1,
                ..
            }
        ));
        assert_eq!(
            engine
                .read_catalogue()
                .unwrap()
                .list_sources()
                .unwrap()
                .len(),
            1,
            "the same source again, not a second one"
        );
    }

    #[test]
    fn a_reformatted_card_reusing_file_names_is_not_mistaken_for_the_last_one() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("IMG_0001.raw"), b"first shoot").unwrap();
        let job = engine.import(request(dir.path(), &card)).unwrap().job;
        finished(&events, job);
        assert!(
            std::fs::read_dir(dir.path().join("state"))
                .unwrap()
                .next()
                .is_none(),
            "a clean import leaves nothing to resume"
        );

        // The card was reformatted and the camera started again at 0001.
        std::fs::write(card.join("IMG_0001.raw"), b"second shoot").unwrap();
        let job = engine.import(request(dir.path(), &card)).unwrap().job;
        assert!(matches!(
            finished(&events, job),
            Event::ImportFinished {
                copied: 1,
                skipped: 0,
                ..
            }
        ));
        assert_eq!(
            std::fs::read(dir.path().join("Archive/IMG_0001.raw")).unwrap(),
            b"first shoot"
        );
        assert_eq!(
            std::fs::read(dir.path().join("Archive/IMG_0001_2.raw")).unwrap(),
            b"second shoot"
        );
    }

    #[test]
    fn a_backup_root_receives_a_verified_second_copy() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"photo a").unwrap();
        let mut req = request(dir.path(), &card);
        req.backup_root = Some(dir.path().join("Backup"));
        let job = engine.import(req).unwrap().job;
        assert!(matches!(
            finished(&events, job),
            Event::ImportFinished {
                copied: 1,
                failed: 0,
                ..
            }
        ));
        assert_eq!(
            std::fs::read(dir.path().join("Backup/a.raw")).unwrap(),
            b"photo a"
        );
    }

    #[test]
    fn an_import_that_cannot_make_sense_is_refused_before_anything_is_registered() {
        let (engine, _events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();

        assert!(
            engine
                .import(request(dir.path(), &dir.path().join("Nowhere")))
                .is_err()
        );
        let mut into_itself = request(dir.path(), &card);
        into_itself.destination_root = card.clone();
        assert!(engine.import(into_itself).is_err());
        assert!(
            engine
                .read_catalogue()
                .unwrap()
                .list_sources()
                .unwrap()
                .is_empty(),
            "a refused import registers nothing"
        );
    }

    #[test]
    fn a_template_with_date_folders_never_escapes_the_archive_for_a_photo_with_no_date() {
        let (engine, events, dir) = new_engine();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        // No readable capture time: `{year}` and `{date}` render as nothing, which used to make
        // the destination an absolute path.
        std::fs::write(card.join("a.raw"), b"photo a").unwrap();
        let mut req = request(dir.path(), &card);
        req.profile = simple_profile("{year}/{date}/{original}.{ext}");
        let job = engine.import(req).unwrap().job;
        assert!(matches!(
            finished(&events, job),
            Event::ImportFinished {
                copied: 1,
                failed: 0,
                ..
            }
        ));
        assert_eq!(
            std::fs::read(dir.path().join("Archive/a.raw")).unwrap(),
            b"photo a"
        );
    }
}
