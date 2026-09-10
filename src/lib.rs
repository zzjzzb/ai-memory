//! Personal AI memory semantic layer: working / episodic / profile,
//! hybrid recall, and per-project [`MemoryPolicy`] on a single SQLite kernel.

mod embedder;
mod error;
mod heuristic;
mod policy;
mod sqlite;
mod store;
mod types;
mod util;
mod vector;

pub use embedder::{Embedder, HashEmbedder};
pub use error::{Error, Result};
pub use heuristic::infer_tier;
pub use policy::{MemoryPolicy, PromotePolicy, RecallWeights, RetentionPolicy};
pub use sqlite::{SqliteStore, SqliteStoreBuilder};
pub use store::{MemoryStore, ProjectHandle};
pub use types::{
    ConsolidateReport, Memory, Project, RecallHit, RecallQuery, RememberRequest, Tier,
};
pub use vector::{cosine, BruteForceCosine, VectorIndex};

use std::path::Path;

/// Open or create a local SQLite store at `path`.
pub fn open(path: impl AsRef<Path>) -> Result<SqliteStore> {
    SqliteStore::open(path)
}

/// Open an in-memory store (tests / scratch).
pub fn open_in_memory() -> Result<SqliteStore> {
    SqliteStore::open_in_memory()
}
