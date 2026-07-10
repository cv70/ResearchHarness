# 多智能体角色

本文档定义 AgentRunner 抽象和多智能体角色边界。

Rust Orchestrator 调度角色，Agent 只执行被分配的认知任务。所有角色都可以由同一个 Claude Code 或 Codex 后端扮演。

## AgentRunner 抽象

Agent 层通过统一 `AgentRunner` 抽象调用外部 CLI 智能体。

```text
AgentRequest:
  role
  working_directory
  system_prompt
  task_prompt
  allowed_paths
  context_files
  timeout_seconds

AgentResponse:
  stdout
  stderr
  exit_status
  duration
  artifact_paths
```

原则：

- Agent 输出都是候选产物。
- Rust 必须验证路径、指标、状态和记忆写入。
- 同一个后端可以按不同 prompt 扮演不同角色。

## CoordinatorAgent

职责：

- 基于 run 状态、业务记忆、实验历史、决策记忆和研究手册，建议本轮调度。
- 在连续崩溃或连续退化后建议收敛策略。

输入：

- Run 状态。
- `business.md`
- `experiments.md` 摘要。
- `decisions.md`
- `playbook.md`

输出：

- 本轮角色调用建议。
- 本轮目标摘要。
- 是否允许 DebugAgent 介入的条件。

禁止：

- 不修改代码。
- 不写文件。
- 不决定 keep、discard、crash。

## ResearchAgent

职责：

- 提出一个可归因的实验假设。
- 说明动机、预期收益、风险和历史依据。

输入：

- `business.md`
- `experiments.md` 摘要。
- `decisions.md`
- `playbook.md`
- 当前最佳指标。
- 当前 Git 状态。

输出：

- 一个实验假设。
- 预期影响。
- 可能修改的文件。
- 风险说明。

禁止：

- 一次提出多个互相独立的实验。
- 修改评估标准或实验目标。
- 直接修改代码。

## PlanningAgent

职责：

- 把实验假设转成可执行计划。
- 明确改动范围、成功标准、失败信号和 Debug 边界。

输入：

- ResearchAgent 的实验假设。
- 配置中的可修改路径和只读路径。
- `playbook.md` 中的策略和禁忌。
- 当前最佳指标。

输出：

- `plan.md` 候选内容。
- 可修改文件范围。
- 成功标准和失败信号。
- DebugAgent 允许修复的边界。

禁止：

- 不直接修改代码。
- 不扩大 ResearchAgent 的实验目标。
- 不改变指标定义或优化方向。

## CodingAgent

职责：

- 按 `plan.md` 修改允许范围内的代码。
- 生成实现摘要。

输入：

- `plan.md`
- 允许修改路径。
- 最近相关失败记录。

输出：

- 代码改动。
- 实现摘要。

禁止：

- 不修改只读文件。
- 不修改评估逻辑，除非配置明确允许。
- 不在实现阶段改变实验假设。
- 不把多个独立实验混入一轮。

## ReviewAgent

职责：

- 审查 diff 是否符合计划、路径约束和工程质量要求。
- 给出 `pass` 或 `fail` 以及必要修复意见。

输入：

- `plan.md`
- Git diff。
- 可修改路径和只读路径规则。
- 业务约束。

输出：

- `pass` 或 `fail`。
- 修复意见。

注意：

- ReviewAgent 通过不代表最终通过。
- Rust 仍必须执行硬路径校验。

## DebugAgent

职责：

- 只在 Review 失败、命令崩溃或日志显示明显实现错误时介入。
- 做最小修复并保持原实验意图不变。

输入：

- `plan.md`
- 当前 diff。
- Review 失败意见或崩溃日志。
- Debug 修复边界。

输出：

- 最小修复改动。
- 修复摘要。
- 无法修复时的放弃原因。

限制：

- 默认最多 1 次。
- 不能扩大改动范围。
- 不能把失败实验改造成另一个新实验。
- 不能改变实验目标。

## AnalystAgent

职责：

- 基于计划、diff、指标和日志解释实验结果。
- 复盘哪些判断正确、哪些判断失效。
- 提炼对下一轮有用的经验信号。

输入：

- `plan.md`
- `diff.patch`
- MetricSnapshot。
- `log_excerpt.md`
- 实验档案索引。

输出：

- `analysis.md` 候选内容。
- follow-up 建议。
- 可复用经验信号。

禁止：

- 不拥有最终裁决权。
- 不覆盖 Rust 的指标比较结果。
- 不修改代码或记忆文件。

## MemoryAgent

职责：

- 将 AnalystAgent 的复盘转成记忆候选。
- 判断经验等级：单次观察、稳定经验或研究手册规则。

输入：

- `analysis.md`
- 最近实验历史。
- 当前 `business.md`
- 当前 `decisions.md`
- 当前 `playbook.md`

输出：

- `reflection.md` 候选内容。
- `experiments.md` 追加候选。
- `decisions.md` 追加候选。
- `playbook.md` 追加或更新候选。

禁止：

- 不直接写文件。
- 不删除历史。
- 不把单次噪声升级为长期策略。

