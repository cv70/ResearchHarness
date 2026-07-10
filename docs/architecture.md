# ResearchHarness 架构总览

ResearchHarness 是一个用 Rust 实现的自动实验引擎，用来在目标代码仓库中持续运行自主实验。

它不实现具体实验逻辑，也不重写训练器。它负责把外部编码智能体、目标项目实验命令、Git 工作区、日志、指标、记忆和复盘组织成一个可靠闭环。

## 文档索引

- [配置与目录结构](configuration.md)：`research.toml`、运行时目录、实验档案文件。
- [核心数据模型](data-model.md)：Run、Experiment、Metric、Learning、PlaybookRule 等实现类型。
- [多智能体角色](agents.md)：AgentRunner 抽象和各智能体职责边界。
- [实验生命周期](experiment-lifecycle.md)：单轮实验状态机、主流程、失败分支。
- [记忆与复利机制](memory-and-compounding.md)：长期记忆、日志复盘、经验升级、下一轮 prompt 注入。
- [实现计划](implementation-plan.md)：Rust 模块建议、测试策略、落地顺序。

## 系统目标

- 自动完成“提出实验 -> 修改代码 -> 运行实验 -> 解析指标 -> 保留或回滚 -> 复盘沉淀”的循环。
- 每轮实验都留下可追溯档案。
- 每轮实验都提炼可复用经验，让后续实验质量产生复利。
- 用 Rust 持有可信边界，避免把最终决策交给 LLM 自然语言输出。

## 非目标

- 不用 Rust 重写训练循环。
- 不硬编码 LLM、深度学习或特定领域实验假设。
- 不绑定特定模型供应商或 LLM API。
- v1 不引入 SQLite、向量数据库或外部记忆服务。

## 系统边界

ResearchHarness 负责：

- 实验编排。
- Agent 调用。
- Git 分支、提交、diff 和回滚操作。
- 实验命令执行和超时控制。
- 日志归档。
- 指标解析。
- keep、discard、crash 决策。
- Markdown 长期记忆写入。

目标项目负责：

- 被修改的源码。
- 实验命令。
- 评估实现。
- 指标输出格式。
- 领域相关约束和目标。

外部智能体负责：

- 提出实验假设。
- 修改允许范围内的文件。
- 审查或解释改动。
- 总结结果。
- 生成记忆候选。

## 分层架构

系统按六层组织，依赖方向只能自上而下。

### CLI 层

CLI 是用户入口，只负责解析命令、加载配置、构造服务并调用 Orchestrator。

第一版命令：

- `research-harness init`
- `research-harness setup --tag <tag>`
- `research-harness run`
- `research-harness status`
- `research-harness memory add-business`
- `research-harness memory add-experiment`

CLI 不直接操作 Git、不运行实验命令、不写记忆文件。

### Orchestrator 层

Orchestrator 是真实控制者，负责实验状态机和角色调度。

它负责创建 run 和 experiment、加载上下文、调用 Agent、调用执行层服务、执行最终决策，并确保每个失败分支都有归档和复盘。

`CoordinatorAgent` 只是调度建议者，不替代 Orchestrator。

### Agent 层

Agent 层通过统一 `AgentRunner` 抽象调用 Claude Code 或 Codex。

同一个后端可以扮演所有角色，也可以后续按角色配置不同后端。Agent 输出都是候选产物，Rust 必须验证后才能采纳。

详细角色见 [多智能体角色](agents.md)。

### Execution 层

Execution 层负责确定性执行能力：

- `Workspace`：Git 分支、base commit、diff、提交、回滚、路径校验。
- `Runner`：执行目标项目实验命令、超时控制、stdout/stderr 归档。
- `Metrics`：从日志中解析指标并和历史最佳值比较。
- `Archive`：为每轮实验创建完整档案目录。

Execution 层不调用 LLM，也不解释实验意义。

### Memory 层

Memory 层负责长期记忆和复利资产。

v1 使用 Markdown，不引入数据库。记忆文件包括 `business.md`、`experiments.md`、`decisions.md`、`playbook.md`。

详细规则见 [记忆与复利机制](memory-and-compounding.md)。

### Policy 层

Policy 层定义可信边界和经验升级规则。

只能 Rust 执行：

- Git 提交、回滚和分支操作。
- 允许路径和只读路径校验。
- 指标解析和比较。
- keep、discard、crash 决策。
- `state.toml` 更新。
- 记忆文件和实验档案写入。

Agent 不能用自然语言结论覆盖 Rust 的指标结果，不能跳过日志归档或复盘，不能把多个独立实验混入一轮。

## 核心闭环

1. 加载配置、记忆和 run 状态。
2. 创建单轮实验档案。
3. 调用 Agent 产生假设、计划、代码改动和审查结论。
4. Rust 校验路径和只读文件。
5. Rust 提交候选改动。
6. Runner 执行实验命令并保存完整日志。
7. Metrics 解析指标。
8. Rust 决定 keep、discard 或 crash。
9. AnalystAgent 和 MemoryAgent 生成复盘与经验候选。
10. Rust 写入实验记录、稳定经验和研究手册。
11. 下一轮实验读取这些经验，形成复利。

完整流程见 [实验生命周期](experiment-lifecycle.md)。

