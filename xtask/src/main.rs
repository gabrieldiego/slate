#![forbid(unsafe_code)]

use std::path::Path;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check") => run("cargo", &["check", "--workspace"]),
        Some("test") => run("cargo", &["test", "--workspace"]),
        Some("run") => run("cargo", &["run", "-p", "slate"]),
        Some("fmt") => run("cargo", &["fmt", "--all"]),
        Some("chrome-snapshot") => chrome_snapshot(),
        Some("snapshot") => snapshot(),
        Some("snapshot-html") => snapshot_html(),
        Some("snapshot-local") => snapshot_local(),
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- <check|test|run|fmt|chrome-snapshot|snapshot|snapshot-html|snapshot-local>"
            );
            ExitCode::from(2)
        }
    }
}

fn run(program: &str, args: &[&str]) -> ExitCode {
    match Command::new(program).args(args).status() {
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
    let result = run(
        "cargo",
        &[
            "run",
            "-j",
            "1",
            "-p",
            "slate-chrome",
            "--bin",
            "slate-chrome-snapshot",
            "--",
            output.as_ref(),
        ],
    );
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
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

    let result = run("cargo", &args);
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
    }
    result
}
