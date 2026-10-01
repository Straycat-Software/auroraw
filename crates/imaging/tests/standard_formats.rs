// SPDX-License-Identifier: GPL-3.0-or-later
//! JPEG, PNG and TIFF, decoded directly (no embedded preview to extract: the file already is
//! the image). Synthetic fixtures, generated in the test, so nothing external is needed.

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

fn write_test_image(path: &std::path::Path, format: ImageFormat) {
    let img = ImageBuffer::from_fn(64, 48, |x, y| Rgb([(x * 4) as u8, (y * 5) as u8, 128]));
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, format)
        .unwrap();
}

#[test]
fn jpeg_png_and_tiff_all_decode_to_a_thumbnail_never_upscaled() {
    let dir = auroraw_testkit::temp_dir();
    for (name, format) in [
        ("a.jpg", ImageFormat::Jpeg),
        ("a.png", ImageFormat::Png),
        ("a.tif", ImageFormat::Tiff),
    ] {
        let path = dir.path().join(name);
        write_test_image(&path, format);

        let (metadata, thumbnail, _hash) =
            auroraw_imaging::process(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(metadata.pixel_width, Some(64), "{name}");
        assert_eq!(metadata.pixel_height, Some(48), "{name}");
        assert!(!thumbnail.jpeg.is_empty(), "{name}");
        // Smaller than the 256 px target already: never upscaled.
        assert_eq!((thumbnail.width, thumbnail.height), (64, 48), "{name}");
    }
}

#[test]
fn a_corrupt_standard_file_fails_cleanly_not_a_panic() {
    let dir = auroraw_testkit::temp_dir();
    let path = dir.path().join("bad.jpg");
    std::fs::write(&path, b"not a jpeg at all, just some arbitrary bytes").unwrap();
    assert!(auroraw_imaging::embedded_preview(&path).is_err());
}

#[test]
fn an_unrecognised_extension_is_refused_cleanly() {
    let dir = auroraw_testkit::temp_dir();
    let path = dir.path().join("photo.xyz");
    std::fs::write(&path, b"whatever").unwrap();
    assert!(auroraw_imaging::read_metadata(&path).is_err());
    assert!(auroraw_imaging::embedded_preview(&path).is_err());
}

/// A JPEG with this GPS altitude in its EXIF: the test image with an APP1 segment spliced in after the
/// start-of-image marker.
fn jpeg_with_altitude(path: &std::path::Path, altitude: (u32, u32), reference: Option<u8>) {
    write_test_image(path, ImageFormat::Jpeg);
    let mut writer = exif::experimental::Writer::new();
    let mut fields = vec![exif::Field {
        tag: exif::Tag::GPSAltitude,
        ifd_num: exif::In::PRIMARY,
        value: exif::Value::Rational(vec![exif::Rational {
            num: altitude.0,
            denom: altitude.1,
        }]),
    }];
    if let Some(byte) = reference {
        fields.push(exif::Field {
            tag: exif::Tag::GPSAltitudeRef,
            ifd_num: exif::In::PRIMARY,
            value: exif::Value::Byte(vec![byte]),
        });
    }
    for field in &fields {
        writer.push_field(field);
    }
    let mut tiff = std::io::Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();
    let length = u16::try_from(2 + 6 + tiff.len()).unwrap();
    let image = std::fs::read(path).unwrap();
    let mut spliced = image[..2].to_vec();
    spliced.extend([0xFF, 0xE1]);
    spliced.extend(length.to_be_bytes());
    spliced.extend(b"Exif\0\0");
    spliced.extend(tiff);
    spliced.extend(&image[2..]);
    std::fs::write(path, spliced).unwrap();
}

#[test]
fn a_photo_taken_below_sea_level_is_read_with_its_reference() {
    let dir = auroraw_testkit::temp_dir();
    let cases = [
        ("below.jpg", Some(1), Some("1")),
        ("above.jpg", Some(0), Some("0")),
        ("unsaid.jpg", None, None),
    ];
    for (name, byte, expected) in cases {
        let path = dir.path().join(name);
        jpeg_with_altitude(&path, (4300, 10), byte);
        let metadata =
            auroraw_imaging::read_metadata(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(metadata.gps_altitude.as_deref(), Some("4300/10"), "{name}");
        assert_eq!(metadata.gps_altitude_ref.as_deref(), expected, "{name}");
    }
}
