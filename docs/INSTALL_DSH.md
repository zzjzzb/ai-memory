# Install ai-memory in DeepSeek Harness (5 minutes)

English. 中文：[INSTALL_DSH.zh-CN.md](INSTALL_DSH.zh-CN.md)

One command adds this **repository root** as a dsh plugin. Long chats store turns in **Rust + SQLite** and inject a **token-budgeted** slice (`## Memory (project: …)`), not the full transcript.

This is a Cordis Host over the `ai-memory` crate — **not** a TypeScript memory rewrite and **not** an auto-LLM “extract facts” plugin.

---

## 5-minute path (happy path only)

Do these in order. Use the **same** profile name everywhere (`web` below). If step 3 fails with *Ignored build scripts*, that is expected the first time — do step 4, then re-run step 3.

### 1. Tools

```bash
node -v          # v20 or newer
pnpm -v          # dsh plugin add forwards to pnpm
dsh --help       # DeepSeek Harness CLI
cargo --version  # rustc 1.74+ (prepare compiles Rust)
```

Missing `dsh`? Install from [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) first. Missing `cargo`? [rustup.rs](https://rustup.rs/).

### 2. Pin a commit SHA

```bash
git ls-remote https://github.com/zzjzzb/ai-memory.git refs/heads/main
```

**Expected** (SHA will differ; copy the **left** column, 40 hex characters):

```text
f3ce0b5c1a2b3c4d5e6f7890aabbccddeeff0011	refs/heads/main
```

Call that value `COMMIT`. Paste it into the next command. Do not use floating `main` on a machine you care about.

### 3. Add the plugin

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#COMMIT
```

Replace `COMMIT` with the SHA from step 2 (no `#` doubling).

**Expected on success** (cargo takes a few minutes the first time; lines may interleave with pnpm):

```text
[dsh-ai-memory] building ai-memory CLI (Rust source of truth)…
[dsh-ai-memory] wrote bin/ai-memory
[dsh-ai-memory] building napi addon…
[dsh-ai-memory] wrote ai-memory.node (from libai_memory_node.so)
[dsh-ai-memory] prepare: using napi (CLI also built)
```

macOS may say `libai_memory_node.dylib`; Windows `ai_memory_node.dll`. That is still success.

**Napi fail-soft (still success):** if the addon fails but the CLI was written:

```text
[dsh-ai-memory] napi crate build failed — plugin will use the CLI fallback if present
[dsh-ai-memory] prepare: using CLI fallback (napi addon not built). Runtime still uses the Rust crate, not a JS store.
```

**Expected first-time failure (pnpm ≥10 blocked `prepare`):**

```text
[ERR_PNPM_IGNORED_BUILDS] Ignored build scripts: dsh-ai-memory

Run "pnpm approve-builds" to pick which dependencies should be allowed to run scripts.
```

dsh may also tell you to copy that package key into the profile `pnpm-workspace.yaml`. Go to step 4, then **re-run the exact add** from this step.

<a id="allow-prepare"></a>

### 4. Allow `prepare` (only if step 3 printed Ignored build scripts)

Profile file (create if missing):

```text
~/.dsh/profiles/web/pnpm-workspace.yaml
```

If you set `DSH_HOME`, use `$DSH_HOME/profiles/web/pnpm-workspace.yaml` instead.

**Full file you can paste** when the file does not exist yet:

```yaml
packages:
  - '.'
allowBuilds:
  dsh-ai-memory: true
```

If the file **already exists**, add `allowBuilds` at the **top level** (sibling of `packages`, not nested inside it). Keep other keys. The package key must be exactly `dsh-ai-memory` (what pnpm printed), not the GitHub URL.

Then re-run step 3’s add command (same `COMMIT`).

Treat `allowBuilds` as permission to run this package’s install-time code on your machine, outside the agent sandbox. Official rule: [Package and install a plugin](https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/publish).

### 5. Confirm the layer

Use the **same** `--profile` as `plugin add`:

```bash
dsh --profile web --dump-config
```

**Expected** (search for this heading; surrounding YAML can vary):

```text
# == dsh-ai-memory
```

You should also see a plugin row like:

```yaml
- id: dsh-ai-memory
  name: dsh-ai-memory
  config:
    projectId: dsh
    tokenBudget: 8192
    policy: chat
    prefetchEnabled: true
```

If `# == dsh-ai-memory` is missing, the package is a plain dependency and the layer did **not** activate — see [Troubleshooting](#troubleshooting).

Restart so the layer is composed: `dsh --profile web` or `dsh web`.

### 6. First use: remember one fact + budget pack

Start dsh with `--profile web`. In a chat, paste this:

```text
Call tool memory_remember with text "User prefers dark mode" and tier "profile".
Then call tool memory_recall with text "dark mode".
```

The agent should invoke **these exact tool names**:

| Step | Tool | Arguments |
|------|------|-----------|
| 1 | `memory_remember` | `{ "text": "User prefers dark mode", "tier": "profile" }` |
| 2 | `memory_recall` | `{ "text": "dark mode" }` |

Other crate tools (later): `memory_forget`, `memory_pin`, `memory_consolidate`, `memory_compact`.

On the **next** model call, the system prompt should contain Cordis section `ai-memory:pack`, looking like:

```text
## Memory (project: dsh, 1 hits)
- [profile id=mem-… score=…] User prefers dark mode
```

That is `prefetch_within_budget`, not the full chat. If you see the entire transcript instead, something else is dumping history — this plugin did not.

No Web UI? [Flagship headless sim](#flagship-sim).

---

## What you installed (30 seconds)

One long support session (sidebar bug, then a duplicate invoice) would blow past a ~1M (or smaller) window if you dumped the chat. This plugin:

1. Persists turns with `memory_remember` / `memory_pin`.
2. Injects `ai-memory:pack` from Rust `prefetch_within_budget` before the next model call.
3. Isolates by `projectId`. Two projects on one `.db` do not leak recall.

Root `package.json` npm name is **`dsh-ai-memory`**. `Cargo.toml` crate name is **`ai-memory`**. Same repository.

---

## Prerequisites (if a check in step 1 failed)

| Need | Why | Check |
|------|-----|--------|
| **Node.js 20+** | Plugin `engines`; wrong Node often fails to load `.node` | `node -v` |
| **pnpm** | `dsh plugin add` = pnpm in the profile directory | `pnpm -v` |
| **dsh CLI** | Writes the profile + bundle list | `dsh --help` |
| **Git** | `github:zzjzzb/ai-memory` is a git fetch | `git --version` |
| **Rust `cargo`** | `prepare` builds napi and/or the CLI | `cargo --version` (MSRV **1.74+**) |
| **Network** | Public GitHub | [github.com/zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory) |

`npx @deepseek-ai/dsh --help` is one way to get the CLI if it is not on PATH. You still need `pnpm` for `dsh plugin add`.

A **profile** lives at `~/.dsh/profiles/<name>/` (or `$DSH_HOME/profiles/<name>/`). `web` is the usual UI profile; first `dsh plugin --profile web …` creates it.

Local clone instead of GitHub:

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
dsh plugin --profile web add .
dsh --profile web --dump-config
```

Do **not** use `github:zzjzzb/ai-memory#path:integrations/dsh-ai-memory` as the user path — that git fetch does not include the Rust crate `prepare` must compile.

---

## Config knobs (copy-paste)

Later layers win. A patch **replaces the whole `config` object** (no deep-merge). Edit the **profile** `cordis.patch.yml`, not this repo.

```yaml
# ~/.dsh/profiles/web/cordis.patch.yml
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
        # cliPath: /usr/local/bin/ai-memory
```

| Field | Default | Meaning |
|-------|---------|---------|
| `dbPath` | `~/.local/share/ai-memory/dsh.db` | SQLite file. Empty → `AI_MEMORY_DB` or that default. `:memory:` is for tests. |
| `projectId` | `dsh` | Isolation key. Support vs HR need different ids. |
| `tokenBudget` | `8192` | `prefetch_within_budget` cap (`ceil(chars/4)`). `256` matches the flagship sim. |
| `policy` | `chat` | `chat` / `journal` / `default` — used when the project is **created**. |
| `prefetchEnabled` | `true` | Register `ai-memory:pack`. |
| `sectionOrder` | `40` | Prompt section order (persona is typically 0). |
| `cliPath` | (auto) | Force the `ai-memory` CLI if napi `.node` did not load. |

Do **not** insert a second `id: dsh-ai-memory` row on top of the bundle layer — duplicate `id` can crash boot. Override by `id` with **one** row as above.

---

## How this differs from me-too Memory plugins

| Typical Memory plugin | This plugin |
|----------------------|-------------|
| Extra LLM call to auto-extract facts | Tools + **explicit** compact / consolidate |
| JS/Python reimplementation of memory | **Rust crate** is the source of truth |
| “Supports 1M-token prompts” | Stores the long session; **packs a slice** |
| One global bag of facts | `projectId` isolation |

---

## Binding (napi vs CLI)

1. **napi-rs** — preferred. In-process `HostSession.dispatch`.
2. **`ai-memory` CLI** — same JSON envelope, subprocess, if `ai-memory.node` is missing or fails to load.

`prepare` tries both and **succeeds if either exists**. Nothing reimplements recall in JavaScript.

---

<a id="flagship-sim"></a>

## Flagship scenario (headless)

Tickets **T-1042** (sidebar) and **T-1088** (duplicate invoice), pin Ada Chen, project `sme-hr` must not leak T-1042.

```bash
cargo build --bin ai-memory
node scenarios/dsh-support-agent/sim/run.mjs
npm test --prefix scenarios/dsh-support-agent
cargo test --test dsh_support_scenario
```

**You should see:** `tokens <= tokenBudget`; `## Memory (project: sme-support, …)`; tight pack still contains `PINNED-BILLING-OWNER-ADA`; `sme-hr` pack has **no** T-1042. Details: [scenarios/dsh-support-agent/README.md](../scenarios/dsh-support-agent/README.md).

Map onto a real profile: same `github:zzjzzb/ai-memory#COMMIT` add, set `projectId: sme-support` in the profile patch, play [`seed/tickets.json`](../scenarios/dsh-support-agent/seed/tickets.json).

---

<a id="troubleshooting"></a>

## Troubleshooting

| Symptom | Likely cause | What to do |
|---------|--------------|------------|
| `Ignored build scripts: dsh-ai-memory` / `ERR_PNPM_IGNORED_BUILDS` | pnpm ≥10 blocked `prepare` | Paste [step 4](#allow-prepare) into the **profile** `pnpm-workspace.yaml`, re-run the **same** `add` |
| `allowBuilds` set but add still ignores scripts | Wrong file / nested under `packages` / wrong key | File must be **that profile’s** `pnpm-workspace.yaml`. Key `dsh-ai-memory: true` at top level. Not `ignoredBuiltDependencies` |
| `cargo: command not found` / `rustc` missing during prepare | No Rust toolchain | [rustup](https://rustup.rs/), then re-add. JS-only tests: `DSH_AI_MEMORY_SKIP_NATIVE=1` |
| napi compile error, then `prepare: using CLI fallback` | Addon failed; CLI built | **OK.** Plugin uses the CLI. Confirm `--dump-config` still has `# == dsh-ai-memory` |
| prepare: neither napi nor CLI produced | Both Rust builds failed | `rustc` **1.74+** (`rustup update`). Re-add. Check disk space |
| `SyntaxError` / `Unexpected token` loading the plugin | **Wrong Node** | `node -v` must be **≥ 20**. Rebuild `.node` on this Node (`prepare`) |
| `.node` present but `invalid ELF` / `wrong architecture` | Addon built on another OS/CPU | Rebuild on this machine. Or rely on CLI fallback (`bin/ai-memory`) / set `cliPath` |
| `ai-memory CLI failed to start` | `.node` missing **and** CLI missing/not executable | `allowBuilds` + re-add, or `npm run prepare` in a clone. Look under the installed package for `ai-memory.node` and `bin/ai-memory` |
| `dsh: command not found` | CLI not on PATH | Same environment that already runs `dsh web` |
| Git 401 / `Repository not found` | Typo or private-fork credentials | Public spec is `github:zzjzzb/ai-memory`. `gh auth login` only for private forks |
| `pnpm: command not found` | dsh plugin add needs pnpm | [pnpm.io/installation](https://pnpm.io/installation) |
| `--dump-config` has **no** `# == dsh-ai-memory` | Layer not activated | You added **root** `github:zzjzzb/ai-memory`, not a subdirectory URL? `dsh plugin --profile web list`. Re-add after `allowBuilds`. Confirm root `package.json` has `dsh.bundle.patch` |
| Added plugin but dump-config looks empty / old | **Profile name mismatch** | `dsh plugin --profile web add` then **`dsh --profile web --dump-config`**. Bare `dsh --dump-config` is a different profile. Typo `web` vs `Web` vs `default` |
| Layer heading present, tools missing | Profile not restarted; inject failed | Restart `dsh --profile web`. Boot logs: cannot resolve `dsh-ai-memory` |
| Duplicate loader `id: dsh-ai-memory` | Bundle layer **and** a manual insert | One path only: `dsh plugin add` **or** a manual patch row |
| Empty memory / wrong tickets | Wrong `projectId` or `dbPath` | Default db `~/.local/share/ai-memory/dsh.db`. Isolation is per project |
| Pack looks like the full chat | Other prompt dumping, or you are not looking at `ai-memory:pack` | Search `## Memory (project:`. Do not paste the transcript yourself |

---

## Next step: dsh.pub (optional)

You do **not** need the catalog to use the plugin. Topic **`dsh-plugin`** is already on the repo.

When you want a directory row, submit the repository URL yourself:

- [https://dsh.pub/en/submit/](https://dsh.pub/en/submit/)
- [https://dsh.pub/zh/submit/](https://dsh.pub/zh/submit/)

This guide does not submit for you.

---

## Related docs

- Endorsement / architecture: [INTEGRATION_DSH.md](INTEGRATION_DSH.md)
- Plugin internals: [integrations/dsh-ai-memory/README.md](../integrations/dsh-ai-memory/README.md)
- Crate usage: [USAGE.md](USAGE.md)
