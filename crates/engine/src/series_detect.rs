// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding objective series (spec §5.3, D-034, D-101): photos of one camera taken within a few seconds of one
//! another. Pure: it reads what the catalogue lists (`SeriesCandidate`, already ordered by camera, then time,
//! then name) and says which photos go together and what kind of series each is. What is done with the answer
//! (writing the files, the rows, the events) is the coordinator's.

use auroraw_catalogue::SeriesCandidate;
use auroraw_types::PhotoId;

/// The most photos one series holds: a long stream of shots, each within the gap of the last (a walk in the
/// street), is cut into series of this size instead of becoming one.
pub const MAX_SERIES: usize = 100;

/// The default gap, in seconds, between two photos of one series (D-084).
pub const DEFAULT_GAP: u32 = 2;

/// What a detected series is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    /// Frames of a burst.
    Burst,
    /// An exposure bracket: same aperture, ISO and focal length, three or more different exposure times.
    Bracket,
}

impl Detected {
    /// The kind as the series file writes it.
    pub fn kind(self) -> &'static str {
        match self {
            Detected::Burst => "burst",
            Detected::Bracket => "bracket",
        }
    }
}

/// A series found: its photos in capture order, and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The photos, in the order they were taken.
    pub members: Vec<PhotoId>,
    /// What it is.
    pub kind: Detected,
}

/// Groups `candidates` (ordered by camera, then capture time, then name) into series: consecutive photos of one
/// camera at most `gap` seconds apart, at least two, at most [`MAX_SERIES`].
pub fn detect(candidates: &[SeriesCandidate], gap: u32) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut current: Vec<&SeriesCandidate> = Vec::new();
    for candidate in candidates {
        let joins = current.last().is_some_and(|last| {
            last.camera == candidate.camera
                && candidate.capture_time - last.capture_time <= i64::from(gap)
                && current.len() < MAX_SERIES
        });
        if !joins {
            close(&mut current, &mut groups);
        }
        current.push(candidate);
    }
    close(&mut current, &mut groups);
    groups
}

fn close(current: &mut Vec<&SeriesCandidate>, groups: &mut Vec<Group>) {
    if current.len() >= 2 {
        groups.push(Group {
            members: current.iter().map(|c| c.id).collect(),
            kind: if is_bracket(current) {
                Detected::Bracket
            } else {
                Detected::Burst
            },
        });
    }
    current.clear();
}

/// Three or more photos with one aperture, one ISO and one focal length whose exposure times are not all the same
/// (at least three different ones): a bracket. Anything less clear is a burst.
fn is_bracket(photos: &[&SeriesCandidate]) -> bool {
    if photos.len() < 3 {
        return false;
    }
    let first = photos[0];
    let same = |f: &dyn Fn(&SeriesCandidate) -> Option<f64>| {
        f(first).is_some() && photos.iter().all(|p| f(p) == f(first))
    };
    if !(same(&|p| p.aperture) && same(&|p| p.iso.map(|i| i as f64)) && same(&|p| p.focal_length)) {
        return false;
    }
    let mut times: Vec<i64> = photos
        .iter()
        .filter_map(|p| p.shutter)
        // Distinct at a millionth of a second, whatever the rounding of the file's rational.
        .map(|s| (s * 1_000_000.0).round() as i64)
        .collect();
    if times.len() < photos.len() {
        return false;
    }
    times.sort_unstable();
    times.dedup();
    times.len() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(n: u8, time: i64, camera: Option<i64>) -> SeriesCandidate {
        SeriesCandidate {
            id: PhotoId::from_bytes([n; 16]),
            capture_time: time,
            camera,
            iso: Some(100),
            aperture: Some(5.6),
            shutter: Some(1.0 / 250.0),
            focal_length: Some(50.0),
            filename: format!("IMG_{n:04}.jpg"),
        }
    }

    fn ids(group: &Group) -> Vec<u8> {
        group.members.iter().map(|id| id.as_bytes()[0]).collect()
    }

    #[test]
    fn photos_within_the_gap_form_a_series_and_the_gap_itself_is_inside() {
        let shots = [
            shot(1, 100, None),
            shot(2, 102, None),
            shot(3, 104, None),
            shot(4, 107, None),
        ];
        let groups = detect(&shots, 2);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            ids(&groups[0]),
            [1, 2, 3],
            "two seconds apart is still one series"
        );
        assert_eq!(groups[0].kind, Detected::Burst);
        // One second more of gap joins the fourth as well.
        let wider = detect(&shots, 3);
        assert_eq!(ids(&wider[0]), [1, 2, 3, 4]);
        // A lone photo is not a series, and a gap of 0 groups only photos of the same second.
        assert!(detect(&[shot(1, 100, None)], 2).is_empty());
        let same_second = detect(
            &[shot(1, 100, None), shot(2, 100, None), shot(3, 101, None)],
            0,
        );
        assert_eq!(same_second.len(), 1);
        assert_eq!(ids(&same_second[0]), [1, 2]);
    }

    #[test]
    fn two_cameras_never_share_a_series_and_the_order_is_the_catalogues() {
        // The catalogue lists camera by camera, so each camera's run is contiguous.
        let shots = [
            shot(1, 100, Some(1)),
            shot(2, 101, Some(1)),
            shot(3, 100, Some(2)),
            shot(4, 101, Some(2)),
        ];
        let groups = detect(&shots, 2);
        assert_eq!(groups.len(), 2);
        assert_eq!(ids(&groups[0]), [1, 2]);
        assert_eq!(ids(&groups[1]), [3, 4]);
        // An unknown camera is one camera of its own.
        let unknown = detect(
            &[
                shot(1, 100, None),
                shot(2, 101, None),
                shot(3, 102, Some(1)),
            ],
            2,
        );
        assert_eq!(unknown.len(), 1);
        assert_eq!(ids(&unknown[0]), [1, 2]);
    }

    #[test]
    fn a_long_stream_is_cut_into_series_of_at_most_the_cap() {
        let shots: Vec<_> = (0..250u32)
            .map(|i| shot((i % 250) as u8, 1000 + i64::from(i), None))
            .collect();
        let groups = detect(&shots, 2);
        let sizes: Vec<usize> = groups.iter().map(|g| g.members.len()).collect();
        assert_eq!(sizes, [MAX_SERIES, MAX_SERIES, 50]);
    }

    #[test]
    fn a_bracket_has_one_aperture_iso_and_focal_length_and_three_exposure_times() {
        let mut shots = [shot(1, 100, None), shot(2, 101, None), shot(3, 102, None)];
        shots[0].shutter = Some(1.0 / 500.0);
        shots[1].shutter = Some(1.0 / 125.0);
        shots[2].shutter = Some(1.0 / 30.0);
        assert_eq!(detect(&shots, 2)[0].kind, Detected::Bracket);
        // The same exposure three times is a burst.
        let burst = [shot(1, 100, None), shot(2, 101, None), shot(3, 102, None)];
        assert_eq!(detect(&burst, 2)[0].kind, Detected::Burst);
        // Two exposure times are not enough, and a different aperture is not a bracket.
        let mut two = shots.clone();
        two[2].shutter = two[0].shutter;
        assert_eq!(detect(&two, 2)[0].kind, Detected::Burst);
        let mut other_aperture = shots.clone();
        other_aperture[1].aperture = Some(8.0);
        assert_eq!(detect(&other_aperture, 2)[0].kind, Detected::Burst);
        // What the file did not say cannot make a bracket.
        let mut unknown = shots.clone();
        unknown[1].shutter = None;
        assert_eq!(detect(&unknown, 2)[0].kind, Detected::Burst);
        let mut no_aperture = shots.clone();
        for s in &mut no_aperture {
            s.aperture = None;
        }
        assert_eq!(detect(&no_aperture, 2)[0].kind, Detected::Burst);
    }
}
