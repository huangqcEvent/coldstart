use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use coldstart_adapters::{VerificationCommandExecutor, WindowsVerificationExecutor};
use coldstart_core::RepositoryVerificationCommand;
use windows_sys::Win32::{
    Foundation::{CloseHandle, STILL_ACTIVE},
    System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

fn probe_command(
    mode: &str,
    args: Vec<String>,
    timeout_seconds: u32,
) -> RepositoryVerificationCommand {
    let mut command_args = vec![mode.to_owned()];
    command_args.extend(args);
    RepositoryVerificationCommand {
        name: format!("probe-{mode}"),
        executable: PathBuf::from(env!("CARGO_BIN_EXE_coldstart-verification-probe")),
        args: command_args,
        timeout_seconds,
        position: 0,
    }
}

fn wait_for_pid(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(value) = fs::read_to_string(path)
            && let Ok(pid) = value.trim().parse()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "probe child pid file did not appear"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_until_stopped(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while process_is_alive(pid) {
        assert!(
            Instant::now() < deadline,
            "probe child {pid} is still alive"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn process_is_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0u32;
        let succeeded = GetExitCodeProcess(handle, &mut exit_code) != 0;
        CloseHandle(handle);
        succeeded && exit_code == STILL_ACTIVE as u32
    }
}

struct EnvironmentGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvironmentGuard {
    fn set(values: &[(&'static str, &'static str)]) -> Self {
        let lock = ENVIRONMENT_LOCK.lock().expect("environment lock");
        let saved = values
            .iter()
            .map(|(name, value)| {
                let previous = std::env::var_os(name);
                unsafe { std::env::set_var(name, value) };
                (*name, previous)
            })
            .collect();
        Self { _lock: lock, saved }
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        for (name, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

#[test]
fn executes_a_probe_and_captures_both_streams() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let result = WindowsVerificationExecutor
        .execute(&probe_command("success", Vec::new(), 10), directory.path());
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out);
    assert!(result.launch_error.is_none());
    assert!(result.infrastructure_error.is_none());
    assert!(result.output_summary.contains("stdout-ok"));
    assert!(result.output_summary.contains("stderr-ok"));
}

#[test]
fn preserves_non_zero_exit_codes_and_failure_output() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let result = WindowsVerificationExecutor
        .execute(&probe_command("fail", Vec::new(), 10), directory.path());
    assert_eq!(result.exit_code, Some(7));
    assert!(!result.timed_out);
    assert!(result.output_summary.contains("probe-failure"));
}

#[test]
fn timeout_terminates_the_child_tree() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let pid_file = directory.path().join("child.pid");
    let command = probe_command(
        "spawn-child",
        vec![pid_file.to_string_lossy().into_owned()],
        1,
    );
    let result = WindowsVerificationExecutor.execute(&command, directory.path());
    assert!(result.timed_out);
    assert!(result.exit_code.is_some());
    wait_until_stopped(wait_for_pid(&pid_file));
}

#[test]
fn bounds_each_stream_and_the_combined_tail() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let result = WindowsVerificationExecutor.execute(
        &probe_command("large-output", Vec::new(), 10),
        directory.path(),
    );
    assert_eq!(result.exit_code, Some(0));
    assert!(result.truncated);
    assert!(result.output_summary.contains("STDOUT-END"));
    assert!(result.output_summary.contains("STDERR-END"));
    assert!(
        result.output_summary.len() <= 64 * 1024 + 256,
        "combined output exceeded its bound: {} bytes",
        result.output_summary.len()
    );
}

#[test]
fn strips_terminal_controls_and_redacts_secret_shaped_output() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let result = WindowsVerificationExecutor.execute(
        &probe_command("secret-output", Vec::new(), 10),
        directory.path(),
    );
    assert_eq!(result.exit_code, Some(0));
    assert!(result.output_summary.contains("safe-marker"));
    assert!(result.output_summary.contains("***"));
    assert!(!result.output_summary.contains('\u{1b}'));
    assert!(
        !result
            .output_summary
            .contains("ghp_probeSecretValue1234567890")
    );
    assert!(
        !result
            .output_summary
            .contains("probeBearerSecret1234567890")
    );
    assert!(!result.output_summary.contains("hunter2"));
    assert!(!result.output_summary.contains("probe-title"));
}

#[test]
fn clears_the_environment_except_the_explicit_whitelist() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let _environment = EnvironmentGuard::set(&[
        ("NODE_OPTIONS", "--require=leaked-node-options"),
        ("GITHUB_TOKEN", "ghp_leakedGithubToken1234567890"),
        ("COLDSTART_PROBE_SECRET", "leaked-custom-secret"),
    ]);
    let result = WindowsVerificationExecutor.execute(
        &probe_command("print-env", Vec::new(), 10),
        directory.path(),
    );
    assert_eq!(result.exit_code, Some(0));
    let summary = result.output_summary.to_ascii_uppercase();
    assert!(summary.contains("PATH="));
    assert!(summary.contains("SYSTEMROOT="));
    assert!(!summary.contains("NODE_OPTIONS="));
    assert!(!summary.contains("GITHUB_TOKEN="));
    assert!(!summary.contains("COLDSTART_PROBE_SECRET="));
    assert!(!result.output_summary.contains("leaked-node-options"));
    assert!(!result.output_summary.contains("leakedGithubToken"));
}
