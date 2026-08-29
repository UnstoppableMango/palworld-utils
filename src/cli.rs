use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{inspect, steam_id};

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
    /// Convert a SteamID64 to a Palworld PlayerUId
    SteamId { input: String },
    /// Inspect a Palworld save file
    Inspect { path: PathBuf },
}

pub fn execute() {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert => {}
        Command::SteamId { input } => match steam_id::steam_id64_to_palworld_player_id(&input) {
            Ok(player_id) => println!("{player_id}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        },
        Command::Inspect { path } => match inspect::inspect(&path) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        },
    }
}
