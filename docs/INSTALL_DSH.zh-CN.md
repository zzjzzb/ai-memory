# 在 DeepSeek Harness 里安装 ai-memory（5 分钟）

中文。[English](INSTALL_DSH.md)

一条命令把本仓库 **根目录** 加成 dsh 插件。长对话把回合存进 **Rust + SQLite**，再注入一段 **有 token 预算** 的切片（`## Memory (project: …)`），而不是整段 transcript。

这是套在 `ai-memory` crate 上的 Cordis Host —— **不是**用 TypeScript 重写记忆，**也不是**自动让 LLM「抽事实」的插件。

---

## 5 分钟路径（只走成功路径）

按顺序做。全程使用 **同一个** profile 名（下面用 `web`）。若第 3 步因 *Ignored build scripts* 失败，这是第一次的常见情况 —— 做第 4 步，再重跑第 3 步。

### 1. 工具

```bash
node -v          # v20 或更新
pnpm -v          # dsh plugin add 会转发给 pnpm
dsh --help       # DeepSeek Harness CLI
cargo --version  # rustc 1.74+（prepare 会编 Rust）
```

没有 `dsh`？先装 [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)。没有 `cargo`？用 [rustup.rs](https://rustup.rs/)。

### 2. 钉死 commit SHA

```bash
git ls-remote https://github.com/zzjzzb/ai-memory.git refs/heads/main
```

**应看到**（SHA 会变；复制 **左列** 40 位十六进制）：

```text
f3ce0b5c1a2b3c4d5e6f7890aabbccddeeff0011	refs/heads/main
```

把这个值叫 `COMMIT`，贴进下一条命令。不要在你在意的机器上跟踪浮动的 `main`。

### 3. 添加插件

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#COMMIT
```

把 `COMMIT` 换成第 2 步的 SHA（不要写成两个 `#`）。

**成功时应看到**（第一次 cargo 可能要几分钟；日志会和 pnpm 交错）：

```text
[dsh-ai-memory] building ai-memory CLI (Rust source of truth)…
[dsh-ai-memory] wrote bin/ai-memory
[dsh-ai-memory] building napi addon…
[dsh-ai-memory] wrote ai-memory.node (from libai_memory_node.so)
[dsh-ai-memory] prepare: using napi (CLI also built)
```

macOS 可能是 `libai_memory_node.dylib`，Windows 是 `ai_memory_node.dll`。都算成功。

**napi 失败但仍算成功：** 插件编失败、CLI 写成功时：

```text
[dsh-ai-memory] napi crate build failed — plugin will use the CLI fallback if present
[dsh-ai-memory] prepare: using CLI fallback (napi addon not built). Runtime still uses the Rust crate, not a JS store.
```

**第一次常见失败（pnpm ≥10 拦住 `prepare`）：**

```text
[ERR_PNPM_IGNORED_BUILDS] Ignored build scripts: dsh-ai-memory

Run "pnpm approve-builds" to pick which dependencies should be allowed to run scripts.
```

dsh 也可能提示：把该包名键写进 profile 的 `pnpm-workspace.yaml`。去做第 4 步，然后 **原样重跑** 本步的 add。

<a id="allow-prepare"></a>

### 4. 允许 `prepare`（仅当第 3 步打印了 Ignored build scripts）

profile 文件（没有就新建）：

```text
~/.dsh/profiles/web/pnpm-workspace.yaml
```

若设置了 `DSH_HOME`，则用 `$DSH_HOME/profiles/web/pnpm-workspace.yaml`。

**文件还不存在时，整份可贴：**

```yaml
packages:
  - '.'
allowBuilds:
  dsh-ai-memory: true
```

若文件 **已存在**，把 `allowBuilds` 加在 **顶层**（和 `packages` 并列，不要嵌进 `packages`）。保留其它键。包名键必须是 pnpm 打印的 `dsh-ai-memory`，不是 GitHub URL。

然后重跑第 3 步的 add（同一个 `COMMIT`）。

把 `allowBuilds` 理解成：允许这个包在安装时在你的机器上执行代码，而且不在 agent 沙箱里。官方规则：[打包与安装插件](https://deepseek-harness.github.io/deepseek-harness/develop/basic/publish)。

### 5. 确认层已激活

`--profile` 必须和 `plugin add` **相同**：

```bash
dsh --profile web --dump-config
```

**应看到**（搜这个标题；周围 YAML 可能略有不同）：

```text
# == dsh-ai-memory
```

还应看到类似：

```yaml
- id: dsh-ai-memory
  name: dsh-ai-memory
  config:
    projectId: dsh
    tokenBudget: 8192
    policy: chat
    prefetchEnabled: true
```

若没有 `# == dsh-ai-memory`，说明包装上了但只是普通依赖，**没有**激活层 —— 见 [故障排除](#troubleshooting)。

重启以使层生效：`dsh --profile web` 或 `dsh web`。

### 6. 第一次使用：记住一条事实 + 预算 pack

用 `--profile web` 启动 dsh。在对话里粘贴：

```text
Call tool memory_remember with text "User prefers dark mode" and tier "profile".
Then call tool memory_recall with text "dark mode".
```

代理应调用 **这些工具名**：

| 步骤 | 工具 | 参数 |
|------|------|------|
| 1 | `memory_remember` | `{ "text": "User prefers dark mode", "tier": "profile" }` |
| 2 | `memory_recall` | `{ "text": "dark mode" }` |

其它 crate 工具（以后用）：`memory_forget`、`memory_pin`、`memory_consolidate`、`memory_compact`。

**下一轮** 模型调用时，系统提示应含 Cordis 段 `ai-memory:pack`，形如：

```text
## Memory (project: dsh, 1 hits)
- [profile id=mem-… score=…] User prefers dark mode
```

这是 `prefetch_within_budget`，不是整段聊天。若看到完整 transcript，那是别处在倒历史 —— 不是本插件。

没有网页？做 [旗舰无头模拟](#flagship-sim)。

---

## 你装上了什么（30 秒）

一条很长的支持会话（侧栏 bug，然后重复发票）若倒进模型，会撑爆约 100 万（或更小）的窗口。本插件会：

1. 用 `memory_remember` / `memory_pin` 落盘。
2. 下一轮模型调用前注入 Rust `prefetch_within_budget` 的 `ai-memory:pack`。
3. 按 `projectId` 隔离。同一个 `.db` 上两个项目，召回不会串。

根目录 `package.json` 的 npm 名是 **`dsh-ai-memory`**。`Cargo.toml` 的 crate 名是 **`ai-memory`**。同一个仓库。

---

## 前置条件（第 1 步检查失败时）

| 需要 | 为什么 | 怎么确认 |
|------|--------|----------|
| **Node.js 20+** | 插件 `engines`；Node 不对常常加载不了 `.node` | `node -v` |
| **pnpm** | `dsh plugin add` = 在 profile 目录里跑 pnpm | `pnpm -v` |
| **dsh CLI** | 写 profile 和 bundle 列表 | `dsh --help` |
| **Git** | `github:zzjzzb/ai-memory` 是 git 拉取 | `git --version` |
| **Rust `cargo`** | `prepare` 编 napi 和/或 CLI | `cargo --version`（MSRV **1.74+**） |
| **网络** | 公开 GitHub | [github.com/zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory) |

若 CLI 不在 PATH，可用 `npx @deepseek-ai/dsh --help`。`dsh plugin add` 仍然需要 `pnpm`。

**profile** 一般在 `~/.dsh/profiles/<name>/`（或 `$DSH_HOME/profiles/<name>/`）。`web` 是常用 UI profile；第一次 `dsh plugin --profile web …` 会创建它。

不拉 GitHub、用本地 clone：

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
dsh plugin --profile web add .
dsh --profile web --dump-config
```

**不要**把 `github:zzjzzb/ai-memory#path:integrations/dsh-ai-memory` 当作用户路径 —— 那种 git 拉取带不上 `prepare` 必须编译的 Rust crate。

---

## 配置旋钮（可复制）

后写的层覆盖先写的。补丁会 **整份替换 `config` 对象**（不按 key 深合并）。改 **profile** 的 `cordis.patch.yml`，不要改本仓库。

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

| 字段 | 默认 | 含义 |
|------|------|------|
| `dbPath` | `~/.local/share/ai-memory/dsh.db` | SQLite 文件。空则用 `AI_MEMORY_DB` 或上述默认。`:memory:` 只给测试。 |
| `projectId` | `dsh` | 隔离键。支持 vs 人事要用不同 id。 |
| `tokenBudget` | `8192` | `prefetch_within_budget` 上限（`ceil(字符/4)`）。`256` 对齐旗舰模拟。 |
| `policy` | `chat` | `chat` / `journal` / `default` —— **仅在创建项目时** 使用。 |
| `prefetchEnabled` | `true` | 注册 `ai-memory:pack`。 |
| `sectionOrder` | `40` | 提示段顺序（persona 一般是 0）。 |
| `cliPath` | （自动） | napi 的 `.node` 没加载时强制走 `ai-memory` CLI。 |

**不要**在组合包层之外再插一条 `id: dsh-ai-memory` —— 重复 `id` 可能让启动崩溃。要覆盖配置，只用上面这一行。

---

## 和扎堆的 Memory 插件有何不同

| 常见 Memory 插件 | 本插件 |
|------------------|--------|
| 再调一次 LLM 自动抽事实 | 工具 + **显式** compact / consolidate |
| 用 JS/Python 再实现一套记忆 | **Rust crate** 才是真相源 |
| 「支持 100 万 token prompt」 | 长 session 存在盘上；每次只 **打包切片** |
| 全局一个事实袋子 | `projectId` 隔离 |

---

## 绑定（napi vs CLI）

1. **napi-rs** —— 首选。同进程 `HostSession.dispatch`。
2. **`ai-memory` CLI** —— 同一套 JSON 信封，子进程；当 `ai-memory.node` 缺失或加载失败时用。

`prepare` 会两样都试，**有一样成功就算成功**。不会在 JavaScript 里重写召回。

---

<a id="flagship-sim"></a>

## 旗舰场景（无头）

工单 **T-1042**（侧栏）、**T-1088**（重复发票），pin Ada Chen，项目 `sme-hr` 不得泄漏 T-1042。

```bash
cargo build --bin ai-memory
node scenarios/dsh-support-agent/sim/run.mjs
npm test --prefix scenarios/dsh-support-agent
cargo test --test dsh_support_scenario
```

**应看到：** `tokens <= tokenBudget`；`## Memory (project: sme-support, …)`；紧 pack 仍含 `PINNED-BILLING-OWNER-ADA`；`sme-hr` 的 pack **没有** T-1042。细节：[scenarios/dsh-support-agent/README.zh-CN.md](../scenarios/dsh-support-agent/README.zh-CN.md)。

映射到真 profile：同样 `github:zzjzzb/ai-memory#COMMIT`，在 profile 补丁里设 `projectId: sme-support`，把 [`seed/tickets.json`](../scenarios/dsh-support-agent/seed/tickets.json) 当用户句。

---

<a id="troubleshooting"></a>

## 故障排除

| 现象 | 常见原因 | 怎么办 |
|------|----------|--------|
| `Ignored build scripts: dsh-ai-memory` / `ERR_PNPM_IGNORED_BUILDS` | pnpm ≥10 拦住了 `prepare` | 把 [第 4 步](#allow-prepare) 贴进 **该 profile** 的 `pnpm-workspace.yaml`，再跑 **同一条** `add` |
| 写了 `allowBuilds` 但 add 仍忽略脚本 | 改错文件 / 嵌进了 `packages` / 键名不对 | 必须是 **该 profile** 的 `pnpm-workspace.yaml`。顶层 `dsh-ai-memory: true`。不要写进 `ignoredBuiltDependencies` |
| prepare 时 `cargo: command not found` / 没有 `rustc` | 没有 Rust 工具链 | [rustup](https://rustup.rs/) 后重新 add。只跑 JS 测试：`DSH_AI_MEMORY_SKIP_NATIVE=1` |
| napi 编译失败，随后 `prepare: using CLI fallback` | 插件失败；CLI 编出来了 | **可以。** 插件走 CLI。确认 `--dump-config` 仍有 `# == dsh-ai-memory` |
| prepare：napi 和 CLI 都没有 | 两次 Rust 构建都失败 | `rustc` **1.74+**（`rustup update`）。重新 add。检查磁盘空间 |
| 加载插件时 `SyntaxError` / `Unexpected token` | **Node 不对** | `node -v` 必须 **≥ 20**。在这个 Node 上重新 `prepare` 编 `.node` |
| 有 `.node` 但 `invalid ELF` / `wrong architecture` | 插件是在别的 OS/CPU 上编的 | 在本机重新编。或走 CLI 回退（`bin/ai-memory`）/ 设 `cliPath` |
| `ai-memory CLI failed to start` | `.node` 没有 **而且** CLI 没有或不可执行 | `allowBuilds` 后重新 add，或在 clone 里 `npm run prepare`。在已安装包下找 `ai-memory.node` 和 `bin/ai-memory` |
| `dsh: command not found` | CLI 不在 PATH | 用已经能跑 `dsh web` 的那个环境 |
| Git 401 / `Repository not found` | 写错或私有 fork 没登录 | 公开 spec 是 `github:zzjzzb/ai-memory`。只有私有 fork 才需要 `gh auth login` |
| `pnpm: command not found` | dsh plugin add 需要 pnpm | [pnpm.io/installation](https://pnpm.io/installation) |
| `--dump-config` **没有** `# == dsh-ai-memory` | 层没激活 | 加的是 **根** `github:zzjzzb/ai-memory`，不是子目录 URL？`dsh plugin --profile web list`。写完 `allowBuilds` 再 add。确认根 `package.json` 有 `dsh.bundle.patch` |
| 加了插件但 dump-config 是空的 / 旧的 | **profile 名不一致** | `dsh plugin --profile web add` 之后必须是 **`dsh --profile web --dump-config`**。光写 `dsh --dump-config` 是另一个 profile。注意 `web` / `Web` / `default` |
| 有层标题但没有工具 | 没重启 profile；inject 失败 | 重启 `dsh --profile web`。启动日志：无法解析 `dsh-ai-memory` |
| 重复 loader `id: dsh-ai-memory` | 组合包层 **和** 手动 insert 了同一个 id | 只走一条路：要么 `dsh plugin add`，要么手动补丁行 |
| 记忆是空的 / 工单不对 | `projectId` 或 `dbPath` 不对 | 默认库 `~/.local/share/ai-memory/dsh.db`。按项目隔离 |
| pack 看起来像整段聊天 | 别处在倒 prompt，或你没看 `ai-memory:pack` | 搜 `## Memory (project:`。不要自己把 transcript 贴进去 |

---

## 下一步：dsh.pub（可选）

**不必**上目录也能用插件。Topic **`dsh-plugin`** 已经打在仓库上。

若要目录条目，请自行提交仓库 URL：

- [https://dsh.pub/zh/submit/](https://dsh.pub/zh/submit/)
- [https://dsh.pub/en/submit/](https://dsh.pub/en/submit/)

本指南不会替你提交。

---

## 相关文档

- 背书 / 架构：[INTEGRATION_DSH.zh-CN.md](INTEGRATION_DSH.zh-CN.md)
- 插件内部：[integrations/dsh-ai-memory/README.md](../integrations/dsh-ai-memory/README.md)
- Crate 用法：[USAGE.zh-CN.md](USAGE.zh-CN.md)
