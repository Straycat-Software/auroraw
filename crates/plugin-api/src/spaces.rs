// SPDX-License-Identifier: MIT OR Apache-2.0
//! The data spaces of the develop pipeline, as a plugin names them (decision D-142, design note 006 §3.2).
//!
//! An operation declares the space of the data it reads and the space of the data it writes
//! ([`Declaration::input_space`](crate::Declaration::input_space)), because whether it is correct depends on it: a
//! white balance on values that already have a white point is a mistake the declaration can catch. These are **the five
//! spaces of definition v1**, in the order the chain passes through them; the three families of D-142 (raw, scene-linear,
//! display-referred) are refined by them. A new space is a new API version. The pipeline crate holds the same table in
//! its definition and a test there checks that the two are equal.

/// The decoder's samples, as counts, the levels not yet applied.
pub const SENSOR_RAW: &str = "sensor-raw";
/// One linear value per photosite, levels applied, white at 1.0, the colour filter still present.
pub const MOSAIC_LINEAR: &str = "mosaic-linear";
/// Linear RGB per pixel in the camera's primaries, no upper bound, white balance not yet applied.
pub const CAMERA_LINEAR: &str = "camera-linear";
/// Linear RGB per pixel in the working space's primaries, scene-referred, no upper bound.
pub const WORKING_LINEAR: &str = "working-linear";
/// Values in the output space's encoding, bounded to 0..1.
pub const DISPLAY_REFERRED: &str = "display-referred";

/// The five spaces, in the order the chain passes through them.
pub const ALL: [&str; 5] = [
    SENSOR_RAW,
    MOSAIC_LINEAR,
    CAMERA_LINEAR,
    WORKING_LINEAR,
    DISPLAY_REFERRED,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The strings are what a plugin writes in its declaration (`input_space`, `output_space`) and what the pipeline
    /// definition's fingerprint is made of, so they are held here **as literals**, not as the constants (see the stages').
    #[test]
    fn the_five_spaces_are_named_as_declarations_write_them() {
        assert_eq!(
            ALL,
            [
                "sensor-raw",
                "mosaic-linear",
                "camera-linear",
                "working-linear",
                "display-referred"
            ]
        );
    }
}
