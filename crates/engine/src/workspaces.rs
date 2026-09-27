// SPDX-License-Identifier: GPL-3.0-or-later
//! Opening and creating workspaces the way a person does (M1 plan, workflow revision D-090): the
//! workspace is the folder that is backed up and synced, so the databases derived from it live
//! elsewhere, on this machine only (D-026, D-075, design note 001 §5.7): the catalogue in the data
//! folder, the previews in the cache folder, both named after the workspace's identifier, next to
//! a small registry of the workspaces this machine knows. This crate resolves no platform folder
//! itself; the application gives them in [`LocalDirs`].

use std::path::{Path, PathBuf};

use auroraw_catalogue::{Catalogue, Registry, RegistryEntry};
use auroraw_types::{Timestamp, WorkspaceId};
use auroraw_workspace::{Access, Workspace, WorkspaceError};

use crate::command::Command;
use crate::coordinator::Outcome;
use crate::error::Result;
use crate::{Engine, EventReceiver};

/// Where this machine keeps what belongs to it and not to a workspace.
#[derive(Debug, Clone)]
pub struct LocalDirs {
    /// Catalogue databases and the registry (deletable and rebuildable, but not a cache).
    pub data: PathBuf,
    /// Thumbnails and previews (deletable without loss).
    pub cache: PathBuf,
}

impl LocalDirs {
    /// The registry of workspaces this machine knows.
    pub fn registry_path(&self) -> PathBuf {
        self.data.join("registry.json")
    }

    /// What this machine keeps for one workspace besides its databases (the import form, the state
    /// of an interrupted import).
    pub fn workspace_data(&self, workspace: WorkspaceId) -> PathBuf {
        self.data.join("catalogues").join(workspace.to_string())
    }

    /// The catalogue database of a workspace.
    pub fn catalogue_path(&self, workspace: WorkspaceId) -> PathBuf {
        self.data
            .join("catalogues")
            .join(workspace.to_string())
            .join("catalogue.db")
    }

    /// The previews database of a workspace.
    pub fn previews_path(&self, workspace: WorkspaceId) -> PathBuf {
        self.cache
            .join("catalogues")
            .join(workspace.to_string())
            .join("previews.db")
    }
}

/// A workspace that was just opened or created, with its running engine.
pub struct OpenedWorkspace {
    /// The engine over it.
    pub engine: Engine,
    /// What the engine reports.
    pub events: EventReceiver,
    /// The workspace's identifier (from its marker).
    pub workspace_id: WorkspaceId,
    /// The catalogue's name (from its marker).
    pub name: String,
    /// The workspace folder.
    pub root: PathBuf,
    /// Where this workspace's thumbnails are cached.
    pub previews_path: PathBuf,
    /// Whether the local catalogue was missing (a workspace from another machine, or a deleted
    /// database) and was rebuilt from the workspace's files.
    pub rebuilt: bool,
}

/// A workspace this machine knows, for the welcome list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownWorkspace {
    /// Its identifier.
    pub workspace_id: WorkspaceId,
    /// Its name.
    pub name: String,
    /// Where it was last found.
    pub path: PathBuf,
    /// When it was last opened here.
    pub opened: Option<Timestamp>,
    /// Whether its folder (with its marker) is where it was.
    pub found: bool,
}

fn is_workspace(root: &Path) -> bool {
    root.join("workspace.json").is_file()
}

fn known(entry: &RegistryEntry) -> KnownWorkspace {
    KnownWorkspace {
        workspace_id: entry.workspace_id,
        name: entry.name.clone(),
        path: entry.workspace_path.clone(),
        opened: entry.opened,
        found: is_workspace(&entry.workspace_path),
    }
}

/// Opens the workspace with the writer's lock. A workspace that was just closed by this very
/// process is still being let go of by its engine's thread, so a held lock is retried for a moment
/// before it is reported as another program's.
fn open_for_writing(root: &Path) -> Result<Workspace> {
    let mut attempts = 0;
    loop {
        let workspace = Workspace::open(root)?;
        if workspace.access() == Access::ReadWrite {
            return Ok(workspace);
        }
        attempts += 1;
        if attempts >= 30 {
            return Err(WorkspaceError::ReadOnly.into());
        }
        drop(workspace);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Creates the folder a file will live in.
fn make_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Removes a catalogue database and the files SQLite keeps beside it.
fn remove_database(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(file));
    }
}

impl Engine {
    /// Creates a workspace at `root` (a new folder, or an empty one) called `name`, with its local
    /// catalogue, and remembers it as the workspace to reopen.
    pub fn create_workspace(root: &Path, name: &str, dirs: &LocalDirs) -> Result<OpenedWorkspace> {
        let workspace = Workspace::create(root, name)?;
        let id = workspace.workspace_id();
        let catalogue_path = dirs.catalogue_path(id);
        if let Some(parent) = catalogue_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A database left by an earlier workspace with this identifier cannot be trusted.
        remove_database(&catalogue_path);
        let catalogue = Catalogue::create(&catalogue_path, id)?;
        let (engine, events) = Self::spawn(workspace, catalogue, catalogue_path.clone());
        let previews_path = dirs.previews_path(id);
        make_parent(&previews_path)?;
        let opened = OpenedWorkspace {
            engine,
            events,
            workspace_id: id,
            name: name.to_string(),
            root: root.to_path_buf(),
            previews_path,
            rebuilt: false,
        };
        remember(dirs, &opened, &catalogue_path)?;
        Ok(opened)
    }

    /// Opens the workspace at `root`. Its catalogue is the one in `dirs`; when that is missing,
    /// unreadable or belongs to another workspace, it is rebuilt from the workspace's files
    /// (D-026: the database is an index, never the truth).
    pub fn open_workspace(root: &Path, dirs: &LocalDirs) -> Result<OpenedWorkspace> {
        let workspace = open_for_writing(root)?;
        let id = workspace.workspace_id();
        let name = workspace.marker().catalogue_name.clone();
        let catalogue_path = dirs.catalogue_path(id);

        let existing = Catalogue::open(&catalogue_path)
            .ok()
            .filter(|catalogue| catalogue.workspace_id().is_ok_and(|found| found == id));
        let (catalogue, rebuilt) = match existing {
            Some(catalogue) => (catalogue, false),
            None => {
                if let Some(parent) = catalogue_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                remove_database(&catalogue_path);
                (Catalogue::create(&catalogue_path, id)?, true)
            }
        };
        let (engine, events) = Self::spawn(workspace, catalogue, catalogue_path.clone());
        if rebuilt {
            match engine.submit_and_wait(Command::Rebuild)? {
                Outcome::Applied => {}
                other => unreachable!("Rebuild always answers Applied, not {other:?}"),
            }
        }
        let previews_path = dirs.previews_path(id);
        make_parent(&previews_path)?;
        let opened = OpenedWorkspace {
            engine,
            events,
            workspace_id: id,
            name,
            root: root.to_path_buf(),
            previews_path,
            rebuilt,
        };
        remember(dirs, &opened, &catalogue_path)?;
        Ok(opened)
    }

    /// Every workspace this machine knows, most recently opened first.
    pub fn known_workspaces(dirs: &LocalDirs) -> Vec<KnownWorkspace> {
        Registry::load(&dirs.registry_path())
            .map(|registry| registry.recent().into_iter().map(known).collect())
            .unwrap_or_default()
    }

    /// The workspace to reopen at launch, if there is one.
    pub fn last_opened_workspace(dirs: &LocalDirs) -> Option<KnownWorkspace> {
        Registry::load(&dirs.registry_path())
            .ok()?
            .last_opened()
            .map(known)
    }

    /// Takes a workspace off the welcome list (its folder and its data are not touched).
    pub fn forget_workspace(dirs: &LocalDirs, workspace: WorkspaceId) -> Result<()> {
        let mut registry = Registry::load(&dirs.registry_path())?;
        registry.remove(workspace);
        registry.save(&dirs.registry_path())?;
        Ok(())
    }
}

/// Records an opened workspace in the registry, and as the one to reopen.
fn remember(dirs: &LocalDirs, opened: &OpenedWorkspace, catalogue_path: &Path) -> Result<()> {
    let path = dirs.registry_path();
    let mut registry = Registry::load(&path)?;
    registry.upsert(RegistryEntry {
        workspace_id: opened.workspace_id,
        name: opened.name.clone(),
        workspace_path: opened.root.clone(),
        catalogue_path: catalogue_path.to_path_buf(),
        opened: None,
    });
    registry.touch(opened.workspace_id, Timestamp::now());
    registry.save(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use auroraw_format::sidecar::Flag;
    use auroraw_testkit::temp_dir;

    use super::*;

    fn dirs(root: &Path) -> LocalDirs {
        LocalDirs {
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    #[test]
    fn a_new_workspace_keeps_its_databases_out_of_its_own_folder() {
        let dir = temp_dir();
        let dirs = dirs(dir.path());
        let root = dir.path().join("Photos/Family");
        let opened = Engine::create_workspace(&root, "Family", &dirs).unwrap();

        assert_eq!(opened.name, "Family");
        assert!(root.join("workspace.json").is_file());
        assert!(dirs.catalogue_path(opened.workspace_id).is_file());
        assert_eq!(
            opened.previews_path,
            dirs.cache
                .join("catalogues")
                .join(opened.workspace_id.to_string())
                .join("previews.db")
        );
        let inside: Vec<String> = walk(&root);
        assert!(
            inside
                .iter()
                .all(|p| !p.ends_with(".db") && !p.contains("sqlite")),
            "the workspace holds no database: {inside:?}"
        );
    }

    fn walk(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.push(path.to_string_lossy().into_owned());
            }
        }
        out
    }

    #[test]
    fn creating_and_opening_are_remembered_and_the_last_one_is_offered_again() {
        let dir = temp_dir();
        let dirs = dirs(dir.path());
        assert!(Engine::known_workspaces(&dirs).is_empty());
        assert!(Engine::last_opened_workspace(&dirs).is_none());

        let a = Engine::create_workspace(&dir.path().join("A"), "Alpha", &dirs).unwrap();
        let b = Engine::create_workspace(&dir.path().join("B"), "Beta", &dirs).unwrap();
        drop((a.engine, b.engine));

        let last = Engine::last_opened_workspace(&dirs).unwrap();
        assert_eq!(last.name, "Beta");
        assert!(last.found);

        // Opening Alpha again makes it the one to reopen and moves it to the top.
        std::thread::sleep(Duration::from_millis(1100));
        let a = Engine::open_workspace(&dir.path().join("A"), &dirs).unwrap();
        drop(a.engine);
        assert_eq!(Engine::last_opened_workspace(&dirs).unwrap().name, "Alpha");
        let names: Vec<String> = Engine::known_workspaces(&dirs)
            .into_iter()
            .map(|w| w.name)
            .collect();
        assert_eq!(names, ["Alpha", "Beta"]);
    }

    #[test]
    fn a_workspace_that_moved_or_vanished_is_listed_as_not_found() {
        let dir = temp_dir();
        let dirs = dirs(dir.path());
        let a = Engine::create_workspace(&dir.path().join("A"), "Alpha", &dirs).unwrap();
        drop(a.engine);
        // (Windows refuses while the closed engine's thread is still letting go of the database.)
        let started = std::time::Instant::now();
        while std::fs::rename(dir.path().join("A"), dir.path().join("Moved")).is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the database stayed open"
            );
            std::thread::sleep(Duration::from_millis(100));
        }

        let known = Engine::known_workspaces(&dirs);
        assert_eq!(known.len(), 1);
        assert!(!known[0].found);
        assert!(!Engine::last_opened_workspace(&dirs).unwrap().found);

        Engine::forget_workspace(&dirs, known[0].workspace_id).unwrap();
        assert!(Engine::known_workspaces(&dirs).is_empty());
        assert!(Engine::last_opened_workspace(&dirs).is_none());
    }

    #[test]
    fn a_workspace_whose_local_catalogue_is_missing_is_rebuilt_from_its_files() {
        let dir = temp_dir();
        let dirs = dirs(dir.path());
        let root = dir.path().join("Photos");
        let first = Engine::create_workspace(&root, "Main", &dirs).unwrap();
        let card = dir.path().join("Card");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("a.raw"), b"one photo").unwrap();
        let (id, engine) = (first.workspace_id, first.engine);
        let auroraw_id = {
            let Outcome::SourceAdded(source) = engine
                .submit_and_wait(Command::AddSource {
                    name: "Card".into(),
                    root: card.clone(),
                    kind: "local-folder".into(),
                })
                .unwrap()
            else {
                panic!("expected SourceAdded");
            };
            let Outcome::Scanned { new, .. } = engine
                .submit_and_wait(Command::ScanSource { source_id: source })
                .unwrap()
            else {
                panic!("expected Scanned");
            };
            let Outcome::PhotosAdded(added) = engine
                .submit_and_wait(Command::AddNewPhotos {
                    source_id: source,
                    paths: new,
                })
                .unwrap()
            else {
                panic!("expected PhotosAdded");
            };
            engine
                .submit_and_wait(Command::SetFlag {
                    photo_id: added[0],
                    flag: Some(Flag::Picked),
                })
                .unwrap();
            added[0]
        };
        drop(engine);

        // This machine loses its database (a new machine, a cleaned data folder).
        // (Windows refuses while the closed engine's thread is still letting go of the database.)
        let started = std::time::Instant::now();
        while std::fs::remove_dir_all(&dirs.data).is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the database stayed open"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let reopened = Engine::open_workspace(&root, &dirs).unwrap();
        assert!(reopened.rebuilt);
        assert_eq!(reopened.workspace_id, id);
        let photo = reopened
            .engine
            .read_catalogue()
            .unwrap()
            .photo(&auroraw_id)
            .unwrap()
            .expect("the photo is back");
        assert_eq!(photo.filename, "a.raw");
        assert_eq!(photo.flag, 1, "what the sidecar held is back too");
        drop(reopened.engine);

        // The next opening finds the rebuilt database and does not rebuild again.
        assert!(!Engine::open_workspace(&root, &dirs).unwrap().rebuilt);
    }

    #[test]
    fn a_folder_that_is_not_a_workspace_is_refused() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.path().join("Plain")).unwrap();
        assert!(Engine::open_workspace(&dir.path().join("Plain"), &dirs(dir.path())).is_err());
    }

    #[test]
    fn a_closed_workspace_lets_go_of_its_lock_so_it_can_be_opened_again() {
        let dir = temp_dir();
        let dirs = dirs(dir.path());
        let root = dir.path().join("Main");
        let first = Engine::create_workspace(&root, "Main", &dirs).unwrap();
        // A background job is running when the last handle goes away: it is cancelled, not waited for.
        drop((first.engine, first.events));

        let started = std::time::Instant::now();
        let again = Engine::open_workspace(&root, &dirs).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the coordinator of the closed workspace stopped"
        );
        drop(again.engine);
    }
}
