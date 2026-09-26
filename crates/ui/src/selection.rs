// SPDX-License-Identifier: GPL-3.0-or-later
//! Which photos of the grid are selected (D-097). Three things, as in a file manager: the **selection** (a set
//! of photos, kept by identifier so that a reload or a photo arriving does not lose it), the **cursor** (where
//! the keyboard is: the `GridView`'s current index, which is not this type's business) and the **anchor**
//! (where a range starts). Pure, so it is unit-tested without Qt; `PhotoGrid` owns one and turns its changes
//! into signals.

use std::collections::HashSet;

use auroraw_types::PhotoId;

/// The selection and its anchor.
#[derive(Debug, Default)]
pub struct Selection {
    ids: HashSet<PhotoId>,
    anchor: Option<PhotoId>,
}

impl Selection {
    /// How many photos are selected.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether `id` is selected.
    pub fn contains(&self, id: &PhotoId) -> bool {
        self.ids.contains(id)
    }

    /// Where a range starts, if it has been set.
    pub fn anchor(&self) -> Option<PhotoId> {
        self.anchor
    }

    /// Selects `id` alone and makes it the anchor (a click, an arrow).
    pub fn only(&mut self, id: PhotoId) {
        self.ids.clear();
        self.ids.insert(id);
        self.anchor = Some(id);
    }

    /// Adds `id` if it is not selected, removes it if it is, and makes it the anchor (Ctrl+click, Space).
    pub fn toggle(&mut self, id: PhotoId) {
        if !self.ids.remove(&id) {
            self.ids.insert(id);
        }
        self.anchor = Some(id);
    }

    /// Selects the photos of `items` between the anchor's row and `row`, both included, in whichever order
    /// they lie. Without `additive` they replace the selection (Shift); with it they are added to it
    /// (Ctrl+Shift). The anchor stays where it was (it is set to the row asked from when there was none).
    pub fn range(&mut self, items: &[PhotoId], anchor_row: usize, row: usize, additive: bool) {
        let last = items.len().saturating_sub(1);
        let (from, to) = (anchor_row.min(last), row.min(last));
        let (low, high) = (from.min(to), from.max(to));
        if !additive {
            self.ids.clear();
        }
        self.ids
            .extend(items.iter().skip(low).take(high - low + 1).copied());
        if self.anchor.is_none() {
            self.anchor = items.get(from).copied();
        }
    }

    /// Selects every photo of `items` (Ctrl+A).
    pub fn all(&mut self, items: &[PhotoId]) {
        self.ids = items.iter().copied().collect();
    }

    /// Selects nothing (Escape). The anchor is forgotten with it.
    pub fn none(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    /// Selects what is not selected, and only that.
    pub fn invert(&mut self, items: &[PhotoId]) {
        self.ids = items
            .iter()
            .filter(|id| !self.ids.contains(id))
            .copied()
            .collect();
    }

    /// Selects exactly these photos (after an undo: the ones it touched); there is no anchor until one is set.
    pub fn set(&mut self, ids: impl IntoIterator<Item = PhotoId>) {
        self.ids = ids.into_iter().collect();
        self.anchor = None;
    }

    /// The photos selected now, to come back to (a rubber band that adds to what was selected).
    pub fn snapshot(&self) -> HashSet<PhotoId> {
        self.ids.clone()
    }

    /// Selects `base` and, besides, these photos (the rubber band as it is now).
    pub fn set_over(&mut self, base: &HashSet<PhotoId>, ids: impl IntoIterator<Item = PhotoId>) {
        self.ids = base.iter().copied().chain(ids).collect();
    }

    /// Keeps of the selection (and of the anchor) what is still in `items`: the list was read again.
    pub fn retain(&mut self, items: &[PhotoId]) {
        let listed: HashSet<&PhotoId> = items.iter().collect();
        self.ids.retain(|id| listed.contains(id));
        if self.anchor.is_some_and(|a| !listed.contains(&a)) {
            self.anchor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photos(n: usize) -> Vec<PhotoId> {
        (0..n).map(|_| PhotoId::random()).collect()
    }

    fn rows(selection: &Selection, items: &[PhotoId]) -> Vec<usize> {
        (0..items.len())
            .filter(|i| selection.contains(&items[*i]))
            .collect()
    }

    #[test]
    fn a_click_selects_one_photo_and_anchors_there() {
        let items = photos(5);
        let mut s = Selection::default();
        s.only(items[1]);
        s.only(items[3]);
        assert_eq!(rows(&s, &items), vec![3]);
        assert_eq!(s.anchor(), Some(items[3]));
    }

    #[test]
    fn ctrl_toggles_one_photo_without_touching_the_others() {
        let items = photos(5);
        let mut s = Selection::default();
        s.only(items[0]);
        s.toggle(items[2]);
        s.toggle(items[4]);
        assert_eq!(rows(&s, &items), vec![0, 2, 4]);
        s.toggle(items[2]);
        assert_eq!(rows(&s, &items), vec![0, 4]);
        assert_eq!(
            s.anchor(),
            Some(items[2]),
            "the last photo touched is the anchor"
        );
    }

    #[test]
    fn a_range_runs_from_the_anchor_in_either_direction_and_the_anchor_stays() {
        let items = photos(10);
        let mut s = Selection::default();
        s.only(items[4]);
        s.range(&items, 4, 7, false);
        assert_eq!(rows(&s, &items), vec![4, 5, 6, 7]);
        s.range(&items, 4, 1, false);
        assert_eq!(
            rows(&s, &items),
            vec![1, 2, 3, 4],
            "it replaces the last range"
        );
        assert_eq!(s.anchor(), Some(items[4]));
        s.range(&items, 4, 4, false);
        assert_eq!(rows(&s, &items), vec![4], "a range of one");
    }

    #[test]
    fn an_additive_range_keeps_what_was_selected() {
        let items = photos(10);
        let mut s = Selection::default();
        s.only(items[0]);
        s.toggle(items[5]);
        s.range(&items, 5, 8, true);
        assert_eq!(rows(&s, &items), vec![0, 5, 6, 7, 8]);
    }

    #[test]
    fn a_range_with_no_anchor_starts_where_it_was_asked_from() {
        let items = photos(6);
        let mut s = Selection::default();
        s.range(&items, 2, 4, false);
        assert_eq!(rows(&s, &items), vec![2, 3, 4]);
        assert_eq!(s.anchor(), Some(items[2]));
    }

    #[test]
    fn rows_past_the_end_are_clamped() {
        let items = photos(3);
        let mut s = Selection::default();
        s.range(&items, 1, 99, false);
        assert_eq!(rows(&s, &items), vec![1, 2]);
    }

    #[test]
    fn all_none_and_invert() {
        let items = photos(6);
        let mut s = Selection::default();
        s.all(&items);
        assert_eq!(s.len(), 6);
        s.none();
        assert_eq!((s.len(), s.anchor()), (0, None));
        s.only(items[1]);
        s.toggle(items[2]);
        s.invert(&items);
        assert_eq!(rows(&s, &items), vec![0, 3, 4, 5]);
    }

    #[test]
    fn a_list_read_again_keeps_what_is_still_in_it() {
        let items = photos(6);
        let mut s = Selection::default();
        s.only(items[1]);
        s.range(&items, 1, 4, false);
        let fewer: Vec<PhotoId> = items
            .iter()
            .copied()
            .filter(|p| *p != items[2] && *p != items[1])
            .collect();
        s.retain(&fewer);
        assert_eq!(s.len(), 2);
        assert_eq!(s.anchor(), None, "its anchor left the list");
    }

    #[test]
    fn set_selects_exactly_these() {
        let items = photos(4);
        let mut s = Selection::default();
        s.only(items[0]);
        s.set([items[2], items[3]]);
        assert_eq!(rows(&s, &items), vec![2, 3]);
    }

    #[test]
    fn a_rubber_band_adds_to_a_snapshot_and_can_shrink_back() {
        let items = photos(6);
        let mut s = Selection::default();
        s.only(items[0]);
        let base = s.snapshot();
        s.set_over(&base, [items[2], items[3]]);
        assert_eq!(rows(&s, &items), vec![0, 2, 3]);
        s.set_over(&base, [items[2]]);
        assert_eq!(rows(&s, &items), vec![0, 2], "the band got smaller");
        s.set_over(&HashSet::new(), [items[4]]);
        assert_eq!(rows(&s, &items), vec![4], "without adding, it replaces");
    }

    /// Any sequence of the operations agrees with a plain vector of booleans.
    #[test]
    fn any_sequence_agrees_with_a_plain_model() {
        let items = photos(12);
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n as u64) as usize
        };
        for _ in 0..200 {
            let mut s = Selection::default();
            let mut model = vec![false; items.len()];
            let mut anchor: Option<usize> = None;
            for _ in 0..40 {
                let row = next(items.len());
                match next(6) {
                    0 => {
                        s.only(items[row]);
                        model = vec![false; items.len()];
                        model[row] = true;
                        anchor = Some(row);
                    }
                    1 => {
                        s.toggle(items[row]);
                        model[row] = !model[row];
                        anchor = Some(row);
                    }
                    2 | 3 => {
                        let additive = next(2) == 0;
                        let from = anchor.unwrap_or(row);
                        s.range(&items, from, row, additive);
                        if !additive {
                            model = vec![false; items.len()];
                        }
                        model[from.min(row)..=from.max(row)]
                            .iter_mut()
                            .for_each(|m| *m = true);
                        anchor = anchor.or(Some(from));
                    }
                    4 => {
                        s.invert(&items);
                        model.iter_mut().for_each(|m| *m = !*m);
                    }
                    _ => {
                        s.none();
                        model = vec![false; items.len()];
                        anchor = None;
                    }
                }
                let expected: Vec<usize> = (0..items.len()).filter(|i| model[*i]).collect();
                assert_eq!(rows(&s, &items), expected);
            }
        }
    }
}
