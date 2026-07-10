# ResearchHarness

ResearchHarness 是一个用 Rust 实现的自动实验引擎，目标是在目标代码仓库中持续运行自主实验。

它不实现具体实验逻辑，也不重写训练器。它负责把外部编码智能体、目标项目实验命令、Git 工作区、日志、指标、记忆和复盘组织成一个可靠闭环。

## 核心目标

- 自动完成“提出实验 -> 修改代码 -> 运行实验 -> 解析指标 -> 保留或回滚 -> 复盘沉淀”的循环。
- 每轮实验都保存完整日志和可追溯档案。
- 每轮实验都提炼可复用经验，让后续实验质量产生复利。
- 用 Rust 持有可信边界，避免把最终决策交给 LLM 自然语言输出。

## 系统边界

ResearchHarness 负责：

- 多智能体实验编排。
- Claude Code / Codex 等外部 Agent 调用。
- Git 分支、提交、diff 和回滚。
- 实验命令执行、超时控制和日志归档。
- 指标解析和 keep/discard/crash 决策。
- Markdown 长期记忆写入。

目标项目负责：

- 实验代码。
- 实验命令。
- 评估实现。
- 指标输出格式。
- 领域约束和实验目标。

## 设计文档

- [架构总览](docs/architecture.md)
- [配置与目录结构](docs/configuration.md)
- [核心数据模型](docs/data-model.md)
- [多智能体角色](docs/agents.md)
- [实验生命周期](docs/experiment-lifecycle.md)
- [记忆与复利机制](docs/memory-and-compounding.md)
- [实现计划](docs/implementation-plan.md)

## 设计原则

- Rust 是可信控制平面：提交、回滚、指标比较、记忆写入都由 Rust 执行。
- Agent 只负责认知任务：提出假设、修改允许文件、解释结果、生成记忆候选。
- 每轮实验只允许一个主要假设，保证结果可归因。
- 失败实验也必须保留日志和复盘，因为失败经验同样能减少未来重复试错。
- 长期记忆分层沉淀：单次观察进入 `experiments.md`，稳定经验进入 `decisions.md`，可执行策略进入 `playbook.md`。

## 当前状态

当前仓库已经落地 Rust 2024 edition 的第一版工程骨架，包含：

- CLI 入口。
- `research.toml` 配置加载和校验。
- 核心数据模型和实验状态。
- Markdown 长期记忆初始化和追加。
- 实验档案目录、manifest、日志归档和日志摘录。
- Git workspace 封装。
- 实验命令运行和指标解析。
- AgentRunner 抽象、mock agent 和 CLI agent 适配器。
- 单轮 Orchestrator 闭环。

## 本地开发

```bash
cargo test
cargo fmt --check
```

第一版可用命令：

```bash
cargo run -- init
cargo run -- setup --tag test
cargo run -- run --tag test --once
cargo run -- status --tag test
```

连续自动实验循环、真实 Claude Code / Codex 适配参数和更完整的经验升级策略仍按 [实现计划](docs/implementation-plan.md) 继续迭代。
