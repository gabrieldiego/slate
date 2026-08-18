#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check") => run_cargo(&["check", "--workspace"]),
        Some("test") => run_cargo_workspace_tests(),
        Some("run") => run_cargo(&["run", "-p", "slate"]),
        Some("fmt") => run_cargo_without_jobs(&["fmt", "--all"]),
        Some("chrome-snapshot") => chrome_snapshot(),
        Some("chrome-verify") => chrome_verify(),
        Some("snapshot") => snapshot(),
        Some("snapshot-html") => snapshot_html(),
        Some("snapshot-local") => snapshot_local(),
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- <check|test|run|fmt|chrome-snapshot|chrome-verify|snapshot|snapshot-html|snapshot-local>"
            );
            ExitCode::from(2)
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live directly under the workspace root")
        .to_path_buf()
}

fn run_cargo(args: &[&str]) -> ExitCode {
    let jobs = std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "1".to_owned());
    let mut command = cargo_command();
    command
        .current_dir(workspace_root())
        .args(cargo_args_with_jobs(args, &jobs));
    configure_local_rust_env(&mut command);
    run_command(&mut command, "cargo")
}

fn run_cargo_without_jobs(args: &[&str]) -> ExitCode {
    let mut command = cargo_command();
    command.current_dir(workspace_root()).args(args);
    configure_local_rust_env(&mut command);
    run_command(&mut command, "cargo")
}

fn run_cargo_workspace_tests() -> ExitCode {
    let test_threads = std::env::var("SLATE_TEST_THREADS").unwrap_or_else(|_| "1".to_owned());
    run_cargo(&[
        "test",
        "--workspace",
        "--",
        "--test-threads",
        test_threads.as_str(),
    ])
}

fn cargo_command() -> Command {
    let root = workspace_root();
    let limits = root.join("scripts/with-build-limits.sh");
    if limits.is_file() {
        let mut command = Command::new(limits);
        command.arg("cargo");
        command
    } else {
        Command::new("cargo")
    }
}

fn cargo_args_with_jobs(args: &[&str], jobs: &str) -> Vec<String> {
    let mut cargo_args = Vec::with_capacity(args.len() + 2);
    let mut inserted_jobs = false;
    for arg in args {
        if !inserted_jobs && *arg == "--" {
            cargo_args.push("-j".to_owned());
            cargo_args.push(jobs.to_owned());
            inserted_jobs = true;
        }
        cargo_args.push((*arg).to_owned());
    }

    if !inserted_jobs {
        cargo_args.push("-j".to_owned());
        cargo_args.push(jobs.to_owned());
    }

    cargo_args
}

fn configure_local_rust_env(command: &mut Command) {
    let root = workspace_root();
    let rustup_home = root.join(".rustup");
    if rustup_home.is_dir() {
        command.env("RUSTUP_HOME", rustup_home);
    }
    let cargo_home = root.join(".cargo");
    if cargo_home.is_dir() {
        command.env("CARGO_HOME", cargo_home);
    }
}

fn run_command(command: &mut Command, program: &str) -> ExitCode {
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            let code = status.code().unwrap_or(1).clamp(1, 255);
            let code = u8::try_from(code).unwrap_or(1);
            ExitCode::from(code)
        }
        Err(error) => {
            eprintln!("failed to run {program}: {error}");
            ExitCode::from(1)
        }
    }
}

fn snapshot() -> ExitCode {
    run_snapshot(Path::new("target/slate-ui.png"), None)
}

fn chrome_snapshot() -> ExitCode {
    let path = Path::new("target/slate-chrome-headless.png");
    let output = path.to_string_lossy();
    let result = run_cargo(&[
        "run",
        "-p",
        "slate-chrome",
        "--bin",
        "slate-chrome-snapshot",
        "--",
        output.as_ref(),
    ]);
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
    }
    result
}

fn chrome_verify() -> ExitCode {
    let path = Path::new("target/slate-chrome-verification");
    let output = path.to_string_lossy();
    let result = run_cargo(&[
        "run",
        "-p",
        "slate-chrome",
        "--bin",
        "slate-chrome-snapshot",
        "--",
        "--verify",
        output.as_ref(),
    ]);
    if result == ExitCode::SUCCESS {
        println!("wrote {}/report.json", path.display());
    }
    result
}

fn snapshot_html() -> ExitCode {
    run_snapshot(
        Path::new("target/slate-ui-html.png"),
        Some("slate://tests/hello"),
    )
}

fn snapshot_local() -> ExitCode {
    run_snapshot(
        Path::new("target/slate-ui-local.png"),
        Some("examples/local-page.html"),
    )
}

fn run_snapshot(path: &Path, address: Option<&str>) -> ExitCode {
    let output = path.to_string_lossy();
    let mut args = vec![
        "run",
        "-p",
        "slate",
        "--",
        "--headless",
        "--exit",
        "--output",
        output.as_ref(),
    ];
    if let Some(address) = address {
        args.push(address);
    }

    let result = run_cargo(&args);
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::cargo_args_with_jobs;

    #[test]
    fn cargo_jobs_are_added_to_plain_subcommands() {
        assert_eq!(
            cargo_args_with_jobs(&["check", "--workspace"], "1"),
            ["check", "--workspace", "-j", "1"]
        );
    }

    #[test]
    fn cargo_jobs_are_inserted_before_run_separator() {
        assert_eq!(
            cargo_args_with_jobs(&["run", "-p", "slate", "--", "--headless"], "2"),
            ["run", "-p", "slate", "-j", "2", "--", "--headless"]
        );
    }
}
