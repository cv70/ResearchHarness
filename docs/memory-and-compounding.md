# 记忆与复利机制

本文档定义长期记忆、日志复盘和经验升级机制。

系统的长期能力来自实验资产复用，而不是单纯增加实验次数。

## 记忆分类

`business.md`：

- 项目目标。
- 业务约束。
- 评估规则。
- 领域事实。
- 人类明确给出的长期说明。

`experiments.md`：

- 单轮实验事实记录。
- 指标结果。
- 简短复盘。
- archive 路径。

`decisions.md`：

- 多次实验支持的稳定经验。
- 明确的失败模式。
- 已验证的有效策略。

`playbook.md`：

- 会影响后续实验选择的可执行策略。
- 禁忌。
- 优先级规则。
- 调度倾向。

## 实验档案与日志

每轮实验保存完整 archive：

- `manifest.toml`
- `plan.md`
- `diff.patch`
- `run.log`
- `log_excerpt.md`
- `analysis.md`
- `reflection.md`

原则：

- 完整日志保存在 `run.log`。
- Agent 默认读取 `log_excerpt.md`，避免上下文被原始日志淹没。
- DebugAgent 可读取更多日志用于排错。
- `experiments.md` 只保存摘要，不保存完整日志。
- `discard` 和 `crash` 实验同样保留 archive。

## 复盘流程

1. Runner 保存完整日志和日志摘录。
2. Metrics 解析指标并给出比较结果。
3. AnalystAgent 基于计划、diff、指标和日志解释实验结果。
4. AnalystAgent 复盘哪些判断正确、哪些判断失效。
5. MemoryAgent 将复盘转成经验候选。
6. Rust 根据经验等级写入对应记忆文件。
7. 下一轮 Agent prompt 注入相关经验。

## 经验生命周期

`single-observation`：

- 单轮事实和初步判断。
- 写入 `experiments.md`。
- 默认所有新经验先处于这一层。

`stable-decision`：

- 多个实验支持，或证据非常明确。
- 写入 `decisions.md`。
- 用于减少重复试错。

`playbook-rule`：

- 能直接影响未来实验优先级、禁忌或调度的策略。
- 写入或更新 `playbook.md`。
- 必须可执行。

升级示例：

- 单次 OOM：`single-observation`。
- 连续多次增大深度导致 OOM：`stable-decision`。
- “扩大模型前先降低 batch size 或减少上下文窗口”：`playbook-rule`。

## 下一轮 Prompt 注入规则

CoordinatorAgent：

- 必须读取 `playbook.md` 和 run 状态。
- 用于决定是否继续探索、收敛、避开高风险方向。

ResearchAgent：

- 必须读取相关历史实验和 playbook 规则。
- 用于提出更有根据的单一假设。

PlanningAgent：

- 必须读取 playbook 中的禁忌和策略。
- 用于限制改动范围和定义失败信号。

DebugAgent：

- 只读取当前实验计划、失败日志和允许修复边界。
- 不读取过多长期策略，避免在 Debug 阶段改变目标。

## 复利判断标准

系统是否产生复利，取决于以下信号：

- 重复失败减少。
- 有效实验区域更快收敛。
- 失败日志转化为明确禁忌。
- 成功实验转化为邻域探索策略。
- 长期记忆影响下一轮假设，而不是只做事后记录。

## 写入边界

只能 Rust 写入：

- `business.md`
- `experiments.md`
- `decisions.md`
- `playbook.md`

Agent 只能生成候选内容。

`playbook.md` 不能堆积：

- 原始日志。
- 冗长复盘。
- 无法行动的泛泛总结。
- 缺少证据的单次偶然判断。

## 实验记录格式

成功示例：

```markdown
## 2026-07-10 12:30:00 - exp-00042 - abc1234

- Status: keep
- Metric: val_bpb=0.997900
- Previous Best: val_bpb=1.001200
- Hypothesis: Increase embedding learning rate.
- Changes: Adjusted `EMBEDDING_LR` from 0.2 to 0.6.
- Result: Validation BPB improved by 0.003300.
- Decision: Keep.
- Reflection: Higher embedding LR helped without increasing VRAM; nearby values are worth exploring.
- Experience Level: stable-decision
- Archive: `.research-harness/runs/jul10/experiments/exp-00042/`
- Follow-up: Test nearby values.
```

崩溃示例：

```markdown
## 2026-07-10 12:44:00 - exp-00043 - no-commit

- Status: crash
- Metric: unavailable
- Previous Best: val_bpb=0.997900
- Hypothesis: Double model depth.
- Changes: Increased `DEPTH` from 8 to 16.
- Result: Experiment timed out or exited non-zero.
- Decision: Discard and reset.
- Reflection: Depth increase exceeded available memory; future depth changes should first reduce batch size.
- Experience Level: single-observation
- Archive: `.research-harness/runs/jul10/experiments/exp-00043/`
- Follow-up: Try smaller depth increase or lower batch size.
```

