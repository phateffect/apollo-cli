pub mod cli;
pub mod client;
pub mod config;
pub mod error;
pub mod model;

mod commands;
mod output;
mod prompt;
mod render;

use anyhow::Result;

pub async fn run(cli: cli::Cli) -> Result<()> {
    commands::execute(cli).await
}
