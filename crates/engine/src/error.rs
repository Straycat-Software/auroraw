// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_catalogue::CatalogueError;
use auroraw_workspace::WorkspaceError;

/// What can go wrong asking the engine to do something.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The workspace refused the operation.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    /// The catalogue refused the operation.
    #[error(transparent)]
    Catalogue(#[from] CatalogueError),
    /// The catalogue does not index this workspace (note 001 §5.3): open with `Engine::rebuild`
    /// instead of `Engine::open`, or point at the right pair of paths.
    #[error("this catalogue does not belong to this workspace; rebuild it")]
    WrongCatalogue,
    /// The named entity is not in the workspace.
    #[error("{kind} {id} is not in the workspace")]
    NotFound {
        /// What kind of entity (`"photo"`, `"keyword"`).
        kind: &'static str,
        /// Its identifier, as text.
        id: String,
    },
    /// A command that cannot be done the way it was asked (an edit that is not one, in a batch).
    #[error("{0}")]
    InvalidCommand(String),
    /// A keyword needs a name, and the name cannot contain `|` (it separates the parts of a path).
    #[error("A keyword needs a name, without |.")]
    KeywordName,
    /// A sibling has the name already (whatever the case).
    #[error("There is already a keyword named “{0}” there.")]
    KeywordNameTaken(String),
    /// A keyword cannot be moved under itself or one of its own keywords.
    #[error("A keyword cannot be moved under itself or under one of its own keywords.")]
    KeywordCycle,
    /// A collection needs a name.
    #[error("A collection needs a name.")]
    CollectionName,
    /// A sibling has the name already (whatever the case).
    #[error("There is already a collection named “{0}” there.")]
    CollectionNameTaken(String),
    /// A collection cannot be moved under itself or one of its own collections.
    #[error("A collection cannot be moved under itself or under one of its own collections.")]
    CollectionCycle,
    /// The coordinator thread has already stopped (a command was sent after `shutdown`, or it
    /// panicked: the latter is always a bug, since every command is caught, never propagated).
    #[error("the engine is no longer running")]
    Stopped,
    /// Reading a sidecar's size and modification time off disk failed (the file vanished between
    /// the workspace read and the stat, for instance).
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A source refused an operation (WP4): unreachable, or no file at the path asked for.
    #[error(transparent)]
    Source(#[from] auroraw_plugin_api::source::SourceError),
    /// The folder is inside a source that is already registered: its photos are there already.
    #[error("this folder is already part of the source \"{0}\"")]
    AlreadyCovered(String),
    /// The folder contains sources that are already registered: adding it would list their photos
    /// twice unless they are merged into it.
    #[error("this folder contains the sources {0}; merge them into it, or add another folder")]
    ContainsSources(String),
    /// The previews database, or decoding a photo for its thumbnail, refused an operation (WP8).
    #[error(transparent)]
    Imaging(#[from] auroraw_imaging::ImagingError),
}

/// The result type this crate uses throughout.
pub type Result<T> = std::result::Result<T, EngineError>;
