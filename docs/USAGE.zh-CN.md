# 使用 ai-memory

简体中文。[English](USAGE.md) · 架构：[ARCHITECTURE.zh-CN.md](ARCHITECTURE.zh-CN.md) · DeepSeek Harness：[INTEGRATION_DSH.zh-CN.md](INTEGRATION_DSH.zh-CN.md)

**是什么：** 本地 Rust + SQLite 库，按 **project** 存 agent 记忆（working / episodic / profile）。模型循环仍由你的编排层负责（pi、Claude-like、Codex-like）。本库负责存、召回，以及打出一小段 **有 token 预算** 的 prompt。

**不是什么：** 不是 LLM 客户端，不是把 100 万 token 塞进模型，也不会在后台自动 `consolidate`。

## 最小例子

```toml
[dependencies]
ai-memory = { git = "https://github.com/zzjzzb/ai-memory" }
```

```rust
use ai_memory::{open, MemoryPolicy, MemoryStore, RememberRequest, Tier, TokenBudget};

fn main() -> ai_memory::Result<()> {
    let store = open("./memory.db")?;
    store.create_project("support-bot", MemoryPolicy::chat())?;
    let session = store.session("support-bot")?;

    session.remember_turn([
        RememberRequest::new("用户偏好深色模式").with_tier(Tier::Profile),
        RememberRequest::new("工单：发票页侧边栏重叠").with_tier(Tier::Working),
    ])?;

    let pack = session.prefetch_within_budget("侧边栏 发票", TokenBudget::new(8_192))?;
    let _system = pack.render();

    session.compact_working()?;
    session.end_turn_consolidate()?;
    Ok(())
}
```

```bash
cargo run --example harness_loop_sim
cargo run --example assistant_sim
cargo run --example two_projects
```

`open()` 已自动打开 WAL 等 SQLite 默认，不必再调一堆旋钮。

## 一个超长 session：很多相关工单，对话超过约 100 万 token

中小团队常在 **同一个 session** 里连续处理相关问题，直到聊天记录大于模型窗口（100 万，或更小）。

**下一轮模型调用塞不下全部历史。** 把历史存下来，每次只送一段有预算的切片。

| 要做 | 不要做 |
|------|--------|
| 每轮 `remember` / `remember_turn` 写入项目 | 把完整 transcript 塞进 prompt |
| 模型调用前 `prefetch_within_budget(query, TokenBudget { max_tokens: 2000..=32000 })` | 宣传本库能「支持 100 万 token 的 prompt」 |
| 必须留下的事实就 `pin` | 每个编排层自己截断 |
| working 太多时显式 `compact_working` | 默认走需要联网的 LLM 摘要（本库不做） |
| 需要过期/晋升时自己 `end_turn_consolidate` | 以为 consolidate 会在后台跑 |

**固定用法**

1. 把用户/助手笔记写入 working，长期事实写入 profile。
2. 每次调模型前：混合召回，再按 **token 预算** 装填（先 pin、再高分）。默认按 `ceil(字符数/4)` 估 token，不依赖 tiktoken。有真实分词器就实现 [`TokenEstimator`](../src/harness/tokens.rs)。
3. 可选压缩：保留 pin + 最新 working，其余折成 **一条抽取式 episodic**，再删掉那些 working。默认 [`ExtractiveCompactor`](../src/harness/compact.rs) 离线。以后的 `Compactor` 可以接 LLM，默认不能联网。
4. TTL 删除和层级晋升仍然要你自己调用 `consolidate`。

本库不会把 100 万 token 塞进模型。它把超长 session **存在磁盘上**，每轮只喂约 2k–32k。

## 编排循环

```rust
use ai_memory::{memory_tool_specs, MemoryPolicy, SqliteStore, TokenBudget, TOOL_RECALL};
use serde_json::json;

let store = SqliteStore::open("./memory.db")?;
store.create_project("support-bot", MemoryPolicy::chat())?;
let session = store.session("support-bot")?;

let _tools = memory_tool_specs();

let pack = session.prefetch_within_budget("用户问题", TokenBudget::new(4096))?;
let _system = pack.render();

let _ = session.call_tool(TOOL_RECALL, json!({"text": "主题", "limit": 5}));
session.compact_working()?;
session.end_turn_consolidate()?;
```

工具名：`memory_remember`、`memory_recall`、`memory_forget`、`memory_pin`、`memory_consolidate`。`call_tool` 不 panic，失败时 `ok: false`。

仍提供不带预算的 `prefetch` + `pack_context`。

## API 一页纸

**打开：** `open("./memory.db")` / `open_in_memory()` / `SqliteStore::builder()`。有真实向量模型再注入 [`Embedder`](#embedder)；默认是离线 `HashEmbedder`。

**项目：** `create_project`、`store.project("id")`、`store.session("id")`。隔离靠 `project_id`，召回不会串项目。

**写入：** `remember` 或 `remember_many` / `remember_turn`（一个事务）。可指定 `tier`，否则用简单启发式。

**读取：** `get`、`list_memories`、`recall`。默认 `recall("...")` 只取 8 条。过期未 pin 的行会立刻从召回中消失（pin 会留下）。

**带预算读取：** `prefetch_within_budget` / `pack_context_budgeted`。看 `pack.tokens`。

**pin / forget / compact / consolidate：** `pin` 让记录熬过 TTL，并在预算紧时优先进入 pack。`compact_working` 不是 `consolidate`。consolidate 负责删除过期未 pin，以及 working→episodic→profile。

## MemoryPolicy（按项目）

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
| candidate_prune / scan_limit | 256 / 2048 |

预设：`MemoryPolicy::chat()`、`MemoryPolicy::journal()`。保留时间应 **长于** 晋升延迟。

## 透明性能

`open()` / `store.session` 已启用 WAL、`synchronous=NORMAL`、外键、`temp_store=MEMORY`、约 16 MiB 缓存、5 秒 busy_timeout、语句缓存、embed LRU、召回剪枝。用 `store.applied_pragmas()` 查看。

批量写入请用 `remember_many`。**`consolidate` 和 `compact_working` 仍须显式调用。**

```bash
cargo bench   # 不在 cargo test 里
```

## Embedder 与向量

```rust
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

默认召回：暴力余弦。可选 `--features sqlite-vec` 使用 `SqliteVecIndex`（默认 `cargo test` 不打开）。

## 层级

| 层级 | 用途 |
|------|------|
| `working` | 当前 session / 草稿 |
| `episodic` | 日/工单日志（含 compact 折页） |
| `profile` | 长期事实 |
