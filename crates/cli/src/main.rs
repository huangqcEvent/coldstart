mod error;
mod project_home;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use project_home::ProjectHome;

use crate::error::CliError;

#[derive(Debug, Parser)]
#[command(name = "coldstart", version, about = "ColdStart project control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Init { name, path } => {
            let home = ProjectHome::at(path);
            let project = home.initialize(&name)?;
            println!(
                "Initialized project \"{}\" at {}",
                project.name,
                project.root_path.display()
            );
        }
    }
    Ok(())
}
