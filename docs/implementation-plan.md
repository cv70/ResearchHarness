# 实现计划

本文档定义 Rust 工程的模块划分、测试策略和建议实现顺序。

## Rust 模块建议

```text
src/
  main.rs
  cli.rs
  config.rs
  core.rs
  orchestrator.rs
  policy.rs
  agents/
    mod.rs
    cli_runner.rs
    mock.rs
  execution/
    mod.rs
    workspace.rs
    runner.rs
    metrics.rs
    archive.rs
  memory/
    mod.rs
    store.rs
    learning.rs
```

## 模块职责

`cli.rs`：

- 解析命令。
- 加载配置。
- 调用 Orchestrator。

`config.rs`：

- 读取 `research.toml`。
- 校验必填字段。
- 提供默认值。

`core.rs`：

- 定义 Run、Experiment、MetricSnapshot、Learning 等核心类型。
- 定义状态枚举和错误类型。

`orchestrator.rs`：

- 实现单轮实验状态机。
- 调度 Agent 和 Execution 服务。
- 执行最终 keep、discard、crash 决策。

`policy.rs`：

- 路径策略。
- 经验升级策略。
- Debug 次数策略。
- keep/discard 比较策略。

`agents/`：

- `AgentRunner` trait。
- Claude Code / Codex CLI 适配器。
- 测试用 mock runner。

`execution/`：

- `workspace.rs`：Git 操作和路径校验。
- `runner.rs`：实验命令执行和超时。
- `metrics.rs`：指标解析。
- `archive.rs`：实验档案创建和写入。

`memory/`：

- `store.rs`：Markdown 记忆初始化、读取、追加。
- `learning.rs`：经验分级和 playbook 更新。

## 推荐 crate

- `clap`：CLI 解析。
- `serde` 和 `toml`：配置、状态和 manifest。
- `thiserror`：错误类型。
- `regex`：指标解析。
- `chrono` 或 `time`：时间戳。
- `tempfile`：测试。

Git 初期通过 `git` CLI 实现。后续需要更强库级控制时再引入 `gix`。

## 单元测试

- 配置加载和校验。
- 默认配置值。
- 状态机状态转换。
- 指标 regex 解析和方向比较。
- 允许路径和只读路径校验。
- 实验档案目录创建。
- 完整日志和日志摘录归档。
- Markdown 记忆初始化和追加。
- 经验分级规则。
- Agent request 构造和角色契约。

## 集成测试

- 成功实验保留提交并更新最佳指标。
- 指标变差时回滚到 base commit。
- 命令崩溃时保留档案并回滚。
- 缺失指标视为 crash。
- 修改只读文件时拒绝实验。
- DebugAgent 最多触发配置次数。
- `discard` 和 `crash` 实验同样生成 `manifest.toml`、`run.log`、`analysis.md`、`reflection.md`。
- 多次重复经验可以升级到 `decisions.md` 或 `playbook.md`。
- 下一轮 Agent prompt 包含相关 playbook 规则。

## 手动验收

- 在 Python `autoresearch` 仓库配置 `research.toml`。
- 跑 baseline 实验。
- 确认日志写入 `.research-harness/runs/<tag>/experiments/<id>/run.log`。
- 确认 `experiments.md` 被追加。
- 确认有效经验进入 `decisions.md` 或 `playbook.md`。
- 确认指标变差的实验会回滚。
- 确认指标改进的实验留在分支上。

## 实现顺序

1. 创建 Rust CLI 骨架。
2. 实现配置加载和校验。
3. 实现核心数据模型和状态机。
4. 实现 Markdown memory 初始化和追加。
5. 实现实验 archive、manifest、日志归档和日志摘录。
6. 实现 Git workspace 封装。
7. 实现实验命令运行和超时。
8. 实现指标解析和比较。
9. 实现经验分级和 `playbook.md` 更新规则。
10. 实现 mock `AgentRunner`。
11. 实现单轮 Orchestrator。
12. 增加 Codex 和 Claude CLI 适配器。
13. 增加持续循环模式。
14. 增加 status 和 memory 子命令。

这个顺序先把确定性基础设施测试稳固，再允许真实智能体修改代码。

