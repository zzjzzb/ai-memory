# Install ai-memory in DeepSeek Harness (5 minutes)

English. 中文：[INSTALL_DSH.zh-CN.md](INSTALL_DSH.zh-CN.md)

**Goal:** one command adds this repo as a real dsh plugin. After that, a long support chat stores turns in **Rust + SQLite** and injects a **token-budgeted** slice (`## Memory (project: …)`), not the full transcript.

This is a Cordis **Host** over the `ai-memory` crate. It is **not** a TypeScript rewrite of memory and **not** an auto-LLM “extract facts” plugin.

**Skim once → copy-paste the blocks → you should see `# == dsh-ai-memory` in `--dump-config`.**

---

## 0. What you are adding (30 seconds)

Imagine one DeepSeek Harness session that handles related tickets all day (sidebar bug, then a duplicate invoice, then a follow-up). Dumping the chat into the model would blow past a ~1M (or smaller) window.

With this plugin the agent:

1. Calls `memory_remember` / `memory_pin` as it works.
2. Before the next model call, dsh injects system-prompt section `ai-memory:pack` from Rust `prefetch_within_budget`.
3. Isolation is `projectId`. Two projects on the same `.db` file do not leak recall.

Flagship walkthrough (no Web UI required): [scenarios/dsh-support-agent/](../scenarios/dsh-support-agent/README.md).

---

## 1. Prerequisites

You need a machine that can run **dsh** and compile this repo once (git install runs a `prepare` script that builds Rust).

| Need | Why | Check |
|------|-----|--------|
| **Node.js 20+** | Plugin `engines`; dsh itself often wants a current Node | `node -v` → `v20` or newer |
| **pnpm** | `dsh plugin add` forwards to pnpm in the profile directory | `pnpm -v` |
| **dsh CLI** | Installs the bundle into a **profile** | `dsh --help` |
| **Git** | `github:zzjzzb/ai-memory` is a git fetch | `git --version` |
| **Rust `cargo`** | `prepare` builds the napi addon + `ai-memory` CLI | `cargo --version` (crate MSRV **1.74+**) |
| **Network** | Public GitHub clone | Browser: [github.com/zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory) |

This repository is **dual-purpose**: `Cargo.toml` is the Rust crate (`ai-memory`); root `package.json` is the **dsh bundle** (`dsh-ai-memory`). You do not install two products.

### 1.1 Install dsh if `dsh --help` fails

Follow the official project: [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness). Typical paths:

```bash
# Example: run the published CLI via npx (starts the web app; also provides `dsh` in that environment)
npx @deepseek-ai/dsh --help

# Or install from the harness repo / your existing dsh setup so `dsh` is on PATH.
# You need `pnpm` on PATH as well — plugin add is pnpm under the hood.
```

If your team already boots `dsh web` or `dsh --profile web`, you are done with this step.

Official bundle contract (why `package.json` + `cordis.patch.yml` matter, and the `allowBuilds` rule): [Package and install a plugin](https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/publish) · [中文](https://deepseek-harness.github.io/deepseek-harness/develop/basic/publish).

### 1.2 Create or reuse a profile

A **profile** is one runnable composition (plugins + config), usually under:

```text
~/.dsh/profiles/<name>/          # default on Linux / macOS
$DSH_HOME/profiles/<name>/       # if you set DSH_HOME
```

The **web** profile is what most people use for the browser UI. First use of `dsh plugin --profile web …` **creates** it if missing.

```bash
# Reuse the web UI profile (recommended)
dsh plugin --profile web list

# Or a dedicated profile (created on first plugin add)
dsh plugin --profile support-ops list
```

You should see a profile directory appear (or an existing one). Remember the **name** (`web`, `support-ops`, …). Every command below uses `--profile <that-name>`.

---

## 2. One-command install from GitHub

**Pin a commit.** A later `main` push must not silently change what compiles on your machine. Copy a full SHA from [Commits](https://github.com/zzjzzb/ai-memory/commits/main) or:

```bash
git ls-remote https://github.com/zzjzzb/ai-memory.git refs/heads/main
```

Then install (replace `web` and `<commit>`):

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
```

Floating `main` (not recommended for production):

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory
```

That spec installs the **repository root**. Root `package.json` declares `"dsh": { "bundle": { "patch": "./cordis.patch.yml" } }`. dsh then appends `dsh-ai-memory` to the profile’s `dsh.profile.bundles` list. You do **not** add a second `github:…#path:integrations/…` spec.

### 2.1 `allowBuilds` / `prepare` (expected the first time)

Git install fetches **source**, not a prebuilt `.node` file. Nothing runs the package’s `build` script. This repo therefore ships a **`prepare`** script: it compiles the napi addon and the `ai-memory` CLI from the Rust crate in the same checkout.

**pnpm ≥10 refuses that `prepare` until you allow it.** The first `add` often **fails**. dsh / pnpm will print a package key. Copy **that exact key** into the profile’s `pnpm-workspace.yaml`. For this plugin the key is `dsh-ai-memory`:

```bash
# Profile file (create it if missing). Default path:
#   ~/.dsh/profiles/web/pnpm-workspace.yaml
```

```yaml
allowBuilds:
  dsh-ai-memory: true
```

If the file already has other keys (`packages:`, …), add `allowBuilds` as a **sibling** at the top level — do not nest it inside `packages`.

Then **re-run the same add**:

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
```

You should see cargo compile (first time: a few minutes). Treat `allowBuilds` as permission to run this package’s install-time code on your machine, **outside** the agent sandbox. Only allow sources you trust. Official wording: [Installing from GitHub: the build-script catch](https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/publish).

Need a compiler? Install Rust from [rustup.rs](https://rustup.rs/), then retry `add`.

### 2.2 Local clone (same bundle, no GitHub fetch)

From a checkout of this repo:

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
dsh plugin --profile web add .
dsh --profile web --dump-config    # look for "# == dsh-ai-memory"
```

Developers can still `dsh plugin add ./integrations/dsh-ai-memory` from a clone; **users** should use the root spec above so `github:zzjzzb/ai-memory` matches dsh.pub.

---

## 3. Verify (you should see this)

### 3.1 Bundle layer (required)

```bash
dsh --profile web --dump-config
```

Search the output for a layer heading like:

```text
# == dsh-ai-memory
```

You should also see the inserted row (`id: dsh-ai-memory`, `name: dsh-ai-memory`) with defaults `projectId: dsh`, `tokenBudget: 8192`.

If that heading is missing, the package installed as a **plain dependency** and did **not** activate. See [Troubleshooting](#8-troubleshooting).

Restart the profile after a successful add so the new layer is composed (`dsh --profile web` / `dsh web`).

### 3.2 Optional: tools + first remember / recall

1. Start dsh with the same profile (`dsh --profile web` or `dsh web`).
2. In a session, the model should have tools named like the crate: `memory_remember`, `memory_recall`, `memory_forget`, `memory_pin`, `memory_consolidate`, `memory_compact`.
3. Ask (or call the tool): remember `User prefers dark mode` as profile.
4. Next user message, look at the **system prompt** (or debug dump). You want a short section:

```text
## Memory (project: dsh, N hits)
…
```

not the entire chat history. That text is `ContextPack.render()` from Rust, injected as Cordis section `ai-memory:pack`.

No dsh UI? Run the headless sim in [section 7](#7-flagship-scenario-headless--how-it-maps-to-real-dsh).

---

## 4. Config knobs (copy-paste)

The bundle layer sets defaults. **Later layers win**, and a patch **replaces the whole `config` object** (it does not deep-merge keys). Override in the **profile** `cordis.patch.yml` (same directory as the profile `package.json`), not by editing this repo.

```yaml
# ~/.dsh/profiles/web/cordis.patch.yml  (example)
- insert:
    - id: dsh-ai-memory
      name: dsh-ai-memory
      config:
        dbPath: ~/.local/share/ai-memory/dsh.db
        projectId: sme-support
        tokenBudget: 8192
        policy: chat
        prefetchEnabled: true
        sectionOrder: 40
        # cliPath: /usr/local/bin/ai-memory   # only if napi failed and you have a CLI
```

| Field | Default | Meaning |
|-------|---------|---------|
| `dbPath` | `~/.local/share/ai-memory/dsh.db` | SQLite file. `open()` applies WAL defaults. Empty → env `AI_MEMORY_DB` or that default. Use `:memory:` only for tests. |
| `projectId` | `dsh` | Isolation key (`WHERE project_id = ?`). Support vs HR should be different ids. |
| `tokenBudget` | `8192` | Cap for `prefetch_within_budget` (`ceil(chars/4)` by default). Try `256` to match the flagship sim; use 2k–32k in real chats. |
| `policy` | `chat` | `chat` / `journal` / `default` — used **when the project is created**. |
| `prefetchEnabled` | `true` | Register system-prompt section `ai-memory:pack`. |
| `sectionOrder` | `40` | Prompt section order (persona is typically 0). |
| `cliPath` | (auto) | Override `ai-memory` CLI if the napi `.node` addon did not load. |

Do **not** also insert the same `id: dsh-ai-memory` row if you only wanted the bundle layer — duplicate `id` can crash boot. Override by `id` in the profile patch as above (one row).

---

## 5. How this differs from me-too Memory plugins

| Typical Memory plugin | This plugin |
|----------------------|-------------|
| Extra LLM call to auto-extract facts | Tools + **explicit** compact / consolidate |
| JS/Python reimplementation of memory | **Rust crate** is the source of truth |
| “Supports 1M-token prompts” | Stores the long session; **packs a slice** |
| One global bag of facts | `projectId` isolation |
| Hidden vector-store magic | Transparent SQLite defaults on `open()` |

dsh may grow its own extraction features. This integration does **not** depend on them.

---

## 6. Binding (napi vs CLI)

1. **napi-rs** (`ai-memory-node`) — preferred. In-process `HostSession.dispatch`.
2. **`ai-memory` CLI** — same JSON envelope, subprocess. Used if the `.node` file is missing or fails to load.

`prepare` tries to build **both**. You should not rewrite recall in JavaScript.

---

## 7. Flagship scenario (headless) + how it maps to real dsh

Same story as a real support agent: tickets **T-1042** (sidebar overlap) and **T-1088** (duplicate invoice), pin **Ada Chen** as billing owner, project `sme-hr` must not leak T-1042.

### 7.1 Headless sim (no dsh CLI / no Web UI)

From a clone of this repo:

```bash
cargo build --bin ai-memory
# optional in-process addon:
# cargo build -p ai-memory-node
# or: npm run prepare

node scenarios/dsh-support-agent/sim/run.mjs
# same checks:
npm test --prefix scenarios/dsh-support-agent
cargo test --test dsh_support_scenario
```

**What you should see:** pack `tokens <= tokenBudget`; header `## Memory (project: sme-support, …)`; tight budget still contains `PINNED-BILLING-OWNER-ADA`; `sme-hr` pack must **not** contain T-1042. Details: [scenarios/dsh-support-agent/README.md](../scenarios/dsh-support-agent/README.md).

The sim builds a fake Cordis `ctx` and calls the same `apply(ctx)` as dsh. That is enough to prove the plugin without the Web UI.

### 7.2 Map the sim onto a real profile

```bash
dsh plugin --profile support-ops add github:zzjzzb/ai-memory#<commit>
# allowBuilds as in §2.1 if needed, then re-add
```

Put the [config example](#4-config-knobs-copy-paste) in that profile with `projectId: sme-support` and `tokenBudget: 256` (or 8192 for real chats). Play lines from [`scenarios/dsh-support-agent/seed/tickets.json`](../scenarios/dsh-support-agent/seed/tickets.json). After remember/pin, the system prompt should show the short `ai-memory:pack` section — same shape the sim printed.

---

## 8. Troubleshooting

| Symptom | Likely cause | What to do |
|---------|--------------|------------|
| First `dsh plugin add github:…` fails mentioning ignored build scripts / `Ignored build scripts` | pnpm ≥10 blocked `prepare` | Add `allowBuilds: { dsh-ai-memory: true }` to the **profile** `pnpm-workspace.yaml`, re-run **the same** `add` |
| `prepare` / cargo error: `rustc` / `cargo` not found | No Rust toolchain | Install via [rustup](https://rustup.rs/), `cargo --version`, re-run `add` |
| Build fails with edition / MSRV errors | Node is fine; **Rust is too old** | Need rustc **1.74+** (see crate `rust-version`). `rustup update` |
| `node: … unexpected token` / plugin does not load | Node too old | `node -v` must be **≥ 20** |
| `dsh: command not found` | CLI not on PATH | Install DeepSeek Harness; use the same environment that already runs `dsh web` |
| Git fetch 401 / `Repository not found` | Not logged in to GitHub, or typo | Repo is **public**. Check the spec `github:zzjzzb/ai-memory`. For private forks, `gh auth login` or a git credential helper |
| `pnpm` not found | dsh plugin add needs pnpm | Install [pnpm](https://pnpm.io/installation), ensure it is on PATH |
| `--dump-config` has **no** `# == dsh-ai-memory` | Package has no `dsh.bundle.patch`, wrong spec, or add did not reconcile | Confirm you added **root** `github:zzjzzb/ai-memory` (not a random subdirectory URL). `dsh plugin --profile web list`. Re-add. |
| Layer name is there but tools missing | Profile not restarted; or `inject` failed | Restart `dsh --profile web`. Check boot logs for module resolve errors |
| Duplicate loader `id: dsh-ai-memory` | Bundle layer **and** a manual insert of the same id | Use **one** path: `dsh plugin add` **or** a manual patch row, not both. Remove the extra insert |
| Addon missing; CLI errors `failed to start` | `prepare` skipped or failed; napi fallback to CLI also missing | Allow builds, re-add, or `npm run prepare` in a clone. Confirm `ai-memory.node` and/or `bin/ai-memory` exist under the installed package |
| napi fails to load, CLI works | Wrong Node ABI / platform `.node` | CLI fallback is expected. Rebuild on this machine (`prepare`) or set `cliPath` |
| Memory empty / wrong tickets | Wrong `projectId` or `dbPath` | Isolation is by project. Check profile config. Default db is `~/.local/share/ai-memory/dsh.db` |
| Pack looks like the full chat | You are not using this plugin’s section, or budget is huge **and** you also dump history elsewhere | Look for `ai-memory:pack` / `## Memory (project:`. Do not paste the transcript into the system prompt yourself |

---

## 9. Next step: submit to dsh.pub (optional)

You do **not** need the catalog to use the plugin. GitHub topic **`dsh-plugin`** is already on [zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory).

When you want a directory row, submit the **repository URL** (bundle at **root**, which this install path now satisfies):

- English: [https://dsh.pub/en/submit/](https://dsh.pub/en/submit/)
- 中文: [https://dsh.pub/zh/submit/](https://dsh.pub/zh/submit/)

dsh.pub reads root package metadata, checks patch / entry / README / license, and does **not** run your code. This document does not submit for you.

---

## Related docs

- Endorsement / architecture: [INTEGRATION_DSH.md](INTEGRATION_DSH.md)
- Plugin internals: [integrations/dsh-ai-memory/README.md](../integrations/dsh-ai-memory/README.md)
- Crate usage: [USAGE.md](USAGE.md)
