// SPDX-License-Identifier: GPL-3.0-or-later
//! Series in the coordinator (WP9, D-101): the edits a person makes (group, take out, dissolve, resolve, reopen:
//! each a step of the history, a [`Change::Series`] holding the state file before and after, so that do, undo and
//! redo are one operation), the detection that forms series by itself when photos arrive (not a step: nobody
//! asked for it and it touches only photos that were in no series), and the shrinking of a series whose photo
//! left with its source.

use std::collections::HashSet;

use auroraw_catalogue::SidecarStat;
use auroraw_format::sidecar::Flag;
use auroraw_format::state::Series;
use auroraw_types::{PhotoId, SeriesId, Timestamp};

use super::{Coordinator, Outcome, stat_from};
use crate::command::Command;
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::history::{Change, SeriesAction};
use crate::series_detect;

/// A series as it is after `gone` photos left it: `None` when fewer than two remain (it is dissolved), else with
/// its cover (and what was kept) adjusted.
fn without(series: &Series, gone: &HashSet<PhotoId>) -> Option<Series> {
    let members: Vec<PhotoId> = series
        .members
        .iter()
        .filter(|m| !gone.contains(m))
        .copied()
        .collect();
    if members.len() < 2 {
        return None;
    }
    let cover = if gone.contains(&series.cover) {
        members[0]
    } else {
        series.cover
    };
    Some(Series {
        updated: Timestamp::now(),
        cover,
        kept: series
            .kept
            .iter()
            .filter(|k| !gone.contains(k))
            .copied()
            .collect(),
        members,
        ..series.clone()
    })
}

impl Coordinator {
    /// A series' state file, as it is now.
    fn read_series_state(&self, id: &SeriesId) -> Result<Series> {
        self.workspace
            .read_series(id)?
            .and_then(|loaded| loaded.current())
            .ok_or_else(|| EngineError::NotFound {
                kind: "series",
                id: id.to_string(),
            })
    }

    /// The series a photo is in, if any.
    fn series_of(&self, photo: &PhotoId) -> Result<Option<SeriesId>> {
        Ok(self.catalogue.photo(photo)?.and_then(|row| row.series_id))
    }

    /// Puts a series in the given state: the file and the catalogue's rows (`None`: it is gone). The one place a
    /// series is written, whether the step is done, undone or redone.
    pub(super) fn write_series_state(
        &mut self,
        id: SeriesId,
        state: &Option<Series>,
    ) -> Result<()> {
        match state {
            Some(series) => {
                self.workspace.write_series(series)?;
                let meta = std::fs::metadata(self.workspace.series_path(&id))?;
                let stat: SidecarStat = stat_from(meta.len(), meta.modified().ok());
                self.catalogue.apply_series(series, stat)?;
            }
            None => {
                self.workspace.remove_series(&id)?;
                self.catalogue.remove_series(&id)?;
            }
        }
        let _ = self.events.send(Event::SeriesChanged);
        Ok(())
    }

    /// The states the series that `photos` are in are left in once `photos` are out of them, as changes.
    fn leaving(&mut self, photos: &[PhotoId], action: SeriesAction) -> Result<Vec<Change>> {
        let gone: HashSet<PhotoId> = photos.iter().copied().collect();
        let mut affected: Vec<SeriesId> = Vec::new();
        for photo in photos {
            if let Some(series) = self.series_of(photo)?
                && !affected.contains(&series)
            {
                affected.push(series);
            }
        }
        let mut changes = Vec::new();
        for id in affected {
            let before = self.read_series_state(&id)?;
            let after = without(&before, &gone);
            changes.push(Change::Series {
                action,
                id,
                before: Some(before),
                after,
            });
        }
        Ok(changes)
    }

    /// Applies changes to series, all or nothing, and records them as one step.
    fn apply_series_changes(&mut self, mut changes: Vec<Change>) -> Result<()> {
        let mut done: Vec<Change> = Vec::new();
        for change in changes.drain(..) {
            let Change::Series { id, after, .. } = &change else {
                unreachable!("only series changes are applied here");
            };
            match self.write_series_state(*id, after) {
                Ok(()) => done.push(change),
                Err(e) => {
                    self.take_back(&done);
                    return Err(e);
                }
            }
        }
        self.record(done);
        Ok(())
    }

    pub(super) fn group_photos(&mut self, photos: Vec<PhotoId>) -> Result<Outcome> {
        let mut unique: Vec<PhotoId> = Vec::new();
        for photo in photos {
            if !unique.contains(&photo) {
                unique.push(photo);
            }
        }
        if unique.len() < 2 {
            return Err(EngineError::InvalidCommand(
                "A series needs at least two photos.".into(),
            ));
        }
        // In capture order, then by name, and every photo has to be in the catalogue.
        let mut rows = Vec::new();
        for photo in &unique {
            rows.push(
                self.catalogue
                    .photo(photo)?
                    .ok_or_else(|| EngineError::NotFound {
                        kind: "photo",
                        id: photo.to_string(),
                    })?,
            );
        }
        rows.sort_by(|a, b| {
            (a.capture_time, &a.filename, a.id.to_string()).cmp(&(
                b.capture_time,
                &b.filename,
                b.id.to_string(),
            ))
        });
        let members: Vec<PhotoId> = rows.iter().map(|r| r.id).collect();
        let mut changes = self.leaving(&members, SeriesAction::Group)?;
        let id = SeriesId::random();
        changes.push(Change::Series {
            action: SeriesAction::Group,
            id,
            before: None,
            after: Some(Series {
                id,
                updated: Timestamp::now(),
                kind: "manual".into(),
                cover: members[0],
                resolved: false,
                kept: Vec::new(),
                members,
                extra: Default::default(),
            }),
        });
        self.apply_series_changes(changes)?;
        Ok(Outcome::SeriesGrouped(id))
    }

    pub(super) fn remove_from_series(&mut self, photos: Vec<PhotoId>) -> Result<Outcome> {
        let changes = self.leaving(&photos, SeriesAction::Ungroup)?;
        if !changes.is_empty() {
            self.apply_series_changes(changes)?;
        }
        Ok(Outcome::Applied)
    }

    pub(super) fn dissolve_series(&mut self, id: SeriesId) -> Result<Outcome> {
        let before = self.read_series_state(&id)?;
        self.apply_series_changes(vec![Change::Series {
            action: SeriesAction::Ungroup,
            id,
            before: Some(before),
            after: None,
        }])?;
        Ok(Outcome::Applied)
    }

    pub(super) fn resolve_series(&mut self, id: SeriesId, keep: Vec<PhotoId>) -> Result<Outcome> {
        let before = self.read_series_state(&id)?;
        let kept: Vec<PhotoId> = before
            .members
            .iter()
            .filter(|m| keep.contains(m))
            .copied()
            .collect();
        if kept.is_empty() {
            return Err(EngineError::InvalidCommand(
                "Resolving a series keeps at least one of its photos.".into(),
            ));
        }
        // The flags first (one change per photo that changes), then the series: undo takes them back in the
        // other order, and a photo that has left the workspace meanwhile is skipped.
        let mut changes: Vec<Change> = Vec::new();
        for member in &before.members {
            let flag = if kept.contains(member) {
                Flag::Picked
            } else {
                Flag::Rejected
            };
            match self.apply_edit(&Command::SetFlag {
                photo_id: *member,
                flag: Some(flag),
            }) {
                Ok(change) => changes.extend(change),
                Err(EngineError::NotFound { .. }) => {}
                Err(e) => {
                    self.take_back(&changes);
                    return Err(e);
                }
            }
        }
        let after = Series {
            updated: Timestamp::now(),
            resolved: true,
            kept,
            ..before.clone()
        };
        if let Err(e) = self.write_series_state(id, &Some(after.clone())) {
            self.take_back(&changes);
            return Err(e);
        }
        changes.push(Change::Series {
            action: SeriesAction::Resolve,
            id,
            before: Some(before),
            after: Some(after),
        });
        self.record(changes);
        Ok(Outcome::Applied)
    }

    pub(super) fn reopen_series(&mut self, id: SeriesId) -> Result<Outcome> {
        let before = self.read_series_state(&id)?;
        let after = Series {
            updated: Timestamp::now(),
            resolved: false,
            kept: Vec::new(),
            ..before.clone()
        };
        self.apply_series_changes(vec![Change::Series {
            action: SeriesAction::Reopen,
            id,
            before: Some(before),
            after: Some(after),
        }])?;
        Ok(Outcome::Applied)
    }

    /// Forms series among the photos that are in none. Not a step of the history: nobody asked for these
    /// series one by one, they only touch photos that were in none, and taking them away is what *Ungroup* is for.
    pub(super) fn detect_series(&mut self, regroup: bool) -> Result<Outcome> {
        if regroup {
            let dissolved: Vec<SeriesId> = self
                .catalogue
                .series_infos()?
                .into_iter()
                .filter(|s| !s.resolved && (s.kind == "burst" || s.kind == "bracket"))
                .map(|s| s.id)
                .collect();
            for id in &dissolved {
                self.write_series_state(*id, &None)?;
            }
            // Steps that were about these series can no longer be undone.
            let ids: HashSet<SeriesId> = dissolved.into_iter().collect();
            if self.history.forget_series(&ids) {
                self.report_history();
            }
        }
        let candidates = self.catalogue.series_candidates()?;
        let groups = series_detect::detect(&candidates, self.series_gap);
        let (mut formed, mut photos) = (0, 0);
        for group in groups {
            let id = SeriesId::random();
            let series = Series {
                id,
                updated: Timestamp::now(),
                kind: group.kind.kind().into(),
                cover: group.members[0],
                resolved: false,
                kept: Vec::new(),
                members: group.members,
                extra: Default::default(),
            };
            photos += series.members.len();
            self.write_series_state(id, &Some(series))?;
            formed += 1;
        }
        Ok(Outcome::SeriesDetected {
            series: formed,
            photos,
        })
    }

    /// A photo that has left the workspace (its source was removed) leaves its series too, which is dissolved when
    /// fewer than two photos remain. Not a step: the photo is gone.
    pub(super) fn leave_series_on_removal(&mut self, photo: PhotoId) {
        let Ok(Some(id)) = self.series_of(&photo) else {
            return;
        };
        let Ok(before) = self.read_series_state(&id) else {
            return;
        };
        let after = without(&before, &HashSet::from([photo]));
        let _ = self.write_series_state(id, &after);
    }
}
