// SPDX-License-Identifier: GPL-3.0-or-later
//! The collections panel's model (WP10, slice 3), `keyword_list.rs`'s twin: the manual collections as a flat,
//! depth-first list of the rows that are visible (a collapsed collection hides what is inside it, what was typed
//! shows the matches with their ancestors), with how many photos each holds and, for the current selection,
//! whether it holds none, some or all of it. Putting photos in is not here: the grid does it to its selection as
//! one action. Everything that edits goes through the engine, one step of the history each; the rows are the
//! catalogue's (`Catalogue::collections_with_counts`, already in tree order).

use core::pin::Pin;
use std::collections::{HashMap, HashSet};

use auroraw_catalogue::CollectionRow;
use auroraw_engine::{Command, EngineError, Outcome};
use auroraw_types::CollectionId;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::models::qobject::CollectionList;
use crate::session;

const ROLE_COLLECTION_ID: i32 = 0x0100;
const ROLE_NAME: i32 = 0x0101;
const ROLE_DEPTH: i32 = 0x0102;
const ROLE_PHOTOS: i32 = 0x0103;
const ROLE_STATE: i32 = 0x0104;
const ROLE_HAS_CHILDREN: i32 = 0x0105;
const ROLE_EXPANDED: i32 = 0x0106;

/// The Rust side of the model.
#[derive(Default)]
pub struct CollectionListRust {
    pub(crate) count: i32,
    /// Every manual collection, a parent before its children.
    all: Vec<CollectionRow>,
    /// The indices into `all` that are shown.
    visible: Vec<usize>,
    collapsed: HashSet<CollectionId>,
    filter: String,
    /// How many selected photos each collection holds, and how many are selected.
    usage: HashMap<CollectionId, usize>,
    selected: usize,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

/// What the panel is told when the engine refuses a collection edit: a code that the interface turns into a
/// sentence in the person's language (`name`, `taken:<name>`, `cycle`), or `other:` and the engine's own
/// (English) words for what has no sentence of its own.
pub(crate) fn reason(error: &EngineError) -> String {
    match error {
        EngineError::CollectionName => "name".into(),
        EngineError::CollectionNameTaken(name) => format!("taken:{name}"),
        EngineError::CollectionCycle => "cycle".into(),
        other => format!("other:{other}"),
    }
}

impl CollectionListRust {
    /// Which rows of `all` are shown now.
    fn recompute(&mut self) {
        let parent_of: HashMap<CollectionId, Option<CollectionId>> =
            self.all.iter().map(|c| (c.id, c.parent)).collect();
        let needle = self.filter.to_lowercase();
        if needle.is_empty() {
            self.visible = (0..self.all.len())
                .filter(|i| {
                    // Shown unless an ancestor is collapsed.
                    let mut parent = self.all[*i].parent;
                    while let Some(p) = parent {
                        if self.collapsed.contains(&p) {
                            return false;
                        }
                        parent = parent_of.get(&p).copied().flatten();
                    }
                    true
                })
                .collect();
            return;
        }
        let mut shown: HashSet<CollectionId> = HashSet::new();
        for row in &self.all {
            if row.name.to_lowercase().contains(&needle) {
                shown.insert(row.id);
                let mut parent = row.parent;
                while let Some(p) = parent {
                    if !shown.insert(p) {
                        break;
                    }
                    parent = parent_of.get(&p).copied().flatten();
                }
            }
        }
        self.visible = (0..self.all.len())
            .filter(|i| shown.contains(&self.all[*i].id))
            .collect();
    }

    fn row(&self, row: i32) -> Option<&CollectionRow> {
        usize::try_from(row)
            .ok()
            .and_then(|r| self.visible.get(r))
            .map(|i| &self.all[*i])
    }

    fn collection(&self, id: CollectionId) -> Option<&CollectionRow> {
        self.all.iter().find(|c| c.id == id)
    }

    /// `id` and every collection inside it.
    fn branch_of(&self, id: CollectionId) -> Vec<CollectionId> {
        let mut branch = vec![id];
        let mut i = 0;
        while i < branch.len() {
            let parent = branch[i];
            branch.extend(
                self.all
                    .iter()
                    .filter(|c| c.parent == Some(parent))
                    .map(|c| c.id),
            );
            i += 1;
        }
        branch
    }

    /// `Folder › Sub › Name`, for the Move dialog.
    fn path_of(&self, id: CollectionId) -> String {
        let mut names = Vec::new();
        let mut at = Some(id);
        while let Some(current) = at {
            let Some(row) = self.collection(current) else {
                break;
            };
            names.push(row.name.as_str());
            at = row.parent;
            if names.len() > self.all.len() {
                break; // (a cycle cannot be listed, but this cannot loop whatever the rows are)
            }
        }
        names.reverse();
        names.join(" › ")
    }

    /// Whether the collection `id` can be put under `parent` (`None`: the top level): it is not where it is
    /// already, not under itself or one of its own, and no sibling there has its name.
    fn may_move(&self, id: CollectionId, parent: Option<CollectionId>) -> bool {
        let Some(collection) = self.collection(id) else {
            return false;
        };
        if collection.parent == parent {
            return false;
        }
        if let Some(parent) = parent
            && (self.collection(parent).is_none() || self.branch_of(id).contains(&parent))
        {
            return false;
        }
        let name = collection.name.to_lowercase();
        !self
            .all
            .iter()
            .any(|c| c.parent == parent && c.id != id && c.name.to_lowercase() == name)
    }
}

impl CollectionList {
    /// Rebuilds the visible rows and tells the views.
    fn rebuilt(mut self: Pin<&mut Self>) {
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().recompute();
            self.as_mut().end_reset_model();
        }
        let count = self.visible.len() as i32;
        self.set_count(count);
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let all = session::current()
            .and_then(|s| s.engine.read_catalogue().ok())
            .and_then(|c| c.collections_with_counts().ok())
            .unwrap_or_default();
        // The same collections in the same places (only the counts moved): the views are told the rows
        // changed, nothing is reset, so the scroll position and the rows under the mouse stay.
        let same = all.len() == self.all.len()
            && all
                .iter()
                .zip(&self.all)
                .all(|(a, b)| a.id == b.id && a.parent == b.parent && a.name == b.name);
        self.as_mut().rust_mut().all = all;
        if !same {
            self.rebuilt();
            return;
        }
        let last = self.visible.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            self.as_mut()
                .data_changed(&first, &end, &QVector::<i32>::default());
        }
    }

    pub fn set_filter(mut self: Pin<&mut Self>, filter: &QString) {
        self.as_mut().rust_mut().filter = filter.to_string().trim().to_string();
        self.rebuilt();
    }

    pub fn toggle_expanded(mut self: Pin<&mut Self>, row: i32) {
        let Some(id) = self.row(row).map(|c| c.id) else {
            return;
        };
        if self.collapsed.contains(&id) {
            self.as_mut().rust_mut().collapsed.remove(&id);
        } else {
            self.as_mut().rust_mut().collapsed.insert(id);
        }
        self.rebuilt();
    }

    pub fn apply_usage(mut self: Pin<&mut Self>, usage: &QString, selected: i32) {
        let parsed: HashMap<CollectionId, usize> =
            serde_json::from_str::<HashMap<String, usize>>(&usage.to_string())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(id, n)| id.parse().ok().map(|id| (id, n)))
                .collect();
        self.as_mut().rust_mut().usage = parsed;
        self.as_mut().rust_mut().selected = selected.max(0) as usize;
        let last = self.visible.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            let mut roles = QVector::<i32>::default();
            roles.append(ROLE_STATE);
            self.as_mut().data_changed(&first, &end, &roles);
        }
    }

    pub fn id_at(&self, row: i32) -> QString {
        self.row(row)
            .map(|c| text(&c.id.to_string()))
            .unwrap_or_default()
    }

    pub fn name_at(&self, row: i32) -> QString {
        self.row(row).map(|c| text(&c.name)).unwrap_or_default()
    }

    /// The row of the best match for what was typed: the name itself, else the first that starts with it,
    /// else the first that contains it.
    pub fn best_match(&self, typed: &QString) -> i32 {
        let needle = typed.to_string().trim().to_lowercase();
        if needle.is_empty() {
            return -1;
        }
        let names: Vec<String> = self
            .visible
            .iter()
            .map(|i| self.all[*i].name.to_lowercase())
            .collect();
        names
            .iter()
            .position(|n| *n == needle)
            .or_else(|| names.iter().position(|n| n.starts_with(&needle)))
            .or_else(|| names.iter().position(|n| n.contains(&needle)))
            .map_or(-1, |row| row as i32)
    }

    /// Makes an empty collection under `parent` (an identifier; empty for the top level); its identifier, or
    /// the existing one's when that name is there already, or `error:` and why not.
    pub fn create(mut self: Pin<&mut Self>, name: &QString, parent: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("error:other:No workspace is open.");
        };
        let name = name.to_string().trim().to_string();
        if name.is_empty() {
            return text("error:name");
        }
        let parent: Option<CollectionId> = parent.to_string().parse().ok();
        if let Some(existing) = self
            .all
            .iter()
            .find(|c| c.parent == parent && c.name.to_lowercase() == name.to_lowercase())
        {
            return text(&existing.id.to_string());
        }
        match session.engine.submit_and_wait(Command::CreateCollection {
            name,
            parent,
            id: None,
            photos: Vec::new(),
        }) {
            Ok(Outcome::CollectionCreated(id)) => {
                if let Some(parent) = parent {
                    self.as_mut().rust_mut().collapsed.remove(&parent);
                }
                self.as_mut().refresh();
                text(&id.to_string())
            }
            Ok(other) => text(&format!("error:other:Unexpected answer {other:?}")),
            Err(e) => text(&format!("error:{}", reason(&e))),
        }
    }

    pub fn rename(mut self: Pin<&mut Self>, row: i32, name: &QString) -> QString {
        let (Some(session), Some(id)) = (session::current(), self.row(row).map(|c| c.id)) else {
            return text("other:No such collection.");
        };
        let new_name = name.to_string().trim().to_string();
        if new_name.is_empty() {
            return text("name");
        }
        match session.engine.submit_and_wait(Command::RenameCollection {
            collection_id: id,
            new_name,
        }) {
            Ok(_) => {
                self.as_mut().refresh();
                QString::default()
            }
            Err(e) => text(&reason(&e)),
        }
    }

    pub fn find_sibling(&self, name: &QString, parent: &QString) -> QString {
        let name = name.to_string().trim().to_lowercase();
        let parent: Option<CollectionId> = parent.to_string().parse().ok();
        self.all
            .iter()
            .find(|c| c.parent == parent && c.name.to_lowercase() == name)
            .map(|c| text(&c.id.to_string()))
            .unwrap_or_default()
    }

    pub fn name_of(&self, id: &QString) -> QString {
        id.to_string()
            .parse()
            .ok()
            .and_then(|id| self.collection(id))
            .map(|c| text(&c.name))
            .unwrap_or_default()
    }

    pub fn has_collection(&self, id: &QString) -> bool {
        id.to_string()
            .parse()
            .is_ok_and(|id| self.collection(id).is_some())
    }

    pub fn can_move(&self, id: &QString, parent: &QString) -> bool {
        let Ok(id) = id.to_string().parse::<CollectionId>() else {
            return false;
        };
        let parent = parent.to_string();
        let parent = if parent.is_empty() {
            None
        } else {
            match parent.parse::<CollectionId>() {
                Ok(parent) => Some(parent),
                Err(_) => return false,
            }
        };
        self.may_move(id, parent)
    }

    /// Where the collection can go, for the Move dialog: JSON `[{"id": ..., "path": "A › B"}]`, in the
    /// tree's order, without its own branch and the places it would clash.
    pub fn move_targets(&self, id: &QString) -> QString {
        let Ok(id) = id.to_string().parse::<CollectionId>() else {
            return text("[]");
        };
        let targets: Vec<serde_json::Value> = self
            .all
            .iter()
            .filter(|c| self.may_move(id, Some(c.id)))
            .map(|c| serde_json::json!({ "id": c.id.to_string(), "path": self.path_of(c.id) }))
            .collect();
        text(&serde_json::Value::Array(targets).to_string())
    }

    /// What deleting the collection takes with it, for the confirmation: JSON `{"name", "collections",
    /// "photos"}` (how many collections are in the branch, and how many photos are in any of them).
    pub fn branch(&self, id: &QString) -> QString {
        let Some(collection) = id
            .to_string()
            .parse::<CollectionId>()
            .ok()
            .and_then(|id| self.collection(id))
        else {
            return text("{}");
        };
        let ids = self.branch_of(collection.id);
        let photos = session::current()
            .and_then(|s| s.engine.read_catalogue().ok())
            .and_then(|c| c.photos_in_collections(&ids).ok())
            .map_or(0, |p| p.len());
        text(
            &serde_json::json!({ "name": collection.name, "collections": ids.len(), "photos": photos })
                .to_string(),
        )
    }

    /// Puts the collection under `parent` (an identifier; empty for the top level); empty, or why not.
    pub fn move_collection(mut self: Pin<&mut Self>, id: &QString, parent: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("other:No workspace is open.");
        };
        let Ok(collection_id) = id.to_string().parse::<CollectionId>() else {
            return text("other:No such collection.");
        };
        let new_parent: Option<CollectionId> = parent.to_string().parse().ok();
        // What was moved is to be seen where it went.
        if let Some(parent) = new_parent {
            self.as_mut().rust_mut().collapsed.remove(&parent);
        }
        match session.engine.submit_and_wait(Command::MoveCollection {
            collection_id,
            new_parent,
        }) {
            Ok(_) => {
                self.as_mut().refresh();
                QString::default()
            }
            Err(e) => text(&reason(&e)),
        }
    }

    /// Deletes the collection and the ones inside it (one step of the history); empty, or why not.
    pub fn remove(mut self: Pin<&mut Self>, id: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("other:No workspace is open.");
        };
        let Ok(collection_id) = id.to_string().parse::<CollectionId>() else {
            return text("other:No such collection.");
        };
        match session
            .engine
            .submit_and_wait(Command::DeleteCollection { collection_id })
        {
            Ok(_) => {
                self.as_mut().refresh();
                QString::default()
            }
            Err(e) => text(&reason(&e)),
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(collection) = self.row(index.row()) else {
            return QVariant::default();
        };
        match role {
            ROLE_COLLECTION_ID => QVariant::from(&text(&collection.id.to_string())),
            ROLE_NAME => QVariant::from(&text(&collection.name)),
            ROLE_DEPTH => QVariant::from(&collection.depth),
            ROLE_PHOTOS => QVariant::from(&(collection.photos as i32)),
            ROLE_STATE => {
                let held = self.usage.get(&collection.id).copied().unwrap_or(0);
                let state = if self.selected == 0 || held == 0 {
                    0
                } else if held >= self.selected {
                    2
                } else {
                    1
                };
                QVariant::from(&state)
            }
            ROLE_HAS_CHILDREN => {
                QVariant::from(&self.all.iter().any(|c| c.parent == Some(collection.id)))
            }
            ROLE_EXPANDED => QVariant::from(&(!self.collapsed.contains(&collection.id))),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_COLLECTION_ID, QByteArray::from("collectionId"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_DEPTH, QByteArray::from("depth"));
        roles.insert(ROLE_PHOTOS, QByteArray::from("photos"));
        roles.insert(ROLE_STATE, QByteArray::from("held"));
        roles.insert(ROLE_HAS_CHILDREN, QByteArray::from("hasChildren"));
        roles.insert(ROLE_EXPANDED, QByteArray::from("expanded"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.visible.len() as i32
    }
}
