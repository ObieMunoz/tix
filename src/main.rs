use clap::Parser;

/// Corporate git workflow assistant — ticket-prefix discipline and branch protection.
#[derive(Parser)]
#[command(name = "tix", version, about, long_about = None)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    Ok(())
}
