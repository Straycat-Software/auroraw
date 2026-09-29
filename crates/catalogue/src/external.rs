// SPDX-License-Identifier: GPL-3.0-or-later
//! The base of the external XMP files (D-047, WP10; design note 003 §8.1): what Auroraw last read
//! from the XMP file another application keeps next to a photo's original, with the file's size and
//! modification time, so that a later scan can tell that the file changed and what changed in it.
//! One row a photo (`crate::open`'s schema 5). **Local to this machine** and lost by a rebuild: a
//! photo without a row is baselined silently by the next scan, never reported. Nothing here touches
//! a file: the caller reads, parses and decides.

use std::collections::HashMap;

use auroraw_format::sidecar::external::Fields;
use auroraw_types::{PhotoId, SourceId};
use rusqlite::{OptionalExtension, params};

use crate::error::Result;
use crate::open::Catalogue;

/// An external file's size and modification time as a scan listed them. The time is in
/// **nanoseconds** since the Unix epoch when the file system keeps that much (a rewrite within the
/// same second is then still seen), else whatever it keeps, scaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExternalStat {
    /// The size in bytes.
    pub size: u64,
    /// Nanoseconds since the Unix epoch, when the file system reports a time.
    pub modified_ns: Option<i64>,
}

/// What a scan needs to know about a photo's tracked file to decide whether to read it again: no JSON
/// is parsed to get this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalKnown {
    /// The file's path inside the photo's source.
    pub path: String,
    /// The stat the base was read at.
    pub stat: ExternalStat,
    /// The stat of a newer state noticed and not yet answered, if any.
    pub pending: Option<ExternalStat>,
}

/// A change noticed in the file and not yet accepted or ignored: the file's newer fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPending {
    /// The stat the newer state was read at.
    pub stat: ExternalStat,
    /// What the file held then.
    pub file: Fields,
}

/// A photo's tracked external file, whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRow {
    /// The photo.
    pub photo_id: PhotoId,
    /// The file's path inside the photo's source.
    pub path: String,
    /// The stat the base was read at.
    pub stat: ExternalStat,
    /// What Auroraw last read from (or, later, wrote to) the file.
    pub base: Fields,
    /// A newer state waiting for an answer, if any.
    pub pending: Option<ExternalPending>,
}

fn stat_of(size: i64, modified_ns: Option<i64>) -> ExternalStat {
    ExternalStat {
        size: u64::try_from(size).unwrap_or(0),
        modified_ns,
    }
}

/// A row as SQLite gives it, before its JSON is read.
type RawRow = (
    String,
    String,
    i64,
    Option<i64>,
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

fn raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
    ))
}

/// A row whose photo identifier and base can be read; anything else is no row.
fn parse_row(
    (photo, path, size, modified_ns, base, pending, pending_size, pending_ns): RawRow,
) -> Option<ExternalRow> {
    let base = serde_json::from_str::<Fields>(&base).ok()?;
    let pending = match (pending, pending_size) {
        (Some(json), Some(size)) => {
            serde_json::from_str::<Fields>(&json)
                .ok()
                .map(|file| ExternalPending {
                    stat: stat_of(size, pending_ns),
                    file,
                })
        }
        _ => None,
    };
    Some(ExternalRow {
        photo_id: photo.parse().ok()?,
        path,
        stat: stat_of(size, modified_ns),
        base,
        pending,
    })
}

impl Catalogue {
    /// The tracked external file of every photo whose original is in `source`, without their fields.
    pub fn external_stats(&self, source: &SourceId) -> Result<HashMap<PhotoId, ExternalKnown>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.photo_id, e.path, e.size, e.modified_ns, e.pending_size, e.pending_modified_ns
             FROM external_xmp e JOIN photo p ON p.id = e.photo_id
             WHERE p.source_id = ?1",
        )?;
        let rows = stmt.query_map([source.to_string()], |r| {
            let photo: String = r.get(0)?;
            let pending_size: Option<i64> = r.get(4)?;
            let pending_ns: Option<i64> = r.get(5)?;
            Ok((
                photo,
                ExternalKnown {
                    path: r.get(1)?,
                    stat: stat_of(r.get(2)?, r.get(3)?),
                    pending: pending_size.map(|size| stat_of(size, pending_ns)),
                },
            ))
        })?;
        let mut known = HashMap::new();
        for row in rows {
            let (photo, known_file) = row?;
            if let Ok(photo) = photo.parse() {
                known.insert(photo, known_file);
            }
        }
        Ok(known)
    }

    /// A photo's tracked external file, whole. A base that does not parse (written by another
    /// version) reads as no row at all: the next scan baselines it again.
    pub fn external_of(&self, photo: &PhotoId) -> Result<Option<ExternalRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT photo_id, path, size, modified_ns, base, pending, pending_size, pending_modified_ns
                 FROM external_xmp WHERE photo_id = ?1",
                [photo.to_string()],
                raw_row,
            )
            .optional()?;
        Ok(row.and_then(parse_row))
    }

    /// Every photo with a change noticed in its external file and not yet answered, by file path.
    pub fn pending_externals(&self) -> Result<Vec<ExternalRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, path, size, modified_ns, base, pending, pending_size, pending_modified_ns
             FROM external_xmp WHERE pending IS NOT NULL ORDER BY path, photo_id",
        )?;
        let rows = stmt.query_map([], raw_row)?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(row) = parse_row(row?) {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Notes a change in a photo's external file that nobody has answered yet (replacing an earlier
    /// one): the base stays what it was, for the comparison. `false` when the photo has no tracked file.
    pub fn set_external_pending(
        &mut self,
        photo: &PhotoId,
        pending: &ExternalPending,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE external_xmp SET pending = ?2, pending_size = ?3, pending_modified_ns = ?4
             WHERE photo_id = ?1",
            params![
                photo.to_string(),
                serde_json::to_string(&pending.file)?,
                pending.stat.size as i64,
                pending.stat.modified_ns
            ],
        )?;
        Ok(changed > 0)
    }

    /// Answers changes: for each photo, the base becomes `fields`, read at `stat` (what was accepted,
    /// ignored, or found to agree already), and nothing is pending any more. One transaction; a photo
    /// with no tracked file is skipped.
    pub fn settle_externals(&mut self, settled: &[(PhotoId, ExternalStat, Fields)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (photo, stat, fields) in settled {
            tx.execute(
                "UPDATE external_xmp SET size = ?2, modified_ns = ?3, base = ?4,
                   pending = NULL, pending_size = NULL, pending_modified_ns = NULL
                 WHERE photo_id = ?1",
                params![
                    photo.to_string(),
                    stat.size as i64,
                    stat.modified_ns,
                    serde_json::to_string(fields)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Records what was read from a photo's external file: its `path`, the `stat` it was read at and
    /// its `base` fields. Replaces any earlier row and clears any pending state (the file is now
    /// what Auroraw has seen). Fails if the photo is not in the catalogue.
    pub fn set_external_base(
        &mut self,
        photo: &PhotoId,
        path: &str,
        stat: ExternalStat,
        base: &Fields,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO external_xmp(photo_id, path, size, modified_ns, base)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(photo_id) DO UPDATE SET
               path = excluded.path, size = excluded.size, modified_ns = excluded.modified_ns,
               base = excluded.base,
               pending = NULL, pending_size = NULL, pending_modified_ns = NULL",
            params![
                photo.to_string(),
                path,
                stat.size as i64,
                stat.modified_ns,
                serde_json::to_string(base)?
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use auroraw_types::WorkspaceId;

    use super::*;

    fn photo_row(cat: &Catalogue, id: PhotoId, source: &SourceId) {
        cat.conn
            .execute(
                "INSERT INTO photo(id, source_id, path, filename, fingerprint, hash, capture_time, sidecar_size)
                 VALUES (?1, ?2, 'a.jpg', 'a.jpg', 'fp', 'h', 0, 1)",
                params![id.to_string(), source.to_string()],
            )
            .unwrap();
    }

    fn fields(title: &str) -> Fields {
        Fields {
            title: Some(title.into()),
            rating: Some(3),
            keywords: vec!["Fauna|Birds".into()],
            ..Fields::default()
        }
    }

    const STAT: ExternalStat = ExternalStat {
        size: 1200,
        modified_ns: Some(1_700_000_000_123_456_789),
    };

    #[test]
    fn a_base_round_trips_and_a_second_one_replaces_it() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (source, photo) = (SourceId::random(), PhotoId::random());
        photo_row(&cat, photo, &source);
        assert_eq!(cat.external_of(&photo).unwrap(), None);

        cat.set_external_base(&photo, "d/a.xmp", STAT, &fields("one"))
            .unwrap();
        let row = cat.external_of(&photo).unwrap().unwrap();
        assert_eq!(row.path, "d/a.xmp");
        assert_eq!(row.stat, STAT, "nanoseconds survive");
        assert_eq!(row.base, fields("one"));
        assert_eq!(row.pending, None);

        let newer = ExternalStat {
            size: 1300,
            modified_ns: None,
        };
        cat.set_external_base(&photo, "d/a.ARW.xmp", newer, &fields("two"))
            .unwrap();
        let row = cat.external_of(&photo).unwrap().unwrap();
        assert_eq!(
            (row.path.as_str(), row.stat, row.base),
            ("d/a.ARW.xmp", newer, fields("two"))
        );
    }

    #[test]
    fn the_stats_of_a_source_come_without_parsing_anything_and_only_its_own() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (here, elsewhere) = (SourceId::random(), SourceId::random());
        let (a, b) = (PhotoId::random(), PhotoId::random());
        photo_row(&cat, a, &here);
        photo_row(&cat, b, &elsewhere);
        cat.set_external_base(&a, "a.xmp", STAT, &fields("a"))
            .unwrap();
        cat.set_external_base(&b, "b.xmp", STAT, &fields("b"))
            .unwrap();
        // A pending state, as a later scan will leave it.
        cat.conn
            .execute(
                "UPDATE external_xmp SET pending = '{}', pending_size = 7, pending_modified_ns = 9 WHERE photo_id = ?1",
                [a.to_string()],
            )
            .unwrap();

        let known = cat.external_stats(&here).unwrap();
        assert_eq!(known.len(), 1);
        let k = &known[&a];
        assert_eq!((k.path.as_str(), k.stat), ("a.xmp", STAT));
        assert_eq!(
            k.pending,
            Some(ExternalStat {
                size: 7,
                modified_ns: Some(9)
            })
        );
        // And a new base clears the pending state.
        cat.set_external_base(&a, "a.xmp", STAT, &fields("a"))
            .unwrap();
        assert_eq!(cat.external_stats(&here).unwrap()[&a].pending, None);
    }

    #[test]
    fn a_base_this_version_cannot_read_is_no_row_and_a_missing_photo_is_refused() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (source, photo) = (SourceId::random(), PhotoId::random());
        photo_row(&cat, photo, &source);
        cat.set_external_base(&photo, "a.xmp", STAT, &fields("x"))
            .unwrap();
        cat.conn
            .execute("UPDATE external_xmp SET base = 'not json'", [])
            .unwrap();
        assert_eq!(cat.external_of(&photo).unwrap(), None);
        assert!(
            cat.set_external_base(&PhotoId::random(), "z.xmp", STAT, &fields("x"))
                .is_err(),
            "the foreign key holds"
        );
    }

    #[test]
    fn removing_a_photo_takes_its_row_and_relinking_it_drops_the_row_it_no_longer_matches() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (source, photo) = (SourceId::random(), PhotoId::random());
        photo_row(&cat, photo, &source);
        cat.set_external_base(&photo, "a.xmp", STAT, &fields("x"))
            .unwrap();
        let fp = auroraw_types::Fingerprint::from_bytes([1; 32]);
        cat.apply_relink(&photo, &source, "moved/a.jpg", "a.jpg", &fp)
            .unwrap();
        assert_eq!(
            cat.external_of(&photo).unwrap(),
            None,
            "its path is no longer valid"
        );

        cat.set_external_base(&photo, "moved/a.xmp", STAT, &fields("x"))
            .unwrap();
        cat.remove_photo(&photo).unwrap();
        assert_eq!(cat.external_of(&photo).unwrap(), None);
        let rows: i64 = cat
            .conn
            .query_row("SELECT COUNT(*) FROM external_xmp", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn a_rebuild_starts_without_any_base() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        let workspace = WorkspaceId::random();
        let mut cat = Catalogue::create(&path, workspace).unwrap();
        let (source, photo) = (SourceId::random(), PhotoId::random());
        photo_row(&cat, photo, &source);
        cat.set_external_base(&photo, "a.xmp", STAT, &fields("x"))
            .unwrap();
        drop(cat);

        let input = crate::RebuildInput {
            photos: &[],
            versions: &[],
            vocabulary: &[],
            sources: &[],
            collections: &[],
            series: &[],
        };
        let rebuilt = crate::rebuild_to_file(&path, workspace, &input).unwrap();
        let rows: i64 = rebuilt
            .conn
            .query_row("SELECT COUNT(*) FROM external_xmp", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "the table is there, empty");
    }

    #[test]
    fn a_change_waits_as_pending_beside_the_base_until_it_is_answered() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (source, a, b) = (SourceId::random(), PhotoId::random(), PhotoId::random());
        photo_row(&cat, a, &source);
        photo_row(&cat, b, &source);
        cat.set_external_base(&a, "z/a.xmp", STAT, &fields("a"))
            .unwrap();
        cat.set_external_base(&b, "b.xmp", STAT, &fields("b"))
            .unwrap();
        assert!(cat.pending_externals().unwrap().is_empty());

        let newer = ExternalStat {
            size: 2000,
            modified_ns: Some(5),
        };
        let pending = ExternalPending {
            stat: newer,
            file: fields("a2"),
        };
        assert!(cat.set_external_pending(&a, &pending).unwrap());
        assert!(
            !cat.set_external_pending(&PhotoId::random(), &pending)
                .unwrap()
        );
        let rows = cat.pending_externals().unwrap();
        assert_eq!(rows.len(), 1, "only the photo with a change waiting");
        assert_eq!(rows[0].photo_id, a);
        assert_eq!(rows[0].base, fields("a"), "the base is what it was");
        assert_eq!(rows[0].pending, Some(pending.clone()));
        assert_eq!(cat.external_of(&a).unwrap().unwrap().pending, Some(pending));

        // A newer change replaces the older.
        let newest = ExternalPending {
            stat: ExternalStat {
                size: 2100,
                modified_ns: Some(6),
            },
            file: fields("a3"),
        };
        cat.set_external_pending(&a, &newest).unwrap();
        assert_eq!(cat.pending_externals().unwrap()[0].pending, Some(newest));

        // Answered: the base becomes what was answered, at its stat, and nothing is pending.
        cat.settle_externals(&[
            (a, newer, fields("a2")),
            (PhotoId::random(), newer, fields("x")),
        ])
        .unwrap();
        assert!(cat.pending_externals().unwrap().is_empty());
        let row = cat.external_of(&a).unwrap().unwrap();
        assert_eq!(
            (row.stat, row.base, row.path.as_str()),
            (newer, fields("a2"), "z/a.xmp")
        );
    }

    #[test]
    fn pending_changes_are_listed_by_file_path() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let source = SourceId::random();
        let ids: Vec<PhotoId> = (0..3).map(|_| PhotoId::random()).collect();
        for (id, path) in ids.iter().zip(["c.xmp", "a.xmp", "b.xmp"]) {
            photo_row(&cat, *id, &source);
            cat.set_external_base(id, path, STAT, &fields(path))
                .unwrap();
            cat.set_external_pending(
                id,
                &ExternalPending {
                    stat: STAT,
                    file: fields("new"),
                },
            )
            .unwrap();
        }
        let paths: Vec<String> = cat
            .pending_externals()
            .unwrap()
            .into_iter()
            .map(|r| r.path)
            .collect();
        assert_eq!(paths, ["a.xmp", "b.xmp", "c.xmp"]);
    }
}
