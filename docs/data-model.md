# 核心数据模型

本文档定义 Rust 实现时需要稳定下来的核心类型、状态和经验等级。

## Run

一次长期自动实验会话。

字段：

- `tag`
- `branch`
- `started_at`
- `best_metric`
- `best_commit`
- `experiment_count`
- `consecutive_crashes`
- `consecutive_regressions`

职责：

- 记录当前 run 的总体进展。
- 为 CoordinatorAgent 提供失败趋势和当前最佳结果。
- 持久化到 `runs/<run-tag>/state.toml`。

## Experiment

单轮实验。

字段：

- `id`
- `run_tag`
- `base_commit`
- `candidate_commit`
- `status`
- `hypothesis`
- `metric_snapshot`
- `archive_path`
- `debug_attempts`

状态：

- `planned`：已创建实验 ID 和档案目录。
- `edited`：CodingAgent 已产生代码改动。
- `reviewed`：ReviewAgent 已审查。
- `running`：Runner 正在执行实验命令。
- `kept`：指标改进，候选提交保留。
- `discarded`：指标未改进，已回滚。
- `crashed`：命令失败、超时、路径违规或指标缺失。
- `archived`：日志、复盘和记忆写入已完成。

约束：

- 每个 Experiment 只有一个主要假设。
- 每个 Experiment 都必须进入 `archived`，不能因为失败跳过归档。

## ExperimentArchive

单轮实验的完整档案。

字段：

- `manifest_path`
- `plan_path`
- `diff_path`
- `run_log_path`
- `log_excerpt_path`
- `analysis_path`
- `reflection_path`

职责：

- 保存实验当时的完整上下文。
- 支持后续人工审查和 Agent 复盘。
- 为经验升级提供证据来源。

## MetricSnapshot

指标解析和比较结果。

字段：

- `name`
- `value`
- `previous_best`
- `direction`
- `improved`
- `source_log`

规则：

- `direction = lower` 时，当前值小于历史最佳值才算改进。
- `direction = higher` 时，当前值大于历史最佳值才算改进。
- 指标缺失、无法解析或命令失败时，实验状态为 `crashed`。

## Learning

复盘中提炼出的经验。

字段：

- `summary`
- `evidence`
- `level`
- `source_experiment_ids`
- `recommended_action`

经验等级：

- `single-observation`：单次观察，写入 `experiments.md`。
- `stable-decision`：稳定经验，写入 `decisions.md`。
- `playbook-rule`：可执行策略，写入或更新 `playbook.md`。

升级规则：

- 单次实验默认只能产生 `single-observation`。
- 多次重复信号或非常明确的失败原因可以升级为 `stable-decision`。
- 能直接影响未来实验优先级、禁忌或调度方式的稳定经验，才能升级为 `playbook-rule`。

## PlaybookRule

能影响后续实验选择的策略资产。

字段：

- `rule`
- `when_to_apply`
- `why`
- `evidence`
- `priority`

约束：

- 必须是可执行策略。
- 必须能被 CoordinatorAgent、ResearchAgent 或 PlanningAgent 使用。
- 不能保存原始日志、冗长复盘或无法行动的泛泛总结。

## 状态持久化关系

- Run 写入 `state.toml`。
- Experiment 元数据写入 `manifest.toml`。
- MetricSnapshot 同时进入 `manifest.toml` 和 `experiments.md` 摘要。
- Learning 根据等级进入 `experiments.md`、`decisions.md` 或 `playbook.md`。
- PlaybookRule 只进入 `playbook.md`。

