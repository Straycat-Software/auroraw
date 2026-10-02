// SPDX-License-Identifier: MIT OR Apache-2.0
//! The plugin declaration (architecture §8.3, decisions D-078 and D-142): identifier, version, API version,
//! family, permissions. The host refuses a declaration it cannot satisfy and says why.
//!
//! D-078 also lists a panel, a pipeline stage with ordering constraints, and parameters with limits and defaults:
//! those describe an **operation** plugin's place in the develop pipeline, and D-142 gives them their shape. A
//! declaration is the first of two layers: the small, stable one that `develop`, the version sidecar and the panels
//! read (identifier, version, API version, family, stage and placement, panel, typed [parameters](crate::ParamSpec),
//! the data space it reads and the one it writes, its [cost class](crate::CostClass), the version of the operation and
//! whether it may be used twice in one edit, permissions). The second layer,
//! the implementation descriptor the pipeline alone reads (halo, passes, shaders, the CPU twin), is not here: its data
//! form is defined when the first external GPU operation arrives (M3), and for the built-in operations it is a Rust
//! trait.
//!
//! The fields of the operation are optional or empty for the other families, and a declaration that gives them to
//! another family is refused: a source with a cost class is a mistake the declaration can catch.

use crate::operation::{CostClass, ParamError, ParamSpec};
use crate::{Permissions, spaces};
use serde::{Deserialize, Serialize};

/// What family of capability a plugin provides (architecture §8.1). The families M1 uses, and the operation (WP13);
/// `Export` and others join when their work package builds them, matching how `Family` itself only exists because the
/// declaration schema does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// Lists, describes, reads and watches files (`plugin_api::Source`, WP4).
    Source,
    /// Decodes a file's own bytes to pixels (`plugin_api::Decoder`, WP6).
    Import,
    /// An operation of the develop pipeline (D-142): placed at a stage, with typed parameters, reading and writing a
    /// data space.
    Operation,
}

/// An operation's placement in the develop pipeline: the stage it runs at, and constraints on its order relative to
/// other operations of the same stage (architecture §8.3, D-142, note 006 §7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// The pipeline stage this operation runs at: one of [`crate::stages`] in definition v1. `develop` refuses an
    /// identifier the definition it renders with does not have; a declaration cannot know which definitions will exist,
    /// so [`Declaration::validate`] does not.
    pub stage: String,
    /// Must run after these other **operations**, if they are present in the stage.
    ///
    /// What these name is operations, by their identifier (`auroraw.exposure`), **not stages**: the order of the stages
    /// is the definition's, and the constraints only order the operations that share one. A constraint that names an
    /// operation of another stage is an error at load time (the registry's, when it builds the order); one that names
    /// the declaring operation itself is refused here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Must run before these other **operations**, if they are present in the stage (see [`Placement::after`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
}

/// What every plugin declares (architecture §8.3, D-078, D-142), shown to the person before
/// installation and read by the host before it is trusted with anything.
///
/// **A field this API does not know is ignored when a declaration is read**, not refused: the fields of a newer API
/// version arrive with its `api_version`, which the host checks, and a typo in an optional field (`step`, a misspelt
/// `cost`) is the plugin author's to catch with the required ones, whose absence is an error. Refusing unknown fields
/// is a decision for the stable API (M5), when the format stops moving; `serde` cannot do it for the flattened
/// parameter specs in any case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    /// A stable identifier for this plugin, unique in the index (architecture §8.7).
    pub identifier: String,
    /// The plugin's own version.
    pub version: String,
    /// The plugin API version this declaration was written against. The host refuses a
    /// declaration whose `api_version` it does not support.
    pub api_version: u32,
    /// What capability family this plugin provides.
    pub family: Family,
    /// The panel this plugin's settings appear under, if it has any (operation plugins only; see
    /// this module's doc comment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// This plugin's place in the develop pipeline, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// The operation's typed parameters, in the order the panel shows them (operations only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParamSpec>,
    /// The data space the operation reads, one of [`crate::spaces`] (operations only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_space: Option<String>,
    /// The data space the operation writes, one of [`crate::spaces`] (operations only).
    ///
    /// The declaration has both because D-142 does not decide that an operation may not change the space; **the pipeline
    /// does** (only the definition's spine changes it, note 006 §3.2) and refuses an operation whose two differ when it
    /// reads the declaration, so a plugin that declares two different spaces is not loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_space: Option<String>,
    /// What the operation costs (operations only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostClass>,
    /// The version of the **operation**, which a new edit is written with and the version sidecar stores with the values
    /// (D-142): a whole number from 1, absent for 1 (operations only; see [`Declaration::operation_version`]).
    ///
    /// It is **not** the plugin's [`version`](Declaration::version), which is its release. The number moves when a value
    /// an older edit stored would render differently under the new code (a parameter's meaning or scale changed, one was
    /// removed), and not for a fix, a speed-up or a new optional parameter. The author bumps it; nothing derives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_version: Option<u32>,
    /// The **older** operation versions the plugin still renders, besides its own (operations only). An edit written with
    /// a version that is neither this one nor one of these is not rendered: the operation is skipped, disabled and
    /// marked (architecture §7.2). Each is below [`operation_version`](Declaration::operation_version), once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_reads: Vec<u32>,
    /// Whether one edit may hold the operation more than once (a graduated filter, a local adjustment), so that a panel
    /// offers to add another (operations only). False when absent: most operations are used once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allows_several: bool,
    /// What the plugin asks the host to grant it.
    #[serde(default)]
    pub permissions: Permissions,
}

/// Why the host refused a declaration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeclarationError {
    /// The identifier is empty.
    #[error("a plugin's identifier cannot be empty")]
    EmptyIdentifier,
    /// The version is empty.
    #[error("a plugin's version cannot be empty")]
    EmptyVersion,
    /// The declaration was written against an API version this host does not support.
    #[error("this plugin needs API version {requested}, this host supports up to {supported}")]
    UnsupportedApiVersion {
        /// The version the declaration asked for.
        requested: u32,
        /// The highest version this host understands.
        supported: u32,
    },
    /// A placement names the declaring operation itself in its ordering constraints.
    #[error("operation {operation:?} cannot be constrained relative to itself")]
    SelfConstrainedPlacement {
        /// The operation that names itself.
        operation: String,
    },
    /// A placement names one operation in both `after` and `before`: it can never be ordered.
    #[error("operation {operation:?} is in both `after` and `before`, and cannot be ordered")]
    ContradictoryPlacement {
        /// The operation that is named twice.
        operation: String,
    },
    /// An ordering constraint names nothing.
    #[error("an ordering constraint cannot be empty")]
    EmptyConstraint,
    /// An operation declares no placement.
    #[error("an operation needs a placement: the stage it runs at")]
    OperationWithoutPlacement,
    /// A placement's stage is empty.
    #[error("a placement needs a stage")]
    EmptyStage,
    /// An operation does not say which space it reads or writes.
    #[error("an operation must declare the data space it {side}")]
    MissingSpace {
        /// `"reads"` or `"writes"`.
        side: &'static str,
    },
    /// A data space this API does not know.
    #[error("{0:?} is not a data space of this API version")]
    UnknownSpace(String),
    /// An operation does not declare its cost class.
    #[error("an operation must declare its cost class")]
    MissingCost,
    /// An operation version is 0 (its own, or one of `also_reads`): they start at 1.
    #[error("the version of an operation starts at 1, and 0 is not one")]
    ZeroOperationVersion,
    /// The versions an operation still renders besides its own include one that is not older than it.
    #[error(
        "`also_reads` lists version {version}, which is not older than the operation's own version {current}"
    )]
    AlsoReadsNotOlder {
        /// The version listed.
        version: u32,
        /// The operation's own version.
        current: u32,
    },
    /// The same older version is listed twice.
    #[error("`also_reads` lists version {0} twice")]
    DuplicateAlsoReads(u32),
    /// Two parameters have one key.
    #[error("two parameters have the key {0:?}")]
    DuplicateParameter(String),
    /// A parameter does not hold together.
    #[error("parameter {key:?}: {error}")]
    BadParameter {
        /// The parameter's key.
        key: String,
        /// What is wrong.
        error: ParamError,
    },
    /// A declaration of another family gives a field that only an operation has.
    #[error("{field} belongs to the operation family, and this plugin is another")]
    NotAnOperation {
        /// The field.
        field: &'static str,
    },
}

/// The plugin API version this host understands (architecture §8.2b): the interface is
/// experimental until M5, so this changes without a deprecation period until then.
pub const HOST_API_VERSION: u32 = 0;

impl Declaration {
    /// Checks that this declaration is well formed and that this host can satisfy it
    /// (architecture §8.3: "the host refuses a declaration it cannot satisfy and says why").
    /// Does not check permissions against what the person has actually granted; that happens at
    /// installation and at load time, not here.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        if self.identifier.trim().is_empty() {
            return Err(DeclarationError::EmptyIdentifier);
        }
        if self.version.trim().is_empty() {
            return Err(DeclarationError::EmptyVersion);
        }
        if self.api_version > HOST_API_VERSION {
            return Err(DeclarationError::UnsupportedApiVersion {
                requested: self.api_version,
                supported: HOST_API_VERSION,
            });
        }
        if let Some(placement) = &self.placement {
            if self.family == Family::Operation && placement.stage.trim().is_empty() {
                return Err(DeclarationError::EmptyStage);
            }
            let constraints = placement.after.iter().chain(&placement.before);
            for name in constraints {
                if name.trim().is_empty() {
                    return Err(DeclarationError::EmptyConstraint);
                }
                if *name == self.identifier {
                    return Err(DeclarationError::SelfConstrainedPlacement {
                        operation: name.clone(),
                    });
                }
            }
            // Needs no definition to see: a constraint that asks for both orders.
            if let Some(name) = placement
                .after
                .iter()
                .find(|a| placement.before.contains(a))
            {
                return Err(DeclarationError::ContradictoryPlacement {
                    operation: name.clone(),
                });
            }
        }
        if self.family == Family::Operation {
            self.validate_operation()
        } else {
            self.validate_not_an_operation()
        }
    }

    /// The version of the operation a new edit is written with: its [`operation_version`](Declaration::operation_version),
    /// or 1 when it gives none. Meaningful for an operation only.
    pub fn operation_version(&self) -> u32 {
        self.operation_version.unwrap_or(1)
    }

    /// Every version of the operation the plugin renders, in increasing order: the older ones it still reads
    /// ([`also_reads`](Declaration::also_reads)) and its own. What the pipeline checks an edit's version against.
    pub fn operation_versions(&self) -> Vec<u32> {
        let mut all = self.also_reads.clone();
        all.push(self.operation_version());
        all.sort_unstable();
        all.dedup();
        all
    }

    /// The checks of the operation family: a placement, the two spaces, a cost class, the version numbers, and
    /// parameters that hold together with distinct keys.
    fn validate_operation(&self) -> Result<(), DeclarationError> {
        if self.placement.is_none() {
            return Err(DeclarationError::OperationWithoutPlacement);
        }
        for (space, side) in [(&self.input_space, "reads"), (&self.output_space, "writes")] {
            match space {
                None => return Err(DeclarationError::MissingSpace { side }),
                Some(name) if !spaces::ALL.contains(&name.as_str()) => {
                    return Err(DeclarationError::UnknownSpace(name.clone()));
                }
                Some(_) => {}
            }
        }
        if self.cost.is_none() {
            return Err(DeclarationError::MissingCost);
        }
        if self.operation_version == Some(0) {
            return Err(DeclarationError::ZeroOperationVersion);
        }
        let current = self.operation_version();
        let mut seen = std::collections::HashSet::new();
        for &version in &self.also_reads {
            if version == 0 {
                return Err(DeclarationError::ZeroOperationVersion);
            }
            if version >= current {
                return Err(DeclarationError::AlsoReadsNotOlder { version, current });
            }
            if !seen.insert(version) {
                return Err(DeclarationError::DuplicateAlsoReads(version));
            }
        }
        let mut keys = std::collections::HashSet::new();
        for parameter in &self.parameters {
            parameter
                .validate()
                .map_err(|error| DeclarationError::BadParameter {
                    key: parameter.key.clone(),
                    error,
                })?;
            if !keys.insert(parameter.key.as_str()) {
                return Err(DeclarationError::DuplicateParameter(parameter.key.clone()));
            }
        }
        Ok(())
    }

    /// A source or an import has no parameters, spaces or cost: a declaration that gives them is mistaken about what
    /// it is.
    fn validate_not_an_operation(&self) -> Result<(), DeclarationError> {
        let field = if !self.parameters.is_empty() {
            "parameters"
        } else if self.input_space.is_some() {
            "input_space"
        } else if self.output_space.is_some() {
            "output_space"
        } else if self.cost.is_some() {
            "cost"
        } else if self.operation_version.is_some() {
            "operation_version"
        } else if !self.also_reads.is_empty() {
            "also_reads"
        } else if self.allows_several {
            "allows_several"
        } else {
            return Ok(());
        };
        Err(DeclarationError::NotAnOperation { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> Declaration {
        Declaration {
            identifier: "org.auroraw.rawler".into(),
            version: "0.1.0".into(),
            api_version: HOST_API_VERSION,
            family: Family::Import,
            panel: None,
            placement: None,
            parameters: Vec::new(),
            input_space: None,
            output_space: None,
            cost: None,
            operation_version: None,
            also_reads: Vec::new(),
            allows_several: false,
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn a_well_formed_declaration_validates() {
        assert!(minimal().validate().is_ok());
    }

    #[test]
    fn an_empty_identifier_is_refused() {
        let mut d = minimal();
        d.identifier = "  ".into();
        assert_eq!(d.validate(), Err(DeclarationError::EmptyIdentifier));
    }

    #[test]
    fn an_empty_version_is_refused() {
        let mut d = minimal();
        d.version.clear();
        assert_eq!(d.validate(), Err(DeclarationError::EmptyVersion));
    }

    #[test]
    fn a_future_api_version_is_refused_with_both_numbers() {
        let mut d = minimal();
        d.api_version = HOST_API_VERSION + 1;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::UnsupportedApiVersion {
                requested: HOST_API_VERSION + 1,
                supported: HOST_API_VERSION,
            })
        );
    }

    #[test]
    fn an_operation_constrained_relative_to_itself_is_refused() {
        let mut d = minimal();
        d.placement = Some(Placement {
            stage: "scene-linear".into(),
            after: vec!["org.auroraw.rawler".into()],
            before: vec![],
        });
        assert_eq!(
            d.validate(),
            Err(DeclarationError::SelfConstrainedPlacement {
                operation: "org.auroraw.rawler".into()
            })
        );
        // Naming the stage is not naming oneself: the constraints name operations.
        d.placement = Some(Placement {
            stage: "scene-linear".into(),
            after: vec!["scene-linear".into()],
            before: vec![],
        });
        assert!(d.validate().is_ok());
    }

    use crate::operation::{ParamKind, ParamSpec};
    use crate::{spaces, stages};

    /// An exposure operation, as a plugin or a built-in declares it.
    fn exposure() -> Declaration {
        Declaration {
            identifier: "auroraw.exposure".into(),
            version: "1".into(),
            api_version: HOST_API_VERSION,
            family: Family::Operation,
            panel: Some("tone".into()),
            placement: Some(Placement {
                stage: stages::SCENE_LINEAR.into(),
                after: vec!["auroraw.white-balance".into()],
                before: vec![],
            }),
            parameters: vec![ParamSpec {
                key: "ev".into(),
                label: "exposure.ev".into(),
                kind: ParamKind::Float {
                    min: -5.0,
                    max: 5.0,
                    default: 0.0,
                },
            }],
            input_space: Some(spaces::WORKING_LINEAR.into()),
            output_space: Some(spaces::WORKING_LINEAR.into()),
            cost: Some(CostClass::Interactive),
            operation_version: None,
            also_reads: Vec::new(),
            allows_several: false,
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn an_operation_with_everything_it_needs_validates() {
        assert_eq!(exposure().validate(), Ok(()));
    }

    #[test]
    fn what_an_operation_needs_is_named_when_it_is_missing() {
        let mut d = exposure();
        d.placement = None;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::OperationWithoutPlacement)
        );
        let mut d = exposure();
        d.input_space = None;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::MissingSpace { side: "reads" })
        );
        let mut d = exposure();
        d.output_space = None;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::MissingSpace { side: "writes" })
        );
        let mut d = exposure();
        d.cost = None;
        assert_eq!(d.validate(), Err(DeclarationError::MissingCost));
        let mut d = exposure();
        d.placement.as_mut().unwrap().stage = " ".into();
        assert_eq!(d.validate(), Err(DeclarationError::EmptyStage));
    }

    #[test]
    fn a_space_this_api_does_not_know_is_refused() {
        let mut d = exposure();
        d.output_space = Some("display".into());
        assert_eq!(
            d.validate(),
            Err(DeclarationError::UnknownSpace("display".into()))
        );
        // Every space the constants name is known.
        for name in spaces::ALL {
            let mut d = exposure();
            d.input_space = Some(name.into());
            assert_eq!(d.validate(), Ok(()), "{name}");
        }
    }

    #[test]
    fn the_parameters_hold_together_and_their_keys_are_distinct() {
        let mut d = exposure();
        d.parameters.push(d.parameters[0].clone());
        assert_eq!(
            d.validate(),
            Err(DeclarationError::DuplicateParameter("ev".into()))
        );
        let mut d = exposure();
        d.parameters[0].kind = ParamKind::Float {
            min: 1.0,
            max: 0.0,
            default: 0.5,
        };
        assert!(matches!(
            d.validate(),
            Err(DeclarationError::BadParameter { key, .. }) if key == "ev"
        ));
    }

    #[test]
    fn the_ordering_constraints_name_operations_and_never_nothing_or_oneself() {
        let mut d = exposure();
        d.placement.as_mut().unwrap().before = vec!["auroraw.exposure".into()];
        assert_eq!(
            d.validate(),
            Err(DeclarationError::SelfConstrainedPlacement {
                operation: "auroraw.exposure".into()
            })
        );
        let mut d = exposure();
        d.placement.as_mut().unwrap().after.push("".into());
        assert_eq!(d.validate(), Err(DeclarationError::EmptyConstraint));
    }

    #[test]
    fn an_operation_cannot_be_asked_to_run_both_before_and_after_another() {
        let mut d = exposure();
        let placement = d.placement.as_mut().unwrap();
        placement.after = vec!["auroraw.tone".into(), "auroraw.curve".into()];
        placement.before = vec!["auroraw.tone".into()];
        assert_eq!(
            d.validate(),
            Err(DeclarationError::ContradictoryPlacement {
                operation: "auroraw.tone".into()
            })
        );
        // The same names on one side, or different names on the two, are fine.
        let placement = d.placement.as_mut().unwrap();
        placement.before = vec!["auroraw.vignette".into()];
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn an_operation_without_a_version_is_version_1_and_reads_only_that() {
        let d = exposure();
        assert_eq!(d.operation_version(), 1);
        assert_eq!(d.operation_versions(), vec![1]);
        assert!(!d.allows_several);
    }

    #[test]
    fn the_versions_of_an_operation_are_its_own_and_the_older_ones_it_still_reads() {
        let mut d = exposure();
        d.operation_version = Some(4);
        d.also_reads = vec![3, 1];
        assert_eq!(d.validate(), Ok(()));
        assert_eq!(d.operation_version(), 4);
        assert_eq!(d.operation_versions(), vec![1, 3, 4]);
        // The plugin's release is not the operation's version: no relation is read from the string.
        d.version = "9.9.9".into();
        assert_eq!(d.operation_versions(), vec![1, 3, 4]);
    }

    #[test]
    fn a_version_of_an_operation_starts_at_1_and_the_older_ones_are_older_and_distinct() {
        let mut d = exposure();
        d.operation_version = Some(0);
        assert_eq!(d.validate(), Err(DeclarationError::ZeroOperationVersion));
        let mut d = exposure();
        d.operation_version = Some(2);
        d.also_reads = vec![2];
        assert_eq!(
            d.validate(),
            Err(DeclarationError::AlsoReadsNotOlder {
                version: 2,
                current: 2
            })
        );
        d.also_reads = vec![3];
        assert_eq!(
            d.validate(),
            Err(DeclarationError::AlsoReadsNotOlder {
                version: 3,
                current: 2
            })
        );
        let mut d = exposure();
        d.operation_version = Some(3);
        d.also_reads = vec![1, 2, 1];
        assert_eq!(d.validate(), Err(DeclarationError::DuplicateAlsoReads(1)));
        // Nothing is older than version 1, and 0 is not a version.
        d.also_reads = vec![0];
        assert_eq!(d.validate(), Err(DeclarationError::ZeroOperationVersion));
    }

    #[test]
    fn the_fields_of_an_operation_are_refused_on_another_family() {
        let mut d = minimal();
        d.parameters = exposure().parameters;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "parameters"
            })
        );
        let mut d = minimal();
        d.input_space = Some(spaces::SENSOR_RAW.into());
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "input_space"
            })
        );
        let mut d = minimal();
        d.output_space = Some(spaces::SENSOR_RAW.into());
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "output_space"
            })
        );
        let mut d = minimal();
        d.cost = Some(CostClass::Heavy);
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation { field: "cost" })
        );
        let mut d = minimal();
        d.operation_version = Some(1);
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "operation_version"
            })
        );
        let mut d = minimal();
        d.also_reads = vec![1];
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "also_reads"
            })
        );
        let mut d = minimal();
        d.allows_several = true;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::NotAnOperation {
                field: "allows_several"
            })
        );
    }

    #[test]
    fn an_operation_round_trips_through_json_and_a_declaration_without_its_fields_is_still_read() {
        let d = exposure();
        let text = serde_json::to_string(&d).unwrap();
        let back: Declaration = serde_json::from_str(&text).unwrap();
        assert_eq!(back, d);
        assert!(text.contains(r#""family":"operation""#), "{text}");
        assert!(text.contains(r#""cost":"interactive""#), "{text}");
        // The new fields are left out when they say nothing, so an operation written before them is the same text...
        assert!(!text.contains("operation_version"), "{text}");
        assert!(!text.contains("also_reads"), "{text}");
        assert!(!text.contains("allows_several"), "{text}");
        // ... and are written when they do, and read back.
        let mut several = exposure();
        several.operation_version = Some(2);
        several.also_reads = vec![1];
        several.allows_several = true;
        let text = serde_json::to_string(&several).unwrap();
        assert!(text.contains(r#""operation_version":2"#), "{text}");
        assert!(text.contains(r#""also_reads":[1]"#), "{text}");
        assert!(text.contains(r#""allows_several":true"#), "{text}");
        assert_eq!(serde_json::from_str::<Declaration>(&text).unwrap(), several);
        // A declaration written before the operation family: the new fields are absent, and that is fine.
        let old = r#"{ "identifier": "org.auroraw.rawler", "version": "0.1.0", "api_version": 0, "family": "import" }"#;
        let read: Declaration = serde_json::from_str(old).unwrap();
        assert_eq!(read, minimal());
        assert_eq!(read.validate(), Ok(()));
    }

    #[test]
    fn round_trips_through_json() {
        let d = minimal();
        let text = serde_json::to_string(&d).unwrap();
        let back: Declaration = serde_json::from_str(&text).unwrap();
        assert_eq!(d, back);
    }
}
