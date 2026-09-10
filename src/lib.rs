//! Personal AI memory semantic layer: working / episodic / profile,
//! hybrid recall, and per-project [`MemoryPolicy`] on a single SQLite kernel.

mod embed_cache;
mod embedder;
mod error;
pub mod harness;
mod heuristic;
mod policy;
mod sqlite;
mod store;
mod types;
mod util;
mod vector;

#[cfg(feature = "sqlite-vec")]
mod sqlite_vec_index;

pub use embedder::{Embedder, HashEmbedder};
pub use error::{Error, Result};
pub use harness::{
    memory_tool_specs, AgentSession, ContextBlock, ContextPack, ToolResponse, ToolSpec,
    TOOL_CONSOLIDATE, TOOL_FORGET, TOOL_PIN, TOOL_RECALL, TOOL_REMEMBER,
};
pub use heuristic::infer_tier;
pub use policy::{MemoryPolicy, PromotePolicy, RecallWeights, RetentionPolicy};
pub use sqlite::{AppliedPragmas, SqliteStore, SqliteStoreBuilder};
pub use store::{MemoryStore, ProjectHandle};
pub use types::{
    ConsolidateReport, Memory, MemoryListFilter, Project, RecallHit, RecallQuery, RememberRequest,
    Tier,
};
pub use vector::{cosine, BruteForceCosine, VectorIndex};

#[cfg(feature = "sqlite-vec")]
pub use sqlite_vec_index::SqliteVecIndex;

use std::path::Path;
use std::sync::Arc;

/// Open or create a local SQLite store at `path` (default [`HashEmbedder`]).
///
/// Applies file-store PRAGMAs automatically (WAL, `synchronous=NORMAL`,
/// `foreign_keys=ON`, `temp_store=MEMORY`, ~16 MiB `cache_size`, 5s busy
/// timeout). See [`SqliteStore::applied_pragmas`].
pub fn open(path: impl AsRef<Path>) -> Result<SqliteStore> {
    SqliteStore::open(path)
}

/// Open or create a local store and inject an [`Embedder`].
pub fn open_with_embedder(
    path: impl AsRef<Path>,
    embedder: Arc<dyn Embedder>,
) -> Result<SqliteStore> {
    SqliteStore::open_with_embedder(path, embedder)
}

/// Open an in-memory store (tests / scratch).
pub fn open_in_memory() -> Result<SqliteStore> {
    SqliteStore::open_in_memory()
}

/// In-memory store with an explicit [`Embedder`].
pub fn open_in_memory_with_embedder(embedder: Arc<dyn Embedder>) -> Result<SqliteStore> {
    SqliteStore::open_in_memory_with_embedder(embedder)
}
