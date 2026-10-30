use std::{error::Error, ffi::OsStr, io, path::PathBuf};

use coldstart_mcp::ColdStartMcp;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("coldstart-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let project_home = parse_project_home(std::env::args_os().skip(1))?;
    let server = ColdStartMcp::new(project_home)?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn parse_project_home(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<PathBuf, io::Error> {
    let mut args = args.into_iter();
    let mut project_home = None;

    while let Some(argument) = args.next() {
        if argument == OsStr::new("--project-home") {
            if project_home.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--project-home may only be provided once",
                ));
            }
            let value = args.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--project-home requires a path",
                )
            })?;
            project_home = Some(PathBuf::from(value));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown argument: {}", argument.to_string_lossy()),
            ));
        }
    }

    project_home.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: coldstart-mcp --project-home <path>",
        )
    })
}
