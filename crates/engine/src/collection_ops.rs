// SPDX-License-Identifier: GPL-3.0-or-later
//! Manual collections in the coordinator (WP10, slice 3): the edits a person makes (make, rename, move, delete a
//! branch, add photos, take photos out: each a step of the history, a [`Change::Collections`] holding the state
//! files before and after, so that do, undo and redo are one operation, [`Coordinator::write_collections`]), and
//! the leaving of a photo whose source was removed. Only `manual` collections are ours: a smart or client-selection
//! file is read by nothing here and an edit that names one is refused.

use std::collections::HashSet;

use auroraw_catalogue::{CollectionRow, SidecarStat};
use auroraw_format::state::{Collection, collection_kind};
use auroraw_types::{CollectionId, MemberRef, PhotoId, Timestamp};

use super::{Coordinator, Outcome, stat_from};
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::history::{Change, CollectionAction, CollectionDelta, Direction};

fn checked_collection_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EngineError::CollectionName);
    }
    Ok(name.to_string())
}

/// `id` and every collection inside it, the collection itself first.
fn branch_of(rows: &[CollectionRow], id: CollectionId) -> Vec<CollectionId> {
    let mut branch = vec![id];
    let mut i = 0;
    while i < branch.len() {
        let parent = branch[i];
        branch.extend(
            rows.iter()
                .filter(|r| r.parent == Some(parent))
                .map(|r| r.id),
        );
        i += 1;
    }
    branch
}

/// Two siblings do not share a name (whatever the case); `except` is the collection being renamed or moved.
fn ensure_name_is_free(
    rows: &[CollectionRow],
    name: &str,
    parent: Option<CollectionId>,
    except: Option<CollectionId>,
) -> Result<()> {
    let lower = name.to_lowercase();
    if rows
        .iter()
        .any(|r| r.parent == parent && Some(r.id) != except && r.name.to_lowercase() == lower)
    {
        return Err(EngineError::CollectionNameTaken(name.to_string()));
    }
    Ok(())
}

fn not_found(kind: &'static str, id: impl ToString) -> EngineError {
    EngineError::NotFound {
        kind,
        id: id.to_string(),
    }
}

/// `photos` without repeats, in their order.
fn distinct(photos: Vec<PhotoId>) -> Vec<PhotoId> {
    let mut seen = HashSet::new();
    photos.into_iter().filter(|p| seen.insert(*p)).collect()
}

impl Coordinator {
    /// A manual collection's state file, as it is now.
    fn read_collection_state(&self, id: &CollectionId) -> Result<Collection> {
        let collection = self
            .workspace
            .read_collection(id)?
            .and_then(|loaded| loaded.current())
            .ok_or_else(|| not_found("collection", id))?;
        if collection.kind != collection_kind::MANUAL {
            return Err(not_found("manual collection", id));
        }
        Ok(collection)
    }

    /// Every photo has to be in the catalogue.
    fn require_photos(&self, photos: &[PhotoId]) -> Result<()> {
        for photo in photos {
            if self.catalogue.photo(photo)?.is_none() {
                return Err(not_found("photo", photo));
            }
        }
        Ok(())
    }

    /// Puts collections in the state one direction of their deltas leads to: the files and the catalogue's rows
    /// (`None`: it is gone). The one place a collection is written, whether the step is done, undone or redone.
    pub(super) fn write_collections(
        &mut self,
        deltas: &[CollectionDelta],
        direction: Direction,
    ) -> Result<()> {
        for delta in deltas {
            let target = match direction {
                Direction::Undo => &delta.before,
                Direction::Redo => &delta.after,
            };
            match target {
                Some(collection) => {
                    self.workspace.write_collection(collection)?;
                    let meta = std::fs::metadata(self.workspace.collection_path(&delta.id))?;
                    let stat: SidecarStat = stat_from(meta.len(), meta.modified().ok());
                    self.catalogue.apply_collection(collection, stat)?;
                }
                None => {
                    self.workspace.remove_collection(&delta.id)?;
                    self.catalogue.remove_collection(&delta.id)?;
                }
            }
        }
        let _ = self.events.send(Event::CollectionsChanged);
        Ok(())
    }

    /// Does one step: writes the deltas and records them.
    fn do_collections(
        &mut self,
        action: CollectionAction,
        deltas: Vec<CollectionDelta>,
    ) -> Result<()> {
        self.write_collections(&deltas, Direction::Redo)?;
        self.record(vec![Change::Collections { action, deltas }]);
        Ok(())
    }

    pub(super) fn create_collection(
        &mut self,
        name: String,
        parent: Option<CollectionId>,
        id: Option<CollectionId>,
        photos: Vec<PhotoId>,
    ) -> Result<Outcome> {
        let name = checked_collection_name(&name)?;
        let rows = self.catalogue.collections_with_counts()?;
        if let Some(parent) = parent
            && !rows.iter().any(|r| r.id == parent)
        {
            return Err(not_found("manual collection", parent));
        }
        ensure_name_is_free(&rows, &name, parent, None)?;
        let id = id.unwrap_or_else(CollectionId::random);
        if self.workspace.read_collection(&id)?.is_some() {
            return Err(EngineError::InvalidCommand(format!(
                "the collection {id} exists already"
            )));
        }
        let photos = distinct(photos);
        self.require_photos(&photos)?;
        let state = Collection {
            id,
            updated: Timestamp::now(),
            name,
            kind: collection_kind::MANUAL.into(),
            parent,
            members: photos.iter().map(|p| MemberRef::Photo(*p)).collect(),
            query: None,
            query_schema: None,
            extra: Default::default(),
        };
        let delta = CollectionDelta {
            id,
            before: None,
            after: Some(state),
        };
        self.do_collections(CollectionAction::Create, vec![delta])?;
        Ok(Outcome::CollectionCreated(id))
    }

    pub(super) fn rename_collection(
        &mut self,
        id: CollectionId,
        new_name: String,
    ) -> Result<Outcome> {
        let name = checked_collection_name(&new_name)?;
        let before = self.read_collection_state(&id)?;
        let rows = self.catalogue.collections_with_counts()?;
        ensure_name_is_free(&rows, &name, before.parent, Some(id))?;
        if before.name == name {
            return Ok(Outcome::Applied);
        }
        let after = Collection {
            name,
            updated: Timestamp::now(),
            ..before.clone()
        };
        let delta = CollectionDelta {
            id,
            before: Some(before),
            after: Some(after),
        };
        self.do_collections(CollectionAction::Rename, vec![delta])?;
        Ok(Outcome::Applied)
    }

    pub(super) fn move_collection(
        &mut self,
        id: CollectionId,
        new_parent: Option<CollectionId>,
    ) -> Result<Outcome> {
        let before = self.read_collection_state(&id)?;
        if before.parent == new_parent {
            return Ok(Outcome::Applied);
        }
        let rows = self.catalogue.collections_with_counts()?;
        if let Some(parent) = new_parent {
            if !rows.iter().any(|r| r.id == parent) {
                return Err(not_found("manual collection", parent));
            }
            if branch_of(&rows, id).contains(&parent) {
                return Err(EngineError::CollectionCycle);
            }
        }
        ensure_name_is_free(&rows, &before.name, new_parent, Some(id))?;
        let after = Collection {
            parent: new_parent,
            updated: Timestamp::now(),
            ..before.clone()
        };
        let delta = CollectionDelta {
            id,
            before: Some(before),
            after: Some(after),
        };
        self.do_collections(CollectionAction::Move, vec![delta])?;
        Ok(Outcome::Applied)
    }

    pub(super) fn delete_collection(&mut self, id: CollectionId) -> Result<Outcome> {
        let rows = self.catalogue.collections_with_counts()?;
        if !rows.iter().any(|r| r.id == id) {
            return Err(not_found("manual collection", id));
        }
        let mut deltas = Vec::new();
        for member in branch_of(&rows, id) {
            deltas.push(CollectionDelta {
                id: member,
                before: Some(self.read_collection_state(&member)?),
                after: None,
            });
        }
        self.do_collections(CollectionAction::Delete, deltas)?;
        Ok(Outcome::Applied)
    }

    pub(super) fn add_to_collection(
        &mut self,
        id: CollectionId,
        photos: Vec<PhotoId>,
    ) -> Result<Outcome> {
        let before = self.read_collection_state(&id)?;
        let photos = distinct(photos);
        self.require_photos(&photos)?;
        let held: HashSet<PhotoId> = before.members.iter().map(MemberRef::photo).collect();
        let fresh: Vec<PhotoId> = photos.into_iter().filter(|p| !held.contains(p)).collect();
        if fresh.is_empty() {
            return Ok(Outcome::Applied);
        }
        let mut members = before.members.clone();
        members.extend(fresh.into_iter().map(MemberRef::Photo));
        let after = Collection {
            members,
            updated: Timestamp::now(),
            ..before.clone()
        };
        let delta = CollectionDelta {
            id,
            before: Some(before),
            after: Some(after),
        };
        self.do_collections(CollectionAction::Add, vec![delta])?;
        Ok(Outcome::Applied)
    }

    pub(super) fn remove_from_collection(
        &mut self,
        id: CollectionId,
        photos: Vec<PhotoId>,
    ) -> Result<Outcome> {
        let before = self.read_collection_state(&id)?;
        let gone: HashSet<PhotoId> = photos.into_iter().collect();
        let members: Vec<MemberRef> = before
            .members
            .iter()
            .filter(|m| !matches!(m, MemberRef::Photo(p) if gone.contains(p)))
            .copied()
            .collect();
        if members.len() == before.members.len() {
            return Ok(Outcome::Applied);
        }
        let after = Collection {
            members,
            updated: Timestamp::now(),
            ..before.clone()
        };
        let delta = CollectionDelta {
            id,
            before: Some(before),
            after: Some(after),
        };
        self.do_collections(CollectionAction::Remove, vec![delta])?;
        Ok(Outcome::Applied)
    }

    /// A photo that has left the workspace (its source was removed) leaves its collections too. Not a step: the
    /// photo is gone. To be called before the catalogue forgets the photo (it is the catalogue that says which
    /// collections hold it).
    pub(super) fn leave_collections_on_removal(&mut self, photo: PhotoId) {
        let Ok(ids) = self.catalogue.collections_holding(&photo) else {
            return;
        };
        for id in ids {
            let Ok(before) = self.read_collection_state(&id) else {
                continue;
            };
            let after = Collection {
                members: before
                    .members
                    .iter()
                    .filter(|m| m.photo() != photo)
                    .copied()
                    .collect(),
                updated: Timestamp::now(),
                ..before.clone()
            };
            let delta = CollectionDelta {
                id,
                before: Some(before),
                after: Some(after),
            };
            let _ = self.write_collections(&[delta], Direction::Redo);
        }
    }
}
