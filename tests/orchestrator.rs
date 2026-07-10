use std::{fs, process::Command};

use research_harness::{
    agents::{AgentRequest, AgentResponse, AgentRole, AgentRunner, mock::MockAgentRunner},
    config::Config,
    core::{ExperimentStatus, Result},
    orchestrator::Orchestrator,
};
use tempfile::tempdir;

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture_repo(command: &str, modifiable: &[&str], readonly: &[&str]) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);

    fs::write(dir.path().join("train.py"), "# baseline\n").unwrap();
    let modifiable = modifiable
        .iter()
        .map(|path| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let readonly = readonly
        .iter()
        .map(|path| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        dir.path().join("research.toml"),
        format!(
            r#"[project]
name = "fixture"

[workspace]
modifiable = [{modifiable}]
readonly = [{readonly}]

[experiment]
command = "{command}"
log_file = "run.log"
timeout_seconds = 5
archive_logs = true
max_log_excerpt_lines = 20
max_debug_attempts = 1

[metric]
name = "val_bpb"
regex = "^val_bpb:\\s+([0-9.]+)"
direction = "lower"

[agent]
backend = "mock"
"#
        ),
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "initial"]);
    dir
}

#[test]
fn run_once_with_mock_agent_archives_successful_experiment() {
    let dir = fixture_repo(
        "printf 'val_bpb:          0.900000\\n'",
        &["train.py"],
        &["research.toml"],
    );

    let config = Config::load(dir.path()).unwrap();
    let orchestrator = Orchestrator::new(dir.path(), config);
    orchestrator.setup_run("test").unwrap();
    let outcome = orchestrator.run_once("test", &MockAgentRunner).unwrap();

    assert_eq!(outcome.status, ExperimentStatus::Kept);
    assert!(outcome.archive_path.join("run.log").exists());
    assert!(outcome.archive_path.join("analysis.md").exists());
    assert!(outcome.archive_path.join("reflection.md").exists());
    let experiments =
        fs::read_to_string(dir.path().join(".research-harness/memory/experiments.md")).unwrap();
    assert!(experiments.contains("val_bpb=0.900000"));
}

#[test]
fn run_once_rejects_dirty_user_workspace_before_agents_run() {
    let dir = fixture_repo(
        "printf 'val_bpb:          0.900000\\n'",
        &["train.py"],
        &["research.toml"],
    );
    fs::write(dir.path().join("train.py"), "# user work\n").unwrap();

    let config = Config::load(dir.path()).unwrap();
    let orchestrator = Orchestrator::new(dir.path(), config);
    let err = orchestrator
        .run_once("test", &MockAgentRunner)
        .expect_err("dirty workspace should be rejected");
    assert!(err.to_string().contains("uncommitted user changes"));
    assert_eq!(
        fs::read_to_string(dir.path().join("train.py")).unwrap(),
        "# user work\n"
    );
}

#[derive(Debug)]
struct ReadonlyEditingAgent;

impl AgentRunner for ReadonlyEditingAgent {
    fn run(&self, request: &AgentRequest) -> Result<AgentResponse> {
        if request.role == AgentRole::Coding {
            fs::write(
                request.working_directory.join("research.toml"),
                "# illegal change\n",
            )?;
        }
        Ok(AgentResponse {
            stdout: format!("response for {:?}", request.role),
            stderr: String::new(),
            exit_status: Some(0),
            duration: std::time::Duration::from_millis(1),
            artifact_paths: Vec::new(),
        })
    }
}

#[test]
fn path_policy_failure_is_archived_and_rolled_back() {
    let dir = fixture_repo(
        "printf 'val_bpb:          0.900000\\n'",
        &["train.py"],
        &["research.toml"],
    );
    let original_config = fs::read_to_string(dir.path().join("research.toml")).unwrap();

    let config = Config::load(dir.path()).unwrap();
    let orchestrator = Orchestrator::new(dir.path(), config);
    orchestrator.setup_run("test").unwrap();
    let outcome = orchestrator
        .run_once("test", &ReadonlyEditingAgent)
        .unwrap();

    assert_eq!(outcome.status, ExperimentStatus::Crashed);
    assert_eq!(
        fs::read_to_string(dir.path().join("research.toml")).unwrap(),
        original_config
    );
    assert!(outcome.archive_path.join("analysis.md").exists());
    let experiments =
        fs::read_to_string(dir.path().join(".research-harness/memory/experiments.md")).unwrap();
    assert!(experiments.contains("Status: Crashed"));
}

#[test]
fn run_once_from_different_cwd_writes_log_under_target_root() {
    let dir = fixture_repo(
        "printf 'val_bpb:          0.900000\\n'",
        &["train.py"],
        &["research.toml"],
    );
    let other = tempdir().unwrap();
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(other.path()).unwrap();

    let config = Config::load(dir.path()).unwrap();
    let orchestrator = Orchestrator::new(dir.path(), config);
    orchestrator.setup_run("test").unwrap();
    let outcome = orchestrator.run_once("test", &MockAgentRunner).unwrap();

    std::env::set_current_dir(old_cwd).unwrap();
    assert_eq!(outcome.status, ExperimentStatus::Kept);
    assert!(outcome.archive_path.join("run.log").exists());
    assert!(
        !other
            .path()
            .join(".research-harness/runs/test/experiments/exp-00001/run.log")
            .exists()
    );
}
