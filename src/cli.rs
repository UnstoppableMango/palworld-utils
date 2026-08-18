use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "palworld-utils",
    about = "Utilities for working with Palworld save data"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Convert Palworld save data
    Convert,
}

pub fn execute() {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert => {}
    }
}
