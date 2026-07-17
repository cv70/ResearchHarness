use std::{fs, path::PathBuf};

use chrono::Utc;

use crate::{
    agents::{AgentRequest, AgentRole, AgentRunner},
    config::Config,
    core::{ExperimentStatus, HarnessError, MetricSnapshot, Result, Run},
    execution::{
        archive::{ArchiveStore, write_log_excerpt},
        metrics::parse_metric,
        runner::{ExperimentCommand, run_command},
        workspace::Workspace,
    },
    memory::store::MemoryStore,
    policy::PathPolicy,
};

#[derive(Debug, Clone)]
pub struct Orchestrator {
    workspace_root: PathBuf,
    config: Config,
}

#[derive(Debug, Clone)]
pub struct RunOnceOutcome {
    pub experiment_id: String,
    pub status: ExperimentStatus,
    pub metric: Option<MetricSnapshot>,
    pub archive_path: PathBuf,
}

struct ExperimentContext<R: AgentRunner> {
    workspace: Workspace,
    memory: MemoryStore,
    archive_store: ArchiveStore,
    run: Run,
    experiment: crate::core::Experiment,
    archive: crate::core::ExperimentArchive,
    base_commit: String,
    allowed_paths: Vec<PathBuf>,
    agent: R,
    state_path: PathBuf,
}

impl Orchestrator {
    pub fn new(workspace_root: impl Into<PathBuf>, config: Config) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            config,
        }
    }

    pub fn init_workspace(root: impl Into<PathBuf>) -> Result<()> {
        let root = root.into();
        Config::write_default(&root)?;
        MemoryStore::new(&root).init()?;
        Ok(())
    }

    pub fn setup_run(&self, tag: &str) -> Result<Run> {
        let workspace = Workspace::new(&self.workspace_root);
        workspace.ensure_git_repo()?;
        let branch = workspace.current_branch()?;
        let run = Run::new(tag, branch);
        let archive = ArchiveStore::new(&self.workspace_root, tag);
        archive.init_run_dirs()?;
        fs::write(archive.state_path(), toml::to_string_pretty(&run)?)?;
        MemoryStore::new(&self.workspace_root).init()?;
        Ok(run)
    }

    pub fn status(&self, tag: &str) -> Result<String> {
        let archive = ArchiveStore::new(&self.workspace_root, tag);
        let state_path = archive.state_path();
        if !state_path.exists() {
            return Ok(format!("run `{tag}` has not been set up"));
        }
        Ok(fs::read_to_string(state_path)?)
    }

    pub fn run_once<R: AgentRunner + Clone>(&self, tag: &str, agent: &R) -> Result<RunOnceOutcome> {
        let mut context = self.prepare_context(tag, agent.clone())?;
        let experiment_id = context.experiment.id.clone();
        let archive_path = context.experiment.archive_path.clone();

        match self.execute_experiment(&mut context) {
            Ok((status, metric)) => {
                self.archive_results(&mut context, status, metric.clone())?;
                Ok(RunOnceOutcome {
                    experiment_id,
                    status,
                    metric,
                    archive_path,
                })
            }
            Err(err) => {
                self.archive_crash(&mut context, err)?;
                Ok(RunOnceOutcome {
                    experiment_id,
                    status: ExperimentStatus::Crashed,
                    metric: None,
                    archive_path,
                })
            }
        }
    }

    fn prepare_context<R: AgentRunner>(&self, tag: &str, agent: R) -> Result<ExperimentContext<R>> {
        let workspace = Workspace::new(&self.workspace_root);
        workspace.ensure_git_repo()?;
        if workspace.has_user_changes()? {
            return Err(HarnessError::Experiment(
                "workspace has uncommitted user changes; commit or stash them before running"
                    .to_string(),
            ));
        }
        let memory = MemoryStore::new(&self.workspace_root);
        memory.init()?;

        let archive_store = ArchiveStore::new(&self.workspace_root, tag);
        archive_store.init_run_dirs()?;
        let state_path = archive_store.state_path();
        let run = if state_path.exists() {
            toml::from_str::<Run>(&fs::read_to_string(&state_path)?)?
        } else {
            Run::new(tag, workspace.current_branch()?)
        };

        let base_commit = workspace.head_commit()?;
        let experiment_index = run.experiment_count + 1;
        let (experiment, archive) =
            archive_store.create_experiment(tag, experiment_index, base_commit.clone())?;

        let allowed_paths = self
            .config
            .workspace
            .modifiable
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        Ok(ExperimentContext {
            workspace,
            memory,
            archive_store,
            run,
            experiment,
            archive,
            base_commit,
            allowed_paths,
            agent,
            state_path,
        })
    }

    fn execute_experiment<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
    ) -> std::result::Result<(ExperimentStatus, Option<MetricSnapshot>), HarnessError> {
        self.execute_agent_planning(context)?;
        self.execute_coding_and_review(context)?;
        self.execute_experiment_command(context)
    }

    fn execute_agent_planning<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
    ) -> Result<()> {
        let snapshot = context.memory.load()?;

        self.call_agent(
            &context.agent,
            AgentRole::Coordinator,
            "生成本轮调度建议。",
            &snapshot.playbook,
            &context.allowed_paths,
        )?;
        let research = self.call_agent(
            &context.agent,
            AgentRole::Research,
            "提出一个可归因的实验假设。",
            &snapshot.experiments,
            &context.allowed_paths,
        )?;
        context.experiment.hypothesis = Some(research.stdout.trim().to_string());
        let plan = self.call_agent(
            &context.agent,
            AgentRole::Planning,
            "将实验假设转成执行计划。",
            &research.stdout,
            &context.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.plan_path, &plan.stdout)?;
        Ok(())
    }

    fn execute_coding_and_review<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
    ) -> Result<()> {
        self.call_agent(
            &context.agent,
            AgentRole::Coding,
            "按 plan.md 修改允许范围内的代码。",
            &fs::read_to_string(&context.archive.plan_path).unwrap_or_default(),
            &context.allowed_paths,
        )?;
        let diff = context.workspace.diff()?;
        ArchiveStore::write_text(&context.archive.diff_path, &diff)?;
        let changed_files = context.workspace.user_changed_files()?;
        let path_policy = PathPolicy::new(
            self.config.workspace.modifiable.clone(),
            self.config.workspace.readonly.clone(),
        );
        path_policy.check_changed_paths(changed_files.iter())?;

        self.call_agent(
            &context.agent,
            AgentRole::Review,
            "审查 diff 是否符合计划。",
            &diff,
            &context.allowed_paths,
        )?;

        context.experiment.status = ExperimentStatus::Reviewed;
        context.archive_store.write_manifest(&context.experiment)?;

        let candidate_commit = if !changed_files.is_empty() {
            let commit = context.workspace.commit_paths(
                &changed_files,
                &format!("experiment {}", context.experiment.id),
            )?;
            Some(commit)
        } else {
            None
        };
        context.experiment.candidate_commit = candidate_commit;
        Ok(())
    }

    fn execute_experiment_command<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
    ) -> Result<(ExperimentStatus, Option<MetricSnapshot>)> {
        let command = ExperimentCommand {
            command: self.config.experiment.command.clone(),
            timeout_seconds: self.config.experiment.timeout_seconds,
            log_path: context.archive.run_log_path.clone(),
        };
        context.experiment.status = ExperimentStatus::Running;
        context.archive_store.write_manifest(&context.experiment)?;

        let command_result = run_command(&self.workspace_root, &command)?;
        let _ = write_log_excerpt(
            &context.archive.run_log_path,
            &context.archive.log_excerpt_path,
            self.config.experiment.max_log_excerpt_lines,
        );

        let previous_best = context.run.best_metric.as_ref().map(|metric| metric.value);
        if command_result.ensure_success().is_err() {
            self.rollback_workspace(context)?;
            context.run.consecutive_crashes += 1;
            return Ok((ExperimentStatus::Crashed, None));
        }

        match parse_metric(
            &self.config.metric,
            &context.archive.run_log_path,
            previous_best,
        ) {
            Ok(snapshot) if snapshot.improved => {
                context.run.best_metric = Some(snapshot.clone());
                context.run.best_commit = context.experiment.candidate_commit.clone();
                context.run.consecutive_crashes = 0;
                context.run.consecutive_regressions = 0;
                context.experiment.metric_snapshot = Some(snapshot.clone());
                Ok((ExperimentStatus::Kept, Some(snapshot)))
            }
            Ok(snapshot) => {
                self.rollback_workspace(context)?;
                context.run.consecutive_regressions += 1;
                context.experiment.metric_snapshot = Some(snapshot.clone());
                Ok((ExperimentStatus::Discarded, Some(snapshot)))
            }
            Err(_) => {
                self.rollback_workspace(context)?;
                context.run.consecutive_crashes += 1;
                Ok((ExperimentStatus::Crashed, None))
            }
        }
    }

    fn rollback_workspace<R: AgentRunner>(&self, context: &mut ExperimentContext<R>) -> Result<()> {
        context.workspace.reset_hard(&context.base_commit)?;
        context.workspace.clean_user_untracked()?;
        Ok(())
    }

    fn archive_results<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
        _status: ExperimentStatus,
        _metric: Option<MetricSnapshot>,
    ) -> Result<()> {
        let analysis = self.call_agent(
            &context.agent,
            AgentRole::Analyst,
            "解释实验结果并生成复盘。",
            &fs::read_to_string(&context.archive.log_excerpt_path).unwrap_or_default(),
            &context.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.analysis_path, &analysis.stdout)?;
        let reflection = self.call_agent(
            &context.agent,
            AgentRole::Memory,
            "将复盘转成记忆候选。",
            &analysis.stdout,
            &context.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.reflection_path, &reflection.stdout)?;
        self.finalize_experiment(context)
    }

    fn archive_crash<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
        err: HarnessError,
    ) -> Result<()> {
        self.rollback_workspace(context)?;
        context.experiment.status = ExperimentStatus::Crashed;
        context.run.consecutive_crashes += 1;
        context.run.experiment_count += 1;

        ArchiveStore::write_text(
            &context.archive.analysis_path,
            format!("Experiment crashed before command execution.\n\nError: {err}\n"),
        )?;
        ArchiveStore::write_text(
            &context.archive.reflection_path,
            "Failure archived. Review the error and diff before retrying.\n",
        )?;
        self.finalize_experiment(context)
    }

    fn finalize_experiment<R: AgentRunner>(
        &self,
        context: &mut ExperimentContext<R>,
    ) -> Result<()> {
        let experiment_record =
            render_experiment_record(&context.experiment, &context.archive.run_log_path);
        context.memory.append_experiment(&experiment_record)?;

        context.experiment.status = ExperimentStatus::Archived;
        context.archive_store.write_manifest(&context.experiment)?;
        fs::write(&context.state_path, toml::to_string_pretty(&context.run)?)?;

        Ok(())
    }

    fn call_agent<R: AgentRunner>(
        &self,
        agent: &R,
        role: AgentRole,
        task: &str,
        context: &str,
        allowed_paths: &[PathBuf],
    ) -> Result<crate::agents::AgentResponse> {
        agent.run(&AgentRequest {
            role,
            working_directory: self.workspace_root.clone(),
            system_prompt: "你是 ResearchHarness 自动实验系统中的一个角色。".to_string(),
            task_prompt: format!("{task}\n\n上下文：\n{context}"),
            allowed_paths: allowed_paths.to_vec(),
            context_files: Vec::new(),
            timeout_seconds: 120,
        })
    }
}

fn render_experiment_record(
    experiment: &crate::core::Experiment,
    log_path: &std::path::Path,
) -> String {
    let metric = experiment
        .metric_snapshot
        .as_ref()
        .map(|m| format!("{}={:.6}", m.name, m.value))
        .unwrap_or_else(|| "unavailable".to_string());
    let commit = experiment
        .candidate_commit
        .as_deref()
        .map(|s| s.chars().take(7).collect::<String>())
        .unwrap_or_else(|| "no-commit".to_string());
    format!(
        "## {} - {} - {}\n\n- Status: {:?}\n- Metric: {}\n- Hypothesis: {}\n- Archive: `{}`\n- Follow-up: review `analysis.md` and `reflection.md`.\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S"),
        experiment.id,
        commit,
        experiment.status,
        metric,
        experiment.hypothesis.as_deref().unwrap_or("unknown"),
        log_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .display(),
    )
}
