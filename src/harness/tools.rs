use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::store::MemoryStore;
use crate::types::{RecallQuery, RememberRequest, Tier};

use super::context::ContextPack;

pub const TOOL_REMEMBER: &str = "memory_remember";
pub const TOOL_RECALL: &str = "memory_recall";
pub const TOOL_FORGET: &str = "memory_forget";
pub const TOOL_PIN: &str = "memory_pin";
pub const TOOL_CONSOLIDATE: &str = "memory_consolidate";

/// JSON Schema tool descriptor (harness-agnostic).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema object (`type`, `properties`, `required`).
    pub parameters: Value,
}

impl ToolSpec {
    /// OpenAI-style `{type: function, function: {name, description, parameters}}`.
    pub fn openai_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }

    /// Anthropic-style `{name, description, input_schema}`.
    pub fn anthropic_tool(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.parameters,
        })
    }
}

/// Result of [`dispatch_tool`] / [`super::AgentSession::call_tool`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolResponse {
    pub ok: bool,
    pub name: String,
    pub data: Value,
    pub error: Option<String>,
}

impl ToolResponse {
    pub fn ok(name: impl Into<String>, data: Value) -> Self {
        Self {
            ok: true,
            name: name.into(),
            data,
            error: None,
        }
    }

    pub fn err(name: impl Into<String>, error: impl ToString) -> Self {
        Self {
            ok: false,
            name: name.into(),
            data: Value::Null,
            error: Some(error.to_string()),
        }
    }
}

/// Five memory tools for a generic tool-calling loop.
pub fn memory_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: TOOL_REMEMBER.into(),
            description:
                "Store a memory in the current agent project. Optional tier: working, episodic, profile."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Memory text to store"},
                    "tier": {
                        "type": "string",
                        "enum": ["working", "episodic", "profile"]
                    },
                    "metadata": {"type": "object"}
                },
                "required": ["text"]
            }),
        },
        ToolSpec {
            name: TOOL_RECALL.into(),
            description: "Recall memories in the current project (hybrid time + keyword + vector)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Query text"},
                    "limit": {"type": "integer", "minimum": 1}
                },
                "required": ["text"]
            }),
        },
        ToolSpec {
            name: TOOL_FORGET.into(),
            description: "Delete a memory by id in the current project.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string"}
                },
                "required": ["memory_id"]
            }),
        },
        ToolSpec {
            name: TOOL_PIN.into(),
            description: "Pin a memory so retention TTL will not expire it.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string"}
                },
                "required": ["memory_id"]
            }),
        },
        ToolSpec {
            name: TOOL_CONSOLIDATE.into(),
            description: "Apply this project's retention and promote rules (working→episodic→profile)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

pub fn dispatch_tool(
    store: &dyn MemoryStore,
    project_id: &str,
    name: &str,
    args: &Value,
) -> Result<Value> {
    match name {
        TOOL_REMEMBER => {
            let text = req_str(args, "text")?;
            let mut req = RememberRequest::new(text);
            if let Some(tier) = opt_str(args, "tier") {
                req = req.with_tier(Tier::parse(&tier)?);
            }
            if let Some(meta) = args.get("metadata").cloned() {
                if !meta.is_null() {
                    req = req.with_metadata(meta);
                }
            }
            let mem = store.remember(project_id, req)?;
            Ok(json!({
                "id": mem.id,
                "tier": mem.tier.as_str(),
                "text": mem.text,
                "pinned": mem.pinned,
            }))
        }
        TOOL_RECALL => {
            let text = req_str(args, "text")?;
            let mut q = RecallQuery::new(text);
            if let Some(n) = args.get("limit").and_then(|v| v.as_u64()) {
                q = q.with_limit(n as usize);
            }
            let hits = store.recall(project_id, q)?;
            let pack = ContextPack::from_hits(project_id, &hits);
            let listed: Vec<Value> = hits
                .iter()
                .map(|h| {
                    json!({
                        "id": h.memory.id,
                        "tier": h.memory.tier.as_str(),
                        "score": h.score,
                        "text": h.memory.text,
                    })
                })
                .collect();
            Ok(json!({
                "hits": listed,
                "context": pack.render(),
            }))
        }
        TOOL_FORGET => {
            let id = req_str(args, "memory_id")?;
            store.forget(project_id, &id)?;
            Ok(json!({"forgotten": id}))
        }
        TOOL_PIN => {
            let id = req_str(args, "memory_id")?;
            store.pin(project_id, &id)?;
            Ok(json!({"pinned": id}))
        }
        TOOL_CONSOLIDATE => {
            let report = store.consolidate(project_id)?;
            Ok(json!({
                "expired": report.expired,
                "promoted_to_episodic": report.promoted_to_episodic,
                "promoted_to_profile": report.promoted_to_profile,
            }))
        }
        other => Err(Error::UnknownTool(other.to_string())),
    }
}

fn req_str(args: &Value, key: &str) -> Result<String> {
    match args.get(key).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(Error::InvalidToolArgs(format!(
            "missing string field `{key}`"
        ))),
    }
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}
