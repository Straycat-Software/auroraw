// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP export to the source folders (spec §5.7, D-024, D-028, D-065, D-066; design note 003 §8 and
//! §8.1): what a person asks for, what comes back, and which file each photo's export goes to.
//!
//! The work itself is `crate::xmp_export_job`; this file is the request, the report, and the pure
//! decision of the file name, made from a folder's listing alone.

use std::collections::HashMap;

use auroraw_catalogue::ExternalStat;
use auroraw_format::sidecar::Metadata;
use auroraw_format::sidecar::external::Fields;
use auroraw_format::state::KeywordEntry;
use auroraw_format::xmp::Xmp;
use auroraw_plugin_api::source::Entry;
use auroraw_types::{KeywordId, PhotoId, SourceId};

use crate::external_xmp::{XmpFile, owners, without_extension};

/// Which photos an export is for.
#[derive(Debug, Clone, PartialEq)]
pub enum XmpScope {
    /// These photos (the selection).
    Photos(Vec<PhotoId>),
    /// Every photo whose original is in this source.
    Source(SourceId),
}

/// The name of a file the export creates (D-028); a file that already exists keeps its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XmpNaming {
    /// `photo.xmp`, the default; `photo.ARW.xmp` for a photo whose stem another photo of the folder
    /// shares (`A.CR2` and `A.DNG`), since `photo.xmp` would then belong to neither.
    #[default]
    Stem,
    /// `photo.ARW.xmp`, always.
    FullName,
}

/// What to do with a file that already exists at the destination (design note 003 §8.1 item 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XmpExisting {
    /// Rewrite only what Auroraw owns, keep everything else in the file, and hold the file back when
    /// another application changed it since Auroraw last saw it.
    #[default]
    Merge,
    /// Write a new file over it; the old one is first kept under the workspace's `removed/`.
    Replace,
    /// Leave every file that exists alone; write only the missing ones.
    SkipExisting,
}

/// How an export is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmpExportOptions {
    /// The name of a new file.
    pub naming: XmpNaming,
    /// What to do with a file that exists.
    pub existing: XmpExisting,
    /// Whether a rejected photo is written as `xmp:Rating` `-1`, which other software understands.
    pub rejected_as_minus_one: bool,
}

impl Default for XmpExportOptions {
    fn default() -> Self {
        Self {
            naming: XmpNaming::default(),
            existing: XmpExisting::default(),
            rejected_as_minus_one: true,
        }
    }
}

/// How an export ended: one count for each thing that can happen to a photo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XmpExportReport {
    /// Files written, new or merged.
    pub written: usize,
    /// Files that already said what would be written: left as they are.
    pub up_to_date: usize,
    /// Files another application changed since Auroraw last saw them: nothing was written, the change
    /// waits in the review (`Event::ExternalChanges`) to be accepted or ignored, then export again.
    pub held_back: usize,
    /// Files that exist, left alone because the export was asked to only add missing ones.
    pub skipped_existing: usize,
    /// Photos whose source could not be reached.
    pub unreachable: usize,
    /// Files that exist but are not XMP (or too large): never touched, except by Replace.
    pub unreadable: usize,
    /// New files named `photo.ARW.xmp` although `photo.xmp` was asked, because another photo of the
    /// folder shares the stem.
    pub name_shared: usize,
    /// Photos whose file could not be written (read-only source, permission, disk), or that left.
    pub failed: usize,
    /// The first reason a file could not be written, for a person to read.
    pub first_error: Option<String>,
}

/// A photo in the scope of an export.
#[derive(Debug, Clone)]
pub(crate) struct ExportPhoto {
    pub photo_id: PhotoId,
    pub source_id: SourceId,
    /// The original's path inside its source.
    pub path: String,
}

/// Where a photo's export goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Destination {
    /// A file the photo already owns: it is written in its own name, whatever the naming asked for, so
    /// that the folder never holds two `.xmp` files for one photo.
    Existing(XmpFile),
    /// A file to create.
    New {
        path: String,
        /// `photo.xmp` was asked but the stem is shared: the full name is used instead.
        shared_stem: bool,
    },
}

impl Destination {
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Existing(file) => &file.path,
            Self::New { path, .. } => path,
        }
    }
}

/// The folder of an original's path (`""` at the root of the source).
pub(crate) fn folder_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// The destination of each original of one folder. `entries` is what the folder holds (its files, with
/// the paths they have inside the source) and `originals` the paths of every photo original in it:
/// the same inputs `crate::external_xmp::owners` reads a scan with, so the export ties a file to a photo
/// exactly as detection does.
pub(crate) fn destinations(
    entries: &[Entry],
    originals: &[String],
    naming: XmpNaming,
) -> HashMap<String, Destination> {
    let owned = owners(entries, &[], originals.iter().map(String::as_str));
    let mut stems: HashMap<String, usize> = HashMap::new();
    for original in originals {
        *stems
            .entry(without_extension(original).to_lowercase())
            .or_default() += 1;
    }
    originals
        .iter()
        .map(|original| {
            let destination = match owned.get(original) {
                Some(file) => Destination::Existing(file.clone()),
                None => {
                    let unique = stems.get(&without_extension(original).to_lowercase()) == Some(&1);
                    match naming {
                        XmpNaming::Stem if unique => Destination::New {
                            path: format!("{}.xmp", without_extension(original)),
                            shared_stem: false,
                        },
                        _ => Destination::New {
                            path: format!("{original}.xmp"),
                            shared_stem: naming == XmpNaming::Stem,
                        },
                    }
                }
            };
            (original.clone(), destination)
        })
        .collect()
}

/// The file already at a photo's destination, as the export job read it.
#[derive(Debug, Clone)]
pub(crate) struct ExportExisting {
    /// Its bytes.
    pub bytes: Vec<u8>,
    /// Its size and modification time, taken just before it was read.
    pub stat: ExternalStat,
    /// What it holds, or `None` when it is not XMP (only sent for a Replace, which does not need it).
    pub parsed: Option<(Xmp, Fields)>,
}

/// What the coordinator tells the export job to do with a photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExportDecision {
    /// Write these bytes to the destination.
    Write(Vec<u8>),
    /// The file already says it: leave it.
    UpToDate,
    /// Another application changed the file: nothing is written, the change waits for a review.
    HeldBack,
    /// Nothing can be written for this photo, and why.
    Failed(String),
}

/// The keywords of a photo as an export writes them: their current paths, without any that is marked
/// do-not-export (D-045) or lies under one that is (a path names its ancestors, so a hidden ancestor
/// would show in every path below it).
pub(crate) fn exported_keywords(
    meta: &Metadata,
    vocabulary: &[KeywordEntry],
    key_paths: &HashMap<KeywordId, String>,
) -> Vec<String> {
    if meta.keyword_ids.is_empty() {
        return meta.keyword_paths.clone();
    }
    let by_id: HashMap<KeywordId, &KeywordEntry> = vocabulary.iter().map(|k| (k.id, k)).collect();
    let hidden = |id: KeywordId| {
        let mut seen = std::collections::HashSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            match by_id.get(&id) {
                Some(entry) if !entry.export => return true,
                Some(entry) => current = entry.parent,
                None => break,
            }
        }
        false
    };
    let aligned = meta.keyword_ids.len() == meta.keyword_paths.len();
    meta.keyword_ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            if by_id.contains_key(id) {
                if hidden(*id) {
                    None
                } else {
                    key_paths.get(id).cloned()
                }
            } else {
                aligned.then(|| meta.keyword_paths[i].clone())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.into(),
            size: 10,
            modified: Some(UNIX_EPOCH + Duration::new(1_700_000_000, 0)),
        }
    }

    fn plan(files: &[&str], originals: &[&str], naming: XmpNaming) -> Vec<(String, String, bool)> {
        let entries: Vec<Entry> = files.iter().map(|f| entry(f)).collect();
        let originals: Vec<String> = originals.iter().map(ToString::to_string).collect();
        let mut out: Vec<_> = destinations(&entries, &originals, naming)
            .into_iter()
            .map(|(original, d)| {
                let existing = matches!(d, Destination::Existing(_));
                (original, d.path().to_string(), existing)
            })
            .collect();
        out.sort();
        out
    }

    fn row(a: &str, b: &str, existing: bool) -> (String, String, bool) {
        (a.into(), b.into(), existing)
    }

    #[test]
    fn a_new_file_is_named_after_the_stem_or_the_full_name_as_asked() {
        assert_eq!(
            plan(&["d/a.jpg"], &["d/a.jpg"], XmpNaming::Stem),
            [row("d/a.jpg", "d/a.xmp", false)]
        );
        assert_eq!(
            plan(&["d/a.jpg"], &["d/a.jpg"], XmpNaming::FullName),
            [row("d/a.jpg", "d/a.jpg.xmp", false)]
        );
    }

    #[test]
    fn a_file_the_photo_already_owns_is_kept_whatever_the_naming() {
        assert_eq!(
            plan(&["a.ARW", "a.ARW.xmp"], &["a.ARW"], XmpNaming::Stem),
            [row("a.ARW", "a.ARW.xmp", true)]
        );
        assert_eq!(
            plan(&["a.ARW", "a.xmp"], &["a.ARW"], XmpNaming::FullName),
            [row("a.ARW", "a.xmp", true)]
        );
    }

    #[test]
    fn photos_sharing_a_stem_get_full_names_so_that_no_two_share_a_file() {
        let planned = plan(&["A.CR2", "A.DNG"], &["A.CR2", "A.DNG"], XmpNaming::Stem);
        assert_eq!(
            planned,
            [
                row("A.CR2", "A.CR2.xmp", false),
                row("A.DNG", "A.DNG.xmp", false)
            ]
        );
        // ... and a stem-named file already there belongs to neither, so it is not touched.
        let with_orphan = plan(
            &["A.CR2", "A.DNG", "A.xmp"],
            &["A.CR2", "A.DNG"],
            XmpNaming::Stem,
        );
        assert!(
            with_orphan
                .iter()
                .all(|(_, path, existing)| path != "A.xmp" && !existing)
        );
    }

    #[test]
    fn a_stem_is_compared_without_case_and_only_within_a_folder() {
        assert_eq!(
            plan(
                &["d/IMG_1.JPG", "e/img_1.jpg"],
                &["d/IMG_1.JPG"],
                XmpNaming::Stem
            ),
            [row("d/IMG_1.JPG", "d/IMG_1.xmp", false)]
        );
        assert_eq!(
            plan(
                &["d/IMG_1.JPG", "d/img_1.CR2"],
                &["d/IMG_1.JPG", "d/img_1.CR2"],
                XmpNaming::Stem
            )
            .len(),
            2
        );
    }

    #[test]
    fn a_shared_stem_is_reported_only_when_the_stem_name_was_asked() {
        let entries: Vec<Entry> = Vec::new();
        let originals = vec!["A.CR2".to_string(), "A.DNG".to_string()];
        for (naming, expected) in [(XmpNaming::Stem, true), (XmpNaming::FullName, false)] {
            let planned = destinations(&entries, &originals, naming);
            assert!(planned.values().all(|d| matches!(
                d,
                Destination::New { shared_stem, .. } if *shared_stem == expected
            )));
        }
    }

    #[test]
    fn the_folder_of_a_path() {
        assert_eq!(folder_of("a/b/c.jpg"), "a/b");
        assert_eq!(folder_of("c.jpg"), "");
    }

    fn keyword(byte: u8, name: &str, parent: Option<u8>, export: bool) -> KeywordEntry {
        KeywordEntry {
            id: KeywordId::from_bytes([byte; 8]),
            name: name.into(),
            parent: parent.map(|p| KeywordId::from_bytes([p; 8])),
            synonyms: Vec::new(),
            export,
            extra: Default::default(),
        }
    }

    #[test]
    fn a_do_not_export_keyword_and_everything_below_it_stays_home() {
        let vocabulary = vec![
            keyword(1, "Places", None, true),
            keyword(2, "Secret spot", Some(1), false),
            keyword(3, "Cabin", Some(2), true),
            keyword(4, "Quebec", Some(1), true),
        ];
        let paths = auroraw_catalogue::keyword_paths(&vocabulary);
        let mut meta = Metadata::default();
        for (byte, path) in [
            (3, "Places|Secret spot|Cabin"),
            (4, "Places|Quebec"),
            (2, "Places|Secret spot"),
        ] {
            meta.push_keyword(KeywordId::from_bytes([byte; 8]), path);
        }
        assert_eq!(
            exported_keywords(&meta, &vocabulary, &paths),
            ["Places|Quebec"]
        );
    }

    #[test]
    fn a_keyword_the_vocabulary_does_not_know_is_written_as_the_sidecar_named_it() {
        let mut meta = Metadata::default();
        meta.push_keyword(KeywordId::from_bytes([9; 8]), "Old|Name");
        assert_eq!(exported_keywords(&meta, &[], &HashMap::new()), ["Old|Name"]);
        let foreign = Metadata {
            keyword_paths: vec!["From|Elsewhere".into()],
            ..Metadata::default()
        };
        assert_eq!(
            exported_keywords(&foreign, &[], &HashMap::new()),
            ["From|Elsewhere"]
        );
    }
}
