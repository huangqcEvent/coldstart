use std::{
    env, fs,
    io::{self, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mode = args.first().map(String::as_str).unwrap_or("success");
    match mode {
        "success" => {
            println!("stdout-ok");
            eprintln!("stderr-ok");
        }
        "fail" => {
            println!("probe-failure");
            eprintln!("verification probe failed on purpose");
            std::process::exit(7);
        }
        "sleep" => {
            let milliseconds = args
                .get(1)
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(60_000);
            thread::sleep(Duration::from_millis(milliseconds));
        }
        "spawn-child" => {
            let pid_file = args.get(1).expect("spawn-child requires a pid file");
            thread::sleep(Duration::from_millis(150));
            let mut child = Command::new(env::current_exe().expect("probe executable path"))
                .args(["sleep", "60000"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("child probe must spawn");
            fs::write(pid_file, child.id().to_string()).expect("child pid must persist");
            thread::sleep(Duration::from_secs(60));
            let _ = child.wait();
        }
        "large-output" => {
            let stdout = io::stdout();
            let stderr = io::stderr();
            let mut stdout = stdout.lock();
            let mut stderr = stderr.lock();
            for _ in 0..(40 * 1024) {
                stdout.write_all(b"A").expect("stdout must accept bytes");
                stderr.write_all(b"B").expect("stderr must accept bytes");
            }
            stdout.write_all(b"\nSTDOUT-END\n").expect("stdout end");
            stderr.write_all(b"\nSTDERR-END\n").expect("stderr end");
            stdout.flush().expect("stdout flush");
            stderr.flush().expect("stderr flush");
        }
        "secret-output" => {
            println!("\u{1b}[31mTOKEN=ghp_probeSecretValue1234567890\u{1b}[0m");
            eprintln!("Authorization: Bearer probeBearerSecret1234567890");
            println!("password=hunter2");
            print!("\u{1b}]0;probe-title\u{7}\u{1b}[0m");
            println!("safe-marker");
        }
        "print-env" => {
            for (name, value) in env::vars_os() {
                println!("{}={}", name.to_string_lossy(), value.to_string_lossy());
            }
        }
        other => {
            eprintln!("unknown probe mode: {other}");
            std::process::exit(2);
        }
    }
}
