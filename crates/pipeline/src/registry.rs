// SPDX-License-Identifier: GPL-3.0-or-later
//! The operations the engine knows: the built-in ones, which the definition lists, and the ones plugins
//! declare. The engine fills the registry and hands it to the pipeline; **the pipeline never loads a
//! plugin** (D-140), which keeps `pipeline` off `plugin-host`.
//!
//! A registry is **checked when it is loaded**, never silently repaired (design note 006 §3.4): a
//! declaration that names no stage the definition has, that reads a data space its stage does not deliver,
//! that constrains itself, that names an operation of another stage, or whose constraints cannot all hold
//! with the canonical order, is refused with the reason, and the rest of the registry stays valid.
//!
//! **A stand-in for the declaration.** What an operation declares (its stage, its placement, its input data
//! space, its versions) is layer 1 of D-142, in `plugin-api`, with work package 13. [`OperationInfo`] is the
//! part of it the pipeline reads, and is built from the declaration when that arrives.

use std::collections::{BTreeMap, BTreeSet};

use crate::definition::{DataSpace, Definition};
use crate::recipe::OperationId;

/// What the pipeline needs to know of an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationInfo {
    /// The operation's identifier.
    pub id: OperationId,
    /// The stage it runs in (a stage identifier of the definition).
    pub stage: String,
    /// Operations of the same stage it must come after, **if they are in the recipe**. Naming an operation
    /// that is not there is not an error: the constraint is relative and optional (note 006 §3.4).
    pub after: Vec<OperationId>,
    /// Operations of the same stage it must come before, if they are in the recipe.
    pub before: Vec<OperationId>,
    /// The data space it reads and returns, which its stage must deliver to it.
    pub input_space: DataSpace,
    /// Whether a recipe may hold it more than once.
    pub allows_several: bool,
    /// The versions of the operation the engine can run. A recipe that asks for another is not an error:
    /// the operation is skipped, disabled and marked (architecture §7.2).
    pub versions: Vec<u32>,
}

/// Why a declaration was refused when the registry was loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two operations have the same identifier (a plugin may not take a built-in's).
    #[error("the operation {operation} is declared twice")]
    DuplicateId {
        /// The identifier.
        operation: OperationId,
    },
    /// The declared stage is not one of the definition's.
    #[error(
        "the operation {operation} names the stage {stage:?}, which the definition does not have"
    )]
    UnknownStage {
        /// The operation.
        operation: OperationId,
        /// The stage it named.
        stage: String,
    },
    /// The data space the operation reads is not the one its stage delivers to operations.
    #[error(
        "the operation {operation} reads {declared} but the stage {stage} delivers {expected} to its operations"
    )]
    WrongSpace {
        /// The operation.
        operation: OperationId,
        /// Its stage.
        stage: String,
        /// The space it declared.
        declared: DataSpace,
        /// The space its stage delivers.
        expected: DataSpace,
    },
    /// A constraint that names the operation itself.
    #[error("the operation {operation} is constrained relative to itself")]
    SelfConstraint {
        /// The operation.
        operation: OperationId,
    },
    /// A constraint that names an operation of another stage: no constraint can move an operation out of
    /// its stage, and one that tries is an error, not silently ignored.
    #[error(
        "the operation {operation} (stage {stage}) is constrained relative to {other}, which is in the stage {other_stage}"
    )]
    ConstraintAcrossStages {
        /// The operation that declared the constraint.
        operation: OperationId,
        /// Its stage.
        stage: String,
        /// The operation it names.
        other: OperationId,
        /// That operation's stage.
        other_stage: String,
    },
    /// Constraints that cannot all hold together with the canonical order of the stage.
    #[error("the constraints of the stage {stage} cannot all hold: {} are in a cycle", operations.iter().map(OperationId::as_str).collect::<Vec<_>>().join(", "))]
    ConstraintCycle {
        /// The stage.
        stage: String,
        /// The operations in the cycle, sorted.
        operations: Vec<OperationId>,
    },
}

/// An operation as the registry holds it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) info: OperationInfo,
    /// The position of its stage in the definition.
    pub(crate) stage: usize,
    /// Its place in its stage's canonical order, for a built-in; `None` for a plugin's.
    pub(crate) canonical_rank: Option<usize>,
}

/// The operations the pipeline can place and validate.
#[derive(Debug, Clone)]
pub struct OperationRegistry {
    definition: &'static Definition,
    entries: BTreeMap<OperationId, Entry>,
}

impl OperationRegistry {
    /// The built-in operations of `definition` (version 1 of each) and `external` declarations (a
    /// plugin's), checked together.
    ///
    /// Returns every problem found, so that one bad declaration does not hide another.
    pub fn new(
        definition: &'static Definition,
        external: Vec<OperationInfo>,
    ) -> Result<OperationRegistry, Vec<RegistryError>> {
        let mut errors = Vec::new();
        let mut entries: BTreeMap<OperationId, Entry> = BTreeMap::new();

        for (stage_index, stage) in definition.stages.iter().enumerate() {
            for (rank, id) in stage.operations.iter().enumerate() {
                let info = OperationInfo {
                    id: OperationId::from(*id),
                    stage: stage.id.to_string(),
                    after: Vec::new(),
                    before: Vec::new(),
                    input_space: stage.operations_read,
                    allows_several: false,
                    versions: vec![1],
                };
                entries.insert(
                    info.id.clone(),
                    Entry {
                        info,
                        stage: stage_index,
                        canonical_rank: Some(rank),
                    },
                );
            }
        }

        for info in external {
            if entries.contains_key(&info.id) {
                errors.push(RegistryError::DuplicateId { operation: info.id });
                continue;
            }
            let Some(stage_index) = definition.stage_index(&info.stage) else {
                errors.push(RegistryError::UnknownStage {
                    operation: info.id,
                    stage: info.stage,
                });
                continue;
            };
            let expected = definition.stages[stage_index].operations_read;
            if info.input_space != expected {
                errors.push(RegistryError::WrongSpace {
                    operation: info.id,
                    stage: info.stage,
                    declared: info.input_space,
                    expected,
                });
                continue;
            }
            entries.insert(
                info.id.clone(),
                Entry {
                    info,
                    stage: stage_index,
                    canonical_rank: None,
                },
            );
        }

        // Constraints: each names operations of its own stage, and not itself.
        let mut refused = BTreeSet::new();
        for (id, entry) in &entries {
            for other_id in entry.info.after.iter().chain(&entry.info.before) {
                if other_id == id {
                    errors.push(RegistryError::SelfConstraint {
                        operation: id.clone(),
                    });
                    refused.insert(id.clone());
                } else if let Some(other) = entries.get(other_id)
                    && other.stage != entry.stage
                {
                    errors.push(RegistryError::ConstraintAcrossStages {
                        operation: id.clone(),
                        stage: entry.info.stage.clone(),
                        other: other_id.clone(),
                        other_stage: other.info.stage.clone(),
                    });
                    refused.insert(id.clone());
                }
            }
        }
        for id in &refused {
            entries.remove(id);
        }

        // Constraints that cannot all hold together with the canonical order.
        for (stage_index, stage) in definition.stages.iter().enumerate() {
            if let Some(cycle) = find_cycle(&entries, stage_index) {
                errors.push(RegistryError::ConstraintCycle {
                    stage: stage.id.to_string(),
                    operations: cycle,
                });
            }
        }

        if errors.is_empty() {
            Ok(OperationRegistry {
                definition,
                entries,
            })
        } else {
            Err(errors)
        }
    }

    /// The definition the registry was loaded against.
    pub fn definition(&self) -> &'static Definition {
        self.definition
    }

    /// Whether the registry knows an operation.
    pub fn contains(&self, id: &OperationId) -> bool {
        self.entries.contains_key(id)
    }

    pub(crate) fn get(&self, id: &OperationId) -> Option<&Entry> {
        self.entries.get(id)
    }
}

/// The operations of a stage that cannot be put in any order that respects the canonical order and
/// every constraint, or `None` if there is an order. Kahn's algorithm: what is left when no operation is
/// free of predecessors is the cycle (and what hangs off it).
fn find_cycle(entries: &BTreeMap<OperationId, Entry>, stage: usize) -> Option<Vec<OperationId>> {
    let members: Vec<&OperationId> = entries
        .iter()
        .filter(|(_, e)| e.stage == stage)
        .map(|(id, _)| id)
        .collect();
    // `edges` holds `(first, second)`: first must run before second.
    let mut edges: BTreeSet<(&OperationId, &OperationId)> = BTreeSet::new();
    let mut canonical: Vec<(usize, &OperationId)> = members
        .iter()
        .filter_map(|id| entries[*id].canonical_rank.map(|rank| (rank, *id)))
        .collect();
    canonical.sort();
    for pair in canonical.windows(2) {
        edges.insert((pair[0].1, pair[1].1));
    }
    for id in &members {
        let info = &entries[*id].info;
        for other in &info.after {
            if let Some((key, _)) = entries
                .get_key_value(other)
                .filter(|(_, e)| e.stage == stage)
            {
                edges.insert((key, id));
            }
        }
        for other in &info.before {
            if let Some((key, _)) = entries
                .get_key_value(other)
                .filter(|(_, e)| e.stage == stage)
            {
                edges.insert((id, key));
            }
        }
    }
    let mut remaining: BTreeSet<&OperationId> = members.into_iter().collect();
    loop {
        let free: Vec<&OperationId> = remaining
            .iter()
            .copied()
            .filter(|id| {
                !edges
                    .iter()
                    .any(|(first, second)| second == id && remaining.contains(first))
            })
            .collect();
        if free.is_empty() {
            break;
        }
        for id in free {
            remaining.remove(id);
        }
    }
    if remaining.is_empty() {
        None
    } else {
        Some(remaining.into_iter().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::{V1, names};

    fn plugin(id: &str, stage: &str, space: DataSpace) -> OperationInfo {
        OperationInfo {
            id: OperationId::from(id),
            stage: stage.to_string(),
            after: vec![],
            before: vec![],
            input_space: space,
            allows_several: false,
            versions: vec![1],
        }
    }

    fn ids(list: &[&str]) -> Vec<OperationId> {
        list.iter().map(|s| OperationId::from(*s)).collect()
    }

    #[test]
    fn the_builtins_alone_form_a_valid_registry() {
        let registry = OperationRegistry::new(&V1, vec![]).expect("the built-ins are valid");
        assert!(registry.contains(&OperationId::from("auroraw.white-balance")));
        assert!(!registry.contains(&OperationId::from("acme.dehaze")));
        let wb = registry
            .get(&OperationId::from("auroraw.white-balance"))
            .expect("known");
        assert_eq!(wb.info.stage, names::INPUT_COLOUR);
        assert_eq!(wb.info.input_space, DataSpace::CameraLinear);
        assert_eq!(wb.canonical_rank, Some(0));
    }

    #[test]
    fn a_plugin_in_a_stage_that_reads_its_space_is_accepted() {
        let p = plugin("acme.dehaze", names::SCENE_LINEAR, DataSpace::WorkingLinear);
        assert!(OperationRegistry::new(&V1, vec![p]).is_ok());
    }

    #[test]
    fn a_plugin_may_not_take_a_builtins_identifier() {
        let p = plugin(
            "auroraw.exposure",
            names::SCENE_LINEAR,
            DataSpace::WorkingLinear,
        );
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert_eq!(
            errors,
            vec![RegistryError::DuplicateId {
                operation: OperationId::from("auroraw.exposure")
            }]
        );
        // Nor two plugins one another's.
        let a = plugin("acme.x", names::DETAIL, DataSpace::WorkingLinear);
        let errors = OperationRegistry::new(&V1, vec![a.clone(), a]).expect_err("refused");
        assert!(
            matches!(errors.as_slice(), [RegistryError::DuplicateId { .. }]),
            "{errors:?}"
        );
    }

    #[test]
    fn an_unknown_stage_is_refused_and_a_stage_name_is_not_a_space() {
        let p = plugin("acme.x", "camera-colour", DataSpace::CameraLinear);
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert!(
            matches!(errors.as_slice(), [RegistryError::UnknownStage { stage, .. }] if stage == "camera-colour"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_working_space_operation_in_a_camera_stage_is_refused() {
        // Saturation is wrong on camera primaries: the declaration's space is what lets the pipeline say so.
        let p = plugin("acme.saturate", names::CAMERA_RGB, DataSpace::WorkingLinear);
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert_eq!(
            errors,
            vec![RegistryError::WrongSpace {
                operation: OperationId::from("acme.saturate"),
                stage: names::CAMERA_RGB.to_string(),
                declared: DataSpace::WorkingLinear,
                expected: DataSpace::CameraLinear,
            }]
        );
    }

    #[test]
    fn the_levels_stage_delivers_mosaic_values_to_its_operations() {
        // `raw-linear` receives counts, but its operations read what the levels turned them into.
        let ok = plugin("acme.stuck", names::RAW_LINEAR, DataSpace::MosaicLinear);
        assert!(OperationRegistry::new(&V1, vec![ok]).is_ok());
        let bad = plugin("acme.stuck", names::RAW_LINEAR, DataSpace::SensorRaw);
        assert!(OperationRegistry::new(&V1, vec![bad]).is_err());
    }

    #[test]
    fn a_constraint_naming_the_operation_itself_is_refused() {
        let mut p = plugin("acme.x", names::DETAIL, DataSpace::WorkingLinear);
        p.after = ids(&["acme.x"]);
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert_eq!(
            errors,
            vec![RegistryError::SelfConstraint {
                operation: OperationId::from("acme.x")
            }]
        );
    }

    #[test]
    fn a_constraint_naming_an_operation_of_another_stage_is_an_error_not_ignored() {
        // Sharpening is in `detail`; a plugin of `scene-linear` that asks to come after it.
        let mut p = plugin("acme.x", names::SCENE_LINEAR, DataSpace::WorkingLinear);
        p.after = ids(&["auroraw.sharpening"]);
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert_eq!(
            errors,
            vec![RegistryError::ConstraintAcrossStages {
                operation: OperationId::from("acme.x"),
                stage: names::SCENE_LINEAR.to_string(),
                other: OperationId::from("auroraw.sharpening"),
                other_stage: names::DETAIL.to_string(),
            }]
        );
    }

    #[test]
    fn a_constraint_naming_an_operation_nobody_declared_is_fine_because_it_is_relative() {
        // A plugin that wants to come after another plugin that is not installed.
        let mut p = plugin("acme.x", names::DETAIL, DataSpace::WorkingLinear);
        p.after = ids(&["other.not-installed"]);
        assert!(OperationRegistry::new(&V1, vec![p]).is_ok());
    }

    #[test]
    fn constraints_that_contradict_the_canonical_order_are_a_cycle() {
        // White balance is before highlight reconstruction in the canonical order of `input-colour`; a
        // plugin that must come before the balance and after the reconstruction cannot be placed.
        let mut p = plugin("acme.x", names::INPUT_COLOUR, DataSpace::CameraLinear);
        p.before = ids(&["auroraw.white-balance"]);
        p.after = ids(&["auroraw.highlight-reconstruction"]);
        let errors = OperationRegistry::new(&V1, vec![p]).expect_err("refused");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let RegistryError::ConstraintCycle { stage, operations } = &errors[0] else {
            panic!("expected a cycle, got {errors:?}");
        };
        assert_eq!(stage, names::INPUT_COLOUR);
        assert_eq!(
            operations,
            &ids(&[
                "acme.x",
                "auroraw.highlight-reconstruction",
                "auroraw.white-balance"
            ])
        );
    }

    #[test]
    fn two_plugins_that_each_want_to_be_first_form_a_cycle() {
        let mut a = plugin("acme.a", names::DETAIL, DataSpace::WorkingLinear);
        let mut b = plugin("acme.b", names::DETAIL, DataSpace::WorkingLinear);
        a.before = ids(&["acme.b"]);
        b.before = ids(&["acme.a"]);
        let errors = OperationRegistry::new(&V1, vec![a, b]).expect_err("refused");
        assert!(
            matches!(errors.as_slice(), [RegistryError::ConstraintCycle { operations, .. }] if operations.len() == 2),
            "{errors:?}"
        );
    }

    #[test]
    fn constraints_that_agree_with_the_canonical_order_are_not_a_cycle() {
        let mut p = plugin("acme.x", names::INPUT_COLOUR, DataSpace::CameraLinear);
        p.after = ids(&["auroraw.white-balance"]);
        p.before = ids(&["auroraw.highlight-reconstruction"]);
        assert!(OperationRegistry::new(&V1, vec![p]).is_ok());
    }

    #[test]
    fn every_problem_is_reported_and_one_bad_declaration_does_not_hide_another() {
        let bad_stage = plugin("acme.a", "nowhere", DataSpace::WorkingLinear);
        let bad_space = plugin("acme.b", names::DETAIL, DataSpace::CameraLinear);
        let good = plugin("acme.c", names::DETAIL, DataSpace::WorkingLinear);
        let errors =
            OperationRegistry::new(&V1, vec![bad_stage, bad_space, good]).expect_err("refused");
        assert_eq!(errors.len(), 2, "{errors:?}");
    }
}
