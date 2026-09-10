use crate::error::Result;
use crate::policy::MemoryPolicy;
use crate::types::{
    ConsolidateReport, Memory, MemoryListFilter, Project, RecallHit, RecallQuery, RememberRequest,
};

/// Backend-agnostic memory kernel.
///
/// SQLite is the MVP implementation. A future Lance (or other) store can
/// implement this trait; projects still share one engine and differ only by
/// [`MemoryPolicy`].
pub trait MemoryStore: Send + Sync {
    fn create_project(&self, id: &str, policy: MemoryPolicy) -> Result<Project>;
    fn get_project(&self, id: &str) -> Result<Option<Project>>;
    fn list_projects(&self) -> Result<Vec<Project>>;
    fn set_policy(&self, project_id: &str, policy: MemoryPolicy) -> Result<()>;
    fn policy(&self, project_id: &str) -> Result<MemoryPolicy>;
    /// Delete a project and cascade its memories + embeddings.
    fn delete_project(&self, project_id: &str) -> Result<()>;

    fn remember(&self, project_id: &str, req: RememberRequest) -> Result<Memory>;
    /// Insert many memories in one transaction. Empty input yields an empty vec.
    /// If any item is invalid, nothing is written.
    fn remember_many(&self, project_id: &str, reqs: Vec<RememberRequest>) -> Result<Vec<Memory>>;
    fn get(&self, project_id: &str, memory_id: &str) -> Result<Option<Memory>>;
    fn list_memories(&self, project_id: &str) -> Result<Vec<Memory>>;
    fn list_memories_filtered(
        &self,
        project_id: &str,
        filter: MemoryListFilter,
    ) -> Result<Vec<Memory>>;

    fn pin(&self, project_id: &str, memory_id: &str) -> Result<()>;
    fn unpin(&self, project_id: &str, memory_id: &str) -> Result<()>;
    fn forget(&self, project_id: &str, memory_id: &str) -> Result<()>;

    /// Recall **only** within `project_id`. Ranked hybrid time + keyword + vector.
    fn recall(&self, project_id: &str, query: RecallQuery) -> Result<Vec<RecallHit>>;

    /// Apply that project's retention + promote/consolidate rules.
    fn consolidate(&self, project_id: &str) -> Result<ConsolidateReport>;
}

/// Store scoped to a single project so callers cannot accidentally omit the id.
#[derive(Clone, Debug)]
pub struct ProjectHandle<S> {
    store: S,
    project_id: String,
}

impl<S: MemoryStore + Clone> ProjectHandle<S> {
    pub(crate) fn new(store: S, project_id: String) -> Self {
        Self { store, project_id }
    }

    pub fn id(&self) -> &str {
        &self.project_id
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn policy(&self) -> Result<MemoryPolicy> {
        self.store.policy(&self.project_id)
    }

    pub fn set_policy(&self, policy: MemoryPolicy) -> Result<()> {
        self.store.set_policy(&self.project_id, policy)
    }

    pub fn remember(&self, req: RememberRequest) -> Result<Memory> {
        self.store.remember(&self.project_id, req)
    }

    pub fn remember_many(&self, reqs: Vec<RememberRequest>) -> Result<Vec<Memory>> {
        self.store.remember_many(&self.project_id, reqs)
    }

    pub fn get(&self, memory_id: &str) -> Result<Option<Memory>> {
        self.store.get(&self.project_id, memory_id)
    }

    pub fn list_memories(&self) -> Result<Vec<Memory>> {
        self.store.list_memories(&self.project_id)
    }

    pub fn list_memories_filtered(&self, filter: MemoryListFilter) -> Result<Vec<Memory>> {
        self.store.list_memories_filtered(&self.project_id, filter)
    }

    pub fn delete(self) -> Result<()> {
        self.store.delete_project(&self.project_id)
    }

    pub fn pin(&self, memory_id: &str) -> Result<()> {
        self.store.pin(&self.project_id, memory_id)
    }

    pub fn unpin(&self, memory_id: &str) -> Result<()> {
        self.store.unpin(&self.project_id, memory_id)
    }

    pub fn forget(&self, memory_id: &str) -> Result<()> {
        self.store.forget(&self.project_id, memory_id)
    }

    pub fn recall(&self, query: RecallQuery) -> Result<Vec<RecallHit>> {
        self.store.recall(&self.project_id, query)
    }

    pub fn consolidate(&self) -> Result<ConsolidateReport> {
        self.store.consolidate(&self.project_id)
    }
}
