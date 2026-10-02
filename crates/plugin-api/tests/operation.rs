// SPDX-License-Identifier: MIT OR Apache-2.0
//! The typed parameters of an operation (D-142): what each kind allows, what a spec that does not hold together is
//! refused for, and that the JSON a plugin writes is the JSON this crate reads.

use auroraw_plugin_api::{MAX_KEY_LEN, OperationId, ParamError, ParamKind, ParamSpec, ParamValue};
use proptest::prelude::*;

fn spec(key: &str, kind: ParamKind) -> ParamSpec {
    ParamSpec {
        key: key.into(),
        label: format!("{key}-label"),
        kind,
    }
}

fn exposure() -> ParamSpec {
    spec(
        "ev",
        ParamKind::Float {
            min: -5.0,
            max: 5.0,
            default: 0.0,
        },
    )
}

/// One of each kind, valid.
fn every_kind() -> Vec<ParamSpec> {
    vec![
        spec("on", ParamKind::Bool { default: true }),
        spec(
            "radius",
            ParamKind::Int {
                min: 1,
                max: 50,
                default: 5,
            },
        ),
        exposure(),
        spec(
            "method",
            ParamKind::Enum {
                choices: vec!["linear".into(), "cubic".into(), "lanczos".into()],
                default: 1,
            },
        ),
        spec(
            "tint",
            ParamKind::Colour {
                default: [1.0, 0.9, 0.8],
            },
        ),
        spec(
            "centre",
            ParamKind::Point {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
                default: [0.5, 0.5],
            },
        ),
        spec(
            "stops",
            ParamKind::List {
                item: Box::new(ParamKind::Float {
                    min: 0.0,
                    max: 1.0,
                    default: 0.5,
                }),
                min_len: 1,
                max_len: 4,
                default: vec![ParamValue::Float(0.25), ParamValue::Float(0.75)],
            },
        ),
        spec(
            "tone",
            ParamKind::Curve {
                min_points: 2,
                max_points: 16,
                default: vec![[0.0, 0.0], [1.0, 1.0]],
            },
        ),
    ]
}

#[test]
fn every_kind_holds_together_and_allows_its_own_default() {
    for s in every_kind() {
        s.validate().unwrap_or_else(|e| panic!("{}: {e}", s.key));
        s.check(&s.default_value())
            .unwrap_or_else(|e| panic!("{}: {e}", s.key));
    }
}

#[test]
fn a_value_of_another_kind_is_refused() {
    let wrong = ParamValue::Bool(true);
    for s in every_kind().into_iter().filter(|s| s.key != "on") {
        assert!(
            matches!(s.check(&wrong), Err(ParamError::WrongKind { .. })),
            "{}",
            s.key
        );
    }
    assert!(matches!(
        every_kind()[0].check(&ParamValue::Int(1)),
        Err(ParamError::WrongKind { .. })
    ));
}

#[test]
fn the_limits_are_included_and_what_is_past_them_is_refused() {
    let radius = &every_kind()[1];
    assert!(radius.check(&ParamValue::Int(1)).is_ok());
    assert!(radius.check(&ParamValue::Int(50)).is_ok());
    assert!(matches!(
        radius.check(&ParamValue::Int(0)),
        Err(ParamError::OutOfRange { .. })
    ));
    assert!(matches!(
        radius.check(&ParamValue::Int(51)),
        Err(ParamError::OutOfRange { .. })
    ));
    let ev = exposure();
    assert!(ev.check(&ParamValue::Float(-5.0)).is_ok());
    assert!(ev.check(&ParamValue::Float(5.0)).is_ok());
    assert!(matches!(
        ev.check(&ParamValue::Float(5.0001)),
        Err(ParamError::OutOfRange { .. })
    ));
    let point = &every_kind()[5];
    assert!(point.check(&ParamValue::Point([0.0, 1.0])).is_ok());
    assert!(matches!(
        point.check(&ParamValue::Point([0.5, 1.5])),
        Err(ParamError::OutOfRange {
            what: "the y of the point"
        })
    ));
}

#[test]
fn a_point_is_held_to_the_box_of_each_axis() {
    // Different limits on the two axes, so that an axis read from the other one's limits is caught.
    let point = spec(
        "centre",
        ParamKind::Point {
            min: [0.0, -5.0],
            max: [1.0, 10.0],
            default: [0.5, 0.0],
        },
    );
    assert_eq!(point.validate(), Ok(()));
    assert!(point.check(&ParamValue::Point([1.0, 10.0])).is_ok());
    assert!(point.check(&ParamValue::Point([0.0, -5.0])).is_ok());
    assert!(
        point.check(&ParamValue::Point([0.5, 7.0])).is_ok(),
        "y past the x limits"
    );
    assert!(point.check(&ParamValue::Point([0.5, -4.0])).is_ok());
    assert_eq!(
        point.check(&ParamValue::Point([5.0, 0.0])),
        Err(ParamError::OutOfRange {
            what: "the x of the point"
        })
    );
    assert_eq!(
        point.check(&ParamValue::Point([0.5, 11.0])),
        Err(ParamError::OutOfRange {
            what: "the y of the point"
        })
    );
    assert_eq!(
        point.check(&ParamValue::Point([0.5, -6.0])),
        Err(ParamError::OutOfRange {
            what: "the y of the point"
        })
    );
}

#[test]
fn a_number_that_is_not_finite_is_refused_wherever_it_is() {
    let kinds = every_kind();
    assert_eq!(
        kinds[2].check(&ParamValue::Float(f64::NAN)),
        Err(ParamError::NotFinite)
    );
    assert_eq!(
        kinds[2].check(&ParamValue::Float(f64::INFINITY)),
        Err(ParamError::NotFinite)
    );
    assert_eq!(
        kinds[4].check(&ParamValue::Colour([1.0, f64::NAN, 1.0])),
        Err(ParamError::NotFinite)
    );
    assert_eq!(
        kinds[5].check(&ParamValue::Point([f64::NEG_INFINITY, 0.0])),
        Err(ParamError::NotFinite)
    );
    assert_eq!(
        kinds[7].check(&ParamValue::Curve(vec![[0.0, 0.0], [f64::NAN, 1.0]])),
        Err(ParamError::NotFinite)
    );
}

#[test]
fn a_choice_past_the_last_is_refused() {
    let method = &every_kind()[3];
    assert!(method.check(&ParamValue::Enum(2)).is_ok());
    assert_eq!(
        method.check(&ParamValue::Enum(3)),
        Err(ParamError::NoSuchChoice { index: 3, count: 3 })
    );
}

#[test]
fn a_list_has_its_length_and_each_item_its_kind() {
    let stops = &every_kind()[6];
    assert!(
        stops
            .check(&ParamValue::List(vec![ParamValue::Float(0.1)]))
            .is_ok()
    );
    assert_eq!(
        stops.check(&ParamValue::List(Vec::new())),
        Err(ParamError::Length {
            count: 0,
            min: 1,
            max: 4
        })
    );
    assert!(matches!(
        stops.check(&ParamValue::List(vec![ParamValue::Float(0.1); 5])),
        Err(ParamError::Length { count: 5, .. })
    ));
    assert!(matches!(
        stops.check(&ParamValue::List(vec![ParamValue::Float(2.0)])),
        Err(ParamError::OutOfRange { .. })
    ));
    assert!(matches!(
        stops.check(&ParamValue::List(vec![ParamValue::Int(1)])),
        Err(ParamError::WrongKind { .. })
    ));
}

#[test]
fn a_curve_has_its_length_and_strictly_increasing_x() {
    let tone = &every_kind()[7];
    assert!(
        tone.check(&ParamValue::Curve(vec![[0.0, 0.1], [0.5, 0.4], [1.0, 0.9]]))
            .is_ok()
    );
    assert!(matches!(
        tone.check(&ParamValue::Curve(vec![[0.0, 0.0]])),
        Err(ParamError::Length { count: 1, .. })
    ));
    for bad in [vec![[0.5, 0.0], [0.5, 1.0]], vec![[0.6, 0.0], [0.4, 1.0]]] {
        assert_eq!(
            tone.check(&ParamValue::Curve(bad)),
            Err(ParamError::CurveNotIncreasing)
        );
    }
}

/// Why each spec is refused is held exactly, and not only that it is: the variant is what tells a plugin author what
/// to change (a spec with inverted limits is also a spec whose default cannot be inside them, and the second message
/// would hide the first).
#[test]
fn a_spec_that_does_not_hold_together_is_refused_for_the_right_reason() {
    let limits = |what| Err(ParamError::BadLimits { what });
    let default = |e: ParamError| Err(ParamError::BadDefault(Box::new(e)));
    let int = |min, max, default| spec("x", ParamKind::Int { min, max, default });
    let float = |min, max, default| spec("x", ParamKind::Float { min, max, default });
    let enumeration = |choices: &[&str], default| {
        spec(
            "x",
            ParamKind::Enum {
                choices: choices.iter().map(|c| c.to_string()).collect(),
                default,
            },
        )
    };
    let list = |item: ParamKind, min_len, max_len, default| {
        spec(
            "x",
            ParamKind::List {
                item: Box::new(item),
                min_len,
                max_len,
                default,
            },
        )
    };
    let bool_item = || ParamKind::Bool { default: false };
    let curve = |min_points, max_points, default| {
        spec(
            "x",
            ParamKind::Curve {
                min_points,
                max_points,
                default,
            },
        )
    };
    let cases: Vec<(&str, ParamSpec, Result<(), ParamError>)> = vec![
        // Limits the wrong way round, one kind at a time: the explicit check names the cause.
        ("int limits", int(10, 1, 5), limits("min is above max")),
        (
            "float limits",
            float(1.0, 0.0, 0.5),
            limits("min is above max"),
        ),
        (
            "point box, x",
            spec(
                "x",
                ParamKind::Point {
                    min: [1.0, 0.0],
                    max: [0.0, 1.0],
                    default: [0.5, 0.5],
                },
            ),
            limits("min is above max"),
        ),
        (
            "point box, y",
            spec(
                "x",
                ParamKind::Point {
                    min: [0.0, 1.0],
                    max: [1.0, 0.0],
                    default: [0.5, 0.5],
                },
            ),
            limits("min is above max"),
        ),
        (
            "list lengths",
            list(bool_item(), 3, 1, vec![]),
            limits("min_len is above max_len"),
        ),
        (
            "curve point counts",
            curve(4, 2, vec![[0.0, 0.0], [1.0, 1.0]]),
            limits("min_points is above max_points"),
        ),
        // A list whose item kind is itself wrong, with an empty default that hides it from the default's check.
        (
            "a list of items with inverted limits",
            list(
                ParamKind::Int {
                    min: 5,
                    max: 1,
                    default: 3,
                },
                0,
                3,
                vec![],
            ),
            limits("min is above max"),
        ),
        // Nothing to choose from, or a choice without a label (an empty or a blank one).
        (
            "no choice",
            enumeration(&[], 0),
            limits("an enum needs choices, each with a label"),
        ),
        (
            "an empty choice label",
            enumeration(&["a", ""], 0),
            limits("an enum needs choices, each with a label"),
        ),
        (
            "a blank choice label",
            enumeration(&["a", " "], 0),
            limits("an enum needs choices, each with a label"),
        ),
        // A limit that is not a number.
        (
            "a limit that is not finite",
            float(f64::NEG_INFINITY, 1.0, 0.0),
            Err(ParamError::NotFinite),
        ),
        // A default the spec does not allow.
        (
            "an int default past the limit",
            int(0, 10, 11),
            default(ParamError::OutOfRange { what: "the number" }),
        ),
        (
            "a default index past the choices",
            enumeration(&["a"], 1),
            default(ParamError::NoSuchChoice { index: 1, count: 1 }),
        ),
        (
            "a default list of items past their own limit",
            list(
                ParamKind::Int {
                    min: 0,
                    max: 1,
                    default: 0,
                },
                0,
                3,
                vec![ParamValue::Int(2)],
            ),
            default(ParamError::OutOfRange { what: "the number" }),
        ),
        (
            "a curve default that does not increase",
            curve(2, 4, vec![[0.0, 0.0], [0.0, 1.0]]),
            default(ParamError::CurveNotIncreasing),
        ),
        // A key or a label that says nothing.
        (
            "an empty key",
            ParamSpec {
                key: " ".into(),
                label: "label".into(),
                kind: bool_item(),
            },
            Err(ParamError::EmptyKeyOrLabel),
        ),
        (
            "an empty label",
            ParamSpec {
                key: "x".into(),
                label: String::new(),
                kind: bool_item(),
            },
            Err(ParamError::EmptyKeyOrLabel),
        ),
    ];
    for (what, s, expected) in cases {
        assert_eq!(s.validate(), expected, "{what}");
    }
}

#[test]
fn a_key_is_a_name_every_format_can_store() {
    let keyed = |key: &str| ParamSpec {
        key: key.into(),
        label: "label".into(),
        kind: ParamKind::Bool { default: false },
    };
    for good in [
        "ev",
        "a",
        "highlight_recovery",
        "strength2",
        &"k".repeat(MAX_KEY_LEN),
    ] {
        assert_eq!(keyed(good).validate(), Ok(()), "{good:?}");
    }
    for (bad, what) in [
        (
            "ev ",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
        (" ev", "must start with a lowercase ASCII letter"),
        ("EV", "must start with a lowercase ASCII letter"),
        (
            "eV",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
        (
            "a\nb",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
        ("2fast", "must start with a lowercase ASCII letter"),
        ("_x", "must start with a lowercase ASCII letter"),
        (
            "a-b",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
        (
            "a.b",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
        (
            "température",
            "may hold only lowercase ASCII letters, digits and `_`",
        ),
    ] {
        assert_eq!(
            keyed(bad).validate(),
            Err(ParamError::BadKey { what }),
            "{bad:?}"
        );
    }
    assert_eq!(
        keyed(&"k".repeat(MAX_KEY_LEN + 1)).validate(),
        Err(ParamError::BadKey {
            what: "is longer than 64 characters"
        })
    );
}

#[test]
fn what_a_plugin_writes_in_json_is_read_back() {
    for s in every_kind() {
        let text = serde_json::to_string(&s).unwrap();
        let back: ParamSpec = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s, "{text}");
    }
    // A plugin in another language writes whole numbers where we write floats.
    let by_hand: ParamSpec = serde_json::from_str(
        r#"{ "key": "gain", "label": "gain-label", "type": "float", "min": 0, "max": 4, "default": 1 }"#,
    )
    .unwrap();
    assert_eq!(
        by_hand.kind,
        ParamKind::Float {
            min: 0.0,
            max: 4.0,
            default: 1.0
        }
    );
    for v in [
        ParamValue::Bool(true),
        ParamValue::Int(-3),
        ParamValue::Float(0.25),
        ParamValue::Enum(2),
        ParamValue::Colour([0.1, 0.2, 0.3]),
        ParamValue::Point([0.5, 0.5]),
        ParamValue::List(vec![ParamValue::Int(1), ParamValue::Int(2)]),
        ParamValue::Curve(vec![[0.0, 0.0], [1.0, 1.0]]),
    ] {
        let text = serde_json::to_string(&v).unwrap();
        assert_eq!(
            serde_json::from_str::<ParamValue>(&text).unwrap(),
            v,
            "{text}"
        );
    }
}

#[test]
fn an_operation_identifier_is_its_text() {
    let id = OperationId::from("auroraw.exposure");
    assert_eq!(id.as_str(), "auroraw.exposure");
    assert_eq!(id.to_string(), "auroraw.exposure");
    assert_eq!(serde_json::to_string(&id).unwrap(), r#""auroraw.exposure""#);
    assert_eq!(
        serde_json::from_str::<OperationId>(r#""auroraw.exposure""#).unwrap(),
        id
    );
    assert!(OperationId::new("a") < OperationId::new("b"));
    assert_eq!(OperationId::from(String::from("a")), OperationId::new("a"));
}

proptest! {
    /// A float parameter allows exactly what is between its limits.
    #[test]
    fn a_float_is_allowed_exactly_between_its_limits(
        a in -1.0e6f64..1.0e6,
        b in -1.0e6f64..1.0e6,
        v in -2.0e6f64..2.0e6,
    ) {
        let (min, max) = if a <= b { (a, b) } else { (b, a) };
        let s = spec("x", ParamKind::Float { min, max, default: min });
        prop_assert!(s.validate().is_ok());
        prop_assert_eq!(s.check(&ParamValue::Float(v)).is_ok(), v >= min && v <= max);
    }

    /// A curve is allowed exactly when its x strictly increase. The x are whole numbers from a small range, so that
    /// equal neighbours (the edge of the rule) are as common as increasing runs, which random floats never give.
    #[test]
    fn a_curve_is_allowed_exactly_when_its_x_strictly_increase(
        points in proptest::collection::vec((0i32..6, -10.0f64..10.0), 2..8),
    ) {
        let s = spec("x", ParamKind::Curve { min_points: 2, max_points: 8, default: vec![[0.0, 0.0], [1.0, 1.0]] });
        let curve: Vec<[f64; 2]> = points.iter().map(|(x, y)| [f64::from(*x), *y]).collect();
        let increasing = curve.windows(2).all(|w| w[0][0] < w[1][0]);
        prop_assert_eq!(s.check(&ParamValue::Curve(curve)).is_ok(), increasing);
    }

    /// A spec made of limits in the right order and a default between them holds together, and allows its own
    /// default (and, for a number, refuses what is just past the limits).
    #[test]
    fn a_spec_with_its_default_between_its_limits_holds_together_and_allows_it(
        a in -1000i64..1000,
        b in -1000i64..1000,
        c in -1000i64..1000,
        n in 1usize..6,
        d in 0usize..6,
        point in proptest::array::uniform4(-100.0f64..100.0),
        tint in proptest::array::uniform3(-10.0f64..10.0),
        flag in any::<bool>(),
    ) {
        let mut v = [a, b, c];
        v.sort_unstable();
        let (min, default, max) = (v[0], v[1], v[2]);
        let (lo, hi) = ([point[0].min(point[1]), point[2].min(point[3])], [point[0].max(point[1]), point[2].max(point[3])]);
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let kinds = vec![
            ParamKind::Bool { default: flag },
            ParamKind::Int { min, max, default },
            ParamKind::Float { min: min as f64, max: max as f64, default: default as f64 },
            ParamKind::Enum { choices: (0..n).map(|i| format!("c{i}")).collect(), default: (d % n) as u32 },
            ParamKind::Colour { default: tint },
            ParamKind::Point { min: lo, max: hi, default: mid },
            ParamKind::List {
                item: Box::new(ParamKind::Int { min, max, default }),
                min_len: 0,
                max_len: n as u32,
                default: vec![ParamValue::Int(default); d % (n + 1)],
            },
            ParamKind::Curve { min_points: 2, max_points: 6, default: vec![[0.0, tint[0]], [1.0, tint[1]]] },
        ];
        for kind in kinds {
            let s = spec("x", kind);
            prop_assert_eq!(s.validate(), Ok(()), "{:?}", s);
            prop_assert_eq!(s.check(&s.default_value()), Ok(()), "{:?}", s);
        }
        // Just past the limits of the number is refused, and the limits themselves are not.
        let s = spec("x", ParamKind::Int { min, max, default });
        prop_assert!(s.check(&ParamValue::Int(min)).is_ok() && s.check(&ParamValue::Int(max)).is_ok());
        prop_assert!(s.check(&ParamValue::Int(max + 1)).is_err() && s.check(&ParamValue::Int(min - 1)).is_err());
    }
}
