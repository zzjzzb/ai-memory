# dsh-ai-memory

Thin [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) **Host** plugin. It is a Cordis `apply(ctx)` layer over the **Rust** `ai-memory` crate (SQLite + `MemoryPolicy` + `prefetch_within_budget`). It does **not** reimplement memory in TypeScript and does **not** pitch auto-LLM extraction.

**Install in 5 minutes:** [docs/INSTALL_DSH.md](../../docs/INSTALL_DSH.md) · [中文](../../docs/INSTALL_DSH.zh-CN.md)

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
dsh --profile web --dump-config    # look for "# == dsh-ai-memory"
```

The **repository root** is the installable dsh bundle (`dsh.bundle.patch` → `cordis.patch.yml`). This directory is the Host implementation; root `index.js` re-exports it. `prepare` (from root or here) compiles napi + the `ai-memory` CLI.

Endorsement / architecture: [INTEGRATION_DSH.md](../../docs/INTEGRATION_DSH.md) · [中文](../../docs/INTEGRATION_DSH.zh-CN.md)

**Flagship consumer demo:** [SME support / ops scenario](../../scenarios/dsh-support-agent/README.md) · [中文](../../scenarios/dsh-support-agent/README.zh-CN.md) — seed tickets, headless Cordis `apply(ctx)` sim, and `dsh plugin add` steps.

## Install

**Users:** copy-paste from [INSTALL_DSH.md](../../docs/INSTALL_DSH.md) (prerequisites, pin commit, `allowBuilds`, verify, config, troubleshooting). Short form:

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
# First add often fails on pnpm ≥10 until the profile pnpm-workspace.yaml has:
#   allowBuilds:
#     dsh-ai-memory: true
# Then re-run the same add. Official: https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/publish
dsh --profile web --dump-config   # "# == dsh-ai-memory"
```

From a clone of this repository:

```bash
dsh plugin --profile web add .                              # root bundle (same as github:)
dsh plugin --profile web add ./integrations/dsh-ai-memory   # this folder, developers
dsh --profile web --dump-config
```

Do **not** use `github:zzjzzb/ai-memory#path:integrations/dsh-ai-memory` as the supported user path — a subdirectory git fetch does not include the Rust crate `prepare` must compile.

This is **not** an official App Store listing. GitHub topic `dsh-plugin` is already set. Submitting the repo at [dsh.pub/submit](https://dsh.pub/en/submit/) is optional; dsh.pub expects the bundle at the **repository root**.

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

Copy-paste YAML: [INSTALL_DSH.md §4](../../docs/INSTALL_DSH.md#4-config-knobs-copy-paste).

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
node scripts/check-dsh-bundle.mjs
cd integrations/dsh-ai-memory && DSH_AI_MEMORY_SKIP_NATIVE=1 npm test
# usage scenario (needs `cargo build --bin ai-memory` or the napi addon)
npm test --prefix scenarios/dsh-support-agent
```

## Manual smoke

```bash
cargo build --release --bin ai-memory
cargo build --release -p ai-memory-node
cd integrations/dsh-ai-memory && npm test && npm run prepare
# or from repo root: npm run prepare && dsh plugin --profile web add .
dsh plugin --profile web add link:$(pwd)
dsh --profile web --dump-config
# In a session: call memory_remember, then see ## Memory in the system prompt.
```
