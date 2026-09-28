// SPDX-License-Identifier: GPL-3.0-or-later
//! What the similar-photo suggestions read and write (WP9, D-105): each photo's 64-bit perceptual hash (of its
//! thumbnail, made by the thumbnail workers and written by the engine), the photos that still lack one, and the
//! photos near another in time that have one. Nearness in hash is the engine's to judge (SQLite has no popcount);
//! near in time is what makes the set small enough to judge in memory.

use auroraw_types::{PhotoId, SeriesId};
use rusqlite::{OptionalExtension, params};

use crate::error::{CatalogueError, Result};
#[cfg(test)]
use crate::open::CURRENT_SCHEMA;
use crate::open::Catalogue;

/// A photo near another in time, with its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarCandidate {
    /// The photo.
    pub id: PhotoId,
    /// Its perceptual hash.
    pub phash: u64,
    /// Seconds since the Unix epoch.
    pub capture_time: i64,
    /// The series it is in, if any.
    pub series_id: Option<SeriesId>,
}

/// A photo's own hash, time and series, and the hashed photos taken near it (not in a resolved series).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarSet {
    /// The photo's hash.
    pub reference: u64,
    /// When it was taken.
    pub capture_time: i64,
    /// The series it is in, if any.
    pub series_id: Option<SeriesId>,
    /// The others.
    pub candidates: Vec<SimilarCandidate>,
}

// SQLite stores an integer as a signed 64-bit number: the hash's bits go in and come out unchanged.
fn to_sql(hash: u64) -> i64 {
    i64::from_ne_bytes(hash.to_ne_bytes())
}

fn from_sql(value: i64) -> u64 {
    u64::from_ne_bytes(value.to_ne_bytes())
}

impl Catalogue {
    /// Records a photo's perceptual hash.
    pub fn apply_phash(&mut self, photo_id: &PhotoId, phash: u64) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE photo SET phash = ?1 WHERE id = ?2",
            params![to_sql(phash), photo_id.to_string()],
        )?;
        if changed == 0 {
            return Err(CatalogueError::NotFound {
                kind: "photo",
                id: photo_id.to_string(),
            });
        }
        Ok(())
    }

    /// A photo's perceptual hash, `None` while it has none (or the photo is not there).
    pub fn phash_of(&self, photo_id: &PhotoId) -> Result<Option<u64>> {
        let value: Option<Option<i64>> = self
            .conn
            .query_row(
                "SELECT phash FROM photo WHERE id = ?1",
                [photo_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value.flatten().map(from_sql))
    }

    /// Up to `limit` photos that have no hash yet (a photo with no file to make a thumbnail of is one of them for good:
    /// the workers give up on it).
    pub fn unhashed(&self, limit: usize) -> Result<Vec<PhotoId>> {
        let mut statement = self.conn.prepare(
            "SELECT id FROM photo WHERE phash IS NULL ORDER BY capture_time DESC LIMIT ?1",
        )?;
        let ids = statement
            .query_map([limit as i64], |r| r.get::<_, String>(0))?
            .filter_map(|id| id.ok()?.parse().ok())
            .collect();
        Ok(ids)
    }

    /// How many photos have no hash yet.
    pub fn unhashed_count(&self) -> Result<usize> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM photo WHERE phash IS NULL", [], |r| {
                    r.get(0)
                })?;
        Ok(count as usize)
    }

    /// The photo's hash and the hashed photos taken within `window_secs` of it (either side), leaving out the photos of a
    /// resolved series (they are settled). `None` when the photo has no hash yet or no capture time.
    pub fn similar_set(&self, photo_id: &PhotoId, window_secs: i64) -> Result<Option<SimilarSet>> {
        let reference: Option<(Option<i64>, i64, Option<String>)> = self
            .conn
            .query_row(
                "SELECT phash, capture_time, series_id FROM photo WHERE id = ?1",
                [photo_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((Some(phash), capture_time, series)) = reference else {
            return Ok(None);
        };
        if capture_time <= 0 {
            return Ok(None);
        }
        let mut statement = self.conn.prepare(
            "SELECT p.id, p.phash, p.capture_time, p.series_id
             FROM photo p LEFT JOIN series s ON s.id = p.series_id
             WHERE p.phash IS NOT NULL AND p.id != ?1 AND p.capture_time BETWEEN ?2 AND ?3
               AND (s.resolved IS NULL OR s.resolved = 0)",
        )?;
        let candidates = statement
            .query_map(
                params![
                    photo_id.to_string(),
                    capture_time - window_secs,
                    capture_time + window_secs
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )?
            .filter_map(|row| {
                let (id, hash, time, series) = row.ok()?;
                Some(SimilarCandidate {
                    id: id.parse().ok()?,
                    phash: from_sql(hash),
                    capture_time: time,
                    series_id: series.and_then(|s| s.parse().ok()),
                })
            })
            .collect();
        Ok(Some(SimilarSet {
            reference: from_sql(phash),
            capture_time,
            series_id: series.and_then(|s| s.parse().ok()),
            candidates,
        }))
    }
}

#[cfg(test)]
mod tests {
    use auroraw_types::WorkspaceId;

    use super::*;

    fn insert(cat: &Catalogue, id: PhotoId, time: i64, series: Option<&str>) {
        cat.conn
            .execute(
                "INSERT INTO photo(id, filename, capture_time, sidecar_size, series_id) VALUES (?1, 'a.jpg', ?2, 1, ?3)",
                params![id.to_string(), time, series],
            )
            .unwrap();
    }

    fn series(cat: &Catalogue, id: SeriesId, resolved: bool) {
        cat.conn
            .execute(
                "INSERT INTO series(id, cover_photo_id, kind, resolved, sidecar_size) VALUES (?1, NULL, 'manual', ?2, 1)",
                params![id.to_string(), resolved],
            )
            .unwrap();
    }

    #[test]
    fn a_hash_round_trips_with_every_bit_including_the_top_one() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let id = PhotoId::random();
        insert(&cat, id, 100, None);
        assert_eq!(cat.phash_of(&id).unwrap(), None);
        cat.apply_phash(&id, u64::MAX - 5).unwrap();
        assert_eq!(cat.phash_of(&id).unwrap(), Some(u64::MAX - 5));
        assert!(matches!(
            cat.apply_phash(&PhotoId::random(), 1),
            Err(CatalogueError::NotFound { .. })
        ));
    }

    #[test]
    fn the_photos_without_a_hash_are_listed_and_counted() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (a, b, c) = (PhotoId::random(), PhotoId::random(), PhotoId::random());
        for (n, id) in [a, b, c].into_iter().enumerate() {
            insert(&cat, id, 100 + n as i64, None);
        }
        assert_eq!(cat.unhashed_count().unwrap(), 3);
        cat.apply_phash(&b, 7).unwrap();
        assert_eq!(cat.unhashed_count().unwrap(), 2);
        let mut listed = cat.unhashed(10).unwrap();
        listed.sort_by_key(|id| id.to_string());
        let mut expected = vec![a, c];
        expected.sort_by_key(|id| id.to_string());
        assert_eq!(listed, expected);
        assert_eq!(cat.unhashed(1).unwrap().len(), 1);
    }

    #[test]
    fn the_similar_set_is_the_hashed_photos_within_the_window_but_not_those_of_a_resolved_series() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (open, settled) = (SeriesId::random(), SeriesId::random());
        series(&cat, open, false);
        series(&cat, settled, true);
        let ids: Vec<PhotoId> = (0..7).map(|_| PhotoId::random()).collect();
        let (me, near, far, in_open, in_settled, unhashed, no_time) =
            (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5], ids[6]);
        insert(&cat, me, 10_000, None);
        insert(&cat, near, 10_000 + 600, None);
        insert(&cat, far, 10_000 + 7_200, None);
        insert(&cat, in_open, 10_000 - 60, Some(&open.to_string()));
        insert(&cat, in_settled, 10_000 + 30, Some(&settled.to_string()));
        insert(&cat, unhashed, 10_000 + 10, None);
        insert(&cat, no_time, 0, None);
        for (n, id) in [me, near, far, in_open, in_settled, no_time]
            .into_iter()
            .enumerate()
        {
            cat.apply_phash(&id, n as u64).unwrap();
        }
        let set = cat.similar_set(&me, 1_800).unwrap().unwrap();
        assert_eq!(set.reference, 0);
        let mut listed: Vec<PhotoId> = set.candidates.iter().map(|c| c.id).collect();
        listed.sort_by_key(|id| id.to_string());
        let mut expected = vec![near, in_open];
        expected.sort_by_key(|id| id.to_string());
        assert_eq!(listed, expected, "near in time, hashed, not settled");
        let member = set.candidates.iter().find(|c| c.id == in_open).unwrap();
        assert_eq!(member.series_id, Some(open));
        // No hash, or no capture time, no set.
        assert_eq!(cat.similar_set(&unhashed, 1_800).unwrap(), None);
        assert_eq!(cat.similar_set(&no_time, 1_800).unwrap(), None);
    }

    #[test]
    fn a_version_1_catalogue_gets_the_column_when_it_is_opened() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("old.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            // Made as schema 1 would have it: none of phash, location or synonyms exist yet.
            cat.conn
                .execute_batch(
                    "ALTER TABLE photo DROP COLUMN phash; DROP TABLE location;
                     ALTER TABLE keyword DROP COLUMN synonyms;",
                )
                .unwrap();
            cat.conn.pragma_update(None, "user_version", 1).unwrap();
            insert(&cat, PhotoId::random(), 5, None);
        }
        let mut cat = Catalogue::open(&path).unwrap();
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA);
        assert_eq!(
            cat.unhashed_count().unwrap(),
            1,
            "the photo it had is there, unhashed"
        );
        let id = cat.unhashed(1).unwrap()[0];
        cat.apply_phash(&id, 9).unwrap();
        // Opened again, nothing more happens.
        drop(cat);
        assert_eq!(Catalogue::open(&path).unwrap().unhashed_count().unwrap(), 0);
    }
}
