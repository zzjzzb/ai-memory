# 使用 ai-memory

简体中文使用说明。English: [USAGE.md](USAGE.md) · 架构：[ARCHITECTURE.zh-CN.md](ARCHITECTURE.zh-CN.md)

`ai-memory` 是一个**本地** Rust 库，用来给个人 AI 存记忆：短暂的 working 笔记、按天/事件的 episodic 日志、长期的 profile 事实。所有项目共用同一套 **Rust + SQLite** 内核，差异只在 [`MemoryPolicy`](#memorypolicy)，而不是换存储引擎。

它放在 agent 编排层**下面**（pi、Claude-like、Codex-like、DeepSeek-like）：模型循环仍由你负责，本库只存和召回记忆。它**不是**云服务、同步产品、多语言 SDK，也不是完整 LLM harness。

## 能解决什么问题

| 场景 | 做法 |
|------|------|
| 聊天助手要记住偏好 | `remember` 写入 profile，回答前 `recall` |
| 日记与聊天隔离 | 使用另一个 `project` id，召回不会串项目 |
| 草稿过期即可 | working 层 TTL；过期且未 pin 的记录不会出现在召回里 |
| 重要内容必须留下 | `pin` |
| 把会话笔记升成更长期记忆 | 按该项目的 promote 规则 `consolidate` |
| 调节「时间 / 关键词 / 向量」权重 | 每个项目自己的 recall 权重 |

## 功能一览

- 打开/创建本地 SQLite 文件（或内存库）
- 项目 CRUD：创建、读取、列表、更新策略、删除（级联记忆）
- `remember`（文本 + 可选 metadata + 可选层级；省略则用启发式）
- 按 id `get`（限定项目）、带层级/时间/pin/过期过滤的 `list`
- `pin` / `unpin` / `forget`
- 混合 `recall`：时间窗 + 关键词重叠 + 向量余弦，带分项得分
- TTL/保留策略对 **recall 和默认 list** 立即生效，不只在 `consolidate` 时
- `consolidate`：过期未 pin 记录，working→episodic→profile
- 打开时注入 [`Embedder`](#embedder) 与 [`VectorIndex`](#向量)
- 可选 Cargo feature `sqlite-vec`（默认关闭）
- [`AgentSession`](#编排适配层) + JSON 工具描述，接到通用 tool-calling 循环
- `remember_many` 单事务批量写入

非目标：同步、服务端、多租户云、FFI 绑定、Lance 后端、默认走网络 Embedding API。

## 快速开始

```toml
[dependencies]
ai-memory = { git = "https://github.com/zzjzzb/ai-memory" }
```

```rust
use std::time::Duration;
use ai_memory::{open, MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, Tier};

fn main() -> ai_memory::Result<()> {
    let store = open("./memory.db")?;

    let mut policy = MemoryPolicy::default();
    policy.recall.vector = 0.5;
    policy.promote.working_to_episodic_after = Duration::from_secs(60 * 60);
    store.create_project("my-app", policy)?;

    let app = store.project("my-app")?;
    app.remember(
        RememberRequest::new("用户偏好深色模式").with_tier(Tier::Profile),
    )?;

    for hit in app.recall(RecallQuery::new("主题偏好"))? {
        println!("{:.3} [{}] {}", hit.score, hit.memory.tier, hit.memory.text);
    }

    app.consolidate()?;
    Ok(())
}
```

示例：

```bash
cargo run --example two_projects
cargo run --example assistant_sim
cargo run --example harness_loop_sim
```

## 编排适配层

给已经有工具循环的中小团队 agent（pi、Claude-like、Codex-like、DeepSeek-like）。完整图见 [ARCHITECTURE.zh-CN.md](ARCHITECTURE.zh-CN.md)。

```rust
use ai_memory::{memory_tool_specs, MemoryPolicy, SqliteStore, TOOL_RECALL};
use serde_json::json;

let store = SqliteStore::open("./memory.db")?;
store.create_project("support-bot", MemoryPolicy::chat())?;
let session = store.session("support-bot")?; // 或 AgentSession::attach / ::sqlite

let tools = memory_tool_specs();
let _openai = tools.iter().map(|t| t.openai_tool());
let _anthropic = tools.iter().map(|t| t.anthropic_tool());

let hits = session.prefetch("用户问题")?;
let system = session.pack_context(&hits).render();

let result = session.call_tool(TOOL_RECALL, json!({"text": "主题", "limit": 5}));
session.end_turn_consolidate()?;
```

工具名：`memory_remember`、`memory_recall`、`memory_forget`、`memory_pin`、`memory_consolidate`。`call_tool` 不 panic，失败时 `ok: false`。批量写入用 `remember_many` / `session.remember_turn`。`open()` 已自动套上 WAL 等 SQLite 默认；session 继承同一套。本进程是**单写者**（`Mutex<Connection>`）。

## API 导览

### 打开存储

```rust
use std::sync::Arc;
use ai_memory::{open, open_in_memory, open_in_memory_with_embedder, HashEmbedder, SqliteStore};

let file = open("./memory.db")?;
let mem = open_in_memory()?;
let custom = open_in_memory_with_embedder(Arc::new(HashEmbedder::new(64)))?;
let built = SqliteStore::builder()
    .path("./memory.db")
    .embedder(Arc::new(HashEmbedder::new(32)))
    .build()?;
```

默认嵌入器是离线确定性的 `HashEmbedder`。生产环境请自行实现 `Embedder`。`open()` 会自动应用 SQLite PRAGMA，见 [透明性能](#透明性能)。

### 项目

每条记忆只属于一个 **project**。API 都要带 `project_id`，或使用 `store.project("id")`。

```rust
store.create_project("chat", MemoryPolicy::chat())?;
store.create_project("journal", MemoryPolicy::journal())?;
let chat = store.project("chat")?;

store.set_policy("chat", MemoryPolicy::default())?; // 更新策略
let _ = store.get_project("chat")?;
let _ = store.list_projects()?;
store.delete_project("journal")?; // 级联删除记忆与向量
```

### 写入、读取、列表、固定、忘记

```rust
use ai_memory::{MemoryListFilter, RememberRequest, Tier};

let m = chat.remember(
    RememberRequest::new("草稿：试试新侧边栏")
        .with_tier(Tier::Working)
        .with_metadata(serde_json::json!({"role": "user"})),
)?;

let _ = chat.get(&m.id)?;
let profile_only = chat.list_memories_filtered(
    MemoryListFilter::new().with_tiers(vec![Tier::Profile]),
)?;

chat.pin(&m.id)?;
chat.unpin(&m.id)?;
chat.forget(&m.id)?;
```

`list_memories()` 会隐藏已过期且未 pin 的行（与 recall 相同）。需要看原始行时用 `.including_expired()`。`get` 仍可按 id 取回，以便过期后再 `pin`。

### 召回（混合）

始终限定在一个项目内。

```
score = w_time * 新近度 + w_keyword * 词重叠 + w_vector * 余弦⁺
```

权重来自该项目的 `MemoryPolicy.recall`（会做归一化）。`RecallQuery` 可设 `since` / `until` / `tiers` / `min_score` / `limit`。

命中带 `score` 以及时间/关键词/向量分。被召回的行会增加 `access_count`（供 episodic→profile 使用）。

### 整理（consolidate）

```rust
let report = chat.consolidate()?;
// report.expired, promoted_to_episodic, promoted_to_profile
```

顺序：先按 TTL 删除未 pin 行，再按年龄 working→episodic，再按年龄 **且** `access_count` 做 episodic→profile。每一层的保留时间应 **长于** 对应的晋升延迟，否则会先过期升不上去。

## MemoryPolicy

按项目配置。默认值：

| 项 | 默认 |
|------|------|
| working TTL | 24 小时 |
| episodic TTL | 30 天 |
| profile TTL | 永久 |
| working→episodic | 1 小时 |
| episodic→profile | 7 天且访问 ≥ 2 |
| pin 跳过过期 | true |
| 召回权重 | 时间 0.30、关键词 0.30、向量 0.40 |
| 新近度半衰期 | 7 天 |
| candidate_prune | 256（`0` = 对所有存活行打分） |
| scan_limit | 剪枝前最多 2048 行（`0` = 不限制） |

预设：`MemoryPolicy::chat()`（偏向量、working 更短），`MemoryPolicy::journal()`（偏关键词、晋升更快）。

**TTL 与召回：** 过期且未 pin 的记忆会立刻从 `recall` 中消失，不必先 `consolidate`（consolidate 仍会真正删除它们）。额外上限：`candidate_prune` 256、`scan_limit` 2048（`0` 表示不限制）。

## 透明性能

`open()` / `open_in_memory()` / `store.session("id")` 就该够快，不必再调一堆旋钮。

**免费得到**

- 打开文件库：WAL、`synchronous=NORMAL`、`foreign_keys=ON`、`temp_store=MEMORY`、约 16 MiB `cache_size`、5 秒 `busy_timeout`、预编译语句缓存
- 进程内 embed LRU：相同文本 + embedding 维度不会重复计算（本进程内可跨项目共享向量字节；每次 remember 仍插入自己的行）
- 召回：TTL 过滤、pin+新近 `scan_limit`、向量前按关键词/新近度剪枝、默认 `limit` 8
- 单条 `remember` 与 `remember_many` 共用同一套单条事务辅助

查看：`store.applied_pragmas()`。

**你仍须自己调用**

- 一轮里写多条时用 `remember_many` / `session.remember_turn`
- **`consolidate`** — 不会在后台跑。TTL 只是隐藏过期未 pin 行；consolidate 负责删除并晋升

```bash
cargo bench   # 不在 cargo test 里；见 benches/memory_hot_path.rs
```

完整对照表：[ARCHITECTURE.zh-CN.md](ARCHITECTURE.zh-CN.md#5-透明性能你免费得到什么)。

## 项目隔离

召回 SQL 始终带 `WHERE project_id = ?`。用错误的项目去 pin/forget/get 不会改到另一个项目。

## Embedder

```rust
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

通过 `open_with_embedder` / `open_in_memory_with_embedder` / `SqliteStore::builder().embedder(...)` 注入。可用 `store.embedder()` 查看。

## 向量

默认：SQLite blob + 进程内 `BruteForceCosine`。

可选：

```toml
ai-memory = { git = "https://github.com/zzjzzb/ai-memory", features = ["sqlite-vec"] }
```

```rust
use std::sync::Arc;
use ai_memory::{SqliteStore, SqliteVecIndex};

let store = SqliteStore::builder()
    .in_memory()
    .vector_index(Arc::new(SqliteVecIndex::new()?))
    .build()?;
```

默认 `cargo test` 走暴力余弦。`cargo test --features sqlite-vec` 才会跑扩展。环境编不过 C 扩展时关掉 feature 即可，默认召回仍然可用。

未来 Lance 应实现 `MemoryStore`。不要自研存储引擎。

## 层级

| 层级 | 用途 |
|------|------|
| `working` | 会话 / 短暂 |
| `episodic` | 日/事件日志 |
| `profile` | 长期事实 |

省略层级时用简单启发式（如 `I prefer` → profile，`Today I` → episodic，否则 working）。
