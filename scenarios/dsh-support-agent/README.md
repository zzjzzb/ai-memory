# Scenario: SME support / ops agent (dsh + ai-memory)

English. 中文：[README.zh-CN.md](README.zh-CN.md)

Flagship **usage scenario** for the [dsh-ai-memory](../../integrations/dsh-ai-memory/) Cordis plugin. It is not a second memory product in TypeScript. Rust `HostSession` / `prefetch_within_budget` stay the source of truth.

Endorsement docs: [INTEGRATION_DSH.md](../../docs/INTEGRATION_DSH.md) · [中文](../../docs/INTEGRATION_DSH.zh-CN.md)

**Install the plugin in 5 minutes:** [docs/INSTALL_DSH.md](../../docs/INSTALL_DSH.md) · [中文](../../docs/INSTALL_DSH.zh-CN.md) (`dsh plugin add github:zzjzzb/ai-memory`).

## Story

A mid-size company runs **one long DeepSeek Harness session** for support / ops. Related tickets land in the same chat:

| Ticket | What happens |
|--------|----------------|
| **T-1042** | Maya (Acme): Settings **sidebar overlaps** the invoice table after the nav redesign; follow-up at 150% zoom. |
| **T-1088** | Finance: invoice **INV-22091 charged twice**; they want a reversal. |
| **Pins** | Durable fact: billing owner is **Ada Chen** (`PINNED-BILLING-OWNER-ADA`). |
| **Isolation** | A second project `sme-hr` holds handbook notes on the **same** `.db` — recall must not leak. |

The raw transcript (plus diagnostic dumps) is larger than a typical pack and would blow past a ~1M (or smaller) model window if you stuffed history. The agent **persists each turn** with `memory_remember` and injects a **budgeted** `ai-memory:pack` section (`prefetch_within_budget`) instead.

## Headless / CI path (no dsh Web UI)

The sim constructs a fake Cordis `ctx` and calls `apply(ctx)` from `integrations/dsh-ai-memory` — the same `ctx.tools.register` + `ctx.systemPrompt.section` path dsh uses. Binding is napi-rs or the `ai-memory` CLI. Nothing reimplements recall in JS.

```bash
# from repo root — build the host at least once
cargo build --bin ai-memory
# optional in-process addon
cargo build -p ai-memory-node
# or: npm run prepare --prefix integrations/dsh-ai-memory

node scenarios/dsh-support-agent/sim/run.mjs
# same:
npm test --prefix scenarios/dsh-support-agent
npm run sim --prefix scenarios/dsh-support-agent
```

### What to observe

- **Budgeted pack:** `tokens <= tokenBudget` (seed default 256). Header looks like `## Memory (project: sme-support, N hits)`.
- **Transcript vs pack:** printed raw session `ceil(chars/4)` is **larger** than the pack. Do not dump history.
- **Pins survive:** tight budget (96) still contains `PINNED-BILLING-OWNER-ADA`.
- **Project isolation:** `sme-hr` pack has handbook notes and must **not** contain T-1042 / sidebar overlap.
- **Tools:** `memory_remember`, `memory_recall`, `memory_forget`, `memory_pin`, `memory_consolidate`, `memory_compact`.

Seed: [`seed/tickets.json`](seed/tickets.json). Driver: [`lib/driver.mjs`](lib/driver.mjs).

Rust smoke (same seed, `HostSession`, no Node): `cargo test --test dsh_support_scenario`.

## Real dsh (`dsh plugin add`)

When you have the [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) CLI, use the **root** GitHub spec (same as [INSTALL_DSH.md](../../docs/INSTALL_DSH.md)):

```bash
dsh plugin --profile support-ops add github:zzjzzb/ai-memory#<commit>
dsh --profile support-ops --dump-config    # look for "# == dsh-ai-memory"

# From a clone of this repo:
# dsh plugin --profile support-ops add .

# Git install needs pnpm allowBuilds (prepare compiles Rust):
#   allowBuilds:
#     dsh-ai-memory: true
```

In the **profile** patch, set the same knobs the sim uses:

```yaml
- insert:
    - id: dsh-ai-memory
      name: dsh-ai-memory
      config:
        projectId: sme-support
        tokenBudget: 256          # raise to 8192 for real chats; 256 matches the sim
        policy: chat
        prefetchEnabled: true
```

2. Start dsh with that profile (Web UI or CLI — whatever you already use).
3. Play the seed tickets as user messages (copy lines from `seed/tickets.json`). After each turn, ask the model to `memory_remember` the note (or remember yourself via the tool). Pin the Ada billing fact with `memory_pin`.
4. On the next model call, inspect the system prompt: section **`ai-memory:pack`** must be a short `## Memory (project: sme-support, …)` slice, not the full chat.
5. Optional second profile / `projectId: sme-hr` on the same `dbPath` to confirm isolation.

`dsh` is **not** required for CI. Publishing this bundle to [dsh.pub](https://dsh.pub/en/submit/) is optional; the installable package is the **repository root**. See [INSTALL_DSH.md](../../docs/INSTALL_DSH.md).

## Out of scope

- Rewriting memory in JavaScript
- Auto-LLM extraction / a competing Memory product
- Merging the plugin-only PR as a substitute for this scenario
