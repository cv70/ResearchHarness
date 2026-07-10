# 实验生命周期

本文档定义单轮实验如何从计划推进到归档，以及每种失败分支如何处理。

## 状态机

单轮 Experiment 状态：

- `planned`
- `edited`
- `reviewed`
- `running`
- `kept`
- `discarded`
- `crashed`
- `archived`

约束：

- 单轮实验必须按状态机推进。
- 每轮实验只能有一个主要假设。
- 每轮实验都必须进入 `archived`。
- 不能因为失败、回滚或崩溃跳过日志归档和复盘。

## 主流程

1. `planned`：创建 experiment ID 和 archive 目录，记录 base commit。
2. 加载 `business.md`、`experiments.md` 摘要、`decisions.md`、`playbook.md`。
3. CoordinatorAgent 生成调度建议。
4. ResearchAgent 生成一个实验假设。
5. PlanningAgent 生成执行计划，Rust 写入 `plan.md`。
6. CodingAgent 修改代码。
7. ReviewAgent 审查 diff。
8. 如 Review 失败，DebugAgent 在配置次数内做最小修复。
9. Rust 执行路径校验和只读文件校验。
10. Rust 保存 `diff.patch`。
11. Rust 提交候选改动。
12. Runner 运行实验命令，保存完整 `run.log` 和 `log_excerpt.md`。
13. 如命令崩溃，DebugAgent 可在配置次数内做最小修复并重试。
14. Metrics 解析日志指标。
15. Rust 比较当前指标和历史最佳指标。
16. 指标改进时，状态为 `kept`，候选提交成为新基线。
17. 指标退化、缺失、崩溃或超时时，状态为 `discarded` 或 `crashed`，回滚到 base commit。
18. AnalystAgent 解释结果，Rust 写入 `analysis.md`。
19. MemoryAgent 生成复盘，Rust 写入 `reflection.md`。
20. Rust 追加 `experiments.md`，必要时追加 `decisions.md` 或更新 `playbook.md`。
21. Rust 更新 `state.toml`。
22. 状态进入 `archived`，开始下一轮。

## 决策规则

`keep`：

- 实验命令成功。
- 指标成功解析。
- 指标相对历史最佳值改进。
- 候选改动通过路径和只读文件校验。

`discard`：

- 实验命令成功。
- 指标成功解析。
- 指标未改进。
- 默认情况下指标持平也视为 discard。

`crash`：

- Review 失败且 Debug 后仍失败。
- 路径违规。
- 修改只读文件。
- 实验命令非零退出且 Debug 后仍失败。
- 实验超时。
- 指标缺失或无法解析。

## 失败分支

Review 失败：

- 允许 DebugAgent 做最小修复。
- 仍失败则 `crashed`。
- 必须保存 diff、review 意见和 reflection。

路径违规：

- 立即 `crashed`。
- 回滚到 base commit。
- 记录违规路径。

命令非零退出：

- 保存完整日志。
- DebugAgent 可在配置次数内做最小修复并重试。
- 仍失败则 `crashed`。

超时：

- 直接 `crashed`。
- 不继续 Debug，除非后续配置明确允许。

指标缺失：

- `crashed`。
- `log_excerpt.md` 应包含最接近指标输出位置的日志片段。

指标退化：

- `discarded`。
- 回滚到 base commit。
- 仍然生成 `analysis.md` 和 `reflection.md`。

指标持平：

- 默认 `discarded`。
- 未来可配置“同指标但显著简化”作为保留策略，但 v1 默认不启用。

## Debug 规则

- 默认 `max_debug_attempts = 1`。
- DebugAgent 只能修复执行错误。
- DebugAgent 不能改变实验假设。
- DebugAgent 不能扩大改动范围。
- DebugAgent 的改动也必须经过 Review 和 Rust 路径校验。

## 归档规则

无论结果如何，每轮都必须归档：

- `manifest.toml`
- `plan.md`
- `diff.patch`
- `run.log`
- `log_excerpt.md`
- `analysis.md`
- `reflection.md`

如果某个阶段未产生对应内容，文件仍应存在，并写明未产生原因。

