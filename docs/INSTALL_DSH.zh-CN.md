# 在 DeepSeek Harness 里安装 ai-memory（5 分钟）

中文。[English](INSTALL_DSH.md)

**目标：** 一条命令把本仓库加成真正的 dsh 插件。之后，很长的支持会话会把回合存进 **Rust + SQLite**，再注入一段 **有 token 预算** 的切片（`## Memory (project: …)`），而不是整段 transcript。

这是套在 `ai-memory` crate 上面的 Cordis **Host**。**不是**用 TypeScript 重写记忆，**也不是**自动让 LLM「抽事实」的 Memory 插件。

**浏览一遍 → 复制命令 → 在 `--dump-config` 里看到 `# == dsh-ai-memory` 即成功。**

---

## 0. 你在装什么（30 秒）

想象一条 DeepSeek Harness 会话：白天连续处理相关工单（侧栏挡住发票表，然后重复扣款，然后跟进）。如果把聊天全文塞进模型，会撑爆约 100 万（或更小）的窗口。

装上本插件后，代理会：

1. 边干边调用 `memory_remember` / `memory_pin`。
2. 下一轮模型调用前，dsh 把系统提示里的 `ai-memory:pack` 段换成 Rust 的 `prefetch_within_budget` 结果。
3. 隔离键是 `projectId`。同一个 `.db` 上两个项目，召回不会串。

旗舰走查（不需要网页）：[scenarios/dsh-support-agent/](../scenarios/dsh-support-agent/README.zh-CN.md)。

---

## 1. 前置条件

机器要能跑 **dsh**，并且能把本仓库编译一次（git 安装会跑 `prepare`，里面编 Rust）。

| 需要 | 为什么 | 怎么确认 |
|------|--------|----------|
| **Node.js 20+** | 插件 `engines`；dsh 本身也常用较新的 Node | `node -v` → `v20` 或更新 |
| **pnpm** | `dsh plugin add` 在 profile 目录里转发给 pnpm | `pnpm -v` |
| **dsh CLI** | 把组合包装进 **profile** | `dsh --help` |
| **Git** | `github:zzjzzb/ai-memory` 是 git 拉取 | `git --version` |
| **Rust `cargo`** | `prepare` 编 napi 插件 + `ai-memory` CLI | `cargo --version`（crate MSRV **1.74+**） |
| **网络** | 拉公开 GitHub 仓库 | 浏览器打开 [github.com/zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory) |

本仓库是 **双用途**：`Cargo.toml` 是 Rust crate（`ai-memory`）；根目录 `package.json` 才是 **dsh 组合包**（`dsh-ai-memory`）。不是两套产品。

### 1.1 若 `dsh --help` 失败：先装 dsh

以官方项目为准：[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)。常见做法：

```bash
# 示例：用 npx 跑已发布的 CLI（同时会起 web；该环境里要有 dsh）
npx @deepseek-ai/dsh --help

# 或从 harness 仓库 / 你们现有的 dsh 安装里把 `dsh` 放进 PATH。
# 还需要 PATH 上有 `pnpm` —— 加插件底层就是 pnpm。
```

如果团队已经在跑 `dsh web` 或 `dsh --profile web`，这一步已经完成。

官方组合包约定（为什么需要 `package.json` + `cordis.patch.yml`，以及 `allowBuilds` 规则）：[打包与安装插件](https://deepseek-harness.github.io/deepseek-harness/develop/basic/publish) · [English](https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/publish)。

### 1.2 创建或沿用一个 profile

**profile** 是一套可启动的组合（插件 + 配置），一般在：

```text
~/.dsh/profiles/<name>/          # Linux / macOS 默认
$DSH_HOME/profiles/<name>/       # 若设置了 DSH_HOME
```

浏览器 UI 多用 **web** 这个 profile。第一次执行 `dsh plugin --profile web …` 时，若还没有就会 **自动创建**。

```bash
# 沿用 web UI profile（推荐）
dsh plugin --profile web list

# 或单独一个 profile（第一次 plugin add 时创建）
dsh plugin --profile support-ops list
```

你会看到 profile 目录出现（或本来就有）。记住 **名字**（`web`、`support-ops` …）。下面每条命令都用 `--profile <这个名字>`。

---

## 2. 从 GitHub 一条命令安装

**请钉死 commit。** 之后 `main` 再 push，不应悄悄改你机器上编译出来的东西。从 [Commits](https://github.com/zzjzzb/ai-memory/commits/main) 复制完整 SHA，或：

```bash
git ls-remote https://github.com/zzjzzb/ai-memory.git refs/heads/main
```

然后安装（把 `web` 和 `<commit>` 换成你的）：

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
```

跟踪 `main`（生产环境不推荐）：

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory
```

这个 spec 装的是 **仓库根目录**。根上的 `package.json` 声明了 `"dsh": { "bundle": { "patch": "./cordis.patch.yml" } }`。dsh 会把 `dsh-ai-memory` 追加进该 profile 的 `dsh.profile.bundles`。你 **不必** 再加 `github:…#path:integrations/…`。

### 2.1 `allowBuilds` / `prepare`（第一次多半会碰到）

git 安装拉到的是 **源码**，不是预编译的 `.node`。不会跑包里的 `build` 脚本。所以本仓库提供 **`prepare`**：在同一次 checkout 里用 Rust crate 编译 napi 插件和 `ai-memory` CLI。

**pnpm ≥10 在你明确允许之前会拒绝跑这个 `prepare`。** 第一次 `add` 常常 **失败**。dsh / pnpm 会打印一个包名键。把 **打印出来的那个键** 原样写进该 profile 的 `pnpm-workspace.yaml`。对本插件来说，键就是 `dsh-ai-memory`：

```bash
# profile 文件（没有就新建）。默认路径：
#   ~/.dsh/profiles/web/pnpm-workspace.yaml
```

```yaml
allowBuilds:
  dsh-ai-memory: true
```

若文件里已有别的键（`packages:` 等），把 `allowBuilds` 放在 **顶层并列**，不要塞进 `packages` 里面。

然后 **用同一条 add 再跑一遍**：

```bash
dsh plugin --profile web add github:zzjzzb/ai-memory#<commit>
```

这时应看到 cargo 编译（第一次可能要几分钟）。把 `allowBuilds` 理解成：允许这个包在 **安装时** 在你的机器上执行代码，而且 **不在** agent 沙箱里。只对你信任的源码授权。官方表述：[从 GitHub 安装：构建脚本这一关](https://deepseek-harness.github.io/deepseek-harness/develop/basic/publish)。

没有编译器？先到 [rustup.rs](https://rustup.rs/) 安装 Rust，再重试 `add`。

### 2.2 本地 clone（同一套组合包，不拉 GitHub）

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
dsh plugin --profile web add .
dsh --profile web --dump-config    # 找 "# == dsh-ai-memory"
```

开发者仍可从 clone 里 `dsh plugin add ./integrations/dsh-ai-memory`；**用户**请用上面的根 spec，这样 `github:zzjzzb/ai-memory` 才和 dsh.pub 一致。

---

## 3. 验证（你应当看到这些）

### 3.1 组合包层（必做）

```bash
dsh --profile web --dump-config
```

在输出里搜索类似：

```text
# == dsh-ai-memory
```

还应看到插入的那一行（`id: dsh-ai-memory`，`name: dsh-ai-memory`），默认 `projectId: dsh`、`tokenBudget: 8192`。

若没有这个标题，说明包装上了但只是 **普通依赖**，**没有**激活层。见 [故障排除](#8-故障排除)。

`add` 成功后请重启该 profile，新层才会组合进去（`dsh --profile web` / `dsh web`）。

### 3.2 可选：工具列表 + 第一次 remember / recall

1. 用同一个 profile 启动 dsh（`dsh --profile web` 或 `dsh web`）。
2. 会话里应出现与 crate 同名的工具：`memory_remember`、`memory_recall`、`memory_forget`、`memory_pin`、`memory_consolidate`、`memory_compact`。
3. 让模型（或你直接调工具）把 `User prefers dark mode` 记成 profile。
4. 下一句用户话时看 **系统提示**（或调试 dump）。你要的是一小段：

```text
## Memory (project: dsh, N hits)
…
```

而不是整段聊天。那段文字来自 Rust 的 `ContextPack.render()`，以 Cordis 段 `ai-memory:pack` 注入。

没有 dsh 界面？做 [第 7 节](#7-旗舰场景无头模拟--如何对应到真机-dsh) 的无头模拟即可。

---

## 4. 配置旋钮（可复制）

组合包层给出默认值。**后写的层覆盖先写的**，而且补丁会 **整份替换 `config` 对象**（不会按 key 深合并）。请改 **profile** 里的 `cordis.patch.yml`（和该 profile 的 `package.json` 同一目录），不要改本仓库。

```yaml
# ~/.dsh/profiles/web/cordis.patch.yml  （示例）
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
        # cliPath: /usr/local/bin/ai-memory   # 仅当 napi 失败且你已有 CLI
```

| 字段 | 默认 | 含义 |
|------|------|------|
| `dbPath` | `~/.local/share/ai-memory/dsh.db` | SQLite 文件。`open()` 会套 WAL 等默认。空则用环境变量 `AI_MEMORY_DB` 或上述默认。`:memory:` 只给测试用。 |
| `projectId` | `dsh` | 隔离键（`WHERE project_id = ?`）。支持 vs 人事应使用不同 id。 |
| `tokenBudget` | `8192` | `prefetch_within_budget` 上限（默认 `ceil(字符数/4)`）。可用 `256` 对齐旗舰模拟；真实对话用 2k–32k。 |
| `policy` | `chat` | `chat` / `journal` / `default` —— **仅在创建项目时** 使用。 |
| `prefetchEnabled` | `true` | 注册系统提示段 `ai-memory:pack`。 |
| `sectionOrder` | `40` | 提示段顺序（persona 一般是 0）。 |
| `cliPath` | （自动） | napi 的 `.node` 没加载成功时，覆盖 `ai-memory` CLI 路径。 |

如果只想靠组合包层生效，**不要**再手动插入同一条 `id: dsh-ai-memory` —— 重复 `id` 可能让启动崩溃。要覆盖配置，用上面这种按 `id` 写的 **一行** 即可。

---

## 5. 和扎堆的 Memory 插件有何不同

| 常见 Memory 插件 | 本插件 |
|------------------|--------|
| 再调一次 LLM 自动抽事实 | 工具 + **显式** compact / consolidate |
| 用 JS/Python 再实现一套记忆 | **Rust crate** 才是真相源 |
| 「支持 100 万 token prompt」 | 长 session 存在盘上；每次只 **打包切片** |
| 全局一个事实袋子 | `projectId` 隔离 |
| 藏起来的向量库魔法 | `open()` 上的透明 SQLite 默认 |

dsh 以后也许会自带抽取。本集成 **不依赖** 那套。

---

## 6. 绑定（napi vs CLI）

1. **napi-rs**（`ai-memory-node`）——首选。同进程 `HostSession.dispatch`。
2. **`ai-memory` CLI** ——同一套 JSON 信封，子进程。`.node` 缺失或加载失败时用。

`prepare` 会尽量把 **两样都编出来**。不要在 JavaScript 里重写召回。

---

## 7. 旗舰场景（无头）以及如何对应到真机 dsh

和真支持代理同一套故事：工单 **T-1042**（侧栏重叠）、**T-1088**（重复发票），pin 账单负责人 **Ada Chen**，项目 `sme-hr` 不得泄漏 T-1042。

### 7.1 无头模拟（不需要 dsh CLI / 网页）

在本仓库 clone 里：

```bash
cargo build --bin ai-memory
# 可选：同进程插件
# cargo build -p ai-memory-node
# 或：npm run prepare

node scenarios/dsh-support-agent/sim/run.mjs
# 同一套检查：
npm test --prefix scenarios/dsh-support-agent
cargo test --test dsh_support_scenario
```

**应看到：** pack 的 `tokens <= tokenBudget`；标题 `## Memory (project: sme-support, …)`；紧预算下仍有 `PINNED-BILLING-OWNER-ADA`；`sme-hr` 的 pack **不能**出现 T-1042。细节：[scenarios/dsh-support-agent/README.zh-CN.md](../scenarios/dsh-support-agent/README.zh-CN.md)。

模拟器用假的 Cordis `ctx` 调用和 dsh 同一套 `apply(ctx)`。没有 Web UI 也足以证明插件。

### 7.2 把模拟映射到真 profile

```bash
dsh plugin --profile support-ops add github:zzjzzb/ai-memory#<commit>
# 若需要，按 §2.1 写 allowBuilds，再 add 一次
```

在该 profile 里贴 [配置示例](#4-配置旋钮可复制)，`projectId: sme-support`，`tokenBudget: 256`（真实对话可改 8192）。把 [`scenarios/dsh-support-agent/seed/tickets.json`](../scenarios/dsh-support-agent/seed/tickets.json) 里的用户句贴进会话。remember / pin 之后，系统提示应出现短的 `ai-memory:pack`，形状和模拟器打印的一样。

---

## 8. 故障排除

| 现象 | 常见原因 | 怎么办 |
|------|----------|--------|
| 第一次 `dsh plugin add github:…` 失败，提示 ignored build scripts / `Ignored build scripts` | pnpm ≥10 拦住了 `prepare` | 在 **profile** 的 `pnpm-workspace.yaml` 里加 `allowBuilds: { dsh-ai-memory: true }`，再跑 **同一条** `add` |
| `prepare` / cargo 报 `rustc` / `cargo` 找不到 | 没有 Rust 工具链 | 用 [rustup](https://rustup.rs/) 安装，`cargo --version` 后重新 `add` |
| 编译报 edition / MSRV | Node 没问题；**Rust 太旧** | 需要 rustc **1.74+**（见 crate 的 `rust-version`）。`rustup update` |
| `node: … unexpected token` / 插件加载失败 | Node 太旧 | `node -v` 必须 **≥ 20** |
| `dsh: command not found` | CLI 不在 PATH | 安装 DeepSeek Harness；用已经能跑 `dsh web` 的那个环境 |
| Git 拉取 401 / `Repository not found` | 没登录 GitHub，或写错地址 | 本仓库是 **公开** 的。核对 spec：`github:zzjzzb/ai-memory`。私有 fork 需要 `gh auth login` 或 git 凭据 |
| 找不到 `pnpm` | dsh plugin add 依赖 pnpm | 安装 [pnpm](https://pnpm.io/installation)，保证在 PATH 里 |
| `--dump-config` **没有** `# == dsh-ai-memory` | 包没有 `dsh.bundle.patch`、spec 不对、或 add 没调和 bundles | 确认加的是 **根** `github:zzjzzb/ai-memory`（不是随便一个子目录 URL）。`dsh plugin --profile web list`。再 add 一次 |
| 层名字在，但没有工具 | 没重启 profile；或 `inject` 失败 | 重启 `dsh --profile web`。看启动日志里的模块解析错误 |
| 重复 loader `id: dsh-ai-memory` | 组合包层 **和** 手动 insert 了同一个 id | 只走一条路：要么 `dsh plugin add`，要么手动补丁行，不要两套。删掉多余的 insert |
| 没有 addon；CLI 报 `failed to start` | `prepare` 被跳过或失败；napi 回退到 CLI 也没有 | 允许构建后重新 add，或在 clone 里 `npm run prepare`。确认已安装包下有 `ai-memory.node` 和/或 `bin/ai-memory` |
| napi 加载失败，CLI 可用 | `.node` 的 Node ABI / 平台不对 | CLI 回退是预期行为。在本机重新 `prepare`，或设置 `cliPath` |
| 记忆是空的 / 工单不对 | `projectId` 或 `dbPath` 不对 | 按项目隔离。检查 profile 配置。默认库路径是 `~/.local/share/ai-memory/dsh.db` |
| pack 看起来像整段聊天 | 没用本插件的段，或预算很大 **并且** 你还在别处倒历史 | 找 `ai-memory:pack` / `## Memory (project:`。不要自己把 transcript 贴进系统提示 |

---

## 9. 下一步：提交到 dsh.pub（可选）

**不必**上目录也能用插件。GitHub topic **`dsh-plugin`** 已经打在 [zzjzzb/ai-memory](https://github.com/zzjzzb/ai-memory) 上。

若要目录条目，提交 **仓库 URL**（组合包在 **根目录**，本安装路径已满足）：

- 中文：[https://dsh.pub/zh/submit/](https://dsh.pub/zh/submit/)
- English：[https://dsh.pub/en/submit/](https://dsh.pub/en/submit/)

dsh.pub 读取根目录包元数据，检查 patch / 入口 / README / 许可证，**不会**执行你的代码。本文不会替你提交。

---

## 相关文档

- 背书 / 架构：[INTEGRATION_DSH.zh-CN.md](INTEGRATION_DSH.zh-CN.md)
- 插件内部：[integrations/dsh-ai-memory/README.md](../integrations/dsh-ai-memory/README.md)
- Crate 用法：[USAGE.zh-CN.md](USAGE.zh-CN.md)
