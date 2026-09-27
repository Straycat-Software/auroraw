// SPDX-License-Identifier: GPL-3.0-or-later
//! Import as a person uses it (spec §5.2, M1 plan workflow revision D-090, D-093): copying the
//! photos of a card or a folder to another folder, verified, with an optional second destination.
//! It is a convenience separate from the catalogue: the destination need not be one of the
//! catalogue's sources. When it is (or a person agrees to make it one) the copied photos also enter
//! the catalogue with their metadata template; when it is not, they are only copied.
//!
//! Kept here rather than in `ui` so the same steps are testable without a window and reusable by a
//! future CLI `import`.

use std::path::{Path, PathBuf};

use auroraw_import::Profile;
use auroraw_sources::filesystem::LOCAL_FOLDER;
use auroraw_sources::volumes::{has_dcim, list_removable_volumes};
use auroraw_types::SourceId;

use crate::command::Command;
use crate::coordinator::Outcome;
use crate::error::{EngineError, Result};
use crate::import_job::Registration;
use crate::job::JobId;
use crate::sources_api::{AddPlan, SourceInfo};
use crate::{Engine, paths};

/// A removable volume this machine has mounted right now (a memory card, a USB drive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    /// The device's name, for display.
    pub name: String,
    /// Where it is mounted.
    pub mount_point: PathBuf,
    /// Whether it looks like a camera's storage (a `DCIM` folder, M1 plan §6 item 6).
    pub has_dcim: bool,
}

/// Everything [`Engine::import`] needs beyond what the profile itself carries. Both folders and the
/// backup are canonical paths (see [`crate::paths::resolve`]).
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// The card or folder to import from.
    pub source_root: PathBuf,
    /// The folder photos are copied into.
    pub destination_root: PathBuf,
    /// Layout (`{path}` keeps the card's folders), pairing rule, metadata template.
    pub profile: Profile,
    /// A second folder every file is also copied and verified into, laid out by the same
    /// destination template unless the profile names its own backup templates.
    pub backup_root: Option<PathBuf>,
    /// A folder, local to this machine, where the job's resumable state is kept.
    pub state_dir: PathBuf,
    /// When the destination is not inside a source of the catalogue: make it one first, so that the
    /// photos enter the catalogue.
    pub add_destination_as_source: bool,
}

/// An import that was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportStarted {
    /// The job doing the copy.
    pub job: JobId,
    /// The source the destination was registered as, when it was added for this import (a person
    /// may want its other files scanned afterwards).
    pub added_source: Option<SourceId>,
    /// Whether the photos are registered in the catalogue (else they are only copied).
    pub registered: bool,
}

/// What an import destination is to the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinationKind {
    /// Inside (or equal to) a source: the copied photos enter the catalogue there.
    Covered(SourceInfo),
    /// Not in the catalogue: a copy only, unless it is added as a source.
    NotCovered,
    /// A folder that contains sources of the catalogue: it cannot be added as a source from here.
    ContainsSources(Vec<SourceInfo>),
}

/// What a card or folder looks like, for offering to keep its folders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportSourceInfo {
    /// Whether it has a `DCIM` folder (a camera's own layout).
    pub has_dcim: bool,
    /// The camera folders below `DCIM` (`100CANON`, `101NIKON`...), sorted.
    pub camera_folders: Vec<String>,
}

fn invalid(message: String) -> EngineError {
    EngineError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

/// A short stable name for a pair of folders (FNV-1a, 64 bits): what tells one import's state file
/// from another's.
fn state_key(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

impl Engine {
    /// Every removable volume mounted right now, camera cards first.
    pub fn removable_volumes() -> Vec<VolumeInfo> {
        let mut volumes: Vec<VolumeInfo> = list_removable_volumes()
            .into_iter()
            .map(|v| VolumeInfo {
                has_dcim: has_dcim(&v.mount_point),
                name: v.name,
                mount_point: v.mount_point,
            })
            .collect();
        volumes.sort_by_key(|v| !v.has_dcim);
        volumes
    }

    /// Looks at a card or folder without reading any photo: does it have a camera's `DCIM` layout,
    /// and which camera folders. The folder may be the card's root or `DCIM` itself.
    pub fn inspect_import_source(root: &Path) -> ImportSourceInfo {
        let is_dcim = |path: &Path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("DCIM"))
        };
        let dcim = if is_dcim(root) && root.is_dir() {
            root.to_path_buf()
        } else {
            let Ok(entries) = std::fs::read_dir(root) else {
                return ImportSourceInfo::default();
            };
            let Some(found) = entries
                .flatten()
                .map(|entry| entry.path())
                .find(|path| is_dcim(path) && path.is_dir())
            else {
                return ImportSourceInfo::default();
            };
            found
        };
        let mut camera_folders: Vec<String> = std::fs::read_dir(&dcim)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        camera_folders.sort();
        ImportSourceInfo {
            has_dcim: true,
            camera_folders,
        }
    }

    /// What `destination` is to the catalogue: inside a source (the photos enter it), not in it (a
    /// copy, unless it is added), or a folder that contains sources.
    pub fn import_destination(&self, destination: &Path) -> Result<DestinationKind> {
        Ok(match self.plan_add_source(destination)? {
            AddPlan::InsideExisting(source) => DestinationKind::Covered(source),
            AddPlan::ContainsExisting(sources) => DestinationKind::ContainsSources(sources),
            AddPlan::Free => match self.covering_source(destination)? {
                Some((source, _)) => DestinationKind::Covered(source),
                None => DestinationKind::NotCovered,
            },
        })
    }

    /// Starts an import as a background job and returns its identifier at once; progress and the
    /// outcome arrive as `Event::ImportItem`, `Event::JobProgress` and `Event::ImportFinished` (or
    /// `Event::ImportAborted` when the source cannot be read at all).
    ///
    /// A job that was interrupted earlier from the same card to the same destination resumes where
    /// it stopped (its state file is still there); one that finished without a failure removed its
    /// state, so a reformatted card that reuses the same file names is never mistaken for the last
    /// one.
    pub fn import(&self, request: ImportRequest) -> Result<ImportStarted> {
        let ImportRequest {
            source_root,
            destination_root,
            mut profile,
            backup_root,
            state_dir,
            add_destination_as_source,
        } = request;
        // However the paths arrived, compared and stored they are canonical (D-092): on a Mac a
        // temporary folder is behind a symlink, on Windows a path may carry its verbatim prefix.
        let canonical = |path: PathBuf| {
            paths::resolve(&path.to_string_lossy()).map_err(|e| invalid(e.to_string()))
        };
        let source_root = canonical(source_root)?;
        let destination_root = if destination_root.as_os_str().is_empty() {
            destination_root
        } else {
            canonical(destination_root)?
        };
        let backup_root = backup_root.map(canonical).transpose()?;
        if !source_root.is_dir() {
            return Err(invalid(format!(
                "{} is not a folder",
                source_root.display()
            )));
        }
        if destination_root.as_os_str().is_empty() {
            return Err(invalid("no destination folder chosen".to_string()));
        }
        if paths::is_inside(&destination_root, &source_root) {
            return Err(invalid(
                "the destination is the folder being imported from, or inside it".to_string(),
            ));
        }
        if let Some(backup) = &backup_root
            && (paths::is_inside(backup, &source_root)
                || paths::is_inside(backup, &destination_root)
                || paths::is_inside(&destination_root, backup))
        {
            return Err(invalid(
                "the backup folder must be a different folder from the destination and the source"
                    .to_string(),
            ));
        }

        // What the catalogue makes of the destination, decided before anything is created.
        let kind = self.import_destination(&destination_root)?;
        if let DestinationKind::ContainsSources(sources) = &kind
            && add_destination_as_source
        {
            let names: Vec<String> = sources.iter().map(|s| format!("\"{}\"", s.name)).collect();
            return Err(EngineError::ContainsSources(names.join(", ")));
        }

        std::fs::create_dir_all(&destination_root)?;
        std::fs::create_dir_all(&state_dir)?;
        let mut added_source = None;
        let registration = match kind {
            DestinationKind::Covered(source) => {
                let root = paths::resolve(&source.path.to_string_lossy()).unwrap_or(source.path);
                let prefix = paths::relative_to(&destination_root, &root)
                    .unwrap_or_default()
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                Some(Registration {
                    source_id: source.id,
                    prefix,
                })
            }
            DestinationKind::NotCovered if add_destination_as_source => {
                let name = destination_root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| destination_root.display().to_string());
                match self.submit_and_wait(Command::AddSource {
                    name,
                    root: destination_root.clone(),
                    kind: LOCAL_FOLDER.to_string(),
                })? {
                    Outcome::SourceAdded(id) => {
                        added_source = Some(id);
                        Some(Registration {
                            source_id: id,
                            prefix: String::new(),
                        })
                    }
                    other => unreachable!("AddSource answers SourceAdded, not {other:?}"),
                }
            }
            DestinationKind::NotCovered | DestinationKind::ContainsSources(_) => None,
        };

        let backup_roots = match backup_root {
            Some(root) => {
                std::fs::create_dir_all(&root)?;
                if profile.backup_templates.is_empty() {
                    profile.backup_templates = vec![profile.destination_template.clone()];
                }
                vec![root]
            }
            None => {
                profile.backup_templates.clear();
                Vec::new()
            }
        };
        // One state file per card and destination: an interrupted import resumes, a different
        // destination is a different import.
        let key = state_key(&format!(
            "{}\n{}",
            source_root.display(),
            destination_root.display()
        ));
        let registered = registration.is_some();
        match self.submit_and_wait(Command::Import {
            source_root,
            destination_root,
            registration,
            profile,
            backup_roots,
            state_path: state_dir.join(format!("import-{key}.json")),
        })? {
            Outcome::ImportStarted { job } => Ok(ImportStarted {
                job,
                added_source,
                registered,
            }),
            other => unreachable!("Import answers ImportStarted, not {other:?}"),
        }
    }
}
