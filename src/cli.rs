use std::{env, path::PathBuf};

use clap::{Parser, Subcommand};

use crate::{
    agents::{cli_runner::CliAgentRunner, mock::MockAgentRunner},
    config::Config,
    core::{HarnessError, Result},
    memory::store::MemoryStore,
    orchestrator::Orchestrator,
};

#[derive(Debug, Parser)]
#[command(name = "research-harness")]
#[command(about = "Rust automation engine for autonomous experiment loops")]
pub struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    Setup {
        #[arg(long)]
        tag: String,
    },
    Run {
        #[arg(long)]
        tag: String,
        #[arg(long)]
        once: bool,
    },
    Status {
        #[arg(long)]
        tag: String,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    AddBusiness { text: String },
    AddExperiment { text: String },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => {
            Orchestrator::init_workspace(&cli.root)?;
            println!("initialized ResearchHarness at {}", cli.root.display());
        }
        Command::Setup { tag } => {
            let config = Config::load(&cli.root)?;
            let orchestrator = Orchestrator::new(&cli.root, config);
            let run = orchestrator.setup_run(&tag)?;
            println!("setup run `{}` on branch `{}`", run.tag, run.branch);
        }
        Command::Run { tag, once } => {
            if !once {
                return Err(HarnessError::InvalidConfig(
                    "v1 supports `run --once`; continuous loop is not implemented yet".to_string(),
                ));
            }
            let config = Config::load(&cli.root)?;
            let backend = config.agent.backend.clone();
            let orchestrator = Orchestrator::new(&cli.root, config);
            let outcome = run_with_backend(&orchestrator, &tag, &backend)?;
            print_outcome(outcome);
        }
        Command::Status { tag } => {
            println!("{}", Orchestrator::status(&cli.root, &tag)?);
        }
        Command::Memory { command } => {
            let root = if cli.root.as_os_str() == "." {
                env::current_dir()?
            } else {
                cli.root.clone()
            };
            let store = MemoryStore::new(root);
            store.init()?;
            match command {
                MemoryCommand::AddBusiness { text } => store.append_business(&text)?,
                MemoryCommand::AddExperiment { text } => store.append_experiment(&text)?,
            }
            println!("memory updated");
        }
    }
    Ok(())
}

fn print_outcome(outcome: crate::orchestrator::RunOnceOutcome) {
    println!("experiment_id: {}", outcome.experiment_id);
    println!("status: {:?}", outcome.status);
    if let Some(metric) = outcome.metric {
        println!("metric: {}={:.6}", metric.name, metric.value);
    } else {
        println!("metric: unavailable");
    }
    println!("archive: {}", outcome.archive_path.display());
}

fn run_with_backend(
    orchestrator: &Orchestrator,
    tag: &str,
    backend: &str,
) -> Result<crate::orchestrator::RunOnceOutcome> {
    if backend == "mock" {
        orchestrator.run_once(tag, &MockAgentRunner)
    } else {
        let runner = CliAgentRunner::new(backend, Vec::new());
        orchestrator.run_once(tag, &runner)
    }
}
