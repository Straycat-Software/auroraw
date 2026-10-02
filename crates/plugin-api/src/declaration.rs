// SPDX-License-Identifier: MIT OR Apache-2.0
//! The plugin declaration (architecture §8.3, decisions D-078 and D-142): identifier, version, API version,
//! family, permissions. The host refuses a declaration it cannot satisfy and says why.
//!
//! D-078 also lists a panel, a pipeline stage with ordering constraints, and parameters with limits and defaults:
//! those describe an **operation** plugin's place in the develop pipeline, and D-142 gives them their shape. A
//! declaration is the first of two layers: the small, stable one that `develop`, the version sidecar and the panels
//! read (identifier, version, API version, family, stage and placement, panel, typed [parameters](crate::ParamSpec),
//! the data space it reads and the one it writes, its [cost class](crate::CostClass), permissions). The second layer,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_space: Option<String>,
    /// What the operation costs (operations only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostClass>,
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

    /// The checks of the operation family: a placement, the two spaces, a cost class, and parameters that hold together
    /// with distinct keys.
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
    }

    #[test]
    fn an_operation_round_trips_through_json_and_a_declaration_without_its_fields_is_still_read() {
        let d = exposure();
        let text = serde_json::to_string(&d).unwrap();
        let back: Declaration = serde_json::from_str(&text).unwrap();
        assert_eq!(back, d);
        assert!(text.contains(r#""family":"operation""#), "{text}");
        assert!(text.contains(r#""cost":"interactive""#), "{text}");
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
