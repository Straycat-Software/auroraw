// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::HashSet;

use super::fields::Fields;
use crate::sidecar::{Flag, Metadata};
use crate::xmp::{Property, Xmp, XmpError, ns};

/// The largest foreign XMP file worth reading (8 MiB): an editor's history can be long, but a file
/// this size is not a sidecar, and the caller checks the listing's size before reading anything.
pub const MAX_BYTES: usize = 8 << 20;

/// Why a foreign XMP file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ExternalError {
    /// The file is not valid XMP.
    #[error(transparent)]
    Xmp(#[from] XmpError),
    /// The file is larger than [`MAX_BYTES`].
    #[error("larger than {MAX_BYTES} bytes: {0}")]
    TooLarge(usize),
}

/// Reads a foreign XMP file (Lightroom, darktable, digiKam, ExifTool...) into the fields Auroraw
/// compares. Unlike a sidecar, it needs no `aur:Schema` or `aur:PhotoId`; everything it does not
/// understand (develop settings, history, other namespaces) is simply not looked at. Never writes.
///
/// - **Rating** is one axis: `xmp:Rating="-1"` (the rejected mark other software writes) and
///   `aur:Flag="rejected"` both read as `-1`; a rating above 5 is ignored.
/// - **Keywords** are the union of `lr:hierarchicalSubject`, `digiKam:TagsList` (whose `/` is the
///   hierarchy, read as `|`) and the `dc:subject` names that are not a level of any path (a tool that
///   writes both the hierarchy and its flat names would otherwise add every parent as a keyword).
pub fn read(bytes: &[u8]) -> Result<Fields, ExternalError> {
    if bytes.len() > MAX_BYTES {
        return Err(ExternalError::TooLarge(bytes.len()));
    }
    let mut xmp = Xmp::from_bytes(bytes)?;
    let keywords = keyword_union(&xmp);
    // (`take_from` leaves a rating it cannot read as 0 to 255, so the rejected mark is taken first.)
    let rejected = take_rejected_rating(&mut xmp.properties);
    let mut meta = Metadata::take_from(&mut xmp.properties);
    if rejected {
        meta.flag = Some(Flag::Rejected);
    }
    meta.keyword_ids.clear();
    meta.keyword_paths = keywords;
    Ok(Fields::from_metadata(&meta, |_| None))
}

fn take_rejected_rating(props: &mut Vec<Property>) -> bool {
    let at = props
        .iter()
        .position(|p| p.is(ns::XMP, "Rating") && p.as_text().is_some_and(|t| t.trim() == "-1"));
    match at {
        Some(i) => {
            props.remove(i);
            true
        }
        None => false,
    }
}

fn texts(xmp: &Xmp, namespace: &str, name: &str) -> Vec<String> {
    xmp.get(namespace, name)
        .and_then(Property::as_texts)
        .map(|items| items.into_iter().map(str::to_string).collect())
        .unwrap_or_default()
}

fn keyword_union(xmp: &Xmp) -> Vec<String> {
    let mut paths = texts(xmp, ns::LR, "hierarchicalSubject");
    paths.extend(
        texts(xmp, ns::DIGIKAM, "TagsList")
            .into_iter()
            .map(|p| p.replace('/', "|")),
    );
    let levels: HashSet<String> = paths
        .iter()
        .flat_map(|p| p.split('|'))
        .map(|level| level.trim().to_lowercase())
        .collect();
    paths.extend(
        texts(xmp, ns::DC, "subject")
            .into_iter()
            .filter(|name| !levels.contains(&name.trim().to_lowercase())),
    );
    paths
}

/// What only a file Auroraw wrote itself carries beyond [`Fields`]: the flag and the stars, which the
/// single foreign rating axis cannot hold (a rejected photo keeps its stars, spec §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OwnExtras {
    /// `aur:Flag`.
    pub flag: Option<Flag>,
    /// The stars: `aur:Stars` (written next to a `-1` rating), else the file's own `xmp:Rating` when it
    /// is 1 to 5 (a rejected photo exported without the `-1` option).
    pub stars: Option<u8>,
}

/// The flag and the stars of a file that carries Auroraw's export marker `aur:Export` (design note
/// 003 §4.5, §8), or `None` for any other file, including one that cannot be read. Used when a photo
/// is first added from such a file, so that re-reading an export gives back exactly the same flag and
/// stars; never for a comparison, which is [`read`]'s and only ever sees the rating axis.
pub fn own_extras(bytes: &[u8]) -> Option<OwnExtras> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let xmp = Xmp::from_bytes(bytes).ok()?;
    xmp.get(ns::AUR, "Export")?;
    let parsed = |namespace: &str, name: &str| -> Option<u8> {
        xmp.get(namespace, name)?
            .as_text()?
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|n| (1..=5).contains(n))
    };
    Some(OwnExtras {
        flag: xmp
            .get(ns::AUR, "Flag")
            .and_then(Property::as_text)
            .and_then(|t| t.trim().parse().ok()),
        stars: parsed(ns::AUR, "Stars").or_else(|| parsed(ns::XMP, "Rating")),
    })
}
