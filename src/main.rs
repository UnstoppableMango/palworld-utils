mod character_raw_data;
mod cli;
mod group_raw_data;
mod inspect;
mod migrate_player;
mod palworld_type_hints;
mod palworld_types;
mod sav_container;
mod steam_id;

fn main() {
    cli::execute();
}
