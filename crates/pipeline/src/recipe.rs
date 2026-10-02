// SPDX-License-Identifier: GPL-3.0-or-later
//! The recipe: the pipeline's only input about a version (design note 005 §2.2, D-140). A plain value that
//! `develop` builds: a definition version and the operation instances in pipeline order, each with its
//! operation identifier, its own version, whether it is enabled and its typed parameters.
//!
//! The recipe knows nothing of sidecars, history or versions in the catalogue's sense, and has **no `mask`
//! field before M3**: the definition version is the extension point.
//!
//! **The types are `plugin-api`'s.** `OperationId` and `ParamValue` are defined there (D-140, D-142), because plugins
//! declare them and the version sidecar stores them; this module re-exports them so that a recipe reads the same as
//! before, and holds what is the pipeline's alone: **the canonical binary encoding of a value** (the bit pattern,
//! `-0.0` normalised, `NaN` refused, never JSON), which the cache keys and the proof of determinism are hashes of.
//! `plugin-api` says whether a value is *allowed* (`ParamKind::check`); it does not normalise one, and the encoding
//! cannot be a method of a type this crate does not own, so it is [`encode_param`].

pub use auroraw_plugin_api::{OperationId, ParamValue};

/// Why a value has no canonical encoding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// A number that is not a number. It would make two equal recipes hash differently (a `NaN` is not
    /// equal to itself) and no declaration's limits allow one, so it is refused, not normalised.
    #[error("a parameter holds NaN, which has no canonical encoding")]
    NotANumber,
}

/// A float's canonical bytes: its bit pattern, with `-0.0` normalised to `+0.0` (they compare equal and
/// must hash alike) and `NaN` refused.
fn put_float(out: &mut Vec<u8>, v: f64) -> Result<(), EncodeError> {
    if v.is_nan() {
        return Err(EncodeError::NotANumber);
    }
    let v = if v == 0.0 { 0.0 } else { v };
    out.extend_from_slice(&v.to_bits().to_le_bytes());
    Ok(())
}

/// Appends the canonical encoding: a tag byte, then the payload; every sequence length-prefixed, so
/// that two different values never encode alike.
pub fn encode_param(value: &ParamValue, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    match value {
        ParamValue::Bool(b) => out.extend_from_slice(&[0, u8::from(*b)]),
        ParamValue::Int(i) => {
            out.push(1);
            out.extend_from_slice(&i.to_le_bytes());
        }
        ParamValue::Float(v) => {
            out.push(2);
            put_float(out, *v)?;
        }
        ParamValue::Enum(i) => {
            out.push(3);
            out.extend_from_slice(&i.to_le_bytes());
        }
        ParamValue::Colour(c) => {
            out.push(4);
            for v in c {
                put_float(out, *v)?;
            }
        }
        ParamValue::Point(p) => {
            out.push(5);
            for v in p {
                put_float(out, *v)?;
            }
        }
        ParamValue::List(items) => {
            out.push(6);
            out.extend_from_slice(&(items.len() as u32).to_le_bytes());
            for item in items {
                encode_param(item, out)?;
            }
        }
        ParamValue::Curve(points) => {
            out.push(7);
            out.extend_from_slice(&(points.len() as u32).to_le_bytes());
            for point in points {
                for v in point {
                    put_float(out, *v)?;
                }
            }
        }
    }
    Ok(())
}

/// An operation in a recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationInstance {
    /// Which operation.
    pub operation: OperationId,
    /// The operation's own version (architecture §7.2), recorded so that an old edit renders the same
    /// after the operation is updated.
    pub op_version: u32,
    /// Whether it runs. An operation whose plugin is missing arrives disabled with its parameters kept.
    pub enabled: bool,
    /// Its parameters, typed by its declaration.
    pub params: Vec<ParamValue>,
}

impl OperationInstance {
    /// An enabled instance of version 1 with `params`.
    pub fn new(operation: &str, params: Vec<ParamValue>) -> OperationInstance {
        OperationInstance {
            operation: OperationId::from(operation),
            op_version: 1,
            enabled: true,
            params,
        }
    }

    /// Appends the canonical encoding: the identifier, the version and the parameters. The `enabled` flag
    /// is not in it: a disabled operation is left out of a recipe's hash altogether (see `plan`).
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        let id = self.operation.as_str();
        out.extend_from_slice(&(id.len() as u32).to_le_bytes());
        out.extend_from_slice(id.as_bytes());
        out.extend_from_slice(&self.op_version.to_le_bytes());
        out.extend_from_slice(&(self.params.len() as u32).to_le_bytes());
        for param in &self.params {
            encode_param(param, out)?;
        }
        Ok(())
    }
}

/// What the pipeline renders: a definition version and the operations in pipeline order.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// The pipeline definition the recipe is written against ([`crate::definition::by_version`]).
    pub definition: u32,
    /// The operations, in pipeline order, already placed by `develop`.
    pub operations: Vec<OperationInstance>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(v: &ParamValue) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::new();
        encode_param(v, &mut out)?;
        Ok(out)
    }

    /// The bytes themselves, one value of each of the eight kinds, as hex: the contract the cache keys and the proof of
    /// determinism are hashes of. A tag, a width or a length prefix that changes (a list's count as `u16`, say) moves
    /// every key of every saved recipe, so it is a decision that edits this test, and after the definition's release a
    /// new definition version.
    #[test]
    fn the_canonical_bytes_of_each_kind_are_these() {
        let hex = |v: &ParamValue| {
            encoded(v)
                .expect("encodes")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let one = "000000000000f03f";
        let half = "000000000000e03f";
        let zero = "0000000000000000";
        let two = "0000000000000040";
        let minus_one = "000000000000f0bf";
        for (value, expected) in [
            (ParamValue::Bool(true), "0001".to_string()),
            (ParamValue::Int(-2), "01feffffffffffffff".to_string()),
            (ParamValue::Float(1.0), format!("02{one}")),
            (ParamValue::Enum(3), "0303000000".to_string()),
            (
                ParamValue::Colour([1.0, 0.5, 0.0]),
                format!("04{one}{half}{zero}"),
            ),
            (
                ParamValue::Point([2.0, -1.0]),
                format!("05{two}{minus_one}"),
            ),
            (
                ParamValue::List(vec![ParamValue::Bool(true), ParamValue::Int(1)]),
                "06020000000001010100000000000000".to_string(),
            ),
            (
                ParamValue::Curve(vec![[0.0, 0.0], [1.0, 1.0]]),
                format!("0702000000{zero}{zero}{one}{one}"),
            ),
        ] {
            assert_eq!(hex(&value), expected, "{value:?}");
        }
    }

    #[test]
    fn negative_zero_and_zero_encode_alike() {
        assert_eq!(
            encoded(&ParamValue::Float(-0.0)),
            encoded(&ParamValue::Float(0.0))
        );
        assert_eq!(
            encoded(&ParamValue::Colour([-0.0, 1.0, -0.0])),
            encoded(&ParamValue::Colour([0.0, 1.0, 0.0]))
        );
        assert_eq!(
            encoded(&ParamValue::Curve(vec![[-0.0, -0.0], [1.0, 1.0]])),
            encoded(&ParamValue::Curve(vec![[0.0, 0.0], [1.0, 1.0]]))
        );
    }

    #[test]
    fn nan_has_no_encoding_wherever_it_hides() {
        for v in [
            ParamValue::Float(f64::NAN),
            ParamValue::Colour([0.0, f64::NAN, 0.0]),
            ParamValue::Point([f64::NAN, 0.0]),
            ParamValue::List(vec![ParamValue::Int(1), ParamValue::Float(f64::NAN)]),
            ParamValue::Curve(vec![[0.0, 0.0], [0.5, f64::NAN]]),
        ] {
            assert_eq!(encoded(&v), Err(EncodeError::NotANumber), "{v:?}");
        }
        // Infinity is a number: the declaration's limits refuse it, the encoding does not.
        assert!(encoded(&ParamValue::Float(f64::INFINITY)).is_ok());
    }

    #[test]
    fn different_values_never_encode_alike() {
        let values = [
            ParamValue::Bool(true),
            ParamValue::Bool(false),
            ParamValue::Int(1),
            ParamValue::Float(1.0),
            ParamValue::Enum(1),
            ParamValue::Colour([1.0, 0.0, 0.0]),
            ParamValue::Point([1.0, 0.0]),
            ParamValue::List(vec![]),
            ParamValue::List(vec![ParamValue::Int(1)]),
            ParamValue::List(vec![ParamValue::List(vec![ParamValue::Int(1)])]),
            ParamValue::Curve(vec![]),
            ParamValue::Curve(vec![[1.0, 0.0]]),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for v in &values {
            assert!(
                seen.insert(encoded(v).expect("encodes")),
                "{v:?} encodes like another value"
            );
        }
        // The type is in the encoding: an int 1, a float 1.0 and an enum 1 are three values.
        assert_ne!(
            encoded(&ParamValue::Int(1)),
            encoded(&ParamValue::Float(1.0))
        );
        assert_ne!(encoded(&ParamValue::Int(1)), encoded(&ParamValue::Enum(1)));
    }

    #[test]
    fn a_list_of_two_is_not_two_lists_of_one() {
        let two = ParamValue::List(vec![ParamValue::Int(1), ParamValue::Int(2)]);
        let nested = ParamValue::List(vec![
            ParamValue::List(vec![ParamValue::Int(1)]),
            ParamValue::List(vec![ParamValue::Int(2)]),
        ]);
        assert_ne!(encoded(&two), encoded(&nested));
    }

    #[test]
    fn an_operation_encodes_its_identifier_version_and_parameters() {
        let a = OperationInstance::new("auroraw.exposure", vec![ParamValue::Float(0.5)]);
        let mut other_version = a.clone();
        other_version.op_version = 2;
        let mut other_param = a.clone();
        other_param.params = vec![ParamValue::Float(0.25)];
        let enc = |o: &OperationInstance| {
            let mut out = Vec::new();
            o.encode(&mut out).expect("encodes");
            out
        };
        assert_ne!(enc(&a), enc(&other_version));
        assert_ne!(enc(&a), enc(&other_param));
        assert_ne!(
            enc(&a),
            enc(&OperationInstance::new(
                "auroraw.tone",
                vec![ParamValue::Float(0.5)]
            ))
        );
    }

    #[test]
    fn an_identifier_prefix_does_not_collide_with_a_longer_one() {
        // Length-prefixing: "a" with a parameter must not equal "ab" with one fewer byte.
        let enc = |id: &str, p: Vec<ParamValue>| {
            let mut out = Vec::new();
            OperationInstance::new(id, p)
                .encode(&mut out)
                .expect("encodes");
            out
        };
        assert_ne!(enc("ab", vec![]), enc("a", vec![ParamValue::Enum(0)]));
    }
}
