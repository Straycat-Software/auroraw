// SPDX-License-Identifier: GPL-3.0-or-later
//! Checking a recipe before it is rendered (design note 006 §3.5). `develop` **places** the operations;
//! the pipeline **validates** what it is given and refuses, with a typed error, a recipe that cannot be
//! rendered as written. Placement rules live in one place, and the pipeline is tested with hand-written
//! recipes.
//!
//! What makes a recipe invalid:
//!
//! 1. an operation the registry does not know, while enabled (a disabled one is a missing plugin, kept);
//! 2. the stages out of the definition's order, or an operation in a stage that is not its own;
//! 3. within a stage, a built-in operation out of its canonical order, or a constraint that does not hold;
//! 4. an operation that appears twice where its declaration allows one.
//!
//! Two more checks of §3.5 are made **when the registry is loaded**, because they are properties of a
//! declaration and not of a recipe: a stage the definition does not have, and a data space the stage does
//! not deliver ([`crate::registry`]).
//!
//! What makes an operation **inert** and not an error: it is disabled, or its `op_version` names no
//! implementation the registry holds. It is skipped, listed in [`ValidRecipe::inert`] so that the report
//! can say so, and its parameters are kept (architecture §7.2).
//!
//! Legality is not placement: a recipe in which an operation without constraints sits before a built-in is
//! legal, and `develop` does not produce it (an unconstrained plugin goes at the end of its stage).
//!
//! **Parameters are not checked here.** A recipe with three floats for a two-parameter operation is valid
//! as far as this module goes, and the only guard on a parameter is that `NaN` has no canonical encoding,
//! at hashing. Checking a value against the type, the limits and the count a declaration gives needs
//! `ParamSpec`, which comes with work package 13 (D-142, layer 1): that is where the check will live.
//!
//! **An operation that may appear several times** (`allows_several`): a constraint is checked against
//! **every** copy of the operation it names, so `after: [X]` needs every copy of `X` before it and
//! `before: [X]` every copy after.

use std::collections::BTreeMap;

use crate::definition::{Definition, by_version};
use crate::recipe::{OperationId, OperationInstance, Recipe};
use crate::registry::OperationRegistry;

/// Which side of another operation a constraint puts an operation on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// The operation must come after the other.
    After,
    /// The operation must come before the other.
    Before,
}

/// Why a recipe cannot be rendered as written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecipeError {
    /// The recipe names a definition version the engine does not know.
    #[error(
        "the recipe is written against pipeline definition {0}, which this engine does not know"
    )]
    UnknownDefinition(u32),
    /// The registry was loaded against another definition than the recipe's.
    #[error(
        "the recipe is written against pipeline definition {recipe} but the registry was loaded against {registry}"
    )]
    DefinitionMismatch {
        /// The recipe's version.
        recipe: u32,
        /// The registry's.
        registry: u32,
    },
    /// An enabled operation that nobody declared.
    #[error("the operation {operation} is enabled but is not in the registry")]
    UnknownOperation {
        /// The operation.
        operation: OperationId,
    },
    /// An operation whose stage comes before the stage of an operation listed earlier.
    #[error(
        "the operation {operation} belongs to the stage {stage}, which comes before {after_stage}, where an earlier operation already is"
    )]
    OutOfStageOrder {
        /// The operation.
        operation: OperationId,
        /// Its stage.
        stage: String,
        /// The later stage an earlier operation is in.
        after_stage: String,
    },
    /// A built-in operation before one that comes before it in the stage's canonical order.
    #[error(
        "in the stage {stage}, {operation} is listed before {must_come_after}, which comes first in the canonical order"
    )]
    BreaksCanonicalOrder {
        /// The stage.
        stage: String,
        /// The operation listed too early.
        operation: OperationId,
        /// The built-in operation it must follow.
        must_come_after: OperationId,
    },
    /// A constraint of a declaration that does not hold in the recipe's order.
    #[error("{operation} must come {} {other}, and does not", match constraint { Constraint::After => "after", Constraint::Before => "before" })]
    ConstraintViolated {
        /// The operation that declared the constraint.
        operation: OperationId,
        /// Which side.
        constraint: Constraint,
        /// The operation it names.
        other: OperationId,
    },
    /// An operation listed twice that its declaration allows once.
    #[error("the operation {operation} is listed more than once")]
    Duplicate {
        /// The operation.
        operation: OperationId,
    },
}

/// Why an operation is in the recipe but will not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InertReason {
    /// It is disabled (by the person, or because its plugin is missing).
    Disabled,
    /// It is not in the registry, and disabled: its plugin is missing and its settings are kept.
    MissingOperation,
    /// The registry holds the operation but not this version of it.
    UnknownVersion(u32),
}

/// An operation that is in the recipe and will not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inert {
    /// The operation.
    pub operation: OperationId,
    /// Why.
    pub reason: InertReason,
}

/// An operation that will run, with the position of its stage in the definition.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedOperation {
    /// The operation as the recipe has it.
    pub instance: OperationInstance,
    /// The position of its stage in the definition.
    pub stage: usize,
}

/// A recipe that has been checked: the operations that will run, in order, each with its stage, and the
/// ones that will not.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidRecipe {
    definition: &'static Definition,
    active: Vec<PlacedOperation>,
    inert: Vec<Inert>,
}

impl ValidRecipe {
    /// A valid recipe for any definition, built by hand: for tests of what depends on the definition (the
    /// registry and `by_version` only know version 1).
    #[cfg(test)]
    pub(crate) fn assume(
        definition: &'static Definition,
        active: Vec<PlacedOperation>,
    ) -> ValidRecipe {
        ValidRecipe {
            definition,
            active,
            inert: Vec::new(),
        }
    }

    /// The definition the recipe is rendered with.
    pub fn definition(&self) -> &'static Definition {
        self.definition
    }

    /// The operations that will run, in order.
    pub fn active(&self) -> &[PlacedOperation] {
        &self.active
    }

    /// The operations that are in the recipe and will not run, in the recipe's order.
    pub fn inert(&self) -> &[Inert] {
        &self.inert
    }
}

/// Checks `recipe` against `registry` and returns it with each operation's stage, or **every** reason it
/// cannot be rendered as written.
pub fn validate(
    registry: &OperationRegistry,
    recipe: &Recipe,
) -> Result<ValidRecipe, Vec<RecipeError>> {
    let Some(definition) = by_version(recipe.definition) else {
        return Err(vec![RecipeError::UnknownDefinition(recipe.definition)]);
    };
    if registry.definition().version != definition.version {
        return Err(vec![RecipeError::DefinitionMismatch {
            recipe: definition.version,
            registry: registry.definition().version,
        }]);
    }
    let mut errors = Vec::new();
    let mut active: Vec<PlacedOperation> = Vec::new();
    let mut inert = Vec::new();

    // What runs, what does not, and what cannot be said at all.
    for instance in &recipe.operations {
        match registry.get(&instance.operation) {
            None if instance.enabled => errors.push(RecipeError::UnknownOperation {
                operation: instance.operation.clone(),
            }),
            None => inert.push(Inert {
                operation: instance.operation.clone(),
                reason: InertReason::MissingOperation,
            }),
            Some(_) if !instance.enabled => inert.push(Inert {
                operation: instance.operation.clone(),
                reason: InertReason::Disabled,
            }),
            Some(entry) if !entry.info.versions.contains(&instance.op_version) => {
                inert.push(Inert {
                    operation: instance.operation.clone(),
                    reason: InertReason::UnknownVersion(instance.op_version),
                })
            }
            Some(entry) => active.push(PlacedOperation {
                instance: instance.clone(),
                stage: entry.stage,
            }),
        }
    }

    // An operation listed twice that its declaration allows once.
    let mut count: BTreeMap<&OperationId, usize> = BTreeMap::new();
    for op in &active {
        *count.entry(&op.instance.operation).or_default() += 1;
    }
    for (id, n) in &count {
        let allows_several = registry.get(id).is_some_and(|e| e.info.allows_several);
        if *n > 1 && !allows_several {
            errors.push(RecipeError::Duplicate {
                operation: (*id).clone(),
            });
        }
    }

    // The stages in the definition's order.
    let mut latest: Option<usize> = None;
    for op in &active {
        match latest {
            Some(stage) if op.stage < stage => errors.push(RecipeError::OutOfStageOrder {
                operation: op.instance.operation.clone(),
                stage: definition.stages[op.stage].id.to_string(),
                after_stage: definition.stages[stage].id.to_string(),
            }),
            _ => latest = Some(op.stage),
        }
    }

    // Within a stage, the canonical order of the built-ins.
    for (stage_index, stage) in definition.stages.iter().enumerate() {
        // The built-in with the highest rank listed so far in this stage.
        let mut highest: Option<(usize, &OperationId)> = None;
        for op in active.iter().filter(|op| op.stage == stage_index) {
            let Some(rank) = registry
                .get(&op.instance.operation)
                .and_then(|e| e.canonical_rank)
            else {
                continue;
            };
            match highest {
                Some((top, top_id)) if rank < top => {
                    errors.push(RecipeError::BreaksCanonicalOrder {
                        stage: stage.id.to_string(),
                        operation: op.instance.operation.clone(),
                        must_come_after: top_id.clone(),
                    })
                }
                _ => highest = Some((rank, &op.instance.operation)),
            }
        }
    }

    // The constraints of the declarations, between operations that are both there: against every copy of
    // the operation a constraint names.
    let mut positions: BTreeMap<&OperationId, Vec<usize>> = BTreeMap::new();
    for (i, op) in active.iter().enumerate() {
        positions.entry(&op.instance.operation).or_default().push(i);
    }
    for (here, op) in active.iter().enumerate() {
        let Some(entry) = registry.get(&op.instance.operation) else {
            continue;
        };
        for other in &entry.info.after {
            if positions
                .get(other)
                .is_some_and(|copies| copies.iter().any(|&there| there > here))
            {
                errors.push(RecipeError::ConstraintViolated {
                    operation: op.instance.operation.clone(),
                    constraint: Constraint::After,
                    other: other.clone(),
                });
            }
        }
        for other in &entry.info.before {
            if positions
                .get(other)
                .is_some_and(|copies| copies.iter().any(|&there| there < here))
            {
                errors.push(RecipeError::ConstraintViolated {
                    operation: op.instance.operation.clone(),
                    constraint: Constraint::Before,
                    other: other.clone(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(ValidRecipe {
            definition,
            active,
            inert,
        })
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::{DataSpace, V1};
    use crate::recipe::ParamValue;
    use crate::registry::OperationInfo;
    use auroraw_plugin_api::stages;

    pub(crate) fn registry() -> OperationRegistry {
        OperationRegistry::new(&V1, vec![]).expect("the built-ins are valid")
    }

    pub(crate) fn recipe(ops: &[&str]) -> Recipe {
        Recipe {
            definition: 1,
            operations: ops
                .iter()
                .map(|id| OperationInstance::new(id, vec![ParamValue::Float(0.0)]))
                .collect(),
        }
    }

    fn plugin(
        id: &str,
        stage: &str,
        space: DataSpace,
        after: &[&str],
        before: &[&str],
    ) -> OperationInfo {
        OperationInfo {
            id: OperationId::from(id),
            stage: stage.to_string(),
            after: after.iter().map(|s| OperationId::from(*s)).collect(),
            before: before.iter().map(|s| OperationId::from(*s)).collect(),
            input_space: space,
            allows_several: false,
            versions: vec![1],
        }
    }

    #[test]
    fn an_empty_recipe_is_valid_and_renders_the_flat_default() {
        let valid = validate(&registry(), &recipe(&[])).expect("valid");
        assert!(valid.active().is_empty() && valid.inert().is_empty());
        assert_eq!(valid.definition().version, 1);
    }

    #[test]
    fn a_recipe_in_canonical_order_is_valid_and_each_operation_knows_its_stage() {
        let r = recipe(&[
            "auroraw.noise-reduction",
            "auroraw.white-balance",
            "auroraw.highlight-reconstruction",
            "auroraw.exposure",
            "auroraw.crop",
            "auroraw.sharpening",
            "auroraw.tone-map",
        ]);
        let valid = validate(&registry(), &r).expect("valid");
        let stages: Vec<&str> = valid
            .active()
            .iter()
            .map(|op| V1.stages[op.stage].id)
            .collect();
        assert_eq!(
            stages,
            [
                stages::CAMERA_RGB,
                stages::INPUT_COLOUR,
                stages::INPUT_COLOUR,
                stages::SCENE_LINEAR,
                stages::GEOMETRY,
                stages::DETAIL,
                stages::DISPLAY
            ]
        );
    }

    #[test]
    fn an_unknown_definition_is_refused() {
        let mut r = recipe(&[]);
        r.definition = 99;
        assert_eq!(
            validate(&registry(), &r),
            Err(vec![RecipeError::UnknownDefinition(99)])
        );
    }

    #[test]
    fn an_enabled_operation_nobody_declared_is_an_error_and_a_disabled_one_is_a_missing_plugin() {
        let mut r = recipe(&["auroraw.exposure", "acme.dehaze"]);
        let errors = validate(&registry(), &r).expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::UnknownOperation {
                operation: OperationId::from("acme.dehaze")
            }]
        );
        // The same operation disabled, as `develop` hands a recipe whose plugin is missing: kept, marked.
        r.operations[1].enabled = false;
        let valid = validate(&registry(), &r).expect("valid");
        assert_eq!(valid.active().len(), 1);
        assert_eq!(
            valid.inert(),
            [Inert {
                operation: OperationId::from("acme.dehaze"),
                reason: InertReason::MissingOperation
            }]
        );
    }

    #[test]
    fn a_disabled_known_operation_and_an_unknown_version_are_inert_not_errors() {
        let mut r = recipe(&["auroraw.exposure", "auroraw.tone", "auroraw.sharpening"]);
        r.operations[0].enabled = false;
        r.operations[1].op_version = 7;
        let valid = validate(&registry(), &r).expect("valid");
        assert_eq!(valid.active().len(), 1, "only sharpening runs");
        assert_eq!(
            valid.inert(),
            [
                Inert {
                    operation: OperationId::from("auroraw.exposure"),
                    reason: InertReason::Disabled
                },
                Inert {
                    operation: OperationId::from("auroraw.tone"),
                    reason: InertReason::UnknownVersion(7)
                },
            ]
        );
    }

    #[test]
    fn stages_out_of_the_definitions_order_are_refused() {
        // Sharpening (detail) before exposure (scene-linear): geometry and detail come after.
        let errors = validate(
            &registry(),
            &recipe(&["auroraw.sharpening", "auroraw.exposure"]),
        )
        .expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::OutOfStageOrder {
                operation: OperationId::from("auroraw.exposure"),
                stage: stages::SCENE_LINEAR.to_string(),
                after_stage: stages::DETAIL.to_string(),
            }]
        );
    }

    #[test]
    fn highlight_reconstruction_before_white_balance_breaks_the_canonical_order() {
        // The rule of dependence as a check: the reconstruction reads the balance, so it comes after it.
        let errors = validate(
            &registry(),
            &recipe(&["auroraw.highlight-reconstruction", "auroraw.white-balance"]),
        )
        .expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::BreaksCanonicalOrder {
                stage: stages::INPUT_COLOUR.to_string(),
                operation: OperationId::from("auroraw.white-balance"),
                must_come_after: OperationId::from("auroraw.highlight-reconstruction"),
            }]
        );
    }

    #[test]
    fn white_balance_before_the_denoiser_is_out_of_the_definitions_order() {
        // The plan's original placement is not expressible: it is a different definition.
        let errors = validate(
            &registry(),
            &recipe(&["auroraw.white-balance", "auroraw.noise-reduction"]),
        )
        .expect_err("refused");
        assert!(
            matches!(errors.as_slice(), [RecipeError::OutOfStageOrder { stage, after_stage, .. }]
            if stage == stages::CAMERA_RGB && after_stage == stages::INPUT_COLOUR),
            "{errors:?}"
        );
    }

    #[test]
    fn an_operation_listed_twice_is_refused_unless_it_allows_several() {
        let errors = validate(
            &registry(),
            &recipe(&["auroraw.exposure", "auroraw.exposure"]),
        )
        .expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::Duplicate {
                operation: OperationId::from("auroraw.exposure")
            }]
        );

        let mut several = plugin(
            "acme.grain",
            stages::DETAIL,
            DataSpace::WorkingLinear,
            &[],
            &[],
        );
        several.allows_several = true;
        let registry = OperationRegistry::new(&V1, vec![several]).expect("valid");
        assert!(validate(&registry, &recipe(&["acme.grain", "acme.grain"])).is_ok());
    }

    #[test]
    fn a_plugins_constraints_hold_only_between_operations_that_are_both_there() {
        let p = plugin(
            "acme.x",
            stages::INPUT_COLOUR,
            DataSpace::CameraLinear,
            &["auroraw.white-balance"],
            &["auroraw.highlight-reconstruction"],
        );
        let registry = OperationRegistry::new(&V1, vec![p]).expect("valid");
        // Between the two built-ins: both constraints hold.
        let fine = recipe(&[
            "auroraw.white-balance",
            "acme.x",
            "auroraw.highlight-reconstruction",
        ]);
        assert!(validate(&registry, &fine).is_ok());
        // Before the balance: the `after` constraint fails.
        let early = recipe(&[
            "acme.x",
            "auroraw.white-balance",
            "auroraw.highlight-reconstruction",
        ]);
        let errors = validate(&registry, &early).expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::ConstraintViolated {
                operation: OperationId::from("acme.x"),
                constraint: Constraint::After,
                other: OperationId::from("auroraw.white-balance"),
            }]
        );
        // After the reconstruction: the `before` constraint fails.
        let late = recipe(&[
            "auroraw.white-balance",
            "auroraw.highlight-reconstruction",
            "acme.x",
        ]);
        assert!(matches!(
            validate(&registry, &late).expect_err("refused").as_slice(),
            [RecipeError::ConstraintViolated {
                constraint: Constraint::Before,
                ..
            }]
        ));
        // The named operations absent: nothing to be relative to, so nothing fails.
        assert!(validate(&registry, &recipe(&["acme.x"])).is_ok());
    }

    #[test]
    fn a_constraint_is_checked_against_every_copy_of_an_operation_that_may_appear_several_times() {
        let mut grain = plugin(
            "acme.grain",
            stages::DETAIL,
            DataSpace::WorkingLinear,
            &[],
            &[],
        );
        grain.allows_several = true;
        let after_all = plugin(
            "acme.y",
            stages::DETAIL,
            DataSpace::WorkingLinear,
            &["acme.grain"],
            &[],
        );
        let before_all = plugin(
            "acme.z",
            stages::DETAIL,
            DataSpace::WorkingLinear,
            &[],
            &["acme.grain"],
        );
        let registry =
            OperationRegistry::new(&V1, vec![grain, after_all, before_all]).expect("valid");

        // `after`: every copy before it. [grain, y, grain] has one after it: refused. (For `after` checking
        // the last copy is the same as checking every one, since the last is the latest.)
        assert!(validate(&registry, &recipe(&["acme.grain", "acme.grain", "acme.y"])).is_ok());
        let errors = validate(&registry, &recipe(&["acme.grain", "acme.y", "acme.grain"]))
            .expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::ConstraintViolated {
                operation: OperationId::from("acme.y"),
                constraint: Constraint::After,
                other: OperationId::from("acme.grain"),
            }]
        );

        // `before`: every copy after it. [grain, z, grain] has one before it: refused. Here the first copy
        // is the one that decides, and a check against the last copy alone would have let this through.
        assert!(validate(&registry, &recipe(&["acme.z", "acme.grain", "acme.grain"])).is_ok());
        let errors = validate(&registry, &recipe(&["acme.grain", "acme.z", "acme.grain"]))
            .expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::ConstraintViolated {
                operation: OperationId::from("acme.z"),
                constraint: Constraint::Before,
                other: OperationId::from("acme.grain"),
            }]
        );
    }

    #[test]
    fn every_problem_is_reported_together() {
        // Sharpening first, exposure twice, and an operation nobody declared.
        let r = recipe(&[
            "auroraw.sharpening",
            "auroraw.exposure",
            "auroraw.exposure",
            "acme.nope",
        ]);
        let errors = validate(&registry(), &r).expect_err("refused");
        let exposure = OperationId::from("auroraw.exposure");
        let out_of_order = RecipeError::OutOfStageOrder {
            operation: exposure.clone(),
            stage: stages::SCENE_LINEAR.to_string(),
            after_stage: stages::DETAIL.to_string(),
        };
        assert_eq!(
            errors,
            vec![
                RecipeError::UnknownOperation {
                    operation: OperationId::from("acme.nope")
                },
                RecipeError::Duplicate {
                    operation: exposure
                },
                // Each of the two exposures comes after sharpening, which is a later stage.
                out_of_order.clone(),
                out_of_order,
            ]
        );
    }

    #[test]
    fn a_recipe_for_a_registry_of_another_definition_is_refused() {
        // A registry can only be loaded against a definition the engine has; a version mismatch is the
        // case of a registry kept across an upgrade. Simulated with a definition of another version.
        const OTHER: crate::definition::Definition =
            crate::definition::Definition { version: 2, ..V1 };
        let registry = OperationRegistry::new(&OTHER, vec![]).expect("valid");
        let errors = validate(&registry, &recipe(&[])).expect_err("refused");
        assert_eq!(
            errors,
            vec![RecipeError::DefinitionMismatch {
                recipe: 1,
                registry: 2
            }]
        );
    }
}
