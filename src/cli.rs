use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{inspect, migrate_player, steam_id};

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
    /// Migrate a player's saves to a new PlayerUId (e.g. co-op host ->
    /// dedicated server), writing the result to a separate output
    /// directory -- the save at `save_path` is never modified.
    MigratePlayer {
        /// Path to the save folder (containing Level.sav and Players/)
        save_path: PathBuf,
        /// New PlayerUId, from an already-existing throwaway character on
        /// the target server
        new_guid: String,
        /// Old PlayerUId (the player's current PlayerUId; for a co-op host
        /// this is always 00000000000000000000000000000001)
        old_guid: String,
        /// Directory to write the migrated save into
        #[arg(long)]
        output: PathBuf,
    },
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
        Command::MigratePlayer {
            save_path,
            new_guid,
            old_guid,
            output,
        } => match migrate_player::migrate_player(&save_path, &output, &new_guid, &old_guid) {
            Ok(()) => println!("{}", output.display()),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        },
    }
}
