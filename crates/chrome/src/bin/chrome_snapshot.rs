#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: slate-chrome-snapshot [--size WIDTHxHEIGHT] [output.png]\n       slate-chrome-snapshot --verify output-dir";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut output = None;
    let mut size = None;
    let mut verification_output = None;

    while let Some(argument) = args.next() {
        if argument == "--size" {
            let Some(value) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            if verification_output.is_some() {
                eprintln!("{USAGE}\n--size is only supported for single snapshot output");
                return ExitCode::from(2);
            }
            match parse_size(&value.to_string_lossy()) {
                Ok(parsed) => size = Some(parsed),
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::from(2);
                }
            }
        } else if argument == "--verify" {
            let Some(value) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            if verification_output.is_some() || output.is_some() {
                eprintln!("{USAGE}\nunexpected argument: --verify");
                return ExitCode::from(2);
            }
            if size.is_some() {
                eprintln!("{USAGE}\n--verify uses the canonical 1672x941 viewport");
                return ExitCode::from(2);
            }
            verification_output = Some(PathBuf::from(value));
        } else {
            if output.is_some() || verification_output.is_some() {
                eprintln!(
                    "{USAGE}\nunexpected argument: {}",
                    argument.to_string_lossy()
                );
                return ExitCode::from(2);
            }
            output = Some(PathBuf::from(argument));
        }
    }

    if let Some(output) = verification_output {
        return match slate_chrome::write_headless_chrome_verification_report(&output) {
            Ok(()) => {
                println!("wrote verification report to {}", output.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!(
                    "failed to write verification report to {}: {error}",
                    output.display()
                );
                ExitCode::from(1)
            }
        };
    }

    let output = output.unwrap_or_else(|| PathBuf::from("target/slate-chrome-headless.png"));
    let result = match size {
        Some(size) => slate_chrome::write_headless_chrome_snapshot_with_size(&output, size),
        None => slate_chrome::write_headless_chrome_snapshot(&output),
    };

    match result {
        Ok(()) => {
            println!("wrote {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write {}: {error}", output.display());
            ExitCode::from(1)
        }
    }
}

fn parse_size(input: &str) -> Result<[u32; 2], String> {
    let Some((width, height)) = input.split_once('x') else {
        return Err(format!("invalid size '{input}', expected WIDTHxHEIGHT"));
    };
    let width = width
        .parse::<u32>()
        .map_err(|error| format!("invalid width in '{input}': {error}"))?;
    let height = height
        .parse::<u32>()
        .map_err(|error| format!("invalid height in '{input}': {error}"))?;

    if width == 0 || height == 0 {
        return Err(format!(
            "invalid size '{input}', dimensions must be nonzero"
        ));
    }

    Ok([width, height])
}
