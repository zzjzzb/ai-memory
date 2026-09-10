# Using ai-memory

English usage guide. 中文版：[USAGE.zh-CN.md](USAGE.zh-CN.md)

`ai-memory` is a **local** Rust library for personal AI memory: short-lived working notes, episodic event logs, and durable profile facts. One **Rust + SQLite** kernel serves every project. Projects differ by [`MemoryPolicy`](#memorypolicy), not by storage engines.

It is **not** a cloud service, sync product, or multi-language SDK (yet).

## Problems it solves

| Scenario | What you do |
|----------|-------------|
| Chat assistant that should remember preferences | `remember` profile facts; `recall` before answering |
| Journal / daily log, separate from chat | Second `project` id — recall cannot leak across projects |
| Scratch notes that should fade | Working-tier TTL; expired unpinned rows are hidden from recall |
| Important notes that must survive TTL | `pin` |
| Promote session notes into longer-term memory | `consolidate` using that project's promote rules |
| Tune ranking (recency vs keywords vs vectors) | Per-project recall weights |

## Features

- Open/create a local SQLite file (or in-memory)
- Project CRUD: create, get, list, update policy, delete (cascades memories)
- `remember` (text + optional metadata + optional tier; heuristic if omitted)
- `get` by id (scoped to project), `list` with tier/time/pin/expiry filters
- `pin` / `unpin` / `forget`
- Hybrid `recall`: time window + keyword overlap + vector cosine, ranked scores
- TTL/retention applies to **recall and default list**, not only `consolidate`
- `consolidate`: expire unpinned rows, working→episodic→profile
- Inject [`Embedder`](#embedder) and [`VectorIndex`](#vectors) on open
- Optional Cargo feature `sqlite-vec` (not default)

Non-goals: sync, server, multi-tenant cloud, FFI bindings, Lance backend, network embedding APIs as the default.

## Quickstart

```toml
[dependencies]
ai-memory = { git = "https://github.com/zzjzzb/ai-memory" }
```

```rust
use std::time::Duration;
use ai_memory::{open, MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, Tier};

fn main() -> ai_memory::Result<()> {
    let store = open("./memory.db")?;

    let mut policy = MemoryPolicy::default();
    policy.recall.vector = 0.5;
    policy.promote.working_to_episodic_after = Duration::from_secs(60 * 60);
    store.create_project("my-app", policy)?;

    let app = store.project("my-app")?;
    app.remember(
        RememberRequest::new("User prefers dark mode").with_tier(Tier::Profile),
    )?;

    for hit in app.recall(RecallQuery::new("theme preference"))? {
        println!("{:.3} [{}] {}", hit.score, hit.memory.tier, hit.memory.text);
    }

    app.consolidate()?;
    Ok(())
}
```

Examples:

```bash
cargo run --example two_projects
cargo run --example assistant_sim
```

## API tour

### Open a store

```rust
use std::sync::Arc;
use ai_memory::{open, open_in_memory, open_in_memory_with_embedder, HashEmbedder, SqliteStore};

let file = open("./memory.db")?;
let mem = open_in_memory()?;
let custom = open_in_memory_with_embedder(Arc::new(HashEmbedder::new(64)))?;
let built = SqliteStore::builder()
    .path("./memory.db")
    .embedder(Arc::new(HashEmbedder::new(32)))
    .build()?;
```

Default embedder is `HashEmbedder` (deterministic, offline). Production quality needs your own `Embedder`.

### Projects

Every memory belongs to one **project**. APIs take `project_id`, or use `store.project("id")`.

```rust
store.create_project("chat", MemoryPolicy::chat())?;
store.create_project("journal", MemoryPolicy::journal())?;
let chat = store.project("chat")?;

store.set_policy("chat", MemoryPolicy::default())?; // update
let _ = store.get_project("chat")?;
let _ = store.list_projects()?;
store.delete_project("journal")?; // cascades memories + embeddings
```

### Remember, get, list, pin, forget

```rust
use ai_memory::{MemoryListFilter, RememberRequest, Tier};

let m = chat.remember(
    RememberRequest::new("scratch: try the new sidebar")
        .with_tier(Tier::Working)
        .with_metadata(serde_json::json!({"role": "user"})),
)?;

let _ = chat.get(&m.id)?;
let profile_only = chat.list_memories_filtered(
    MemoryListFilter::new().with_tiers(vec![Tier::Profile]),
)?;

chat.pin(&m.id)?;
chat.unpin(&m.id)?;
chat.forget(&m.id)?;
```

`list_memories()` hides expired unpinned rows (same rule as recall). Use `.including_expired()` to see them. `get` still returns a row by id so you can `pin` it after the TTL.

### Recall (hybrid)

Always scoped to one project.

```
score = w_time * recency + w_keyword * token_overlap + w_vector * cosine⁺
```

Weights come from that project's `MemoryPolicy.recall` (normalized). Optional `since` / `until` / `tiers` / `min_score` / `limit` on `RecallQuery`.

Hits include `score`, `time_score`, `keyword_score`, `vector_score`. Returning a hit increments `access_count` (used by episodic→profile promotion).

### Consolidate

```rust
let report = chat.consolidate()?;
// report.expired, promoted_to_episodic, promoted_to_profile
```

Order: expire unpinned rows past TTL, then promote working→episodic by age, then episodic→profile by age **and** `access_count`. Keep retention **longer** than the promote delay, or items expire first.

## MemoryPolicy

Per project. Defaults:

| Knob | Default |
|------|---------|
| working TTL | 24h |
| episodic TTL | 30 days |
| profile TTL | none (keep) |
| working→episodic | 1 hour |
| episodic→profile | 7 days and ≥ 2 accesses |
| pinned skips expiry | true |
| recall weights | time 0.30, keyword 0.30, vector 0.40 |
| recency half-life | 7 days |

Presets: `MemoryPolicy::chat()` (vector-heavy, shorter working TTL), `MemoryPolicy::journal()` (keyword-heavy, faster promote).

**TTL and recall:** expired non-pinned memories are omitted from `recall` immediately. You do not have to call `consolidate` first (consolidate still deletes them).

## Isolation

Recall SQL is always `WHERE project_id = ?`. Pin/forget/get with the wrong project do not mutate the other project.

## Embedder

```rust
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

Inject on `open_with_embedder` / `open_in_memory_with_embedder` / `SqliteStore::builder().embedder(...)`. Inspect with `store.embedder()`.

## Vectors

Default: `BruteForceCosine` over embeddings stored as SQLite blobs.

Optional:

```toml
ai-memory = { git = "https://github.com/zzjzzb/ai-memory", features = ["sqlite-vec"] }
```

```rust
use std::sync::Arc;
use ai_memory::{SqliteStore, SqliteVecIndex};

let store = SqliteStore::builder()
    .in_memory()
    .vector_index(Arc::new(SqliteVecIndex::new()?))
    .build()?;
```

`cargo test` (default) stays on brute-force. `cargo test --features sqlite-vec` exercises the extension. If the C extension is awkward in your environment, leave the feature off — default recall still works.

A Lance backend would implement `MemoryStore` later. Do not invent a custom engine.

## Tiers

| Tier | Role |
|------|------|
| `working` | Session / ephemeral |
| `episodic` | Day / event logs |
| `profile` | Durable facts |

Omitted tier → small keyword heuristic (`I prefer` → profile, `Today I` → episodic, else working).
