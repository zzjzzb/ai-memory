# 场景：中型公司支持 / 运维代理（dsh + ai-memory）

中文。[English](README.md)

[dsh-ai-memory](../../integrations/dsh-ai-memory/) 薄 Cordis 插件的旗舰 **用法场景**。不是用 TypeScript 再做一套记忆。真相源仍是 Rust 的 `HostSession` / `prefetch_within_budget`。

背书文档：[INTEGRATION_DSH.zh-CN.md](../../docs/INTEGRATION_DSH.zh-CN.md) · [English](../../docs/INTEGRATION_DSH.md)

## 故事

中型公司用 **一条很长的 DeepSeek Harness 会话** 处理支持 / 运维。相关工单进同一条聊天：

| 工单 | 内容 |
|------|------|
| **T-1042** | Acme 的 Maya：改导航后 Settings **侧栏挡住**发票表；150% 缩放下还有后续。 |
| **T-1088** | 财务：发票 **INV-22091 收了两次**；只要冲正、不要贷记。 |
| **Pin** | 耐久事实：账单负责人是 **Ada Chen**（`PINNED-BILLING-OWNER-ADA`）。 |
| **隔离** | 同一 `.db` 上的项目 `sme-hr` 只放手册笔记 —— 召回不能串。 |

原始 transcript（加上诊断 dump）比一次 pack 大，若整段塞进 prompt，会撑爆约 100 万（或更小）的模型窗口。代理用 `memory_remember` **先落盘**，再用预算内的 `ai-memory:pack`（`prefetch_within_budget`）注入，而不是倒历史。

## 无头 / CI（不需要 dsh 网页）

模拟器用假的 Cordis `ctx` 调用插件的 `apply(ctx)`，路径与 dsh 相同：`ctx.tools.register` + `ctx.systemPrompt.section`。绑定是 napi-rs 或 `ai-memory` CLI。JS 不实现召回。

```bash
cargo build --bin ai-memory
node scenarios/dsh-support-agent/sim/run.mjs
npm test --prefix scenarios/dsh-support-agent
```

### 看什么

- **预算 pack：** `tokens <= tokenBudget`（种子默认 256）。标题形如 `## Memory (project: sme-support, N hits)`。
- **transcript vs pack：** 打印的原始会话 `ceil(字符/4)` **大于** pack。不要倒历史。
- **Pin 能留下：** 紧预算（96）仍含 `PINNED-BILLING-OWNER-ADA`。
- **项目隔离：** `sme-hr` 只有手册，**不能**出现 T-1042 / sidebar overlap。
- **工具：** `memory_remember` / `recall` / `forget` / `pin` / `consolidate` / `compact`。

种子：[`seed/tickets.json`](seed/tickets.json)。Rust 冒烟：`cargo test --test dsh_support_scenario`。

## 真机 dsh（`dsh plugin add`）

已安装 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) CLI 时：

```bash
dsh plugin --profile support-ops add ./integrations/dsh-ai-memory
dsh --profile support-ops --dump-config    # 应出现 "# == dsh-ai-memory"
```

在 profile 补丁里把 `projectId` 设为 `sme-support`，`tokenBudget` 可先用 `256` 对齐本模拟（真实对话再调到 8192）。把 `seed/tickets.json` 里的用户句贴进会话，让模型（或你）调用 `memory_remember`，并对 Ada 账单事实 `memory_pin`。下一轮系统提示应出现短的 **`ai-memory:pack`**，而不是整段聊天。

CI **不依赖** dsh。提交 [dsh.pub](https://dsh.pub/zh/submit/) 是下一步，不在本 PR。
