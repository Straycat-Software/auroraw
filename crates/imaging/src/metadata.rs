// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a photo's metadata: the cache of the original's data a photo sidecar keeps (design
//! note 003 §4.3, D-074), read once so a rebuild never needs to reopen the file (design note 004
//! §8, spike 3: 20 ms cold per file, impossible for an offline source).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rawler::formats::tiff::value::Rational;

use crate::error::{ImagingError, Result};
use crate::format::is_standard;

/// What the sidecar's `Original` caches, read from the file itself. Field shapes mirror
/// `auroraw_format::sidecar::Original` deliberately: this crate does not depend on `format`
/// (architecture §3.2, the two are siblings under `engine`), so translating one into the other,
/// when WP7 wires the two together, is a straight field-by-field copy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// `exif:DateTimeOriginal`, ISO 8601 text, as the file wrote it.
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
    /// `exif:PixelXDimension`: the embedded preview's width, not the sensor's (this crate makes
    /// no full RAW decode without the image engine, M2).
    pub pixel_width: Option<u32>,
    /// `exif:PixelYDimension`, likewise the preview's height.
    pub pixel_height: Option<u32>,
    /// `tiff:Orientation`, 1 to 8.
    pub orientation: Option<u32>,
    /// `exif:GPSLatitude`, XMP `GPSCoordinate` text (`"DDD,MM.mmmmmmR"`).
    pub gps_latitude: Option<String>,
    /// `exif:GPSLongitude`, likewise.
    pub gps_longitude: Option<String>,
    /// `exif:GPSAltitude`, a rational in metres: a distance, never negative.
    pub gps_altitude: Option<String>,
    /// `exif:GPSAltitudeRef`: `"0"` above sea level, `"1"` below. Only with an altitude, and only when
    /// the file says which.
    pub gps_altitude_ref: Option<String>,
}

/// The XMP text of an EXIF `GPSAltitudeRef` byte: `"0"` (above sea level) or `"1"` (below); any other
/// value is a file's mistake and says nothing.
fn altitude_ref_text(byte: u8) -> Option<String> {
    matches!(byte, 0 | 1).then(|| byte.to_string())
}

/// The altitude and its reference in a standard file's EXIF.
fn standard_altitude(exif: &exif::Exif) -> (Option<String>, Option<String>) {
    let altitude = match exif
        .get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(exif::Value::Rational(v)) if !v.is_empty() => {
            Some(format!("{}/{}", v[0].num, v[0].denom))
        }
        _ => None,
    };
    let reference = altitude.as_ref().and_then(|_| {
        match exif
            .get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY)
            .map(|f| &f.value)
        {
            Some(exif::Value::Byte(v)) => v.first().copied().and_then(altitude_ref_text),
            _ => None,
        }
    });
    (altitude, reference)
}

fn rational_text(r: &Rational) -> String {
    format!("{}/{}", r.n, r.d)
}

fn xmp_coordinate(whole: &[Rational; 3], reference: &str) -> String {
    let degrees = whole[0].n as f64 / whole[0].d.max(1) as f64;
    let minutes = whole[1].n as f64 / whole[1].d.max(1) as f64;
    let seconds = whole[2].n as f64 / whole[2].d.max(1) as f64;
    format!(
        "{},{:.6}{}",
        degrees as i64,
        minutes + seconds / 60.0,
        reference
    )
}

/// Reads what this crate can find, dispatching by extension ([`is_standard`]). Never a full RAW
/// decode: only the container's own metadata and, for the pixel dimensions, the embedded
/// preview's (WP5 does not build the full demosaic path; that needs the image engine, M2).
pub fn read_metadata(path: &Path) -> Result<Metadata> {
    if is_standard(path) {
        read_standard_metadata(path)
    } else {
        read_raw_metadata(path)
    }
}

fn io_err(path: &Path, source: std::io::Error) -> ImagingError {
    ImagingError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn decode_err(path: &Path, message: impl std::fmt::Display) -> ImagingError {
    ImagingError::Decode {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

fn read_raw_metadata(path: &Path) -> Result<Metadata> {
    let source = rawler::rawsource::RawSource::new(path).map_err(|e| io_err(path, e))?;
    let decoder = rawler::get_decoder(&source).map_err(|e| decode_err(path, e))?;
    let params = rawler::decoders::RawDecodeParams::default();
    let raw_meta = decoder
        .raw_metadata(&source, &params)
        .map_err(|e| decode_err(path, e))?;
    let exif = &raw_meta.exif;

    let iso = exif
        .iso_speed
        .map(|v| v.to_string())
        .or_else(|| exif.iso_speed_ratings.map(|v| v.to_string()))
        .into_iter()
        .collect();

    let (gps_latitude, gps_longitude, gps_altitude, gps_altitude_ref) = match &exif.gps {
        Some(gps) => (
            match (&gps.gps_latitude, &gps.gps_latitude_ref) {
                (Some(v), Some(r)) => Some(xmp_coordinate(v, r)),
                _ => None,
            },
            match (&gps.gps_longitude, &gps.gps_longitude_ref) {
                (Some(v), Some(r)) => Some(xmp_coordinate(v, r)),
                _ => None,
            },
            gps.gps_altitude.as_ref().map(rational_text),
            gps.gps_altitude
                .as_ref()
                .and(gps.gps_altitude_ref)
                .and_then(altitude_ref_text),
        ),
        None => (None, None, None, None),
    };

    Ok(Metadata {
        capture_time: exif
            .date_time_original
            .as_deref()
            .and_then(|t| normalise_capture_time(t, exif.offset_time_original.as_deref())),
        make: (!raw_meta.make.is_empty()).then_some(raw_meta.make.clone()),
        model: (!raw_meta.model.is_empty()).then_some(raw_meta.model.clone()),
        serial: exif.serial_number.clone(),
        lens: exif.lens_model.clone(),
        exposure_time: exif.exposure_time.as_ref().map(rational_text),
        f_number: exif.fnumber.as_ref().map(rational_text),
        iso,
        focal_length: exif.focal_length.as_ref().map(rational_text),
        focal_length_35mm: None, // rawler's Exif has no dedicated field for this (a real gap).
        pixel_width: None,       // filled from the embedded preview by `crate::preview`, if any.
        pixel_height: None,
        orientation: exif.orientation.map(u32::from),
        gps_latitude,
        gps_longitude,
        gps_altitude,
        gps_altitude_ref,
    })
}

fn read_standard_metadata(path: &Path) -> Result<Metadata> {
    let (pixel_width, pixel_height) = image::ImageReader::open(path)
        .map_err(|e| io_err(path, e))?
        .with_guessed_format()
        .map_err(|e| io_err(path, e))?
        .into_dimensions()
        .map(|(w, h)| (Some(w), Some(h)))
        .unwrap_or((None, None));

    let mut metadata = Metadata {
        pixel_width,
        pixel_height,
        ..Metadata::default()
    };

    let file = File::open(path).map_err(|e| io_err(path, e))?;
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        // No EXIF segment at all (common for PNG): the dimensions above are still useful.
        return Ok(metadata);
    };

    let text = |tag: exif::Tag| {
        exif.get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string())
    };
    let rational = |tag: exif::Tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Rational(v)) if !v.is_empty() => {
            Some(format!("{}/{}", v[0].num, v[0].denom))
        }
        _ => None,
    };

    metadata.capture_time = text(exif::Tag::DateTimeOriginal)
        .and_then(|t| normalise_capture_time(&t, text(exif::Tag::OffsetTimeOriginal).as_deref()));
    metadata.make = text(exif::Tag::Make);
    metadata.model = text(exif::Tag::Model);
    metadata.serial = text(exif::Tag::BodySerialNumber);
    metadata.lens = text(exif::Tag::LensModel);
    metadata.exposure_time = rational(exif::Tag::ExposureTime);
    metadata.f_number = rational(exif::Tag::FNumber);
    metadata.iso = match exif
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(exif::Value::Short(v)) => v.iter().map(|n| n.to_string()).collect(),
        _ => Vec::new(),
    };
    metadata.focal_length = rational(exif::Tag::FocalLength);
    metadata.focal_length_35mm = text(exif::Tag::FocalLengthIn35mmFilm);
    metadata.orientation = match exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(exif::Value::Short(v)) => v.first().map(|&n| n as u32),
        _ => None,
    };

    let gps_rational =
        |tag: exif::Tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
            Some(exif::Value::Rational(v)) if v.len() == 3 => Some([
                Rational {
                    n: v[0].num,
                    d: v[0].denom,
                },
                Rational {
                    n: v[1].num,
                    d: v[1].denom,
                },
                Rational {
                    n: v[2].num,
                    d: v[2].denom,
                },
            ]),
            _ => None,
        };
    if let (Some(coord), Some(reference)) = (
        gps_rational(exif::Tag::GPSLatitude),
        text(exif::Tag::GPSLatitudeRef),
    ) {
        metadata.gps_latitude = Some(xmp_coordinate(&coord, &reference));
    }
    if let (Some(coord), Some(reference)) = (
        gps_rational(exif::Tag::GPSLongitude),
        text(exif::Tag::GPSLongitudeRef),
    ) {
        metadata.gps_longitude = Some(xmp_coordinate(&coord, &reference));
    }
    (metadata.gps_altitude, metadata.gps_altitude_ref) = standard_altitude(&exif);

    Ok(metadata)
}

/// A camera's `DateTimeOriginal` (`2016:09:02 10:28:00`, or `2016-09-02 10:28:00`) as an ISO 8601
/// timestamp with an offset (`2016-09-02T10:28:00+02:00`), which is what the sidecars and the
/// catalogue hold (an EXIF date has no zone of its own). When the camera also wrote its offset
/// (`OffsetTimeOriginal`) it is used; otherwise the wall-clock time is kept as if it were UTC
/// (`Z`): the date and time are those of the place the photo was taken, and that is what sorting
/// and naming by date want. `None` for text that is not a date and time.
pub fn normalise_capture_time(text: &str, offset: Option<&str>) -> Option<String> {
    let text = text.trim().trim_matches('"');
    let (date, time) = text.split_once(['T', ' '])?;
    let date: Vec<&str> = date.split([':', '-']).collect();
    let time = time.split(['+', 'Z', 'z']).next()?;
    let time: Vec<&str> = time.split(':').collect();
    let number = |part: &str| part.trim().parse::<u32>().ok();
    let (year, month, day) = (
        number(date.first()?)?,
        number(date.get(1)?)?,
        number(date.get(2)?)?,
    );
    let (hour, minute) = (number(time.first()?)?, number(time.get(1)?)?);
    let second = time
        .get(2)
        .map(|s| s.split('.').next().unwrap_or(s))
        .and_then(number)
        .unwrap_or(0);
    if year == 0
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let zone = offset
        .map(|o| o.trim().trim_matches('"'))
        .filter(|o| {
            o.len() == 6
                && matches!(o.as_bytes()[0], b'+' | b'-')
                && o.as_bytes()[3] == b':'
                && o[1..3].chars().all(|c| c.is_ascii_digit())
                && o[4..].chars().all(|c| c.is_ascii_digit())
        })
        .unwrap_or("Z");
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{zone}"
    ))
}

#[cfg(test)]
mod capture_time_tests {
    use super::normalise_capture_time as n;
    use super::{altitude_ref_text, standard_altitude};

    #[test]
    fn the_forms_cameras_and_readers_write_become_one_iso_timestamp() {
        assert_eq!(
            n("2016:09:02 10:28:00", None).as_deref(),
            Some("2016-09-02T10:28:00Z")
        );
        assert_eq!(
            n("2016-09-02 10:28:00", None).as_deref(),
            Some("2016-09-02T10:28:00Z")
        );
        assert_eq!(
            n("2016:09:02 10:28:00", Some("+02:00")).as_deref(),
            Some("2016-09-02T10:28:00+02:00")
        );
        assert_eq!(
            n("2016-09-02T10:28:00Z", None).as_deref(),
            Some("2016-09-02T10:28:00Z")
        );
        assert_eq!(
            n("2016:09:02 10:28:00.45", Some("bogus")).as_deref(),
            Some("2016-09-02T10:28:00Z")
        );
    }

    /// The EXIF of a file with these GPS altitude fields, as a reader would meet it.
    fn exif_with(altitude: Option<(u32, u32)>, reference: Option<u8>) -> exif::Exif {
        let mut fields = Vec::new();
        if let Some((num, denom)) = altitude {
            fields.push(exif::Field {
                tag: exif::Tag::GPSAltitude,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Rational(vec![exif::Rational { num, denom }]),
            });
        }
        if let Some(byte) = reference {
            fields.push(exif::Field {
                tag: exif::Tag::GPSAltitudeRef,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Byte(vec![byte]),
            });
        }
        let mut writer = exif::experimental::Writer::new();
        for field in &fields {
            writer.push_field(field);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        writer.write(&mut out, false).expect("the EXIF is written");
        exif::Reader::new()
            .read_raw(out.into_inner())
            .expect("the EXIF reads back")
    }

    #[test]
    fn an_altitude_below_sea_level_keeps_its_reference() {
        let below = standard_altitude(&exif_with(Some((4300, 100)), Some(1)));
        assert_eq!(below, (Some("4300/100".into()), Some("1".into())));
        let above = standard_altitude(&exif_with(Some((180, 1)), Some(0)));
        assert_eq!(above, (Some("180/1".into()), Some("0".into())));
    }

    #[test]
    fn a_file_that_does_not_say_which_side_of_sea_level_says_nothing() {
        assert_eq!(
            standard_altitude(&exif_with(Some((180, 1)), None)),
            (Some("180/1".into()), None)
        );
        // A reference with no altitude to qualify, or a value the standard does not have, is ignored.
        assert_eq!(standard_altitude(&exif_with(None, Some(1))), (None, None));
        assert_eq!(
            standard_altitude(&exif_with(Some((180, 1)), Some(7))),
            (Some("180/1".into()), None)
        );
        assert_eq!(altitude_ref_text(0), Some("0".into()));
        assert_eq!(altitude_ref_text(2), None);
    }

    #[test]
    fn text_that_is_not_a_date_is_refused() {
        for bad in [
            "",
            "0000:00:00 00:00:00",
            "yesterday",
            "2016:13:02 10:28:00",
            "2016:09:02",
            "2016:09:02 25:00:00",
        ] {
            assert_eq!(n(bad, None), None, "{bad:?}");
        }
    }
}
