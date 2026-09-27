// SPDX-License-Identifier: GPL-3.0-or-later
//! Turning paired photos into a plan: a sequence number per photo (several cameras sorted
//! together by capture time, M1 plan §6 item 6), a rendered destination for each file, and a
//! unique suffix when two files would land on the same path -- the case that same-numbered shots
//! from two different cameras produce when a template is built from the original name, which is
//! exactly the "clashing numbers" case the plan text calls out by name.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::discover::stem_and_extension;
use crate::pair::PhotoGroup;
use crate::template::{TemplateContext, render, safe_relative};

/// One file of a planned photo: where it is now, and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFile {
    /// Its path inside the source.
    pub source_path: String,
    /// Its size, from the source listing.
    pub size: u64,
    /// Where it lands, relative to the destination root.
    pub destination: PathBuf,
    /// Where its verified backup copies land, each relative to its own root.
    pub backup_destinations: Vec<PathBuf>,
}

/// One photo, planned: the file that is developed, and its companion if it has one, both with
/// their destinations already decided.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPhoto {
    /// The file that is developed.
    pub original: PlannedFile,
    /// Its companion, if the photo has one.
    pub companion: Option<PlannedFile>,
}

/// The paths (folded to lower case, see `folded`) this run already assigned in the destination, so the next collision (two cameras'
/// same-numbered shots rendering to the same destination) gets a suffix instead of silently
/// landing on top of it. The caller may pre-seed this too; files already on disk are the job of
/// [`Exists`], which is asked about every candidate.
pub type UsedPaths = HashSet<PathBuf>;

/// Which folder a planned path is relative to, for asking whether something is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// The archive the photos are imported into.
    Destination,
    /// The n-th backup folder, in the profile's own order.
    Backup(usize),
}

/// Whether a file already exists at a planned path (relative to `Root`). An import never
/// overwrites one: a file left by an earlier import, or by anything else, gets the newcomer a
/// suffix instead (spec §5.2, D-031's spirit: an import must not destroy what is already kept).
pub type Exists<'a> = &'a dyn Fn(Root, &Path) -> bool;

/// A path folded for comparison: two names that differ only by case are the same name on Windows
/// and macOS, so they are never given to two files (the names themselves keep their case).
fn folded(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

fn unique(mut path: PathBuf, used: &mut UsedPaths, root: Root, exists: Exists) -> PathBuf {
    let taken = |p: &Path, used: &UsedPaths| used.contains(&folded(p)) || exists(root, p);
    if !taken(&path, used) {
        used.insert(folded(&path));
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut n = 2u32;
    loop {
        let name = match &ext {
            Some(ext) => format!("{stem}_{n}.{ext}"),
            None => format!("{stem}_{n}"),
        };
        let candidate = parent.join(name);
        if !taken(&candidate, used) {
            used.insert(folded(&candidate));
            path = candidate;
            break;
        }
        n += 1;
    }
    path
}

/// The templates and the names already taken, for planning one file at a time.
struct Namer<'a> {
    destination_template: &'a str,
    backup_templates: &'a [String],
    used: &'a mut UsedPaths,
    backup_used: Vec<UsedPaths>,
    exists: Exists<'a>,
}

impl Namer<'_> {
    fn planned_file(&mut self, source_path: &str, size: u64, ctx: &TemplateContext) -> PlannedFile {
        let fallback = render("{original}.{ext}", ctx);
        let destination = unique(
            safe_relative(&render(self.destination_template, ctx), &fallback),
            self.used,
            Root::Destination,
            self.exists,
        );
        let exists = self.exists;
        let backup_destinations = self
            .backup_templates
            .iter()
            .zip(self.backup_used.iter_mut())
            .enumerate()
            .map(|(i, (t, used))| {
                let relative = safe_relative(&render(t, ctx), &fallback);
                unique(relative, used, Root::Backup(i), exists)
            })
            .collect();
        PlannedFile {
            source_path: source_path.to_string(),
            size,
            destination,
            backup_destinations,
        }
    }
}

/// Plans every group: sorted by capture time (ties broken by source path, so the order is
/// deterministic even when two cameras agree to the second), numbered from 1, each file's
/// destination rendered from the templates and made unique against `used` and against what
/// `exists` says is already on disk. Each backup root has its own namespace: a backup laid out
/// like the archive must land on the same relative path, not be pushed to a suffix by it.
pub fn plan(
    mut groups: Vec<PhotoGroup>,
    destination_template: &str,
    backup_templates: &[String],
    used: &mut UsedPaths,
    exists: Exists,
) -> Vec<PlannedPhoto> {
    let mut namer = Namer {
        destination_template,
        backup_templates,
        used,
        backup_used: vec![UsedPaths::new(); backup_templates.len()],
        exists,
    };
    groups.sort_by(|a, b| {
        a.original
            .capture_time
            .cmp(&b.original.capture_time)
            .then_with(|| a.original.path.cmp(&b.original.path))
    });

    let mut out = Vec::with_capacity(groups.len());
    for (i, group) in groups.into_iter().enumerate() {
        let sequence = i as u32 + 1;
        let (stem, ext) = stem_and_extension(&group.original.path);
        let ctx = TemplateContext {
            capture_time: group.original.capture_time,
            sequence,
            camera: group.original.camera.as_deref(),
            original_stem: stem,
            extension: ext,
            source_path: &group.original.path,
        };
        let original = namer.planned_file(&group.original.path, group.original.size, &ctx);
        let companion = group.companion.map(|c| {
            let (stem, ext) = stem_and_extension(&c.path);
            let ctx = TemplateContext {
                original_stem: stem,
                extension: ext,
                source_path: &c.path,
                ..ctx
            };
            namer.planned_file(&c.path, c.size, &ctx)
        });
        out.push(PlannedPhoto {
            original,
            companion,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::DiscoveredFile;
    use time::macros::datetime;

    fn file(path: &str, camera: &str, when: time::OffsetDateTime) -> DiscoveredFile {
        DiscoveredFile {
            path: path.to_string(),
            size: 10,
            capture_time: Some(when),
            camera: Some(camera.to_string()),
        }
    }

    fn group(f: DiscoveredFile) -> PhotoGroup {
        PhotoGroup {
            original: f,
            companion: None,
        }
    }

    #[test]
    fn sequence_numbers_follow_capture_time_across_cameras() {
        let a = file(
            "A/IMG_0002.CR3",
            "Camera A",
            datetime!(2026-01-01 10:00 UTC),
        );
        let b = file("B/IMG_0001.CR3", "Camera B", datetime!(2026-01-01 9:00 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{seq:02}_{original}.{ext}",
            &[],
            &mut used,
            &|_, _| false,
        );
        // B was captured first, despite sorting after A alphabetically and having a lower own
        // camera-assigned number.
        assert_eq!(planned[0].original.source_path, "B/IMG_0001.CR3");
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("01_IMG_0001.CR3")
        );
        assert_eq!(planned[1].original.source_path, "A/IMG_0002.CR3");
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("02_IMG_0002.CR3")
        );
    }

    #[test]
    fn two_cameras_with_the_same_number_get_a_unique_suffix() {
        let a = file("A/IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let b = file("B/IMG_0001.CR3", "Camera B", datetime!(2026-01-01 9:05 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{original}.{ext}",
            &[],
            &mut used,
            &|_, _| false,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001.CR3")
        );
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("IMG_0001_2.CR3"),
            "the second file to land on the same path gets a suffix, not silently overwritten"
        );
    }

    #[test]
    fn a_path_already_used_before_planning_starts_is_also_avoided() {
        let a = file("A/IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let mut used = UsedPaths::new();
        used.insert(PathBuf::from("img_0001.cr3"));
        let planned = plan(
            vec![group(a)],
            "{original}.{ext}",
            &[],
            &mut used,
            &|_, _| false,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001_2.CR3")
        );
    }

    #[test]
    fn a_companion_renders_its_own_extension_at_the_same_sequence() {
        let mut raw = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let jpeg = file("IMG_0001.JPG", "Camera A", datetime!(2026-01-01 9:00 UTC));
        raw.path = "IMG_0001.CR3".into();
        let g = PhotoGroup {
            original: raw,
            companion: Some(jpeg),
        };
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![g],
            "{seq:02}_{original}.{ext}",
            &[],
            &mut used,
            &|_, _| false,
        );
        assert_eq!(
            planned[0].companion.as_ref().unwrap().destination,
            PathBuf::from("01_IMG_0001.JPG")
        );
    }

    #[test]
    fn backup_destinations_are_rendered_and_made_unique_too() {
        let a = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let b = file("IMG_0002.CR3", "Camera A", datetime!(2026-01-01 9:01 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{original}.{ext}",
            &["backup/{original}.{ext}".to_string()],
            &mut used,
            &|_, _| false,
        );
        assert_eq!(
            planned[0].original.backup_destinations,
            vec![PathBuf::from("backup/IMG_0001.CR3")]
        );
        assert_eq!(
            planned[1].original.backup_destinations,
            vec![PathBuf::from("backup/IMG_0002.CR3")]
        );
    }

    #[test]
    fn a_file_already_on_disk_is_never_overwritten_the_newcomer_gets_a_suffix() {
        let a = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let on_disk = |root: Root, p: &Path| {
            root == Root::Destination
                && (p == Path::new("IMG_0001.CR3") || p == Path::new("IMG_0001_2.CR3"))
        };
        let planned = plan(
            vec![group(a)],
            "{original}.{ext}",
            &[],
            &mut UsedPaths::new(),
            &on_disk,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001_3.CR3")
        );
    }

    #[test]
    fn a_backup_laid_out_like_the_archive_lands_on_the_same_relative_path() {
        let a = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let planned = plan(
            vec![group(a)],
            "{original}.{ext}",
            &["{original}.{ext}".to_string()],
            &mut UsedPaths::new(),
            &|_, _| false,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001.CR3")
        );
        assert_eq!(
            planned[0].original.backup_destinations,
            vec![PathBuf::from("IMG_0001.CR3")],
            "a different root, its own namespace"
        );
    }

    #[test]
    fn two_names_that_differ_only_by_case_never_share_a_destination_and_each_keeps_its_own_case() {
        let a = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let b = file(
            "copy/img_0001.cr3",
            "Camera B",
            datetime!(2026-01-01 9:05 UTC),
        );
        let planned = plan(
            vec![group(a), group(b)],
            "{name}",
            &[],
            &mut UsedPaths::new(),
            &|_, _| false,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001.CR3")
        );
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("img_0001_2.cr3"),
            "the newcomer is numbered, in its own case"
        );
    }

    #[test]
    fn keeping_the_cards_folders_merges_into_a_folder_that_exists_and_numbers_only_a_clash() {
        let a = file(
            "DCIM/100CANON/IMG_0001.CR2",
            "Camera A",
            datetime!(2026-01-01 9:00 UTC),
        );
        let b = file(
            "DCIM/100CANON/IMG_0002.CR2",
            "Camera A",
            datetime!(2026-01-01 9:01 UTC),
        );
        // `100CANON/IMG_0001.CR2` is already in the archive from an earlier card: the folder is
        // merged into, and only the file that clashes gets a number.
        let on_disk = |root: Root, p: &Path| {
            root == Root::Destination && p == Path::new("100CANON/IMG_0001.CR2")
        };
        let planned = plan(
            vec![group(a), group(b)],
            "{path}",
            &[],
            &mut UsedPaths::new(),
            &on_disk,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("100CANON/IMG_0001_2.CR2")
        );
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("100CANON/IMG_0002.CR2")
        );
    }
}
