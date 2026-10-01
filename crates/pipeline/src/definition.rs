// SPDX-License-Identifier: GPL-3.0-or-later
//! The pipeline definition v1 (design note 006 §3, decided in D-146): the **stages**, the **data space**
//! at each boundary, the working space, the **fixed spine** the definition owns, and the **canonical order**
//! of the built-in operations in each stage. It holds no parameters, no plugin and no mask: a recipe
//! carries those (note 005 §2.2).
//!
//! A definition is an **immutable, versioned value**. A released version is never edited: the engine keeps
//! every released definition and renders a recipe with the one it names, and [`verify_released`] holds a
//! fingerprint of each so that "no edit of version 1 after release" does not depend on a person's memory
//! (note 006 §3.6). **Version 1 is not released yet**: it freezes at the start of increment B, once the
//! working space (note 006 §5) is settled and the denoiser's choice has confirmed the placement of white
//! balance (note 006 §4.6). Until then it may change, and each change is a commit to this file, not a
//! version.
//!
//! The identifiers of the stages and of the data spaces are part of the plugin surface (a declaration names
//! them), so they move to `plugin-api` as constants with work package 13, and a test then checks that this
//! table equals them. They are here for now, in [`names`].

use std::fmt;

/// The identifiers of the stages and of the data spaces, as text.
///
/// These are a plugin's vocabulary (`Placement.stage`, an operation's input space), so they belong in
/// `plugin-api` (note 006 §3.2, review of the note); they are here until work package 13 moves them.
pub mod names {
    /// Stage 1: the black and white levels, and the operations on the mosaic.
    pub const RAW_LINEAR: &str = "raw-linear";
    /// Stage 2: the demosaic.
    pub const DEMOSAIC: &str = "demosaic";
    /// Stage 3: the operations on camera RGB that do not need the colour interpretation.
    pub const CAMERA_RGB: &str = "camera-rgb";
    /// Stage 4: the white balance and the camera-to-working step.
    pub const INPUT_COLOUR: &str = "input-colour";
    /// Stage 5: the operations on scene-linear values in the working space.
    pub const SCENE_LINEAR: &str = "scene-linear";
    /// Stage 6: orientation, crop and straighten.
    pub const GEOMETRY: &str = "geometry";
    /// Stage 7: sharpening.
    pub const DETAIL: &str = "detail";
    /// Stage 8: the tone map and the output transform.
    pub const DISPLAY: &str = "display";

    /// The stages of definition v1, in order.
    pub const STAGES: [&str; 8] = [
        RAW_LINEAR,
        DEMOSAIC,
        CAMERA_RGB,
        INPUT_COLOUR,
        SCENE_LINEAR,
        GEOMETRY,
        DETAIL,
        DISPLAY,
    ];

    /// The data spaces, in the order the chain passes through them.
    pub const SPACES: [&str; 5] = [
        "sensor-raw",
        "mosaic-linear",
        "camera-linear",
        "working-linear",
        "display-referred",
    ];

    /// The working space of definition v1: linear Rec.2020 primaries, D65 white (provisional until the
    /// ProPhoto measurement, note 006 §5).
    pub const WORKING_SPACE_V1: &str = "rec2020-linear-d65";
}

/// A state of the data that matters for whether an operation is correct (note 006 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DataSpace {
    /// The decoder's samples, as counts, the levels not yet applied.
    SensorRaw,
    /// One linear value per photosite, levels applied, white at 1.0, the colour filter still present.
    MosaicLinear,
    /// Linear RGB per pixel in the camera's primaries, no upper bound, white balance not yet applied.
    CameraLinear,
    /// Linear RGB per pixel in the working space's primaries, scene-referred, no upper bound.
    WorkingLinear,
    /// Values in the output space's encoding, bounded to 0..1.
    DisplayReferred,
}

/// The three families of D-142's "input and output data space", of which the five spaces are refinements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceFamily {
    /// Sensor data.
    Raw,
    /// Linear light, scene-referred.
    SceneLinear,
    /// What is shown.
    DisplayReferred,
}

impl DataSpace {
    /// The five spaces, in the order the chain passes through them.
    pub const ALL: [DataSpace; 5] = [
        DataSpace::SensorRaw,
        DataSpace::MosaicLinear,
        DataSpace::CameraLinear,
        DataSpace::WorkingLinear,
        DataSpace::DisplayReferred,
    ];

    /// The identifier a declaration uses.
    pub const fn name(self) -> &'static str {
        match self {
            DataSpace::SensorRaw => "sensor-raw",
            DataSpace::MosaicLinear => "mosaic-linear",
            DataSpace::CameraLinear => "camera-linear",
            DataSpace::WorkingLinear => "working-linear",
            DataSpace::DisplayReferred => "display-referred",
        }
    }

    /// The space a declaration names, if it is one.
    pub fn from_name(name: &str) -> Option<DataSpace> {
        DataSpace::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The family of D-142 this space refines.
    pub const fn family(self) -> SpaceFamily {
        match self {
            DataSpace::SensorRaw | DataSpace::MosaicLinear => SpaceFamily::Raw,
            DataSpace::CameraLinear | DataSpace::WorkingLinear => SpaceFamily::SceneLinear,
            DataSpace::DisplayReferred => SpaceFamily::DisplayReferred,
        }
    }
}

impl fmt::Display for DataSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Whether a spine step runs before the stage's operations or after them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runs {
    /// Before every operation of the stage (the levels, the orientation).
    BeforeOperations,
    /// After every operation of the stage (the camera-to-working matrix, the output transform).
    AfterOperations,
}

/// A step the definition always does: a person cannot remove or reorder it (note 006 §3.1). Its inputs
/// come from the image and the output, not from the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpineStep {
    /// The step's name.
    pub name: &'static str,
    /// Where it runs in its stage.
    pub runs: Runs,
}

/// A stage of the definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage {
    /// The stage's identifier ([`names`]).
    pub id: &'static str,
    /// The space the stage receives.
    pub input: DataSpace,
    /// The space the stage returns.
    pub output: DataSpace,
    /// The space an operation of this stage reads and returns. It differs from `input` where a spine step
    /// that changes the space runs first (the levels, in `raw-linear`).
    pub operations_read: DataSpace,
    /// What the definition always does in this stage.
    pub spine: &'static [SpineStep],
    /// The built-in operations of this stage, in their **canonical order** (note 006 §3.4): their
    /// identifiers are provisional until work package 15 fixes them.
    pub operations: &'static [&'static str],
}

/// A pipeline definition: an immutable, versioned value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Definition {
    /// The version, which enters every recipe's hash (note 006 §3.6).
    pub version: u32,
    /// The working space, part of the definition: changing it is a new version.
    pub working_space: &'static str,
    /// The stages, in order.
    pub stages: &'static [Stage],
}

const fn spine(name: &'static str, runs: Runs) -> SpineStep {
    SpineStep { name, runs }
}

const V1_STAGES: &[Stage] = &[
    Stage {
        id: names::RAW_LINEAR,
        input: DataSpace::SensorRaw,
        output: DataSpace::MosaicLinear,
        operations_read: DataSpace::MosaicLinear,
        spine: &[spine("levels", Runs::BeforeOperations)],
        operations: &["auroraw.hot-pixels"],
    },
    Stage {
        id: names::DEMOSAIC,
        input: DataSpace::MosaicLinear,
        output: DataSpace::CameraLinear,
        operations_read: DataSpace::MosaicLinear,
        spine: &[spine("demosaic", Runs::BeforeOperations)],
        operations: &[],
    },
    Stage {
        id: names::CAMERA_RGB,
        input: DataSpace::CameraLinear,
        output: DataSpace::CameraLinear,
        operations_read: DataSpace::CameraLinear,
        spine: &[],
        operations: &["auroraw.noise-reduction"],
    },
    Stage {
        id: names::INPUT_COLOUR,
        input: DataSpace::CameraLinear,
        output: DataSpace::WorkingLinear,
        operations_read: DataSpace::CameraLinear,
        spine: &[spine("camera-to-working", Runs::AfterOperations)],
        operations: &["auroraw.white-balance", "auroraw.highlight-reconstruction"],
    },
    Stage {
        id: names::SCENE_LINEAR,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
        operations_read: DataSpace::WorkingLinear,
        spine: &[],
        operations: &[
            "auroraw.exposure",
            "auroraw.tone",
            "auroraw.curve",
            "auroraw.saturation-vibrance",
            "auroraw.hsl",
            "auroraw.colour-grading",
        ],
    },
    Stage {
        id: names::GEOMETRY,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
        operations_read: DataSpace::WorkingLinear,
        spine: &[spine("orientation-and-crop", Runs::BeforeOperations)],
        operations: &["auroraw.crop", "auroraw.straighten"],
    },
    Stage {
        id: names::DETAIL,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
        operations_read: DataSpace::WorkingLinear,
        spine: &[],
        operations: &["auroraw.sharpening"],
    },
    Stage {
        id: names::DISPLAY,
        input: DataSpace::WorkingLinear,
        output: DataSpace::DisplayReferred,
        operations_read: DataSpace::WorkingLinear,
        spine: &[spine("output-transform", Runs::AfterOperations)],
        operations: &["auroraw.tone-map"],
    },
];

/// The definition v1 (note 006 §3.3): eight stages. **Not released**: see the module documentation.
pub const V1: Definition = Definition {
    version: 1,
    working_space: names::WORKING_SPACE_V1,
    stages: V1_STAGES,
};

/// The definition a version number names, if the engine knows it. A released version stays here forever.
pub fn by_version(version: u32) -> Option<&'static Definition> {
    match version {
        1 => Some(&V1),
        _ => None,
    }
}

/// The fingerprints of the **released** definitions, as `(version, blake3 of the canonical encoding in
/// hexadecimal)`. A version is added here the day it is released, and from then on
/// [`verify_released`] fails if its definition changes. **Empty: nothing is released yet.**
pub const RELEASED: &[(u32, &str)] = &[];

impl Definition {
    /// The position of a stage, by identifier.
    pub fn stage_index(&self, id: &str) -> Option<usize> {
        self.stages.iter().position(|s| s.id == id)
    }

    /// The stage that lists `operation` among its built-in operations, and its position in the canonical
    /// order of that stage.
    pub fn builtin(&self, operation: &str) -> Option<(usize, usize)> {
        self.stages.iter().enumerate().find_map(|(stage, s)| {
            s.operations
                .iter()
                .position(|o| *o == operation)
                .map(|rank| (stage, rank))
        })
    }

    /// The canonical encoding the fingerprint is taken over: the version, the working space, and for each
    /// stage its identifier, its spaces, its **spine** (steps and where they run) and its canonical order.
    /// Every string is length-prefixed, so two different definitions never encode alike.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn text(out: &mut Vec<u8>, s: &str) {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        let mut out = Vec::new();
        text(&mut out, "auroraw-pipeline-definition");
        out.extend_from_slice(&self.version.to_le_bytes());
        text(&mut out, self.working_space);
        out.extend_from_slice(&(self.stages.len() as u32).to_le_bytes());
        for stage in self.stages {
            text(&mut out, stage.id);
            text(&mut out, stage.input.name());
            text(&mut out, stage.output.name());
            text(&mut out, stage.operations_read.name());
            out.extend_from_slice(&(stage.spine.len() as u32).to_le_bytes());
            for step in stage.spine {
                text(&mut out, step.name);
                out.push(match step.runs {
                    Runs::BeforeOperations => 0,
                    Runs::AfterOperations => 1,
                });
            }
            out.extend_from_slice(&(stage.operations.len() as u32).to_le_bytes());
            for operation in stage.operations {
                text(&mut out, operation);
            }
        }
        out
    }

    /// The fingerprint: a `blake3` of [`Definition::canonical_bytes`], in hexadecimal.
    pub fn fingerprint(&self) -> String {
        blake3::hash(&self.canonical_bytes()).to_hex().to_string()
    }
}

/// Why a released definition no longer matches its recorded fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleasedError {
    /// The engine does not know a released version at all.
    #[error("released definition version {0} is not known to the engine")]
    Missing(u32),
    /// A released definition was edited. A released version is never edited: a change is a new version.
    #[error(
        "definition version {version} was released with fingerprint {recorded} and is now {current}: a released version is never edited"
    )]
    Changed {
        /// The version.
        version: u32,
        /// The fingerprint recorded at release.
        recorded: String,
        /// The fingerprint now.
        current: String,
    },
}

/// Checks `released` (pairs of version and recorded fingerprint) against `lookup`: every released
/// definition must still exist and still have its recorded fingerprint.
pub fn verify_released(
    released: &[(u32, &str)],
    lookup: impl Fn(u32) -> Option<&'static Definition>,
) -> Result<(), Vec<ReleasedError>> {
    let mut errors = Vec::new();
    for &(version, recorded) in released {
        match lookup(version) {
            None => errors.push(ReleasedError::Missing(version)),
            Some(definition) => {
                let current = definition.fingerprint();
                if current != recorded {
                    errors.push(ReleasedError::Changed {
                        version,
                        recorded: recorded.to_string(),
                        current,
                    });
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stages_are_the_eight_of_note_006_in_order() {
        let ids: Vec<&str> = V1.stages.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            names::STAGES,
            "the definition's stage list equals the identifiers (plugin-api's, from WP13)"
        );
    }

    #[test]
    fn the_spaces_chain_from_the_sensor_to_the_display() {
        assert_eq!(V1.stages[0].input, DataSpace::SensorRaw);
        assert_eq!(
            V1.stages.last().expect("stages").output,
            DataSpace::DisplayReferred
        );
        for pair in V1.stages.windows(2) {
            assert_eq!(
                pair[0].output, pair[1].input,
                "stage {} returns {} but stage {} receives {}",
                pair[0].id, pair[0].output, pair[1].id, pair[1].input
            );
        }
    }

    #[test]
    fn the_five_spaces_are_named_as_the_constants_and_refine_d142s_three() {
        let names: Vec<&str> = DataSpace::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names, super::names::SPACES);
        for space in DataSpace::ALL {
            assert_eq!(DataSpace::from_name(space.name()), Some(space));
        }
        assert_eq!(
            DataSpace::from_name("camera-rgb"),
            None,
            "a stage name is not a space"
        );
        assert_eq!(DataSpace::MosaicLinear.family(), SpaceFamily::Raw);
        assert_eq!(DataSpace::CameraLinear.family(), SpaceFamily::SceneLinear);
        assert_eq!(DataSpace::WorkingLinear.family(), SpaceFamily::SceneLinear);
        assert_eq!(
            DataSpace::DisplayReferred.family(),
            SpaceFamily::DisplayReferred
        );
    }

    #[test]
    fn a_stage_and_a_space_never_share_a_name() {
        // "an operation in camera-linear" must say one thing (review of note 006).
        for stage in names::STAGES {
            assert!(
                !names::SPACES.contains(&stage),
                "{stage} is both a stage and a space"
            );
        }
    }

    #[test]
    fn the_builtin_operations_are_unique_and_namespaced() {
        let mut seen = std::collections::BTreeSet::new();
        for stage in V1.stages {
            for operation in stage.operations {
                assert!(operation.starts_with("auroraw."), "{operation}");
                assert!(seen.insert(*operation), "{operation} is in two places");
            }
        }
        assert!(seen.contains("auroraw.white-balance"));
    }

    #[test]
    fn white_balance_comes_after_the_denoiser_and_highlights_after_the_balance() {
        // The decision of note 006 §4.6 and the rule of dependence of §3.4, as data.
        let stage_of = |op: &str| V1.builtin(op).expect("a built-in").0;
        assert!(stage_of("auroraw.noise-reduction") < stage_of("auroraw.white-balance"));
        let (stage, wb) = V1.builtin("auroraw.white-balance").expect("a built-in");
        let (hr_stage, hr) = V1
            .builtin("auroraw.highlight-reconstruction")
            .expect("a built-in");
        assert_eq!(stage, hr_stage);
        assert!(
            wb < hr,
            "highlight reconstruction reads the balance, so it comes after it"
        );
        assert!(
            stage_of("auroraw.crop") < stage_of("auroraw.sharpening"),
            "geometry before detail"
        );
    }

    #[test]
    fn an_operation_reads_the_space_its_stage_delivers_to_it() {
        // Operations read what the stage receives, except in `raw-linear`, where the levels (the spine,
        // before the operations) turn counts into mosaic values first.
        for stage in V1.stages {
            let expected = if stage.id == names::RAW_LINEAR {
                stage.output
            } else {
                stage.input
            };
            assert_eq!(stage.operations_read, expected, "stage {}", stage.id);
        }
    }

    #[test]
    fn the_fingerprint_is_stable_and_a_blake3_hex_digest() {
        let a = V1.fingerprint();
        assert_eq!(a, V1.fingerprint());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ---- the fingerprint sees every part of a definition ----

    const SWAPPED_STAGES: &[Stage] = &[
        V1_STAGES[1],
        V1_STAGES[0],
        V1_STAGES[2],
        V1_STAGES[3],
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        V1_STAGES[7],
    ];
    const DROPPED_SPINE: &[Stage] = &[
        Stage {
            spine: &[],
            ..V1_STAGES[0]
        },
        V1_STAGES[1],
        V1_STAGES[2],
        V1_STAGES[3],
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        V1_STAGES[7],
    ];
    const MOVED_SPINE: &[Stage] = &[
        V1_STAGES[0],
        V1_STAGES[1],
        V1_STAGES[2],
        Stage {
            spine: &[spine("camera-to-working", Runs::BeforeOperations)],
            ..V1_STAGES[3]
        },
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        V1_STAGES[7],
    ];
    const RENAMED_SPINE: &[Stage] = &[
        V1_STAGES[0],
        V1_STAGES[1],
        V1_STAGES[2],
        V1_STAGES[3],
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        Stage {
            spine: &[spine("output-transform-2", Runs::AfterOperations)],
            ..V1_STAGES[7]
        },
    ];
    const REORDERED_OPERATIONS: &[Stage] = &[
        V1_STAGES[0],
        V1_STAGES[1],
        V1_STAGES[2],
        Stage {
            operations: &["auroraw.highlight-reconstruction", "auroraw.white-balance"],
            ..V1_STAGES[3]
        },
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        V1_STAGES[7],
    ];
    const OTHER_SPACE: &[Stage] = &[
        V1_STAGES[0],
        V1_STAGES[1],
        Stage {
            operations_read: DataSpace::WorkingLinear,
            ..V1_STAGES[2]
        },
        V1_STAGES[3],
        V1_STAGES[4],
        V1_STAGES[5],
        V1_STAGES[6],
        V1_STAGES[7],
    ];

    #[test]
    fn the_fingerprint_changes_with_every_part_of_the_definition() {
        let base = V1.fingerprint();
        let variants = [
            ("the version", Definition { version: 2, ..V1 }),
            (
                "the working space",
                Definition {
                    working_space: "prophoto-linear-d50",
                    ..V1
                },
            ),
            (
                "the order of the stages",
                Definition {
                    stages: SWAPPED_STAGES,
                    ..V1
                },
            ),
            (
                "a spine step removed",
                Definition {
                    stages: DROPPED_SPINE,
                    ..V1
                },
            ),
            (
                "a spine step moved to the other side of the operations",
                Definition {
                    stages: MOVED_SPINE,
                    ..V1
                },
            ),
            (
                "a spine step renamed",
                Definition {
                    stages: RENAMED_SPINE,
                    ..V1
                },
            ),
            (
                "the canonical order",
                Definition {
                    stages: REORDERED_OPERATIONS,
                    ..V1
                },
            ),
            (
                "a space an operation reads",
                Definition {
                    stages: OTHER_SPACE,
                    ..V1
                },
            ),
        ];
        let mut seen = std::collections::BTreeSet::from([base.clone()]);
        for (what, definition) in variants {
            let fingerprint = definition.fingerprint();
            assert_ne!(fingerprint, base, "the fingerprint does not see {what}");
            assert!(
                seen.insert(fingerprint),
                "two variants share a fingerprint: {what}"
            );
        }
    }

    #[test]
    fn nothing_is_released_yet_and_every_released_version_still_matches() {
        assert!(
            RELEASED.is_empty(),
            "when a version is released its fingerprint goes in RELEASED"
        );
        assert_eq!(verify_released(RELEASED, by_version), Ok(()));
    }

    #[test]
    fn a_released_definition_that_was_edited_is_reported() {
        // The mechanism, on a version that pretends to be released with another definition's fingerprint.
        let recorded = Definition {
            stages: SWAPPED_STAGES,
            ..V1
        }
        .fingerprint();
        let released = [(1, recorded.as_str())];
        let errors =
            verify_released(&released, by_version).expect_err("an edited release is caught");
        assert!(
            matches!(
                errors.as_slice(),
                [ReleasedError::Changed { version: 1, .. }]
            ),
            "{errors:?}"
        );
        // And a released version the engine no longer knows.
        let errors =
            verify_released(&[(7, "00")], by_version).expect_err("a missing version is caught");
        assert_eq!(errors, vec![ReleasedError::Missing(7)]);
        // Recorded with its true fingerprint, it passes.
        let truth = V1.fingerprint();
        assert_eq!(verify_released(&[(1, truth.as_str())], by_version), Ok(()));
    }
}
