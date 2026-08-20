//! Ties the container layer, `uesave`, and the Palworld type-hint table
//! together into a minimal diagnostic: read a `.sav` file end to end and
//! print a short summary. First real exercise of `sav_container` and
//! `palworld_types` against a live file.

use std::io::Cursor;
use std::path::Path;

use crate::{palworld_types, sav_container};

#[derive(Debug)]
pub enum InspectError {
    Io(std::io::Error),
    Sav(sav_container::SavError),
    Parse(uesave::ParseError),
}

impl std::fmt::Display for InspectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InspectError::Io(err) => write!(f, "io error: {err}"),
            InspectError::Sav(err) => write!(f, "{err}"),
            InspectError::Parse(err) => write!(f, "failed to parse GVAS data: {err}"),
        }
    }
}

impl std::error::Error for InspectError {}

impl From<std::io::Error> for InspectError {
    fn from(err: std::io::Error) -> Self {
        InspectError::Io(err)
    }
}

impl From<sav_container::SavError> for InspectError {
    fn from(err: sav_container::SavError) -> Self {
        InspectError::Sav(err)
    }
}

impl From<uesave::ParseError> for InspectError {
    fn from(err: uesave::ParseError) -> Self {
        InspectError::Parse(err)
    }
}

pub fn inspect(path: &Path) -> Result<String, InspectError> {
    let raw = std::fs::read(path)?;
    let decoded = sav_container::decompress_sav(&raw)?;

    let save = uesave::SaveReader::new()
        .types(palworld_types::palworld_types())
        .read(Cursor::new(decoded.data))?;

    let magic = String::from_utf8_lossy(&decoded.magic);
    let mut names: Vec<&str> = save
        .root
        .properties
        .0
        .keys()
        .map(|k| k.1.as_str())
        .collect();
    names.sort_unstable();

    Ok(format!(
        "save_game_type: {}\nformat: {magic} (save_type 0x{:02X})\nproperties ({}): {}",
        save.root.save_game_type,
        decoded.save_type,
        names.len(),
        names.join(", "),
    ))
}
