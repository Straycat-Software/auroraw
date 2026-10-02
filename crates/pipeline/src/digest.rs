// SPDX-License-Identifier: GPL-3.0-or-later
//! The digest of a version's **state as stored** (design note 007 §4.4): one function that `develop`, the version
//! sidecar and the catalogue share, so that "did the state change", the `digest` of every history step and the key of
//! the main version's thumbnail are the same hash.
//!
//! It is **not** the hash of the recipe the pipeline receives. That recipe is positional, so its hash is a function of
//! the state *and of the declarations*: an update that adds an optional parameter, or a declaration that reorders its
//! parameters, would change every recomputed digest, and the history of every saved version would be set aside as
//! "edited by something else". The state is stored by key, so the digest is made of keys: it does not cover the
//! plugin's release, the declarations or the positional list, and it **does** cover `enabled`, which the stage keys
//! ([`crate::plan`]) leave out because a disabled operation does not change a cache.
//!
//! **The bytes are a persisted contract**, like the stage keys' proof of determinism (D-140): a digest is written in a
//! sidecar and compared by a later build. A test holds them, and changing what the digest covers is a **new tag**
//! ([`DIGEST_TAG`]), which a build that does not know it reads as "not checkable", not as "different". The tag is in the
//! hashed stream and in the text form of a digest, so that a stored digest says which definition of the hash it is.
//!
//! The stream, in order, every length a `u32` in little endian and every integer little endian:
//!
//! 1. the bytes `auroraw-state-digest` and a `0`, the tag, the definition version, the number of instances;
//! 2. per instance, **in the stored order**: the operation identifier (its length, then its UTF-8 bytes), the
//!    `op_version`, `enabled` as `0` or `1`, the number of parameters;
//! 3. per parameter, **sorted by the bytes of the key**: the key (its length, then its UTF-8 bytes), then the value in
//!    the canonical encoding of [`encode_param`] (the bit pattern, `-0.0` as `+0.0`, `NaN` refused).

use std::fmt;

use crate::recipe::{EncodeError, OperationId, ParamValue, encode_param};

/// The tag of this definition of the digest. A change of what the digest covers or of its stream is a new number, and a
/// build reads a digest of a tag it does not know as not checkable.
pub const DIGEST_TAG: u32 = 1;

/// The bytes that open the stream, so that no other hash in the application can equal one of these.
const PREFIX: &[u8] = b"auroraw-state-digest\0";

/// An operation as the version sidecar stores it: by key, not by position.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredInstance {
    /// Which operation.
    pub operation: OperationId,
    /// The version of the operation the values were written with.
    pub op_version: u32,
    /// Whether the person has it on. It is the person's, and what the machine could run is decided later, elsewhere.
    pub enabled: bool,
    /// The values the sidecar stores, by key, in any order: the digest sorts them.
    pub params: Vec<(String, ParamValue)>,
}

/// Why a state has no digest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// A value has no canonical encoding (a `NaN`).
    #[error(transparent)]
    Encode(#[from] EncodeError),
    /// An instance stores one key twice: the state is not what a sidecar writes, and no order of the two is the right one.
    #[error("the operation {operation} stores the parameter {key:?} twice")]
    DuplicateKey {
        /// The operation.
        operation: OperationId,
        /// The key.
        key: String,
    },
}

/// The digest of a state: a tag and 32 bytes of `blake3`.
///
/// Its text form is `<tag>:<64 lowercase hex digits>`, which is what a sidecar stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateDigest {
    tag: u32,
    hash: [u8; 32],
}

/// Why a text is not a digest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseDigestError {
    /// There is no `:` between a tag and the hash.
    #[error("a digest is `<tag>:<64 hex digits>`, and this has no `:`")]
    NoTag,
    /// The tag is not a whole number written in digits without a leading zero.
    #[error("the tag of a digest is a whole number, written in digits without a leading zero")]
    BadTag,
    /// The hash is not 64 lowercase hexadecimal digits.
    #[error("the hash of a digest is 64 lowercase hexadecimal digits")]
    BadHash,
}

impl StateDigest {
    /// The tag of the definition of the hash this digest was made with.
    pub fn tag(&self) -> u32 {
        self.tag
    }

    /// Whether this build knows the definition of the hash: only then can the digest be compared with one it computes.
    /// A digest that is not current is kept, not refused, and the next write recomputes it.
    pub fn is_current(&self) -> bool {
        self.tag == DIGEST_TAG
    }

    /// The hash.
    pub fn hash(&self) -> &[u8; 32] {
        &self.hash
    }

    /// Reads the text form. Any tag is accepted (see [`StateDigest::is_current`]); it is written in digits without a
    /// leading zero, and the hash is exactly 64 lowercase hexadecimal digits, so that **one digest has one text**.
    pub fn parse(text: &str) -> Result<StateDigest, ParseDigestError> {
        let (tag, hex) = text.split_once(':').ok_or(ParseDigestError::NoTag)?;
        // Digits only, and no leading zero (`01` and `1` would be two texts of one digest).
        if tag.is_empty()
            || !tag.bytes().all(|b| b.is_ascii_digit())
            || (tag.len() > 1 && tag.starts_with('0'))
        {
            return Err(ParseDigestError::BadTag);
        }
        let tag: u32 = tag.parse().map_err(|_| ParseDigestError::BadTag)?;
        if hex.len() != 64 {
            return Err(ParseDigestError::BadHash);
        }
        let mut hash = [0u8; 32];
        for (byte, pair) in hash.iter_mut().zip(hex.as_bytes().chunks(2)) {
            let digit = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            match (digit(pair[0]), digit(pair[1])) {
                (Some(high), Some(low)) => *byte = high << 4 | low,
                _ => return Err(ParseDigestError::BadHash),
            }
        }
        Ok(StateDigest { tag, hash })
    }
}

impl fmt::Display for StateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.tag)?;
        self.hash.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

fn put_len(hasher: &mut blake3::Hasher, len: usize) {
    hasher.update(&(len as u32).to_le_bytes());
}

fn put_text(hasher: &mut blake3::Hasher, text: &str) {
    put_len(hasher, text.len());
    hasher.update(text.as_bytes());
}

/// The digest of the state `instances` of a version written against the pipeline definition `definition`.
///
/// `Err` if a value holds a `NaN` or an instance stores a key twice; a state a sidecar reads back is never either.
pub fn state_digest(
    definition: u32,
    instances: &[StoredInstance],
) -> Result<StateDigest, DigestError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PREFIX);
    hasher.update(&DIGEST_TAG.to_le_bytes());
    hasher.update(&definition.to_le_bytes());
    put_len(&mut hasher, instances.len());
    for instance in instances {
        put_text(&mut hasher, instance.operation.as_str());
        hasher.update(&instance.op_version.to_le_bytes());
        hasher.update(&[u8::from(instance.enabled)]);
        put_len(&mut hasher, instance.params.len());
        let mut sorted: Vec<&(String, ParamValue)> = instance.params.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(pair) = sorted.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(DigestError::DuplicateKey {
                operation: instance.operation.clone(),
                key: pair[0].0.clone(),
            });
        }
        for (key, value) in sorted {
            put_text(&mut hasher, key);
            let mut bytes = Vec::new();
            encode_param(value, &mut bytes)?;
            hasher.update(&bytes);
        }
    }
    Ok(StateDigest {
        tag: DIGEST_TAG,
        hash: *hasher.finalize().as_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn instance(id: &str, enabled: bool, params: &[(&str, ParamValue)]) -> StoredInstance {
        StoredInstance {
            operation: OperationId::from(id),
            op_version: 1,
            enabled,
            params: params
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        }
    }

    /// A small state: the order the keys are given in is not the sorted one.
    fn state() -> Vec<StoredInstance> {
        vec![
            instance(
                "auroraw.exposure",
                true,
                &[
                    ("ev", ParamValue::Float(0.5)),
                    ("black", ParamValue::Float(0.0)),
                ],
            ),
            instance("auroraw.tone-map", false, &[("look", ParamValue::Enum(2))]),
        ]
    }

    fn digest(instances: &[StoredInstance]) -> StateDigest {
        state_digest(1, instances).expect("a digest")
    }

    /// The stream, written out by hand from the description of the module, and hashed: what the function must give.
    #[test]
    fn the_stream_is_the_one_the_module_describes() {
        let mut s: Vec<u8> = Vec::new();
        s.extend_from_slice(b"auroraw-state-digest\0");
        s.extend_from_slice(&1u32.to_le_bytes()); // the tag
        s.extend_from_slice(&1u32.to_le_bytes()); // the definition
        s.extend_from_slice(&2u32.to_le_bytes()); // two instances
        // exposure: enabled, version 1, two parameters, "black" before "ev"
        s.extend_from_slice(&16u32.to_le_bytes());
        s.extend_from_slice(b"auroraw.exposure");
        s.extend_from_slice(&1u32.to_le_bytes());
        s.push(1);
        s.extend_from_slice(&2u32.to_le_bytes());
        s.extend_from_slice(&5u32.to_le_bytes());
        s.extend_from_slice(b"black");
        s.push(2); // a float
        s.extend_from_slice(&0f64.to_le_bytes());
        s.extend_from_slice(&2u32.to_le_bytes());
        s.extend_from_slice(b"ev");
        s.push(2);
        s.extend_from_slice(&0.5f64.to_le_bytes());
        // tone map: disabled, version 1, one parameter
        s.extend_from_slice(&16u32.to_le_bytes());
        s.extend_from_slice(b"auroraw.tone-map");
        s.extend_from_slice(&1u32.to_le_bytes());
        s.push(0);
        s.extend_from_slice(&1u32.to_le_bytes());
        s.extend_from_slice(&4u32.to_le_bytes());
        s.extend_from_slice(b"look");
        s.push(3); // an enum
        s.extend_from_slice(&2u32.to_le_bytes());
        assert_eq!(digest(&state()).hash(), blake3::hash(&s).as_bytes());
    }

    /// The persisted contract, as text: if this changes, every stored digest is a different one, so it is a new tag.
    #[test]
    fn the_digest_of_a_known_state_is_this() {
        assert_eq!(
            digest(&state()).to_string(),
            "1:7006010bd1ba8779951b2b816569dba603ce90f39028422ab246b33618da05f0"
        );
    }

    #[test]
    fn the_order_of_the_keys_does_not_matter_and_the_order_of_the_instances_does() {
        let mut shuffled = state();
        shuffled[0].params.reverse();
        assert_eq!(digest(&state()), digest(&shuffled));
        let mut swapped = state();
        swapped.reverse();
        assert_ne!(digest(&state()), digest(&swapped));
    }

    /// The keys are sorted by their **bytes**: `B` (0x42) comes before `a` (0x61), where a sort that ignores the case
    /// would put `a` first. The stream is written by hand, in the order the bytes give.
    #[test]
    fn the_keys_are_sorted_by_their_bytes_and_not_by_their_letters() {
        let mut s: Vec<u8> = Vec::new();
        s.extend_from_slice(b"auroraw-state-digest\0");
        s.extend_from_slice(&1u32.to_le_bytes()); // the tag
        s.extend_from_slice(&1u32.to_le_bytes()); // the definition
        s.extend_from_slice(&1u32.to_le_bytes()); // one instance
        s.extend_from_slice(&3u32.to_le_bytes());
        s.extend_from_slice(b"a.b");
        s.extend_from_slice(&1u32.to_le_bytes());
        s.push(1);
        s.extend_from_slice(&3u32.to_le_bytes());
        for (key, n) in [("B", 1i64), ("a", 2), ("c", 3)] {
            s.extend_from_slice(&1u32.to_le_bytes());
            s.extend_from_slice(key.as_bytes());
            s.push(1); // an int
            s.extend_from_slice(&n.to_le_bytes());
        }
        let given = [("a", 2), ("c", 3), ("B", 1)].map(|(key, n)| (key, ParamValue::Int(n)));
        assert_eq!(
            digest(&[instance("a.b", true, &given)]).hash(),
            blake3::hash(&s).as_bytes()
        );
    }

    #[test]
    fn everything_the_state_says_moves_the_digest() {
        let base = digest(&state());
        let mut enabled = state();
        enabled[1].enabled = true;
        assert_ne!(base, digest(&enabled), "enabled is in the digest");
        let mut version = state();
        version[0].op_version = 2;
        assert_ne!(base, digest(&version), "the version of the operation");
        let mut value = state();
        value[0].params[0].1 = ParamValue::Float(0.6);
        assert_ne!(base, digest(&value), "a value");
        let mut key = state();
        key[0].params[0].0 = "exposure".to_string();
        assert_ne!(base, digest(&key), "a key");
        let mut id = state();
        id[1].operation = OperationId::from("auroraw.tonemap");
        assert_ne!(base, digest(&id), "an identifier");
        let mut fewer = state();
        fewer.pop();
        assert_ne!(base, digest(&fewer), "an instance fewer");
        assert_ne!(
            base,
            state_digest(2, &state()).expect("a digest"),
            "the definition version"
        );
        assert_ne!(
            digest(&[]),
            state_digest(2, &[]).expect("a digest"),
            "the definition version of an empty state"
        );
    }

    #[test]
    fn a_value_of_another_kind_is_another_value() {
        let of = |v: ParamValue| digest(&[instance("a.b", true, &[("x", v)])]);
        let all = [
            of(ParamValue::Int(1)),
            of(ParamValue::Float(1.0)),
            of(ParamValue::Enum(1)),
            of(ParamValue::Bool(true)),
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_negative_zero_is_a_zero_and_a_nan_is_refused() {
        let of = |v: f64| state_digest(1, &[instance("a.b", true, &[("x", ParamValue::Float(v))])]);
        assert_eq!(of(-0.0), of(0.0));
        assert_eq!(
            of(f64::NAN),
            Err(DigestError::Encode(EncodeError::NotANumber))
        );
    }

    #[test]
    fn a_key_stored_twice_is_refused_whatever_its_values_and_wherever_it_is() {
        let refused =
            |params: &[(&str, ParamValue)]| state_digest(1, &[instance("a.b", true, params)]);
        let twice = |key: &str| {
            Err(DigestError::DuplicateKey {
                operation: OperationId::from("a.b"),
                key: key.to_string(),
            })
        };
        // The same value twice, and two values: neither order of them is the right one.
        assert_eq!(
            refused(&[("x", ParamValue::Int(1)), ("x", ParamValue::Int(1))]),
            twice("x")
        );
        assert_eq!(
            refused(&[("x", ParamValue::Int(1)), ("x", ParamValue::Int(2))]),
            twice("x")
        );
        // Not next to each other as given: sorting brings them together.
        assert_eq!(
            refused(&[
                ("x", ParamValue::Int(1)),
                ("y", ParamValue::Int(1)),
                ("x", ParamValue::Int(2))
            ]),
            twice("x")
        );
    }

    #[test]
    fn the_text_form_reads_back_and_a_tag_this_build_does_not_know_is_kept_not_current() {
        let d = digest(&state());
        let text = d.to_string();
        assert_eq!(StateDigest::parse(&text), Ok(d));
        assert!(d.is_current());
        let hex = &text[text.find(':').unwrap() + 1..];
        // A tag below the current one (0 is a tag) is as unknown as a tag above it.
        for tag in [0, DIGEST_TAG + 1, u32::MAX] {
            let parsed = StateDigest::parse(&format!("{tag}:{hex}")).expect("any tag reads");
            assert_eq!(parsed.tag(), tag);
            assert!(!parsed.is_current(), "tag {tag}");
            assert_eq!(parsed.hash(), d.hash());
            assert_eq!(parsed.to_string(), format!("{tag}:{hex}"), "one text");
        }
    }

    #[test]
    fn a_text_that_is_not_a_digest_is_refused_for_what_is_wrong_with_it() {
        let good = digest(&state()).to_string();
        let hex = &good[2..];
        assert_eq!(StateDigest::parse(hex), Err(ParseDigestError::NoTag));
        // The tag: digits, a whole number that fits, and no other text for the same number.
        for tag in [
            "",
            "v1",
            "+1",
            "-1",
            " 1",
            "1 ",
            "99999999999",
            "01",
            "00",
            "001",
        ] {
            assert_eq!(
                StateDigest::parse(&format!("{tag}:{hex}")),
                Err(ParseDigestError::BadTag),
                "tag {tag:?}"
            );
        }
        assert!(
            StateDigest::parse(&format!("0:{hex}")).is_ok(),
            "0 is a tag"
        );
        // The hash: exactly 64 digits, lowercase, hexadecimal.
        for bad in [
            hex[1..].to_string(),
            format!("{hex}0"),
            format!("{hex}00"),
            hex.to_uppercase(),
            format!("{}g", &hex[..63]),
            String::new(),
        ] {
            assert_eq!(
                StateDigest::parse(&format!("1:{bad}")),
                Err(ParseDigestError::BadHash),
                "hash {bad:?}"
            );
        }
    }

    proptest! {
        /// Every bit of every float: two states that differ in one value have one digest exactly when the values are equal
        /// numbers (`-0.0` and `0.0` are), whatever the bit patterns.
        #[test]
        fn two_floats_have_one_digest_exactly_when_they_are_equal(a in any::<u64>(), b in any::<u64>()) {
            let (x, y) = (f64::from_bits(a), f64::from_bits(b));
            prop_assume!(!x.is_nan() && !y.is_nan());
            let of = |v: f64| digest(&[instance("a.b", true, &[("x", ParamValue::Float(v))])]);
            prop_assert_eq!(of(x) == of(y), x == y);
        }
    }
}
