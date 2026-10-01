// SPDX-License-Identifier: GPL-3.0-or-later
//! Photo and version sidecars: fixtures, round trips, unknown content, the derived copy and its
//! digest (testing strategy §3, design note 003 §9).

use auroraw_format::sidecar::*;
use auroraw_format::state::Loaded;
use auroraw_format::xmp::{Property, Xmp, ns};
use auroraw_types::{ContentHash, Fingerprint, KeywordId, PhotoId, SourceId, Timestamp, VersionId};
use proptest::prelude::*;

fn photo_id() -> PhotoId {
    "3f2a91c0d77e4b5a8c1e0f9d2b6a4c31".parse().unwrap()
}

fn kid(n: u8) -> KeywordId {
    KeywordId::from_bytes([n; 8])
}

/// A fully populated photo sidecar with fixed values.
fn full_photo() -> PhotoSidecar {
    let mut p = PhotoSidecar::new(photo_id());
    let m = &mut p.meta;
    m.rating = Some(4);
    m.flag = Some(Flag::Picked);
    m.label = Some("Green".into());
    m.title = Some("Heron at dawn".into());
    m.caption = Some("A grey heron & its reflection".into());
    m.push_keyword(kid(0x91), "Fauna|Birds|Heron");
    m.push_keyword(kid(0x92), "Places|Canada|Quebec");
    m.creator = vec!["Marie Tremblay".into()];
    m.rights = Some("(c) 2026 Marie Tremblay".into());
    m.usage_terms = Some("Editorial use only".into());
    m.web_statement = Some("https://example.org/rights".into());
    m.credit = Some("Marie Tremblay".into());
    m.source = Some("Own work".into());
    m.headline = Some("Dawn".into());
    m.instructions = Some("Embargo lifted".into());
    m.sublocation = Some("Lac Saint-Jean".into());
    m.city = Some("Roberval".into());
    m.region = Some("Quebec".into());
    m.country = Some("Canada".into());
    m.country_code = Some("CA".into());
    m.persons = vec!["Marie".into()];
    m.event = Some("Wedding".into());
    m.custom = vec![CustomField {
        name: "Model release".into(),
        value: "on file".into(),
    }];
    m.original.capture_time = Some("2026-05-14T06:41:09.250-04:00".into());
    m.original.make = Some("SONY".into());
    m.original.model = Some("ILCE-7RM4".into());
    m.original.serial = Some("1234567".into());
    m.original.lens = Some("FE 100-400mm F4.5-5.6 GM OSS".into());
    m.original.exposure_time = Some("1/1000".into());
    m.original.f_number = Some("56/10".into());
    m.original.iso = vec!["400".into()];
    m.original.focal_length = Some("4000/10".into());
    m.original.focal_length_35mm = Some("400".into());
    m.original.pixel_width = Some(9504);
    m.original.pixel_height = Some(6336);
    m.original.orientation = Some(1);
    m.original.gps_latitude = Some("48,31.5000N".into());
    m.original.gps_longitude = Some("72,13.2000W".into());
    m.original.gps_altitude = Some("1800/10".into());
    m.overlay = Some(Overlay {
        capture_time: Some("2026-05-14T07:41:09.250-04:00".into()),
        gps: Some(OverlayGps {
            latitude: "48,31.6000N".into(),
            longitude: "72,13.3000W".into(),
            altitude: None,
            extra: vec![],
        }),
        camera: Some("Sony A7R IV".into()),
        lens: None,
        orientation: Some(6),
        extra: vec![],
    });
    p.files = vec![
        FileEntry {
            role: FileRole::Original,
            name: "DSC00396.ARW".into(),
            format: Some("ARW".into()),
            size: 61_815_808,
            fingerprint: Fingerprint::from_bytes([1; 32]),
            hash: Some(ContentHash::from_bytes([2; 32])),
            locations: vec![Location {
                source: SourceId::from_bytes([0xb8; 8]),
                path: "2026/2026-05-14/DSC00396.ARW".into(),
                seen: Some("2026-09-21T14:02:11Z".parse().unwrap()),
                extra: vec![],
            }],
            extra: vec![],
        },
        FileEntry {
            role: FileRole::Companion,
            name: "DSC00396.JPG".into(),
            format: Some("JPEG".into()),
            size: 12_345_678,
            fingerprint: Fingerprint::from_bytes([3; 32]),
            hash: None,
            locations: vec![],
            extra: vec![],
        },
    ];
    p.main_version = Some(VersionId::from_bytes([0x7b; 8]));
    p.imported = Some("2026-09-21T14:02:11Z".parse().unwrap());
    p
}

fn fixture_path(name: &str) -> String {
    format!(
        "{}/tests/fixtures/sidecars/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Compares bytes with a fixture; `UPDATE_FIXTURES=1` rewrites it (an explicit, reviewed act).
fn check_fixture(name: &str, bytes: &[u8]) {
    let path = fixture_path(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, bytes).unwrap();
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("missing fixture {name}; run with UPDATE_FIXTURES=1"));
    assert_eq!(
        String::from_utf8_lossy(bytes),
        String::from_utf8_lossy(&expected),
        "{name}"
    );
}

fn current<T>(l: Loaded<T>) -> T {
    l.current().expect("a current schema")
}

#[test]
fn the_full_photo_sidecar_matches_its_fixture_and_reads_back() {
    let photo = full_photo();
    let bytes = photo.to_bytes();
    check_fixture("photo-full.xmp", &bytes);
    let back = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(back, photo);
    assert_eq!(back.to_bytes(), bytes);
    assert!(
        back.extra.is_empty(),
        "everything written is understood: {:?}",
        back.extra
    );
}

#[test]
fn the_fixture_file_itself_reads_and_rewrites_identically() {
    let bytes = std::fs::read(fixture_path("photo-full.xmp")).unwrap();
    let photo = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(photo.to_bytes(), bytes);
}

/// A photo taken at the Dead Sea: 430 m below sea level (`exif:GPSAltitude` is a distance, never negative;
/// `exif:GPSAltitudeRef` says which side).
fn dead_sea_photo() -> PhotoSidecar {
    let mut p = PhotoSidecar::new(photo_id());
    let o = &mut p.meta.original;
    o.capture_time = Some("2026-03-02T09:15:00+02:00".into());
    o.gps_latitude = Some("31,30.0000N".into());
    o.gps_longitude = Some("35,28.0000E".into());
    o.gps_altitude = Some("4300/10".into());
    o.gps_altitude_ref = Some("1".into());
    p.imported = Some("2026-09-21T14:02:11Z".parse().unwrap());
    p
}

#[test]
fn an_altitude_below_sea_level_keeps_its_reference_in_the_sidecar() {
    let photo = dead_sea_photo();
    let bytes = photo.to_bytes();
    check_fixture("photo-below-sea-level.xmp", &bytes);
    assert!(
        String::from_utf8_lossy(&bytes).contains("<exif:GPSAltitudeRef>1</exif:GPSAltitudeRef>"),
        "written as the XMP standard spells it, for any reader"
    );
    let back = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(back, photo);
    assert_eq!(back.to_bytes(), bytes);
    assert!(back.extra.is_empty(), "understood: {:?}", back.extra);
}

#[test]
fn a_sidecar_written_before_the_reference_was_kept_reads_without_one_and_is_not_given_one() {
    // `photo-full.xmp` has an altitude and no reference: every sidecar of the first releases. It still reads
    // (as above sea level, which is what it always meant) and rewrites as it was.
    let bytes = std::fs::read(fixture_path("photo-full.xmp")).unwrap();
    let photo = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(photo.meta.original.gps_altitude.as_deref(), Some("1800/10"));
    assert_eq!(photo.meta.original.gps_altitude_ref, None);
    assert_eq!(photo.to_bytes(), bytes);
}

/// A photo whose place names Auroraw found (design note 008): the record says which fields are still its.
fn placed_photo() -> PhotoSidecar {
    let mut p = PhotoSidecar::new(photo_id());
    let m = &mut p.meta;
    m.original.gps_latitude = Some("45,30.1140N".into());
    m.original.gps_longitude = Some("73,34.0440W".into());
    m.city = Some("Montréal".into());
    m.region = Some("Quebec".into());
    m.country = Some("Canada".into());
    m.country_code = Some("CA".into());
    m.place_filled = Some(PlaceFilled {
        latitude: "45.50190".into(),
        longitude: "-73.56740".into(),
        city: Some("Montréal".into()),
        region: Some("Quebec".into()),
        country: Some("Canada".into()),
        country_code: Some("CA".into()),
        extra: vec![],
    });
    p.imported = Some("2026-09-21T14:02:11Z".parse().unwrap());
    p
}

#[test]
fn the_record_of_filled_place_names_matches_its_fixture_and_reads_back() {
    let photo = placed_photo();
    let bytes = photo.to_bytes();
    check_fixture("photo-places.xmp", &bytes);
    let back = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(back, photo);
    assert_eq!(back.to_bytes(), bytes);
    assert!(back.extra.is_empty(), "understood: {:?}", back.extra);
    // Other software reads the four fields as the IPTC fields they are; the record is Auroraw's own.
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("<photoshop:City>Montréal</photoshop:City>"),
        "{text}"
    );
    assert!(text.contains("aur:PlaceFilled"), "{text}");
}

#[test]
fn a_person_writing_a_field_takes_it_out_of_the_record_and_the_record_goes_with_the_last() {
    let mut meta = placed_photo().meta;
    // Writing a field releases it, whatever is written: the same text, another, or nothing.
    assert!(meta.release_place_field(PlaceField::City));
    assert!(
        !meta.release_place_field(PlaceField::City),
        "already theirs"
    );
    let record = meta.place_filled.as_ref().expect("three fields are left");
    assert_eq!(record.get(PlaceField::City), None);
    assert_eq!(record.get(PlaceField::Region), Some("Quebec"));
    assert_eq!(
        meta.city.as_deref(),
        Some("Montréal"),
        "the text itself is kept"
    );
    for field in [
        PlaceField::Region,
        PlaceField::Country,
        PlaceField::CountryCode,
    ] {
        meta.release_place_field(field);
    }
    assert_eq!(meta.place_filled, None, "nothing of Auroraw's is left");
    // A photo that never had a record has nothing to release.
    let mut plain = Metadata::default();
    assert!(!plain.release_place_field(PlaceField::Country));
}

#[test]
fn what_a_newer_version_adds_to_the_record_is_kept() {
    let mut photo = placed_photo();
    let record = photo.meta.place_filled.as_mut().unwrap();
    record
        .extra
        .push(Property::text(ns::AUR, "TownId", "6077243"));
    let bytes = photo.to_bytes();
    let back = current(PhotoSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(back.meta.place_filled.as_ref().unwrap().extra.len(), 1);
    assert_eq!(back.to_bytes(), bytes);
    // Releasing every field does not drop a record that holds something we do not understand.
    let mut meta = back.meta;
    for field in [
        PlaceField::City,
        PlaceField::Region,
        PlaceField::Country,
        PlaceField::CountryCode,
    ] {
        meta.release_place_field(field);
    }
    assert!(meta.place_filled.is_some());
}

#[test]
fn stars_and_flag_are_kept_apart() {
    let mut p = PhotoSidecar::new(photo_id());
    p.meta.rating = Some(3);
    p.meta.flag = Some(Flag::Rejected);
    let text = String::from_utf8(p.to_bytes()).unwrap();
    assert!(text.contains("<xmp:Rating>3</xmp:Rating>"), "{text}");
    assert!(text.contains("<aur:Flag>rejected</aur:Flag>"), "{text}");
    let back = current(PhotoSidecar::from_bytes(text.as_bytes()).unwrap());
    assert_eq!(
        (back.meta.rating, back.meta.flag),
        (Some(3), Some(Flag::Rejected))
    );
}

#[test]
fn keywords_are_written_in_the_three_forms() {
    let text = String::from_utf8(full_photo().to_bytes()).unwrap();
    assert!(
        text.contains("<rdf:li>Heron</rdf:li>"),
        "the leaf names in dc:subject"
    );
    assert!(
        text.contains("<rdf:li>Fauna|Birds|Heron</rdf:li>"),
        "the path in lr:hierarchicalSubject"
    );
    assert!(
        text.contains("<rdf:li>9191919191919191</rdf:li>"),
        "the identifier in aur:KeywordIds"
    );
    let photo = current(PhotoSidecar::from_bytes(text.as_bytes()).unwrap());
    let keywords = photo.meta.keywords();
    assert_eq!(keywords[0].id, Some(kid(0x91)));
    assert_eq!(keywords[0].path, "Fauna|Birds|Heron");
}

#[test]
fn unknown_properties_survive_at_every_level() {
    let mut xmp = full_photo().to_xmp();
    // a property of another namespace, a property of ours from a newer version, and unknown
    // fields inside a file entry, a location and the overlay
    xmp.properties.push(Property::text(
        "http://ns.adobe.com/camera-raw-settings/1.0/",
        "Exposure2012",
        "+0.35",
    ));
    xmp.properties
        .push(Property::text(ns::AUR, "FutureThing", "kept"));
    for p in xmp.properties.iter_mut() {
        if p.is(ns::AUR, "Overlay")
            && let auroraw_format::xmp::Value::Struct(f) = &mut p.value
        {
            f.push(Property::text(ns::AUR, "Future", "in the overlay"));
        }
    }
    let photo = current(PhotoSidecar::from_bytes(&xmp.to_bytes()).unwrap());
    assert_eq!(photo.extra.len(), 2);
    assert_eq!(photo.meta.overlay.as_ref().unwrap().extra.len(), 1);
    let again = Xmp::from_bytes(&photo.to_bytes()).unwrap();
    for (ns_, name) in [
        (
            "http://ns.adobe.com/camera-raw-settings/1.0/",
            "Exposure2012",
        ),
        (ns::AUR, "FutureThing"),
    ] {
        assert!(again.get(ns_, name).is_some(), "{name}");
    }
    let overlay = again.get(ns::AUR, "Overlay").unwrap().as_fields().unwrap();
    assert!(overlay.iter().any(|f| f.name == "Future"));
}

#[test]
fn custom_fields_round_trip_several_and_write_nothing_when_there_are_none() {
    // WP10's own custom-field support: no UI creates one yet, but the format and the round trip
    // already work for any name, not just the fixed IPTC/XMP set above.
    let mut p = PhotoSidecar::new(photo_id());
    p.meta.custom = vec![
        CustomField {
            name: "Model release".into(),
            value: "on file".into(),
        },
        CustomField {
            name: "Internal ID".into(),
            value: "PR-2026-0417".into(),
        },
    ];
    let text = String::from_utf8(p.to_bytes()).unwrap();
    assert!(text.contains("<aur:Custom>"), "{text}");
    let back = current(PhotoSidecar::from_bytes(text.as_bytes()).unwrap());
    assert_eq!(back.meta.custom, p.meta.custom);

    let none = PhotoSidecar::new(photo_id());
    let text = String::from_utf8(none.to_bytes()).unwrap();
    assert!(
        !text.contains("aur:Custom"),
        "nothing to say, nothing written: {text}"
    );
}

#[test]
fn a_property_of_the_wrong_shape_is_kept_not_dropped() {
    let mut xmp = full_photo().to_xmp();
    xmp.take(ns::XMP, "Rating");
    xmp.properties
        .push(Property::text(ns::XMP, "Rating", "many"));
    let photo = current(PhotoSidecar::from_bytes(&xmp.to_bytes()).unwrap());
    assert_eq!(photo.meta.rating, None);
    assert!(photo.extra.iter().any(|p| p.is(ns::XMP, "Rating")));
    assert!(
        Xmp::from_bytes(&photo.to_bytes())
            .unwrap()
            .get(ns::XMP, "Rating")
            .is_some()
    );
}

#[test]
fn a_newer_schema_is_reported_and_not_interpreted() {
    let mut xmp = full_photo().to_xmp();
    xmp.take(ns::AUR, "Schema");
    xmp.properties
        .insert(0, Property::text(ns::AUR, "Schema", "7"));
    match PhotoSidecar::from_bytes(&xmp.to_bytes()).unwrap() {
        Loaded::Newer(s) => assert_eq!(s.0, 7),
        Loaded::Current(_) => panic!("must not be interpreted"),
    }
}

#[test]
fn foreign_xmp_is_not_a_sidecar() {
    let foreign = Xmp {
        properties: vec![Property::text(ns::XMP, "Rating", "3")],
        prefixes: vec![],
    };
    assert!(matches!(
        PhotoSidecar::from_bytes(&foreign.to_bytes()),
        Err(SidecarError::Missing(_))
    ));
    assert!(PhotoSidecar::from_bytes(b"garbage").is_err());
}

// ---- the version sidecar and its derived copy ----

fn version_of(photo: &PhotoSidecar) -> VersionSidecar {
    let mut v = VersionSidecar::new(photo, VersionId::from_bytes([0x7b; 8]));
    v.name = Some("Black and white".into());
    v.created = Some("2026-09-21T15:00:00Z".parse::<Timestamp>().unwrap());
    v
}

#[test]
fn a_new_version_copies_the_photo_and_matches_its_fixture() {
    let photo = full_photo();
    let v = version_of(&photo);
    assert_eq!(v.meta.rating, Some(4));
    assert!(v.is_copy_current(&photo));
    let bytes = v.to_bytes();
    check_fixture("version-plain.xmp", &bytes);
    let back = current(VersionSidecar::from_bytes(&bytes).unwrap());
    assert_eq!(back, v);
    assert_eq!(back.to_bytes(), bytes);
}

#[test]
fn overrides_win_over_the_photo_and_the_others_follow_it() {
    let mut photo = full_photo();
    let mut v = version_of(&photo);
    v.overrides = vec![OverrideField::Rating, OverrideField::Title];
    v.meta.rating = Some(1);
    v.meta.title = Some("Its own title".into());
    v.refresh_copy(&photo);
    check_fixture("version-overrides.xmp", &v.to_bytes());
    assert_eq!(
        v.meta.rating,
        Some(1),
        "an overridden field keeps the version's value"
    );

    // the photo changes: the caption follows, the overridden fields do not
    photo.meta.caption = Some("Another caption".into());
    photo.meta.rating = Some(5);
    photo.meta.title = Some("Another title".into());
    assert!(
        !v.is_copy_current(&photo),
        "the copy notices that the photo changed"
    );
    v.refresh_copy(&photo);
    assert!(v.is_copy_current(&photo));
    assert_eq!(v.meta.caption.as_deref(), Some("Another caption"));
    assert_eq!(v.meta.rating, Some(1));
    assert_eq!(v.meta.title.as_deref(), Some("Its own title"));
}

#[test]
fn a_version_adds_keywords_and_never_removes_the_photos() {
    let mut photo = full_photo();
    let mut v = version_of(&photo);
    v.meta.push_keyword(kid(0xaa), "Style|Black and white");
    v.added_keyword_ids = vec![kid(0xaa)];
    v.refresh_copy(&photo);
    assert_eq!(v.meta.keyword_ids.len(), 3);
    // the photo gets a new keyword: the union follows
    photo.meta.push_keyword(kid(0xbb), "Season|Spring");
    v.refresh_copy(&photo);
    assert_eq!(v.meta.keyword_ids.len(), 4);
    assert!(v.meta.keyword_ids.contains(&kid(0xaa)) && v.meta.keyword_ids.contains(&kid(0xbb)));
    // the photo drops one: the version follows, and keeps its own addition
    photo.meta.keyword_ids.remove(0);
    photo.meta.keyword_paths.remove(0);
    v.refresh_copy(&photo);
    assert!(!v.meta.keyword_ids.contains(&kid(0x91)) && v.meta.keyword_ids.contains(&kid(0xaa)));
}

#[test]
fn renaming_a_keyword_does_not_change_the_digest_or_make_copies_stale() {
    let mut photo = full_photo();
    let v = version_of(&photo);
    let before = photo.meta.digest();
    photo.meta.keyword_paths[0] = "Fauna|Birds|Grey heron".into(); // a rename: the snapshot changes
    assert_eq!(
        photo.meta.digest(),
        before,
        "the digest counts keywords by identifier"
    );
    assert!(v.is_copy_current(&photo));
    photo.meta.push_keyword(kid(0xcc), "New"); // a real change
    assert_ne!(photo.meta.digest(), before);
    assert!(!v.is_copy_current(&photo));
}

#[test]
fn the_digest_is_stable_and_known() {
    // If this changes, every version copy in every workspace looks out of date at once.
    let d = full_photo().meta.digest();
    assert_eq!(d.to_string(), KNOWN_DIGEST);
}

const KNOWN_DIGEST: &str =
    "blake3:607922d63704b59ebcc35ac642f20baf951852fa866cfed5848dd739c9a57afb";

#[test]
fn the_development_of_a_later_version_is_kept() {
    let photo = full_photo();
    let mut v = version_of(&photo);
    v.extra.push(Property::text(ns::AUR, "PipelineSchema", "2"));
    v.extra
        .push(Property::text(ns::AUR, "Pipeline", "{\"ops\":[]}"));
    let back = current(VersionSidecar::from_bytes(&v.to_bytes()).unwrap());
    assert_eq!(back.extra.len(), 2);
    assert_eq!(back.to_bytes(), v.to_bytes());
}

// ---- generated round trip ----

fn word() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            5 => proptest::char::range('a', 'z'), 1 => Just(' '), 1 => Just('&'), 1 => Just('<'),
            1 => Just('"'), 1 => Just('é'), 1 => Just('鷺'),
        ],
        0..10,
    )
    .prop_map(|c| c.into_iter().collect())
}

fn segment() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z ]{0,6}"
}

fn metadata() -> impl Strategy<Value = Metadata> {
    let strings = proptest::collection::vec(proptest::option::of(word()), 8);
    let keywords = proptest::collection::vec(
        (any::<[u8; 8]>(), proptest::collection::vec(segment(), 1..4)),
        0..4,
    );
    (
        proptest::option::of(0u8..=5),
        proptest::option::of(prop_oneof![Just(Flag::Picked), Just(Flag::Rejected)]),
        strings,
        keywords,
        proptest::collection::vec(word(), 0..3),
        proptest::option::of((1u32..20000, 1u32..20000, 1u32..9)),
    )
        .prop_map(|(rating, flag, s, kws, creators, dims)| {
            let mut m = Metadata {
                rating,
                flag,
                ..Metadata::default()
            };
            m.label = s[0].clone();
            m.title = s[1].clone();
            m.caption = s[2].clone();
            m.rights = s[3].clone();
            m.city = s[4].clone();
            m.original.make = s[5].clone();
            m.original.capture_time = s[6].clone();
            m.event = s[7].clone();
            for (id, path) in kws {
                m.push_keyword(KeywordId::from_bytes(id), path.join("|"));
            }
            m.creator = creators;
            if let Some((w, h, o)) = dims {
                m.original.pixel_width = Some(w);
                m.original.pixel_height = Some(h);
                m.original.orientation = Some(o);
            }
            m
        })
}

proptest! {
    #[test]
    fn any_photo_sidecar_reads_back_the_same(meta in metadata(), id in any::<[u8; 16]>(), size in any::<u64>(), fp in any::<[u8; 32]>()) {
        let mut p = PhotoSidecar::new(PhotoId::from_bytes(id));
        p.meta = meta;
        p.files.push(FileEntry {
            role: FileRole::Original, name: "a.arw".into(), format: None, size,
            fingerprint: Fingerprint::from_bytes(fp), hash: None, locations: vec![], extra: vec![],
        });
        let bytes = p.to_bytes();
        let back = current(PhotoSidecar::from_bytes(&bytes).unwrap());
        prop_assert_eq!(&back, &p);
        prop_assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn reading_arbitrary_bytes_as_a_sidecar_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = PhotoSidecar::from_bytes(&bytes);
        let _ = VersionSidecar::from_bytes(&bytes);
    }
}

#[test]
fn a_position_is_the_corrections_when_there_is_one_and_the_files_otherwise() {
    let mut meta = Metadata::default();
    assert_eq!(meta.position(), None);
    meta.original.gps_latitude = Some("45,30.1140N".into());
    meta.original.gps_longitude = Some("73,34.0440W".into());
    let (lat, lon) = meta.position().unwrap();
    assert!(
        (lat - 45.5019).abs() < 1e-6 && (lon + 73.5674).abs() < 1e-6,
        "{lat} {lon}"
    );
    meta.overlay = Some(Overlay {
        gps: Some(OverlayGps {
            latitude: "33,52,7.68S".into(),
            longitude: "151,12,33.48E".into(),
            altitude: None,
            extra: vec![],
        }),
        ..Overlay::default()
    });
    let (lat, lon) = meta.position().unwrap();
    assert!(
        (lat + 33.8688).abs() < 1e-4 && (lon - 151.2093).abs() < 1e-4,
        "{lat} {lon}"
    );
    // Half a position, or text that is not one, is no position.
    let mut broken = Metadata::default();
    broken.original.gps_latitude = Some("45,30.1140N".into());
    assert_eq!(broken.position(), None);
    for bad in [
        "",
        "N",
        "45,30.1140",
        "95,0N",
        "45,61N",
        "x,0N",
        "45,30,10,5N",
        "-3,0N",
    ] {
        assert_eq!(parse_gps_coordinate(bad, 'N', 'S', 90.0), None, "{bad:?}");
    }
    assert_eq!(parse_gps_coordinate("0,0S", 'N', 'S', 90.0), Some(-0.0));
}
