// SPDX-License-Identifier: GPL-3.0-or-later
//! Foreign XMP files next to originals (spec §5.7, D-047; design note 003 §8): what other software
//! wrote, read into the plain fields Auroraw compares. Real-looking fixtures from Lightroom, darktable,
//! digiKam and ExifTool; the rating's single axis; the keyword union; and that nothing here ever needs
//! a sidecar's own properties.

use auroraw_format::sidecar::external::{
    ExternalError, Field, Fields, MAX_BYTES, Merge, keyword_key, merge, read,
};
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

// ---- the three-way comparison (design note 003 §8.1) ----

fn title(t: &str) -> Fields {
    Fields {
        title: Some(t.into()),
        ..Fields::default()
    }
}

fn keywords(paths: &[&str]) -> Fields {
    Fields {
        keywords: paths.iter().map(|p| p.to_string()).collect(),
        ..Fields::default()
    }
}

fn only(m: &Merge) -> Vec<Field> {
    let mut fields: Vec<Field> = m.taken.iter().map(|c| c.field).collect();
    fields.extend(m.conflicts.iter().map(|c| c.field));
    fields
}

#[test]
fn a_field_unchanged_in_the_file_is_left_to_the_photo_whatever_the_photo_says() {
    let m = merge(Some(&title("a")), &title("a"), &title("mine"));
    assert!(m.is_empty() && m.converged.is_empty());
}

#[test]
fn a_field_changed_only_in_the_file_is_taken() {
    let m = merge(Some(&title("a")), &title("b"), &title("a"));
    assert_eq!(m.taken.len(), 1);
    assert_eq!(
        (
            m.taken[0].field,
            m.taken[0].mine.as_str(),
            m.taken[0].file.as_str()
        ),
        (Field::Title, "a", "b")
    );
    assert!(m.conflicts.is_empty());
}

#[test]
fn a_field_the_file_changed_to_what_the_photo_already_says_is_converged_and_needs_no_answer() {
    let m = merge(Some(&title("a")), &title("b"), &title("b"));
    assert!(m.is_empty(), "nothing to take or ask");
    assert_eq!(m.converged, [Field::Title]);
}

#[test]
fn a_field_changed_on_both_sides_to_different_values_is_a_conflict() {
    let m = merge(Some(&title("a")), &title("b"), &title("c"));
    assert!(m.taken.is_empty());
    assert_eq!(m.conflicts.len(), 1);
    let c = &m.conflicts[0];
    assert_eq!(
        (c.field, c.base.as_deref(), c.mine.as_str(), c.file.as_str()),
        (Field::Title, Some("a"), "c", "b")
    );
}

#[test]
fn without_a_base_every_difference_is_a_conflict_and_a_match_is_nothing() {
    let m = merge(None, &title("b"), &title("c"));
    assert_eq!(m.conflicts.len(), 1);
    assert_eq!(m.conflicts[0].base, None);
    assert!(merge(None, &title("b"), &title("b")).is_empty());
    // A field the file lacks and the photo has is a difference too: nothing is assumed.
    assert_eq!(
        merge(None, &Fields::default(), &title("c")).conflicts.len(),
        1
    );
}

#[test]
fn a_file_that_gains_or_loses_a_field_is_a_change_like_any_other() {
    let gained = merge(Some(&Fields::default()), &title("new"), &Fields::default());
    assert_eq!(only(&gained), [Field::Title]);
    let lost = merge(Some(&title("old")), &Fields::default(), &title("old"));
    assert_eq!(
        lost.taken[0].file, "",
        "cleared in the file, cleared in the photo"
    );
}

#[test]
fn the_rating_axis_changes_as_one_field() {
    let r = |n: Option<i8>| Fields {
        rating: n,
        ..Fields::default()
    };
    // Another tool rejects a 3-star photo: one field.
    let m = merge(Some(&r(Some(3))), &r(Some(-1)), &r(Some(3)));
    assert_eq!(only(&m), [Field::Rating]);
    assert_eq!(m.taken[0].file, "-1");
    // And un-rejects it to 4.
    let m = merge(Some(&r(Some(-1))), &r(Some(4)), &r(Some(-1)));
    assert_eq!(m.taken[0].file, "4");
    // 0 is unset.
    assert!(merge(Some(&r(None)), &r(None), &r(Some(2))).is_empty());
}

#[test]
fn keywords_are_sets_against_the_base_and_never_conflict() {
    let base = keywords(&["Fauna|Birds|Heron", "Places|Quebec"]);
    let file = keywords(&["Fauna|Birds|Heron", "Sunrise", "places|quebec"]);
    let mine = keywords(&["Fauna|Birds|Heron", "Places|Quebec", "Mine"]);
    let m = merge(Some(&base), &file, &mine);
    assert_eq!(
        m.keywords.add,
        ["Sunrise"],
        "the file added it, the photo lacks it"
    );
    assert!(
        m.keywords.remove.is_empty(),
        "spellings of one path are one keyword"
    );
    assert!(m.conflicts.is_empty() && m.taken.is_empty());

    // The file drops one that the photo still has: offered; one the photo already dropped: nothing.
    let dropped = keywords(&["Fauna|Birds|Heron"]);
    let m = merge(Some(&base), &dropped, &mine);
    assert_eq!(m.keywords.remove, ["Places|Quebec"]);
    let m = merge(Some(&base), &dropped, &keywords(&["Fauna|Birds|Heron"]));
    assert!(m.keywords.remove.is_empty() && m.is_empty());

    // A keyword the photo added and the file never had is not in the base: left alone.
    assert!(merge(Some(&base), &base, &mine).is_empty());
    // A keyword the photo renamed no longer matches the base's spelling: left alone, not removed.
    let renamed = keywords(&["Fauna|Birds|Grey heron", "Places|Quebec"]);
    let m = merge(Some(&base), &keywords(&["Places|Quebec"]), &renamed);
    assert!(
        m.keywords.remove.is_empty(),
        "the photo no longer has that path"
    );
}

#[test]
fn with_no_base_the_files_keywords_are_offered_and_nothing_is_removed() {
    let m = merge(None, &keywords(&["A", "B"]), &keywords(&["B", "C"]));
    assert_eq!(m.keywords.add, ["A"]);
    assert!(m.keywords.remove.is_empty(), "nothing is lost");
}

#[test]
fn applying_a_field_makes_the_photo_agree_with_the_file() {
    let file = read(&foreign("lightroom.xmp")).unwrap();
    let mut meta = Metadata::default();
    for field in Field::ALL {
        file.apply_field(field, &mut meta);
    }
    let mine = Fields::from_metadata(&meta, |_| None);
    let m = merge(Some(&Fields::default()), &file, &mine);
    // Everything but the keywords (which need identifiers) now agrees.
    assert!(m.taken.is_empty() && m.conflicts.is_empty(), "{m:?}");
    assert_eq!(m.keywords.add, file.keywords);

    // The rating axis, applied.
    let axis = |rating: Option<i8>, from: Metadata| {
        let mut meta = from;
        Fields {
            rating,
            ..Fields::default()
        }
        .apply_field(Field::Rating, &mut meta);
        (meta.rating, meta.flag)
    };
    let stars = Metadata {
        rating: Some(3),
        ..Metadata::default()
    };
    assert_eq!(
        axis(Some(-1), stars.clone()),
        (Some(3), Some(Flag::Rejected)),
        "it keeps its stars"
    );
    let rejected = Metadata {
        rating: Some(3),
        flag: Some(Flag::Rejected),
        ..Metadata::default()
    };
    assert_eq!(axis(Some(4), rejected.clone()), (Some(4), None));
    assert_eq!(axis(None, rejected), (None, None));
    let picked = Metadata {
        flag: Some(Flag::Picked),
        ..stars
    };
    assert_eq!(
        axis(Some(5), picked),
        (Some(5), Some(Flag::Picked)),
        "picked is never touched by a rating"
    );
}

#[test]
fn field_keys_round_trip() {
    for field in Field::ALL {
        assert_eq!(Field::parse(field.key()), Some(field));
    }
    assert_eq!(Field::parse("nonsense"), None);
    assert_eq!(Field::ALL.len(), 20);
}

fn arbitrary_fields() -> impl Strategy<Value = Fields> {
    (
        proptest::option::of(-1i8..=5),
        proptest::option::of("[a-c]{0,3}"),
        proptest::option::of("[a-c]{1,3}"),
        proptest::collection::vec("[a-c]{1,2}", 0..3),
        proptest::collection::vec("[A-Ca-c]{1,2}(\\|[A-Ca-c]{1,2}){0,2}", 0..4),
    )
        .prop_map(|(rating, label, title, creator, keywords)| {
            let meta = Metadata {
                rating: rating.filter(|r| *r >= 1).map(|r| r as u8),
                flag: (rating == Some(-1)).then_some(Flag::Rejected),
                label,
                title,
                creator,
                keyword_paths: keywords,
                ..Metadata::default()
            };
            Fields::from_metadata(&meta, |_| None)
        })
}

proptest! {
    #[test]
    fn a_file_equal_to_its_base_never_asks_anything(base in arbitrary_fields(), mine in arbitrary_fields()) {
        prop_assert!(merge(Some(&base), &base, &mine).is_empty());
    }

    #[test]
    fn a_photo_equal_to_its_base_takes_exactly_what_the_file_changed(base in arbitrary_fields(), file in arbitrary_fields()) {
        let m = merge(Some(&base), &file, &base);
        prop_assert!(m.conflicts.is_empty());
        let changed: Vec<Field> = Field::ALL
            .into_iter()
            .filter(|f| *f != Field::Keywords && file.get(*f) != base.get(*f))
            .collect();
        let mut taken: Vec<Field> = m.taken.iter().map(|c| c.field).collect();
        taken.sort_by_key(|f| f.key());
        let mut expected = changed;
        expected.sort_by_key(|f| f.key());
        prop_assert_eq!(taken, expected);
    }

    #[test]
    fn without_a_base_two_equal_sides_agree_and_the_rest_conflict(file in arbitrary_fields(), mine in arbitrary_fields()) {
        let m = merge(None, &file, &mine);
        prop_assert!(m.taken.is_empty());
        for c in &m.conflicts {
            prop_assert!(file.get(c.field) != mine.get(c.field));
        }
        prop_assert_eq!(merge(None, &file, &file).is_empty(), true);
    }
}
