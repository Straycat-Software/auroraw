// SPDX-License-Identifier: GPL-3.0-or-later
//! What the grid lists (WP9, D-101): the photos the filters let through, and how series show among them. A
//! **collapsed** series is one row, at the place of its first listed member, that stands for all its members
//! (`Item::members`); an **expanded** one lists its members in place, joined by an edge. Pure, so it is tested
//! without Qt; `PhotoGrid` keeps the full list and asks this for the rows.

use std::collections::{HashMap, HashSet};

use auroraw_catalogue::SeriesInfo;
use auroraw_types::{PhotoId, SeriesId};

/// One photo as the grid shows it, or one collapsed series.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: PhotoId,
    pub rating: u8,
    /// The effective flag: 0 none, 1 picked, 2 rejected.
    pub flag: u8,
    /// The colour label: 0 none, else the colour's place in `ColourLabel::ALL` plus one.
    pub label: u8,
    /// The series the photo is in.
    pub series: Option<SeriesId>,
    /// For a **collapsed** series' row: every member that is listed (the row stands for all of them), in the
    /// order they are listed. Empty for a photo.
    pub members: Vec<PhotoId>,
    /// How many members are listed (1 when the filters hide the others).
    pub series_size: u32,
    /// How many members the series has.
    pub series_total: u32,
    pub series_resolved: bool,
    /// Whether the series is shown expanded.
    pub series_open: bool,
    /// Where a member of an expanded series lies in its run: 0 none, 1 first, 2 middle, 3 last.
    pub series_edge: u8,
}

impl Item {
    pub fn photo(id: PhotoId, rating: u8, flag: u8, label: u8, series: Option<SeriesId>) -> Self {
        Self {
            id,
            rating,
            flag,
            label,
            series,
            members: Vec::new(),
            series_size: 0,
            series_total: 0,
            series_resolved: false,
            series_open: false,
            series_edge: 0,
        }
    }

    /// The photos this row stands for.
    pub fn photos(&self) -> Vec<PhotoId> {
        if self.members.is_empty() {
            vec![self.id]
        } else {
            self.members.clone()
        }
    }
}

/// The rows of the grid for `all` (the listed photos, in order), with the series in `info` collapsed unless they are
/// in `open`; and, for each listed photo that a collapsed row hides, the row's photo it stands behind.
pub fn view(
    all: &[Item],
    info: &HashMap<SeriesId, SeriesInfo>,
    open: &HashSet<SeriesId>,
) -> (Vec<Item>, HashMap<PhotoId, PhotoId>) {
    // The listed members of each series.
    let mut listed: HashMap<SeriesId, Vec<usize>> = HashMap::new();
    for (i, item) in all.iter().enumerate() {
        if let Some(series) = item.series.filter(|s| info.contains_key(s)) {
            listed.entry(series).or_default().push(i);
        }
    }
    let mut rows: Vec<Item> = Vec::with_capacity(all.len());
    let mut hidden: HashMap<PhotoId, PhotoId> = HashMap::new();
    let mut placed: HashSet<SeriesId> = HashSet::new();
    for item in all {
        let series = item.series.filter(|s| info.contains_key(s));
        let Some(series) = series else {
            rows.push(Item {
                series: None,
                ..item.clone()
            });
            continue;
        };
        let meta = &info[&series];
        let members = &listed[&series];
        let mut row = item.clone();
        row.series = Some(series);
        row.series_size = members.len() as u32;
        row.series_total = meta.members as u32;
        row.series_resolved = meta.resolved;
        if members.len() < 2 {
            // The others are filtered out: it is a photo, with the mark of its series.
            rows.push(row);
        } else if open.contains(&series) {
            // An open series lists its members in the order they were taken, where its first listed member is (the
            // grid lists the newest photos first, which is the wrong way round for the frames of a burst).
            if placed.insert(series) {
                for i in members.iter().rev() {
                    let mut member = all[*i].clone();
                    member.series = Some(series);
                    member.series_size = row.series_size;
                    member.series_total = row.series_total;
                    member.series_resolved = row.series_resolved;
                    member.series_open = true;
                    rows.push(member);
                }
            }
        } else if placed.insert(series) {
            // The row shows the cover when it is listed, else the first listed member.
            let shown = members
                .iter()
                .map(|i| &all[*i])
                .find(|m| m.id == meta.cover)
                .unwrap_or(&all[members[0]]);
            let mut collapsed = shown.clone();
            collapsed.series = Some(series);
            collapsed.series_size = row.series_size;
            collapsed.series_total = row.series_total;
            collapsed.series_resolved = row.series_resolved;
            collapsed.members = members.iter().map(|i| all[*i].id).collect();
            for member in &collapsed.members {
                if *member != collapsed.id {
                    hidden.insert(*member, collapsed.id);
                }
            }
            rows.push(collapsed);
        }
    }
    // The edges of the runs of an expanded series.
    for i in 0..rows.len() {
        let open_here = rows[i].series.is_some() && rows[i].series_open;
        if !open_here {
            continue;
        }
        let series = rows[i].series;
        let before = i > 0 && rows[i - 1].series == series && rows[i - 1].series_open;
        let after = i + 1 < rows.len() && rows[i + 1].series == series && rows[i + 1].series_open;
        rows[i].series_edge = match (before, after) {
            (false, true) => 1,
            (true, true) => 2,
            (true, false) => 3,
            (false, false) => 0,
        };
    }
    (rows, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(n: u8, series: Option<SeriesId>) -> Item {
        Item::photo(PhotoId::from_bytes([n; 16]), 0, 0, 0, series)
    }

    fn info(id: SeriesId, cover: u8, members: usize, resolved: bool) -> (SeriesId, SeriesInfo) {
        (
            id,
            SeriesInfo {
                id,
                kind: "burst".into(),
                cover: PhotoId::from_bytes([cover; 16]),
                resolved,
                members,
            },
        )
    }

    fn ids(rows: &[Item]) -> Vec<u8> {
        rows.iter().map(|r| r.id.as_bytes()[0]).collect()
    }

    #[test]
    fn a_collapsed_series_is_one_row_at_its_first_listed_member_showing_the_cover() {
        let s = SeriesId::from_bytes([9; 8]);
        // Listed newest first: 5, 4, 3 are the series (3 is its cover, the earliest), 2 and 1 are loners.
        let all = [
            photo(6, None),
            photo(5, Some(s)),
            photo(4, Some(s)),
            photo(3, Some(s)),
            photo(2, None),
        ];
        let (rows, hidden) = view(
            &all,
            &HashMap::from([info(s, 3, 3, false)]),
            &HashSet::new(),
        );
        assert_eq!(ids(&rows), [6, 3, 2]);
        assert_eq!(rows[1].members.len(), 3);
        assert_eq!((rows[1].series_size, rows[1].series_total), (3, 3));
        assert_eq!(hidden.len(), 2, "the two members the row hides");
        assert_eq!(
            hidden[&PhotoId::from_bytes([5; 16])],
            PhotoId::from_bytes([3; 16])
        );
        assert!(rows[0].members.is_empty() && rows[0].series.is_none());
    }

    #[test]
    fn an_expanded_series_lists_its_members_in_capture_order_joined_by_edges() {
        let s = SeriesId::from_bytes([9; 8]);
        let all = [
            photo(5, Some(s)),
            photo(4, Some(s)),
            photo(3, Some(s)),
            photo(2, None),
        ];
        let (rows, hidden) = view(
            &all,
            &HashMap::from([info(s, 3, 3, true)]),
            &HashSet::from([s]),
        );
        assert_eq!(
            ids(&rows),
            [3, 4, 5, 2],
            "the frames in the order they were taken"
        );
        assert!(hidden.is_empty());
        assert_eq!(
            rows.iter().map(|r| r.series_edge).collect::<Vec<_>>(),
            [1, 2, 3, 0]
        );
        assert!(rows[0].series_open && rows[0].series_resolved);
    }

    #[test]
    fn a_series_the_filters_cut_to_one_photo_is_a_photo_with_the_mark_of_its_series() {
        let s = SeriesId::from_bytes([9; 8]);
        let all = [photo(5, Some(s)), photo(2, None)];
        let (rows, hidden) = view(&all, &HashMap::from([info(s, 3, 3, true)]), &HashSet::new());
        assert_eq!(ids(&rows), [5, 2]);
        assert!(rows[0].members.is_empty());
        assert_eq!((rows[0].series_size, rows[0].series_total), (1, 3));
        assert!(hidden.is_empty());
    }

    #[test]
    fn a_cover_that_is_filtered_out_gives_way_to_the_first_listed_member() {
        let s = SeriesId::from_bytes([9; 8]);
        let all = [photo(5, Some(s)), photo(4, Some(s))];
        // The cover, 3, is not listed.
        let (rows, _) = view(
            &all,
            &HashMap::from([info(s, 3, 3, false)]),
            &HashSet::new(),
        );
        assert_eq!(ids(&rows), [5]);
        assert_eq!(rows[0].members.len(), 2);
    }
}
