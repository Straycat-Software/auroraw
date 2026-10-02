// SPDX-License-Identifier: MIT OR Apache-2.0
//! The stages of the develop pipeline, as a plugin names them (decision D-142, design note 006 §3.2 and §7).
//!
//! A [`Placement`](crate::Placement) says at which stage an operation runs, so the identifiers are part of the plugin
//! surface, and they are here and not in the pipeline crate. These are **the eight stages of definition v1**, in the
//! order the chain passes through them; a later definition version can add stages, and `develop` refuses a declaration
//! that names one the definition it renders with does not have. The pipeline crate holds the same table in its own
//! definition (it is what the definition's fingerprint is made of) and a test there checks that the two are equal.

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
pub const ALL: [&str; 8] = [
    RAW_LINEAR,
    DEMOSAIC,
    CAMERA_RGB,
    INPUT_COLOUR,
    SCENE_LINEAR,
    GEOMETRY,
    DETAIL,
    DISPLAY,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The strings are what a plugin writes in its declaration (`placement.stage`) and what the pipeline definition's
    /// fingerprint is made of, so they are held here **as literals**, not as the constants: changing a name is a decision
    /// that edits this test, and after the definition's release it is a new definition version.
    #[test]
    fn the_eight_stages_are_named_as_declarations_write_them() {
        assert_eq!(
            ALL,
            [
                "raw-linear",
                "demosaic",
                "camera-rgb",
                "input-colour",
                "scene-linear",
                "geometry",
                "detail",
                "display"
            ]
        );
    }
}
