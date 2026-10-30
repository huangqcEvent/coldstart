mod error;
mod project_home;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use coldstart_storage::Database;
use project_home::ProjectHome;
use serde_json::json;

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
    Status {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
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
        Command::Status { path, json } => {
            print_status(ProjectHome::at(path), json)?;
        }
    }
    Ok(())
}

fn print_status(home: ProjectHome, as_json: bool) -> Result<(), CliError> {
    let root = home.absolute_root()?;
    let manifest = home.load_manifest()?;
    let project = manifest.to_project(root)?;
    let database_path = home.database_path();
    if !database_path.exists() {
        return Err(CliError::DatabaseMissing(database_path));
    }

    let database = Database::open(&database_path)?;
    let stored = database
        .load_project(&project.id, project.root_path.clone())?
        .ok_or(CliError::ProjectMissingFromDatabase(project.id))?;
    if stored.name != project.name || stored.created_at != project.created_at {
        return Err(CliError::ProjectMetadataMismatch(project.id));
    }
    let event_count = database.event_count(&project.id)?;
    let root_path = project
        .root_path
        .to_str()
        .ok_or_else(|| CliError::NonUtf8Path(project.root_path.clone()))?;
    let created_at = serde_json::to_value(project.created_at)?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "projectId": project.id,
                "name": project.name,
                "rootPath": root_path,
                "createdAt": created_at,
                "eventCount": event_count,
            }))?
        );
    } else {
        println!("Project: {}", project.name);
        println!("ID: {}", project.id);
        println!("Root: {root_path}");
        println!("Created: {}", created_at.as_str().unwrap_or_default());
        println!("Events: {event_count}");
    }

    Ok(())
}
