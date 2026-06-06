mod cli;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let parsed = cli::Cli::parse();
    let result = match &parsed.command {
        cli::Command::Validate(args) => cli::validate(args),
        cli::Command::Inspect(args) => cli::inspect(args),
        cli::Command::Split(_) => unimplemented!("Task 15"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
