//! Thin adapter for agent harnesses (pi, Claude-like, Codex-like, DeepSeek-like).
//!
//! This crate is **not** an LLM loop. Harnesses own tools + model I/O; `AgentSession`
//! binds a project-scoped [`crate::store::MemoryStore`] and exposes JSON tool specs.

mod compact;
mod context;
mod session;
mod tokens;
mod tools;

pub use compact::{CompactPlan, CompactReport, Compactor, ExtractiveCompactor};
pub use context::{ContextBlock, ContextPack};
pub use session::AgentSession;
pub use tokens::{CharsPer4, TokenBudget, TokenEstimator};
pub use tools::{
    memory_tool_specs, ToolResponse, ToolSpec, TOOL_CONSOLIDATE, TOOL_FORGET, TOOL_PIN,
    TOOL_RECALL, TOOL_REMEMBER,
};
