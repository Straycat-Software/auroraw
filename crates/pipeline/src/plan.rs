// SPDX-License-Identifier: GPL-3.0-or-later
//! The cache key of a stage, and what a change reruns (design note 005 §2.6, note 006 §3.4).
//!
//! The output of stage *n* is cached under **the hash of the recipe up to it**: the definition version, and
//! for every stage up to *n* its identifier followed by the operations that run in it, in order, each with
//! its identifier, its version and its typed parameters in their canonical encoding. The stage identifiers
//! are in the stream so that it encodes the **partition**: which operation ran in which stage. So a change reruns **its stage and every later one**,
//! from the cache of the one before, and **nothing earlier**: the earlier keys do not contain the changed
//! operation. That is the rule of dependence made mechanical: an operation depends only on its own
//! parameters, on the operations before it, and on the image (note 006 §3.4).
//!
//! The image, the view's geometry and the quality are part of the real key too, and come with the render
//! service; this module is the recipe's part, and the model of which stages a change reruns, which the
//! stage caches of the render service must agree with. A **disabled** operation, and one the engine cannot
//! run, are left out of the hash altogether: switching an operation off gives the key of a recipe that
//! never had it, so switching it back on is a cache hit when the stage still holds it.

use crate::definition::Definition;
use crate::recipe::EncodeError;
use crate::validate::ValidRecipe;

/// The key of one stage's cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageKey {
    /// The stage's identifier.
    pub stage: &'static str,
    /// The hash of the recipe up to and including this stage.
    pub hash: [u8; 32],
}

/// The key of every stage of the definition, for a valid recipe.
///
/// `Err` only if a parameter holds a `NaN`, which has no canonical encoding.
pub fn stage_keys(recipe: &ValidRecipe) -> Result<Vec<StageKey>, EncodeError> {
    let definition = recipe.definition();
    let mut running = blake3::Hasher::new();
    running.update(b"auroraw-stage-key\0");
    running.update(&definition.version.to_le_bytes());
    let mut keys = Vec::with_capacity(definition.stages.len());
    for (index, stage) in definition.stages.iter().enumerate() {
        // The stage's identifier opens its operations **in the running hash itself**, not only in the clone
        // that is finalised: the byte stream encodes which operation ran in which stage. Without it, the
        // same operations in the same order but in another partition (a plugin that moves from `camera-rgb`
        // to `input-colour` without bumping its `op_version`) would give the later stages the same keys
        // though the operation ran in another pass. `develop` stores this hash as the proof of determinism
        // (D-140), so the byte stream is a contract.
        running.update(b"stage\0");
        running.update(&(stage.id.len() as u32).to_le_bytes());
        running.update(stage.id.as_bytes());
        for op in recipe.active().iter().filter(|op| op.stage == index) {
            let mut bytes = Vec::new();
            op.instance.encode(&mut bytes)?;
            running.update(&bytes);
        }
        keys.push(StageKey {
            stage: stage.id,
            hash: *running.clone().finalize().as_bytes(),
        });
    }
    Ok(keys)
}

/// Which stages a render must run and which it reads from the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The stages to run, in order.
    pub rerun: Vec<&'static str>,
    /// The stages whose cached output is still right.
    pub reused: Vec<&'static str>,
}

impl Plan {
    /// Whether `stage` is rerun.
    pub fn reran(&self, stage: &str) -> bool {
        self.rerun.contains(&stage)
    }
}

/// A model of the stage caches: what each stage holds the key of. The render service's caches hold the
/// buffers; this holds the keys, so that "which stages does this change rerun" is a **count** a test can
/// assert on any machine (testing strategy §4.6).
#[derive(Debug, Clone)]
pub struct CacheModel {
    definition: &'static Definition,
    held: Vec<Option<[u8; 32]>>,
}

impl CacheModel {
    /// An empty cache for `definition`: every stage runs the first time.
    pub fn new(definition: &'static Definition) -> CacheModel {
        CacheModel {
            definition,
            held: vec![None; definition.stages.len()],
        }
    }

    /// Plans a render with `keys` and records them as held: a stage is rerun when its key is not the one
    /// it holds, and then holds the new one.
    pub fn plan(&mut self, keys: &[StageKey]) -> Plan {
        assert_eq!(keys.len(), self.held.len(), "keys for another definition");
        let mut plan = Plan {
            rerun: Vec::new(),
            reused: Vec::new(),
        };
        for (slot, key) in self.held.iter_mut().zip(keys) {
            if *slot == Some(key.hash) {
                plan.reused.push(key.stage);
            } else {
                plan.rerun.push(key.stage);
                *slot = Some(key.hash);
            }
        }
        plan
    }

    /// Forgets everything held (a source that changed, a device that was lost): the next render runs every
    /// stage.
    pub fn invalidate(&mut self) {
        self.held = vec![None; self.definition.stages.len()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::V1;
    use crate::recipe::{OperationInstance, ParamValue, Recipe};
    use crate::registry::OperationRegistry;
    use crate::validate::validate;
    use auroraw_plugin_api::stages;
    use proptest::prelude::*;

    fn registry() -> OperationRegistry {
        OperationRegistry::new(&V1, vec![]).expect("the built-ins are valid")
    }

    /// A recipe of `ops` (identifier, one float parameter), in the order given.
    fn recipe(ops: &[(&str, f64)]) -> Recipe {
        Recipe {
            definition: 1,
            operations: ops
                .iter()
                .map(|(id, v)| OperationInstance::new(id, vec![ParamValue::Float(*v)]))
                .collect(),
        }
    }

    fn keys(r: &Recipe) -> Vec<StageKey> {
        stage_keys(&validate(&registry(), r).expect("valid")).expect("encodes")
    }

    const FULL: [&str; 14] = [
        "auroraw.hot-pixels",
        "auroraw.noise-reduction",
        "auroraw.white-balance",
        "auroraw.highlight-reconstruction",
        "auroraw.exposure",
        "auroraw.tone",
        "auroraw.curve",
        "auroraw.saturation-vibrance",
        "auroraw.hsl",
        "auroraw.colour-grading",
        "auroraw.crop",
        "auroraw.straighten",
        "auroraw.sharpening",
        "auroraw.tone-map",
    ];

    fn full(values: &[f64; 14]) -> Recipe {
        let ops: Vec<(&str, f64)> = FULL.iter().copied().zip(values.iter().copied()).collect();
        recipe(&ops)
    }

    // ---- the counting-cache test: a change reruns its stage and the later ones, and nothing earlier ----

    #[test]
    fn the_first_render_runs_every_stage_and_an_identical_one_runs_none() {
        let k = keys(&full(&[0.0; 14]));
        let mut cache = CacheModel::new(&V1);
        let first = cache.plan(&k);
        assert_eq!(first.rerun, stages::ALL);
        assert!(first.reused.is_empty());
        let again = cache.plan(&k);
        assert!(
            again.rerun.is_empty(),
            "nothing changed, nothing reruns: {again:?}"
        );
        assert_eq!(again.reused, stages::ALL);
    }

    #[test]
    fn a_white_balance_change_reruns_input_colour_and_what_follows_and_nothing_earlier() {
        // The price of 0.6 ms for a white balance drag (note 006 §2.1) holds only if this does.
        let mut values = [0.0; 14];
        let mut cache = CacheModel::new(&V1);
        cache.plan(&keys(&full(&values)));
        values[2] = 0.7; // auroraw.white-balance
        let plan = cache.plan(&keys(&full(&values)));
        assert_eq!(
            plan.rerun,
            [
                stages::INPUT_COLOUR,
                stages::SCENE_LINEAR,
                stages::GEOMETRY,
                stages::DETAIL,
                stages::DISPLAY
            ]
        );
        assert_eq!(
            plan.reused,
            [stages::RAW_LINEAR, stages::DEMOSAIC, stages::CAMERA_RGB]
        );
        assert!(
            !plan.reran(stages::CAMERA_RGB),
            "the denoiser is not rerun by a balance drag"
        );
    }

    #[test]
    fn a_highlight_reconstruction_change_reruns_from_input_colour_because_it_reads_the_balance() {
        // It sits after the balance in the same stage: its change reruns that stage, never the denoiser.
        let mut values = [0.0; 14];
        let mut cache = CacheModel::new(&V1);
        cache.plan(&keys(&full(&values)));
        values[3] = 1.0; // auroraw.highlight-reconstruction
        let plan = cache.plan(&keys(&full(&values)));
        assert!(
            !plan.reran(stages::CAMERA_RGB)
                && !plan.reran(stages::DEMOSAIC)
                && !plan.reran(stages::RAW_LINEAR)
        );
        assert!(plan.reran(stages::INPUT_COLOUR));
    }

    #[test]
    fn a_denoiser_change_reruns_everything_after_it_as_the_order_rule_says() {
        let mut values = [0.0; 14];
        let mut cache = CacheModel::new(&V1);
        cache.plan(&keys(&full(&values)));
        values[1] = 0.3; // auroraw.noise-reduction
        let plan = cache.plan(&keys(&full(&values)));
        assert_eq!(plan.reused, [stages::RAW_LINEAR, stages::DEMOSAIC]);
        assert_eq!(plan.rerun.len(), 6);
    }

    #[test]
    fn a_late_change_reruns_only_the_stages_from_it() {
        // Sharpening is in `detail`: only `detail` and `display` rerun.
        let mut values = [0.0; 14];
        let mut cache = CacheModel::new(&V1);
        cache.plan(&keys(&full(&values)));
        values[12] = 0.5; // auroraw.sharpening
        let plan = cache.plan(&keys(&full(&values)));
        assert_eq!(plan.rerun, [stages::DETAIL, stages::DISPLAY]);
    }

    #[test]
    fn invalidating_the_cache_reruns_every_stage() {
        let k = keys(&full(&[0.0; 14]));
        let mut cache = CacheModel::new(&V1);
        cache.plan(&k);
        cache.invalidate();
        assert_eq!(cache.plan(&k).rerun, stages::ALL);
    }

    // ---- the keys themselves ----

    #[test]
    fn two_stages_never_share_a_key_even_with_no_operation_of_their_own() {
        let k = keys(&recipe(&[]));
        let mut seen = std::collections::BTreeSet::new();
        for key in &k {
            assert!(seen.insert(key.hash), "{} shares its key", key.stage);
        }
    }

    #[test]
    fn switching_an_operation_off_gives_the_key_of_a_recipe_that_never_had_it() {
        let with = recipe(&[("auroraw.exposure", 1.0), ("auroraw.sharpening", 0.5)]);
        let mut off = with.clone();
        off.operations[0].enabled = false;
        let without = recipe(&[("auroraw.sharpening", 0.5)]);
        assert_eq!(keys(&off), keys(&without));
        assert_ne!(keys(&with), keys(&off));
        // And switching it back on is a cache hit for the stages it does not touch.
        let mut cache = CacheModel::new(&V1);
        cache.plan(&keys(&with));
        cache.plan(&keys(&off));
        let back = cache.plan(&keys(&with));
        assert!(
            back.reran(stages::SCENE_LINEAR),
            "the stage that holds it must rerun"
        );
    }

    #[test]
    fn the_parameters_of_an_operation_that_does_not_run_are_not_in_any_key() {
        let mut a = recipe(&[("auroraw.exposure", 1.0)]);
        let mut b = a.clone();
        a.operations[0].enabled = false;
        b.operations[0].enabled = false;
        b.operations[0].params = vec![ParamValue::Float(99.0)];
        assert_eq!(keys(&a), keys(&b));
    }

    #[test]
    fn the_definition_version_is_in_every_key() {
        use crate::validate::PlacedOperation;
        // The same operation, in the same stage, under two versions of the definition.
        const OTHER: Definition = Definition { version: 2, ..V1 };
        let op = |definition: &'static Definition| {
            let placed = PlacedOperation {
                instance: OperationInstance::new("auroraw.exposure", vec![ParamValue::Float(0.5)]),
                stage: definition
                    .stage_index(stages::SCENE_LINEAR)
                    .expect("a stage"),
            };
            stage_keys(&crate::validate::ValidRecipe::assume(
                definition,
                vec![placed],
            ))
            .expect("encodes")
        };
        let (one, two) = (op(&V1), op(&OTHER));
        for (a, b) in one.iter().zip(&two) {
            assert_ne!(
                a.hash, b.hash,
                "stage {} has the same key under two definition versions",
                a.stage
            );
        }
    }

    #[test]
    fn the_same_operations_in_another_partition_give_other_keys_from_where_they_differ() {
        // A plugin declared in `camera-rgb` by one registry and in `input-colour` by another: the recipe is
        // the same and both are legal, but the operation ran in another pass. The review's scenario: the
        // keys of every stage from the first that differs must differ, not only `camera-rgb`'s.
        use crate::definition::DataSpace;
        use crate::registry::OperationInfo;
        let declared_in = |stage: &str| {
            OperationRegistry::new(
                &V1,
                vec![OperationInfo {
                    id: "acme.x".into(),
                    stage: stage.to_string(),
                    after: vec![],
                    before: vec![],
                    input_space: DataSpace::CameraLinear,
                    allows_several: false,
                    versions: vec![1],
                }],
            )
            .expect("valid")
        };
        let r = recipe(&[
            ("auroraw.noise-reduction", 0.1),
            ("acme.x", 0.2),
            ("auroraw.white-balance", 0.3),
        ]);
        let key_of = |registry: &OperationRegistry| {
            stage_keys(&validate(registry, &r).expect("valid")).expect("encodes")
        };
        let (early, late) = (
            key_of(&declared_in(stages::CAMERA_RGB)),
            key_of(&declared_in(stages::INPUT_COLOUR)),
        );
        for (a, b) in early.iter().zip(&late) {
            match a.stage {
                stages::RAW_LINEAR | stages::DEMOSAIC => {
                    assert_eq!(a.hash, b.hash, "{} is before the operation", a.stage)
                }
                stage => assert_ne!(
                    a.hash, b.hash,
                    "stage {stage} has one key whichever stage the operation ran in"
                ),
            }
        }
    }

    #[test]
    fn a_nan_parameter_has_no_key() {
        let valid =
            validate(&registry(), &recipe(&[("auroraw.exposure", f64::NAN)])).expect("valid");
        assert_eq!(stage_keys(&valid), Err(EncodeError::NotANumber));
    }

    #[test]
    fn negative_zero_and_zero_give_the_same_keys() {
        assert_eq!(
            keys(&recipe(&[("auroraw.exposure", -0.0)])),
            keys(&recipe(&[("auroraw.exposure", 0.0)]))
        );
    }

    #[test]
    fn the_order_of_operations_is_in_the_key_when_a_plugin_order_is_free() {
        use crate::definition::DataSpace;
        use crate::registry::OperationInfo;
        let free = |id: &str| OperationInfo {
            id: id.into(),
            stage: stages::DETAIL.to_string(),
            after: vec![],
            before: vec![],
            input_space: DataSpace::WorkingLinear,
            allows_several: false,
            versions: vec![1],
        };
        let registry =
            OperationRegistry::new(&V1, vec![free("acme.a"), free("acme.b")]).expect("valid");
        let key_of =
            |r: Recipe| stage_keys(&validate(&registry, &r).expect("valid")).expect("encodes");
        let ab = key_of(recipe(&[("acme.a", 1.0), ("acme.b", 2.0)]));
        let ba = key_of(recipe(&[("acme.b", 2.0), ("acme.a", 1.0)]));
        assert_ne!(
            ab, ba,
            "two operations in two orders are two images and two keys"
        );
    }

    // ---- the property: a change in stage k reruns the stages from k on and none before ----

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// For any recipe made of any subset of the built-ins, changing the parameter of any one of them
        /// reruns exactly the stages from its own stage on, and never an earlier one.
        #[test]
        fn a_change_in_stage_k_reruns_the_stages_from_k_on_and_none_before(
            present in proptest::collection::vec(any::<bool>(), 14),
            values in proptest::collection::vec(-4.0_f64..4.0, 14),
            pick in 0_usize..14,
            delta in 0.001_f64..2.0,
        ) {
            let chosen: Vec<usize> = (0..14).filter(|i| present[*i]).collect();
            prop_assume!(!chosen.is_empty());
            let changed = chosen[pick % chosen.len()];
            let build = |changed_value: f64| {
                let ops: Vec<(&str, f64)> = chosen
                    .iter()
                    .map(|&i| (FULL[i], if i == changed { changed_value } else { values[i] }))
                    .collect();
                recipe(&ops)
            };
            let mut cache = CacheModel::new(&V1);
            cache.plan(&keys(&build(values[changed])));
            let plan = cache.plan(&keys(&build(values[changed] + delta)));
            let stage = V1.builtin(FULL[changed]).expect("a built-in").0;
            let expected: Vec<&str> = V1.stages[stage..].iter().map(|s| s.id).collect();
            prop_assert_eq!(plan.rerun, expected);
        }
    }
}
