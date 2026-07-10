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

    pub fn run_once<R: AgentRunner>(&self, tag: &str, agent: &R) -> Result<RunOnceOutcome> {
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
        let mut run = if state_path.exists() {
            toml::from_str::<Run>(&fs::read_to_string(&state_path)?)?
        } else {
            Run::new(tag, workspace.current_branch()?)
        };

        let base_commit = workspace.head_commit()?;
        let experiment_index = run.experiment_count + 1;
        let (mut experiment, archive) =
            archive_store.create_experiment(tag, experiment_index, base_commit.clone())?;

        let snapshot = memory.load()?;
        let allowed_paths = self
            .config
            .workspace
            .modifiable
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        self.call_agent(
            agent,
            AgentRole::Coordinator,
            "生成本轮调度建议。",
            &snapshot.playbook,
            &allowed_paths,
        )?;
        let research = self.call_agent(
            agent,
            AgentRole::Research,
            "提出一个可归因的实验假设。",
            &snapshot.experiments,
            &allowed_paths,
        )?;
        experiment.hypothesis = Some(research.stdout.trim().to_string());
        let plan = self.call_agent(
            agent,
            AgentRole::Planning,
            "将实验假设转成执行计划。",
            &research.stdout,
            &allowed_paths,
        )?;
        ArchiveStore::write_text(&archive.plan_path, &plan.stdout)?;

        if let Err(err) = self.call_agent(
            agent,
            AgentRole::Coding,
            "按 plan.md 修改允许范围内的代码。",
            &plan.stdout,
            &allowed_paths,
        ) {
            return self.archive_crash(
                workspace,
                memory,
                archive_store,
                run,
                experiment,
                archive,
                base_commit,
                err,
            );
        }
        let diff = workspace.diff()?;
        ArchiveStore::write_text(&archive.diff_path, &diff)?;
        let changed_files = workspace.user_changed_files()?;
        let path_policy = PathPolicy::new(
            self.config.workspace.modifiable.clone(),
            self.config.workspace.readonly.clone(),
        );
        if let Err(err) = path_policy.check_changed_paths(changed_files.iter()) {
            return self.archive_crash(
                workspace,
                memory,
                archive_store,
                run,
                experiment,
                archive,
                base_commit,
                err,
            );
        }

        if let Err(err) = self.call_agent(
            agent,
            AgentRole::Review,
            "审查 diff 是否符合计划。",
            &diff,
            &allowed_paths,
        ) {
            return self.archive_crash(
                workspace,
                memory,
                archive_store,
                run,
                experiment,
                archive,
                base_commit,
                err,
            );
        }

        experiment.status = ExperimentStatus::Reviewed;
        archive_store.write_manifest(&experiment)?;

        let candidate_commit = if !changed_files.is_empty() {
            let commit =
                workspace.commit_paths(&changed_files, &format!("experiment {}", experiment.id))?;
            Some(commit)
        } else {
            None
        };
        experiment.candidate_commit = candidate_commit.clone();

        let command = ExperimentCommand {
            command: self.config.experiment.command.clone(),
            timeout_seconds: self.config.experiment.timeout_seconds,
            log_path: archive.run_log_path.clone(),
        };
        experiment.status = ExperimentStatus::Running;
        archive_store.write_manifest(&experiment)?;
        let command_result = run_command(&self.workspace_root, &command)?;
        let _ = write_log_excerpt(
            &archive.run_log_path,
            &archive.log_excerpt_path,
            self.config.experiment.max_log_excerpt_lines,
        );

        let previous_best = run.best_metric.as_ref().map(|metric| metric.value);
        let mut metric = None;
        let status = if command_result.ensure_success().is_err() {
            workspace.reset_hard(&base_commit)?;
            workspace.clean_user_untracked()?;
            run.consecutive_crashes += 1;
            ExperimentStatus::Crashed
        } else {
            match parse_metric(&self.config.metric, &archive.run_log_path, previous_best) {
                Ok(snapshot) if snapshot.improved => {
                    run.best_metric = Some(snapshot.clone());
                    run.best_commit = candidate_commit;
                    run.consecutive_crashes = 0;
                    run.consecutive_regressions = 0;
                    metric = Some(snapshot);
                    ExperimentStatus::Kept
                }
                Ok(snapshot) => {
                    workspace.reset_hard(&base_commit)?;
                    workspace.clean_user_untracked()?;
                    run.consecutive_regressions += 1;
                    metric = Some(snapshot);
                    ExperimentStatus::Discarded
                }
                Err(_) => {
                    workspace.reset_hard(&base_commit)?;
                    workspace.clean_user_untracked()?;
                    run.consecutive_crashes += 1;
                    ExperimentStatus::Crashed
                }
            }
        };
        experiment.status = status;
        experiment.metric_snapshot = metric.clone();

        let analysis = self.call_agent(
            agent,
            AgentRole::Analyst,
            "解释实验结果并生成复盘。",
            &fs::read_to_string(&archive.log_excerpt_path).unwrap_or_default(),
            &allowed_paths,
        )?;
        ArchiveStore::write_text(&archive.analysis_path, &analysis.stdout)?;
        let reflection = self.call_agent(
            agent,
            AgentRole::Memory,
            "将复盘转成记忆候选。",
            &analysis.stdout,
            &allowed_paths,
        )?;
        ArchiveStore::write_text(&archive.reflection_path, &reflection.stdout)?;

        let experiment_record = render_experiment_record(&experiment, &archive.run_log_path);
        memory.append_experiment(&experiment_record)?;

        experiment.status = ExperimentStatus::Archived;
        archive_store.write_manifest(&experiment)?;
        run.experiment_count += 1;
        fs::write(state_path, toml::to_string_pretty(&run)?)?;

        Ok(RunOnceOutcome {
            experiment_id: experiment.id,
            status,
            metric,
            archive_path: experiment.archive_path,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn archive_crash(
        &self,
        workspace: Workspace,
        memory: MemoryStore,
        archive_store: ArchiveStore,
        mut run: Run,
        mut experiment: crate::core::Experiment,
        archive: crate::core::ExperimentArchive,
        base_commit: String,
        err: HarnessError,
    ) -> Result<RunOnceOutcome> {
        workspace.reset_hard(&base_commit)?;
        workspace.clean_user_untracked()?;
        experiment.status = ExperimentStatus::Crashed;
        run.consecutive_crashes += 1;
        run.experiment_count += 1;

        ArchiveStore::write_text(
            &archive.analysis_path,
            format!("Experiment crashed before command execution.\n\nError: {err}\n"),
        )?;
        ArchiveStore::write_text(
            &archive.reflection_path,
            "Failure archived. Review the error and diff before retrying.\n",
        )?;
        let experiment_record = render_experiment_record(&experiment, &archive.run_log_path);
        memory.append_experiment(&experiment_record)?;
        experiment.status = ExperimentStatus::Archived;
        archive_store.write_manifest(&experiment)?;
        fs::write(archive_store.state_path(), toml::to_string_pretty(&run)?)?;

        Ok(RunOnceOutcome {
            experiment_id: experiment.id,
            status: ExperimentStatus::Crashed,
            metric: None,
            archive_path: experiment.archive_path,
        })
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
