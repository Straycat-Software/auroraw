// SPDX-License-Identifier: GPL-3.0-or-later
//! The photo's technical metadata that the catalogue does not carry (the Info panel, spec §5.7):
//! the serial number, the 35 mm equivalent focal length, the orientation and the GPS position, read
//! straight from the sidecar's `Metadata.original` (the same source `metadata_field_of` already
//! reads). Everything else the panel shows (camera, lens, exposure, dimensions, capture time)
//! already has a catalogue column (`PhotoRow`) and does not need this. Read-only: nothing here is
//! the EXIF-overlay editing spec §5.7 still leaves `[proposed]`.

use auroraw_format::sidecar::Metadata;
use auroraw_types::PhotoId;

use crate::Engine;

/// The fields `PhotoRow` does not carry. Numbers, not formatted text: every user-facing string in
/// this codebase lives in QML (`qsTr`), including this one's `orientation` code, which the panel
/// turns into a phrase itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TechnicalDetails {
    /// The camera's serial number, if the file carries one.
    pub serial: Option<String>,
    /// The 35 mm equivalent focal length, in millimetres.
    pub focal_length_35mm: Option<f64>,
    /// The EXIF orientation code, 1 to 8.
    pub orientation: Option<u32>,
    /// As the file's own XMP already has it, not reformatted.
    pub gps_latitude: Option<String>,
    /// As the file's own XMP already has it, not reformatted.
    pub gps_longitude: Option<String>,
    /// As the file's own XMP already has it, not reformatted.
    pub gps_altitude: Option<String>,
    /// `"0"` above sea level, `"1"` below, `None` when the file did not say (read as above).
    pub gps_altitude_ref: Option<String>,
}

/// `"750/10"` -> `75.0`; the ratio and rounding are entirely `Original`'s own concern, kept local
/// to this one field (`catalogue::populate`'s own `parse_rational` does the same for the columns
/// it reads back from, one crate over).
fn parse_rational(text: &str) -> Option<f64> {
    let (num, den) = text.split_once('/')?;
    let (num, den): (f64, f64) = (num.trim().parse().ok()?, den.trim().parse().ok()?);
    (den != 0.0).then_some(num / den)
}

fn technical_details_from(meta: &Metadata) -> TechnicalDetails {
    let o = &meta.original;
    TechnicalDetails {
        serial: o.serial.clone(),
        focal_length_35mm: o.focal_length_35mm.as_deref().and_then(parse_rational),
        orientation: o.orientation,
        gps_latitude: o.gps_latitude.clone(),
        gps_longitude: o.gps_longitude.clone(),
        gps_altitude: o.gps_altitude.clone(),
        gps_altitude_ref: o.gps_altitude_ref.clone(),
    }
}

impl Engine {
    /// The photo's technical details beyond what the catalogue carries, `None` when the photo is
    /// not in the workspace.
    pub fn technical_details_of(&self, photo_id: &PhotoId) -> Option<TechnicalDetails> {
        let photo = self
            .workspace()
            .read_photo(photo_id)
            .ok()
            .flatten()?
            .current()?;
        Some(technical_details_from(&photo.meta))
    }
}

#[cfg(test)]
mod tests {
    use auroraw_format::sidecar::Original;

    use super::*;

    fn meta_with(original: Original) -> Metadata {
        Metadata {
            original,
            ..Metadata::default()
        }
    }

    #[test]
    fn every_field_round_trips_when_present() {
        let meta = meta_with(Original {
            serial: Some("12345".into()),
            focal_length_35mm: Some("750/10".into()),
            orientation: Some(6),
            gps_latitude: Some("49,17.859N".into()),
            gps_longitude: Some("123,6.573W".into()),
            gps_altitude: Some("180".into()),
            gps_altitude_ref: Some("1".into()),
            ..Original::default()
        });
        let details = technical_details_from(&meta);
        assert_eq!(details.serial.as_deref(), Some("12345"));
        assert_eq!(details.focal_length_35mm, Some(75.0));
        assert_eq!(details.orientation, Some(6));
        assert_eq!(details.gps_latitude.as_deref(), Some("49,17.859N"));
        assert_eq!(details.gps_longitude.as_deref(), Some("123,6.573W"));
        assert_eq!(details.gps_altitude.as_deref(), Some("180"));
        assert_eq!(details.gps_altitude_ref.as_deref(), Some("1"));
    }

    #[test]
    fn every_field_is_absent_when_original_has_none_of_them() {
        let details = technical_details_from(&Metadata::default());
        assert_eq!(details, TechnicalDetails::default());
    }

    #[test]
    fn a_35mm_equivalent_that_is_not_a_rational_is_left_out_rather_than_guessed() {
        let meta = meta_with(Original {
            focal_length_35mm: Some("not-a-number".into()),
            ..Original::default()
        });
        assert_eq!(technical_details_from(&meta).focal_length_35mm, None);
    }

    #[test]
    fn a_zero_denominator_is_left_out_rather_than_dividing_by_zero() {
        let meta = meta_with(Original {
            focal_length_35mm: Some("75/0".into()),
            ..Original::default()
        });
        assert_eq!(technical_details_from(&meta).focal_length_35mm, None);
    }
}
