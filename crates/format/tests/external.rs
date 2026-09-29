// SPDX-License-Identifier: GPL-3.0-or-later
//! Foreign XMP files next to originals (spec §5.7, D-047; design note 003 §8): what other software
//! wrote, read into the plain fields Auroraw compares. Real-looking fixtures from Lightroom, darktable,
//! digiKam and ExifTool; the rating's single axis; the keyword union; and that nothing here ever needs
//! a sidecar's own properties.

use auroraw_format::sidecar::external::{ExternalError, Fields, MAX_BYTES, keyword_key, read};
use auroraw_format::sidecar::{Flag, Metadata};
use auroraw_types::KeywordId;
use proptest::prelude::*;

fn foreign(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/xmp/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// A minimal foreign file with `body` inside its one description.
fn file(attributes: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:aur="https://auroraw.org/ns/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" {attributes}>
{body}
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#
    )
    .into_bytes()
}

#[test]
fn a_lightroom_file_gives_its_rating_label_title_city_and_keyword_paths() {
    let fields = read(&foreign("lightroom.xmp")).unwrap();
    assert_eq!(fields.rating, Some(4));
    assert_eq!(fields.label.as_deref(), Some("Green"));
    assert_eq!(fields.title.as_deref(), Some("Heron at dawn"));
    assert_eq!(fields.city.as_deref(), Some("Roberval"));
    assert_eq!(
        fields.keywords,
        ["Fauna|Birds|Heron", "Places|Canada|Quebec"],
        "the flat names Heron and Quebec are levels of those paths, not keywords of their own"
    );
    // Nothing else is invented: develop settings (`crs:`) are not fields.
    assert_eq!(
        Fields {
            rating: Some(4),
            label: Some("Green".into()),
            title: Some("Heron at dawn".into()),
            city: Some("Roberval".into()),
            keywords: vec!["Fauna|Birds|Heron".into(), "Places|Canada|Quebec".into()],
            ..Fields::default()
        },
        fields
    );
}

#[test]
fn a_darktable_file_that_rejects_a_photo_reads_as_the_rejected_end_of_the_rating_axis() {
    let fields = read(&foreign("darktable.xmp")).unwrap();
    assert_eq!(fields.rating, Some(-1));
    assert_eq!(fields.keywords, ["Fauna|Birds|Heron"]);
    assert_eq!(fields.title, None, "and its history is not looked at");
}

#[test]
fn a_digikam_file_reads_its_tags_list_with_slash_as_the_hierarchy() {
    let fields = read(&foreign("digikam.xmp")).unwrap();
    assert_eq!(fields.rating, Some(3));
    assert_eq!(
        fields.caption.as_deref(),
        Some("A grey heron & its reflection")
    );
    assert_eq!(fields.creator, ["Marie Tremblay"]);
    assert_eq!(fields.rights.as_deref(), Some("(c) 2026 Marie Tremblay"));
    assert_eq!(fields.country.as_deref(), Some("Canada"));
    assert_eq!(
        fields.keywords,
        ["Fauna|Birds|Heron", "Places|Canada|Quebec", "Sunrise"],
        "Heron and Quebec are levels of the tags list; Sunrise is a keyword of its own"
    );
}

#[test]
fn a_file_with_only_flat_subject_names_gives_them_as_top_level_keywords_once_each() {
    let fields = read(&foreign("flat-subject.xmp")).unwrap();
    assert_eq!(fields.rating, Some(2));
    assert_eq!(fields.label.as_deref(), Some("Red"), "canonical spelling");
    assert_eq!(fields.title.as_deref(), Some("Heron at dawn"), "trimmed");
    assert_eq!(fields.headline.as_deref(), Some("Dawn"));
    assert_eq!(
        fields.keywords,
        ["Heron", "Quebec"],
        "Heron and heron are one keyword (the vocabulary refuses two that differ by case)"
    );
}

#[test]
fn the_rating_is_one_axis_and_only_minus_one_to_five_mean_something() {
    let rating = |attributes: &str| read(&file(attributes, "")).unwrap().rating;
    assert_eq!(rating(r#"xmp:Rating="-1""#), Some(-1));
    assert_eq!(rating(r#"xmp:Rating="5""#), Some(5));
    assert_eq!(rating(r#"xmp:Rating="0""#), None, "0 is the same as unset");
    assert_eq!(rating(r#"xmp:Rating="7""#), None, "above 5 is ignored");
    assert_eq!(rating(r#"xmp:Rating="-3""#), None);
    assert_eq!(rating(r#"xmp:Rating="three""#), None);
    assert_eq!(rating(""), None);
    assert_eq!(
        rating(r#"aur:Flag="rejected" xmp:Rating="3""#),
        Some(-1),
        "a file Auroraw exported with a rejected flag is rejected, whatever its stars"
    );
    assert_eq!(rating(r#"aur:Flag="picked" xmp:Rating="3""#), Some(3));
}

#[test]
fn develop_settings_and_unknown_namespaces_are_not_fields() {
    let bytes = file(
        r#"crs:Exposure2012="+0.35" crs:HasSettings="True""#,
        "<crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li></rdf:Seq></crs:ToneCurvePV2012>",
    );
    assert_eq!(read(&bytes).unwrap(), Fields::default());
}

#[test]
fn a_file_that_is_not_xmp_or_is_cut_short_or_too_large_is_an_error() {
    assert!(matches!(
        read(b"not xml at all"),
        Err(ExternalError::Xmp(_))
    ));
    assert!(matches!(read(b""), Err(ExternalError::Xmp(_))));
    let whole = foreign("lightroom.xmp");
    assert!(matches!(
        read(&whole[..whole.len() / 2]),
        Err(ExternalError::Xmp(_))
    ));
    assert!(matches!(
        read(&vec![b' '; MAX_BYTES + 1]),
        Err(ExternalError::TooLarge(_))
    ));
}

#[test]
fn a_file_has_no_need_of_a_sidecars_own_properties() {
    // The design note's `foreign_xmp_is_not_a_sidecar` still holds for a sidecar; this reader is another door.
    assert!(read(&file(r#"xmp:Rating="2""#, "")).is_ok());
}

#[test]
fn fields_round_trip_through_json_and_an_empty_one_is_an_empty_object() {
    let fields = read(&foreign("digikam.xmp")).unwrap();
    let json = serde_json::to_string(&fields).unwrap();
    assert_eq!(serde_json::from_str::<Fields>(&json).unwrap(), fields);
    assert_eq!(serde_json::to_string(&Fields::default()).unwrap(), "{}");
    assert_eq!(
        serde_json::from_str::<Fields>("{}").unwrap(),
        Fields::default()
    );
    assert_eq!(
        serde_json::from_str::<Fields>(r#"{"title":"x","from_a_future_version":1}"#)
            .unwrap()
            .title
            .as_deref(),
        Some("x"),
        "a field a later version adds is not an error"
    );
}

// ---- the same normalisation for Auroraw's own side ----

#[test]
fn stars_and_the_rejected_flag_are_projected_onto_the_one_axis() {
    let project = |rating: Option<u8>, flag: Option<Flag>| {
        let meta = Metadata {
            rating,
            flag,
            ..Metadata::default()
        };
        Fields::from_metadata(&meta, |_| None).rating
    };
    assert_eq!(project(Some(3), None), Some(3));
    assert_eq!(project(Some(0), None), None);
    assert_eq!(project(None, None), None);
    assert_eq!(
        project(Some(3), Some(Flag::Rejected)),
        Some(-1),
        "it keeps its stars, elsewhere"
    );
    assert_eq!(project(None, Some(Flag::Rejected)), Some(-1));
    assert_eq!(
        project(Some(3), Some(Flag::Picked)),
        Some(3),
        "picked is Auroraw's alone"
    );
}

#[test]
fn text_is_compared_trimmed_with_lf_line_ends_and_the_label_in_its_canonical_spelling() {
    let meta = Metadata {
        title: Some("  Heron \r\n at dawn  ".into()),
        caption: Some("   ".into()),
        label: Some("green".into()),
        creator: vec![" Marie ".into(), "".into(), "Jean".into()],
        persons: vec!["Zoé".into(), "Ann".into(), "Zoé".into()],
        ..Metadata::default()
    };
    let fields = Fields::from_metadata(&meta, |_| None);
    assert_eq!(fields.title.as_deref(), Some("Heron \n at dawn"));
    assert_eq!(fields.caption, None);
    assert_eq!(fields.label.as_deref(), Some("Green"));
    assert_eq!(fields.creator, ["Marie", "Jean"], "in order");
    assert_eq!(fields.persons, ["Ann", "Zoé"], "a set");
    let own = Metadata {
        label: Some("Teal".into()),
        ..Metadata::default()
    };
    assert_eq!(
        Fields::from_metadata(&own, |_| None).label.as_deref(),
        Some("Teal"),
        "a label that is not one of the five is another tool's own, kept as it is"
    );
}

#[test]
fn keywords_come_from_the_vocabulary_by_identifier_and_fall_back_to_the_snapshot() {
    let (a, b, c) = (
        KeywordId::from_bytes([1; 8]),
        KeywordId::from_bytes([2; 8]),
        KeywordId::from_bytes([3; 8]),
    );
    let mut meta = Metadata::default();
    meta.push_keyword(a, "Old|Name");
    meta.push_keyword(b, " Fauna | Birds ");
    meta.push_keyword(c, "fauna|birds");
    let fields = Fields::from_metadata(&meta, |id| (*id == a).then(|| "New|Name".to_string()));
    assert_eq!(
        fields.keywords,
        ["Fauna|Birds", "New|Name"],
        "a renamed keyword shows under its current path; two spellings of one path are one keyword"
    );
    // Written by other software: paths and no identifiers.
    let foreign = Metadata {
        keyword_paths: vec!["Places|Quebec".into()],
        ..Metadata::default()
    };
    assert_eq!(
        Fields::from_metadata(&foreign, |_| None).keywords,
        ["Places|Quebec"]
    );
    assert_eq!(keyword_key(" Fauna | Birds "), "fauna|birds");
}

#[test]
fn a_new_photo_takes_the_files_fields_and_keeps_what_the_file_does_not_have() {
    let mut meta = Metadata::default();
    meta.original.make = Some("SONY".into());
    meta.title = Some("From the EXIF".into());
    let fields = read(&foreign("lightroom.xmp")).unwrap();
    fields.fill(&mut meta);
    assert_eq!(meta.rating, Some(4));
    assert_eq!(meta.label.as_deref(), Some("Green"));
    assert_eq!(
        meta.title.as_deref(),
        Some("Heron at dawn"),
        "the file's value wins"
    );
    assert_eq!(meta.city.as_deref(), Some("Roberval"));
    assert_eq!(
        meta.original.make.as_deref(),
        Some("SONY"),
        "capture data untouched"
    );
    assert!(
        meta.keyword_paths.is_empty(),
        "keywords need identifiers: the caller's"
    );

    let mut kept = Metadata {
        caption: Some("mine".into()),
        ..Metadata::default()
    };
    read(&file(r#"xmp:Rating="1""#, ""))
        .unwrap()
        .fill(&mut kept);
    assert_eq!(
        kept.caption.as_deref(),
        Some("mine"),
        "a field the file lacks is not cleared"
    );

    let mut rejected = Metadata::default();
    read(&foreign("darktable.xmp")).unwrap().fill(&mut rejected);
    assert_eq!(
        (rejected.flag, rejected.rating),
        (Some(Flag::Rejected), None)
    );
}

proptest! {
    #[test]
    fn reading_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = read(&bytes);
    }

    #[test]
    fn reading_arbitrary_text_in_a_description_never_panics(rating in ".{0,12}", subject in ".{0,40}") {
        let subject = subject.replace(['<', '>', '&'], "");
        let rating = rating.replace(['<', '>', '&', '"'], "");
        let body = format!("<dc:subject><rdf:Bag><rdf:li>{subject}</rdf:li></rdf:Bag></dc:subject>");
        let _ = read(&file(&format!("xmp:Rating=\"{rating}\""), &body));
    }
}
