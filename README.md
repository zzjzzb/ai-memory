# ai-memory

> **给 Agent 用的本地记忆层：按项目存短中长期记忆，超长会话只往模型塞预算内的一小包；可挂 DeepSeek Harness。**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)

Local **Rust + SQLite** memory for agent harnesses: per-project working / episodic / profile notes, and a **token-budgeted** pack for the next model call (not the full transcript). Optional DeepSeek Harness plugin.

It does **not** stuff a 1M-token transcript into the prompt, run consolidate in the background, or talk to an LLM.

## Install into DeepSeek Harness

Copy-paste guide (pin commit, `allowBuilds`, what you should see):

- English: [docs/INSTALL_DSH.md](docs/INSTALL_DSH.md)
- 中文: [docs/INSTALL_DSH.zh-CN.md](docs/INSTALL_DSH.zh-CN.md)

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
```

Get `<commit>` with `git ls-remote https://github.com/zzjzzb/ai-memory.git refs/heads/main` (left column). After add: `dsh --profile web --dump-config` must show `# == dsh-ai-memory`.

Root `package.json` is the dsh bundle (**npm name `dsh-ai-memory`**). `Cargo.toml` is the Rust crate (**package name `ai-memory`**). Same repo, two manifests, not two products.

**Docs:** [USAGE (EN)](docs/USAGE.md) · [用法 (中文)](docs/USAGE.zh-CN.md) · [ARCHITECTURE (EN)](docs/ARCHITECTURE.md) · [架构 (中文)](docs/ARCHITECTURE.zh-CN.md) · [DeepSeek Harness (EN)](docs/INTEGRATION_DSH.md) · [DeepSeek Harness（中文）](docs/INTEGRATION_DSH.zh-CN.md)

**Flagship dsh demo:** [SME support / ops scenario](scenarios/dsh-support-agent/README.md) · [中文](scenarios/dsh-support-agent/README.zh-CN.md) — one long session, budgeted pack, not a JS memory rewrite.

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
cargo run --bin ai-memory -- --in-memory memory_remember '{"text":"hi","tier":"profile"}'
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

## DeepSeek Harness (showcase)

The intended **consumer / showcase** is [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness): a thin Cordis `apply(ctx)` plugin over **this Rust crate** (SQLite stays here; we do not rewrite memory in JS, and we do not pitch auto-LLM extraction).

Install command and verify steps: **[Install into DeepSeek Harness](#install-into-deepseek-harness)** / [docs/INSTALL_DSH.md](docs/INSTALL_DSH.md).

- Endorsement / architecture: [INTEGRATION_DSH.md](docs/INTEGRATION_DSH.md) · [集成说明（中文）](docs/INTEGRATION_DSH.zh-CN.md)
- Host implementation: [`integrations/dsh-ai-memory/`](integrations/dsh-ai-memory/) (re-exported from the repo root)
- **Flagship usage scenario:** [`scenarios/dsh-support-agent/`](scenarios/dsh-support-agent/) (sidebar + billing tickets; headless `node …/sim/run.mjs` or real `dsh plugin add`)
- Host API: `HostSession` + `ai-memory` CLI; preferred bridge is in-process **napi-rs** (CLI fallback if the `.node` addon is missing)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Develop

```bash
cargo test
cargo test --features sqlite-vec
cargo test -p ai-memory-node
cargo bench
cargo run --example two_projects
cargo run --example assistant_sim
cargo run --example harness_loop_sim
DSH_AI_MEMORY_SKIP_NATIVE=1 npm test --prefix integrations/dsh-ai-memory
node scripts/check-dsh-bundle.mjs
cargo build --bin ai-memory && npm test --prefix scenarios/dsh-support-agent
node scenarios/dsh-support-agent/sim/run.mjs
```

`package.json` at the repo root is only the dsh bundle. Rust-only work does not need `npm install`. If you do run npm scripts and want to skip compiling the host: `DSH_AI_MEMORY_SKIP_NATIVE=1`.
