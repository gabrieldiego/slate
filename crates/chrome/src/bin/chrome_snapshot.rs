#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut output = PathBuf::from("target/slate-chrome-headless.png");
    let mut size = None;

    while let Some(argument) = args.next() {
        if argument == "--size" {
            let Some(value) = args.next() else {
                eprintln!("usage: slate-chrome-snapshot [--size WIDTHxHEIGHT] [output.png]");
                return ExitCode::from(2);
            };
            match parse_size(&value.to_string_lossy()) {
                Ok(parsed) => size = Some(parsed),
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::from(2);
                }
            }
        } else {
            output = PathBuf::from(argument);
            if let Some(extra) = args.next() {
                eprintln!(
                    "usage: slate-chrome-snapshot [--size WIDTHxHEIGHT] [output.png]\nunexpected argument: {}",
                    extra.to_string_lossy()
                );
                return ExitCode::from(2);
            }
        }
    }

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
