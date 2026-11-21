use std::process::ExitCode;

use apollo::cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    match apollo::run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("apollo: {error:#}");
            ExitCode::FAILURE
        }
    }
}
