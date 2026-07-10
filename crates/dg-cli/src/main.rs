use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = dg_cli::Cli::parse();
    dg_cli::run(cli)
}
