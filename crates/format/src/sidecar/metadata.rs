// SPDX-License-Identifier: GPL-3.0-or-later
//! The metadata shared by the photo sidecar and, as a derived copy, by the version sidecar
//! (design note 003 §4.3 and §7).

use auroraw_types::{ContentHash, KeywordId};

use super::extract as x;
use crate::xmp::{ArrayKind, Item, Property, Value, Xmp, ns};

/// The flag of a photo (D-063, spec §5.3). Stars and flag are kept apart (note 003 §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Picked.
    Picked,
    /// Rejected.
    Rejected,
}

impl Flag {
    pub(crate) fn as_text(self) -> &'static str {
        match self {
            Self::Picked => "picked",
            Self::Rejected => "rejected",
        }
    }
}

impl std::str::FromStr for Flag {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "picked" => Ok(Self::Picked),
            "rejected" => Ok(Self::Rejected),
            _ => Err(()),
        }
    }
}

/// A colour label (spec §5.3): the five names other software writes in `xmp:Label`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourLabel {
    /// Red.
    Red,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Purple.
    Purple,
}

impl ColourLabel {
    /// Every colour, in the order they are offered.
    pub const ALL: [ColourLabel; 5] = [
        ColourLabel::Red,
        ColourLabel::Yellow,
        ColourLabel::Green,
        ColourLabel::Blue,
        ColourLabel::Purple,
    ];

    /// The text the sidecar holds (`Red`, `Yellow`, `Green`, `Blue`, `Purple`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
        }
    }
}

impl std::str::FromStr for ColourLabel {
    type Err = ();
    /// Any case; a label that is not one of the five (a foreign file's own) is not a colour.
    fn from_str(s: &str) -> Result<Self, ()> {
        Self::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(s.trim()))
            .ok_or(())
    }
}

/// A keyword of a photo as the sidecar records it: its identifier and the snapshot of its path
/// (note 003 §6). The names are a copy for other software; the identifier is the identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword {
    /// The identifier in the vocabulary, absent for a keyword written by other software.
    pub id: Option<KeywordId>,
    /// The path with `|` between levels, as of the last write.
    pub path: String,
}

/// The original's capture data as read from the file, in the standard XMP properties (D-074).
/// Values are kept as XMP text; consumers convert them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Original {
    /// `exif:DateTimeOriginal`, ISO 8601 with the sub-seconds and the offset when known.
    pub capture_time: Option<String>,
    /// `tiff:Make`.
    pub make: Option<String>,
    /// `tiff:Model`.
    pub model: Option<String>,
    /// `aux:SerialNumber`.
    pub serial: Option<String>,
    /// `aux:Lens`.
    pub lens: Option<String>,
    /// `exif:ExposureTime`, a rational such as `1/250`.
    pub exposure_time: Option<String>,
    /// `exif:FNumber`, a rational such as `28/10`.
    pub f_number: Option<String>,
    /// `exif:ISOSpeedRatings`.
    pub iso: Vec<String>,
    /// `exif:FocalLength`.
    pub focal_length: Option<String>,
    /// `exif:FocalLengthIn35mmFilm`.
    pub focal_length_35mm: Option<String>,
    /// `exif:PixelXDimension`.
    pub pixel_width: Option<u32>,
    /// `exif:PixelYDimension`.
    pub pixel_height: Option<u32>,
    /// `tiff:Orientation`, 1 to 8.
    pub orientation: Option<u32>,
    /// `exif:GPSLatitude`.
    pub gps_latitude: Option<String>,
    /// `exif:GPSLongitude`.
    pub gps_longitude: Option<String>,
    /// `exif:GPSAltitude`: a distance in metres, never negative (XMP's convention, EXIF's).
    pub gps_altitude: Option<String>,
    /// `exif:GPSAltitudeRef`: `"0"` above sea level, `"1"` below. `None` when the file did not say, and in
    /// every sidecar written before it was kept: read as above sea level, as those altitudes always were.
    pub gps_altitude_ref: Option<String>,
}

impl Original {
    /// The properties of the capture data, in the canonical order of the schema.
    pub(crate) fn to_properties(&self) -> Vec<Property> {
        let o = self;
        let props: Vec<Option<Property>> = vec![
            x::opt_text(ns::EXIF, "DateTimeOriginal", &o.capture_time),
            x::opt_text(ns::TIFF, "Make", &o.make),
            x::opt_text(ns::TIFF, "Model", &o.model),
            x::opt_text(ns::AUX, "SerialNumber", &o.serial),
            x::opt_text(ns::AUX, "Lens", &o.lens),
            x::opt_text(ns::EXIF, "ExposureTime", &o.exposure_time),
            x::opt_text(ns::EXIF, "FNumber", &o.f_number),
            x::opt_array(ns::EXIF, "ISOSpeedRatings", ArrayKind::Seq, &o.iso),
            x::opt_text(ns::EXIF, "FocalLength", &o.focal_length),
            x::opt_text(ns::EXIF, "FocalLengthIn35mmFilm", &o.focal_length_35mm),
            o.pixel_width
                .map(|v| Property::text(ns::EXIF, "PixelXDimension", v.to_string())),
            o.pixel_height
                .map(|v| Property::text(ns::EXIF, "PixelYDimension", v.to_string())),
            o.orientation
                .map(|v| Property::text(ns::TIFF, "Orientation", v.to_string())),
            x::opt_text(ns::EXIF, "GPSLatitude", &o.gps_latitude),
            x::opt_text(ns::EXIF, "GPSLongitude", &o.gps_longitude),
            x::opt_text(ns::EXIF, "GPSAltitude", &o.gps_altitude),
            x::opt_text(ns::EXIF, "GPSAltitudeRef", &o.gps_altitude_ref),
        ];
        props.into_iter().flatten().collect()
    }
}

/// The corrected position of an overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayGps {
    /// Latitude, in the XMP text form.
    pub latitude: String,
    /// Longitude, in the XMP text form.
    pub longitude: String,
    /// Altitude, if corrected.
    pub altitude: Option<String>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// The EXIF corrections of the photographer, which leave the original's values visible (spec §5.7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// A corrected capture time.
    pub capture_time: Option<String>,
    /// A corrected position.
    pub gps: Option<OverlayGps>,
    /// A camera name.
    pub camera: Option<String>,
    /// A lens name.
    pub lens: Option<String>,
    /// A corrected orientation.
    pub orientation: Option<u32>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// One metadata field beyond the fixed IPTC/XMP set `Metadata` below already has (WP10's own
/// custom-field support): a name and a value, chosen by whoever set it. No UI creates or shows one
/// yet — the engine command and this round-trip exist so a later slice, or a plugin (spec §5.7),
/// can add that without touching the format again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomField {
    /// Its name.
    pub name: String,
    /// Its value.
    pub value: String,
}

/// The metadata of a photo, or the effective metadata copied into a version (note 003 §4.3, §7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Stars, 0 to 5.
    pub rating: Option<u8>,
    /// Picked or rejected.
    pub flag: Option<Flag>,
    /// The colour label's name.
    pub label: Option<String>,
    /// The title.
    pub title: Option<String>,
    /// The caption.
    pub caption: Option<String>,
    /// The identifiers of the keywords, aligned with `keyword_paths` when both have the same length.
    pub keyword_ids: Vec<KeywordId>,
    /// The paths of the keywords as of the last write (`dc:subject` holds their last levels).
    pub keyword_paths: Vec<String>,
    /// The creators.
    pub creator: Vec<String>,
    /// The copyright notice.
    pub rights: Option<String>,
    /// The usage terms.
    pub usage_terms: Option<String>,
    /// The web statement of rights.
    pub web_statement: Option<String>,
    /// The credit line.
    pub credit: Option<String>,
    /// The source.
    pub source: Option<String>,
    /// The headline.
    pub headline: Option<String>,
    /// The instructions.
    pub instructions: Option<String>,
    /// The sublocation.
    pub sublocation: Option<String>,
    /// The city.
    pub city: Option<String>,
    /// The region or state.
    pub region: Option<String>,
    /// The country.
    pub country: Option<String>,
    /// The ISO country code.
    pub country_code: Option<String>,
    /// The persons shown.
    pub persons: Vec<String>,
    /// The event.
    pub event: Option<String>,
    /// The original's capture data.
    pub original: Original,
    /// The EXIF overlay.
    pub overlay: Option<Overlay>,
    /// What Auroraw wrote into the place fields, and has not been written over since (design note 008 §4).
    pub place_filled: Option<PlaceFilled>,
    /// Fields beyond the fixed set above (WP10's own custom-field support; no UI creates one yet).
    pub custom: Vec<CustomField>,
}

/// A coordinate in XMP's text form (`DDD,MM.mmmmR` or `DDD,MM,SSR`, `R` the hemisphere) as decimal degrees,
/// negative for the `negative` hemisphere. `None` for text that is not one, or a value beyond `limit`.
pub fn parse_gps_coordinate(text: &str, positive: char, negative: char, limit: f64) -> Option<f64> {
    let text = text.trim();
    let hemisphere = text.chars().last()?.to_ascii_uppercase();
    let sign = match hemisphere {
        h if h == positive => 1.0,
        h if h == negative => -1.0,
        _ => return None,
    };
    let number = &text[..text.len() - hemisphere.len_utf8()];
    let mut parts = number.split(',');
    let degrees: f64 = parts.next()?.trim().parse().ok()?;
    let minutes: f64 = parts.next().map_or(Some(0.0), |m| m.trim().parse().ok())?;
    let seconds: f64 = parts.next().map_or(Some(0.0), |s| s.trim().parse().ok())?;
    if parts.next().is_some()
        || degrees < 0.0
        || !(0.0..60.0).contains(&minutes)
        || !(0.0..60.0).contains(&seconds)
    {
        return None;
    }
    let value = degrees + minutes / 60.0 + seconds / 3600.0;
    (value <= limit).then_some(sign * value)
}

/// One of the four fields the place names fill (design note 008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceField {
    /// `photoshop:City`.
    City,
    /// `photoshop:State`, the region.
    Region,
    /// `photoshop:Country`.
    Country,
    /// `Iptc4xmpCore:CountryCode`.
    CountryCode,
}

impl PlaceField {
    /// The four fields, in the order the place names are written and reported.
    pub const ALL: [PlaceField; 4] = [
        PlaceField::City,
        PlaceField::Region,
        PlaceField::Country,
        PlaceField::CountryCode,
    ];

    /// A stable, ASCII name for the field, as the interface and the command line spell it.
    pub const fn key(self) -> &'static str {
        match self {
            PlaceField::City => "city",
            PlaceField::Region => "region",
            PlaceField::Country => "country",
            PlaceField::CountryCode => "country-code",
        }
    }
}

/// The record of what Auroraw filled into the place fields (`aur:PlaceFilled`, design note 008 §4): the
/// position the names were found for, and, for each field, the value written. **A field is listed with its
/// value while it is Auroraw's**: the moment a person writes it (a new value, or the same one) it leaves the
/// record ([`Metadata::release_place_field`]), and a refresh after the position moves touches only what
/// is still listed. **A person who empties a field is answering**, not leaving a gap: the record then keeps
/// the field with an empty text ([`Metadata::decline_place_field`], [`PlaceFilled::is_cleared`]), so that
/// the place job does not fill it again, until a person writes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceFilled {
    /// The latitude the names were found for, in decimal degrees as text.
    pub latitude: String,
    /// The longitude, likewise.
    pub longitude: String,
    /// The city: the text written if it is still Auroraw's, an **empty text if a person emptied it** (a
    /// field they answered, see [`PlaceFilled::is_cleared`]), nothing otherwise.
    pub city: Option<String>,
    /// The region, likewise.
    pub region: Option<String>,
    /// The country, likewise.
    pub country: Option<String>,
    /// The country code, likewise.
    pub country_code: Option<String>,
    /// Properties this version does not know, kept.
    pub extra: Vec<Property>,
}

impl PlaceFilled {
    /// What the record holds for `field`: the text Auroraw wrote and nobody has touched, an empty text for
    /// a field a person emptied ([`PlaceFilled::is_cleared`]), nothing for a field that is theirs or was
    /// never filled. Four states in all, with the field's own text: absent, written, cleared.
    pub fn get(&self, field: PlaceField) -> Option<&str> {
        match field {
            PlaceField::City => self.city.as_deref(),
            PlaceField::Region => self.region.as_deref(),
            PlaceField::Country => self.country.as_deref(),
            PlaceField::CountryCode => self.country_code.as_deref(),
        }
    }

    /// Records `value` as written by Auroraw for `field`.
    pub fn set(&mut self, field: PlaceField, value: Option<String>) {
        match field {
            PlaceField::City => self.city = value,
            PlaceField::Region => self.region = value,
            PlaceField::Country => self.country = value,
            PlaceField::CountryCode => self.country_code = value,
        }
    }

    /// Whether a person emptied `field` after it was filled, or on their own: **an answer, not a gap to
    /// refill** (design note 008 §4). The place job leaves such a field empty, a refresh included, until a
    /// person writes it again.
    pub fn is_cleared(&self, field: PlaceField) -> bool {
        self.get(field) == Some("")
    }

    /// Whether the record holds nothing: no field Auroraw wrote and no field a person emptied. (A record
    /// that holds only emptied fields is not empty: it is what keeps them from being filled again.)
    pub fn is_empty(&self) -> bool {
        self.city.is_none()
            && self.region.is_none()
            && self.country.is_none()
            && self.country_code.is_none()
    }
}

fn leaf(path: &str) -> &str {
    path.rsplit('|').next().unwrap_or(path)
}

impl Metadata {
    /// The text of a place field.
    pub fn place_field(&self, field: PlaceField) -> Option<&str> {
        match field {
            PlaceField::City => self.city.as_deref(),
            PlaceField::Region => self.region.as_deref(),
            PlaceField::Country => self.country.as_deref(),
            PlaceField::CountryCode => self.country_code.as_deref(),
        }
    }

    /// Sets a place field (an empty text clears it). It does not touch the record of what Auroraw wrote:
    /// that is [`Metadata::release_place_field`]'s, for a person's write, and the place job's own, for its
    /// fill.
    pub fn set_place_field(&mut self, field: PlaceField, value: Option<String>) {
        let value = value.filter(|v| !v.is_empty());
        match field {
            PlaceField::City => self.city = value,
            PlaceField::Region => self.region = value,
            PlaceField::Country => self.country = value,
            PlaceField::CountryCode => self.country_code = value,
        }
    }

    /// A person wrote this field: it is theirs from now on, whatever they wrote (design note 008 §4). Takes
    /// it out of the record of what Auroraw filled (an emptied field too: a value is an answer of its own),
    /// and drops the record when nothing is left of it. Returns whether it was in the record.
    pub fn release_place_field(&mut self, field: PlaceField) -> bool {
        let Some(record) = &mut self.place_filled else {
            return false;
        };
        let was = record.get(field).is_some();
        record.set(field, None);
        if record.is_empty() && record.extra.is_empty() {
            self.place_filled = None;
        }
        was
    }

    /// A person emptied this field: that is their answer, and **not a gap to refill** (design note 008 §4).
    /// The record keeps it, as an empty text, so that the place job and its refresh leave it empty until a
    /// person writes it again ([`Metadata::release_place_field`]). Makes the record if there was none: it
    /// then has no position, only the answer.
    pub fn decline_place_field(&mut self, field: PlaceField) {
        self.place_filled
            .get_or_insert_with(PlaceFilled::default)
            .set(field, Some(String::new()));
    }

    /// Where the photo was taken, in decimal degrees `(latitude, longitude)`: the photographer's correction
    /// when there is one (the overlay's position replaces the original's, spec §5.7), else the file's own.
    /// `None` when there is no position, or one that is not readable.
    pub fn position(&self) -> Option<(f64, f64)> {
        let (latitude, longitude) = match &self.overlay {
            Some(Overlay { gps: Some(gps), .. }) => (gps.latitude.as_str(), gps.longitude.as_str()),
            _ => (
                self.original.gps_latitude.as_deref()?,
                self.original.gps_longitude.as_deref()?,
            ),
        };
        Some((
            parse_gps_coordinate(latitude, 'N', 'S', 90.0)?,
            parse_gps_coordinate(longitude, 'E', 'W', 180.0)?,
        ))
    }

    /// Adds a keyword, with its identifier and the current path.
    pub fn push_keyword(&mut self, id: KeywordId, path: impl Into<String>) {
        self.keyword_ids.push(id);
        self.keyword_paths.push(path.into());
    }

    /// The keywords as pairs, when the identifiers and the paths line up.
    pub fn keywords(&self) -> Vec<Keyword> {
        if self.keyword_ids.len() == self.keyword_paths.len() {
            self.keyword_ids
                .iter()
                .zip(&self.keyword_paths)
                .map(|(id, path)| Keyword {
                    id: Some(*id),
                    path: path.clone(),
                })
                .collect()
        } else if self.keyword_ids.is_empty() {
            self.keyword_paths
                .iter()
                .map(|path| Keyword {
                    id: None,
                    path: path.clone(),
                })
                .collect()
        } else {
            // Identifiers and paths that do not line up: the identifiers are the truth.
            self.keyword_ids
                .iter()
                .map(|id| Keyword {
                    id: Some(*id),
                    path: String::new(),
                })
                .collect()
        }
    }

    /// The properties, in the canonical order of the schema.
    pub fn to_properties(&self) -> Vec<Property> {
        let o = &self.original;
        let leaves: Vec<String> = self
            .keyword_paths
            .iter()
            .map(|p| leaf(p).to_string())
            .collect();
        let ids: Vec<String> = self.keyword_ids.iter().map(ToString::to_string).collect();
        let mut props: Vec<Option<Property>> = vec![
            self.rating
                .map(|r| Property::text(ns::XMP, "Rating", r.to_string())),
            self.flag
                .map(|f| Property::text(ns::AUR, "Flag", f.as_text())),
            x::opt_text(ns::XMP, "Label", &self.label),
            x::opt_lang(ns::DC, "title", &self.title),
            x::opt_lang(ns::DC, "description", &self.caption),
            x::opt_array(ns::DC, "subject", ArrayKind::Bag, &leaves),
            x::opt_array(
                ns::LR,
                "hierarchicalSubject",
                ArrayKind::Bag,
                &self.keyword_paths,
            ),
            x::opt_array(ns::AUR, "KeywordIds", ArrayKind::Bag, &ids),
            x::opt_array(ns::DC, "creator", ArrayKind::Seq, &self.creator),
            x::opt_lang(ns::DC, "rights", &self.rights),
            x::opt_lang(ns::XMP_RIGHTS, "UsageTerms", &self.usage_terms),
            x::opt_text(ns::XMP_RIGHTS, "WebStatement", &self.web_statement),
            x::opt_text(ns::PHOTOSHOP, "Credit", &self.credit),
            x::opt_text(ns::PHOTOSHOP, "Source", &self.source),
            x::opt_text(ns::PHOTOSHOP, "Headline", &self.headline),
            x::opt_text(ns::PHOTOSHOP, "Instructions", &self.instructions),
            x::opt_text(ns::IPTC_CORE, "Location", &self.sublocation),
            x::opt_text(ns::PHOTOSHOP, "City", &self.city),
            x::opt_text(ns::PHOTOSHOP, "State", &self.region),
            x::opt_text(ns::PHOTOSHOP, "Country", &self.country),
            x::opt_text(ns::IPTC_CORE, "CountryCode", &self.country_code),
            x::opt_array(ns::IPTC_EXT, "PersonInImage", ArrayKind::Bag, &self.persons),
            x::opt_lang(ns::IPTC_EXT, "Event", &self.event),
        ];
        props.extend(o.to_properties().into_iter().map(Some));
        props.push(custom_property(&self.custom));
        props.push(self.overlay.as_ref().map(overlay_property));
        props.push(self.place_filled.as_ref().map(place_filled_property));
        props.into_iter().flatten().collect()
    }

    /// Takes the properties this version understands out of `props`; the rest stay.
    pub fn take_from(props: &mut Vec<Property>) -> Self {
        let mut m = Self {
            rating: x::parsed::<u8>(props, ns::XMP, "Rating").filter(|r| *r <= 5),
            flag: x::parsed(props, ns::AUR, "Flag"),
            label: x::text(props, ns::XMP, "Label"),
            title: x::lang_text(props, ns::DC, "title"),
            caption: x::lang_text(props, ns::DC, "description"),
            ..Self::default()
        };
        let flat = x::texts(props, ns::DC, "subject");
        let paths = x::texts(props, ns::LR, "hierarchicalSubject");
        m.keyword_ids = x::parsed_array(props, ns::AUR, "KeywordIds").unwrap_or_default();
        m.keyword_paths = paths.or(flat).unwrap_or_default();
        m.creator = x::texts(props, ns::DC, "creator").unwrap_or_default();
        m.rights = x::lang_text(props, ns::DC, "rights");
        m.usage_terms = x::lang_text(props, ns::XMP_RIGHTS, "UsageTerms");
        m.web_statement = x::text(props, ns::XMP_RIGHTS, "WebStatement");
        m.credit = x::text(props, ns::PHOTOSHOP, "Credit");
        m.source = x::text(props, ns::PHOTOSHOP, "Source");
        m.headline = x::text(props, ns::PHOTOSHOP, "Headline");
        m.instructions = x::text(props, ns::PHOTOSHOP, "Instructions");
        m.sublocation = x::text(props, ns::IPTC_CORE, "Location");
        m.city = x::text(props, ns::PHOTOSHOP, "City");
        m.region = x::text(props, ns::PHOTOSHOP, "State");
        m.country = x::text(props, ns::PHOTOSHOP, "Country");
        m.country_code = x::text(props, ns::IPTC_CORE, "CountryCode");
        m.persons = x::texts(props, ns::IPTC_EXT, "PersonInImage").unwrap_or_default();
        m.event = x::lang_text(props, ns::IPTC_EXT, "Event");
        let o = &mut m.original;
        o.capture_time = x::text(props, ns::EXIF, "DateTimeOriginal");
        o.make = x::text(props, ns::TIFF, "Make");
        o.model = x::text(props, ns::TIFF, "Model");
        o.serial = x::text(props, ns::AUX, "SerialNumber");
        o.lens = x::text(props, ns::AUX, "Lens");
        o.exposure_time = x::text(props, ns::EXIF, "ExposureTime");
        o.f_number = x::text(props, ns::EXIF, "FNumber");
        o.iso = x::texts(props, ns::EXIF, "ISOSpeedRatings").unwrap_or_default();
        o.focal_length = x::text(props, ns::EXIF, "FocalLength");
        o.focal_length_35mm = x::text(props, ns::EXIF, "FocalLengthIn35mmFilm");
        o.pixel_width = x::parsed(props, ns::EXIF, "PixelXDimension");
        o.pixel_height = x::parsed(props, ns::EXIF, "PixelYDimension");
        o.orientation = x::parsed(props, ns::TIFF, "Orientation");
        o.gps_latitude = x::text(props, ns::EXIF, "GPSLatitude");
        o.gps_longitude = x::text(props, ns::EXIF, "GPSLongitude");
        o.gps_altitude = x::text(props, ns::EXIF, "GPSAltitude");
        o.gps_altitude_ref = x::text(props, ns::EXIF, "GPSAltitudeRef");
        m.overlay = x::structure(props, ns::AUR, "Overlay").map(overlay_from_fields);
        m.place_filled = x::structure(props, ns::AUR, "PlaceFilled").map(place_filled_from_fields);
        m.custom = x::struct_items(props, ns::AUR, "Custom")
            .map(|items| items.into_iter().filter_map(custom_from_fields).collect())
            .unwrap_or_default();
        m
    }

    /// The digest of the fields that are copied into version sidecars (note 003 §5.3): BLAKE3 of
    /// their canonical form, with the keywords counted by identifier, not by name (§6).
    pub fn digest(&self) -> ContentHash {
        let mut props = self.to_properties();
        if !self.keyword_ids.is_empty() {
            props.retain(|p| !p.is(ns::DC, "subject") && !p.is(ns::LR, "hierarchicalSubject"));
        }
        let bytes = Xmp {
            properties: props,
            prefixes: Vec::new(),
        }
        .to_bytes();
        ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes())
    }
}

/// The custom fields, as an array of structures (`x::struct_items`'s own counterpart on the write
/// side; nothing in `extract.rs` builds one yet, only `Files` in `photo.rs` needed the shape
/// before this).
fn custom_property(custom: &[CustomField]) -> Option<Property> {
    (!custom.is_empty()).then(|| {
        let items = custom
            .iter()
            .map(|c| Item {
                lang: None,
                value: Value::Struct(vec![
                    Property::text(ns::AUR, "Name", c.name.clone()),
                    Property::text(ns::AUR, "Value", c.value.clone()),
                ]),
            })
            .collect();
        Property {
            ns: ns::AUR.into(),
            name: "Custom".into(),
            lang: None,
            value: Value::Array(ArrayKind::Seq, items),
        }
    })
}

fn custom_from_fields(mut fields: Vec<Property>) -> Option<CustomField> {
    let name = x::text(&mut fields, ns::AUR, "Name")?;
    let value = x::text(&mut fields, ns::AUR, "Value")?;
    Some(CustomField { name, value })
}

fn place_filled_property(p: &PlaceFilled) -> Property {
    // (A record that holds only the fields a person emptied has no position to give.)
    let mut fields = Vec::new();
    if !p.latitude.is_empty() {
        fields.push(Property::text(ns::AUR, "Latitude", p.latitude.clone()));
    }
    if !p.longitude.is_empty() {
        fields.push(Property::text(ns::AUR, "Longitude", p.longitude.clone()));
    }
    fields.extend(x::opt_text(ns::AUR, "City", &p.city));
    fields.extend(x::opt_text(ns::AUR, "Region", &p.region));
    fields.extend(x::opt_text(ns::AUR, "Country", &p.country));
    fields.extend(x::opt_text(ns::AUR, "CountryCode", &p.country_code));
    fields.extend(p.extra.iter().cloned());
    Property::structure(ns::AUR, "PlaceFilled", fields)
}

fn place_filled_from_fields(mut fields: Vec<Property>) -> PlaceFilled {
    PlaceFilled {
        latitude: x::text(&mut fields, ns::AUR, "Latitude").unwrap_or_default(),
        longitude: x::text(&mut fields, ns::AUR, "Longitude").unwrap_or_default(),
        city: x::text(&mut fields, ns::AUR, "City"),
        region: x::text(&mut fields, ns::AUR, "Region"),
        country: x::text(&mut fields, ns::AUR, "Country"),
        country_code: x::text(&mut fields, ns::AUR, "CountryCode"),
        extra: fields,
    }
}

fn overlay_property(o: &Overlay) -> Property {
    let mut fields = Vec::new();
    fields.extend(x::opt_text(ns::AUR, "CaptureTime", &o.capture_time));
    if let Some(g) = &o.gps {
        let mut gps = vec![
            Property::text(ns::AUR, "Latitude", g.latitude.clone()),
            Property::text(ns::AUR, "Longitude", g.longitude.clone()),
        ];
        gps.extend(x::opt_text(ns::AUR, "Altitude", &g.altitude));
        gps.extend(g.extra.iter().cloned());
        fields.push(Property::structure(ns::AUR, "Gps", gps));
    }
    fields.extend(x::opt_text(ns::AUR, "Camera", &o.camera));
    fields.extend(x::opt_text(ns::AUR, "Lens", &o.lens));
    fields.extend(
        o.orientation
            .map(|v| Property::text(ns::AUR, "Orientation", v.to_string())),
    );
    fields.extend(o.extra.iter().cloned());
    Property::structure(ns::AUR, "Overlay", fields)
}

fn overlay_from_fields(mut fields: Vec<Property>) -> Overlay {
    let mut gps = None;
    if let Some(mut g) = x::structure(&mut fields, ns::AUR, "Gps") {
        let original = g.clone();
        match (
            x::text(&mut g, ns::AUR, "Latitude"),
            x::text(&mut g, ns::AUR, "Longitude"),
        ) {
            (Some(latitude), Some(longitude)) => {
                let altitude = x::text(&mut g, ns::AUR, "Altitude");
                gps = Some(OverlayGps {
                    latitude,
                    longitude,
                    altitude,
                    extra: g,
                });
            }
            // Not understood: keep the structure as it was.
            _ => fields.push(Property::structure(ns::AUR, "Gps", original)),
        }
    }
    let capture_time = x::text(&mut fields, ns::AUR, "CaptureTime");
    let camera = x::text(&mut fields, ns::AUR, "Camera");
    let lens = x::text(&mut fields, ns::AUR, "Lens");
    let orientation = x::parsed(&mut fields, ns::AUR, "Orientation");
    Overlay {
        capture_time,
        gps,
        camera,
        lens,
        orientation,
        extra: fields,
    }
}
