# 参与贡献 ai-memory

欢迎一起把这个库做好。它是给 agent harness 用的**本地记忆层**——不是 LLM 客户端，也不是云同步产品。

English: [CONTRIBUTING.md](CONTRIBUTING.md)

## 快速开始

```bash
git clone https://github.com/zzjzzb/ai-memory.git
cd ai-memory
cargo test
cargo run --example harness_loop_sim
```

可选向量扩展测试：

```bash
cargo test --features sqlite-vec
```

## 怎么参与

1. 先开 [Issue](https://github.com/zzjzzb/ai-memory/issues) 说 bug 或设计（大改动建议先讨论）。
2. Fork 后从 `main` 拉分支。
3. 改动尽量小而清晰，Rust 写法保持朴素可维护。
4. 有行为变化就补测试，保证 `cargo test` 全绿。
5. 动到公开 API 或推荐接入方式时，同步改文档：
   - [docs/USAGE.zh-CN.md](docs/USAGE.zh-CN.md) / [docs/USAGE.md](docs/USAGE.md)
   - [docs/ARCHITECTURE.zh-CN.md](docs/ARCHITECTURE.zh-CN.md) / [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
   - DeepSeek Harness 展示：[docs/INTEGRATION_DSH.zh-CN.md](docs/INTEGRATION_DSH.zh-CN.md) / [docs/INTEGRATION_DSH.md](docs/INTEGRATION_DSH.md)、[`integrations/dsh-ai-memory/`](integrations/dsh-ai-memory/)（目录内 `DSH_AI_MEMORY_SKIP_NATIVE=1 npm test`），以及旗舰场景 [`scenarios/dsh-support-agent/`](scenarios/dsh-support-agent/)（`cargo test --test dsh_support_scenario`）
6. 向 `main` 提 PR，写清楚**为什么**要改。

## 设计边界（请遵守）

**范围内**

- 按项目隔离的记忆、`MemoryPolicy`、混合召回、Harness 适配层（`AgentSession`、`HostSession`、工具、带 token 预算的 `ContextPack`）
- 薄的 DeepSeek Harness Cordis 插件（调用本 crate：napi 或 CLI，不用 JS 重写存储）
- 驱动该插件的用法场景（无头模拟 + 文档化的 `dsh plugin add`）
- 对开发者透明的本地性能（SQLite 默认、缓存、裁剪）
- 默认离线（测试不依赖网络）

**默认不做（要做请先开 Issue）**

- 完整 LLM harness / 各家 SDK（pi、Claude、Codex、DeepSeek 客户端）
- 多端同步或托管多租户云
- 后台偷偷 `consolidate` 或静默改策略
- 把整段超长对话硬塞进模型（请用 `prefetch_within_budget`）

## 代码风格

- 跟现有模块结构对齐（`store`、`sqlite`、`harness` 等）。
- 公开 API 要让中小团队好用：默认合理、旋钮少。
- 没有明确收益就不要加重新依赖。

## 许可证

MIT OR Apache-2.0 双许可。贡献默认按相同条款接受。
