// SPDX-License-Identifier: GPL-3.0-or-later
//! The similar-photo suggestions (WP9, D-034, D-105): which photos look like one, offered to the photographer and
//! never grouped by themselves. Read-only, like `sources_api`: the hashes are made by the thumbnail workers and
//! written by the coordinator; this only reads them and judges. A photo is similar to another when its hash is within
//! `max_distance` bits of it and it was taken within `window_secs` of it (the window is what keeps two like scenes of
//! different days apart, and what keeps the set small enough to judge in memory).

use auroraw_catalogue::SimilarSet;
use auroraw_types::PhotoId;

use crate::Engine;
use crate::error::Result;

/// How close is close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimilarQuery {
    /// At most this many of the hash's 64 bits differ.
    pub max_distance: u32,
    /// At most this many seconds between the two photos.
    pub window_secs: i64,
    /// At most this many photos are given back.
    pub limit: usize,
}

impl Default for SimilarQuery {
    fn default() -> Self {
        Self {
            max_distance: 10,
            window_secs: 30 * 60,
            limit: 24,
        }
    }
}

/// A photo that looks like another, and how far apart the two hashes are (0 to 64 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimilarPhoto {
    /// The photo.
    pub id: PhotoId,
    /// The Hamming distance of its hash to the other's.
    pub distance: u32,
}

/// A hash with almost no bit set, or almost all, is a flat or empty picture (every bit compares a pixel with its
/// neighbour): two unrelated flat pictures would look alike. Such a hash says nothing.
fn informative(hash: u64) -> bool {
    (6..=58).contains(&hash.count_ones())
}

/// The photos of `set` that look like its reference: informative hashes within the distance, not in the reference's own
/// series (they are grouped already), nearest first, then the nearest in time, then by identifier.
pub(crate) fn rank(set: &SimilarSet, query: &SimilarQuery) -> Vec<SimilarPhoto> {
    if !informative(set.reference) {
        return Vec::new();
    }
    let mut found: Vec<(u32, i64, SimilarPhoto)> = set
        .candidates
        .iter()
        .filter(|c| informative(c.phash))
        .filter(|c| set.series_id.is_none() || c.series_id != set.series_id)
        .filter(|c| (c.capture_time - set.capture_time).abs() <= query.window_secs)
        .filter_map(|c| {
            let distance = (c.phash ^ set.reference).count_ones();
            (distance <= query.max_distance).then_some((
                distance,
                (c.capture_time - set.capture_time).abs(),
                SimilarPhoto { id: c.id, distance },
            ))
        })
        .collect();
    found.sort_by_key(|(distance, apart, photo)| (*distance, *apart, photo.id.to_string()));
    found.truncate(query.limit);
    found.into_iter().map(|(_, _, photo)| photo).collect()
}

impl Engine {
    /// The photos that look like `photo`, nearest first. Empty while the photo has no hash yet (see
    /// [`Self::unhashed_count`]), or no capture time.
    pub fn similar_photos(&self, photo: PhotoId, query: SimilarQuery) -> Result<Vec<SimilarPhoto>> {
        let catalogue = self.read_catalogue()?;
        Ok(catalogue
            .similar_set(&photo, query.window_secs)?
            .map(|set| rank(&set, &query))
            .unwrap_or_default())
    }

    /// How many photos have no perceptual hash yet: the suggestions are not complete while it is not 0.
    pub fn unhashed_count(&self) -> Result<usize> {
        Ok(self.read_catalogue()?.unhashed_count()?)
    }
}

#[cfg(test)]
mod tests {
    use auroraw_catalogue::SimilarCandidate;
    use auroraw_types::SeriesId;

    use super::*;

    /// A hash with `bits` set, low bits first (16 bits in are plenty informative).
    fn hash(bits: u32) -> u64 {
        (1u64 << bits) - 1
    }

    fn candidate(phash: u64, time: i64, series: Option<SeriesId>) -> SimilarCandidate {
        SimilarCandidate {
            id: PhotoId::random(),
            phash,
            capture_time: time,
            series_id: series,
        }
    }

    fn set(reference: u64, candidates: Vec<SimilarCandidate>) -> SimilarSet {
        SimilarSet {
            reference,
            capture_time: 1_000,
            series_id: None,
            candidates,
        }
    }

    #[test]
    fn the_nearest_hash_comes_first_then_the_nearest_in_time() {
        let base = hash(20);
        let far = candidate(base ^ 0b111_1111, 1_000, None); // 7 bits
        let near_late = candidate(base ^ 0b11, 1_500, None); // 2 bits, 500 s away
        let near_early = candidate(base ^ 0b11, 1_100, None); // 2 bits, 100 s away
        let same = candidate(base, 1_900, None); // 0 bits
        let ranked = rank(
            &set(
                base,
                vec![
                    far.clone(),
                    near_late.clone(),
                    near_early.clone(),
                    same.clone(),
                ],
            ),
            &SimilarQuery::default(),
        );
        let ids: Vec<PhotoId> = ranked.iter().map(|p| p.id).collect();
        assert_eq!(ids, [same.id, near_early.id, near_late.id, far.id]);
        assert_eq!(
            ranked.iter().map(|p| p.distance).collect::<Vec<_>>(),
            [0, 2, 2, 7]
        );
    }

    #[test]
    fn the_distance_and_the_window_are_limits() {
        let base = hash(20);
        let close = candidate(base ^ 1, 1_000, None);
        let too_far_in_bits = candidate(base ^ 0xFFF, 1_000, None); // 12 bits
        let too_far_in_time = candidate(base, 1_000 + 4_000, None);
        let query = SimilarQuery {
            max_distance: 10,
            window_secs: 1_800,
            limit: 24,
        };
        let ranked = rank(
            &set(base, vec![close.clone(), too_far_in_bits, too_far_in_time]),
            &query,
        );
        assert_eq!(ranked.iter().map(|p| p.id).collect::<Vec<_>>(), [close.id]);
        let wider = SimilarQuery {
            max_distance: 12,
            window_secs: 5_000,
            limit: 2,
        };
        let more = rank(
            &set(
                base,
                vec![
                    close.clone(),
                    candidate(base ^ 0xFFF, 1_000, None),
                    candidate(base, 5_000, None),
                ],
            ),
            &wider,
        );
        assert_eq!(more.len(), 2, "the limit cuts the list");
    }

    #[test]
    fn a_photos_own_series_is_left_out_and_flat_pictures_say_nothing() {
        let base = hash(20);
        let series = SeriesId::random();
        let mut reference = set(
            base,
            vec![
                candidate(base, 1_000, Some(series)),
                candidate(base, 1_000, Some(SeriesId::random())),
                candidate(0, 1_000, None),
            ],
        );
        reference.series_id = Some(series);
        let ranked = rank(&reference, &SimilarQuery::default());
        assert_eq!(
            ranked.len(),
            1,
            "another series and a plain photo would count; one of its own does not"
        );
        // A reference that is flat suggests nothing, nor does it match another flat picture.
        assert!(
            rank(
                &set(0, vec![candidate(0, 1_000, None)]),
                &SimilarQuery::default()
            )
            .is_empty()
        );
    }
}
