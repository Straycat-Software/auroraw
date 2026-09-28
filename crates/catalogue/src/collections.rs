// SPDX-License-Identifier: GPL-3.0-or-later
//! Manual collections in the catalogue (WP10, slice 3): what the collections panel reads (the tree with
//! how many photos each holds), what the grid reads (the photos of a collection and of the ones under it),
//! and the incremental writes a change of a collection makes. A collection's own state is the workspace's
//! file (`collections/<id>.json`, note 002 §6.3); the rows here follow it, and a rebuild builds them again
//! from the files (`populate::insert_collection`). Only `manual` collections are listed: a smart or
//! client-selection file is somebody else's (a later work package's, or a newer version's) and is left
//! alone.

use std::collections::HashMap;

use auroraw_format::state::{Collection, collection_kind};
use auroraw_types::{CollectionId, MemberRef, PhotoId};
use rusqlite::params;

use crate::SidecarStat;
use crate::error::Result;
use crate::open::Catalogue;

/// One manual collection, as the collections panel needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRow {
    /// Its identifier.
    pub id: CollectionId,
    /// The collection it is inside, `None` at the top level.
    pub parent: Option<CollectionId>,
    /// Its name.
    pub name: String,
    /// How deep it is (0 at the top level).
    pub depth: i32,
    /// How many photos it holds itself (not those of the collections under it).
    pub photos: u64,
}

/// The rows in tree order: a parent before its children, the children of one parent by name (any case).
/// A row whose parent is not among the rows (a folder that is not a manual collection, a file that names a
/// parent that is gone) is shown at the top level rather than lost; a cycle (only a hand-edited file makes
/// one) is not reachable from any top level and is left out.
fn in_tree_order(
    rows: Vec<(CollectionId, Option<CollectionId>, String, u64)>,
) -> Vec<CollectionRow> {
    let known: std::collections::HashSet<CollectionId> = rows.iter().map(|r| r.0).collect();
    let mut children: HashMap<Option<CollectionId>, Vec<usize>> = HashMap::new();
    for (i, (_, parent, _, _)) in rows.iter().enumerate() {
        let parent = parent.filter(|p| known.contains(p));
        children.entry(parent).or_default().push(i);
    }
    for siblings in children.values_mut() {
        siblings.sort_by_key(|i| (rows[*i].2.to_lowercase(), rows[*i].0.to_string()));
    }
    let mut out = Vec::with_capacity(rows.len());
    // Depth first, iteratively: the last sibling goes on the stack first so the first comes off first.
    let mut stack: Vec<(usize, i32)> = children
        .get(&None)
        .map(|top| top.iter().rev().map(|i| (*i, 0)).collect())
        .unwrap_or_default();
    while let Some((i, depth)) = stack.pop() {
        let (id, parent, name, photos) = &rows[i];
        out.push(CollectionRow {
            id: *id,
            parent: *parent,
            name: name.clone(),
            depth,
            photos: *photos,
        });
        if let Some(below) = children.get(&Some(*id)) {
            stack.extend(below.iter().rev().map(|c| (*c, depth + 1)));
        }
    }
    out
}

impl Catalogue {
    /// Writes a collection's row and its members (in order), replacing the ones it had: a collection is always
    /// written whole, the way its file is.
    pub fn apply_collection(&mut self, collection: &Collection, stat: SidecarStat) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = collection.id.to_string();
        tx.execute(
            "INSERT INTO collection(id, parent_id, name, kind, sidecar_size, sidecar_modified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET parent_id = excluded.parent_id, name = excluded.name,
                kind = excluded.kind, sidecar_size = excluded.sidecar_size,
                sidecar_modified = excluded.sidecar_modified",
            params![
                id,
                collection.parent.map(|p| p.to_string()),
                collection.name,
                collection.kind,
                stat.size as i64,
                stat.modified
            ],
        )?;
        tx.execute(
            "DELETE FROM collection_member WHERE collection_id = ?1",
            [&id],
        )?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO collection_member(collection_id, position, photo_id, version_id)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (position, member) in collection.members.iter().enumerate() {
                let (photo, version) = match member {
                    MemberRef::Photo(p) => (p.to_string(), None),
                    MemberRef::Version(p, v) => (p.to_string(), Some(v.to_string())),
                };
                insert.execute(params![id, position as i64, photo, version])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Takes a collection out: its members first (the foreign key), then its row. Its children are the
    /// caller's to take out too (a branch is deleted whole).
    pub fn remove_collection(&mut self, id: &CollectionId) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = id.to_string();
        tx.execute(
            "DELETE FROM collection_member WHERE collection_id = ?1",
            [&id],
        )?;
        tx.execute("DELETE FROM collection WHERE id = ?1", [&id])?;
        tx.commit()?;
        Ok(())
    }

    /// The manual collections in tree order, each with how many photos it holds itself.
    pub fn collections_with_counts(&self) -> Result<Vec<CollectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.parent_id, c.name,
                    (SELECT COUNT(*) FROM collection_member m WHERE m.collection_id = c.id)
             FROM collection c WHERE c.kind = ?1",
        )?;
        let rows = stmt.query_map([collection_kind::MANUAL], |r| {
            let id: String = r.get(0)?;
            let parent: Option<String> = r.get(1)?;
            let bad = |column: usize| {
                rusqlite::Error::InvalidColumnType(
                    column,
                    "collection id".into(),
                    rusqlite::types::Type::Text,
                )
            };
            Ok((
                id.parse().map_err(|_| bad(0))?,
                parent.and_then(|p| p.parse().ok()),
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)? as u64,
            ))
        })?;
        Ok(in_tree_order(rows.collect::<rusqlite::Result<_>>()?))
    }

    /// For each collection, how many of `photos` are in it directly (collections none of them are in are left
    /// out): what the collections panel shows for a selection (none, some or all of it).
    pub fn collection_usage(&self, photos: &[PhotoId]) -> Result<HashMap<CollectionId, usize>> {
        let mut usage: HashMap<CollectionId, usize> = HashMap::new();
        for chunk in photos.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT collection_id, COUNT(DISTINCT photo_id) FROM collection_member
                 WHERE photo_id IN ({placeholders}) GROUP BY collection_id"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let texts: Vec<String> = chunk.iter().map(ToString::to_string).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(&texts), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (collection, count) = row?;
                if let Ok(collection) = collection.parse() {
                    *usage.entry(collection).or_default() += count as usize;
                }
            }
        }
        Ok(usage)
    }

    /// The collections a photo is directly in, whatever their kind: which state files a photo that leaves the
    /// workspace has to be taken out of.
    pub fn collections_holding(&self, photo: &PhotoId) -> Result<Vec<CollectionId>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT collection_id FROM collection_member WHERE photo_id = ?1",
        )?;
        let rows = stmt.query_map([photo.to_string()], |r| r.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for id in rows {
            if let Ok(id) = id?.parse() {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Every photo that is directly in any of `collections`, each once, unordered: what a deletion of a branch
    /// says it takes with it.
    pub fn photos_in_collections(&self, collections: &[CollectionId]) -> Result<Vec<PhotoId>> {
        if collections.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = collections
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT DISTINCT photo_id FROM collection_member WHERE collection_id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let texts: Vec<String> = collections.iter().map(ToString::to_string).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(&texts), |r| {
            r.get::<_, String>(0)
        })?;
        let mut ids = Vec::new();
        for id in rows {
            let id = id?;
            ids.push(id.parse().map_err(|_| {
                rusqlite::Error::InvalidColumnType(
                    0,
                    "photo_id".into(),
                    rusqlite::types::Type::Text,
                )
            })?);
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use auroraw_types::WorkspaceId;

    use super::*;
    use crate::query::{Filter, FlagFilter};

    fn collection(name: &str, parent: Option<CollectionId>, members: &[PhotoId]) -> Collection {
        Collection {
            id: CollectionId::random(),
            updated: "2026-09-28T12:00:00Z".parse().unwrap(),
            name: name.into(),
            kind: collection_kind::MANUAL.into(),
            parent,
            members: members.iter().map(|p| MemberRef::Photo(*p)).collect(),
            query: None,
            query_schema: None,
            extra: Default::default(),
        }
    }

    fn photo(cat: &Catalogue, time: i64) -> PhotoId {
        let id = PhotoId::random();
        cat.conn
            .execute(
                "INSERT INTO photo(id, filename, capture_time, sidecar_size) VALUES (?1, 'a.jpg', ?2, 1)",
                params![id.to_string(), time],
            )
            .unwrap();
        id
    }

    fn open() -> Catalogue {
        Catalogue::open_in_memory(WorkspaceId::random()).unwrap()
    }

    fn put(cat: &mut Catalogue, c: &Collection) {
        cat.apply_collection(c, SidecarStat::default()).unwrap();
    }

    #[test]
    fn applying_a_collection_again_replaces_its_members_and_removing_it_takes_them() {
        let mut cat = open();
        let (a, b, c) = (PhotoId::random(), PhotoId::random(), PhotoId::random());
        let mut picks = collection("Picks", None, &[a, b]);
        put(&mut cat, &picks);
        assert_eq!(cat.collections_with_counts().unwrap()[0].photos, 2);

        picks.members = vec![MemberRef::Photo(c)];
        picks.name = "Best".into();
        put(&mut cat, &picks);
        let rows = cat.collections_with_counts().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].name.as_str(), rows[0].photos), ("Best", 1));
        assert_eq!(cat.photos_in_collections(&[picks.id]).unwrap(), vec![c]);

        cat.remove_collection(&picks.id).unwrap();
        assert!(cat.collections_with_counts().unwrap().is_empty());
        assert!(cat.photos_in_collections(&[picks.id]).unwrap().is_empty());
    }

    #[test]
    fn the_tree_is_in_order_with_its_depths_and_only_manual_collections_are_listed() {
        let mut cat = open();
        let weddings = collection("weddings", None, &[]);
        let marie = collection("Marie", Some(weddings.id), &[PhotoId::random()]);
        let anna = collection("anna", Some(weddings.id), &[]);
        let travel = collection("Travel", None, &[]);
        let mut smart = collection("Smart", None, &[]);
        smart.kind = collection_kind::SMART.into();
        // Written children first: the order is the tree's, not the order they were written in.
        for c in [&marie, &travel, &smart, &anna, &weddings] {
            put(&mut cat, c);
        }
        let rows = cat.collections_with_counts().unwrap();
        let shown: Vec<(&str, i32)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(
            shown,
            vec![("Travel", 0), ("weddings", 0), ("anna", 1), ("Marie", 1)],
            "top level by name (any case), children under their parent, a smart one left out"
        );
        assert_eq!(rows[3].parent, Some(weddings.id));
        assert_eq!(rows[3].photos, 1);
    }

    #[test]
    fn a_collection_whose_parent_is_not_listed_is_shown_at_the_top_level() {
        let mut cat = open();
        let mut lost = collection("Lost", Some(CollectionId::random()), &[]);
        put(&mut cat, &lost);
        assert_eq!(cat.collections_with_counts().unwrap()[0].depth, 0);
        lost.parent = None;
        put(&mut cat, &lost);
        assert_eq!(cat.collections_with_counts().unwrap()[0].depth, 0);
    }

    #[test]
    fn usage_counts_the_given_photos_in_each_collection() {
        let mut cat = open();
        let (a, b, c) = (PhotoId::random(), PhotoId::random(), PhotoId::random());
        let one = collection("One", None, &[a, b]);
        let two = collection("Two", None, &[b, c]);
        let none = collection("None", None, &[c]);
        for x in [&one, &two, &none] {
            put(&mut cat, x);
        }
        let usage = cat.collection_usage(&[a, b]).unwrap();
        assert_eq!(usage.get(&one.id), Some(&2));
        assert_eq!(usage.get(&two.id), Some(&1));
        assert_eq!(usage.get(&none.id), None, "none of them is in it");
    }

    #[test]
    fn filtering_by_a_collection_lists_its_photos_and_those_under_it_each_once() {
        let mut cat = open();
        let (a, b, c, d) = (
            photo(&cat, 40),
            photo(&cat, 30),
            photo(&cat, 20),
            photo(&cat, 10),
        );
        let parent = collection("Parent", None, &[a, b]);
        let child = collection("Child", Some(parent.id), &[b, c]);
        let other = collection("Other", None, &[d]);
        for x in [&parent, &child, &other] {
            put(&mut cat, x);
        }
        let listed = |id: CollectionId| -> Vec<PhotoId> {
            cat.list_filtered(
                &Filter {
                    flags: FlagFilter::All,
                    collection: Some(id),
                    ..Filter::default()
                },
                None,
                100,
            )
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect()
        };
        assert_eq!(
            listed(parent.id),
            vec![a, b, c],
            "b is in both, listed once, newest first"
        );
        assert_eq!(listed(child.id), vec![b, c]);
        assert_eq!(listed(other.id), vec![d]);
        assert!(listed(CollectionId::random()).is_empty());
    }
}
