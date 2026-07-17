use std::process::ExitCode;

use clap::Parser;

fn main() -> anyhow::Result<ExitCode> {
    let cli = dg_cli::Cli::parse();
    dg_cli::run(cli)
}
