use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::sqlite::SqliteStore;
use crate::store::MemoryStore;
use crate::types::{ConsolidateReport, Memory, RecallHit, RecallQuery, RememberRequest};

use super::context::ContextPack;
use super::tools::{dispatch_tool, memory_tool_specs, ToolResponse, ToolSpec};

/// Project-scoped session for one agent in a harness loop.
///
/// Typical turn: [`Self::prefetch`] → [`Self::pack_context`] → model call →
/// [`Self::call_tool`] for each tool use → optional [`Self::end_turn_consolidate`].
#[derive(Clone)]
pub struct AgentSession {
    store: Arc<dyn MemoryStore>,
    project_id: String,
}

impl std::fmt::Debug for AgentSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentSession")
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

impl AgentSession {
    pub fn new(store: Arc<dyn MemoryStore>, project_id: impl Into<String>) -> Result<Self> {
        let project_id = project_id.into();
        if store.get_project(&project_id)?.is_none() {
            return Err(Error::ProjectNotFound(project_id));
        }
        Ok(Self { store, project_id })
    }

    pub fn sqlite(store: SqliteStore, project_id: impl Into<String>) -> Result<Self> {
        Self::new(Arc::new(store), project_id)
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn store(&self) -> &dyn MemoryStore {
        &*self.store
    }

    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        memory_tool_specs()
    }

    /// Recall before the model call.
    pub fn prefetch(&self, query: impl AsRef<str>) -> Result<Vec<RecallHit>> {
        self.prefetch_query(RecallQuery::new(query.as_ref()))
    }

    pub fn prefetch_query(&self, query: RecallQuery) -> Result<Vec<RecallHit>> {
        self.store.recall(&self.project_id, query)
    }

    pub fn pack_context(&self, hits: &[RecallHit]) -> ContextPack {
        ContextPack::from_hits(&self.project_id, hits)
    }

    pub fn remember_turn(
        &self,
        items: impl IntoIterator<Item = RememberRequest>,
    ) -> Result<Vec<Memory>> {
        self.store
            .remember_many(&self.project_id, items.into_iter().collect())
    }

    pub fn end_turn_consolidate(&self) -> Result<ConsolidateReport> {
        self.store.consolidate(&self.project_id)
    }

    /// Map tool name + JSON args → store call. Never panics; `ok: false` on errors.
    pub fn call_tool(&self, name: &str, args: Value) -> ToolResponse {
        match dispatch_tool(&*self.store, &self.project_id, name, &args) {
            Ok(data) => ToolResponse::ok(name, data),
            Err(e) => ToolResponse::err(name, e),
        }
    }
}
