# Using ai-memory

English. 中文：[USAGE.zh-CN.md](USAGE.zh-CN.md) · Architecture: [ARCHITECTURE.md](ARCHITECTURE.md) · DeepSeek Harness: [INTEGRATION_DSH.md](INTEGRATION_DSH.md) · **Install in dsh (5 minutes):** [INSTALL_DSH.md](INSTALL_DSH.md)

**What it is:** a local Rust + SQLite library that stores agent memory (working / episodic / profile) per **project**. Your harness (pi, Claude-like, Codex-like) still owns the model loop. This crate stores, recalls, and packs a **small** prompt slice.

**What it is not:** an LLM client, a 1M-token prompt dump, or a background job that consolidates for you.

## Minimal example

```toml
[dependencies]
ai-memory = { git = "https://github.com/zzjzzb/ai-memory" }
```

```rust
use ai_memory::{open, MemoryPolicy, MemoryStore, RememberRequest, Tier, TokenBudget};

fn main() -> ai_memory::Result<()> {
    let store = open("./memory.db")?;
    store.create_project("support-bot", MemoryPolicy::chat())?;
    let session = store.session("support-bot")?;

    // Persist the turn (do this as the session grows)
    session.remember_turn([
        RememberRequest::new("User prefers dark mode").with_tier(Tier::Profile),
        RememberRequest::new("Ticket: sidebar overlap on invoices").with_tier(Tier::Working),
    ])?;

    // Before the model: a budgeted pack — not the whole transcript
    let pack = session.prefetch_within_budget("sidebar invoices", TokenBudget::new(8_192))?;
    let _system = pack.render(); // paste into your harness system prompt

    // You still call these on purpose (never automatic)
    session.compact_working()?;      // fold old working → one episodic note
    session.end_turn_consolidate()?; // expire / promote by MemoryPolicy
    Ok(())
}
```

```bash
cargo run --example harness_loop_sim
cargo run --example assistant_sim
cargo run --example two_projects
# dsh flagship scenario (Cordis plugin host, no Web UI):
cargo build --bin ai-memory && node scenarios/dsh-support-agent/sim/run.mjs
```

`open()` already turns on WAL and other SQLite defaults. No extra knobs.

## One long session, many tickets, transcript > ~1M tokens

SME agents often stay in **one session** across related issues until the chat is larger than the model window (1M or much smaller).

**You cannot fit that history into the next model call.** Store it; send a budgeted slice.

| Do | Don't |
|----|--------|
| `remember` / `remember_turn` each turn into the project | Stuff the full transcript into the prompt |
| `prefetch_within_budget(query, TokenBudget { max_tokens: 2_000..=32_000 })` before the model | Claim the crate “supports 1M-token prompts” |
| `pin` facts that must survive a tight budget | Hand-roll truncation in every harness |
| `compact_working` when working notes pile up (optional, explicit) | Auto-summarize with a network LLM (not in this crate) |
| `end_turn_consolidate` when you mean to expire/promote | Expect consolidate to run in the background |

**Pattern**

1. Persist user/assistant notes as working (and durable facts as profile).
2. Before each model call, hybrid-recall + pack until the **token budget** is full (pins and high scores first). Tokens default to `ceil(chars/4)` — no tiktoken. Plug in [`TokenEstimator`](../src/harness/tokens.rs) if you have a real tokenizer.
3. Optionally compact: keep pinned + newest working, fold the rest into **one extractive episodic note**, forget those working rows. Default [`ExtractiveCompactor`](../src/harness/compact.rs) is offline. A later `Compactor` may call an LLM; the default must not.
4. Still call `consolidate` yourself for TTL delete + tier promotion.

This crate never puts 1M tokens into the model. It keeps 1M+ of *stored* session on disk and feeds ~2k–32k per turn.

Worked example (dsh plugin + seed tickets): [scenarios/dsh-support-agent/](../scenarios/dsh-support-agent/README.md).

## Harness loop

```rust
use ai_memory::{memory_tool_specs, MemoryPolicy, SqliteStore, TokenBudget, TOOL_RECALL};
use serde_json::json;

let store = SqliteStore::open("./memory.db")?;
store.create_project("support-bot", MemoryPolicy::chat())?;
let session = store.session("support-bot")?; // or AgentSession::attach

let _tools = memory_tool_specs(); // openai_tool() / anthropic_tool()

let pack = session.prefetch_within_budget("user question", TokenBudget::new(4096))?;
let _system = pack.render(); // cites id / tier / score

let _ = session.call_tool(TOOL_RECALL, json!({"text": "theme", "limit": 5}));
session.compact_working()?;
session.end_turn_consolidate()?;
```

Tools: `memory_remember`, `memory_recall`, `memory_forget`, `memory_pin`, `memory_consolidate`. `call_tool` never panics (`ok: false` on errors).

Unbudgeted `prefetch` + `pack_context` still exist if you want raw hits.

## API in one page

**Open:** `open("./memory.db")` / `open_in_memory()` / `SqliteStore::builder()`. Inject [`Embedder`](#embedder) if you have a real one; default is offline `HashEmbedder`.

**Projects:** `create_project`, `store.project("id")`, `store.session("id")`. Isolation is `project_id` — recall cannot leak across projects.

**Write:** `remember` (one row) or `remember_many` / `remember_turn` (one transaction). Optional `tier`; otherwise a small keyword heuristic.

**Read:** `get`, `list_memories` / `list_memories_filtered`, `recall`. Default `recall("...")` limit is 8. Expired unpinned rows are hidden from recall immediately (pin survives).

**Budgeted read:** `prefetch_within_budget` / `pack_context_budgeted`. Inspect `pack.tokens`.

**Pin / forget / compact / consolidate:** `pin` keeps a row past TTL and prefers it in a tight pack. `compact_working` ≠ `consolidate`. Consolidate deletes expired unpinned rows and promotes working→episodic→profile.

## MemoryPolicy (per project)

| Knob | Default |
|------|---------|
| working TTL | 24h |
| episodic TTL | 30 days |
| profile TTL | keep |
| working→episodic | 1 hour |
| episodic→profile | 7 days and ≥ 2 accesses |
| pinned skips expiry | true |
| recall mix | time 0.30, keyword 0.30, vector 0.40 |
| recency half-life | 7 days |
| candidate_prune / scan_limit | 256 / 2048 |

Presets: `MemoryPolicy::chat()`, `MemoryPolicy::journal()`. Keep retention **longer** than the promote delay.

## Transparent performance

`open()` / `store.session` already apply WAL, `synchronous=NORMAL`, `foreign_keys`, `temp_store=MEMORY`, ~16 MiB cache, 5s busy timeout, statement cache, embed LRU, and recall prune. Inspect `store.applied_pragmas()`.

You still choose `remember_many` for a batch of notes, and you still **call `consolidate` and `compact_working` yourself**.

```bash
cargo bench   # not part of cargo test
```

## Embedder and vectors

```rust
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

Default recall: brute-force cosine. Optional `--features sqlite-vec` for `SqliteVecIndex` (not in default `cargo test`).

## Tiers

| Tier | Role |
|------|------|
| `working` | This session / scratch |
| `episodic` | Day / ticket logs (including compact folds) |
| `profile` | Durable facts |
