#![forbid(unsafe_code)]

use slate_browser_core::BrowserState;
use slate_chrome::ChromeView;
use slate_rendering::ServoBackend;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check") => run("cargo", &["check", "--workspace"]),
        Some("test") => run("cargo", &["test", "--workspace"]),
        Some("run") => run("cargo", &["run", "-p", "slate"]),
        Some("fmt") => run("cargo", &["fmt", "--all"]),
        Some("snapshot") => snapshot(),
        _ => {
            eprintln!("usage: cargo run -p xtask -- <check|test|run|fmt|snapshot>");
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
    match write_snapshot(Path::new("target/slate-ui.ppm")) {
        Ok(()) => {
            println!("wrote target/slate-ui.ppm");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write snapshot: {error}");
            ExitCode::from(1)
        }
    }
}

fn write_snapshot(path: &Path) -> io::Result<()> {
    let view = ChromeView::new(BrowserState::new(&ServoBackend));
    let frame = view.render(1280, 720);
    let mut data = Vec::with_capacity(
        frame
            .width()
            .saturating_mul(frame.height())
            .saturating_mul(3)
            .saturating_add(64),
    );

    data.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width(), frame.height()).as_bytes());
    for pixel in frame.pixels() {
        let red = u8::try_from((pixel >> 16) & 0xff).unwrap_or(0);
        let green = u8::try_from((pixel >> 8) & 0xff).unwrap_or(0);
        let blue = u8::try_from(pixel & 0xff).unwrap_or(0);
        data.extend_from_slice(&[red, green, blue]);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)
}
