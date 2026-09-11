//! Stable host surface for Cordis / Node / CLI bindings.
//!
//! The DeepSeek Harness plugin and the `ai-memory` CLI call this module.
//! Memory logic stays in [`crate::SqliteStore`] / [`crate::AgentSession`].

use std::path::Path;

use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::harness::{AgentSession, TokenBudget};
use crate::policy::MemoryPolicy;
use crate::sqlite::SqliteStore;
use crate::store::MemoryStore;
use crate::{open, open_in_memory};

/// Same name as the harness tool / dsh plugin tool.
pub const OP_PREFETCH: &str = "prefetch_within_budget";
/// Explicit extractive fold (not a background job; not `memory_consolidate`).
pub const OP_COMPACT: &str = "memory_compact";

/// Project-scoped session opened for an external host (dsh, CLI, napi).
#[derive(Clone, Debug)]
pub struct HostSession {
    session: AgentSession,
}

impl HostSession {
    /// Open (or create) `db_path`, ensure `project_id` exists, bind a session.
    pub fn open(
        db_path: impl AsRef<Path>,
        project_id: impl AsRef<str>,
        policy: &str,
    ) -> Result<Self> {
        let store = open(db_path)?;
        Self::from_store(store, project_id.as_ref(), policy)
    }

    /// In-memory store for tests / scratch.
    pub fn open_in_memory(project_id: impl AsRef<str>, policy: &str) -> Result<Self> {
        let store = open_in_memory()?;
        Self::from_store(store, project_id.as_ref(), policy)
    }

    fn from_store(store: SqliteStore, project_id: &str, policy: &str) -> Result<Self> {
        if project_id.trim().is_empty() {
            return Err(Error::InvalidToolArgs("project id must not be empty".into()));
        }
        if store.get_project(project_id)?.is_none() {
            store.create_project(project_id, parse_policy(policy)?)?;
        }
        Ok(Self {
            session: store.session(project_id)?,
        })
    }

    pub fn project_id(&self) -> &str {
        self.session.project_id()
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    /// Dispatch a host op. Never panics; envelope is `{ok, name, data, error}`.
    ///
    /// Known ops: the five harness tools (`memory_*`), plus `prefetch_within_budget`
    /// / `prefetch` and `memory_compact` / `compact`.
    pub fn dispatch(&self, op: &str, args: Value) -> Value {
        match op {
            OP_PREFETCH | "prefetch" => match self.prefetch(&args) {
                Ok(data) => envelope(op, true, data, None),
                Err(e) => envelope(op, false, Value::Null, Some(e.to_string())),
            },
            OP_COMPACT | "compact" | "compact_working" => match self.session.compact_working() {
                Ok(report) => envelope(
                    op,
                    true,
                    json!({
                        "kept_working": report.kept_working,
                        "folded_working": report.folded_working,
                        "forgotten_working": report.forgotten_working,
                        "episodic_id": report.episodic_id,
                    }),
                    None,
                ),
                Err(e) => envelope(op, false, Value::Null, Some(e.to_string())),
            },
            other => {
                let r = self.session.call_tool(other, args);
                envelope(&r.name, r.ok, r.data, r.error)
            }
        }
    }

    fn prefetch(&self, args: &Value) -> Result<Value> {
        let query = args
            .get("query")
            .or_else(|| args.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let max_tokens = args
            .get("max_tokens")
            .or_else(|| args.get("token_budget"))
            .and_then(|v| v.as_u64())
            .unwrap_or(TokenBudget::DEFAULT_MAX as u64) as usize;
        if max_tokens == 0 {
            return Err(Error::InvalidToolArgs(
                "max_tokens must be greater than 0".into(),
            ));
        }
        let pack = self
            .session
            .prefetch_within_budget(query, TokenBudget::new(max_tokens))?;
        Ok(json!({
            "project_id": pack.project_id,
            "tokens": pack.tokens,
            "max_tokens": max_tokens,
            "text": pack.render(),
            "blocks": pack.blocks.iter().map(|b| json!({
                "id": b.id,
                "tier": b.tier.as_str(),
                "score": b.score,
                "text": b.text,
            })).collect::<Vec<_>>(),
        }))
    }
}

/// `chat` (default) / `journal` / `default`.
pub fn parse_policy(name: &str) -> Result<MemoryPolicy> {
    match name.trim() {
        "" | "chat" => Ok(MemoryPolicy::chat()),
        "journal" => Ok(MemoryPolicy::journal()),
        "default" => Ok(MemoryPolicy::default()),
        other => Err(Error::InvalidPolicy(format!(
            "unknown policy `{other}` (expected chat, journal, or default)"
        ))),
    }
}

fn envelope(name: &str, ok: bool, data: Value, error: Option<String>) -> Value {
    json!({
        "ok": ok,
        "name": name,
        "data": data,
        "error": error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_policy_names() {
        assert!(parse_policy("chat").is_ok());
        assert!(parse_policy("journal").is_ok());
        assert!(parse_policy("default").is_ok());
        assert!(parse_policy("nope").is_err());
    }

    #[test]
    fn dispatch_remember_prefetch_roundtrip() {
        let host = HostSession::open_in_memory("dsh", "chat").unwrap();
        let saved = host.dispatch(
            "memory_remember",
            json!({"text": "User prefers dark mode", "tier": "profile"}),
        );
        assert_eq!(saved["ok"], true, "{saved}");
        let pack = host.dispatch(
            OP_PREFETCH,
            json!({"query": "theme preference", "max_tokens": 256}),
        );
        assert_eq!(pack["ok"], true, "{pack}");
        let text = pack["data"]["text"].as_str().unwrap();
        assert!(text.contains("dark mode"));
        assert!(pack["data"]["tokens"].as_u64().unwrap() > 0);
    }
}
