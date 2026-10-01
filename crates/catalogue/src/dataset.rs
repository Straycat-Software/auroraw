// SPDX-License-Identifier: GPL-3.0-or-later
//! A synthetic but realistic dataset: shoots of a few hundred photos, a hierarchical keyword
//! vocabulary, bursts that form series, several versions on some photos, a couple of collections.
//! Moved here from spike 3 (docs/spikes/03-catalogue-and-grid.md), for this crate's own tests and
//! for WP3 and later milestones' tests and benchmarks.

use auroraw_format::sidecar::{
    FileEntry, FileRole, Flag, Location, OverrideField, PhotoSidecar, VersionSidecar,
};
use auroraw_format::state::{Collection, KeywordEntry, Series, SourceEntry};
use auroraw_types::{
    CollectionId, ContentHash, Fingerprint, KeywordId, MemberRef, PhotoId, SeriesId, SourceId,
    Timestamp, VersionId,
};

use crate::SidecarStat;

const CAMERAS: &[(&str, &str)] = &[
    ("SONY", "ILCE-7RM4"),
    ("Canon", "EOS R5 Mark II"),
    ("Nikon", "Z8"),
    ("FUJIFILM", "X-T50"),
    ("Panasonic", "DC-S5"),
    ("OLYMPUS", "E-M5 Mark III"),
];
const LENSES: &[&str] = &[
    "FE 24-70mm F2.8 GM",
    "RF 70-200mm F2.8L",
    "NIKKOR Z 50mm f/1.8 S",
    "XF 56mm F1.2 R",
    "LUMIX S 24-105mm F4",
];
const CATEGORIES: &[(&str, &[&str])] = &[
    ("Fauna", &["Birds", "Mammals"]),
    ("Places", &["Canada", "Iceland"]),
    ("People", &["Family", "Friends"]),
    ("Events", &["Weddings", "Travel"]),
];
const LEAVES: &[&str] = &[
    "Heron", "Fox", "Lake", "Mountain", "Marie", "Paul", "Wedding", "Hike", "Sunset", "Portrait",
];

/// A generated catalogue's worth of domain data, ready for [`crate::RebuildInput`].
pub struct Dataset {
    /// The photo sidecars, each with the stat its bytes would have on disk.
    pub photos: Vec<(PhotoSidecar, SidecarStat)>,
    /// The version sidecars.
    pub versions: Vec<(VersionSidecar, SidecarStat)>,
    /// The keyword vocabulary.
    pub vocabulary: Vec<KeywordEntry>,
    /// The sources.
    pub sources: Vec<SourceEntry>,
    /// The collections.
    pub collections: Vec<(Collection, SidecarStat)>,
    /// The series.
    pub series: Vec<(Series, SidecarStat)>,
}

impl Dataset {
    /// Puts the photos in places, for the place filter's tests and measurements (design note 008 §5): about two
    /// photos in three get a place, from a world of 30 countries (a few without regions, a few with a code only), up to
    /// 8 regions each and up to 20 cities a region, a few countries having most of the photos; one place in ten is
    /// written in capitals, so that the filter has spellings to fold. A generator of its own, so that the rest of the
    /// dataset (and every test that depends on it) is the same with or without places.
    pub fn place_photos(&mut self, seed: u64) {
        let mut r = fastrand::Rng::with_seed(seed ^ 0x0070_6c61_6365);
        for (photo, stat) in &mut self.photos {
            if r.u8(0..3) == 0 {
                continue;
            }
            // A few countries have most of the photos.
            let country = r.usize(0..30).min(r.usize(0..30));
            let regions = country % 9; // 0 to 8: some countries have none
            let region = (regions > 0).then(|| r.usize(0..regions));
            let city = r.usize(0..20);
            let shout = |text: String, r: &mut fastrand::Rng| {
                if r.u8(0..10) == 0 {
                    text.to_uppercase()
                } else {
                    text
                }
            };
            let m = &mut photo.meta;
            m.country = Some(shout(format!("Country {country:02}"), &mut r));
            m.country_code = (!country.is_multiple_of(7)).then(|| format!("C{country:02}"));
            m.region = region.map(|region| shout(format!("Région {country:02}-{region}"), &mut r));
            m.city = Some(shout(
                format!("Cité {country:02}-{}-{city}", region.unwrap_or(0)),
                &mut r,
            ));
            *stat = stat_of(&photo.to_bytes());
        }
    }

    /// A [`crate::RebuildInput`] borrowing this dataset.
    pub fn as_rebuild_input(&self) -> crate::RebuildInput<'_> {
        crate::RebuildInput {
            photos: &self.photos,
            versions: &self.versions,
            vocabulary: &self.vocabulary,
            sources: &self.sources,
            collections: &self.collections,
            series: &self.series,
        }
    }
}

fn stat_of(bytes: &[u8]) -> SidecarStat {
    SidecarStat {
        size: bytes.len() as u64,
        modified: Some(1_790_000_000),
    }
}

/// Identifiers are meant to be truly random in production (note 001 §5.2), so their own
/// `random()` draws from the operating system, not from a seed. A **generator** needs the
/// opposite: the whole dataset, identifiers included, reproducible from `seed` alone (so a test
/// failure can be replayed and a fixture regenerated identically). These draw bytes from the
/// seeded `fastrand::Rng` instead.
macro_rules! seeded_id {
    ($r:expr, $ty:ty, $n:literal) => {{
        let mut bytes = [0u8; $n];
        bytes.iter_mut().for_each(|b| *b = $r.u8(..));
        <$ty>::from_bytes(bytes)
    }};
}

fn keyword_tree(r: &mut fastrand::Rng) -> Vec<KeywordEntry> {
    let mut vocabulary = Vec::new();
    for (category, groups) in CATEGORIES {
        let category_id = seeded_id!(r, KeywordId, 8);
        vocabulary.push(KeywordEntry {
            id: category_id,
            name: category.to_string(),
            parent: None,
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        });
        for group in *groups {
            let group_id = seeded_id!(r, KeywordId, 8);
            vocabulary.push(KeywordEntry {
                id: group_id,
                name: group.to_string(),
                parent: Some(category_id),
                synonyms: vec![],
                export: true,
                extra: Default::default(),
            });
            for leaf in LEAVES {
                vocabulary.push(KeywordEntry {
                    id: seeded_id!(r, KeywordId, 8),
                    name: leaf.to_string(),
                    parent: Some(group_id),
                    synonyms: vec![],
                    export: !leaf.eq_ignore_ascii_case("Marie")
                        && !leaf.eq_ignore_ascii_case("Paul"),
                    extra: Default::default(),
                });
            }
        }
    }
    vocabulary
}

/// Full paths (`"Category|Group|Leaf"`) of every leaf keyword, for assigning realistic keyword
/// sets to photos without duplicating the tree-walk this crate does for real (`rebuild.rs`).
fn leaf_paths(vocabulary: &[KeywordEntry]) -> Vec<(KeywordId, String)> {
    let by_id: std::collections::HashMap<_, _> = vocabulary.iter().map(|k| (k.id, k)).collect();
    let has_children: std::collections::HashSet<_> =
        vocabulary.iter().filter_map(|k| k.parent).collect();
    vocabulary
        .iter()
        .filter(|k| !has_children.contains(&k.id))
        .map(|leaf| {
            let mut segments = vec![leaf.name.as_str()];
            let mut current = leaf.parent;
            while let Some(id) = current {
                let parent = &by_id[&id];
                segments.push(parent.name.as_str());
                current = parent.parent;
            }
            segments.reverse();
            (leaf.id, segments.join("|"))
        })
        .collect()
}

/// Generates a dataset of about `photo_count` photos, deterministically from `seed`.
pub fn generate(photo_count: usize, seed: u64) -> Dataset {
    let mut r = fastrand::Rng::with_seed(seed);
    let vocabulary = keyword_tree(&mut r);
    let leaves = leaf_paths(&vocabulary);

    let sources: Vec<SourceEntry> = (0..3)
        .map(|i| SourceEntry {
            id: seeded_id!(r, SourceId, 8),
            kind: "local-folder".into(),
            name: format!("Archive {i}"),
            hint: Default::default(),
            ignore: vec![],
            config: Default::default(),
            extra: Default::default(),
        })
        .collect();

    let mut photos = Vec::with_capacity(photo_count);
    let mut versions = Vec::new();
    let mut series = Vec::new();
    let mut collection_members = Vec::new();

    let mut i = 0;
    while i < photo_count {
        // A "shoot": a run of 1 to 12 photos sharing a camera and close capture times, some of
        // which form a burst (a series).
        let shoot_len = (r.usize(1..=12)).min(photo_count - i);
        let (make, model) = CAMERAS[r.usize(0..CAMERAS.len())];
        let lens = LENSES[r.usize(0..LENSES.len())];
        let shoot_start = 1_700_000_000i64 + i as i64 * 40;
        let mut shoot_photo_ids = Vec::with_capacity(shoot_len);

        for j in 0..shoot_len {
            let mut photo = PhotoSidecar::new(seeded_id!(r, PhotoId, 16));
            let capture = shoot_start + j as i64 * 2;
            let m = &mut photo.meta;
            m.rating = Some(r.u8(0..=5));
            m.flag = if r.u8(0..10) < 2 {
                Some(Flag::Rejected)
            } else if r.u8(0..10) < 5 {
                Some(Flag::Picked)
            } else {
                None
            };
            m.title = (r.u8(0..5) == 0).then(|| "A photograph".to_string());
            m.caption = (r.u8(0..4) == 0).then(|| "A caption of a shoot".to_string());
            let n_keywords = r.usize(0..=6);
            for _ in 0..n_keywords {
                let (id, path) = &leaves[r.usize(0..leaves.len())];
                if !m.keyword_ids.contains(id) {
                    m.push_keyword(*id, path.clone());
                }
            }
            m.creator = vec!["A Photographer".into()];
            m.original.capture_time = Some(format_time(capture));
            m.original.make = Some(make.to_string());
            m.original.model = Some(model.to_string());
            m.original.lens = Some(lens.to_string());
            m.original.exposure_time = Some(format!("1/{}", 100 + r.u32(0..2000)));
            m.original.f_number = Some(format!("{}/10", 14 + r.u32(0..80)));
            m.original.iso = vec![(50 * (1 << r.u32(0..8))).to_string()];
            m.original.focal_length = Some(format!("{}/10", 240 + r.u32(0..1600)));
            m.original.pixel_width = Some(6000 + r.u32(0..4000));
            m.original.pixel_height = Some(4000 + r.u32(0..2600));

            let source = &sources[r.usize(0..sources.len())];
            photo.files.push(FileEntry {
                role: FileRole::Original,
                name: format!("DSC{:05}.ARW", i + j),
                format: Some("ARW".into()),
                size: 20_000_000 + r.u64(0..40_000_000),
                fingerprint: Fingerprint::from_bytes([r.u8(..); 32]),
                hash: (r.u8(0..3) != 0).then(|| ContentHash::from_bytes([r.u8(..); 32])),
                locations: vec![Location {
                    source: source.id,
                    path: format!("2026/{i}/DSC{:05}.ARW", i + j),
                    seen: Some(Timestamp::from_unix(capture)),
                    extra: vec![],
                }],
                extra: vec![],
            });
            photo.imported = Some(Timestamp::from_unix(capture + 60));

            // 70% one version, 20% two, 10% three; a version sometimes overrides the rating.
            let n_versions = match r.u8(0..10) {
                0..=6 => 1,
                7..=8 => 2,
                _ => 3,
            };
            let mut photo_versions = Vec::with_capacity(n_versions);
            for _ in 0..n_versions {
                let mut v = VersionSidecar::new(&photo, seeded_id!(r, VersionId, 8));
                v.name = Some(if photo_versions.is_empty() {
                    "Default".into()
                } else {
                    "Black and white".into()
                });
                v.created = Some(Timestamp::from_unix(capture + 3600));
                if r.u8(0..4) == 0 {
                    v.overrides = vec![OverrideField::Rating];
                    v.meta.rating = Some(r.u8(0..=5));
                }
                v.refresh_copy(&photo);
                photo_versions.push(v);
            }
            if let Some(first) = photo_versions.first() {
                photo.main_version = Some(first.version_id);
            }

            let bytes = photo.to_bytes();
            let stat = stat_of(&bytes);
            for v in &photo_versions {
                let vbytes = v.to_bytes();
                versions.push((v.clone(), stat_of(&vbytes)));
            }
            shoot_photo_ids.push(photo.photo_id);
            if collection_members.len() < 2000 && r.u8(0..30) == 0 {
                collection_members.push(MemberRef::Photo(photo.photo_id));
            }
            photos.push((photo, stat));
        }

        // About one shoot in six forms a resolved burst series.
        if shoot_len >= 3 && r.u8(0..6) == 0 {
            let cover = shoot_photo_ids[0];
            let kept = vec![cover];
            for id in shoot_photo_ids.iter().skip(1) {
                if let Some((photo, _)) = photos.iter_mut().find(|(p, _)| p.photo_id == *id) {
                    photo.meta.flag = Some(Flag::Rejected);
                }
            }
            series.push((
                Series {
                    id: seeded_id!(r, SeriesId, 8),
                    updated: Timestamp::from_unix(shoot_start),
                    kind: "burst".into(),
                    cover,
                    resolved: true,
                    kept,
                    members: shoot_photo_ids.clone(),
                    extra: Default::default(),
                },
                SidecarStat::default(),
            ));
        }

        i += shoot_len;
    }

    // series stats now that their bytes are final
    let series: Vec<(Series, SidecarStat)> = series
        .into_iter()
        .map(|(s, _)| {
            let bytes = auroraw_format::state::write_state(&s);
            let stat = stat_of(&bytes);
            (s, stat)
        })
        .collect();

    let collections = if collection_members.is_empty() {
        vec![]
    } else {
        let collection = Collection {
            id: seeded_id!(r, CollectionId, 8),
            updated: Timestamp::from_unix(1_700_000_000),
            name: "Picks".into(),
            kind: "manual".into(),
            parent: None,
            members: collection_members,
            query: None,
            query_schema: None,
            extra: Default::default(),
        };
        let bytes = auroraw_format::state::write_state(&collection);
        let stat = stat_of(&bytes);
        vec![(collection, stat)]
    };

    Dataset {
        photos,
        versions,
        vocabulary,
        sources,
        collections,
        series,
    }
}

fn format_time(unix: i64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp(unix).unwrap();
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_dataset() {
        let a = generate(500, 7);
        let b = generate(500, 7);
        assert_eq!(a.photos.len(), b.photos.len());
        assert_eq!(a.photos[0].0.photo_id, b.photos[0].0.photo_id);
    }

    #[test]
    fn it_generates_roughly_the_requested_number_of_photos() {
        let d = generate(1000, 1);
        assert!((900..=1000).contains(&d.photos.len()), "{}", d.photos.len());
        assert!(!d.versions.is_empty());
        assert!(!d.series.is_empty());
        assert!(!d.collections.is_empty());
        assert_eq!(d.sources.len(), 3);
        assert!(d.vocabulary.len() > 20);
    }
}
