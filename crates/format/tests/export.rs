// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP export to the source folders (spec §5.7, D-024; design note 003 §8, §8.1): the derived file
//! built from a photo's metadata, and the merge into a file another application wrote, which keeps
//! everything it does not own.

use auroraw_format::sidecar::export::{
    EXPORT_VERSION, ExportView, OWNED, build, effective_original, merge_into, owned_properties,
};
use auroraw_format::sidecar::external::{Fields, OwnExtras, own_extras, read};
use auroraw_format::sidecar::{Flag, Metadata, Original, Overlay, OverlayGps};
use auroraw_format::xmp::{Property, Xmp, ns};
use proptest::prelude::*;

fn foreign(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/xmp/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn photo() -> Metadata {
    Metadata {
        rating: Some(4),
        flag: Some(Flag::Picked),
        label: Some("Green".into()),
        title: Some("Heron at dawn".into()),
        caption: Some("A grey heron & its reflection".into()),
        creator: vec!["Marie Tremblay".into()],
        rights: Some("(c) 2026 Marie Tremblay".into()),
        city: Some("Québec".into()),
        country: Some("Canada".into()),
        persons: vec!["Jean Roy".into()],
        original: Original {
            capture_time: Some("2026-09-01T06:12:00".into()),
            make: Some("Nikon".into()),
            model: Some("Z6".into()),
            serial: Some("1234".into()),
            iso: vec!["200".into()],
            gps_latitude: Some("46,48.5N".into()),
            gps_longitude: Some("71,13.2W".into()),
            ..Original::default()
        },
        ..Metadata::default()
    }
}

fn view<'a>(meta: &'a Metadata, keywords: &'a [String], minus_one: bool) -> ExportView<'a> {
    ExportView {
        meta,
        keywords,
        rejected_as_minus_one: minus_one,
    }
}

/// What a scan of the exported file must compute for this photo: its fields, with only the exported keywords.
fn expected(meta: &Metadata, keywords: &[String]) -> Fields {
    let mut m = meta.clone();
    m.keyword_ids.clear();
    m.keyword_paths = keywords.to_vec();
    Fields::from_metadata(&m, |_| None)
}

fn owned(p: &Property) -> bool {
    OWNED.iter().any(|(n, name)| p.is(n, name))
}

/// The document made of the properties the export does not own.
fn rest(xmp: &Xmp) -> Xmp {
    Xmp {
        properties: xmp
            .properties
            .iter()
            .filter(|p| !owned(p))
            .cloned()
            .collect(),
        prefixes: xmp.prefixes.clone(),
    }
}

#[test]
fn a_new_file_holds_the_owned_properties_the_capture_data_and_the_marker() {
    let meta = photo();
    let keywords = paths(&["Fauna|Birds|Heron", "Places|Canada|Quebec"]);
    let xmp = build(&view(&meta, &keywords, true));
    let text = |n: &str, name: &str| {
        xmp.get(n, name)
            .and_then(Property::as_text)
            .map(str::to_string)
    };
    assert_eq!(text(ns::XMP, "Rating").as_deref(), Some("4"));
    assert_eq!(text(ns::AUR, "Flag").as_deref(), Some("picked"));
    assert_eq!(text(ns::XMP, "Label").as_deref(), Some("Green"));
    assert_eq!(text(ns::AUR, "Export").as_deref(), Some(EXPORT_VERSION));
    assert_eq!(text(ns::TIFF, "Model").as_deref(), Some("Z6"));
    assert_eq!(
        xmp.get(ns::LR, "hierarchicalSubject")
            .and_then(Property::as_texts),
        Some(vec!["Fauna|Birds|Heron", "Places|Canada|Quebec"])
    );
    assert_eq!(
        xmp.get(ns::DC, "subject").and_then(Property::as_texts),
        Some(vec!["Heron", "Quebec"])
    );
    // Nothing that is Auroraw's own beyond the flag, the stars and the marker.
    for gone in [
        "KeywordIds",
        "Files",
        "Overlay",
        "Custom",
        "PhotoId",
        "Schema",
        "Locations",
    ] {
        assert!(xmp.get(ns::AUR, gone).is_none(), "aur:{gone}");
    }
    // It is a file another tool can read, and it reads back.
    let bytes = xmp.to_bytes();
    assert_eq!(
        Xmp::from_bytes(&bytes).unwrap(),
        Xmp {
            prefixes: Xmp::from_bytes(&bytes).unwrap().prefixes,
            ..xmp
        }
    );
}

#[test]
fn an_export_read_back_gives_the_same_fields() {
    let keywords = paths(&["Fauna|Birds|Heron", "Places|Canada|Quebec", "Sunrise"]);
    for (flag, rating) in [
        (None, None),
        (None, Some(3)),
        (Some(Flag::Picked), Some(5)),
        (Some(Flag::Rejected), Some(2)),
        (Some(Flag::Rejected), None),
    ] {
        for minus_one in [true, false] {
            let meta = Metadata {
                flag,
                rating,
                ..photo()
            };
            let bytes = build(&view(&meta, &keywords, minus_one)).to_bytes();
            assert_eq!(
                read(&bytes).unwrap(),
                expected(&meta, &keywords),
                "{flag:?} {rating:?} minus_one={minus_one}"
            );
        }
    }
}

#[test]
fn a_rejected_photo_carries_minus_one_for_other_software_and_its_stars_for_auroraw() {
    let meta = Metadata {
        flag: Some(Flag::Rejected),
        rating: Some(3),
        ..photo()
    };
    let on = build(&view(&meta, &[], true));
    assert_eq!(
        on.get(ns::XMP, "Rating").and_then(Property::as_text),
        Some("-1")
    );
    assert_eq!(
        on.get(ns::AUR, "Flag").and_then(Property::as_text),
        Some("rejected")
    );
    assert_eq!(
        on.get(ns::AUR, "Stars").and_then(Property::as_text),
        Some("3")
    );

    let off = build(&view(&meta, &[], false));
    assert_eq!(
        off.get(ns::XMP, "Rating").and_then(Property::as_text),
        Some("3")
    );
    assert_eq!(
        off.get(ns::AUR, "Flag").and_then(Property::as_text),
        Some("rejected")
    );
    assert!(off.get(ns::AUR, "Stars").is_none());

    // Auroraw gets the flag and the stars back exactly, either way.
    for xmp in [on, off] {
        assert_eq!(
            own_extras(&xmp.to_bytes()),
            Some(OwnExtras {
                flag: Some(Flag::Rejected),
                stars: Some(3)
            })
        );
    }
}

#[test]
fn only_a_file_with_the_marker_has_extras() {
    assert_eq!(own_extras(&foreign("lightroom.xmp")), None);
    assert_eq!(own_extras(&foreign("darktable.xmp")), None);
    assert_eq!(own_extras(b"not xmp at all"), None);
    let picked = build(&view(&photo(), &[], true)).to_bytes();
    assert_eq!(
        own_extras(&picked),
        Some(OwnExtras {
            flag: Some(Flag::Picked),
            stars: Some(4)
        })
    );
}

#[test]
fn keywords_are_written_by_name_and_by_path_without_duplicates() {
    let keywords = paths(&[
        "Places|Paris",
        "People|Paris",
        "places|paris",
        " Fauna | Birds ",
    ]);
    let xmp = build(&view(&photo(), &keywords, true));
    assert_eq!(
        xmp.get(ns::DC, "subject").and_then(Property::as_texts),
        Some(vec!["Paris", "Birds"])
    );
    assert_eq!(
        xmp.get(ns::LR, "hierarchicalSubject")
            .and_then(Property::as_texts),
        Some(vec!["Places|Paris", "People|Paris", "Fauna|Birds"])
    );
}

#[test]
fn a_field_with_nothing_in_it_has_no_property() {
    let meta = Metadata {
        title: Some("  ".into()),
        creator: vec![String::new()],
        rating: Some(0),
        ..Metadata::default()
    };
    let props = owned_properties(&view(&meta, &[], true));
    let names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Export"]);
}

#[test]
fn the_overlay_is_applied_to_the_capture_data_and_a_new_position_drops_the_old_altitude() {
    let mut original = photo().original;
    original.gps_altitude = Some("120/1".into());
    let overlay = Overlay {
        capture_time: Some("2026-09-01T07:00:00".into()),
        gps: Some(OverlayGps {
            latitude: "45,30.0N".into(),
            longitude: "73,34.0W".into(),
            altitude: None,
            extra: Vec::new(),
        }),
        lens: Some("Z 24-70".into()),
        ..Overlay::default()
    };
    let effective = effective_original(&original, Some(&overlay));
    assert_eq!(
        effective.capture_time.as_deref(),
        Some("2026-09-01T07:00:00")
    );
    assert_eq!(effective.gps_latitude.as_deref(), Some("45,30.0N"));
    assert_eq!(effective.gps_altitude, None);
    assert_eq!(effective.lens.as_deref(), Some("Z 24-70"));
    assert_eq!(
        effective.model, original.model,
        "not corrected, not touched"
    );

    let meta = Metadata {
        original,
        overlay: Some(overlay),
        ..photo()
    };
    let xmp = build(&view(&meta, &[], true));
    assert_eq!(
        xmp.get(ns::EXIF, "GPSLatitude").and_then(Property::as_text),
        Some("45,30.0N")
    );
    assert!(xmp.get(ns::EXIF, "GPSAltitude").is_none());
}

// ---- merging into a file that exists -----------------------------------------------------------

#[test]
fn a_merge_keeps_every_property_it_does_not_own_and_rewrites_the_ones_it_does() {
    for name in ["lightroom.xmp", "darktable.xmp", "digikam.xmp"] {
        let bytes = foreign(name);
        let before = Xmp::from_bytes(&bytes).unwrap();
        let mut merged = before.clone();
        let meta = photo();
        let keywords = paths(&["Fauna|Birds|Heron", "Sunrise"]);
        merge_into(&mut merged, &view(&meta, &keywords, true));

        // Everything else is written back as it was, as the same bytes (`crs:`, `darktable:`, ...).
        let kept = |x: &Xmp| {
            Xmp {
                properties: x
                    .properties
                    .iter()
                    .filter(|p| !owned(p) && !p.is(ns::DIGIKAM, "TagsList"))
                    .cloned()
                    .collect(),
                prefixes: x.prefixes.clone(),
            }
            .to_bytes()
        };
        assert_eq!(kept(&merged), kept(&before), "{name}");

        // What it owns is Auroraw's now.
        assert_eq!(
            read(&merged.to_bytes()).unwrap(),
            expected(&meta, &keywords),
            "{name}"
        );
        assert_eq!(
            merged.get(ns::AUR, "Export").and_then(Property::as_text),
            Some(EXPORT_VERSION),
            "{name}"
        );
    }
}

#[test]
fn a_merge_keeps_the_develop_history_of_darktable_byte_for_byte() {
    let before = Xmp::from_bytes(&foreign("darktable.xmp")).unwrap();
    let history = before
        .properties
        .iter()
        .filter(|p| p.ns == "http://darktable.sf.net/")
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        history.len() >= 5,
        "the fixture has a history and parameters"
    );
    let mut merged = before.clone();
    merge_into(&mut merged, &view(&photo(), &[], false));
    let after = merged
        .properties
        .iter()
        .filter(|p| p.ns == "http://darktable.sf.net/")
        .cloned()
        .collect::<Vec<_>>();
    let bytes = |props: Vec<Property>| {
        Xmp {
            properties: props,
            prefixes: before.prefixes.clone(),
        }
        .to_bytes()
    };
    assert_eq!(bytes(after), bytes(history));
}

#[test]
fn a_replaced_property_keeps_its_place_and_a_cleared_one_goes() {
    let before = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    let position = |x: &Xmp, name: &str| x.properties.iter().position(|p| p.is(ns::DC, name));
    let title_at = position(&before, "title").expect("the fixture has a title");
    let meta = Metadata {
        title: Some("Another title".into()),
        caption: None,
        ..photo()
    };
    let mut merged = before.clone();
    merge_into(&mut merged, &view(&meta, &[], true));
    assert_eq!(position(&merged, "title"), Some(title_at));
    assert_eq!(
        merged.get(ns::DC, "title").and_then(Property::as_lang_text),
        Some("Another title")
    );
    assert!(
        merged.get(ns::DC, "description").is_none(),
        "cleared, so removed"
    );
}

#[test]
fn a_merge_keeps_digikams_tag_list_in_step_with_the_keywords() {
    let before = Xmp::from_bytes(&foreign("digikam.xmp")).unwrap();
    let mut merged = before.clone();
    merge_into(
        &mut merged,
        &view(&photo(), &paths(&["Fauna|Birds|Heron"]), true),
    );
    assert_eq!(
        merged
            .get(ns::DIGIKAM, "TagsList")
            .and_then(Property::as_texts),
        Some(vec!["Fauna/Birds/Heron"])
    );
    // No keyword left in Auroraw: none left for digiKam either, or they would come back at the next read.
    let mut cleared = before.clone();
    merge_into(&mut cleared, &view(&photo(), &[], true));
    assert!(cleared.get(ns::DIGIKAM, "TagsList").is_none());
    assert!(read(&cleared.to_bytes()).unwrap().keywords.is_empty());
    // A file without the list does not get one.
    let mut lightroom = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    merge_into(&mut lightroom, &view(&photo(), &paths(&["A|B"]), true));
    assert!(lightroom.get(ns::DIGIKAM, "TagsList").is_none());
}

#[test]
fn a_merge_touches_the_capture_data_only_where_the_overlay_corrected_it() {
    let mut file = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    file.take(ns::TIFF, "Model");
    file.take(ns::EXIF, "GPSLatitude");
    file.properties
        .push(Property::text(ns::TIFF, "Model", "Camera's own model"));
    file.properties
        .push(Property::text(ns::EXIF, "GPSLatitude", "1,0.0N"));
    let untouched = {
        let mut m = file.clone();
        merge_into(&mut m, &view(&photo(), &[], true));
        m
    };
    assert_eq!(
        untouched.get(ns::TIFF, "Model").and_then(Property::as_text),
        Some("Camera's own model"),
        "no overlay, the file's capture data stays"
    );
    let meta = Metadata {
        overlay: Some(Overlay {
            gps: Some(OverlayGps {
                latitude: "45,30.0N".into(),
                longitude: "73,34.0W".into(),
                altitude: None,
                extra: Vec::new(),
            }),
            ..Overlay::default()
        }),
        ..photo()
    };
    let mut corrected = file.clone();
    merge_into(&mut corrected, &view(&meta, &[], true));
    assert_eq!(
        corrected
            .get(ns::EXIF, "GPSLatitude")
            .and_then(Property::as_text),
        Some("45,30.0N")
    );
    assert_eq!(
        corrected.get(ns::TIFF, "Model").and_then(Property::as_text),
        Some("Camera's own model")
    );
}

#[test]
fn a_corrected_position_does_not_inherit_the_files_altitude_reference() {
    let mut file = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    file.properties
        .push(Property::text(ns::EXIF, "GPSAltitude", "120/1"));
    file.properties
        .push(Property::text(ns::EXIF, "GPSAltitudeRef", "1"));
    let meta = Metadata {
        overlay: Some(Overlay {
            gps: Some(OverlayGps {
                latitude: "45,30.0N".into(),
                longitude: "73,34.0W".into(),
                altitude: None,
                extra: Vec::new(),
            }),
            ..Overlay::default()
        }),
        ..photo()
    };
    merge_into(&mut file, &view(&meta, &[], true));
    assert!(file.get(ns::EXIF, "GPSAltitude").is_none());
    assert!(
        file.get(ns::EXIF, "GPSAltitudeRef").is_none(),
        "the reference went with the altitude it qualified"
    );
}

#[test]
fn the_metadata_date_is_stamped_once_and_replaced_not_repeated() {
    use auroraw_format::sidecar::export::stamp_metadata_date;
    let mut file = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    stamp_metadata_date(&mut file, "2026-09-30T18:00:00Z");
    stamp_metadata_date(&mut file, "2026-10-01T09:30:00Z");
    let dates: Vec<_> = file
        .properties
        .iter()
        .filter(|p| p.is(ns::XMP, "MetadataDate"))
        .collect();
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].as_text(), Some("2026-10-01T09:30:00Z"));
    // A merge leaves the date alone: it is not one of the properties an export owns.
    merge_into(&mut file, &view(&photo(), &[], true));
    assert_eq!(
        file.get(ns::XMP, "MetadataDate")
            .and_then(Property::as_text),
        Some("2026-10-01T09:30:00Z")
    );
}

#[test]
fn merging_twice_changes_nothing_more() {
    for name in ["lightroom.xmp", "darktable.xmp", "digikam.xmp"] {
        let mut once = Xmp::from_bytes(&foreign(name)).unwrap();
        let keywords = paths(&["Fauna|Birds|Heron"]);
        merge_into(&mut once, &view(&photo(), &keywords, true));
        let mut twice = Xmp::from_bytes(&once.to_bytes()).unwrap();
        merge_into(&mut twice, &view(&photo(), &keywords, true));
        assert_eq!(once.to_bytes(), twice.to_bytes(), "{name}");
    }
}

#[test]
fn a_file_that_is_not_xmp_is_an_error_the_caller_can_see_before_merging() {
    assert!(Xmp::from_bytes(b"<not><closed>").is_err());
    assert!(Xmp::from_bytes(b"").is_err());
}

fn text_strategy() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[A-Za-z0-9 ,.'é&<>-]{0,24}")
}

proptest! {
    #[test]
    fn a_merge_never_changes_what_it_does_not_own_and_reads_back_as_the_photo(
        title in text_strategy(),
        caption in text_strategy(),
        city in text_strategy(),
        rating in prop::option::of(0u8..=5),
        flag in prop::option::of(prop::sample::select(vec![Flag::Picked, Flag::Rejected])),
        minus_one in any::<bool>(),
        words in prop::collection::vec("[A-Za-z]{1,8}(\\|[A-Za-z]{1,8}){0,2}", 0..5),
        fixture in prop::sample::select(vec!["lightroom.xmp", "darktable.xmp", "digikam.xmp"]),
    ) {
        let meta = Metadata { title, caption, city, rating, flag, ..Metadata::default() };
        let before = Xmp::from_bytes(&foreign(fixture)).unwrap();
        let mut merged = before.clone();
        merge_into(&mut merged, &view(&meta, &words, minus_one));
        let untouched = |x: &Xmp| Xmp {
            properties: x.properties.iter().filter(|p| !owned(p) && !p.is(ns::DIGIKAM, "TagsList")).cloned().collect(),
            prefixes: x.prefixes.clone(),
        }.to_bytes();
        prop_assert_eq!(untouched(&merged), untouched(&before));
        prop_assert_eq!(rest(&merged).properties.len() >= rest(&before).properties.len().saturating_sub(1), true);
        let reread = read(&merged.to_bytes()).unwrap();
        prop_assert_eq!(reread, expected(&meta, &words));
        // and once more changes nothing
        let mut again = Xmp::from_bytes(&merged.to_bytes()).unwrap();
        merge_into(&mut again, &view(&meta, &words, minus_one));
        prop_assert_eq!(again.to_bytes(), merged.to_bytes());
    }
}
