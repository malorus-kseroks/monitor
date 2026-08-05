use clap::{CommandFactory, Parser};
use kernox_monitor::{Cli, RuntimeConfig, run_doctor, run_tui};
use std::io;

#[tokio::main]
async fn main() {
    if let Err(error) = entrypoint().await {
        eprintln!("kernox-monitor: {error:#}");
        std::process::exit(1);
    }
}

async fn entrypoint() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(kernox_monitor::cli::Command::Completions { shell }) = cli.command {
        let mut command = Cli::command();
        clap_complete::generate(shell, &mut command, "kernox-monitor", &mut io::stdout());
        return Ok(());
    }
    let config = RuntimeConfig::resolve(&cli)?;
    if let Some(kernox_monitor::cli::Command::Doctor { json }) = cli.command {
        return run_doctor(&config, json).await;
    }
    run_tui(config).await
}
