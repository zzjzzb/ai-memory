# ai-memory

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)

**Open source** local **Rust + SQLite** memory for agent harnesses (pi, Claude-like, Codex-like). You keep the model loop. This crate stores working / episodic / profile notes **per project** and packs a **token-budgeted** slice for the next call.

It does **not** stuff a 1M-token transcript into the prompt, run consolidate in the background, or talk to an LLM.

**Docs:** [USAGE (EN)](docs/USAGE.md) · [用法 (中文)](docs/USAGE.zh-CN.md) · [ARCHITECTURE (EN)](docs/ARCHITECTURE.md) · [架构 (中文)](docs/ARCHITECTURE.zh-CN.md)

**Contribute:** [CONTRIBUTING.md](CONTRIBUTING.md) · [参与贡献](CONTRIBUTING.zh-CN.md) · [Issues](https://github.com/zzjzzb/ai-memory/issues) · [Pull requests](https://github.com/zzjzzb/ai-memory/pulls)

## Scenario → call → get

One long session, many related tickets, chat bigger than the model (~1M or smaller): **store each turn, then `prefetch_within_budget`**. Pins and high-score hits fill a 2k–32k token pack (`ceil(chars/4)` by default).

```rust
use ai_memory::{open, MemoryPolicy, MemoryStore, RememberRequest, Tier, TokenBudget};

fn main() -> ai_memory::Result<()> {
    let store = open("./memory.db")?;
    store.create_project("support-bot", MemoryPolicy::chat())?;
    let session = store.session("support-bot")?;

    session.remember_turn([
        RememberRequest::new("User prefers dark mode").with_tier(Tier::Profile),
        RememberRequest::new("Ticket: sidebar overlap").with_tier(Tier::Working),
    ])?;

    let pack = session.prefetch_within_budget("sidebar", TokenBudget::new(8_192))?;
    let _system = pack.render(); // your harness system prompt — not the full history

    session.compact_working()?;       // optional, explicit, offline extractive fold
    session.end_turn_consolidate()?;  // optional, explicit TTL + promote
    Ok(())
}
```

```bash
cargo test
cargo run --example harness_loop_sim
```

`open()` applies WAL and other SQLite defaults. Inject a real [`Embedder`](src/embedder.rs) when you have one; default `HashEmbedder` is offline. Optional `--features sqlite-vec`.

## Do / don't

| Do | Don't |
|----|--------|
| Persist turns with `remember_turn` | Dump the full transcript into the model |
| `prefetch_within_budget(query, TokenBudget { max_tokens })` | Expect 1M tokens to fit in one prompt |
| `pin` must-keep facts | Auto-consolidate or auto-compact |
| Call `compact_working` / `consolidate` when you mean to | Add an LLM client in this crate |

Isolation is `project_id`. Two sessions on one file do not leak recall.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Develop

```bash
cargo test
cargo test --features sqlite-vec
cargo bench
cargo run --example two_projects
cargo run --example assistant_sim
cargo run --example harness_loop_sim
```
