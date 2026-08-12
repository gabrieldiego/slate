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
        Some("snapshot-html") => snapshot_html(),
        Some("snapshot-local") => snapshot_local(),
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- <check|test|run|fmt|snapshot|snapshot-html|snapshot-local>"
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
    match write_snapshot(Path::new("target/slate-ui.ppm"), None) {
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

fn snapshot_html() -> ExitCode {
    match write_snapshot(
        Path::new("target/slate-ui-html.ppm"),
        Some("slate://tests/hello"),
    ) {
        Ok(()) => {
            println!("wrote target/slate-ui-html.ppm");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write HTML snapshot: {error}");
            ExitCode::from(1)
        }
    }
}

fn snapshot_local() -> ExitCode {
    match write_snapshot(
        Path::new("target/slate-ui-local.ppm"),
        Some("examples/local-page.html"),
    ) {
        Ok(()) => {
            println!("wrote target/slate-ui-local.ppm");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write local HTML snapshot: {error}");
            ExitCode::from(1)
        }
    }
}

fn write_snapshot(path: &Path, address: Option<&str>) -> io::Result<()> {
    let mut state = BrowserState::new(&ServoBackend);
    if let Some(address) = address {
        state
            .navigate(address)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    }

    let mut view = ChromeView::new(state);
    let _ = view.update_web_viewport(1280, 720);
    let _ = view.refresh_web_viewport();
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
