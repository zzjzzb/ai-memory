# dsh-ai-memory

Thin [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) **Host** plugin. It is a Cordis `apply(ctx)` layer over the **Rust** `ai-memory` crate (SQLite + `MemoryPolicy` + `prefetch_within_budget`). It does **not** reimplement memory in TypeScript and does **not** pitch auto-LLM extraction.

Endorsement / architecture: [INTEGRATION_DSH.md](../../docs/INTEGRATION_DSH.md) · [中文](../../docs/INTEGRATION_DSH.zh-CN.md)

**Flagship consumer demo:** [SME support / ops scenario](../../scenarios/dsh-support-agent/README.md) · [中文](../../scenarios/dsh-support-agent/README.zh-CN.md) — seed tickets, headless Cordis `apply(ctx)` sim, and `dsh plugin add` steps.

## Install

From a clone of this repository (preferred while the bundle lives in a subdirectory):

```bash
dsh plugin --profile demo add ./integrations/dsh-ai-memory
dsh --profile demo --dump-config   # look for "# == dsh-ai-memory"
```

Git install (full repo; pnpm subdirectory protocol). First add may fail until you allow the `prepare` script (it compiles Rust):

```bash
dsh plugin --profile demo add github:zzjzzb/ai-memory#path:integrations/dsh-ai-memory
```

Then in the profile `pnpm-workspace.yaml`:

```yaml
allowBuilds:
  dsh-ai-memory: true
```

Re-run `dsh plugin --profile demo add github:zzjzzb/ai-memory#path:integrations/dsh-ai-memory`.

`prepare` builds the napi addon (in-process) and an `ai-memory` CLI fallback. Pin a commit (`#<sha>:path:…`) so a later push cannot change what compiles on your machine.

This is **not** an official App Store listing. Adding the GitHub topic `dsh-plugin` and submitting the repo at [dsh.pub/submit](https://dsh.pub/en/submit/) is a later, optional step. dsh.pub currently prefers an independently installable package at the **repository root**; this in-repo subdirectory is the consumer/showcase path.

## Configure

`cordis.patch.yml` inserts `id: dsh-ai-memory`. Override in the **profile** patch (later layers replace the whole `config` object):

| Field | Default | Meaning |
|-------|---------|---------|
| `dbPath` | `~/.local/share/ai-memory/dsh.db` | SQLite file (`open()` WAL defaults) |
| `projectId` | `dsh` | Isolation key (`WHERE project_id = ?`) |
| `tokenBudget` | `8192` | `prefetch_within_budget` cap (`ceil(chars/4)`) |
| `policy` | `chat` | `chat` / `journal` / `default` when the project is created |
| `prefetchEnabled` | `true` | Register `ctx.systemPrompt.section` `ai-memory:pack` |
| `sectionOrder` | `40` | Prompt section order (persona is 0; tool guidance is later) |
| `cliPath` | (auto) | Override `ai-memory` CLI if napi is unavailable |

## Tools

Registered on `ctx.tools` with the crate names: `memory_remember`, `memory_recall`, `memory_forget`, `memory_pin`, `memory_consolidate`, plus explicit `memory_compact`.

Budgeted pack is injected as system-prompt section `ai-memory:pack` when `systemPrompt` is present.

## Binding

1. **napi-rs** (`ai-memory-node`) — preferred. `HostSession.dispatch(op, json)` → Rust.
2. **`ai-memory` CLI** — same JSON envelope, subprocess. Documented fallback / TODO to drop once prebuilt napi artifacts are published.

## Tests (no full dsh runtime)

```bash
# from repo root
cargo test
cd integrations/dsh-ai-memory && DSH_AI_MEMORY_SKIP_NATIVE=1 npm test
# usage scenario (needs `cargo build --bin ai-memory` or the napi addon)
npm test --prefix scenarios/dsh-support-agent
```

## Manual smoke

```bash
cargo build --release --bin ai-memory
cargo build --release -p ai-memory-node
cd integrations/dsh-ai-memory && npm test && npm run prepare
dsh plugin --profile demo add link:$(pwd)
dsh --profile demo --dump-config
# In a session: call memory_remember, then see ## Memory in the system prompt.
```
