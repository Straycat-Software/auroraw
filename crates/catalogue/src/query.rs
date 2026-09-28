// SPDX-License-Identifier: GPL-3.0-or-later
//! The queries the grid and the search panel need: keyset paging (never offset, so a page costs
//! the same at the start or the end of a 100,000-photo catalogue, spike 3), counts, a full-text
//! search, and a lookup by keyword. See `docs/spikes/03-catalogue-and-grid.md` for the budgets.

use std::collections::HashMap;

use auroraw_types::{
    CollectionId, ContentHash, Fingerprint, KeywordId, PhotoId, SeriesId, SourceId,
};
use rusqlite::{OptionalExtension, Row, params};

use crate::error::Result;
use crate::open::Catalogue;

/// One row of the grid: enough to display and sort a photo without a further lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoRow {
    /// The photo's identifier.
    pub id: PhotoId,
    /// Seconds since the Unix epoch (0 if the capture time could not be parsed).
    pub capture_time: i64,
    /// The photo's own rating.
    pub rating: u8,
    /// The effective rating (D-063): the main version's if it overrides one.
    pub effective_rating: u8,
    /// Whether the effective rating differs from the photo's own (the main version overrides it).
    pub rating_overridden: bool,
    /// The photo's own flag (0 none, 1 picked, 2 rejected).
    pub flag: u8,
    /// The effective flag.
    pub effective_flag: u8,
    /// The colour label.
    pub label: Option<String>,
    /// The title.
    pub title: Option<String>,
    /// The caption.
    pub caption: Option<String>,
    /// The original file's name.
    pub filename: String,
    /// The camera name, if known.
    pub camera: Option<String>,
    /// The lens name, if known.
    pub lens: Option<String>,
    /// The series this photo belongs to, if any.
    pub series_id: Option<SeriesId>,
    /// How many versions this photo has.
    pub version_count: u32,
    /// The source this photo's file was last found in, if any.
    pub source_id: Option<SourceId>,
    /// Its path inside that source.
    pub path: Option<String>,
    /// Whether the last reconcile found the file at `path` with a different fingerprint (design
    /// note 004 §6.5).
    pub original_changed: bool,
    /// Whether the last reconcile found no file at all matching this photo in its source.
    pub original_missing: bool,
    /// The ISO speed, if known.
    pub iso: Option<i64>,
    /// The f-number, if known.
    pub aperture: Option<f64>,
    /// The exposure time in seconds, if known.
    pub shutter: Option<f64>,
    /// The focal length in millimetres, if known.
    pub focal_length: Option<f64>,
    /// The original's width in pixels, if known.
    pub width: Option<u32>,
    /// The original's height in pixels, if known.
    pub height: Option<u32>,
}

const COLUMNS: &str = "
    p.id, p.capture_time, p.rating, p.effective_rating, p.rating_overridden, p.flag,
    p.effective_flag, p.label, p.title, p.caption, p.filename, c.name, l.name, p.series_id,
    p.version_count, p.source_id, p.path, p.original_changed, p.original_missing,
    p.iso, p.aperture, p.shutter, p.focal_length, p.width, p.height
";
const FROM: &str =
    "FROM photo p LEFT JOIN camera c ON c.id = p.camera_id LEFT JOIN lens l ON l.id = p.lens_id";

fn photo_row(row: &Row) -> rusqlite::Result<PhotoRow> {
    let id: String = row.get(0)?;
    let series_id: Option<String> = row.get(13)?;
    let source_id: Option<String> = row.get(15)?;
    Ok(PhotoRow {
        id: id.parse().map_err(|_| {
            rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
        })?,
        capture_time: row.get(1)?,
        rating: row.get(2)?,
        effective_rating: row.get(3)?,
        rating_overridden: row.get::<_, i64>(4)? != 0,
        flag: row.get(5)?,
        effective_flag: row.get(6)?,
        label: row.get(7)?,
        title: row.get(8)?,
        caption: row.get(9)?,
        filename: row.get(10)?,
        camera: row.get(11)?,
        lens: row.get(12)?,
        series_id: series_id.and_then(|s| s.parse().ok()),
        version_count: row.get(14)?,
        source_id: source_id.and_then(|s| s.parse().ok()),
        path: row.get(16)?,
        original_changed: row.get::<_, i64>(17)? != 0,
        original_missing: row.get::<_, i64>(18)? != 0,
        iso: row.get(19)?,
        aperture: row.get(20)?,
        shutter: row.get(21)?,
        focal_length: row.get(22)?,
        width: row.get::<_, Option<i64>>(23)?.map(|w| w as u32),
        height: row.get::<_, Option<i64>>(24)?.map(|h| h as u32),
    })
}

/// A position in the keyset order (`capture_time` descending, `id` descending as the tie-break):
/// the last row of the previous page. Small and `Copy`, so a caller can hold one across loop
/// iterations without borrowing the page it came from.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    /// The capture time of the last row seen.
    pub capture_time: i64,
    /// Its identifier.
    pub id: PhotoId,
}

/// Which flags a listing shows (spec §5.3: rejected photos are hidden by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlagFilter {
    /// Everything but the rejected photos: the default.
    #[default]
    NotRejected,
    /// Every photo, rejected ones included.
    All,
    /// Picked photos only.
    Picked,
    /// Rejected photos only (the Rejected view).
    Rejected,
}

/// What a listing of the grid is restricted to: the filters compose (a rating and a flag and a keyword).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Photos whose effective rating is at least this (0 for all).
    pub min_rating: u8,
    /// Which flags are shown.
    pub flags: FlagFilter,
    /// Photos carrying this keyword or any keyword under it in the vocabulary tree.
    pub keyword: Option<KeywordId>,
    /// Photos with this colour label (its text, any case: `Red`, `Yellow`...).
    pub label: Option<String>,
    /// Photos by series membership.
    pub series: SeriesFilter,
    /// Photos in this collection or in any collection under it (WP10, slice 3).
    pub collection: Option<CollectionId>,
}

/// Which photos a listing keeps by series (WP9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SeriesFilter {
    /// Every photo.
    #[default]
    Any,
    /// Photos that are in a series.
    InSeries,
    /// Photos of a series that is not resolved.
    Unresolved,
    /// Photos of a resolved series.
    Resolved,
}

/// One keyword of the vocabulary, with how many photos carry it directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordRow {
    /// Its identifier.
    pub id: KeywordId,
    /// Its parent, `None` for a top-level keyword.
    pub parent: Option<KeywordId>,
    /// Its name.
    pub name: String,
    /// Its path in the tree (`Place|Canada|Quebec`).
    pub path: String,
    /// Whether it is written at export (the "do not export" flag is the opposite).
    pub export: bool,
    /// Alternative names (D-045), for the panel's own type-ahead as well as its own display.
    pub synonyms: Vec<String>,
    /// How many photos carry it directly.
    pub photos: u64,
}

impl PhotoRow {
    /// The cursor that continues the list right after this row.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            capture_time: self.capture_time,
            id: self.id,
        }
    }
}

impl Catalogue {
    /// The most recent photos first (`capture_time` descending), the usual grid order. `after`
    /// continues from a previous page's last row.
    pub fn list_recent(&self, after: Option<Cursor>, limit: u32) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "WHERE (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => {
                stmt.query_map(params![c.capture_time, c.id.to_string(), limit], photo_row)?
            }
            None => stmt.query_map(params![limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The photos a [`Filter`] lets through, most recent first, in the same keyset order as
    /// [`Self::list_recent`]: what the grid lists. The filters compose in one statement.
    pub fn list_filtered(
        &self,
        filter: &Filter,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        use rusqlite::types::Value;
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        if filter.min_rating > 0 {
            conditions.push("p.effective_rating >= ?".into());
            values.push(Value::Integer(i64::from(filter.min_rating)));
        }
        match filter.flags {
            FlagFilter::NotRejected => conditions.push("p.effective_flag != 2".into()),
            FlagFilter::All => {}
            FlagFilter::Picked => conditions.push("p.effective_flag = 1".into()),
            FlagFilter::Rejected => conditions.push("p.effective_flag = 2".into()),
        }
        match filter.series {
            SeriesFilter::Any => {}
            SeriesFilter::InSeries => conditions.push("p.series_id IS NOT NULL".into()),
            SeriesFilter::Unresolved => {
                conditions.push("p.series_id IN (SELECT id FROM series WHERE resolved = 0)".into())
            }
            SeriesFilter::Resolved => {
                conditions.push("p.series_id IN (SELECT id FROM series WHERE resolved = 1)".into())
            }
        }
        if let Some(label) = &filter.label {
            conditions.push("p.label = ? COLLATE NOCASE".into());
            values.push(Value::Text(label.clone()));
        }
        if let Some(keyword) = &filter.keyword {
            conditions.push(
                "p.id IN (SELECT pk.photo_id FROM photo_keyword pk JOIN keyword k ON k.id = pk.keyword_id
                          WHERE k.id = ? OR k.path LIKE (SELECT path || '|%' FROM keyword WHERE id = ?))"
                    .into(),
            );
            values.push(Value::Text(keyword.to_string()));
            values.push(Value::Text(keyword.to_string()));
        }
        if let Some(collection) = &filter.collection {
            conditions.push(
                "p.id IN (WITH RECURSIVE branch(id) AS (
                              SELECT ? UNION ALL
                              SELECT c.id FROM collection c JOIN branch b ON c.parent_id = b.id)
                          SELECT photo_id FROM collection_member
                          WHERE collection_id IN (SELECT id FROM branch))"
                    .into(),
            );
            values.push(Value::Text(collection.to_string()));
        }
        if let Some(c) = after {
            conditions.push("(p.capture_time, p.id) < (?, ?)".into());
            values.push(Value::Integer(c.capture_time));
            values.push(Value::Text(c.id.to_string()));
        }
        values.push(Value::Integer(i64::from(limit)));
        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT {COLUMNS} {FROM} {clause} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), photo_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The whole vocabulary, in path order (a parent before its children), with how many photos carry
    /// each keyword directly.
    pub fn keywords_with_counts(&self) -> Result<Vec<KeywordRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT k.id, k.parent_id, k.name, k.path, k.export, k.synonyms,
                    (SELECT COUNT(*) FROM photo_keyword pk WHERE pk.keyword_id = k.id)
             FROM keyword k ORDER BY k.path",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let parent: Option<String> = r.get(1)?;
            let synonyms: String = r.get(5)?;
            let bad = |column: usize| {
                rusqlite::Error::InvalidColumnType(
                    column,
                    "keyword id".into(),
                    rusqlite::types::Type::Text,
                )
            };
            Ok(KeywordRow {
                id: id.parse().map_err(|_| bad(0))?,
                parent: parent.and_then(|p| p.parse().ok()),
                name: r.get(2)?,
                path: r.get(3)?,
                export: r.get::<_, i64>(4)? != 0,
                synonyms: synonyms
                    .split('|')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                photos: r.get::<_, i64>(6)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// For each keyword, how many of `photos` carry it directly (keywords none of them carry are left
    /// out): what a keyword panel shows for a selection (none, some or all of it).
    pub fn keyword_usage(&self, photos: &[PhotoId]) -> Result<HashMap<KeywordId, usize>> {
        let mut usage: HashMap<KeywordId, usize> = HashMap::new();
        for chunk in photos.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT keyword_id, COUNT(*) FROM photo_keyword WHERE photo_id IN ({placeholders})
                 GROUP BY keyword_id"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let texts: Vec<String> = chunk.iter().map(ToString::to_string).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(&texts), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (keyword, count) = row?;
                if let Ok(keyword) = keyword.parse() {
                    *usage.entry(keyword).or_default() += count as usize;
                }
            }
        }
        Ok(usage)
    }

    /// How many photos there are in total.
    pub fn count_all(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM photo", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Photos whose effective rating is at least `min`, most recent first.
    pub fn list_by_min_rating(
        &self,
        min: u8,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} WHERE p.effective_rating >= ?1 {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => stmt.query_map(
                params![min, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![min, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// How many photos have at least `min` as their effective rating.
    pub fn count_by_min_rating(&self, min: u8) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM photo WHERE effective_rating >= ?1",
            [min],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// Photos in a given camera, most recent first.
    pub fn list_by_camera(
        &self,
        camera: &str,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} WHERE c.name = ?1 {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => stmt.query_map(
                params![camera, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![camera, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Photos carrying a keyword (or, with `include_descendants`, any keyword under it in the
    /// vocabulary tree, matched by path prefix), most recent first.
    pub fn list_by_keyword(
        &self,
        keyword: &KeywordId,
        include_descendants: bool,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let base = if include_descendants {
            format!(
                "SELECT {COLUMNS} {FROM}
                 JOIN photo_keyword pk ON pk.photo_id = p.id
                 JOIN keyword k ON k.id = pk.keyword_id
                 WHERE (k.id = ?1 OR k.path LIKE (SELECT path || '|%' FROM keyword WHERE id = ?1))"
            )
        } else {
            format!(
                "SELECT {COLUMNS} {FROM} JOIN photo_keyword pk ON pk.photo_id = p.id AND pk.keyword_id = ?1"
            )
        };
        let sql = format!(
            "{base} {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let text = keyword.to_string();
        let rows = match after {
            Some(c) => stmt.query_map(
                params![text, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![text, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A full-text search over the file name, caption, title and keyword names, ranked by
    /// relevance. `query` follows FTS5's own syntax (a plain word is a prefix-free match; quote a
    /// phrase, or add `*` for a prefix).
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM}
             JOIN photo_fts f ON f.rowid = p.rowid
             WHERE photo_fts MATCH ?1 ORDER BY bm25(photo_fts) LIMIT ?2"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![query, limit], photo_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A single photo by identifier.
    pub fn photo(&self, id: &PhotoId) -> Result<Option<PhotoRow>> {
        let sql = format!("SELECT {COLUMNS} {FROM} WHERE p.id = ?1");
        Ok(self
            .conn
            .query_row(&sql, [id.to_string()], photo_row)
            .optional()?)
    }

    /// Every photo that carries any of `keywords` directly (no descendants), unordered, without
    /// paging: for finding the small set of sidecars a keyword rename must refresh (note 003
    /// §6), not for display.
    pub fn photos_with_keywords(&self, keywords: &[KeywordId]) -> Result<Vec<PhotoId>> {
        if keywords.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = keywords.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT photo_id FROM photo_keyword WHERE keyword_id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let texts: Vec<String> = keywords.iter().map(ToString::to_string).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(&texts), |r| {
            r.get::<_, String>(0)
        })?;
        let mut ids = Vec::new();
        for id in rows {
            let id = id?;
            ids.push(id.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(
                    0,
                    "photo_id".into(),
                    rusqlite::types::Type::Text,
                )
            })?);
        }
        Ok(ids)
    }

    /// A registered source by identifier.
    pub fn source(&self, id: &SourceId) -> Result<Option<SourceRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, name FROM source WHERE id = ?1",
                [id.to_string()],
                source_row,
            )
            .optional()?)
    }

    /// Every registered source.
    pub fn list_sources(&self) -> Result<Vec<SourceRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, name FROM source ORDER BY name")?;
        let rows = stmt.query_map([], source_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every photo whose original lives in `source_id`, by identifier.
    pub fn photos_in_source(&self, source_id: &SourceId) -> Result<Vec<PhotoId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM photo WHERE source_id = ?1")?;
        let rows = stmt.query_map([source_id.to_string()], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
            })?);
        }
        Ok(out)
    }

    /// What is in `source_id`, for a person deciding whether to remove it.
    pub fn source_counts(&self, source_id: &SourceId) -> Result<SourceCounts> {
        let (photos, worked_on) = self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN rating > 0 OR flag > 0 OR version_count > 0
                                        OR caption IS NOT NULL OR title IS NOT NULL
                                        OR EXISTS(SELECT 1 FROM photo_keyword k WHERE k.photo_id = photo.id)
                                      THEN 1 ELSE 0 END), 0)
             FROM photo WHERE source_id = ?1",
            [source_id.to_string()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(SourceCounts {
            photos: photos as u64,
            worked_on: worked_on as u64,
        })
    }

    /// Every photo the catalogue has on record for `source_id`, its primary location (`photo`)
    /// and its secondary ones (`location`, D-108) both: identifier, path, fingerprint and whether
    /// this is its primary location, for a reconcile scan (design note 004 §6.4) to compare
    /// against a fresh listing. `catalogue` does not depend on `sources` (both sit beside each
    /// other under `engine`, architecture §3.2), so the caller wraps each tuple into a
    /// `sources::relink::KnownFile` itself. A photo with no fingerprint or path yet is left out:
    /// nothing to match it against.
    pub fn known_files_in_source(
        &self,
        source_id: &SourceId,
    ) -> Result<Vec<(PhotoId, String, Fingerprint, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, fingerprint, 1 FROM photo
               WHERE source_id = ?1 AND fingerprint IS NOT NULL AND path IS NOT NULL
             UNION ALL
             SELECT photo_id, path, fingerprint, 0 FROM location WHERE source_id = ?1",
        )?;
        let rows = stmt.query_map([source_id.to_string()], |r| {
            let id: String = r.get(0)?;
            let path: String = r.get(1)?;
            let fingerprint: String = r.get(2)?;
            let is_primary: bool = r.get(3)?;
            Ok((id, path, fingerprint, is_primary))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, fingerprint, is_primary) = row?;
            let photo_id = id.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
            })?;
            let fingerprint = fingerprint.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(
                    2,
                    "fingerprint".into(),
                    rusqlite::types::Type::Text,
                )
            })?;
            out.push((photo_id, path, fingerprint, is_primary));
        }
        Ok(out)
    }

    /// Every existing photo whose original's fingerprint matches `fingerprint` (design note 004
    /// §6.3, item 4): candidates for import's skip decision. A fingerprint match alone never
    /// decides anything by itself (the whole-file hash does, WP7); this only narrows the search.
    /// `hash` is `None` when this photo has never had its whole file read (added in place, WP4);
    /// the caller re-reads that photo's own file, at `source_id`/`path` if both are known, to get
    /// a hash to compare against before concluding anything.
    pub fn find_by_fingerprint(
        &self,
        fingerprint: &Fingerprint,
    ) -> Result<Vec<FingerprintCandidate>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, hash, source_id, path FROM photo WHERE fingerprint = ?1")?;
        let rows = stmt.query_map([fingerprint.to_string()], |r| {
            let id: String = r.get(0)?;
            let hash: Option<String> = r.get(1)?;
            let source_id: Option<String> = r.get(2)?;
            let path: Option<String> = r.get(3)?;
            Ok((id, hash, source_id, path))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, hash, source_id, path) = row?;
            let photo_id = id.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
            })?;
            let hash = hash.and_then(|h| h.parse().ok());
            let source_id = source_id.and_then(|s| s.parse().ok());
            out.push(FingerprintCandidate {
                photo_id,
                hash,
                source_id,
                path,
            });
        }
        Ok(out)
    }
}

/// One candidate [`Catalogue::find_by_fingerprint`] found: an existing photo whose original's
/// fingerprint matches the one being looked up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintCandidate {
    /// The existing photo.
    pub photo_id: PhotoId,
    /// Its whole-file hash, if already known.
    pub hash: Option<ContentHash>,
    /// Where its file was last seen: enough to re-read it and compute the hash on demand, when
    /// `hash` is `None`.
    pub source_id: Option<SourceId>,
    /// Its path inside that source.
    pub path: Option<String>,
}

/// What a source holds, in numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCounts {
    /// How many photos have their original in the source.
    pub photos: u64,
    /// How many of them carry work a person did: a rating, a flag, keywords, a title or caption, or
    /// a version.
    pub worked_on: u64,
}

/// One row of the `source` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// Its identifier.
    pub id: SourceId,
    /// The source plugin's identifier (`"local-folder"`, `"removable-volume"`).
    pub kind: String,
    /// Its display name.
    pub name: String,
}

fn source_row(row: &Row) -> rusqlite::Result<SourceRow> {
    let id: String = row.get(0)?;
    Ok(SourceRow {
        id: id.parse().map_err(|_| {
            rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
        })?,
        kind: row.get(1)?,
        name: row.get(2)?,
    })
}
