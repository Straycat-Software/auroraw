// SPDX-License-Identifier: MIT OR Apache-2.0
//! What an operation plugin declares about its parameters, and what a version stores against it (decision D-142,
//! design note 005 §5, architecture §8.3).
//!
//! A declaration's **typed parameters** are the contract between three readers: the panels, which draw a control from a
//! parameter's type and limits; the version sidecar, which stores `(operation identifier, operation version, values)`
//! against it; and the pipeline, which hashes the values into its cache keys. So the types are defined here, in the crate
//! plugins depend on, and not in any of the three. There are eight kinds: a switch, a whole number, a real number, a
//! choice among named options, a colour, a point, a list of values of one kind, and a curve of control points.
//!
//! A [`ParamSpec`] says what a parameter may be ([`ParamSpec::check`] says whether a [`ParamValue`] is that) and what it
//! is by default; [`ParamSpec::validate`] says whether the spec itself holds together (its default is within its own
//! limits, its limits are the right way round). Numbers must be finite: a NaN has no canonical encoding, and no limit
//! allows one.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The identifier of an operation, such as `auroraw.exposure` or a plugin's own.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// An identifier from its text.
    pub fn new(text: impl Into<String>) -> OperationId {
        OperationId(text.into())
    }

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for OperationId {
    fn from(text: &str) -> Self {
        OperationId(text.to_string())
    }
}

impl From<String> for OperationId {
    fn from(text: String) -> Self {
        OperationId(text)
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A typed parameter value: what a version stores for a parameter and what the pipeline renders with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParamValue {
    /// A switch.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A real number.
    Float(f64),
    /// One of a declared set of choices, by its index.
    Enum(u32),
    /// A colour, as three linear components.
    Colour([f64; 3]),
    /// A point, as two coordinates.
    Point([f64; 2]),
    /// A list of values.
    List(Vec<ParamValue>),
    /// A curve, as control points `(x, y)` in order.
    Curve(Vec<[f64; 2]>),
}

/// What an operation costs, so that `develop` can place it by the order rule the spike proved (an operation that is heavy
/// and rarely changed goes early, one the person drags goes late: white balance after the denoiser costs 0.6 ms to change,
/// against 120 ms before it) or warn when a placement defeats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CostClass {
    /// Heavy and rarely changed: a denoiser, a demosaic.
    Heavy,
    /// Light and dragged: an exposure, a tone curve.
    Interactive,
}

/// What a parameter may be, with its limits and its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ParamKind {
    /// A switch.
    Bool {
        /// Its default.
        default: bool,
    },
    /// A whole number between `min` and `max`, both included.
    Int {
        /// The least.
        min: i64,
        /// The most.
        max: i64,
        /// Its default.
        default: i64,
    },
    /// A real number between `min` and `max`, both included.
    Float {
        /// The least.
        min: f64,
        /// The most.
        max: f64,
        /// Its default.
        default: f64,
    },
    /// One of named choices; the value is the index.
    Enum {
        /// The choices, as label keys, in order: at least one.
        choices: Vec<String>,
        /// The index of the default.
        default: u32,
    },
    /// A colour, as three finite linear components.
    Colour {
        /// Its default.
        default: [f64; 3],
    },
    /// A point within the box from `min` to `max`.
    Point {
        /// The least of each coordinate.
        min: [f64; 2],
        /// The most of each coordinate.
        max: [f64; 2],
        /// Its default.
        default: [f64; 2],
    },
    /// A list of between `min_len` and `max_len` values of one kind.
    List {
        /// What each item is.
        item: Box<ParamKind>,
        /// The fewest items.
        min_len: u32,
        /// The most items.
        max_len: u32,
        /// Its default.
        default: Vec<ParamValue>,
    },
    /// A curve of between `min_points` and `max_points` control points `(x, y)`, with strictly increasing `x`.
    Curve {
        /// The fewest points.
        min_points: u32,
        /// The most points.
        max_points: u32,
        /// Its default.
        default: Vec<[f64; 2]>,
    },
}

/// A parameter of an operation: its key, the key of its label, and what it may be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    /// The key the version stores the value under: stable, unique among the operation's parameters.
    pub key: String,
    /// The key of the label the interface translates for the control.
    pub label: String,
    /// What the parameter may be.
    #[serde(flatten)]
    pub kind: ParamKind,
}

/// Why a value is not what a parameter allows, or why a spec does not hold together.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParamError {
    /// The value is of another kind than the parameter.
    #[error("expected {expected}")]
    WrongKind {
        /// What the parameter is.
        expected: &'static str,
    },
    /// A number is NaN or infinite.
    #[error("a number is not finite")]
    NotFinite,
    /// A number is outside the limits.
    #[error("{what} is outside the limits")]
    OutOfRange {
        /// Which number.
        what: &'static str,
    },
    /// A choice index is past the last choice.
    #[error("choice {index} of {count}")]
    NoSuchChoice {
        /// The index.
        index: u32,
        /// How many choices there are.
        count: usize,
    },
    /// A list or a curve has too few or too many items.
    #[error("{count} items, between {min} and {max} are allowed")]
    Length {
        /// How many it has.
        count: usize,
        /// The fewest allowed.
        min: u32,
        /// The most allowed.
        max: u32,
    },
    /// The `x` of a curve's points do not strictly increase.
    #[error("the x of the curve's points must strictly increase")]
    CurveNotIncreasing,
    /// The limits of a spec are the wrong way round, or a spec has nothing to choose from.
    #[error("the limits are inconsistent: {what}")]
    BadLimits {
        /// What is wrong.
        what: &'static str,
    },
    /// The default of a spec is not a value the spec allows.
    #[error("the default is not allowed by the parameter: {0}")]
    BadDefault(Box<ParamError>),
    /// A parameter has an empty key or an empty label.
    #[error("a parameter needs a key and a label")]
    EmptyKeyOrLabel,
}

fn finite(values: &[f64]) -> Result<(), ParamError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(ParamError::NotFinite)
    }
}

fn within(v: f64, min: f64, max: f64, what: &'static str) -> Result<(), ParamError> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(ParamError::OutOfRange { what })
    }
}

fn length(count: usize, min: u32, max: u32) -> Result<(), ParamError> {
    if count >= min as usize && count <= max as usize {
        Ok(())
    } else {
        Err(ParamError::Length { count, min, max })
    }
}

impl ParamKind {
    /// What the kind is, as a word.
    fn word(&self) -> &'static str {
        match self {
            ParamKind::Bool { .. } => "a bool",
            ParamKind::Int { .. } => "an int",
            ParamKind::Float { .. } => "a float",
            ParamKind::Enum { .. } => "an enum",
            ParamKind::Colour { .. } => "a colour",
            ParamKind::Point { .. } => "a point",
            ParamKind::List { .. } => "a list",
            ParamKind::Curve { .. } => "a curve",
        }
    }

    /// The default value of the kind.
    pub fn default_value(&self) -> ParamValue {
        match self {
            ParamKind::Bool { default } => ParamValue::Bool(*default),
            ParamKind::Int { default, .. } => ParamValue::Int(*default),
            ParamKind::Float { default, .. } => ParamValue::Float(*default),
            ParamKind::Enum { default, .. } => ParamValue::Enum(*default),
            ParamKind::Colour { default } => ParamValue::Colour(*default),
            ParamKind::Point { default, .. } => ParamValue::Point(*default),
            ParamKind::List { default, .. } => ParamValue::List(default.clone()),
            ParamKind::Curve { default, .. } => ParamValue::Curve(default.clone()),
        }
    }

    /// Whether `value` is something this kind allows: of the kind, finite, within the limits.
    pub fn check(&self, value: &ParamValue) -> Result<(), ParamError> {
        match (self, value) {
            (ParamKind::Bool { .. }, ParamValue::Bool(_)) => Ok(()),
            (ParamKind::Int { min, max, .. }, ParamValue::Int(v)) => {
                if (*min..=*max).contains(v) {
                    Ok(())
                } else {
                    Err(ParamError::OutOfRange { what: "the number" })
                }
            }
            (ParamKind::Float { min, max, .. }, ParamValue::Float(v)) => {
                finite(&[*v])?;
                within(*v, *min, *max, "the number")
            }
            (ParamKind::Enum { choices, .. }, ParamValue::Enum(index)) => {
                if (*index as usize) < choices.len() {
                    Ok(())
                } else {
                    Err(ParamError::NoSuchChoice {
                        index: *index,
                        count: choices.len(),
                    })
                }
            }
            (ParamKind::Colour { .. }, ParamValue::Colour(c)) => finite(c),
            (ParamKind::Point { min, max, .. }, ParamValue::Point(p)) => {
                finite(p)?;
                within(p[0], min[0], max[0], "the x of the point")?;
                within(p[1], min[1], max[1], "the y of the point")
            }
            (
                ParamKind::List {
                    item,
                    min_len,
                    max_len,
                    ..
                },
                ParamValue::List(items),
            ) => {
                length(items.len(), *min_len, *max_len)?;
                items.iter().try_for_each(|v| item.check(v))
            }
            (
                ParamKind::Curve {
                    min_points,
                    max_points,
                    ..
                },
                ParamValue::Curve(points),
            ) => {
                length(points.len(), *min_points, *max_points)?;
                for p in points {
                    finite(p)?;
                }
                if points.windows(2).all(|w| w[0][0] < w[1][0]) {
                    Ok(())
                } else {
                    Err(ParamError::CurveNotIncreasing)
                }
            }
            (kind, _) => Err(ParamError::WrongKind {
                expected: kind.word(),
            }),
        }
    }

    /// Whether the kind holds together: finite limits the right way round, a choice to choose from, and a default that
    /// the kind itself allows.
    pub fn validate(&self) -> Result<(), ParamError> {
        match self {
            ParamKind::Bool { .. } | ParamKind::Colour { .. } => {}
            ParamKind::Int { min, max, .. } => {
                if min > max {
                    return Err(ParamError::BadLimits {
                        what: "min is above max",
                    });
                }
            }
            ParamKind::Float { min, max, .. } => {
                finite(&[*min, *max])?;
                if min > max {
                    return Err(ParamError::BadLimits {
                        what: "min is above max",
                    });
                }
            }
            ParamKind::Enum { choices, .. } => {
                if choices.is_empty() || choices.iter().any(|c| c.trim().is_empty()) {
                    return Err(ParamError::BadLimits {
                        what: "an enum needs choices, each with a label",
                    });
                }
            }
            ParamKind::Point { min, max, .. } => {
                finite(min)?;
                finite(max)?;
                if min[0] > max[0] || min[1] > max[1] {
                    return Err(ParamError::BadLimits {
                        what: "min is above max",
                    });
                }
            }
            ParamKind::List {
                item,
                min_len,
                max_len,
                ..
            } => {
                if min_len > max_len {
                    return Err(ParamError::BadLimits {
                        what: "min_len is above max_len",
                    });
                }
                item.validate()?;
            }
            ParamKind::Curve {
                min_points,
                max_points,
                ..
            } => {
                if min_points > max_points {
                    return Err(ParamError::BadLimits {
                        what: "min_points is above max_points",
                    });
                }
            }
        }
        self.check(&self.default_value())
            .map_err(|e| ParamError::BadDefault(Box::new(e)))
    }
}

impl ParamSpec {
    /// The default value of the parameter.
    pub fn default_value(&self) -> ParamValue {
        self.kind.default_value()
    }

    /// Whether `value` is something the parameter allows.
    pub fn check(&self, value: &ParamValue) -> Result<(), ParamError> {
        self.kind.check(value)
    }

    /// Whether the parameter holds together: it has a key and a label, and its kind does.
    pub fn validate(&self) -> Result<(), ParamError> {
        if self.key.trim().is_empty() || self.label.trim().is_empty() {
            return Err(ParamError::EmptyKeyOrLabel);
        }
        self.kind.validate()
    }
}
