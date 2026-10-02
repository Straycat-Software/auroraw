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
//! them), so they are `plugin-api`'s constants (`stages`, `spaces`), which this module is made of: the
//! definition and its fingerprint are made of the same strings a plugin writes, and there is no second table to
//! keep equal. What a test still holds is the **order**: the definition's stages are `stages::ALL`, and its
//! [`DataSpace`]s are `spaces::ALL`.

use std::fmt;

use auroraw_plugin_api::{spaces, stages};

/// A working space: linear RGB with these primaries and this white, as CIE 1931 `xy` chromaticities.
///
/// The definition holds **the numbers, not a label**: the maths of the working space is a reason for a new
/// version (note 006 §3.6), so the fingerprint must see it, and the matrices of the colour stages are derived
/// from these chromaticities and not from a second copy of them (review of this definition).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkingSpace {
    /// A name for reports; it is in the fingerprint with the numbers.
    pub name: &'static str,
    /// The red primary.
    pub red: [f64; 2],
    /// The green primary.
    pub green: [f64; 2],
    /// The blue primary.
    pub blue: [f64; 2],
    /// The white point.
    pub white: [f64; 2],
}

/// Linear Rec.2020 primaries (ITU-R BT.2020) with the D65 white: the working space of definition v1,
/// **provisional** until the ProPhoto measurement (note 006 §5).
pub const REC2020_D65: WorkingSpace = WorkingSpace {
    name: "rec2020-linear-d65",
    red: [0.708, 0.292],
    green: [0.170, 0.797],
    blue: [0.131, 0.046],
    white: [0.3127, 0.3290],
};

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
            DataSpace::SensorRaw => spaces::SENSOR_RAW,
            DataSpace::MosaicLinear => spaces::MOSAIC_LINEAR,
            DataSpace::CameraLinear => spaces::CAMERA_LINEAR,
            DataSpace::WorkingLinear => spaces::WORKING_LINEAR,
            DataSpace::DisplayReferred => spaces::DISPLAY_REFERRED,
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
    /// The stage's identifier (one of [`auroraw_plugin_api::stages`]).
    pub id: &'static str,
    /// The space the stage receives.
    pub input: DataSpace,
    /// The space the stage returns.
    pub output: DataSpace,
    /// What the definition always does in this stage.
    pub spine: &'static [SpineStep],
    /// The built-in operations of this stage, in their **canonical order** (note 006 §3.4): their
    /// identifiers are provisional until work package 15 fixes them.
    pub operations: &'static [&'static str],
}

impl Stage {
    /// The space an operation of this stage reads and returns, **derived from the spine** so that there is
    /// one source of truth (review of this definition): where the stage changes the space and its spine
    /// runs **before** the operations (the levels, in `raw-linear`), the operations read what the spine
    /// returns; otherwise they read what the stage receives (the camera-to-working matrix and the output
    /// transform run after the operations, which therefore still see the input space).
    pub fn operations_read(&self) -> DataSpace {
        let spine_changes_it_first = self.input != self.output
            && self.spine.iter().any(|s| s.runs == Runs::BeforeOperations);
        if spine_changes_it_first {
            self.output
        } else {
            self.input
        }
    }
}

/// A pipeline definition: an immutable, versioned value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Definition {
    /// The version, which enters every recipe's hash (note 006 §3.6).
    pub version: u32,
    /// The working space, part of the definition: changing it is a new version.
    pub working_space: WorkingSpace,
    /// The stages, in order.
    pub stages: &'static [Stage],
}

const fn spine(name: &'static str, runs: Runs) -> SpineStep {
    SpineStep { name, runs }
}

const V1_STAGES: &[Stage] = &[
    Stage {
        id: stages::RAW_LINEAR,
        input: DataSpace::SensorRaw,
        output: DataSpace::MosaicLinear,
        spine: &[spine("levels", Runs::BeforeOperations)],
        operations: &["auroraw.hot-pixels"],
    },
    Stage {
        id: stages::DEMOSAIC,
        input: DataSpace::MosaicLinear,
        output: DataSpace::CameraLinear,
        // The operations of this stage, none in M2, would be on the mosaic before the interpolation; the
        // demosaic itself runs after them, so they read what the stage receives.
        spine: &[spine("demosaic", Runs::AfterOperations)],
        operations: &[],
    },
    Stage {
        id: stages::CAMERA_RGB,
        input: DataSpace::CameraLinear,
        output: DataSpace::CameraLinear,
        spine: &[],
        operations: &["auroraw.noise-reduction"],
    },
    Stage {
        id: stages::INPUT_COLOUR,
        input: DataSpace::CameraLinear,
        output: DataSpace::WorkingLinear,
        spine: &[spine("camera-to-working", Runs::AfterOperations)],
        operations: &["auroraw.white-balance", "auroraw.highlight-reconstruction"],
    },
    Stage {
        id: stages::SCENE_LINEAR,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
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
        id: stages::GEOMETRY,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
        spine: &[spine("orientation-and-crop", Runs::BeforeOperations)],
        operations: &["auroraw.crop", "auroraw.straighten"],
    },
    Stage {
        id: stages::DETAIL,
        input: DataSpace::WorkingLinear,
        output: DataSpace::WorkingLinear,
        spine: &[],
        operations: &["auroraw.sharpening"],
    },
    Stage {
        id: stages::DISPLAY,
        input: DataSpace::WorkingLinear,
        output: DataSpace::DisplayReferred,
        spine: &[spine("output-transform", Runs::AfterOperations)],
        operations: &["auroraw.tone-map"],
    },
];

/// The definition v1 (note 006 §3.3): eight stages. **Not released**: see the module documentation.
pub const V1: Definition = Definition {
    version: 1,
    working_space: REC2020_D65,
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
        text(&mut out, self.working_space.name);
        for v in self
            .working_space
            .red
            .iter()
            .chain(&self.working_space.green)
            .chain(&self.working_space.blue)
            .chain(&self.working_space.white)
        {
            out.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        out.extend_from_slice(&(self.stages.len() as u32).to_le_bytes());
        for stage in self.stages {
            text(&mut out, stage.id);
            text(&mut out, stage.input.name());
            text(&mut out, stage.output.name());
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
            stages::ALL,
            "the definition's stage list is the identifiers of plugin-api, in order"
        );
    }

    /// D-146: a declaration names a stage and a space with `plugin-api`'s constants, and the definition is made of those
    /// same constants (there is no second table), so what is left to hold is that the definition **covers** them: it
    /// has every stage a plugin can name and understands every space, in the order the chain passes through them.
    #[test]
    fn the_definition_has_every_stage_and_space_plugin_api_names() {
        for stage in stages::ALL {
            assert!(V1.stage_index(stage).is_some(), "{stage}");
        }
        let space_names: Vec<&str> = DataSpace::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(space_names, spaces::ALL, "the definition's spaces");
        for name in spaces::ALL {
            assert!(DataSpace::from_name(name).is_some(), "{name}");
        }
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
        assert_eq!(names, spaces::ALL);
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
        for stage in stages::ALL {
            assert!(
                !spaces::ALL.contains(&stage),
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
        // Written from note 006 §3.3, stage by stage, and not derived by the rule under test: the levels run
        // first in `raw-linear`, so its operations read mosaic values; the demosaic, the camera-to-working
        // matrix and the output transform run after the operations of their stages, which read what the
        // stage receives.
        let expected = [
            (stages::RAW_LINEAR, DataSpace::MosaicLinear),
            (stages::DEMOSAIC, DataSpace::MosaicLinear),
            (stages::CAMERA_RGB, DataSpace::CameraLinear),
            (stages::INPUT_COLOUR, DataSpace::CameraLinear),
            (stages::SCENE_LINEAR, DataSpace::WorkingLinear),
            (stages::GEOMETRY, DataSpace::WorkingLinear),
            (stages::DETAIL, DataSpace::WorkingLinear),
            (stages::DISPLAY, DataSpace::WorkingLinear),
        ];
        for (id, space) in expected {
            let stage = V1.stages[V1.stage_index(id).expect("a stage")];
            assert_eq!(stage.operations_read(), space, "stage {id}");
        }
    }

    #[test]
    fn a_stage_never_contradicts_itself_about_its_operations() {
        // The review's case: a spine step that runs first and changes the space cannot sit in a stage whose
        // operations are said to read the space before it. With one derived value there is nothing to
        // contradict; what is left to check is that the value is always one of the stage's two spaces.
        for stage in V1.stages {
            let read = stage.operations_read();
            assert!(
                read == stage.input || read == stage.output,
                "stage {}",
                stage.id
            );
            if read == stage.output && stage.input != stage.output {
                assert!(
                    stage.spine.iter().any(|s| s.runs == Runs::BeforeOperations),
                    "stage {} reads its output but no spine step runs first",
                    stage.id
                );
            }
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
    const OTHER_OUTPUT_SPACE: &[Stage] = &[
        V1_STAGES[0],
        V1_STAGES[1],
        Stage {
            output: DataSpace::WorkingLinear,
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
                "the working space's name",
                Definition {
                    working_space: WorkingSpace {
                        name: "prophoto-linear-d50",
                        ..REC2020_D65
                    },
                    ..V1
                },
            ),
            (
                "one chromaticity of the working space (the maths, with its name unchanged)",
                Definition {
                    working_space: WorkingSpace {
                        red: [0.7081, 0.292],
                        ..REC2020_D65
                    },
                    ..V1
                },
            ),
            (
                "the white point of the working space",
                Definition {
                    working_space: WorkingSpace {
                        white: [0.3457, 0.3585],
                        ..REC2020_D65
                    },
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
                "a space a stage returns",
                Definition {
                    stages: OTHER_OUTPUT_SPACE,
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

    /// A definition of one stage and one operation, small enough to read, whose fingerprint is pinned.
    const TINY_STAGES: &[Stage] = &[Stage {
        id: "only",
        input: DataSpace::SensorRaw,
        output: DataSpace::DisplayReferred,
        spine: &[SpineStep {
            name: "everything",
            runs: Runs::AfterOperations,
        }],
        operations: &["tiny.op"],
    }];
    const TINY: Definition = Definition {
        version: 1,
        working_space: REC2020_D65,
        stages: TINY_STAGES,
    };

    #[test]
    fn the_encoding_itself_is_pinned_by_a_tiny_definition() {
        // `verify_released` protects the definitions, and the pinned digest below protects the **encoding**
        // that computes their fingerprints: if someone tidies `canonical_bytes`, every released fingerprint
        // would change at once. If this test fails, the encoding changed: that is a decision, not a cleanup,
        // since it invalidates `RELEASED`. Update the digest only together with that table. (It was pinned from
        // the encoding as it stood when the review asked for it: it holds the encoding still, it does not prove
        // it is the right one.)
        assert_eq!(
            TINY.fingerprint(),
            "67fdb3fb2c31e93d07ac75ff4a382c40d1d37f38439922c4a7a58b7fb1328c1c",
            "the canonical encoding of a definition changed"
        );
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
