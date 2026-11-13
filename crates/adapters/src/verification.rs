//! 验证探针与命令输出清洗。
//!
//! 负责启动受控子进程、限制超时和输出大小，并移除终端控制序列与常见密钥形态；
//! 清洗后的摘要才允许进入报告、事件和 UI。

use std::{
    collections::HashSet,
    ffi::c_void,
    io::{self, Read},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use coldstart_core::RepositoryVerificationCommand;
use regex::Regex;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
};

const MAX_STREAM_BYTES: usize = 32 * 1024;
const MAX_COMBINED_BYTES: usize = 64 * 1024;
const MAX_ERROR_CHARS: usize = 1_200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ALLOWED_ENVIRONMENT: &[&str] = &[
    "SystemRoot",
    "WINDIR",
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "CommonProgramFiles",
    "ComSpec",
    "HOME",
    "USERNAME",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "PNPM_HOME",
    "NODE_HOME",
    "JAVA_HOME",
    "ANDROID_HOME",
    "ANDROID_SDK_ROOT",
    "GRADLE_USER_HOME",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerificationCommandExecution {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output_summary: String,
    pub truncated: bool,
    pub timed_out: bool,
    pub launch_error: Option<String>,
    pub infrastructure_error: Option<String>,
}

pub trait VerificationCommandExecutor: Send + Sync {
    fn execute(
        &self,
        command: &RepositoryVerificationCommand,
        repository: &Path,
    ) -> VerificationCommandExecution;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsVerificationExecutor;

impl VerificationCommandExecutor for WindowsVerificationExecutor {
    fn execute(
        &self,
        command: &RepositoryVerificationCommand,
        repository: &Path,
    ) -> VerificationCommandExecution {
        execute_windows_command(command, repository)
    }
}

fn execute_windows_command(
    command: &RepositoryVerificationCommand,
    repository: &Path,
) -> VerificationCommandExecution {
    let started = Instant::now();
    let job = match JobHandle::create() {
        Ok(job) => job,
        Err(error) => {
            return infrastructure_failure(
                started,
                format!("could not create verification job object: {error}"),
            );
        }
    };

    let mut process = Command::new(&command.executable);
    process
        .current_dir(repository)
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .creation_flags(CREATE_NO_WINDOW);
    apply_allowlisted_environment(&mut process);

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return VerificationCommandExecution {
                duration_ms: elapsed_ms(started),
                launch_error: Some(bounded_error(format!(
                    "could not start verification executable: {error}"
                ))),
                ..VerificationCommandExecution::default()
            };
        }
    };

    if !job.assign(child.as_raw_handle() as HANDLE) {
        let error = io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        return infrastructure_failure(
            started,
            format!("could not assign verification process to its job object: {error}"),
        );
    }

    let stdout = child
        .stdout
        .take()
        .expect("verification stdout must be piped");
    let stderr = child
        .stderr
        .take()
        .expect("verification stderr must be piped");
    let stdout_reader = thread::spawn(move || read_limited(stdout, MAX_STREAM_BYTES));
    let stderr_reader = thread::spawn(move || read_limited(stderr, MAX_STREAM_BYTES));

    let timeout = Duration::from_secs(u64::from(command.timeout_seconds));
    let deadline = started + timeout;
    let mut timed_out = false;
    let mut infrastructure_error = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                if !job.terminate() {
                    let _ = child.kill();
                }
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = job.terminate();
                let _ = child.kill();
                let _ = child.wait();
                infrastructure_error = Some(bounded_error(format!(
                    "could not monitor verification process: {error}"
                )));
                break None;
            }
        }
    };

    // Closing a job with KILL_ON_JOB_CLOSE also cleans up background descendants
    // that would otherwise keep inherited output pipes open.
    drop(job);
    let stdout = join_reader(stdout_reader, &mut infrastructure_error);
    let stderr = join_reader(stderr_reader, &mut infrastructure_error);

    let stdout_text = sanitize_verification_output(&stdout.text);
    let stderr_text = sanitize_verification_output(&stderr.text);
    let mut combined = stdout_text;
    if !stderr_text.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr_text);
    }
    let (mut output_summary, combined_truncated) = tail_by_bytes(&combined, MAX_COMBINED_BYTES);
    let truncated = stdout.truncated || stderr.truncated || combined_truncated;
    if truncated {
        output_summary.push_str("\n[output truncated]");
    }

    VerificationCommandExecution {
        exit_code: status.and_then(|status| status.code()),
        duration_ms: elapsed_ms(started),
        output_summary,
        truncated,
        timed_out,
        launch_error: None,
        infrastructure_error,
    }
}

fn infrastructure_failure(started: Instant, message: String) -> VerificationCommandExecution {
    VerificationCommandExecution {
        duration_ms: elapsed_ms(started),
        infrastructure_error: Some(bounded_error(message)),
        ..VerificationCommandExecution::default()
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn bounded_error(message: String) -> String {
    sanitize_verification_output(&message)
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

fn apply_allowlisted_environment(command: &mut Command) {
    let allowed = ALLOWED_ENVIRONMENT
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for (name, value) in std::env::vars_os() {
        if name
            .to_str()
            .is_some_and(|name| allowed.contains(&name.to_ascii_lowercase()))
        {
            command.env(name, value);
        }
    }
}

#[derive(Default)]
struct CollectedOutput {
    text: String,
    truncated: bool,
}

fn read_limited(mut reader: impl Read, limit: usize) -> CollectedOutput {
    let mut retained = Vec::with_capacity(limit.min(8 * 1024));
    let mut truncated = false;
    let mut buffer = [0u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                retained.extend_from_slice(&buffer[..count]);
                if retained.len() > limit {
                    let excess = retained.len() - limit;
                    retained.drain(..excess);
                    truncated = true;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    CollectedOutput {
        text: String::from_utf8_lossy(&retained).into_owned(),
        truncated,
    }
}

fn join_reader(
    handle: thread::JoinHandle<CollectedOutput>,
    infrastructure_error: &mut Option<String>,
) -> CollectedOutput {
    match handle.join() {
        Ok(output) => output,
        Err(_) => {
            *infrastructure_error =
                Some("verification output reader terminated unexpectedly".to_owned());
            CollectedOutput::default()
        }
    }
}

fn tail_by_bytes(value: &str, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value.to_owned(), false);
    }
    let mut start = value.len() - limit;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    (value[start..].to_owned(), true)
}

pub fn sanitize_verification_output(value: &str) -> String {
    let without_controls = strip_terminal_controls(value);
    let mut redacted = without_controls;
    for (pattern, replacement) in redaction_patterns() {
        redacted = pattern.replace_all(&redacted, *replacement).into_owned();
    }
    redacted
}

fn strip_terminal_controls(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.next() {
                Some('[') => {
                    for next in characters.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = characters.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && characters.peek() == Some(&'\\') {
                            characters.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            }
            continue;
        }
        if character == '\n' || character == '\t' || !character.is_control() {
            output.push(character);
        }
    }
    output
}

fn redaction_patterns() -> &'static Vec<(Regex, &'static str)> {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            (
                r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{8,}",
                "Bearer ***",
            ),
            (
                r"(?i)\b(token|secret|password|passwd|api[_-]?key|access[_-]?key|private[_-]?key)\b\s*[:=]\s*(?:\x22[^\x22]*\x22|'[^']*'|[^\s,;]+)",
                "$1=***",
            ),
            (r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b", "***"),
            (r"\bgithub_pat_[A-Za-z0-9_]{20,}\b", "***"),
            (r"\bsk-[A-Za-z0-9_-]{16,}\b", "***"),
            (r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b", "***"),
            (r"\bAKIA[0-9A-Z]{16}\b", "***"),
            (
                r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
                "***",
            ),
            (
                r"(?s)-----BEGIN [^-]+PRIVATE KEY-----.*?-----END [^-]+PRIVATE KEY-----",
                "***",
            ),
            (r"(?i)(https?://)[^/\s:@]+:[^/\s@]+@", "$1***@"),
        ]
        .into_iter()
        .map(|(pattern, replacement)| {
            (
                Regex::new(pattern).expect("verification redaction regex must compile"),
                replacement,
            )
        })
        .collect()
    })
}

struct JobHandle(HANDLE);

impl JobHandle {
    fn create() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&information as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    fn assign(&self, process: HANDLE) -> bool {
        unsafe { AssignProcessToJobObject(self.0, process) != 0 }
    }

    fn terminate(&self) -> bool {
        unsafe { TerminateJobObject(self.0, 1) != 0 }
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_removes_terminal_sequences_and_redacts_common_secret_shapes() {
        let value = concat!(
            "\u{1b}[31mTOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789\u{1b}[0m\n",
            "Authorization: Bearer abcdefghijklmnop123456\n",
            "password=\x22a private value\x22\n",
            "\u{1b}]0;hidden title\u{7}visible\n"
        );
        let sanitized = sanitize_verification_output(value);
        assert!(sanitized.contains("visible"));
        assert!(sanitized.contains("***"));
        assert!(!sanitized.contains("ghp_"));
        assert!(!sanitized.contains("abcdefghijklmnop123456"));
        assert!(!sanitized.contains("a private value"));
        assert!(!sanitized.contains("hidden title"));
        assert!(!sanitized.contains('\u{1b}'));
    }

    #[test]
    fn tail_truncation_respects_utf8_boundaries_and_keeps_the_end() {
        let value = format!("{}尾部标记", "前".repeat(32));
        let (tail, truncated) = tail_by_bytes(&value, 12);
        assert!(truncated);
        assert!(tail.ends_with("尾部标记"));
        assert!(tail.len() <= 12);
    }
}
