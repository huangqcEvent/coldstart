use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const EXECUTABLE_EXTENSION: &str = "exe";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryVerificationCommand {
    pub name: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub timeout_seconds: u32,
    pub position: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerificationCommandError {
    #[error("verification command name cannot be empty")]
    EmptyName,
    #[error("verification command name exceeds 80 characters")]
    NameTooLong,
    #[error("verification command name has surrounding whitespace")]
    NameHasSurroundingWhitespace,
    #[error("verification command name is duplicated: {0}")]
    DuplicateName(String),
    #[error("verification command executable path must be absolute: {0}")]
    ExecutableNotAbsolute(PathBuf),
    #[error("verification command executable must use the .exe extension: {0}")]
    ExecutableNotExe(PathBuf),
    #[error("verification command executable does not exist: {0}")]
    ExecutableNotFound(PathBuf),
    #[error("verification command executable is not a regular file: {0}")]
    ExecutableNotFile(PathBuf),
    #[error("verification command executable cannot be normalized: {path}: {message}")]
    ExecutableUnreadable { path: PathBuf, message: String },
    #[error("a repository cannot have more than 8 verification commands")]
    TooManyCommands,
    #[error("verification snapshot requires at least one command")]
    NoCommands,
    #[error("verification command {command} timeout must be between 10 and 900 seconds")]
    TimeoutOutOfRange { command: String },
    #[error("verification command timeouts exceed the 3600 second repository budget")]
    TotalTimeoutTooHigh,
    #[error("verification command {command} argument {index} exceeds 2048 characters")]
    ArgumentTooLong { command: String, index: usize },
    #[error("verification command {command} arguments exceed 16 KiB")]
    ArgumentsTooLong { command: String },
    #[error("verification command {command} argument contains a NUL character")]
    ArgumentContainsNul { command: String },
    #[error("verification command positions must be unique and contiguous from zero")]
    InvalidPositions,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    #[default]
    NotRequired,
    Running,
    Passed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationFailureKind {
    NonZeroExit,
    LaunchFailed,
    Timeout,
    GitInvariant,
    Infrastructure,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationCommandResult {
    pub name: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output_summary: String,
    pub truncated: bool,
    pub failure_kind: Option<VerificationFailureKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationResult {
    pub snapshot_hash: String,
    pub status: VerificationStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_duration_ms: Option<u64>,
    pub failure_kind: Option<VerificationFailureKind>,
    pub failure_command: Option<String>,
    pub commands: Vec<VerificationCommandResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationSnapshot {
    pub schema_version: u32,
    pub repository_key: String,
    pub commands: Vec<RepositoryVerificationCommand>,
    pub total_timeout_seconds: u32,
}

impl VerificationSnapshot {
    pub fn new(
        repository_key: impl Into<String>,
        commands: Vec<RepositoryVerificationCommand>,
    ) -> Result<Self, VerificationCommandError> {
        if commands.is_empty() {
            return Err(VerificationCommandError::NoCommands);
        }
        let commands = validate_and_sort_commands(commands)?;
        let total_timeout_seconds = commands
            .iter()
            .map(|command| command.timeout_seconds)
            .sum::<u32>();
        Ok(Self {
            schema_version: 1,
            repository_key: repository_key.into(),
            commands,
            total_timeout_seconds,
        })
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn content_hash(&self) -> String {
        crate::plan::sha256_hex(
            &self
                .canonical_json()
                .expect("verification snapshot must serialize"),
        )
    }
}

pub fn normalize_verification_commands(
    commands: Vec<RepositoryVerificationCommand>,
) -> Result<Vec<RepositoryVerificationCommand>, VerificationCommandError> {
    let mut normalized = validate_and_sort_commands(commands)?;
    for command in &mut normalized {
        command.executable = normalize_executable(&command.executable)?;
    }
    Ok(normalized)
}

fn validate_and_sort_commands(
    commands: Vec<RepositoryVerificationCommand>,
) -> Result<Vec<RepositoryVerificationCommand>, VerificationCommandError> {
    if commands.len() > 8 {
        return Err(VerificationCommandError::TooManyCommands);
    }
    let mut positions = commands
        .iter()
        .map(|command| command.position)
        .collect::<Vec<_>>();
    positions.sort_unstable();
    if positions != (0..commands.len() as u32).collect::<Vec<_>>() {
        return Err(VerificationCommandError::InvalidPositions);
    }

    let mut names = std::collections::BTreeSet::new();
    let mut total_timeout = 0u32;
    for command in &commands {
        if command.name.trim().is_empty() {
            return Err(VerificationCommandError::EmptyName);
        }
        if command.name.chars().count() > 80 {
            return Err(VerificationCommandError::NameTooLong);
        }
        if command.name.trim() != command.name {
            return Err(VerificationCommandError::NameHasSurroundingWhitespace);
        }
        let folded = command.name.to_lowercase();
        if !names.insert(folded) {
            return Err(VerificationCommandError::DuplicateName(
                command.name.clone(),
            ));
        }
        if !(10..=900).contains(&command.timeout_seconds) {
            return Err(VerificationCommandError::TimeoutOutOfRange {
                command: command.name.clone(),
            });
        }
        total_timeout = total_timeout.saturating_add(command.timeout_seconds);
        let mut argument_chars = 0usize;
        for (index, argument) in command.args.iter().enumerate() {
            let length = argument.chars().count();
            if length > 2048 {
                return Err(VerificationCommandError::ArgumentTooLong {
                    command: command.name.clone(),
                    index,
                });
            }
            if argument.contains('\0') {
                return Err(VerificationCommandError::ArgumentContainsNul {
                    command: command.name.clone(),
                });
            }
            argument_chars = argument_chars.saturating_add(length);
        }
        if argument_chars > 16 * 1024 {
            return Err(VerificationCommandError::ArgumentsTooLong {
                command: command.name.clone(),
            });
        }
    }
    if total_timeout > 3600 {
        return Err(VerificationCommandError::TotalTimeoutTooHigh);
    }

    let mut commands = commands;
    commands.sort_by_key(|command| command.position);
    Ok(commands)
}

fn normalize_executable(path: &PathBuf) -> Result<PathBuf, VerificationCommandError> {
    if !path.is_absolute() {
        return Err(VerificationCommandError::ExecutableNotAbsolute(
            path.clone(),
        ));
    }
    let canonical = dunce::canonicalize(path).map_err(|error| {
        VerificationCommandError::ExecutableUnreadable {
            path: path.clone(),
            message: error.to_string(),
        }
    })?;
    if !canonical.is_file() {
        return Err(VerificationCommandError::ExecutableNotFile(canonical));
    }
    if !canonical
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXECUTABLE_EXTENSION))
    {
        return Err(VerificationCommandError::ExecutableNotExe(canonical));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_command() -> RepositoryVerificationCommand {
        RepositoryVerificationCommand {
            name: "cargo-test".to_owned(),
            executable: std::env::current_exe().expect("test exe"),
            args: vec!["test".to_owned(), "--workspace".to_owned()],
            timeout_seconds: 600,
            position: 0,
        }
    }

    #[test]
    fn validates_command_names_paths_arguments_timeouts_and_totals() {
        let valid = valid_command();
        assert_eq!(
            normalize_verification_commands(vec![valid.clone()]).expect("valid"),
            vec![valid.clone()]
        );

        let mut duplicate = valid.clone();
        duplicate.position = 1;
        assert!(matches!(
            normalize_verification_commands(vec![valid.clone(), duplicate]),
            Err(VerificationCommandError::DuplicateName(name)) if name == "cargo-test"
        ));
        assert!(matches!(
            normalize_verification_commands(vec![RepositoryVerificationCommand {
                timeout_seconds: 9,
                ..valid.clone()
            }]),
            Err(VerificationCommandError::TimeoutOutOfRange { .. })
        ));
        assert!(matches!(
            normalize_verification_commands(vec![RepositoryVerificationCommand {
                executable: "cargo.exe".into(),
                ..valid.clone()
            }]),
            Err(VerificationCommandError::ExecutableNotAbsolute(_))
        ));
        assert!(matches!(
            normalize_verification_commands(vec![RepositoryVerificationCommand {
                args: vec!["x".repeat(2049)],
                ..valid
            }]),
            Err(VerificationCommandError::ArgumentTooLong { .. })
        ));
    }

    #[test]
    fn snapshot_hash_is_stable_and_covers_command_order() {
        let exe = std::env::current_exe().expect("test exe");
        let one = RepositoryVerificationCommand {
            name: "a".into(),
            executable: exe.clone(),
            args: vec![],
            timeout_seconds: 10,
            position: 0,
        };
        let two = RepositoryVerificationCommand {
            name: "b".into(),
            executable: exe,
            args: vec![],
            timeout_seconds: 20,
            position: 1,
        };
        let first =
            VerificationSnapshot::new("server", vec![one.clone(), two.clone()]).expect("snapshot");
        let same =
            VerificationSnapshot::new("server", vec![one.clone(), two.clone()]).expect("snapshot");
        let reversed = VerificationSnapshot::new(
            "server",
            vec![
                RepositoryVerificationCommand { position: 0, ..two },
                RepositoryVerificationCommand { position: 1, ..one },
            ],
        )
        .expect("snapshot");
        assert_eq!(first.content_hash(), same.content_hash());
        assert_ne!(first.content_hash(), reversed.content_hash());
        assert_eq!(first.total_timeout_seconds, 30);
    }
}
