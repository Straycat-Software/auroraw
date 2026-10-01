// SPDX-License-Identifier: GPL-3.0-or-later
//! The operations the engine knows: the built-in ones, which the definition lists, and the ones plugins
//! declare. The engine fills the registry and hands it to the pipeline; **the pipeline never loads a
//! plugin** (D-140), which keeps `pipeline` off `plugin-host`.
//!
//! A registry is **checked when it is loaded**, never silently repaired (design note 006 §3.4): a
//! declaration that names no stage the definition has, that reads a data space its stage does not deliver,
//! that constrains itself, that names an operation of another stage, or whose constraints cannot all hold
//! with the canonical order, is **refused with the reason, and the rest of the registry stays valid**:
//! [`OperationRegistry::load`] returns the registry of every declaration that was accepted together with
//! the refusals, so that one plugin with a typo in its stage name does not leave the engine without a
//! registry. **The built-in operations are never refused** (the definition owns them); a cycle is cut by
//! refusing the plugin operations in it.
//!
//! **A stand-in for the declaration.** What an operation declares (its stage, its placement, its input data
//! space, its versions) is layer 1 of D-142, in `plugin-api`, with work package 13. [`OperationInfo`] is the
//! part of it the pipeline reads, and is built from the declaration when that arrives.

use std::collections::{BTreeMap, BTreeSet};

use crate::definition::{DataSpace, Definition};
use crate::recipe::OperationId;

/// What the pipeline needs to know of an operation.
///
/// It says an operation **reads and returns one space** (`input_space`). That holds as long as the spine,
/// and not an operation, changes the space; D-142 declares an input **and** an output space, so the real
/// declaration keeps both and work package 13 validates that they are equal for an operation (the pipeline
/// reads one of the two). The cost class is `develop`'s, not the pipeline's, and is absent here on purpose.
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

/// The outcome of loading a registry: what was accepted, and why the rest was refused.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Every built-in operation and every declaration that was not refused.
    pub registry: OperationRegistry,
    /// Why each refused declaration was refused. Empty when everything was accepted.
    pub refused: Vec<RegistryError>,
}

impl Loaded {
    /// The registry if nothing was refused, else every refusal: all or nothing, for a caller that wants it
    /// (the engine at start-up uses [`Loaded::registry`] and reports [`Loaded::refused`] instead).
    pub fn complete(self) -> Result<OperationRegistry, Vec<RegistryError>> {
        if self.refused.is_empty() {
            Ok(self.registry)
        } else {
            Err(self.refused)
        }
    }
}

impl OperationRegistry {
    /// The built-in operations of `definition` (version 1 of each) and the `external` declarations (a
    /// plugin's) that are valid, with the reasons the others were refused.
    ///
    /// Every problem is reported, so that one bad declaration does not hide another, and **none of them
    /// leaves the engine without a registry**. A built-in operation is never refused.
    ///
    /// **Duplicates**: the first declaration of an identifier is kept, and a later one (or a plugin
    /// taking a built-in's identifier) is refused.
    pub fn load(definition: &'static Definition, external: Vec<OperationInfo>) -> Loaded {
        let mut refused = Vec::new();
        let mut entries: BTreeMap<OperationId, Entry> = BTreeMap::new();

        for (stage_index, stage) in definition.stages.iter().enumerate() {
            for (rank, id) in stage.operations.iter().enumerate() {
                let info = OperationInfo {
                    id: OperationId::from(*id),
                    stage: stage.id.to_string(),
                    after: Vec::new(),
                    before: Vec::new(),
                    input_space: stage.operations_read(),
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
                refused.push(RegistryError::DuplicateId { operation: info.id });
                continue;
            }
            let Some(stage_index) = definition.stage_index(&info.stage) else {
                refused.push(RegistryError::UnknownStage {
                    operation: info.id,
                    stage: info.stage,
                });
                continue;
            };
            let expected = definition.stages[stage_index].operations_read();
            if info.input_space != expected {
                refused.push(RegistryError::WrongSpace {
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
        let mut cut = BTreeSet::new();
        for (id, entry) in &entries {
            for other_id in entry.info.after.iter().chain(&entry.info.before) {
                if other_id == id {
                    refused.push(RegistryError::SelfConstraint {
                        operation: id.clone(),
                    });
                    cut.insert(id.clone());
                } else if let Some(other) = entries.get(other_id)
                    && other.stage != entry.stage
                {
                    refused.push(RegistryError::ConstraintAcrossStages {
                        operation: id.clone(),
                        stage: entry.info.stage.clone(),
                        other: other_id.clone(),
                        other_stage: other.info.stage.clone(),
                    });
                    cut.insert(id.clone());
                }
            }
        }
        for id in &cut {
            entries.remove(id);
        }

        // Constraints that cannot all hold together with the canonical order: the operations **in** a cycle
        // (a strongly connected component), not what merely hangs off it. The plugin operations in it are
        // refused; a built-in is part of the cycle but never leaves, the definition owning it.
        for (stage_index, stage) in definition.stages.iter().enumerate() {
            for cycle in cycles(&entries, stage_index) {
                for id in &cycle {
                    if entries.get(id).is_some_and(|e| e.canonical_rank.is_none()) {
                        entries.remove(id);
                    }
                }
                refused.push(RegistryError::ConstraintCycle {
                    stage: stage.id.to_string(),
                    operations: cycle,
                });
            }
        }

        Loaded {
            registry: OperationRegistry {
                definition,
                entries,
            },
            refused,
        }
    }

    /// [`OperationRegistry::load`], all or nothing: the registry if no declaration was refused, else every
    /// refusal.
    pub fn new(
        definition: &'static Definition,
        external: Vec<OperationInfo>,
    ) -> Result<OperationRegistry, Vec<RegistryError>> {
        OperationRegistry::load(definition, external).complete()
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

/// The cycles among the operations of a stage: every strongly connected component of more than one
/// operation, in the graph of "must come before" made of the canonical order of the built-ins and every
/// constraint (Tarjan's algorithm). A component holds the operations that are **in** a cycle, and not what
/// merely hangs off one, which a topological sort cannot tell apart. Each is sorted, and they come in order.
fn cycles(entries: &BTreeMap<OperationId, Entry>, stage: usize) -> Vec<Vec<OperationId>> {
    let nodes: Vec<&OperationId> = entries
        .iter()
        .filter(|(_, e)| e.stage == stage)
        .map(|(id, _)| id)
        .collect();
    let index_of = |id: &OperationId| nodes.iter().position(|n| *n == id);
    // `successors[a]` holds the operations that must come after `a`.
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut canonical: Vec<(usize, usize)> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, id)| entries[*id].canonical_rank.map(|rank| (rank, i)))
        .collect();
    canonical.sort_unstable();
    for pair in canonical.windows(2) {
        successors[pair[0].1].push(pair[1].1);
    }
    for (i, id) in nodes.iter().enumerate() {
        let info = &entries[*id].info;
        for other in info.after.iter().filter_map(index_of) {
            successors[other].push(i);
        }
        for other in info.before.iter().filter_map(index_of) {
            successors[i].push(other);
        }
    }

    struct Tarjan<'a> {
        successors: &'a [Vec<usize>],
        next: usize,
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        components: Vec<Vec<usize>>,
    }
    impl Tarjan<'_> {
        fn visit(&mut self, v: usize) {
            self.index[v] = Some(self.next);
            self.low[v] = self.next;
            self.next += 1;
            self.stack.push(v);
            self.on_stack[v] = true;
            for &w in &self.successors[v] {
                match self.index[w] {
                    None => {
                        self.visit(w);
                        self.low[v] = self.low[v].min(self.low[w]);
                    }
                    Some(order) if self.on_stack[w] => self.low[v] = self.low[v].min(order),
                    Some(_) => {}
                }
            }
            if Some(self.low[v]) == self.index[v] {
                let mut component = Vec::new();
                while let Some(w) = self.stack.pop() {
                    self.on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }
    let mut tarjan = Tarjan {
        successors: &successors,
        next: 0,
        index: vec![None; nodes.len()],
        low: vec![0; nodes.len()],
        on_stack: vec![false; nodes.len()],
        stack: Vec::new(),
        components: Vec::new(),
    };
    for v in 0..nodes.len() {
        if tarjan.index[v].is_none() {
            tarjan.visit(v);
        }
    }
    let mut found: Vec<Vec<OperationId>> = tarjan
        .components
        .into_iter()
        .filter(|c| c.len() > 1)
        .map(|c| {
            let mut ids: Vec<OperationId> = c.into_iter().map(|i| nodes[i].clone()).collect();
            ids.sort();
            ids
        })
        .collect();
    found.sort();
    found
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

    // ---- loading keeps what is valid: one bad declaration is not "no registry" ----

    #[test]
    fn one_bad_declaration_is_refused_and_the_rest_of_the_registry_loads() {
        let bad = plugin("acme.typo", "scene-linaer", DataSpace::WorkingLinear);
        let good = plugin("acme.dehaze", names::SCENE_LINEAR, DataSpace::WorkingLinear);
        let loaded = OperationRegistry::load(&V1, vec![bad, good]);
        assert_eq!(
            loaded.refused,
            vec![RegistryError::UnknownStage {
                operation: OperationId::from("acme.typo"),
                stage: "scene-linaer".to_string(),
            }]
        );
        let registry = loaded.registry;
        assert!(
            registry.contains(&OperationId::from("acme.dehaze")),
            "the good plugin loaded"
        );
        assert!(
            registry.contains(&OperationId::from("auroraw.white-balance")),
            "the built-ins loaded"
        );
        assert!(
            !registry.contains(&OperationId::from("acme.typo")),
            "the refused one did not"
        );
    }

    #[test]
    fn a_registry_with_nothing_refused_is_complete() {
        let loaded = OperationRegistry::load(
            &V1,
            vec![plugin("acme.x", names::DETAIL, DataSpace::WorkingLinear)],
        );
        assert!(loaded.refused.is_empty());
        assert!(loaded.complete().is_ok());
        let bad = OperationRegistry::load(
            &V1,
            vec![plugin("acme.x", "nowhere", DataSpace::WorkingLinear)],
        );
        assert_eq!(bad.complete().expect_err("something was refused").len(), 1);
    }

    #[test]
    fn a_cycle_is_cut_by_refusing_the_plugin_operations_in_it_and_the_builtins_stay() {
        // The case of the registry tests above: a plugin that must come before the balance and after the
        // reconstruction. The cycle is white balance, reconstruction and the plugin; the plugin goes.
        let mut cycle = plugin("acme.x", names::INPUT_COLOUR, DataSpace::CameraLinear);
        cycle.before = ids(&["auroraw.white-balance"]);
        cycle.after = ids(&["auroraw.highlight-reconstruction"]);
        let fine = plugin("acme.fine", names::INPUT_COLOUR, DataSpace::CameraLinear);
        let loaded = OperationRegistry::load(&V1, vec![cycle, fine]);
        assert_eq!(loaded.refused.len(), 1, "{:?}", loaded.refused);
        let registry = loaded.registry;
        assert!(
            !registry.contains(&OperationId::from("acme.x")),
            "the plugin in the cycle was refused"
        );
        assert!(
            registry.contains(&OperationId::from("auroraw.white-balance")),
            "a built-in is never refused"
        );
        assert!(registry.contains(&OperationId::from("auroraw.highlight-reconstruction")));
        assert!(
            registry.contains(&OperationId::from("acme.fine")),
            "an unrelated plugin of the same stage stays"
        );
    }

    #[test]
    fn the_cycle_names_the_operations_in_it_and_not_what_hangs_off_it() {
        // A and B must each come before the other. C only asks to come after A: it is not in the cycle, and
        // nothing is wrong with it once A is refused (the constraint is relative and optional).
        let mut a = plugin("acme.a", names::DETAIL, DataSpace::WorkingLinear);
        let mut b = plugin("acme.b", names::DETAIL, DataSpace::WorkingLinear);
        let mut c = plugin("acme.c", names::DETAIL, DataSpace::WorkingLinear);
        a.before = ids(&["acme.b"]);
        b.before = ids(&["acme.a"]);
        c.after = ids(&["acme.a"]);
        let loaded = OperationRegistry::load(&V1, vec![a, b, c]);
        assert_eq!(
            loaded.refused,
            vec![RegistryError::ConstraintCycle {
                stage: names::DETAIL.to_string(),
                operations: ids(&["acme.a", "acme.b"]),
            }]
        );
        assert!(
            loaded.registry.contains(&OperationId::from("acme.c")),
            "what hangs off a cycle is not in it"
        );
        assert!(!loaded.registry.contains(&OperationId::from("acme.a")));
        assert!(!loaded.registry.contains(&OperationId::from("acme.b")));
    }

    #[test]
    fn two_separate_cycles_in_a_stage_are_both_reported() {
        let mk = |id: &str, before: &str| {
            let mut p = plugin(id, names::SCENE_LINEAR, DataSpace::WorkingLinear);
            p.before = ids(&[before]);
            p
        };
        let loaded = OperationRegistry::load(
            &V1,
            vec![
                mk("acme.a", "acme.b"),
                mk("acme.b", "acme.a"),
                mk("acme.c", "acme.d"),
                mk("acme.d", "acme.c"),
            ],
        );
        let cycles: Vec<_> = loaded
            .refused
            .iter()
            .filter_map(|e| match e {
                RegistryError::ConstraintCycle { operations, .. } => Some(operations.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            cycles,
            vec![ids(&["acme.a", "acme.b"]), ids(&["acme.c", "acme.d"])]
        );
    }

    #[test]
    fn of_two_declarations_of_one_identifier_the_first_is_kept() {
        let first = plugin("acme.x", names::DETAIL, DataSpace::WorkingLinear);
        let mut second = plugin("acme.x", names::SCENE_LINEAR, DataSpace::WorkingLinear);
        second.versions = vec![9];
        let loaded = OperationRegistry::load(&V1, vec![first, second]);
        assert_eq!(
            loaded.refused,
            vec![RegistryError::DuplicateId {
                operation: OperationId::from("acme.x")
            }]
        );
        let kept = loaded
            .registry
            .get(&OperationId::from("acme.x"))
            .expect("the first is kept");
        assert_eq!(kept.info.stage, names::DETAIL);
    }
}
