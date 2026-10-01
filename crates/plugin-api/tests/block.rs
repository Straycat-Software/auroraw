// SPDX-License-Identifier: MIT OR Apache-2.0
//! The block that carries a decoder's metadata across the sandbox (D-141): what is written is read back exactly, every
//! way a block can be wrong is an error and never a panic, and an unknown tag is skipped.

use auroraw_plugin_api::block::{MAGIC, MAX_BLOCK_LEN, VERSION, tag};
use auroraw_plugin_api::{
    BlackLevel, BlockError, CameraId, ColourMatrix, Illuminant, InputProfile, Levels, NamedProfile,
    NoiseProfile, Orientation, RawImage, Rect, SampleKind, Samples, SensorLayout,
};
use proptest::prelude::*;

/// A Bayer image of 4 x 2 pixels with everything present.
fn bayer() -> RawImage {
    RawImage {
        width: 4,
        height: 2,
        layout: SensorLayout::Cfa {
            width: 2,
            height: 2,
            colours: vec![0, 1, 1, 2],
        },
        samples: Samples::U16((0..8).collect()),
        levels: Levels {
            black: BlackLevel {
                rows: 2,
                columns: 2,
                components: 1,
                values: vec![2047.0, 2047.0, 2048.0, 2047.0],
            },
            white: vec![15520.0],
        },
        white_balance: Some([1.99, 1.0, 1.47]),
        colour: vec![
            ColourMatrix {
                illuminant: Illuminant::D65,
                rows: 3,
                xyz_to_camera: vec![0.7, -0.1, -0.1, -0.4, 1.2, 0.1, -0.1, 0.2, 0.6],
            },
            ColourMatrix {
                illuminant: Illuminant::A,
                rows: 3,
                xyz_to_camera: vec![0.8, -0.2, 0.0, -0.3, 1.1, 0.2, 0.0, 0.1, 0.5],
            },
        ],
        crop: Some(Rect {
            x: 12,
            y: 21,
            width: 3,
            height: 1,
        }),
        active_area: Some(Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        }),
        orientation: Orientation::Rotate90,
        camera: CameraId {
            make: "Nikon".into(),
            model: "D850".into(),
        },
        iso: Some(400),
        noise_profile: Some(NoiseProfile {
            pairs: vec![[1.5e-5, 2.0e-7], [1.6e-5, 2.1e-7]],
        }),
    }
}

/// The samples of `image`, which a reader gets by its own path.
fn samples_of(image: &RawImage) -> Samples {
    image.samples.clone()
}

fn round_trip(image: &RawImage) -> RawImage {
    let block = image.to_block().expect("a valid image is written");
    let (kind, count) = RawImage::block_samples(&block).expect("the samples are described");
    assert_eq!(
        (kind, count),
        (image.samples.kind(), image.samples.len() as u64)
    );
    RawImage::from_block(&block, samples_of(image)).expect("what was written is read")
}

#[test]
fn an_image_with_everything_comes_back_exactly() {
    let image = bayer();
    assert_eq!(round_trip(&image), image);
}

#[test]
fn an_image_with_nothing_optional_comes_back_exactly() {
    // A monochrome file: no matrix, no white balance, no crop, no camera, no ISO, nothing.
    let image = RawImage {
        width: 2,
        height: 2,
        layout: SensorLayout::Mono,
        samples: Samples::F32(vec![0.0, 0.25, 0.5, 1.0]),
        levels: Levels {
            black: BlackLevel {
                rows: 1,
                columns: 1,
                components: 1,
                values: vec![220.0],
            },
            white: vec![65535.0],
        },
        white_balance: None,
        colour: Vec::new(),
        crop: None,
        active_area: None,
        orientation: Orientation::Unknown,
        camera: CameraId::default(),
        iso: None,
        noise_profile: None,
    };
    assert_eq!(round_trip(&image), image);
}

#[test]
fn pixels_that_have_their_channels_carry_the_colours_they_are_in() {
    for profile in [
        InputProfile::Unspecified,
        InputProfile::Named(NamedProfile::Srgb),
        InputProfile::Named(NamedProfile::AdobeRgb),
        InputProfile::Named(NamedProfile::Rec2020),
        InputProfile::Named(NamedProfile::ProPhoto),
        InputProfile::Icc(vec![1, 2, 3, 4, 5]),
    ] {
        let image = RawImage {
            width: 2,
            height: 1,
            layout: SensorLayout::LinearRgb {
                components: 3,
                profile: profile.clone(),
            },
            samples: Samples::U16(vec![1, 2, 3, 4, 5, 6]),
            ..RawImage {
                levels: Levels {
                    black: BlackLevel {
                        rows: 1,
                        columns: 1,
                        components: 3,
                        values: vec![0.0, 0.0, 0.0],
                    },
                    white: vec![65535.0, 65535.0, 65535.0],
                },
                ..bayer()
            }
        };
        assert_eq!(round_trip(&image).layout, image.layout, "{profile:?}");
    }
}

#[test]
fn the_block_starts_with_its_magic_and_its_version() {
    let block = bayer().to_block().unwrap();
    assert_eq!(block[..4], MAGIC);
    assert_eq!(u16::from_le_bytes([block[4], block[5]]), VERSION);
}

#[test]
fn a_tag_this_reader_does_not_know_is_skipped_wherever_it_is() {
    let image = bayer();
    let block = image.to_block().unwrap();
    for unknown in [99u16, 0, tag::PRIVATE, 0xFFFF] {
        let mut extra = Vec::new();
        extra.extend_from_slice(&unknown.to_le_bytes());
        extra.extend_from_slice(&5u32.to_le_bytes());
        extra.extend_from_slice(&[1, 2, 3, 4, 5]);
        // At the end, and at the start (right after the header).
        let mut at_end = block.clone();
        at_end.extend_from_slice(&extra);
        let mut at_start = block[..8].to_vec();
        at_start.extend_from_slice(&extra);
        at_start.extend_from_slice(&block[8..]);
        for text in [at_end, at_start] {
            let read = RawImage::from_block(&text, samples_of(&image)).unwrap();
            assert_eq!(read, image, "tag {unknown}");
        }
    }
}

#[test]
fn a_flag_this_reader_does_not_know_is_ignored() {
    let image = bayer();
    let mut block = image.to_block().unwrap();
    block[6] = 0xFF;
    assert_eq!(
        RawImage::from_block(&block, samples_of(&image)).unwrap(),
        image
    );
}

#[test]
fn a_block_that_is_not_well_formed_is_refused() {
    let image = bayer();
    let block = image.to_block().unwrap();
    let samples = samples_of(&image);
    let read = |b: &[u8]| RawImage::from_block(b, samples.clone());

    assert_eq!(read(&[]), Err(BlockError::Truncated));
    assert_eq!(read(&block[..7]), Err(BlockError::Truncated));
    let mut wrong = block.clone();
    wrong[0] = b'X';
    assert_eq!(read(&wrong), Err(BlockError::BadMagic));
    let mut newer = block.clone();
    newer[4] = (VERSION + 1) as u8;
    assert_eq!(
        read(&newer),
        Err(BlockError::UnsupportedVersion(VERSION + 1))
    );
    let mut zero = block.clone();
    zero[4] = 0;
    zero[5] = 0;
    assert_eq!(read(&zero), Err(BlockError::UnsupportedVersion(0)));
    // The block cut in the middle of a section header, and in the middle of a payload.
    assert_eq!(read(&block[..11]), Err(BlockError::Truncated));
    assert!(matches!(
        read(&block[..20]),
        Err(BlockError::SectionPastEnd(_))
    ));
    // Over the cap.
    let mut huge = block.clone();
    huge.resize(MAX_BLOCK_LEN + 1, 0);
    assert_eq!(read(&huge), Err(BlockError::TooLong(MAX_BLOCK_LEN + 1)));
}

/// The block of `image`, with the section `tag` removed.
fn without(block: &[u8], removed: u16) -> Vec<u8> {
    let mut out = block[..8].to_vec();
    let mut rest = &block[8..];
    while !rest.is_empty() {
        let t = u16::from_le_bytes([rest[0], rest[1]]);
        let n = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
        if t != removed {
            out.extend_from_slice(&rest[..6 + n]);
        }
        rest = &rest[6 + n..];
    }
    out
}

/// The block of `image`, with `again` the section `tag` of the block repeated at its end.
fn with_repeated(block: &[u8], again: u16) -> Vec<u8> {
    let mut rest = &block[8..];
    let mut out = block.to_vec();
    while !rest.is_empty() {
        let t = u16::from_le_bytes([rest[0], rest[1]]);
        let n = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
        if t == again {
            out.extend_from_slice(&rest[..6 + n]);
        }
        rest = &rest[6 + n..];
    }
    out
}

#[test]
fn a_required_section_that_is_missing_is_named() {
    let image = bayer();
    let block = image.to_block().unwrap();
    for required in [tag::GEOMETRY, tag::LAYOUT, tag::SAMPLES, tag::LEVELS] {
        assert_eq!(
            RawImage::from_block(&without(&block, required), samples_of(&image)),
            Err(BlockError::Missing(required)),
            "tag {required}"
        );
    }
}

#[test]
fn a_section_that_may_occur_once_does_not_occur_twice() {
    let image = bayer();
    let block = image.to_block().unwrap();
    for once in [
        tag::GEOMETRY,
        tag::LAYOUT,
        tag::SAMPLES,
        tag::LEVELS,
        tag::WHITE_BALANCE,
        tag::CROP,
        tag::ACTIVE_AREA,
        tag::ORIENTATION,
        tag::CAMERA,
        tag::ISO,
        tag::NOISE_PROFILE,
    ] {
        assert_eq!(
            RawImage::from_block(&with_repeated(&block, once), samples_of(&image)),
            Err(BlockError::Duplicate(once)),
            "tag {once}"
        );
    }
    // The colour matrices are the repeatable ones: a third is a third.
    let three = with_repeated(&block, tag::COLOUR_MATRIX);
    assert_eq!(
        RawImage::from_block(&three, samples_of(&image))
            .unwrap()
            .colour
            .len(),
        4
    );
}

#[test]
fn the_samples_must_be_the_ones_the_block_describes() {
    let image = bayer();
    let block = image.to_block().unwrap();
    assert!(matches!(
        RawImage::from_block(&block, Samples::U16(vec![0; 7])),
        Err(BlockError::SamplesMismatch { .. })
    ));
    assert!(matches!(
        RawImage::from_block(&block, Samples::F32(vec![0.0; 8])),
        Err(BlockError::SamplesMismatch { .. })
    ));
    assert_eq!(
        Samples::from_le_bytes(SampleKind::U16, &[1, 2, 3]),
        Err(BlockError::SamplesNotWhole { bytes: 3, size: 2 })
    );
    assert_eq!(
        Samples::from_le_bytes(SampleKind::U16, &[1, 0, 2, 0]),
        Ok(Samples::U16(vec![1, 2]))
    );
    assert_eq!(
        Samples::from_le_bytes(SampleKind::F32, &1.5f32.to_le_bytes()),
        Ok(Samples::F32(vec![1.5]))
    );
}

#[test]
fn a_nan_or_an_infinity_is_refused_both_ways() {
    // Written: an image with one is not an image a block can carry.
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut image = bayer();
        image.white_balance = Some([1.0, bad, 1.0]);
        assert_eq!(
            image.to_block(),
            Err(BlockError::NotFinite(tag::WHITE_BALANCE))
        );
        let mut image = bayer();
        image.levels.black.values[0] = bad;
        assert_eq!(image.to_block(), Err(BlockError::NotFinite(tag::LEVELS)));
        let mut image = bayer();
        image.colour[0].xyz_to_camera[4] = bad;
        assert_eq!(
            image.to_block(),
            Err(BlockError::NotFinite(tag::COLOUR_MATRIX))
        );
    }
    let mut image = bayer();
    image.noise_profile = Some(NoiseProfile {
        pairs: vec![[f64::NAN, 0.0]],
    });
    assert_eq!(
        image.to_block(),
        Err(BlockError::NotFinite(tag::NOISE_PROFILE))
    );
    // Read: a block from another writer that carries one.
    let image = bayer();
    let mut block = image.to_block().unwrap();
    let at = block
        .windows(4)
        .position(|w| w == 1.99f32.to_le_bytes())
        .expect("the white balance is in the block");
    block[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        RawImage::from_block(&block, samples_of(&image)),
        Err(BlockError::NotFinite(tag::WHITE_BALANCE))
    );
}

#[test]
fn an_image_that_cannot_be_what_it_says_is_refused() {
    type Break = Box<dyn Fn(&mut RawImage)>;
    let broken: Vec<(&str, Break)> = vec![
        ("no pixels", Box::new(|i| i.width = 0)),
        (
            "a pattern of the wrong size",
            Box::new(|i| {
                i.layout = SensorLayout::Cfa {
                    width: 2,
                    height: 2,
                    colours: vec![0, 1, 1],
                }
            }),
        ),
        (
            "a colour above 3",
            Box::new(|i| {
                i.layout = SensorLayout::Cfa {
                    width: 2,
                    height: 2,
                    colours: vec![0, 1, 1, 7],
                }
            }),
        ),
        (
            "samples that are not width * height",
            Box::new(|i| i.samples = Samples::U16(vec![0; 3])),
        ),
        (
            "an empty black pattern",
            Box::new(|i| i.levels.black.rows = 0),
        ),
        (
            "black values that are not rows * columns * components",
            Box::new(|i| i.levels.black.values.pop().map(|_| ()).unwrap_or(())),
        ),
        ("no white", Box::new(|i| i.levels.white.clear())),
        (
            "a matrix of 5 rows",
            Box::new(|i| {
                i.colour[0].rows = 5;
                i.colour[0].xyz_to_camera = vec![0.0; 15];
            }),
        ),
        (
            "a matrix whose values are not rows * 3",
            Box::new(|i| i.colour[0].xyz_to_camera.pop().map(|_| ()).unwrap_or(())),
        ),
        (
            "no noise pair",
            Box::new(|i| i.noise_profile = Some(NoiseProfile { pairs: Vec::new() })),
        ),
    ];
    for (what, make_it) in broken {
        let mut image = bayer();
        make_it(&mut image);
        assert!(image.to_block().is_err(), "{what} is written");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Properties.
// ---------------------------------------------------------------------------------------------------------------------

fn any_f32() -> impl Strategy<Value = f32> {
    // Every finite value, the negative zero and the subnormals included.
    any::<f32>().prop_filter("finite", |v| v.is_finite())
}

fn any_f64() -> impl Strategy<Value = f64> {
    any::<f64>().prop_filter("finite", |v| v.is_finite())
}

fn any_rect() -> impl Strategy<Value = Rect> {
    (any::<u32>(), any::<u32>(), any::<u32>(), any::<u32>()).prop_map(|(x, y, width, height)| {
        Rect {
            x,
            y,
            width,
            height,
        }
    })
}

fn any_image() -> impl Strategy<Value = RawImage> {
    let layout = prop_oneof![
        (1u8..=6, 1u8..=6).prop_flat_map(|(w, h)| {
            proptest::collection::vec(0u8..=3, usize::from(w) * usize::from(h)).prop_map(
                move |colours| SensorLayout::Cfa {
                    width: w,
                    height: h,
                    colours,
                },
            )
        }),
        (
            1u8..=4,
            prop_oneof![
                Just(InputProfile::Unspecified),
                Just(InputProfile::Named(NamedProfile::Srgb)),
                Just(InputProfile::Named(NamedProfile::ProPhoto)),
                proptest::collection::vec(any::<u8>(), 0..64).prop_map(InputProfile::Icc),
            ]
        )
            .prop_map(|(components, profile)| SensorLayout::LinearRgb {
                components,
                profile
            }),
        Just(SensorLayout::Mono),
    ];
    let black = (1u16..=3, 1u16..=3, 1u8..=3).prop_flat_map(|(rows, columns, components)| {
        proptest::collection::vec(
            any_f32(),
            usize::from(rows) * usize::from(columns) * usize::from(components),
        )
        .prop_map(move |values| BlackLevel {
            rows,
            columns,
            components,
            values,
        })
    });
    let matrix = (any::<u8>(), 3u8..=4).prop_flat_map(|(illuminant, rows)| {
        proptest::collection::vec(any_f32(), usize::from(rows) * 3).prop_map(move |xyz_to_camera| {
            ColourMatrix {
                illuminant: Illuminant(illuminant),
                rows,
                xyz_to_camera,
            }
        })
    });
    (
        (1u32..=4, 1u32..=4, layout, black),
        proptest::collection::vec(any_f32(), 1..=4),
        proptest::option::of([any_f32(), any_f32(), any_f32()]),
        proptest::collection::vec(matrix, 0..=3),
        (
            proptest::option::of(any_rect()),
            proptest::option::of(any_rect()),
        ),
        (0u8..=8, "[ -~]{0,12}", "[ -~]{0,12}"),
        (
            proptest::option::of(any::<u32>()),
            proptest::option::of(proptest::collection::vec([any_f64(), any_f64()], 1..=4)),
        ),
        any::<bool>(),
    )
        .prop_flat_map(
            |(
                (width, height, layout, black),
                white,
                wb,
                colour,
                (crop, active),
                (orientation, make, model),
                (iso, noise),
                float,
            )| {
                let components = match &layout {
                    SensorLayout::LinearRgb { components, .. } => usize::from(*components),
                    _ => 1,
                };
                let count = width as usize * height as usize * components;
                let samples = if float {
                    proptest::collection::vec(any_f32(), count)
                        .prop_map(Samples::F32)
                        .boxed()
                } else {
                    proptest::collection::vec(any::<u16>(), count)
                        .prop_map(Samples::U16)
                        .boxed()
                };
                samples.prop_map(move |samples| RawImage {
                    width,
                    height,
                    layout: layout.clone(),
                    samples,
                    levels: Levels {
                        black: black.clone(),
                        white: white.clone(),
                    },
                    white_balance: wb,
                    colour: colour.clone(),
                    crop,
                    active_area: active,
                    orientation: Orientation::from_exif(orientation),
                    camera: CameraId {
                        make: make.clone(),
                        model: model.clone(),
                    },
                    iso,
                    noise_profile: noise.clone().map(|pairs| NoiseProfile { pairs }),
                })
            },
        )
}

proptest! {
    #[test]
    fn what_is_written_is_read_back_exactly(image in any_image()) {
        prop_assert_eq!(round_trip(&image), image);
    }

    #[test]
    fn a_block_is_deterministic(image in any_image()) {
        prop_assert_eq!(image.to_block().unwrap(), image.to_block().unwrap());
    }

    #[test]
    fn no_bytes_make_the_reader_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = RawImage::block_samples(&bytes);
        let _ = RawImage::from_block(&bytes, Samples::U16(Vec::new()));
        // And the same bytes after a valid header, where the sections are parsed.
        let mut framed = MAGIC.to_vec();
        framed.extend_from_slice(&VERSION.to_le_bytes());
        framed.extend_from_slice(&[0, 0]);
        framed.extend_from_slice(&bytes);
        let _ = RawImage::block_samples(&framed);
        let _ = RawImage::from_block(&framed, Samples::U16(Vec::new()));
    }

    #[test]
    fn a_block_with_one_byte_changed_is_an_error_or_another_image_and_never_a_panic(
        image in any_image(),
        at in any::<prop::sample::Index>(),
        byte in any::<u8>(),
    ) {
        let mut block = image.to_block().unwrap();
        let i = at.index(block.len());
        block[i] = byte;
        let _ = RawImage::from_block(&block, image.samples.clone());
    }
}
