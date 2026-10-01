// SPDX-License-Identifier: GPL-3.0-or-later
//! The colour arithmetic of the first stages: the matrices between the camera, the working space and
//! the display (design note 006 §3.3, §5).
//!
//! The working space is **linear Rec.2020 primaries with the D65 white**, as architecture §6.5 and the
//! M2 plan propose, **provisionally**: the plan asks that it be confirmed against linear ProPhoto RGB by
//! measurement before the colour operations are written against it (note 006 §5). Changing it is a new
//! pipeline-definition version; only this table and the matrices derived from it would change.
//!
//! The matrices are **derived from the chromaticities**, in double precision, not copied as rounded
//! numbers, so that the white point maps exactly to white and the tests can say so.

/// A 3x3 matrix, row-major, acting on a column vector of linear RGB.
pub(crate) type Matrix3 = [[f32; 3]; 3];

/// CIE 1931 chromaticities of a set of RGB primaries and of its white.
struct Primaries {
    red: [f64; 2],
    green: [f64; 2],
    blue: [f64; 2],
    white: [f64; 2],
}

/// ITU-R BT.2020, the working space (D65 white).
const WORKING: Primaries = Primaries {
    red: [0.708, 0.292],
    green: [0.170, 0.797],
    blue: [0.131, 0.046],
    white: [0.3127, 0.3290],
};

/// sRGB (IEC 61966-2-1), the display space of the first output (D65 white).
const SRGB: Primaries = Primaries {
    red: [0.64, 0.33],
    green: [0.30, 0.60],
    blue: [0.15, 0.06],
    white: [0.3127, 0.3290],
};

type M64 = [[f64; 3]; 3];

fn mul(a: &M64, b: &M64) -> M64 {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// The inverse of a 3x3 matrix by its cofactors, or `None` if it is singular.
fn inverse(m: &M64) -> Option<M64> {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let k = 1.0 / det;
    Some([
        [
            (e * i - f * h) * k,
            (c * h - b * i) * k,
            (b * f - c * e) * k,
        ],
        [
            (f * g - d * i) * k,
            (a * i - c * g) * k,
            (c * d - a * f) * k,
        ],
        [
            (d * h - e * g) * k,
            (b * g - a * h) * k,
            (a * e - b * d) * k,
        ],
    ])
}

fn xyz_of(xy: [f64; 2]) -> [f64; 3] {
    [xy[0] / xy[1], 1.0, (1.0 - xy[0] - xy[1]) / xy[1]]
}

/// The matrix from linear RGB of these primaries to CIE XYZ, scaled so that RGB (1, 1, 1) is the white.
fn rgb_to_xyz(p: &Primaries) -> M64 {
    let (r, g, b) = (xyz_of(p.red), xyz_of(p.green), xyz_of(p.blue));
    let white = xyz_of(p.white);
    let columns = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let inv = inverse(&columns).expect("primaries that are not collinear");
    let scale: [f64; 3] =
        std::array::from_fn(|i| inv[i][0] * white[0] + inv[i][1] * white[1] + inv[i][2] * white[2]);
    std::array::from_fn(|row| std::array::from_fn(|col| columns[row][col] * scale[col]))
}

fn to_f32(m: &M64) -> Matrix3 {
    std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] as f32))
}

/// Working space (linear Rec.2020) to display (linear sRGB): what the output stage applies before the
/// transfer function.
pub(crate) fn working_to_display() -> Matrix3 {
    let to_display = inverse(&rgb_to_xyz(&SRGB)).expect("sRGB primaries invert");
    to_f32(&mul(&to_display, &rgb_to_xyz(&WORKING)))
}

/// Camera RGB to working space, from the camera's `XYZ to camera` matrix, **by dcraw's method**: the
/// matrix from the working space to the camera is the product of the camera's matrix and the working
/// space's `RGB to XYZ`; its rows are scaled to sum to one, so that a neutral stays neutral (white
/// balance having been applied, the working space's (1, 1, 1) is the camera's (1, 1, 1)); the result is
/// its inverse. `None` if the camera's matrix is degenerate, in which case the caller has no colour
/// interpretation to give.
pub(crate) fn camera_to_working(xyz_to_camera: &Matrix3) -> Option<Matrix3> {
    let xyz_to_cam: M64 =
        std::array::from_fn(|i| std::array::from_fn(|j| f64::from(xyz_to_camera[i][j])));
    let mut working_to_cam = mul(&xyz_to_cam, &rgb_to_xyz(&WORKING));
    for row in &mut working_to_cam {
        let sum: f64 = row.iter().sum();
        if sum.abs() < 1e-9 {
            return None;
        }
        row.iter_mut().for_each(|v| *v /= sum);
    }
    inverse(&working_to_cam).map(|m| to_f32(&m))
}

/// Folds the white balance multipliers into the camera-to-working matrix: `M · diag(m)`. White balance
/// is a per-channel multiplication of the camera RGB, so it folds exactly (note 006 §2.2, the control
/// column), and the `input-colour` stage does one 3x3 multiplication whatever the balance is.
pub(crate) fn fold_white_balance(camera_to_working: &Matrix3, multipliers: [f32; 3]) -> Matrix3 {
    std::array::from_fn(|i| std::array::from_fn(|j| camera_to_working[i][j] * multipliers[j]))
}

/// The matrix of a camera that sees exactly what the working space sees: nothing to interpret. Used by
/// the scenes of the tests and for a camera with no colour data.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const IDENTITY: Matrix3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &Matrix3, v: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    }

    #[test]
    fn working_to_display_matches_the_published_rec2020_to_srgb_matrix() {
        // The matrix as published (ITU-R BT.2407 / the usual tables), to four decimals.
        let published = [
            [1.6605, -0.5876, -0.0728],
            [-0.1246, 1.1329, -0.0083],
            [-0.0182, -0.1006, 1.1187],
        ];
        let derived = working_to_display();
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (derived[i][j] - published[i][j]).abs() < 6e-4,
                    "[{i}][{j}] {} against {}",
                    derived[i][j],
                    published[i][j]
                );
            }
        }
    }

    #[test]
    fn the_working_white_is_the_display_white() {
        let white = apply(&working_to_display(), [1.0, 1.0, 1.0]);
        for c in white {
            assert!((c - 1.0).abs() < 1e-5, "{white:?}");
        }
    }

    #[test]
    fn a_neutral_stays_neutral_through_the_camera_matrix() {
        // A camera matrix invented for this test, in the range real ones have (not a real camera's).
        let xyz_to_cam = [
            [1.1285, -0.5003, -0.0897],
            [-0.5039, 1.2559, 0.1346],
            [-0.0432, 0.1655, 0.5774],
        ];
        let m = camera_to_working(&xyz_to_cam).expect("a usable matrix");
        let neutral = apply(&m, [1.0, 1.0, 1.0]);
        for c in neutral {
            assert!((c - 1.0).abs() < 1e-4, "{neutral:?}");
        }
        // And with the multipliers folded in, the camera's own neutral (1/m) is the working white.
        let wb = [1.9, 1.0, 1.38];
        let folded = fold_white_balance(&m, wb);
        let white = apply(&folded, [1.0 / wb[0], 1.0, 1.0 / wb[2]]);
        for c in white {
            assert!((c - 1.0).abs() < 1e-4, "{white:?}");
        }
    }

    #[test]
    fn a_degenerate_camera_matrix_gives_no_interpretation() {
        let zero = [[0.0; 3]; 3];
        assert_eq!(camera_to_working(&zero), None);
        // A camera whose rows are linearly dependent has no inverse.
        let flat = [[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];
        assert_eq!(camera_to_working(&flat), None);
    }

    #[test]
    fn folding_unit_multipliers_changes_nothing() {
        let m = [[0.9, 0.1, 0.0], [0.05, 0.9, 0.05], [0.0, 0.2, 0.8]];
        assert_eq!(fold_white_balance(&m, [1.0; 3]), m);
        assert_eq!(IDENTITY, fold_white_balance(&IDENTITY, [1.0; 3]));
    }
}
