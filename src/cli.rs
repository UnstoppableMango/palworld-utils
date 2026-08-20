use clap::{Parser, Subcommand};

use crate::steam_id;

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
    }
}
