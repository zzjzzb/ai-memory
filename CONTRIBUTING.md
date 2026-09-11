# Contributing to ai-memory

Thanks for helping. This crate is a **local memory layer** for agent harnesses — not an LLM client and not a cloud sync product.

中文版：[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)

## Quick start

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
cargo test
cargo run --example harness_loop_sim
```

Optional vector extension tests:

```bash
cargo test --features sqlite-vec
```

## How to contribute

1. Open an [issue](https://github.com/zzjzzb/ai-memory/issues) for bugs or design discussion (preferred before large changes).
2. Fork the repo and create a branch from `main`.
3. Keep changes focused. Prefer boring, maintainable Rust.
4. Add or update tests for behavior you change. `cargo test` must stay green.
5. Update docs when you change public API or the recommended harness pattern:
   - [docs/USAGE.md](docs/USAGE.md) / [docs/USAGE.zh-CN.md](docs/USAGE.zh-CN.md)
   - [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) / [docs/ARCHITECTURE.zh-CN.md](docs/ARCHITECTURE.zh-CN.md)
   - DeepSeek Harness showcase: [docs/INTEGRATION_DSH.md](docs/INTEGRATION_DSH.md) / [docs/INTEGRATION_DSH.zh-CN.md](docs/INTEGRATION_DSH.zh-CN.md), [`integrations/dsh-ai-memory/`](integrations/dsh-ai-memory/) (`DSH_AI_MEMORY_SKIP_NATIVE=1 npm test` there), and the flagship scenario [`scenarios/dsh-support-agent/`](scenarios/dsh-support-agent/) (`cargo test --test dsh_support_scenario`)
6. Open a pull request against `main` with a short description of *why*.

## Design boundaries (please respect)

**In scope**

- Project-scoped memory, `MemoryPolicy`, hybrid recall, harness adapter (`AgentSession`, `HostSession`, tools, budgeted `ContextPack`)
- Thin DeepSeek Harness Cordis plugin that calls this crate (napi or CLI — not a JS store)
- Usage scenarios that drive that plugin (headless sim + documented `dsh plugin add`)
- Transparent local performance (SQLite defaults, caches, prune)
- Offline-first defaults (no network required for tests)

**Out of scope (unless discussed in an issue first)**

- Building a full LLM harness / vendor SDK (pi, Claude, Codex, DeepSeek clients)
- Multi-device sync or hosted multi-tenant cloud
- Background auto-`consolidate` or silent policy changes
- Stuffing full transcripts into the model prompt (use `prefetch_within_budget`)

## Code style

- Match existing module layout (`store`, `sqlite`, `harness`, …).
- Public API should stay easy for SME developers: good defaults, few knobs.
- Do not add heavy dependencies without a clear win.

## License

Dual-licensed under MIT OR Apache-2.0. Contributions are accepted under the same terms.
