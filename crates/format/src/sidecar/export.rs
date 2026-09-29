// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP export to the source folders (spec §5.7, D-024, D-028; design note 003 §8 and §8.1): the
//! **derived** file other software reads beside an original, built from a photo's metadata.
//!
//! Two shapes, one table of what the export owns ([`OWNED`]):
//!
//! - [`build`] makes a **new** file: the owned properties, the original's capture data with the
//!   photographer's overlay applied, and the marker `aur:Export`. Never an identifier, a location, a
//!   keyword identifier or an `aur:Files` (they are Auroraw's own), and never a do-not-export keyword.
//! - [`merge_into`] rewrites **only** the owned properties of a file that already exists (Lightroom's,
//!   darktable's, digiKam's or an earlier export) and keeps every other property as it was: develop
//!   settings, history, unknown namespaces. The file is written back in the canonical form of
//!   [`Xmp::to_bytes`], so comments and the packet padding are dropped, but no property is.
//!
//! Nothing here reads a file or decides whether the file may be touched: the three-way comparison
//! against the base (D-047) is made by the caller, with [`super::external::merge`], before it writes.

use std::collections::HashSet;

use super::external::{keyword_key, normalise_path};
use super::{Flag, Metadata, Original, Overlay};
use crate::xmp::{ArrayKind, Property, Xmp, ns};

/// The version of the convention an export follows: the value of the marker `aur:Export`, by which
/// Auroraw recognises a file it wrote itself (D-047).
pub const EXPORT_VERSION: &str = "1";

/// The properties an export owns, in the order it writes them: a merge replaces these and nothing else.
pub const OWNED: &[(&str, &str)] = &[
    (ns::XMP, "Rating"),
    (ns::AUR, "Flag"),
    (ns::AUR, "Stars"),
    (ns::XMP, "Label"),
    (ns::DC, "title"),
    (ns::DC, "description"),
    (ns::DC, "subject"),
    (ns::LR, "hierarchicalSubject"),
    (ns::DC, "creator"),
    (ns::DC, "rights"),
    (ns::XMP_RIGHTS, "UsageTerms"),
    (ns::XMP_RIGHTS, "WebStatement"),
    (ns::PHOTOSHOP, "Credit"),
    (ns::PHOTOSHOP, "Source"),
    (ns::PHOTOSHOP, "Headline"),
    (ns::PHOTOSHOP, "Instructions"),
    (ns::IPTC_CORE, "Location"),
    (ns::PHOTOSHOP, "City"),
    (ns::PHOTOSHOP, "State"),
    (ns::PHOTOSHOP, "Country"),
    (ns::IPTC_CORE, "CountryCode"),
    (ns::IPTC_EXT, "PersonInImage"),
    (ns::IPTC_EXT, "Event"),
    (ns::AUR, "Export"),
];

/// What an export is built from.
#[derive(Debug, Clone, Copy)]
pub struct ExportView<'a> {
    /// The photo's metadata.
    pub meta: &'a Metadata,
    /// The keywords to write, as their **current** paths (`|` between levels, from the vocabulary and
    /// not from the sidecar's snapshot, note 003 §6), with the do-not-export ones already left out.
    pub keywords: &'a [String],
    /// Whether a rejected photo is written as `xmp:Rating` `-1`, which other software understands
    /// (its stars then travel in `aur:Stars`, note 003 §4.5).
    pub rejected_as_minus_one: bool,
}

fn nonblank(value: &Option<String>) -> Option<&String> {
    value.as_ref().filter(|v| !v.trim().is_empty())
}

fn text(ns: &str, name: &str, value: &Option<String>) -> Option<Property> {
    nonblank(value).map(|v| Property::text(ns, name, v.clone()))
}

fn lang(ns: &str, name: &str, value: &Option<String>) -> Option<Property> {
    nonblank(value).map(|v| Property::lang_alt(ns, name, v.clone()))
}

fn array(ns: &str, name: &str, kind: ArrayKind, values: &[String]) -> Option<Property> {
    let values: Vec<String> = values
        .iter()
        .filter(|v| !v.trim().is_empty())
        .cloned()
        .collect();
    (!values.is_empty()).then(|| Property::array(ns, name, kind, values))
}

fn leaf(path: &str) -> &str {
    path.rsplit('|').next().unwrap_or(path)
}

/// The keywords as paths, normalised and without duplicates (compared in lower case).
fn paths(view: &ExportView) -> Vec<String> {
    let mut seen = HashSet::new();
    view.keywords
        .iter()
        .filter_map(|p| normalise_path(p))
        .filter(|p| seen.insert(keyword_key(p)))
        .collect()
}

/// The flat names: the last level of each path, without duplicates.
fn names(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .iter()
        .map(|p| leaf(p).to_string())
        .filter(|n| seen.insert(n.to_lowercase()))
        .collect()
}

/// The properties an export owns, from a photo's metadata, in the order of [`OWNED`] (the marker
/// last). A field with nothing in it has no property.
pub fn owned_properties(view: &ExportView) -> Vec<Property> {
    let m = view.meta;
    let paths = paths(view);
    let stars = m.rating.filter(|r| (1..=5).contains(r));
    let minus_one = m.flag == Some(Flag::Rejected) && view.rejected_as_minus_one;
    let rating = if minus_one {
        Some("-1".to_string())
    } else {
        stars.map(|s| s.to_string())
    };
    let props = vec![
        rating.map(|r| Property::text(ns::XMP, "Rating", r)),
        m.flag.map(|f| Property::text(ns::AUR, "Flag", f.as_text())),
        stars
            .filter(|_| minus_one)
            .map(|s| Property::text(ns::AUR, "Stars", s.to_string())),
        text(ns::XMP, "Label", &m.label),
        lang(ns::DC, "title", &m.title),
        lang(ns::DC, "description", &m.caption),
        array(ns::DC, "subject", ArrayKind::Bag, &names(&paths)),
        array(ns::LR, "hierarchicalSubject", ArrayKind::Bag, &paths),
        array(ns::DC, "creator", ArrayKind::Seq, &m.creator),
        lang(ns::DC, "rights", &m.rights),
        lang(ns::XMP_RIGHTS, "UsageTerms", &m.usage_terms),
        text(ns::XMP_RIGHTS, "WebStatement", &m.web_statement),
        text(ns::PHOTOSHOP, "Credit", &m.credit),
        text(ns::PHOTOSHOP, "Source", &m.source),
        text(ns::PHOTOSHOP, "Headline", &m.headline),
        text(ns::PHOTOSHOP, "Instructions", &m.instructions),
        text(ns::IPTC_CORE, "Location", &m.sublocation),
        text(ns::PHOTOSHOP, "City", &m.city),
        text(ns::PHOTOSHOP, "State", &m.region),
        text(ns::PHOTOSHOP, "Country", &m.country),
        text(ns::IPTC_CORE, "CountryCode", &m.country_code),
        array(ns::IPTC_EXT, "PersonInImage", ArrayKind::Bag, &m.persons),
        lang(ns::IPTC_EXT, "Event", &m.event),
        Some(Property::text(ns::AUR, "Export", EXPORT_VERSION)),
    ];
    props.into_iter().flatten().collect()
}

/// The original's capture data with the photographer's corrections applied (spec §5.7: the EXIF
/// overlay leaves the original's values visible in the workspace, and is what leaves it).
/// A corrected position replaces the three coordinates together: an altitude the correction does
/// not give is dropped, since the original's no longer belongs to where the photo now is.
pub fn effective_original(original: &Original, overlay: Option<&Overlay>) -> Original {
    let mut o = original.clone();
    let Some(overlay) = overlay else {
        return o;
    };
    if let Some(t) = nonblank(&overlay.capture_time) {
        o.capture_time = Some(t.clone());
    }
    if let Some(gps) = &overlay.gps {
        o.gps_latitude = Some(gps.latitude.clone());
        o.gps_longitude = Some(gps.longitude.clone());
        o.gps_altitude = gps.altitude.clone();
    }
    if let Some(c) = nonblank(&overlay.camera) {
        o.model = Some(c.clone());
    }
    if let Some(l) = nonblank(&overlay.lens) {
        o.lens = Some(l.clone());
    }
    if let Some(v) = overlay.orientation {
        o.orientation = Some(v);
    }
    o
}

/// The file for a photo that has none: the owned properties, then the effective capture data.
pub fn build(view: &ExportView) -> Xmp {
    let mut properties = owned_properties(view);
    let marker = properties.pop();
    properties.extend(
        effective_original(&view.meta.original, view.meta.overlay.as_ref()).to_properties(),
    );
    properties.extend(marker);
    Xmp {
        properties,
        prefixes: Vec::new(),
    }
}

/// Replaces the properties named in `keys` by `fresh`, in place: the first old occurrence of each
/// is where its replacement goes (the others, from further `rdf:Description` blocks, go), one with
/// no replacement is removed, and one that was not there is appended, in the order of `keys`.
fn replace_owned(props: &mut Vec<Property>, keys: &[(&str, &str)], mut fresh: Vec<Property>) {
    let owned = |p: &Property| keys.iter().any(|(n, name)| p.is(n, name));
    let mut out = Vec::with_capacity(props.len());
    for p in props.drain(..) {
        if !owned(&p) {
            out.push(p);
        } else if let Some(at) = fresh.iter().position(|f| f.is(&p.ns, &p.name)) {
            out.push(fresh.remove(at));
        }
    }
    for (n, name) in keys {
        if let Some(at) = fresh.iter().position(|f| f.is(n, name)) {
            out.push(fresh.remove(at));
        }
    }
    *props = out;
}

/// The capture properties a photographer's overlay changes, and only those.
fn overlay_keys(overlay: &Overlay) -> Vec<(&'static str, &'static str)> {
    let mut keys = Vec::new();
    if nonblank(&overlay.capture_time).is_some() {
        keys.push((ns::EXIF, "DateTimeOriginal"));
    }
    if overlay.gps.is_some() {
        keys.extend([
            (ns::EXIF, "GPSLatitude"),
            (ns::EXIF, "GPSLongitude"),
            (ns::EXIF, "GPSAltitude"),
        ]);
    }
    if nonblank(&overlay.camera).is_some() {
        keys.push((ns::TIFF, "Model"));
    }
    if nonblank(&overlay.lens).is_some() {
        keys.push((ns::AUX, "Lens"));
    }
    if overlay.orientation.is_some() {
        keys.push((ns::TIFF, "Orientation"));
    }
    keys
}

/// Rewrites the owned properties of a file that already exists and leaves everything else as it
/// was. Where the file also has a `digiKam:TagsList` it is rewritten with the same keywords (with
/// `/` for the hierarchy), or a list left stale would bring removed keywords back at the next read.
/// Capture data is touched only where the photographer's overlay corrected it.
pub fn merge_into(existing: &mut Xmp, view: &ExportView) {
    replace_owned(&mut existing.properties, OWNED, owned_properties(view));

    if existing.get(ns::DIGIKAM, "TagsList").is_some() {
        let tags: Vec<String> = paths(view).iter().map(|p| p.replace('|', "/")).collect();
        let replacement: Vec<Property> = array(ns::DIGIKAM, "TagsList", ArrayKind::Seq, &tags)
            .into_iter()
            .collect();
        replace_owned(
            &mut existing.properties,
            &[(ns::DIGIKAM, "TagsList")],
            replacement,
        );
    }

    if let Some(overlay) = &view.meta.overlay {
        let keys = overlay_keys(overlay);
        if !keys.is_empty() {
            let effective = effective_original(&view.meta.original, Some(overlay));
            let fresh: Vec<Property> = effective
                .to_properties()
                .into_iter()
                .filter(|p| keys.iter().any(|(n, name)| p.is(n, name)))
                .collect();
            replace_owned(&mut existing.properties, &keys, fresh);
        }
    }
}
