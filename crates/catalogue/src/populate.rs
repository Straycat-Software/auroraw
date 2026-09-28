// SPDX-License-Identifier: GPL-3.0-or-later
//! Writing one entity's row (and its dependent rows) into an open transaction. Shared by
//! [`crate::rebuild_to_file`] (which calls these once per entity, from scratch) and, later, by
//! incremental updates from the engine.

use std::collections::HashMap;

use auroraw_format::sidecar::{FileRole, OverrideField, PhotoSidecar, VersionSidecar};
use auroraw_format::state::{Collection, KeywordEntry, Series, SourceEntry};
use auroraw_types::{MemberRef, PhotoId};
use rusqlite::{Transaction, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::SidecarStat;
use crate::effective::{effective_flag, effective_rating, flag_code};
use crate::error::Result;

/// Every version of a photo, keyed by photo identifier, as `rebuild` and the tests need to look
/// a photo's main version up while inserting it.
pub(crate) type VersionsByPhoto<'a> = HashMap<PhotoId, Vec<&'a VersionSidecar>>;

pub(crate) fn group_versions_by_photo(
    versions: &[(VersionSidecar, SidecarStat)],
) -> VersionsByPhoto<'_> {
    let mut map: VersionsByPhoto<'_> = HashMap::new();
    for (v, _) in versions {
        map.entry(v.photo_id).or_default().push(v);
    }
    map
}

/// Seconds since the Unix epoch, or `None` if the text is not a timestamp this crate can parse.
/// Used only for sorting and filtering; an unparseable capture time sorts as the Unix epoch
/// rather than being dropped, so the photo still appears in the catalogue (`--done when--`
/// requires no photo to go missing from a rebuild).
fn parse_capture_time(text: &str) -> Option<i64> {
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
}

/// A rational written `"num/den"`, as EXIF values commonly are.
fn parse_rational(text: &str) -> Option<f64> {
    let (num, den) = text.split_once('/')?;
    let (num, den): (f64, f64) = (num.trim().parse().ok()?, den.trim().parse().ok()?);
    (den != 0.0).then_some(num / den)
}

fn get_or_insert_name(tx: &Transaction, table: &str, name: &str) -> rusqlite::Result<i64> {
    let sql = format!(
        "INSERT INTO {table}(name) VALUES (?1) ON CONFLICT(name) DO UPDATE SET name = excluded.name RETURNING id"
    );
    tx.query_row(&sql, [name], |r| r.get(0))
}

pub(crate) fn insert_source(tx: &Transaction, entry: &SourceEntry) -> Result<()> {
    tx.execute(
        "INSERT INTO source(id, kind, name) VALUES (?1, ?2, ?3)",
        params![entry.id.to_string(), entry.kind, entry.name],
    )?;
    Ok(())
}

/// `path` is the full hierarchical path (`"Fauna|Birds|Heron"`), which `KeywordEntry` itself does
/// not carry (note 002 keeps only the tree, by `parent`); the caller (`rebuild`) walks the tree
/// once to resolve every keyword's path before calling this.
pub(crate) fn insert_keyword(tx: &Transaction, entry: &KeywordEntry, path: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO keyword(id, parent_id, name, path, export, synonyms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.id.to_string(),
            entry.parent.map(|p| p.to_string()),
            entry.name,
            path,
            entry.export as i64,
            entry.synonyms.join("|"),
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_series(tx: &Transaction, series: &Series, stat: SidecarStat) -> Result<()> {
    tx.execute(
        "INSERT INTO series(id, cover_photo_id, kind, resolved, sidecar_size, sidecar_modified) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![series.id.to_string(), series.cover.to_string(), series.kind, series.resolved as i64, stat.size as i64, stat.modified],
    )?;
    Ok(())
}

pub(crate) fn insert_collection(
    tx: &Transaction,
    collection: &Collection,
    stat: SidecarStat,
) -> Result<()> {
    tx.execute(
        "INSERT INTO collection(id, parent_id, name, kind, sidecar_size, sidecar_modified) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            collection.id.to_string(),
            collection.parent.map(|p| p.to_string()),
            collection.name,
            collection.kind,
            stat.size as i64,
            stat.modified
        ],
    )?;
    for (position, member) in collection.members.iter().enumerate() {
        let (photo_id, version_id) = match member {
            MemberRef::Photo(p) => (p.to_string(), None),
            MemberRef::Version(p, v) => (p.to_string(), Some(v.to_string())),
        };
        tx.execute(
            "INSERT INTO collection_member(collection_id, position, photo_id, version_id) VALUES (?1, ?2, ?3, ?4)",
            params![collection.id.to_string(), position as i64, photo_id, version_id],
        )?;
    }
    Ok(())
}

pub(crate) fn insert_photo(
    tx: &Transaction,
    photo: &PhotoSidecar,
    stat: SidecarStat,
    versions: &[&VersionSidecar],
) -> Result<()> {
    let main_version = photo
        .main_version
        .and_then(|id| versions.iter().find(|v| v.version_id == id))
        .copied();
    let own_rating = photo.meta.rating.unwrap_or(0) as i64;
    let own_flag = flag_code(photo.meta.flag);
    let (eff_rating, overridden) = effective_rating(photo, main_version);
    let eff_flag = effective_flag(photo, main_version);

    let original_file = photo
        .files
        .iter()
        .find(|f| f.role == FileRole::Original)
        .or_else(|| photo.files.first());
    let location = original_file.and_then(|f| f.locations.first());
    let camera_id = camera_id(tx, &photo.meta.original.make, &photo.meta.original.model)?;
    let lens_id = match &photo.meta.original.lens {
        Some(name) if !name.is_empty() => Some(get_or_insert_name(tx, "lens", name)?),
        _ => None,
    };

    tx.execute(
        "INSERT INTO photo(
            id, source_id, path, filename, fingerprint, hash, capture_time, camera_id, lens_id,
            iso, aperture, shutter, focal_length, width, height,
            rating, flag, label, title, caption, gps_lat, gps_lon,
            series_id, main_version_id, version_count, effective_rating, effective_flag,
            rating_overridden, imported, sidecar_size, sidecar_modified
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31
        )",
        params![
            photo.photo_id.to_string(),
            location.map(|l| l.source.to_string()),
            location.map(|l| l.path.as_str()),
            original_file.map(|f| f.name.as_str()).unwrap_or_default(),
            original_file.map(|f| f.fingerprint.to_string()),
            original_file
                .and_then(|f| f.hash.as_ref())
                .map(|h| h.to_string()),
            photo
                .meta
                .original
                .capture_time
                .as_deref()
                .and_then(parse_capture_time)
                .unwrap_or(0),
            camera_id,
            lens_id,
            photo
                .meta
                .original
                .iso
                .first()
                .and_then(|s| s.parse::<i64>().ok()),
            photo
                .meta
                .original
                .f_number
                .as_deref()
                .and_then(parse_rational),
            photo
                .meta
                .original
                .exposure_time
                .as_deref()
                .and_then(parse_rational),
            photo
                .meta
                .original
                .focal_length
                .as_deref()
                .and_then(parse_rational),
            photo.meta.original.pixel_width,
            photo.meta.original.pixel_height,
            own_rating,
            own_flag,
            photo.meta.label,
            photo.meta.title,
            photo.meta.caption,
            // Location (§5.11) is out of scope until the map view returns (D-084): left NULL.
            Option::<f64>::None,
            Option::<f64>::None,
            None::<String>, // series_id is filled by a second pass once every series is known
            photo.main_version.map(|v| v.to_string()),
            versions.len() as i64,
            eff_rating,
            eff_flag,
            overridden as i64,
            photo.imported.map(|t| t.unix()),
            stat.size as i64,
            stat.modified,
        ],
    )?;
    // `photo_fts` is a self-contained FTS5 table (schema.sql) that still needs an integer rowid
    // to line up with a photo. `photo` has no `INTEGER PRIMARY KEY` (its key is the text
    // identifier), so SQLite still gave the row just inserted an implicit rowid of its own; reuse
    // it (taken immediately, before any other insert), which is the usual way to pair an FTS5
    // table with a table whose
    // declared primary key is not the rowid.
    let rowid = tx.last_insert_rowid();

    for kw in &photo.meta.keyword_ids {
        tx.execute(
            "INSERT OR IGNORE INTO photo_keyword(photo_id, keyword_id) VALUES (?1, ?2)",
            params![photo.photo_id.to_string(), kw.to_string()],
        )?;
    }
    let keyword_names = photo
        .meta
        .keyword_paths
        .iter()
        .map(|p| p.rsplit('|').next().unwrap_or(p))
        .collect::<Vec<_>>()
        .join(" ");
    tx.execute(
        "INSERT INTO photo_fts(rowid, filename, caption, title, keywords) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![rowid, original_file.map(|f| f.name.as_str()).unwrap_or_default(), photo.meta.caption, photo.meta.title, keyword_names],
    )?;
    // The original file's other locations, if it has any (D-108, exact duplicates): only once its whole-file hash is
    // known, so that only a confirmed duplicate is indexed here, never a location added to the sidecar some other
    // way, unconfirmed. `insert_photo` runs both for a fresh photo and for a rebuild, so this is the one place that
    // needs to preserve them.
    if let Some(file) = original_file
        && let Some(hash) = &file.hash
    {
        for location in file.locations.iter().skip(1) {
            tx.execute(
                "INSERT OR IGNORE INTO location(photo_id, source_id, path, filename, fingerprint, hash, seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    photo.photo_id.to_string(),
                    location.source.to_string(),
                    location.path,
                    file.name,
                    file.fingerprint.to_string(),
                    hash.to_string(),
                    location.seen.map(|t| t.unix()),
                ],
            )?;
        }
    }
    Ok(())
}

fn camera_id(
    tx: &Transaction,
    make: &Option<String>,
    model: &Option<String>,
) -> Result<Option<i64>> {
    let name = match (make.as_deref(), model.as_deref()) {
        (Some(make), Some(model)) if !make.is_empty() || !model.is_empty() => {
            format!("{make} {model}").trim().to_string()
        }
        (Some(make), None) if !make.is_empty() => make.to_string(),
        (None, Some(model)) if !model.is_empty() => model.to_string(),
        _ => return Ok(None),
    };
    Ok(Some(get_or_insert_name(tx, "camera", &name)?))
}

pub(crate) fn insert_version(
    tx: &Transaction,
    version: &VersionSidecar,
    stat: SidecarStat,
) -> Result<()> {
    let rating = version
        .overrides
        .contains(&OverrideField::Rating)
        .then_some(version.meta.rating.unwrap_or(0) as i64);
    let flag = version
        .overrides
        .contains(&OverrideField::Flag)
        .then_some(flag_code(version.meta.flag) as i64);
    let label = version
        .overrides
        .contains(&OverrideField::Label)
        .then(|| version.meta.label.clone())
        .flatten();
    tx.execute(
        "INSERT INTO version(id, photo_id, name, rating, flag, label, created, sidecar_size, sidecar_modified)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            version.version_id.to_string(),
            version.photo_id.to_string(),
            version.name,
            rating,
            flag,
            label,
            version.created.map(|t| t.unix()),
            stat.size as i64,
            stat.modified,
        ],
    )?;
    for kw in &version.added_keyword_ids {
        tx.execute(
            "INSERT OR IGNORE INTO version_keyword(version_id, keyword_id) VALUES (?1, ?2)",
            params![version.version_id.to_string(), kw.to_string()],
        )?;
    }
    Ok(())
}

/// Sets a photo's `series_id`, once every series row exists (a series can be written after its
/// photos in the input, so this runs as a second pass).
pub(crate) fn set_photo_series(
    tx: &Transaction,
    photo_id: &PhotoId,
    series_id: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE photo SET series_id = ?1 WHERE id = ?2",
        params![series_id, photo_id.to_string()],
    )?;
    Ok(())
}
