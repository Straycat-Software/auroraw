// SPDX-License-Identifier: GPL-3.0-or-later
//! Series in the catalogue (WP9, D-101): what detection reads (the photos that belong to no series, with what
//! tells a burst from a bracket), what the grid reads (each series in a line), and the incremental writes a
//! change of a series makes. The series' own state is the workspace's file (`series/<xx>/<id>.json`); the rows
//! here follow it, and a rebuild builds them again from the files.

use auroraw_format::state::Series;
use auroraw_types::{PhotoId, SeriesId};
use rusqlite::params;

use crate::SidecarStat;
use crate::error::{CatalogueError, Result};
use crate::open::Catalogue;

/// A photo that belongs to no series, with what decides whether it starts or joins one.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesCandidate {
    /// The photo.
    pub id: PhotoId,
    /// Seconds since the Unix epoch (never 0: a photo with no capture time is not a candidate).
    pub capture_time: i64,
    /// The camera (a catalogue-internal number), if known.
    pub camera: Option<i64>,
    /// ISO speed.
    pub iso: Option<i64>,
    /// F-number.
    pub aperture: Option<f64>,
    /// Exposure time in seconds.
    pub shutter: Option<f64>,
    /// Focal length in millimetres.
    pub focal_length: Option<f64>,
    /// The original file's name, to order photos taken in the same second.
    pub filename: String,
}

/// One series, as the grid needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesInfo {
    /// The series.
    pub id: SeriesId,
    /// `burst`, `bracket`, `similar` or `manual`.
    pub kind: String,
    /// Its cover photo.
    pub cover: PhotoId,
    /// Whether it was resolved.
    pub resolved: bool,
    /// How many photos are in it.
    pub members: usize,
}

impl Catalogue {
    /// Writes a series' row and gives its members its identifier; the photos that were members and are not any
    /// more (an earlier state of the same series) belong to none.
    pub fn apply_series(&mut self, series: &Series, stat: SidecarStat) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = series.id.to_string();
        tx.execute(
            "INSERT INTO series(id, cover_photo_id, kind, resolved, sidecar_size, sidecar_modified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET cover_photo_id = excluded.cover_photo_id, kind = excluded.kind,
                resolved = excluded.resolved, sidecar_size = excluded.sidecar_size,
                sidecar_modified = excluded.sidecar_modified",
            params![id, series.cover.to_string(), series.kind, series.resolved as i64, stat.size as i64, stat.modified],
        )?;
        tx.execute(
            "UPDATE photo SET series_id = NULL WHERE series_id = ?1",
            [&id],
        )?;
        for member in &series.members {
            tx.execute(
                "UPDATE photo SET series_id = ?1 WHERE id = ?2",
                params![id, member.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Takes a series out: its photos belong to none.
    pub fn remove_series(&mut self, id: &SeriesId) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = id.to_string();
        tx.execute(
            "UPDATE photo SET series_id = NULL WHERE series_id = ?1",
            [&id],
        )?;
        tx.execute("DELETE FROM series WHERE id = ?1", [&id])?;
        tx.commit()?;
        Ok(())
    }

    /// The photos that belong to no series and have a capture time, ordered by camera, then capture time, then
    /// file name: what detection walks.
    pub fn series_candidates(&self) -> Result<Vec<SeriesCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, capture_time, camera_id, iso, aperture, shutter, focal_length, filename
             FROM photo WHERE series_id IS NULL AND capture_time > 0
             ORDER BY camera_id, capture_time, filename, id",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(SeriesCandidate {
                id: id.parse().map_err(|_| {
                    rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
                })?,
                capture_time: r.get(1)?,
                camera: r.get(2)?,
                iso: r.get(3)?,
                aperture: r.get(4)?,
                shutter: r.get(5)?,
                focal_length: r.get(6)?,
                filename: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every series with its size.
    pub fn series_infos(&self) -> Result<Vec<SeriesInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.kind, s.cover_photo_id, s.resolved,
                    (SELECT COUNT(*) FROM photo p WHERE p.series_id = s.id)
             FROM series s",
        )?;
        let rows = stmt.query_map([], |r| {
            let bad = |i: usize, name: &str| {
                rusqlite::Error::InvalidColumnType(i, name.into(), rusqlite::types::Type::Text)
            };
            let id: String = r.get(0)?;
            let cover: String = r.get(2)?;
            Ok(SeriesInfo {
                id: id.parse().map_err(|_| bad(0, "id"))?,
                kind: r.get(1)?,
                cover: cover.parse().map_err(|_| bad(2, "cover_photo_id"))?,
                resolved: r.get::<_, i64>(3)? != 0,
                members: r.get::<_, i64>(4)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The members of a series in capture order (then file name).
    pub fn series_members(&self, id: &SeriesId) -> Result<Vec<PhotoId>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM photo WHERE series_id = ?1 ORDER BY capture_time, filename, id",
        )?;
        let rows = stmt.query_map([id.to_string()], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for id in rows {
            out.push(
                id?.parse()
                    .map_err(|_| CatalogueError::from(rusqlite::Error::InvalidQuery))?,
            );
        }
        Ok(out)
    }
}
