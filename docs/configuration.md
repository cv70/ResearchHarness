# 配置与目录结构

本文档定义用户如何配置目标项目，以及 ResearchHarness 在运行时生成哪些目录和文件。

## `research.toml`

配置文件位于目标仓库根目录。

最小示例：

```toml
[project]
name = "autoresearch"

[workspace]
modifiable = ["train.py"]
readonly = ["prepare.py", "research.toml"]

[experiment]
command = "uv run train.py"
log_file = "run.log"
timeout_seconds = 600
archive_logs = true
max_log_excerpt_lines = 200
max_debug_attempts = 1

[metric]
name = "val_bpb"
regex = "^val_bpb:\\s+([0-9.]+)"
direction = "lower"

[agent]
backend = "codex"
```

## 配置职责

`project`：

- `name`：项目名称，用于日志、状态和提示词。

`workspace`：

- `modifiable`：Agent 允许修改的路径。
- `readonly`：Agent 不能修改的路径。

`experiment`：

- `command`：每轮实验执行命令。
- `log_file`：目标项目内的原始日志文件名。
- `timeout_seconds`：实验命令超时时间。
- `archive_logs`：是否把每轮完整日志归档到 `.research-harness/`。
- `max_log_excerpt_lines`：给 Agent 阅读的日志摘录最大行数。
- `max_debug_attempts`：DebugAgent 每轮最多介入次数，默认 1。

`metric`：

- `name`：主指标名称。
- `regex`：从日志中解析指标的正则表达式，捕获组 1 必须是数值。
- `direction`：`lower` 或 `higher`。

`agent`：

- `backend`：第一版支持 `codex` 或 `claude` 这类 CLI 后端适配器。

## 运行时目录

ResearchHarness 在目标仓库下创建 `.research-harness/`。

```text
.research-harness/
  memory/
    business.md
    experiments.md
    decisions.md
    playbook.md
  runs/
    <run-tag>/
      state.toml
      prompts/
      experiments/
        <experiment-id>/
          manifest.toml
          plan.md
          diff.patch
          run.log
          log_excerpt.md
          analysis.md
          reflection.md
```

## 目录职责

`memory/` 保存跨 run 的长期记忆：

- `business.md`：项目目标、业务约束、评估标准和领域事实。
- `experiments.md`：事实性实验流水。
- `decisions.md`：稳定经验。
- `playbook.md`：影响后续实验选择的策略资产。

`runs/<run-tag>/` 保存一次长期自动实验会话：

- `state.toml`：run 级状态。
- `prompts/`：调用 Agent 时使用的提示词快照。
- `experiments/<experiment-id>/`：单轮实验档案。

## 实验档案文件

每轮实验都必须生成档案，即使最终 `discard` 或 `crash`。

- `manifest.toml`：实验 ID、时间、base commit、candidate commit、状态、指标、耗时、触发角色。
- `plan.md`：研究假设和执行计划。
- `diff.patch`：最终候选改动。
- `run.log`：实验命令完整 stdout/stderr。
- `log_excerpt.md`：供 Agent 阅读的关键日志摘录。
- `analysis.md`：AnalystAgent 的结果解释。
- `reflection.md`：MemoryAgent 的复盘和经验候选。

## 归档原则

- 完整日志只保存在实验档案中。
- `experiments.md` 只保存事实摘要和复盘摘要。
- 失败实验不能丢弃档案，因为失败日志是后续避免重复试错的重要输入。
- Agent 可以读取日志摘录，只有 DebugAgent 在排错时读取更详细日志。

