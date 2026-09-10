use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::sqlite::SqliteStore;
use crate::store::MemoryStore;
use crate::types::{
    ConsolidateReport, Memory, MemoryListFilter, RecallHit, RecallQuery, RememberRequest, Tier,
};

use super::compact::{CompactPlan, CompactReport, Compactor, ExtractiveCompactor};
use super::context::ContextPack;
use super::tokens::{prefetch_hit_limit, CharsPer4, TokenBudget, TokenEstimator};
use super::tools::{dispatch_tool, memory_tool_specs, ToolResponse, ToolSpec};

/// Project-scoped session for one agent in a harness loop.
///
/// Typical turn: persist notes → [`Self::prefetch_within_budget`] → model call →
/// [`Self::call_tool`] → optional [`Self::compact_working`] / [`Self::end_turn_consolidate`].
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

    /// Same as [`Self::new`] — bind an existing store to a project.
    pub fn attach(store: Arc<dyn MemoryStore>, project_id: impl Into<String>) -> Result<Self> {
        Self::new(store, project_id)
    }

    pub fn remember(&self, req: RememberRequest) -> Result<Memory> {
        self.store.remember(&self.project_id, req)
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

    /// Recall before the model call (unbounded pack — you still choose `limit`).
    pub fn prefetch(&self, query: impl AsRef<str>) -> Result<Vec<RecallHit>> {
        self.prefetch_query(RecallQuery::new(query.as_ref()))
    }

    pub fn prefetch_query(&self, query: RecallQuery) -> Result<Vec<RecallHit>> {
        self.store.recall(&self.project_id, query)
    }

    pub fn pack_context(&self, hits: &[RecallHit]) -> ContextPack {
        ContextPack::from_hits(&self.project_id, hits)
    }

    /// Recall extra candidates, then pack to `budget` (default estimator: chars/4).
    pub fn prefetch_within_budget(
        &self,
        query: impl AsRef<str>,
        budget: TokenBudget,
    ) -> Result<ContextPack> {
        self.prefetch_within_budget_with(query, budget, &CharsPer4)
    }

    pub fn prefetch_within_budget_with(
        &self,
        query: impl AsRef<str>,
        budget: TokenBudget,
        estimator: &dyn TokenEstimator,
    ) -> Result<ContextPack> {
        let limit = prefetch_hit_limit(budget.max_tokens);
        let hits = self.prefetch_query(RecallQuery::new(query.as_ref()).with_limit(limit))?;
        Ok(self.pack_context_budgeted_with(&hits, budget, estimator))
    }

    pub fn pack_context_budgeted(&self, hits: &[RecallHit], budget: TokenBudget) -> ContextPack {
        self.pack_context_budgeted_with(hits, budget, &CharsPer4)
    }

    pub fn pack_context_budgeted_with(
        &self,
        hits: &[RecallHit],
        budget: TokenBudget,
        estimator: &dyn TokenEstimator,
    ) -> ContextPack {
        ContextPack::from_hits_budgeted(&self.project_id, hits, budget, estimator)
    }

    pub fn remember_turn(
        &self,
        items: impl IntoIterator<Item = RememberRequest>,
    ) -> Result<Vec<Memory>> {
        self.store
            .remember_many(&self.project_id, items.into_iter().collect())
    }

    /// Fold older unpinned working rows into one extractive episodic note.
    /// Does **not** call consolidate. Default: [`ExtractiveCompactor`].
    pub fn compact_working(&self) -> Result<CompactReport> {
        self.compact_working_with(&ExtractiveCompactor::default())
    }

    pub fn compact_working_with(&self, compactor: &dyn Compactor) -> Result<CompactReport> {
        let working = self.store.list_memories_filtered(
            &self.project_id,
            MemoryListFilter::new().with_tiers(vec![Tier::Working]),
        )?;
        let CompactPlan { keep_ids, fold } = compactor.plan(&working);
        if fold.is_empty() {
            return Ok(CompactReport {
                kept_working: keep_ids.len(),
                folded_working: 0,
                forgotten_working: 0,
                episodic_id: None,
            });
        }

        let n_fold = fold.len();
        let text = compactor
            .folded_text(&fold)
            .unwrap_or_else(|| "Folded working notes (extractive):".into());
        let source_ids: Vec<String> = fold.iter().map(|m| m.id.clone()).collect();
        let episodic = self.store.remember(
            &self.project_id,
            RememberRequest::new(text)
                .with_tier(Tier::Episodic)
                .with_metadata(serde_json::json!({
                    "compacted": true,
                    "source_ids": source_ids,
                })),
        )?;
        for id in &source_ids {
            self.store.forget(&self.project_id, id)?;
        }
        Ok(CompactReport {
            kept_working: keep_ids.len(),
            folded_working: n_fold,
            forgotten_working: n_fold,
            episodic_id: Some(episodic.id),
        })
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

impl crate::sqlite::SqliteStore {
    /// Zero-config session: inherits this store's PRAGMAs, embed cache, and indexes.
    pub fn session(&self, project_id: impl Into<String>) -> Result<AgentSession> {
        AgentSession::sqlite(self.clone(), project_id)
    }
}
