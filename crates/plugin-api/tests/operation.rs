// SPDX-License-Identifier: MIT OR Apache-2.0
//! The typed parameters of an operation (D-142): what each kind allows, what a spec that does not hold together is
//! refused for, and that the JSON a plugin writes is the JSON this crate reads.

use auroraw_plugin_api::{OperationId, ParamError, ParamKind, ParamSpec, ParamValue};
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

#[test]
fn a_spec_that_does_not_hold_together_is_refused() {
    let bad: Vec<(&str, ParamSpec)> = vec![
        (
            "limits the wrong way round",
            spec(
                "x",
                ParamKind::Float {
                    min: 1.0,
                    max: 0.0,
                    default: 0.5,
                },
            ),
        ),
        (
            "a default past the limit",
            spec(
                "x",
                ParamKind::Int {
                    min: 0,
                    max: 10,
                    default: 11,
                },
            ),
        ),
        (
            "no choice",
            spec(
                "x",
                ParamKind::Enum {
                    choices: Vec::new(),
                    default: 0,
                },
            ),
        ),
        (
            "a default index past the choices",
            spec(
                "x",
                ParamKind::Enum {
                    choices: vec!["a".into()],
                    default: 1,
                },
            ),
        ),
        (
            "a limit that is not finite",
            spec(
                "x",
                ParamKind::Float {
                    min: f64::NEG_INFINITY,
                    max: 1.0,
                    default: 0.0,
                },
            ),
        ),
        (
            "a default list of items past their own limit",
            spec(
                "x",
                ParamKind::List {
                    item: Box::new(ParamKind::Int {
                        min: 0,
                        max: 1,
                        default: 0,
                    }),
                    min_len: 0,
                    max_len: 3,
                    default: vec![ParamValue::Int(2)],
                },
            ),
        ),
        (
            "a curve default that does not increase",
            spec(
                "x",
                ParamKind::Curve {
                    min_points: 2,
                    max_points: 4,
                    default: vec![[0.0, 0.0], [0.0, 1.0]],
                },
            ),
        ),
        (
            "an empty key",
            ParamSpec {
                key: " ".into(),
                label: "label".into(),
                kind: ParamKind::Bool { default: false },
            },
        ),
        (
            "an empty label",
            ParamSpec {
                key: "x".into(),
                label: String::new(),
                kind: ParamKind::Bool { default: false },
            },
        ),
    ];
    for (what, s) in bad {
        assert!(s.validate().is_err(), "{what}");
    }
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

    /// A curve is allowed exactly when its x strictly increase.
    #[test]
    fn a_curve_is_allowed_exactly_when_its_x_strictly_increase(
        points in proptest::collection::vec((-10.0f64..10.0, -10.0f64..10.0), 2..8),
    ) {
        let s = spec("x", ParamKind::Curve { min_points: 2, max_points: 8, default: vec![[0.0, 0.0], [1.0, 1.0]] });
        let curve: Vec<[f64; 2]> = points.iter().map(|(x, y)| [*x, *y]).collect();
        let increasing = curve.windows(2).all(|w| w[0][0] < w[1][0]);
        prop_assert_eq!(s.check(&ParamValue::Curve(curve)).is_ok(), increasing);
    }
}
