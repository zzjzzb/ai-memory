# ai-memory

Personal AI memory semantic layer: **working / episodic / profile** memories with **hybrid recall** (time + keyword + vector), scoped per **project**, on a single **Rust + SQLite** kernel.

This is a library crate. Language bindings, sync, and cloud are out of scope.

**Usage docs:** [English](docs/USAGE.md) · [简体中文](docs/USAGE.zh-CN.md)  
**Architecture:** [English](docs/ARCHITECTURE.md) · [简体中文](docs/ARCHITECTURE.zh-CN.md)

This crate is the **memory layer under** agent harnesses (e.g. [pi](https://github.com/earendil-works/pi), Claude / Codex / DeepSeek-style loops). It is not a full LLM harness. Use `AgentSession` + JSON `ToolSpec`s to register `memory_remember` / `memory_recall` / `memory_forget` / `memory_pin` / `memory_consolidate`.

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
    let project = store.project("my-app")?;

    project.remember(
        RememberRequest::new("User prefers dark mode")
            .with_tier(Tier::Profile),
    )?;

    let hits = project.recall(RecallQuery::new("theme preference"))?;
    for hit in hits {
        println!("{:.3} [{}] {}", hit.score, hit.memory.tier, hit.memory.text);
    }

    project.consolidate()?;
    Ok(())
}
```

`open_in_memory()` is the same API without a file. Inject an embedder with `open_with_embedder` / `open_in_memory_with_embedder` / `SqliteStore::builder().embedder(...)`. The default is a deterministic hash encoder (no network, no API keys).

Demos:

```bash
cargo run --example two_projects
cargo run --example assistant_sim
cargo run --example harness_loop_sim
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  MemoryStore trait   (Lance can implement this later)   │
│                                                         │
│  SqliteStore  ── rusqlite / one .db file ─────────────► │
│       │                                                 │
│       ├── projects (id + MemoryPolicy JSON)             │
│       ├── memories (project_id, tier, text, pin, …)     │
│       └── embeddings (blob; brute-force cosine in crate)│
└─────────────────────────────────────────────────────────┘
```

**One stack for all projects.** Isolation and behavior differences come from `project_id` + `MemoryPolicy`, not from swapping storage engines.

### Tiers

| Tier | Role |
|------|------|
| `working` | Session / ephemeral notes |
| `episodic` | Day / event logs |
| `profile` | Durable long-term facts |

`remember` takes an optional explicit tier. If omitted, a small keyword heuristic assigns profile / episodic / working.

### Project isolation

Every memory row carries a `project_id`. Opens are not magically global: `remember` / `recall` / `pin` / `forget` / `consolidate` all take a project id (or use `store.project("id")` which bakes it in). Recall SQL is always `WHERE project_id = ?`. Using another project's memory id for pin/forget still fails (`MemoryNotFound`).

### MemoryPolicy (per project)

Configurable, with defaults:

- **Retention / TTL** per tier (`None` = keep forever). **Recall and default list omit expired unpinned rows** immediately; `consolidate` deletes them. Pinned rows skip expiry when `pinned_skip_expiry` is true.
- **Promote / consolidate**: working → episodic after an age; episodic → profile after age **and** a minimum `access_count` (recall increments it).
- **Recall weights**: mix of recency (exponential half-life), keyword token overlap, and cosine similarity. Weights are normalized at score time.

Keep each tier’s retention **longer** than its promote delay, or items expire before they can move up.

### Hybrid recall

For candidates in **that project only** (optional time window + tier filter):

```
score = w_time * recency + w_keyword * token_overlap + w_vector * cosine⁺
```

Hits are ranked and returned with component scores.

### Vectors

- [`Embedder`](src/embedder.rs) trait + [`HashEmbedder`](src/embedder.rs). Inject on open (see [USAGE](docs/USAGE.md)).
- [`VectorIndex`](src/vector.rs) trait + in-crate [`BruteForceCosine`](src/vector.rs) (default).

Optional `--features sqlite-vec` compiles [sqlite-vec](https://github.com/asg017/sqlite-vec) 0.1.6 and exposes `SqliteVecIndex`. Default `cargo test` does **not** enable it. Inject explicitly:

```rust
SqliteStore::builder()
    .vector_index(std::sync::Arc::new(ai_memory::SqliteVecIndex::new()?))
    .build()?;
```

(`SqliteVecIndex` is only in the crate root when the feature is on.)

A Lance backend would implement `MemoryStore` (and optionally `VectorIndex`) the same way. Still one engine per deployment; policies stay per project.

## Non-goals

- No Zig, no custom database engine
- No sync, server, or multi-tenant cloud
- No Node / Python / mobile FFI bindings (future work)
- No network embedding APIs in the default crate path

## Future work

- Real embedders (ONNX / token APIs) behind `Embedder`
- Query `vec0` virtual tables inside SQLite instead of scoring candidate blobs
- Optional Lance `MemoryStore`
- Language bindings once the Rust API is stable
- Sync / multi-device (explicitly not this crate’s job yet)

## Develop

```bash
cargo test
cargo test --features sqlite-vec   # optional extension path
cargo bench                        # not part of cargo test
cargo run --example two_projects
cargo run --example assistant_sim
cargo run --example harness_loop_sim
```

`open()` applies WAL and other SQLite defaults automatically. Performance knobs stay out of the hot path; you still call `consolidate` yourself. See [USAGE](docs/USAGE.md#transparent-performance).
