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
//! the batch goes on. A source that is not reachable is counted, not tried; nor is a folder that cannot
//! be listed (it would look empty, and every `.xmp` in it absent).
//!
//! The job reads first and writes later, and another application does not wait: a new file is written
//! only if nothing is at its name **at the last moment**, and a file that exists only if it is still the
//! file that was read (same size and time). When either is not so, the photo goes round again from the
//! reading, so that what the other application did goes through the comparison like any other change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use auroraw_catalogue::ExternalStat;
use auroraw_format::sidecar::external::{MAX_BYTES, read};
use auroraw_format::xmp::Xmp;
use auroraw_plugin_api::source::{Entry, Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_types::SourceId;
use auroraw_workspace::{write_beside, write_new_beside};

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::external_xmp::{XmpFile, stat_of};
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

/// How many times one photo goes round, from the reading to the writing, when the file at its destination
/// turns out not to be what was read (or to have appeared).
const ATTEMPTS: usize = 3;

/// The largest file Replace keeps a copy of, in memory, before it is replaced. An XMP file is kilobytes;
/// anything like this is not one, and Auroraw will not load it to replace it: say so and leave it.
const MAX_REPLACED: u64 = 64 * 1024 * 1024;

/// What a folder holds, with the paths its files have inside the source, or why it cannot be listed
/// (an unlisted folder is not an empty one).
fn list_folder(root: &Path, folder: &str) -> std::io::Result<Vec<Entry>> {
    let dir = if folder.is_empty() {
        root.to_path_buf()
    } else {
        root.join(folder)
    };
    let read_dir = std::fs::read_dir(dir)?;
    Ok(read_dir
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
        .collect())
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

    fn destination(&mut self, photo: &ExportPhoto) -> Result<Option<Destination>, Failure> {
        let Some(root) = self.job.roots.get(&photo.source_id).cloned() else {
            return Ok(None);
        };
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
            let entries = list_folder(&root, &folder).map_err(|e| {
                Failure(format!(
                    "the folder of the photo cannot be listed, so what it holds is not known: {e}"
                ))
            })?;
            self.folders.insert(
                key.clone(),
                destinations(&entries, &originals, self.job.options.naming),
            );
        }
        Ok(self
            .folders
            .get(&key)
            .and_then(|known| known.get(&photo.path))
            .cloned())
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
        if (stat.size as usize > MAX_BYTES && !replace) || stat.size > MAX_REPLACED {
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
        let Some(mut destination) = self.destination(photo)? else {
            return Err(Failure(
                "the photo's original is not listed in its source".into(),
            ));
        };
        for _ in 0..ATTEMPTS {
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
            // The file as it was read: what it must still be when it is replaced.
            let read_as = existing.as_ref().map(|e| e.stat);
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
                    if !write(&target, &bytes, read_as)? {
                        // Another application got there first: from the reading again, as an existing file.
                        let stat = stat_of_file(&target).unwrap_or_default();
                        destination = Destination::Existing(XmpFile { path, stat });
                        continue;
                    }
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
                    return Ok(());
                }
                Ok(ExportDecision::UpToDate) => self.report.up_to_date += 1,
                Ok(ExportDecision::HeldBack) => self.report.held_back += 1,
                Ok(ExportDecision::Failed(reason)) => return Err(Failure(reason)),
                Err(_) => return Err(Failure("the engine stopped".into())),
            }
            return Ok(());
        }
        Err(Failure(
            "the file kept changing while it was being exported".into(),
        ))
    }
}

/// Writes `bytes` at `target`. `read_as` is the stat of the file the decision was made on, or `None` for a
/// file that was not there: the new file is written only if nothing is there now, the existing one only if
/// it is still what was read. `Ok(false)` when it was not, and nothing was written.
fn write(target: &Path, bytes: &[u8], read_as: Option<ExternalStat>) -> Result<bool, Failure> {
    match read_as {
        None => match write_new_beside(target, bytes) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        },
        Some(read) => {
            if stat_of_file(target) != Some(read) {
                return Ok(false);
            }
            write_beside(target, bytes)?;
            Ok(true)
        }
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

/// The race the job guards against, played deterministically: the test is the coordinator, and it is
/// another application in between the two of its messages.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobId;
    use crate::xmp_export::XmpExportOptions;
    use auroraw_types::PhotoId;

    /// A valid XMP file whose content (and so its size) is `note`: another application's, in these tests.
    fn xmp(note: &str) -> Vec<u8> {
        format!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" \
             crs:Exposure2012=\"{note}\"/></rdf:RDF></x:xmpmeta>"
        )
        .into_bytes()
    }

    struct Bench {
        _dir: auroraw_testkit::TempDir,
        folder: PathBuf,
        run: Run,
        photo: ExportPhoto,
        prepare: mpsc::Receiver<Inbound>,
    }

    fn bench(existing: Option<&[u8]>) -> Bench {
        let dir = auroraw_testkit::temp_dir();
        let folder = dir.path().join("Card");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.jpg"), b"a photo").unwrap();
        if let Some(bytes) = existing {
            std::fs::write(folder.join("a.xmp"), bytes).unwrap();
        }
        let source_id = SourceId::random();
        let photo = ExportPhoto {
            photo_id: PhotoId::random(),
            source_id,
            path: "a.jpg".into(),
        };
        let (events, _) = mpsc::channel();
        let (inbound, prepare) = mpsc::channel();
        let job = ExportJob {
            job: JobId(1),
            photos: vec![photo.clone()],
            roots: HashMap::from([(source_id, folder.clone())]),
            originals: HashMap::from([(source_id, vec!["a.jpg".to_string()])]),
            options: XmpExportOptions::default(),
            events,
            inbound,
            cancel: CancelToken::new(),
        };
        let run = Run {
            job,
            report: XmpExportReport::default(),
            folders: HashMap::new(),
            by_folder: HashMap::new(),
            online: HashMap::new(),
        };
        Bench {
            _dir: dir,
            folder,
            run,
            photo,
            prepare,
        }
    }

    /// Waits for the job's next question to the coordinator.
    fn asked(
        prepare: &mpsc::Receiver<Inbound>,
    ) -> (
        Option<Box<crate::xmp_export::ExportExisting>>,
        mpsc::Sender<ExportDecision>,
    ) {
        match prepare.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Inbound::ExportPrepare {
                existing, reply, ..
            }) => (existing, reply),
            Ok(_) => panic!("not a question"),
            Err(e) => panic!("the job asked nothing: {e}"),
        }
    }

    #[test]
    fn a_file_made_after_the_listing_is_not_replaced_and_goes_through_the_comparison() {
        let Bench {
            _dir,
            folder,
            mut run,
            photo,
            prepare,
        } = bench(None);
        let worker = std::thread::spawn(move || {
            let result = run.photo(&photo);
            (result.err().map(|f| f.0), run.report)
        });
        let (existing, reply) = asked(&prepare);
        assert!(existing.is_none(), "nothing was there when it looked");
        // Another application makes the file, and the coordinator, knowing nothing of it, says write.
        let theirs = xmp("+0.50");
        std::fs::write(folder.join("a.xmp"), &theirs).unwrap();
        reply.send(ExportDecision::Write(b"ours".to_vec())).unwrap();
        // Not written: the photo goes round again, from the reading of the file that is there.
        let (existing, reply) = asked(&prepare);
        assert_eq!(existing.expect("read this time").bytes, theirs);
        assert_eq!(std::fs::read(folder.join("a.xmp")).unwrap(), theirs);
        reply.send(ExportDecision::HeldBack).unwrap();
        let (failure, report) = worker.join().unwrap();
        assert_eq!(failure, None);
        assert_eq!((report.written, report.held_back), (0, 1));
        assert_eq!(std::fs::read(folder.join("a.xmp")).unwrap(), theirs);
    }

    #[test]
    fn a_file_changed_between_the_reading_and_the_writing_is_read_again_not_replaced() {
        let Bench {
            _dir,
            folder,
            mut run,
            photo,
            prepare,
        } = bench(Some(&xmp("one")));
        let worker = std::thread::spawn(move || {
            let result = run.photo(&photo);
            (result.is_ok(), run.report)
        });
        let (existing, reply) = asked(&prepare);
        assert_eq!(existing.expect("a file").bytes, xmp("one"));
        let saved = xmp("two, saved meanwhile");
        std::fs::write(folder.join("a.xmp"), &saved).unwrap();
        reply.send(ExportDecision::Write(b"ours".to_vec())).unwrap();
        let (existing, reply) = asked(&prepare);
        assert_eq!(existing.expect("read again").bytes, saved);
        assert_eq!(
            std::fs::read(folder.join("a.xmp")).unwrap(),
            saved,
            "still what the other application saved"
        );
        // This time nothing moves, and the write goes through.
        reply.send(ExportDecision::Write(b"ours".to_vec())).unwrap();
        match prepare.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Inbound::ExportWritten { bytes, .. }) => assert_eq!(bytes, b"ours"),
            other => panic!("the write was not reported: {}", other.is_ok()),
        }
        let (ok, report) = worker.join().unwrap();
        assert!(ok);
        assert_eq!(report.written, 1);
        assert_eq!(std::fs::read(folder.join("a.xmp")).unwrap(), b"ours");
    }

    #[test]
    fn a_file_that_keeps_changing_fails_the_photo_and_is_never_replaced() {
        let Bench {
            _dir,
            folder,
            mut run,
            photo,
            prepare,
        } = bench(Some(&xmp("0")));
        let worker = std::thread::spawn(move || run.photo(&photo).err().map(|Failure(why)| why));
        for round in 1..=ATTEMPTS {
            let (existing, reply) = asked(&prepare);
            assert!(existing.is_some());
            std::fs::write(
                folder.join("a.xmp"),
                xmp(&format!("{round}{}", "0".repeat(round))),
            )
            .unwrap();
            reply.send(ExportDecision::Write(b"ours".to_vec())).unwrap();
        }
        let why = worker.join().unwrap().expect("failed");
        assert!(why.contains("kept changing"), "{why}");
        assert_ne!(std::fs::read(folder.join("a.xmp")).unwrap(), b"ours");
    }

    #[test]
    fn a_folder_that_cannot_be_listed_is_an_error_not_an_empty_folder() {
        let dir = auroraw_testkit::temp_dir();
        assert!(list_folder(dir.path(), "gone").is_err());
        std::fs::write(dir.path().join("a_file"), b"x").unwrap();
        assert!(
            list_folder(dir.path(), "a_file").is_err(),
            "a file is not a folder"
        );
        assert!(
            list_folder(dir.path(), "")
                .unwrap()
                .iter()
                .any(|e| e.path == "a_file")
        );
    }
}
