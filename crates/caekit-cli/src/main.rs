//! The `caekit` command-line tool: validates and solves a model file and writes the check
//! or the run report as JSON to standard output.
//!
//! Exit code 0 means a valid model or a converged run, 2 a run that stopped without
//! convergence (the report is still written), and 1 an input, calculation-range or I/O error.

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use caekit::{Model, StopReason, solve};
use serde::Serialize;

const USAGE: &str = "CAEKit engineering simulation\n\nUsage:\n  caekit check <model.json>\n  caekit run <model.json>\n  caekit --version\n\nChecks and reports are written as JSON to standard output.";

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => println!("{USAGE}"),
        [flag] if flag == "--help" || flag == "-h" => println!("{USAGE}"),
        [flag] if flag == "--version" => println!("caekit {}", env!("CARGO_PKG_VERSION")),
        [command, path] if command == "check" || command == "run" => {
            let model = Model::from_json(&std::fs::read(path)?)?;
            if command == "check" {
                print_json(&model.validate()?)?;
            } else {
                let report = solve(&model)?;
                print_json(&report)?;
                if report.stop_reason != StopReason::Converged {
                    eprintln!("caekit: solver stopped with {:?}", report.stop_reason);
                    return Ok(ExitCode::from(2));
                }
            }
        }
        _ => return Err(format!("invalid arguments\n\n{USAGE}").into()),
    }
    Ok(ExitCode::SUCCESS)
}

fn print_json<T: Serialize>(report: &T) -> Result<(), Box<dyn Error>> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    writeln!(stdout)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("caekit: {error}");
            ExitCode::FAILURE
        }
    }
}
