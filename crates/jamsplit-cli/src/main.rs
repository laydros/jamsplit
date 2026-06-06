mod cli;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let parsed = cli::Cli::parse();
    match &parsed.command {
        cli::Command::Validate(args) => to_exit(cli::validate(args).map(|()| true), 1),
        cli::Command::Inspect(args) => to_exit(cli::inspect(args).map(|()| true), 1),
        cli::Command::Split(args) => to_exit(cli::split(args), 2),
    }
}

/// Err -> exit 1 (invalid input); Ok(false) -> `partial_failure_code`.
fn to_exit(result: anyhow::Result<bool>, partial_failure_code: u8) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(partial_failure_code),
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::from(1)
        }
    }
}
