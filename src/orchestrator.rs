use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;

use crate::{
    agents::{AgentRequest, AgentRole, AgentRunner},
    config::Config,
    core::{ExperimentStatus, HarnessError, MetricSnapshot, Result, Run},
    execution::{
        archive::{ArchiveStore, read_log_excerpt},
        metrics::parse_metric_with_regex,
        runner::{ExperimentCommand, run_command},
        workspace::Workspace,
    },
    memory::store::MemoryStore,
    policy::PathPolicy,
};

const AGENT_SYSTEM_PROMPT: &str = "你是 ResearchHarness 自动实验系统中的一个角色。";

#[derive(Debug, Clone)]
pub struct Orchestrator {
    workspace_root: PathBuf,
    config: Config,
    allowed_paths: Arc<[PathBuf]>,
    path_policy: PathPolicy,
    metric_regex: regex::Regex,
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
    agent: R,
    state_path: PathBuf,
    plan: String,
    log_excerpt: String,
}

impl Orchestrator {
    pub fn new(workspace_root: impl Into<PathBuf>, mut config: Config) -> Self {
        let modifiable = std::mem::take(&mut config.workspace.modifiable);
        let allowed_paths: Arc<[PathBuf]> = modifiable.iter().map(PathBuf::from).collect();
        let path_policy = PathPolicy::new(modifiable, config.workspace.readonly.clone());
        let metric_regex = config
            .metric
            .compiled_regex()
            .expect("regex validated in Config::validate");
        Self {
            workspace_root: workspace_root.into(),
            config,
            allowed_paths,
            path_policy,
            metric_regex,
        }
    }

    pub fn init_workspace(root: impl Into<PathBuf>) -> Result<()> {
        let root = root.into();
        Config::write_default(&root)?;
        MemoryStore::new(&root).init()?;
        Ok(())
    }

    pub fn setup_run(&self, tag: &str) -> Result<Run> {
        let (workspace, _memory, archive) = self.ensure_environment(tag)?;
        let branch = workspace.current_branch()?;
        let run = Run::new(tag, branch);
        fs::write(archive.state_path(), toml::to_string_pretty(&run)?)?;
        Ok(run)
    }

    pub fn status(workspace_root: impl AsRef<Path>, tag: &str) -> Result<String> {
        let archive = ArchiveStore::new(workspace_root, tag);
        let state_path = archive.state_path();
        match fs::read_to_string(&state_path) {
            Ok(content) => Ok(content),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let mut msg = String::with_capacity(tag.len() + 32);
                write!(msg, "run `{tag}` has not been set up").unwrap();
                Ok(msg)
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn run_once<R: AgentRunner>(&self, tag: &str, agent: R) -> Result<RunOnceOutcome> {
        let mut context = self.prepare_context(tag, agent)?;
        let experiment_id = context.experiment.id.clone();
        let archive_path = context.experiment.archive_path.clone();

        match self.execute_experiment(&mut context) {
            Ok((status, metric)) => {
                self.archive_results(&mut context)?;
                Ok(RunOnceOutcome {
                    experiment_id,
                    status,
                    metric,
                    archive_path,
                })
            }
            Err(err) => {
                Self::archive_crash(&mut context, &err)?;
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
        let (workspace, memory, archive_store) = self.ensure_environment(tag)?;
        if workspace.has_user_changes()? {
            return Err(HarnessError::Experiment(
                "workspace has uncommitted user changes; commit or stash them before running"
                    .to_string(),
            ));
        }

        let state_path = archive_store.state_path();
        let run = match fs::read_to_string(&state_path) {
            Ok(content) => toml::from_str::<Run>(&content)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Run::new(tag, workspace.current_branch()?)
            }
            Err(err) => return Err(err.into()),
        };

        let base_commit = workspace.head_commit()?;
        let experiment_index = run.experiment_count + 1;
        let (experiment, archive) =
            archive_store.create_experiment(tag, experiment_index, base_commit.clone())?;

        Ok(ExperimentContext {
            workspace,
            memory,
            archive_store,
            run,
            experiment,
            archive,
            base_commit,
            agent,
            state_path,
            plan: String::new(),
            log_excerpt: String::new(),
        })
    }

    fn ensure_environment(&self, tag: &str) -> Result<(Workspace, MemoryStore, ArchiveStore)> {
        let workspace = Workspace::new(&self.workspace_root);
        workspace.ensure_git_repo()?;
        let archive_store = ArchiveStore::new(&self.workspace_root, tag);
        archive_store.init_run_dirs()?;
        let memory = MemoryStore::new(&self.workspace_root);
        memory.init()?;
        Ok((workspace, memory, archive_store))
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
            &self.allowed_paths,
        )?;
        let research = self.call_agent(
            &context.agent,
            AgentRole::Research,
            "提出一个可归因的实验假设。",
            &snapshot.experiments,
            &self.allowed_paths,
        )?;
        context.experiment.hypothesis = Some(research.stdout.trim().to_string());
        let plan = self.call_agent(
            &context.agent,
            AgentRole::Planning,
            "将实验假设转成执行计划。",
            &research.stdout,
            &self.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.plan_path, &plan.stdout)?;
        context.plan = plan.stdout;
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
            &context.plan,
            &self.allowed_paths,
        )?;
        let diff = context.workspace.diff()?;
        ArchiveStore::write_text(&context.archive.diff_path, &diff)?;
        let changed_files = context.workspace.user_changed_files()?;
        self.path_policy.check_changed_paths(changed_files.iter())?;

        self.call_agent(
            &context.agent,
            AgentRole::Review,
            "审查 diff 是否符合计划。",
            &diff,
            &self.allowed_paths,
        )?;

        context.experiment.status = ExperimentStatus::Reviewed;
        context
            .archive_store
            .write_manifest(&context.archive.manifest_path, &context.experiment)?;

        let candidate_commit = if changed_files.is_empty() {
            None
        } else {
            let mut message = String::with_capacity(20 + context.experiment.id.len());
            write!(message, "experiment {}", context.experiment.id).unwrap();
            let commit = context.workspace.commit_paths(&changed_files, &message)?;
            Some(commit)
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
        context
            .archive_store
            .write_manifest(&context.archive.manifest_path, &context.experiment)?;

        let command_result = run_command(&self.workspace_root, &command)?;
        let max_lines = self.config.experiment.max_log_excerpt_lines;
        let log_excerpt =
            read_log_excerpt(&context.archive.run_log_path, max_lines).unwrap_or_default();
        context.log_excerpt = log_excerpt;
        let _ = ArchiveStore::write_text(&context.archive.log_excerpt_path, &context.log_excerpt);

        let previous_best = context.run.best_metric.as_ref().map(|metric| metric.value);
        if command_result.ensure_success().is_err() {
            Self::rollback_workspace(context)?;
            context.run.consecutive_crashes += 1;
            return Ok((ExperimentStatus::Crashed, None));
        }

        let log_content = fs::read_to_string(&context.archive.run_log_path).unwrap_or_default();
        match parse_metric_with_regex(
            &self.metric_regex,
            &self.config.metric,
            &log_content,
            &context.archive.run_log_path,
            previous_best,
        ) {
            Ok(snapshot) => {
                let improved = snapshot.improved;
                context.experiment.metric_snapshot = Some(snapshot);
                if improved {
                    let snapshot = context.experiment.metric_snapshot.clone().unwrap();
                    context.run.best_metric = Some(snapshot.clone());
                    context.run.best_commit = context.experiment.candidate_commit.clone();
                    context.run.consecutive_crashes = 0;
                    context.run.consecutive_regressions = 0;
                    Ok((ExperimentStatus::Kept, Some(snapshot)))
                } else {
                    Self::rollback_workspace(context)?;
                    context.run.consecutive_regressions += 1;
                    Ok((
                        ExperimentStatus::Discarded,
                        context.experiment.metric_snapshot.take(),
                    ))
                }
            }
            Err(err) => {
                let mut analysis =
                    String::with_capacity(80 + err.to_string().len() + context.log_excerpt.len());
                write!(
                    analysis,
                    "Metric parsing failed; experiment treated as crashed.\n\nError: {err}\n\nLog excerpt:\n{}\n",
                    context.log_excerpt
                )
                .unwrap();
                let _ = ArchiveStore::write_text(&context.archive.analysis_path, analysis);
                Self::rollback_workspace(context)?;
                context.run.consecutive_crashes += 1;
                Ok((ExperimentStatus::Crashed, None))
            }
        }
    }

    fn rollback_workspace<R: AgentRunner>(context: &mut ExperimentContext<R>) -> Result<()> {
        context.workspace.reset_hard(&context.base_commit)?;
        context.workspace.clean_user_untracked()?;
        Ok(())
    }

    fn archive_results<R: AgentRunner>(&self, context: &mut ExperimentContext<R>) -> Result<()> {
        let analysis = self.call_agent(
            &context.agent,
            AgentRole::Analyst,
            "解释实验结果并生成复盘。",
            &context.log_excerpt,
            &self.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.analysis_path, &analysis.stdout)?;
        let reflection = self.call_agent(
            &context.agent,
            AgentRole::Memory,
            "将复盘转成记忆候选。",
            &analysis.stdout,
            &self.allowed_paths,
        )?;
        ArchiveStore::write_text(&context.archive.reflection_path, &reflection.stdout)?;
        Self::finalize_experiment(context)
    }

    fn archive_crash<R: AgentRunner>(
        context: &mut ExperimentContext<R>,
        err: &HarnessError,
    ) -> Result<()> {
        Self::rollback_workspace(context)?;
        context.experiment.status = ExperimentStatus::Crashed;
        context.run.consecutive_crashes += 1;

        let mut analysis = String::with_capacity(
            40 + "Experiment crashed before command execution.\n\nError: \n".len()
                + err.to_string().len(),
        );
        write!(
            analysis,
            "Experiment crashed before command execution.\n\nError: {err}\n"
        )
        .unwrap();
        ArchiveStore::write_text(&context.archive.analysis_path, analysis)?;
        ArchiveStore::write_text(
            &context.archive.reflection_path,
            "Failure archived. Review the error and diff before retrying.\n",
        )?;
        Self::finalize_experiment(context)
    }

    fn finalize_experiment<R: AgentRunner>(context: &mut ExperimentContext<R>) -> Result<()> {
        let experiment_record =
            render_experiment_record(&context.experiment, &context.archive.run_log_path);
        context.memory.append_experiment(&experiment_record)?;

        context.experiment.status = ExperimentStatus::Archived;
        context.run.experiment_count += 1;
        context
            .archive_store
            .write_manifest(&context.archive.manifest_path, &context.experiment)?;
        fs::write(&context.state_path, toml::to_string_pretty(&context.run)?)?;

        Ok(())
    }

    fn call_agent<R: AgentRunner>(
        &self,
        agent: &R,
        role: AgentRole,
        task: &str,
        context: &str,
        allowed_paths: &Arc<[PathBuf]>,
    ) -> Result<crate::agents::AgentResponse> {
        let mut task_prompt = String::with_capacity(task.len() + context.len() + 16);
        write!(task_prompt, "{task}\n\n上下文：\n{context}").unwrap();
        agent.run(&AgentRequest {
            role,
            working_directory: self.workspace_root.clone(),
            system_prompt: std::borrow::Cow::Borrowed(AGENT_SYSTEM_PROMPT),
            task_prompt,
            allowed_paths: Arc::clone(allowed_paths),
            context_files: Vec::new(),
            timeout_seconds: 120,
        })
    }
}

fn render_experiment_record(
    experiment: &crate::core::Experiment,
    log_path: &std::path::Path,
) -> String {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
    let archive_path = log_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .display();
    let hypothesis = experiment.hypothesis.as_deref().unwrap_or("unknown");
    let commit_short = experiment
        .candidate_commit
        .as_deref()
        .map(|c| c.get(..7).unwrap_or(c));

    let mut out = String::with_capacity(256);
    write!(out, "## {timestamp} - {} - ", experiment.id).unwrap();
    match commit_short {
        Some(short) => writeln!(out, "{short}\n\n- Status: {:?}", experiment.status),
        None => writeln!(out, "no-commit\n\n- Status: {:?}", experiment.status),
    }
    .unwrap();
    match &experiment.metric_snapshot {
        Some(m) => write!(out, "- Metric: {}={:.6}", m.name, m.value),
        None => out.write_str("- Metric: unavailable"),
    }
    .unwrap();
    writeln!(
        out,
        "\n- Hypothesis: {hypothesis}\n- Archive: `{archive_path}`\n- Follow-up: review `analysis.md` and `reflection.md`.\n"
    )
    .unwrap();
    out
}
