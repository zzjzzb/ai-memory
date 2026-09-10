# 架构

中文。English：[ARCHITECTURE.md](ARCHITECTURE.md)

`ai-memory` 是 agent 编排层下面的**嵌入式记忆 / 数据层**。它**不是**完整的 LLM harness：没有模型客户端、没有 DeepSeek/Claude/Codex SDK、没有联网循环。[earendil-works/pi](https://github.com/earendil-works/pi)（统一 LLM API + agent 循环 + 工具）以及类似的 Claude / Codex / DeepSeek 风格编排层负责模型回合。本库负责 **按项目隔离的记忆**、**混合召回**，以及一层 **JSON 工具适配**。

另见 [USAGE.zh-CN.md](USAGE.zh-CN.md)。

## 1. 分层

```mermaid
flowchart TB
    subgraph sme [中小团队应用]
        App[你的 agent 产品]
    end
    subgraph harness [Agent 编排]
        Pi[pi / Claude-like / Codex-like / DeepSeek-like]
        Loop[工具调用循环]
        Pi --> Loop
    end
    subgraph adapter [ai-memory 适配层]
        Sess[AgentSession]
        Pack[ContextPack]
        Tools["ToolSpec JSON：remember / recall / forget / pin / consolidate"]
        Sess --> Pack
        Sess --> Tools
    end
    subgraph kernel [MemoryStore 内核]
        Store[SqliteStore]
        VecIdx[VectorIndex]
        Emb[Embedder]
        Store --> VecIdx
        Store --> Emb
    end
    App --> Pi
    Loop -->|"prefetch / pack_context / call_tool"| Sess
    Sess --> Store
    VecIdx -.->|可选 feature sqlite-vec| SqliteVec[SqliteVecIndex]
    VecIdx --> BF[默认 BruteForceCosine]
```

所有项目共用一套 **Rust + SQLite** 内核。隔离靠 `project_id` + `MemoryPolicy`，不是换引擎。未来 Lance 同样实现 `MemoryStore` 即可。

## 2. 单次 agent 回合

```mermaid
sequenceDiagram
    participant H as 编排循环
    participant S as AgentSession
    participant K as SqliteStore
    participant M as LLM（由编排层调用）

    H->>S: prefetch(用户消息)
    S->>K: recall(project_id, query)
    K-->>S: 排序命中（已应用 TTL）
    S->>S: pack_context(hits)
    H->>M: system += ContextPack.render() + ToolSpec JSON
    M-->>H: 可选 tool_calls
    loop 每个工具调用
        H->>S: call_tool(name, JSON args)
        S->>K: remember / recall / forget / pin / consolidate
        S-->>H: ToolResponse JSON（成功或错误字符串）
        H->>M: 追加工具结果
    end
    H->>S: remember_turn(用户/助手笔记) 可选
    H->>S: end_turn_consolidate() 可选
```

**模型调用前：** `prefetch` + `ContextPack`，带上 `id` / `tier` / `score`。  
**工具阶段：** `memory_remember`、`memory_recall`、`memory_forget`、`memory_pin`、`memory_consolidate`。  
**回合结束后：** 可选 `remember_many` / `end_turn_consolidate`。

## 3. 多项目隔离

```mermaid
flowchart LR
    DB[(同一个 memory.db WAL 文件)]
    subgraph p1 [项目 support-bot]
        A1[AgentSession]
        Pol1[MemoryPolicy chat]
    end
    subgraph p2 [项目 journal]
        A2[AgentSession]
        Pol2[MemoryPolicy journal]
    end
    A1 -->|"WHERE project_id = support-bot"| DB
    A2 -->|"WHERE project_id = journal"| DB
    Pol1 -.-> DB
    Pol2 -.-> DB
```

多个 agent / 产品共用一个文件。召回、pin、forget、工具分发都带上 session 的 `project_id`。用别人的 memory id 去 `recall` 也拿不到对方文本。

## 4. 性能路径

```mermaid
flowchart TB
    Open[打开文件库] --> WAL[PRAGMA journal_mode=WAL]
    WAL --> Busy[busy_timeout 5s]
    Busy --> Sync[synchronous=NORMAL]
    Remember[remember_many] --> Tx[单次 SQLite 事务]
    Recall[recall] --> SQL[SQL：项目 + 时间 + 层级]
    SQL --> TTL[丢掉过期未 pin]
    TTL --> Prune[关键词 + 新近度廉价剪枝]
    Prune --> Blobs[仅为幸存者加载 embedding blob]
    Blobs --> VI[VectorIndex.similar]
    VI --> Rank[混合打分 + top-k]
    EmbInj[注入 Embedder] --> Remember
    EmbInj --> Recall
    VecInj[注入 VectorIndex / sqlite-vec] --> VI
```

- 文件库启用 **WAL + busy_timeout**。进程内由 `Mutex<Connection>` 串行，**单写者**。WAL 仍方便其他进程读同一文件。
- **批量写入：** `remember_many` / `AgentSession::remember_turn` 先校验并 embedding，再在一个事务里插入（任一项非法则整批不提交）。
- **候选剪枝：** 先按关键词 + 新近度，再算向量；`MemoryPolicy.recall.candidate_prune`（默认 256，`0` 关闭）或 `RecallQuery::with_candidate_limit`。
- **可注入** `Embedder` 与 `VectorIndex`。默认暴力余弦；`--features sqlite-vec` 使用 `SqliteVecIndex`。

## 接到通用 tool-calling 循环

不绑厂商 SDK。注册 JSON Schema，然后按回合驱动：

```rust
use ai_memory::{memory_tool_specs, AgentSession, SqliteStore};

let store = SqliteStore::open("./memory.db")?;
store.create_project("support-bot", ai_memory::MemoryPolicy::chat())?;
let session = AgentSession::sqlite(store, "support-bot")?;

let tools = memory_tool_specs();
let _openai = tools.iter().map(|t| t.openai_tool()).collect::<Vec<_>>();
let _anthropic = tools.iter().map(|t| t.anthropic_tool()).collect::<Vec<_>>();

let hits = session.prefetch("用户问题")?;
let system_memory = session.pack_context(&hits).render();

let result = session.call_tool("memory_recall", serde_json::json!({"text": "主题"}));

session.end_turn_consolidate()?;
```

模拟循环：

```bash
cargo run --example harness_loop_sim
```

## 本库明确不做

- 不替代 pi / Claude / Codex / DeepSeek 编排层
- 不是云同步或多租户服务
- 暂无 FFI 语言绑定
- 不自研存储引擎
