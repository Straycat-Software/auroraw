// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind `Command::ExportXmp` (spec §5.7, D-024; design note 003 §8.1): writes
//! the XMP files other applications read beside the originals.
//!
//! The job does the input and output, the coordinator does the deciding (D-126: it is the only writer
//! of the catalogue and the sidecars, and the only one that sees a photo as it is now). For each photo
//! the job finds the destination, reads the file that is there and parses it, and asks the coordinator
//! what to do with it ([`Inbound::ExportPrepare`]); it waits for the answer, which also paces it, so a
//! cancellation lands within about one photo. A file to write is written **atomically beside its
//! destination**, and the coordinator is told what was written ([`Inbound::ExportWritten`]) so that it
//! records the file as the new base and Auroraw's own write is never reported as an external change.
//!
//! Errors are per file: a read-only source, a permission or a full disk counts the photo as failed and
//! the batch goes on. A source that is not reachable is counted, not tried.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use auroraw_catalogue::ExternalStat;
use auroraw_format::sidecar::external::{MAX_BYTES, read};
use auroraw_format::xmp::Xmp;
use auroraw_plugin_api::source::{Entry, Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_types::SourceId;

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::external_xmp::stat_of;
use crate::job::{CancelToken, JobId};
use crate::xmp_export::{
    Destination, ExportDecision, ExportExisting, ExportPhoto, XmpExisting, XmpExportOptions,
    XmpExportReport, destinations, folder_of,
};

pub(crate) struct ExportJob {
    pub job: JobId,
    /// The photos to export, in the order asked.
    pub photos: Vec<ExportPhoto>,
    /// Where each involved source is on this machine.
    pub roots: HashMap<SourceId, PathBuf>,
    /// The original of every photo of each involved source (the whole source, not only the scope: a
    /// stem is shared by any photo of the folder, exported now or not).
    pub originals: HashMap<SourceId, Vec<String>>,
    pub options: XmpExportOptions,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
}

pub(crate) fn spawn(job: ExportJob) {
    std::thread::spawn(move || run(job));
}

/// What a folder holds, with the paths its files have inside the source.
fn list_folder(root: &Path, folder: &str) -> Vec<Entry> {
    let dir = if folder.is_empty() {
        root.to_path_buf()
    } else {
        root.join(folder)
    };
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .filter_map(|item| {
            let meta = item.metadata().ok().filter(|m| m.is_file())?;
            let name = item.file_name().into_string().ok()?;
            Some(Entry {
                path: if folder.is_empty() {
                    name
                } else {
                    format!("{folder}/{name}")
                },
                size: meta.len(),
                modified: meta.modified().ok(),
            })
        })
        .collect()
}

fn stat_of_file(path: &Path) -> Option<ExternalStat> {
    let meta = std::fs::metadata(path).ok()?;
    Some(stat_of(&Entry {
        path: String::new(),
        size: meta.len(),
        modified: meta.modified().ok(),
    }))
}

/// Why one photo could not be exported.
struct Failure(String);

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

struct Run {
    job: ExportJob,
    report: XmpExportReport,
    /// The destinations of each folder, worked out once.
    folders: HashMap<(SourceId, String), HashMap<String, Destination>>,
    /// The originals of each source, by folder.
    by_folder: HashMap<SourceId, HashMap<String, Vec<String>>>,
    online: HashMap<SourceId, bool>,
}

impl Run {
    fn fail(&mut self, reason: String) {
        self.report.failed += 1;
        self.report.first_error.get_or_insert(reason);
    }

    fn reachable(&mut self, source: &SourceId) -> bool {
        if let Some(known) = self.online.get(source) {
            return *known;
        }
        let up =
            self.job.roots.get(source).is_some_and(|root| {
                FilesystemSource::new(root.clone()).state() == SourceState::Online
            });
        self.online.insert(*source, up);
        up
    }

    fn destination(&mut self, photo: &ExportPhoto) -> Option<Destination> {
        let root = self.job.roots.get(&photo.source_id)?.clone();
        let folder = folder_of(&photo.path).to_string();
        let key = (photo.source_id, folder.clone());
        if !self.folders.contains_key(&key) {
            let originals = self
                .by_folder
                .entry(photo.source_id)
                .or_insert_with(|| {
                    let mut by_folder: HashMap<String, Vec<String>> = HashMap::new();
                    for original in self
                        .job
                        .originals
                        .get(&photo.source_id)
                        .into_iter()
                        .flatten()
                    {
                        by_folder
                            .entry(folder_of(original).to_string())
                            .or_default()
                            .push(original.clone());
                    }
                    by_folder
                })
                .get(&folder)
                .cloned()
                .unwrap_or_default();
            let entries = list_folder(&root, &folder);
            self.folders.insert(
                key.clone(),
                destinations(&entries, &originals, self.job.options.naming),
            );
        }
        self.folders.get(&key)?.get(&photo.path).cloned()
    }

    /// Reads the file at a destination that has one. `None` when it should be left alone (counted).
    fn read_existing(
        &mut self,
        root: &Path,
        path: &str,
    ) -> Result<Option<ExportExisting>, Failure> {
        let file = root.join(path);
        let replace = self.job.options.existing == XmpExisting::Replace;
        let stat =
            stat_of_file(&file).ok_or_else(|| Failure("cannot read the existing file".into()))?;
        if stat.size as usize > MAX_BYTES && !replace {
            self.report.unreadable += 1;
            return Ok(None);
        }
        let bytes = std::fs::read(&file)?;
        let parsed = if bytes.len() > MAX_BYTES {
            None
        } else {
            Xmp::from_bytes(&bytes).ok().zip(read(&bytes).ok())
        };
        if parsed.is_none() && !replace {
            self.report.unreadable += 1;
            return Ok(None);
        }
        Ok(Some(ExportExisting {
            bytes,
            stat,
            parsed,
        }))
    }

    /// One photo. Errors are the photo's own.
    fn photo(&mut self, photo: &ExportPhoto) -> Result<(), Failure> {
        if !self.reachable(&photo.source_id) {
            self.report.unreachable += 1;
            return Ok(());
        }
        let root = self.job.roots[&photo.source_id].clone();
        let Some(destination) = self.destination(photo) else {
            return Err(Failure(
                "the photo's original is not listed in its source".into(),
            ));
        };
        let existing = match &destination {
            Destination::Existing(file) => {
                if self.job.options.existing == XmpExisting::SkipExisting {
                    self.report.skipped_existing += 1;
                    return Ok(());
                }
                match self.read_existing(&root, &file.path)? {
                    Some(existing) => Some(existing),
                    None => return Ok(()),
                }
            }
            Destination::New { .. } => None,
        };
        let path = destination.path().to_string();
        let (reply, answer) = mpsc::channel();
        self.job
            .inbound
            .send(Inbound::ExportPrepare {
                photo_id: photo.photo_id,
                source_id: photo.source_id,
                path: path.clone(),
                existing: existing.map(Box::new),
                options: self.job.options,
                reply,
            })
            .map_err(|_| Failure("the engine stopped".into()))?;
        match answer.recv() {
            Ok(ExportDecision::Write(bytes)) => {
                let target = root.join(&path);
                auroraw_workspace::write_beside(&target, &bytes)?;
                self.report.written += 1;
                if matches!(
                    destination,
                    Destination::New {
                        shared_stem: true,
                        ..
                    }
                ) {
                    self.report.name_shared += 1;
                }
                let stat = stat_of_file(&target).unwrap_or_default();
                let _ = self.job.inbound.send(Inbound::ExportWritten {
                    photo_id: photo.photo_id,
                    path,
                    stat,
                    bytes,
                });
            }
            Ok(ExportDecision::UpToDate) => self.report.up_to_date += 1,
            Ok(ExportDecision::HeldBack) => self.report.held_back += 1,
            Ok(ExportDecision::Failed(reason)) => return Err(Failure(reason)),
            Err(_) => return Err(Failure("the engine stopped".into())),
        }
        Ok(())
    }
}

fn run(job: ExportJob) {
    let photos = job.photos.clone();
    let total = photos.len();
    let events = job.events.clone();
    let inbound = job.inbound.clone();
    let cancel = job.cancel.clone();
    let id = job.job;
    let mut state = Run {
        job,
        report: XmpExportReport::default(),
        folders: HashMap::new(),
        by_folder: HashMap::new(),
        online: HashMap::new(),
    };
    let mut cancelled = false;
    for (done, photo) in photos.iter().enumerate() {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        if let Err(Failure(reason)) = state.photo(photo) {
            state.fail(reason);
        }
        let _ = events.send(Event::JobProgress {
            job: id,
            done: done + 1,
            total,
        });
    }
    // Through `inbound`, so that it is seen after every write the job asked for (`Inbound::Report`'s own
    // doc comment): the coordinator reports the end.
    let _ = inbound.send(Inbound::ExportDone {
        job: id,
        report: state.report,
        cancelled,
    });
}
