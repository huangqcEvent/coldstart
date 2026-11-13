//! ColdStart 命令行初始化与状态检查。
//!
//! CLI 只提供项目初始化、状态读取和必要的目录校验，不承担桌面编排或 MCP 服务；
//! 成功与失败均通过稳定退出码和结构化 JSON 表达。

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use coldstart_app::{AppError, ProjectService};

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

fn run(cli: Cli) -> Result<(), AppError> {
    match cli.command {
        Command::Init { name, path } => {
            let status = ProjectService::new().create_project(&name, path)?;
            println!(
                "Initialized project \"{}\" at {}",
                status.name,
                status.root_path.display()
            );
        }
        Command::Status { path, json } => print_status(path, json)?,
    }
    Ok(())
}

fn print_status(path: PathBuf, as_json: bool) -> Result<(), AppError> {
    let status = ProjectService::new().read_status(path)?;
    let root_path = status
        .root_path
        .to_str()
        .ok_or_else(|| AppError::NonUtf8Path(status.root_path.clone()))?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        let created_at = serde_json::to_value(status.created_at)?;
        println!("Project: {}", status.name);
        println!("ID: {}", status.project_id);
        println!("Root: {root_path}");
        println!("Created: {}", created_at.as_str().unwrap_or_default());
        println!("Events: {}", status.event_count);
    }

    Ok(())
}
