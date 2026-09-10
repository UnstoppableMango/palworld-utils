//! `CharacterSaveParameterMap.Value.RawData` decoding.
//!
//! Ported from `character.py` in `deafdudecomputers/PalworldSaveTools`
//! (MIT). Unlike `group_raw_data.rs`, this `RawData` blob contains a
//! nested, arbitrary Unreal property bag (`object` in the Python source) --
//! it can hold any property type a pal or player might have. Rather than
//! hand-roll a second generic UE property parser, this wraps the extracted
//! bytes in a synthetic minimal GVAS header and hands them to `uesave`'s
//! own reader/writer, which already implements that generically and
//! correctly.
//!
//! The header fields (`save_game_version = 3`, `engine_version = 5.1.1`)
//! were chosen to match Palworld's real save format (`property_tag() ==
//! false`, `property_guid() == true`), confirmed by reading the real header
//! out of an actual `Level.sav`. This was validated end-to-end against real
//! save data during planning: wrapping a real pal's extracted `object`
//! bytes this way correctly located `SaveParameter.OwnerPlayerUId`, and a
//! mutate-then-rewrite round-trip changed only the 4 bytes of that field.
//!
//! Not wired into the CLI yet -- see module doc comment on `palworld_types`.

#![allow(dead_code)]

use std::io::Cursor;

use uesave::{Property, StructValue};

fn synthetic_header() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GVAS"); // magic (only used for a log warning if mismatched)
    buf.extend_from_slice(&3u32.to_le_bytes()); // save_game_version
    buf.extend_from_slice(&0u32.to_le_bytes()); // package_version.ue4 (unused)
    buf.extend_from_slice(&0u32.to_le_bytes()); // package_version.ue5 (read since save_game_version == 3)
    buf.extend_from_slice(&5u16.to_le_bytes()); // engine_version_major
    buf.extend_from_slice(&1u16.to_le_bytes()); // engine_version_minor
    buf.extend_from_slice(&1u16.to_le_bytes()); // engine_version_patch (unused)
    buf.extend_from_slice(&0u32.to_le_bytes()); // engine_version_build (unused)
    buf.extend_from_slice(&0i32.to_le_bytes()); // engine_version branch fstring: empty
    buf.extend_from_slice(&0u32.to_le_bytes()); // custom_version format (unused, present since (5,1) >= (4,12))
    buf.extend_from_slice(&0u32.to_le_bytes()); // custom_version count: 0
    buf.extend_from_slice(&0i32.to_le_bytes()); // save_game_type fstring: empty
    buf
}

#[derive(Debug)]
pub enum CharacterRawDataError {
    Parse(uesave::ParseError),
    Write(uesave::Error),
    MissingSaveParameter,
}

impl std::fmt::Display for CharacterRawDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterRawDataError::Parse(err) => write!(f, "failed to parse RawData: {err}"),
            CharacterRawDataError::Write(err) => write!(f, "failed to write RawData: {err}"),
            CharacterRawDataError::MissingSaveParameter => {
                write!(f, "RawData object has no SaveParameter field")
            }
        }
    }
}

impl std::error::Error for CharacterRawDataError {}

impl From<uesave::ParseError> for CharacterRawDataError {
    fn from(err: uesave::ParseError) -> Self {
        CharacterRawDataError::Parse(err)
    }
}

/// Decodes a `CharacterSaveParameterMap.Value.RawData` byte blob (the full
/// payload: the property bag plus whatever trailing bytes follow it).
///
/// The returned [`uesave::Save`] holds the parsed property bag in
/// `root.properties` (see [`owner_player_uid`]/[`owner_player_uid_mut`]) and
/// the opaque trailing bytes (`unknown_bytes`/`group_id`/`trailing_bytes`/
/// any further unknown bytes, in Python's terms) verbatim in `extra`.
pub fn decode_character_raw_data(char_bytes: &[u8]) -> Result<uesave::Save, CharacterRawDataError> {
    let mut buf = synthetic_header();
    buf.extend_from_slice(char_bytes);
    Ok(uesave::SaveReader::new()
        .error_to_raw(true)
        .read(Cursor::new(&buf))?)
}

/// Re-encodes a [`uesave::Save`] produced by [`decode_character_raw_data`]
/// back into a `RawData` byte blob.
pub fn encode_character_raw_data(save: &uesave::Save) -> Result<Vec<u8>, CharacterRawDataError> {
    let mut out = Vec::new();
    save.write(&mut out).map_err(CharacterRawDataError::Write)?;
    let header_len = synthetic_header().len();
    Ok(out[header_len..].to_vec())
}

pub fn owner_player_uid(
    save: &uesave::Save,
) -> Result<Option<&uesave::FGuid>, CharacterRawDataError> {
    let (_, save_parameter_prop) = save
        .root
        .properties
        .0
        .iter()
        .find(|(k, _)| k.1 == "SaveParameter")
        .ok_or(CharacterRawDataError::MissingSaveParameter)?;
    let Property::Struct(StructValue::Struct(save_parameter)) = save_parameter_prop else {
        return Err(CharacterRawDataError::MissingSaveParameter);
    };
    Ok(save_parameter
        .0
        .iter()
        .find(|(k, _)| k.1 == "OwnerPlayerUId")
        .and_then(|(_, v)| match v {
            Property::Struct(StructValue::Guid(guid)) => Some(guid),
            _ => None,
        }))
}

pub fn owner_player_uid_mut(
    save: &mut uesave::Save,
) -> Result<Option<&mut uesave::FGuid>, CharacterRawDataError> {
    let (_, save_parameter_prop) = save
        .root
        .properties
        .0
        .iter_mut()
        .find(|(k, _)| k.1 == "SaveParameter")
        .ok_or(CharacterRawDataError::MissingSaveParameter)?;
    let Property::Struct(StructValue::Struct(save_parameter)) = save_parameter_prop else {
        return Err(CharacterRawDataError::MissingSaveParameter);
    };
    Ok(save_parameter
        .0
        .iter_mut()
        .find(|(k, _)| k.1 == "OwnerPlayerUId")
        .and_then(|(_, v)| match v {
            Property::Struct(StructValue::Guid(guid)) => Some(guid),
            _ => None,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic fixture authored with the Python reference's own
    // character.encode_bytes() against fake data (fake player/group GUIDs,
    // no real save content) -- not real save data.
    const FIXTURE_HEX: &str = "0e00000053617665506172616d65746572000f00000053747275637450726f70657274790088000000000000002400000050616c496e646976696475616c43686172616374657253617665506172616d65746572000000000000000000000000000000000000060000004c6576656c000c000000496e7450726f706572747900040000000000000000070000000f0000004f776e6572506c61796572554964000f00000053747275637450726f70657274790010000000000000000500000047756964000000000000000000000000000000000000ddccbbaa000000000000000000000000050000004e6f6e6500050000004e6f6e6500010203042222111144443333666655558888777705060708";

    fn fixture_bytes() -> Vec<u8> {
        (0..FIXTURE_HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&FIXTURE_HEX[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn finds_owner_player_uid() {
        let save = decode_character_raw_data(&fixture_bytes()).unwrap();
        let guid = owner_player_uid(&save).unwrap().unwrap();
        assert_eq!(*guid, uesave::FGuid::new(0xaabbccdd, 0, 0, 0));
    }

    #[test]
    fn roundtrips_byte_exact() {
        let fixture = fixture_bytes();
        let save = decode_character_raw_data(&fixture).unwrap();
        let out = encode_character_raw_data(&save).unwrap();
        assert_eq!(out, fixture);
    }

    #[test]
    fn mutating_owner_player_uid_changes_only_that_field() {
        let fixture = fixture_bytes();
        let mut save = decode_character_raw_data(&fixture).unwrap();
        *owner_player_uid_mut(&mut save).unwrap().unwrap() =
            uesave::FGuid::new(0x11223344, 0, 0, 0);
        let out = encode_character_raw_data(&save).unwrap();

        assert_eq!(out.len(), fixture.len());
        let diffs: Vec<usize> = fixture
            .iter()
            .zip(out.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(diffs, vec![213, 214, 215, 216]);
    }
}
