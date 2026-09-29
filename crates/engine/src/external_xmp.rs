// SPDX-License-Identifier: GPL-3.0-or-later
//! Which `.xmp` file another application keeps next to which photo's original (D-047, D-028;
//! design note 003 §8.1 item 7), worked out from the listing of a source alone: no file is opened
//! here. `is_photo_file` drops `.xmp` files from the photos a scan considers, and stays as it is;
//! the `.xmp` entries the listing already holds are what this looks at, so it costs no I/O.
//!
//! An original `dir/Name.ext` is owned by, in this order:
//! 1. `dir/Name.ext.xmp` (the `photo.ARW.xmp` form): unambiguous, and it wins when both exist;
//! 2. `dir/Name.xmp`, claimed only when exactly one photo's original in that folder has that stem
//!    (any case). A RAW and its JPEG are one photo (D-032), so the RAW owns it; two RAWs, or a JPEG
//!    and a PNG, sharing a stem leave `Name.xmp` unclaimed.

use std::collections::HashMap;
use std::time::UNIX_EPOCH;

use auroraw_catalogue::{ExternalKnown, ExternalStat};
use auroraw_format::sidecar::external::Fields;
use auroraw_plugin_api::source::Entry;
use auroraw_types::PhotoId;

/// The tracked file of a photo, as a scan listed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XmpFile {
    /// Its path inside the source, `/` separators, as the listing spelled it.
    pub path: String,
    /// Its size and modification time, from the listing (taken before the file is read: a rewrite
    /// racing the read costs at worst a redundant re-read next time, never a missed change).
    pub stat: ExternalStat,
}

/// What an index job read from a photo's external file, on its way to the coordinator.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExternalSeen {
    pub photo_id: PhotoId,
    pub path: String,
    pub stat: ExternalStat,
    pub fields: Fields,
}

fn stat_of(entry: &Entry) -> ExternalStat {
    ExternalStat {
        size: entry.size,
        modified_ns: entry
            .modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_nanos()).ok()),
    }
}

fn inside_skipped(path: &str, skip: &[String]) -> bool {
    skip.iter()
        .any(|folder| path == folder || path.starts_with(&format!("{folder}/")))
}

/// The path with its last extension taken off (`dir/Name.ext` → `dir/Name`); a name with no extension
/// is its own stem.
fn without_extension(path: &str) -> &str {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    match path[name_start..].rfind('.') {
        Some(dot) if dot > 0 => &path[..name_start + dot],
        _ => path,
    }
}

/// The owner of each `.xmp` in `entries` (outside the `skip`ped folders), by the original's path:
/// `originals` are the photos' original files found by the scan (a group's original, never a companion).
pub(crate) fn owners<'a>(
    entries: &[Entry],
    skip: &[String],
    originals: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, XmpFile> {
    // Every .xmp by its lower-case path without the extension; on a clash of spelling the
    // lexicographically first wins, so that the answer does not depend on the listing's order.
    let mut xmps: HashMap<String, &Entry> = HashMap::new();
    for entry in entries {
        if !entry.path.to_ascii_lowercase().ends_with(".xmp") || inside_skipped(&entry.path, skip) {
            continue;
        }
        let key = without_extension(&entry.path).to_lowercase();
        match xmps.get(&key) {
            Some(held) if held.path <= entry.path => {}
            _ => {
                xmps.insert(key, entry);
            }
        }
    }
    if xmps.is_empty() {
        return HashMap::new();
    }
    let originals: Vec<&str> = originals.into_iter().collect();
    let mut stems: HashMap<String, usize> = HashMap::new();
    for original in &originals {
        *stems
            .entry(without_extension(original).to_lowercase())
            .or_default() += 1;
    }
    let mut owned = HashMap::new();
    for original in originals {
        let claimed = xmps
            .get(&original.to_lowercase())
            .or_else(|| {
                let stem = without_extension(original).to_lowercase();
                (stems.get(&stem) == Some(&1))
                    .then(|| xmps.get(&stem))
                    .flatten()
            })
            .copied();
        if let Some(entry) = claimed {
            owned.insert(
                original.to_string(),
                XmpFile {
                    path: entry.path.clone(),
                    stat: stat_of(entry),
                },
            );
        }
    }
    owned
}

/// Whether a scan has to open `xmp` to know what it holds: nothing is known of it yet, or it is not
/// the file that was tracked.
pub(crate) fn needs_reading(known: Option<&ExternalKnown>, xmp: &XmpFile) -> bool {
    known.is_none_or(|k| k.path != xmp.path)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.into(),
            size: 10,
            modified: Some(UNIX_EPOCH + Duration::new(1_700_000_000, 123)),
        }
    }

    fn owned(files: &[&str], originals: &[&str]) -> Vec<(String, String)> {
        let entries: Vec<Entry> = files.iter().map(|f| entry(f)).collect();
        let mut owned: Vec<_> = owners(&entries, &[], originals.iter().copied())
            .into_iter()
            .map(|(original, xmp)| (original, xmp.path))
            .collect();
        owned.sort();
        owned
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn a_stem_named_xmp_belongs_to_the_one_photo_with_that_stem() {
        assert_eq!(
            owned(&["d/IMG_1.jpg", "d/IMG_1.xmp"], &["d/IMG_1.jpg"]),
            [pair("d/IMG_1.jpg", "d/IMG_1.xmp")]
        );
    }

    #[test]
    fn a_full_name_xmp_wins_over_a_stem_named_one() {
        assert_eq!(
            owned(&["a.ARW", "a.xmp", "a.ARW.xmp"], &["a.ARW"]),
            [pair("a.ARW", "a.ARW.xmp")]
        );
    }

    #[test]
    fn two_photos_sharing_a_stem_leave_the_stem_named_xmp_unclaimed_but_the_full_names_work() {
        assert_eq!(owned(&["A.CR2", "A.DNG", "A.xmp"], &["A.CR2", "A.DNG"]), []);
        assert_eq!(
            owned(
                &["A.CR2", "A.DNG", "A.xmp", "A.CR2.xmp"],
                &["A.CR2", "A.DNG"]
            ),
            [pair("A.CR2", "A.CR2.xmp")]
        );
    }

    #[test]
    fn case_is_ignored_and_each_folder_has_its_own_stems() {
        assert_eq!(
            owned(
                &["d/IMG_1.JPG", "d/img_1.XMP", "e/img_1.jpg"],
                &["d/IMG_1.JPG", "e/img_1.jpg"]
            ),
            [pair("d/IMG_1.JPG", "d/img_1.XMP")]
        );
    }

    #[test]
    fn an_xmp_with_no_photo_and_a_photo_with_no_xmp_are_nothing() {
        assert_eq!(owned(&["lonely.xmp", "a.jpg"], &["a.jpg"]), []);
        assert_eq!(owned(&["a.jpg"], &["a.jpg"]), []);
    }

    #[test]
    fn folders_of_other_sources_are_not_looked_into() {
        let entries = vec![entry("other/a.xmp"), entry("mine/b.xmp")];
        let skip = vec!["other".to_string()];
        let owned = owners(&entries, &skip, ["other/a.jpg", "mine/b.jpg"]);
        assert!(!owned.contains_key("other/a.jpg"));
        assert!(owned.contains_key("mine/b.jpg"));
    }

    #[test]
    fn two_spellings_of_one_name_give_the_same_answer_whatever_the_listing_order() {
        let a = owned(&["a.jpg", "a.xmp", "A.xmp"], &["a.jpg"]);
        let b = owned(&["a.jpg", "A.xmp", "a.xmp"], &["a.jpg"]);
        assert_eq!(a, b);
    }

    #[test]
    fn the_stat_keeps_the_nanoseconds_and_a_file_is_read_only_when_nothing_is_known() {
        let entries = vec![entry("a.xmp")];
        let xmp = owners(&entries, &[], ["a.jpg"]).remove("a.jpg").unwrap();
        assert_eq!(xmp.stat.size, 10);
        assert_eq!(xmp.stat.modified_ns, Some(1_700_000_000_000_000_123));
        assert!(needs_reading(None, &xmp));
        let known = ExternalKnown {
            path: "a.xmp".into(),
            stat: xmp.stat,
            pending: None,
        };
        assert!(!needs_reading(Some(&known), &xmp));
        let other = ExternalKnown {
            path: "a.ARW.xmp".into(),
            ..known
        };
        assert!(
            needs_reading(Some(&other), &xmp),
            "a different file is tracked now"
        );
    }
}
