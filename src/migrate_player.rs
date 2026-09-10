//! Migrates a player's saves from one `PlayerUId` to another (e.g. co-op
//! host -> dedicated server), porting
//! `NFZ-441/Palworld-Co-op-to-Dedicated-Server-Migration-Tool`'s
//! `fix_host_save.py` (MIT) plus a correctness fix it's missing: it never
//! reassigns `OwnerPlayerUId` inside a pal's own `CharacterSaveParameterMap`
//! `RawData` (see `character_raw_data.rs`), so a migrated player's pals show
//! stale ownership under the reference tool. This port always fixes both
//! pal ownership and guild membership -- there's no flag for either, unlike
//! Python's opt-in `--guild-fix`.
//!
//! Unlike the Python reference (which backs up in place then overwrites),
//! this always writes to a separate output directory: the whole save
//! directory is copied to `output_path` up front and every read/write after
//! that happens against the copy, so `save_path` is never touched.
//!
//! Since `oozextract` has no Oodle encoder (see `sav_container.rs`), a `PlM`
//! save can't be written back out in its original compression format --
//! `write_sav` falls back to `PlZ` in that case, which Palworld reads just
//! as well for a given save. This matters in practice: `PlM` is what
//! dedicated servers use, and co-op-host-to-dedicated-server migration is
//! this command's primary use case.

use std::path::Path;

use uesave::{ByteArray, FGuid, Properties, Property, StructValue, ValueVec};

use crate::{character_raw_data, group_raw_data, palworld_types, sav_container};

#[derive(Debug)]
pub enum MigratePlayerError {
    Io(std::io::Error),
    InvalidGuid(uesave::Error),
    Sav(sav_container::SavError),
    Parse(uesave::ParseError),
    Write(uesave::Error),
    CharacterRawData(character_raw_data::CharacterRawDataError),
    GroupRawData(group_raw_data::GroupRawDataError),
    MissingProperty(&'static str),
    UnexpectedShape(&'static str),
}

impl std::fmt::Display for MigratePlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigratePlayerError::Io(err) => write!(f, "io error: {err}"),
            MigratePlayerError::InvalidGuid(err) => write!(f, "invalid GUID: {err}"),
            MigratePlayerError::Sav(err) => write!(f, "{err}"),
            MigratePlayerError::Parse(err) => write!(f, "failed to parse save: {err}"),
            MigratePlayerError::Write(err) => write!(f, "failed to write save: {err}"),
            MigratePlayerError::CharacterRawData(err) => write!(f, "{err}"),
            MigratePlayerError::GroupRawData(err) => write!(f, "{err}"),
            MigratePlayerError::MissingProperty(name) => {
                write!(f, "expected property not found: {name}")
            }
            MigratePlayerError::UnexpectedShape(name) => {
                write!(f, "property had an unexpected shape: {name}")
            }
        }
    }
}

impl std::error::Error for MigratePlayerError {}

impl From<std::io::Error> for MigratePlayerError {
    fn from(err: std::io::Error) -> Self {
        MigratePlayerError::Io(err)
    }
}

impl From<sav_container::SavError> for MigratePlayerError {
    fn from(err: sav_container::SavError) -> Self {
        MigratePlayerError::Sav(err)
    }
}

impl From<uesave::ParseError> for MigratePlayerError {
    fn from(err: uesave::ParseError) -> Self {
        MigratePlayerError::Parse(err)
    }
}

impl From<character_raw_data::CharacterRawDataError> for MigratePlayerError {
    fn from(err: character_raw_data::CharacterRawDataError) -> Self {
        MigratePlayerError::CharacterRawData(err)
    }
}

impl From<group_raw_data::GroupRawDataError> for MigratePlayerError {
    fn from(err: group_raw_data::GroupRawDataError) -> Self {
        MigratePlayerError::GroupRawData(err)
    }
}

fn find<'a>(props: &'a Properties, name: &str) -> Option<&'a Property> {
    props.0.iter().find(|(k, _)| k.1 == name).map(|(_, v)| v)
}

fn find_mut<'a>(props: &'a mut Properties, name: &str) -> Option<&'a mut Property> {
    props
        .0
        .iter_mut()
        .find(|(k, _)| k.1 == name)
        .map(|(_, v)| v)
}

fn as_struct_mut(prop: &mut Property) -> Option<&mut Properties> {
    match prop {
        Property::Struct(StructValue::Struct(p)) => Some(p),
        _ => None,
    }
}

fn as_guid(prop: &Property) -> Option<&FGuid> {
    match prop {
        Property::Struct(StructValue::Guid(g)) => Some(g),
        _ => None,
    }
}

fn as_guid_mut(prop: &mut Property) -> Option<&mut FGuid> {
    match prop {
        Property::Struct(StructValue::Guid(g)) => Some(g),
        _ => None,
    }
}

fn as_bytes_mut(prop: &mut Property) -> Option<&mut Vec<u8>> {
    match prop {
        Property::Array(ValueVec::Byte(ByteArray::Byte(bytes))) => Some(bytes),
        _ => None,
    }
}

fn replace_guid_if_match(guid: &mut FGuid, old: &FGuid, new: &FGuid) {
    if guid == old {
        *guid = *new;
    }
}

/// Sets `SaveData.PlayerUId`/`SaveData.IndividualId.PlayerUId` to `new_guid`
/// on a player's own save, and returns the (unmodified) instance ID that
/// identifies this player's character entry in `CharacterSaveParameterMap`.
fn migrate_own_save(
    save: &mut uesave::Save,
    new_guid: &FGuid,
) -> Result<FGuid, MigratePlayerError> {
    let save_data = as_struct_mut(
        find_mut(&mut save.root.properties, "SaveData")
            .ok_or(MigratePlayerError::MissingProperty("SaveData"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("SaveData"))?;

    *as_guid_mut(
        find_mut(save_data, "PlayerUId").ok_or(MigratePlayerError::MissingProperty("PlayerUId"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("PlayerUId"))? = *new_guid;

    let individual_id = as_struct_mut(
        find_mut(save_data, "IndividualId")
            .ok_or(MigratePlayerError::MissingProperty("IndividualId"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("IndividualId"))?;

    *as_guid_mut(find_mut(individual_id, "PlayerUId").ok_or(
        MigratePlayerError::MissingProperty("IndividualId.PlayerUId"),
    )?)
    .ok_or(MigratePlayerError::UnexpectedShape(
        "IndividualId.PlayerUId",
    ))? = *new_guid;

    let old_instance_id = *as_guid(find(individual_id, "InstanceId").ok_or(
        MigratePlayerError::MissingProperty("IndividualId.InstanceId"),
    )?)
    .ok_or(MigratePlayerError::UnexpectedShape(
        "IndividualId.InstanceId",
    ))?;

    Ok(old_instance_id)
}

/// Walks `worldSaveData.CharacterSaveParameterMap`: retargets the one entry
/// matching `old_instance_id`'s map key, and reassigns pal ownership
/// (`OwnerPlayerUId` inside every entry's `RawData`) wherever it points at
/// `old_guid`.
fn migrate_character_map(
    save: &mut uesave::Save,
    old_instance_id: &FGuid,
    old_guid: &FGuid,
    new_guid: &FGuid,
) -> Result<(), MigratePlayerError> {
    let world_save_data = as_struct_mut(
        find_mut(&mut save.root.properties, "worldSaveData")
            .ok_or(MigratePlayerError::MissingProperty("worldSaveData"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("worldSaveData"))?;

    let Property::Map(entries) = find_mut(world_save_data, "CharacterSaveParameterMap").ok_or(
        MigratePlayerError::MissingProperty("CharacterSaveParameterMap"),
    )?
    else {
        return Err(MigratePlayerError::UnexpectedShape(
            "CharacterSaveParameterMap",
        ));
    };

    for entry in entries.iter_mut() {
        let key = as_struct_mut(&mut entry.key).ok_or(MigratePlayerError::UnexpectedShape(
            "CharacterSaveParameterMap.Key",
        ))?;
        let key_instance_id = as_guid(find(key, "InstanceId").ok_or(
            MigratePlayerError::MissingProperty("CharacterSaveParameterMap.Key.InstanceId"),
        )?)
        .ok_or(MigratePlayerError::UnexpectedShape(
            "CharacterSaveParameterMap.Key.InstanceId",
        ))?;
        if key_instance_id == old_instance_id {
            *as_guid_mut(find_mut(key, "PlayerUId").ok_or(
                MigratePlayerError::MissingProperty("CharacterSaveParameterMap.Key.PlayerUId"),
            )?)
            .ok_or(MigratePlayerError::UnexpectedShape(
                "CharacterSaveParameterMap.Key.PlayerUId",
            ))? = *new_guid;
        }

        let value = as_struct_mut(&mut entry.value).ok_or(MigratePlayerError::UnexpectedShape(
            "CharacterSaveParameterMap.Value",
        ))?;
        let raw_data_bytes = as_bytes_mut(find_mut(value, "RawData").ok_or(
            MigratePlayerError::MissingProperty("CharacterSaveParameterMap.Value.RawData"),
        )?)
        .ok_or(MigratePlayerError::UnexpectedShape(
            "CharacterSaveParameterMap.Value.RawData",
        ))?;

        let mut character_save = character_raw_data::decode_character_raw_data(raw_data_bytes)?;
        if let Some(owner) = character_raw_data::owner_player_uid_mut(&mut character_save)? {
            if owner == old_guid {
                *owner = *new_guid;
                *raw_data_bytes = character_raw_data::encode_character_raw_data(&character_save)?;
            }
        }
    }

    Ok(())
}

/// Walks `worldSaveData.GroupSaveDataMap`, reassigning guild/organization
/// membership wherever it points at `old_guid`.
fn migrate_groups(
    save: &mut uesave::Save,
    old_guid: &FGuid,
    new_guid: &FGuid,
) -> Result<(), MigratePlayerError> {
    let world_save_data = as_struct_mut(
        find_mut(&mut save.root.properties, "worldSaveData")
            .ok_or(MigratePlayerError::MissingProperty("worldSaveData"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("worldSaveData"))?;

    let Property::Map(entries) = find_mut(world_save_data, "GroupSaveDataMap")
        .ok_or(MigratePlayerError::MissingProperty("GroupSaveDataMap"))?
    else {
        return Err(MigratePlayerError::UnexpectedShape("GroupSaveDataMap"));
    };

    for entry in entries.iter_mut() {
        let value = as_struct_mut(&mut entry.value).ok_or(MigratePlayerError::UnexpectedShape(
            "GroupSaveDataMap.Value",
        ))?;

        let Property::Enum(group_type) =
            find(value, "GroupType").ok_or(MigratePlayerError::MissingProperty("GroupType"))?
        else {
            return Err(MigratePlayerError::UnexpectedShape(
                "GroupSaveDataMap.Value.GroupType",
            ));
        };
        let group_type = group_type.clone();

        let raw_data_bytes = as_bytes_mut(find_mut(value, "RawData").ok_or(
            MigratePlayerError::MissingProperty("GroupSaveDataMap.Value.RawData"),
        )?)
        .ok_or(MigratePlayerError::UnexpectedShape(
            "GroupSaveDataMap.Value.RawData",
        ))?;

        let mut group = group_raw_data::decode_group_raw_data(raw_data_bytes, &group_type)?;
        let mut changed = false;
        migrate_group_raw_data(&mut group, old_guid, new_guid, &mut changed);
        if changed {
            *raw_data_bytes = group_raw_data::encode_group_raw_data(&group);
        }
    }

    Ok(())
}

fn migrate_group_raw_data(
    group: &mut group_raw_data::GroupRawData,
    old_guid: &FGuid,
    new_guid: &FGuid,
    changed: &mut bool,
) {
    use group_raw_data::{GroupRawData, GuildTail};

    let mut replace = |guid: &mut FGuid| {
        if guid == old_guid {
            *guid = *new_guid;
            *changed = true;
        }
    };

    match group {
        GroupRawData::Guild {
            individual_character_handle_ids,
            tail,
            ..
        } => {
            for handle in individual_character_handle_ids.iter_mut() {
                replace(&mut handle.guid);
            }
            match tail {
                GuildTail::V1 {
                    admin_player_uid,
                    players,
                    ..
                } => {
                    replace(admin_player_uid);
                    for p in players.iter_mut() {
                        replace(&mut p.player_uid);
                    }
                }
                GuildTail::V2 {
                    admin_player_uid,
                    players,
                    ..
                } => {
                    replace(admin_player_uid);
                    for p in players.iter_mut() {
                        replace(&mut p.info.player_uid);
                    }
                }
            }
        }
        GroupRawData::IndependentGuild {
            individual_character_handle_ids,
            player_uid,
            ..
        } => {
            for handle in individual_character_handle_ids.iter_mut() {
                replace(&mut handle.guid);
            }
            replace(player_uid);
        }
        GroupRawData::Organization {
            individual_character_handle_ids,
            ..
        } => {
            for handle in individual_character_handle_ids.iter_mut() {
                replace(&mut handle.guid);
            }
        }
    }
}

/// Walks `worldSaveData.InLockerCharacterInstanceIDArray` (Dimensional Pal
/// Storage locker), reassigning `PlayerUId` wherever it points at
/// `old_guid`. Fully generic -- no custom decoder needed.
fn migrate_locker(
    save: &mut uesave::Save,
    old_guid: &FGuid,
    new_guid: &FGuid,
) -> Result<(), MigratePlayerError> {
    let world_save_data = as_struct_mut(
        find_mut(&mut save.root.properties, "worldSaveData")
            .ok_or(MigratePlayerError::MissingProperty("worldSaveData"))?,
    )
    .ok_or(MigratePlayerError::UnexpectedShape("worldSaveData"))?;

    let Some(Property::Set(ValueVec::Struct(entries))) =
        find_mut(world_save_data, "InLockerCharacterInstanceIDArray")
    else {
        // Not every save has Dimensional Pal Storage contents.
        return Ok(());
    };

    for entry in entries.iter_mut() {
        let StructValue::Struct(props) = entry else {
            continue;
        };
        if let Some(guid) = find_mut(props, "PlayerUId").and_then(as_guid_mut) {
            replace_guid_if_match(guid, old_guid, new_guid);
        }
    }

    Ok(())
}

/// Walks a `_dps.sav`'s `SaveParameterArray`, reassigning
/// `SaveParameter.OwnerPlayerUId` wherever it points at `old_guid`. Fully
/// generic -- no custom decoder needed (unlike `CharacterSaveParameterMap`,
/// DPS entries aren't wrapped in a `RawData` blob).
fn migrate_dps(
    save: &mut uesave::Save,
    old_guid: &FGuid,
    new_guid: &FGuid,
) -> Result<(), MigratePlayerError> {
    let Property::Array(ValueVec::Struct(entries)) =
        find_mut(&mut save.root.properties, "SaveParameterArray")
            .ok_or(MigratePlayerError::MissingProperty("SaveParameterArray"))?
    else {
        return Err(MigratePlayerError::UnexpectedShape("SaveParameterArray"));
    };

    for entry in entries.iter_mut() {
        let StructValue::Struct(entry_props) = entry else {
            continue;
        };
        let Some(save_parameter) = find_mut(entry_props, "SaveParameter").and_then(as_struct_mut)
        else {
            continue;
        };
        if let Some(guid) = find_mut(save_parameter, "OwnerPlayerUId").and_then(as_guid_mut) {
            replace_guid_if_match(guid, old_guid, new_guid);
        }
    }

    Ok(())
}

fn read_sav(path: &Path) -> Result<(sav_container::DecodedSav, uesave::Save), MigratePlayerError> {
    let raw = std::fs::read(path)?;
    let decoded = sav_container::decompress_sav(&raw)?;
    let save = uesave::SaveReader::new()
        .types(palworld_types::palworld_types())
        .error_to_raw(true)
        .read(std::io::Cursor::new(&decoded.data))?;
    Ok((decoded, save))
}

fn write_sav(
    path: &Path,
    decoded: &sav_container::DecodedSav,
    save: &uesave::Save,
) -> Result<(), MigratePlayerError> {
    let mut buf = Vec::new();
    save.write(&mut buf).map_err(MigratePlayerError::Write)?;
    let container = match sav_container::compress_sav(&buf, decoded.save_type, decoded.magic) {
        Ok(container) => container,
        Err(sav_container::SavError::UnsupportedWrite(_)) => {
            // No Oodle encoder exists (see sav_container.rs), so a PlM
            // original can't be written back out in its original format.
            // Fall back to PlZ (zlib, single-compressed): Palworld reads
            // either compression format for a given save.
            sav_container::compress_sav(&buf, 0x31, *b"PlZ")?
        }
        Err(err) => return Err(err.into()),
    };
    std::fs::write(path, container)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

pub fn migrate_player(
    save_path: &Path,
    output_path: &Path,
    new_guid: &str,
    old_guid: &str,
) -> Result<(), MigratePlayerError> {
    let new_guid = FGuid::parse_str(new_guid).map_err(MigratePlayerError::InvalidGuid)?;
    let old_guid = FGuid::parse_str(old_guid).map_err(MigratePlayerError::InvalidGuid)?;

    copy_dir_recursive(save_path, output_path)?;

    let level_path = output_path.join("Level.sav");
    let old_player_path = output_path
        .join("Players")
        .join(format!("{}.sav", guid_filename(&old_guid)));
    let new_player_path = output_path
        .join("Players")
        .join(format!("{}.sav", guid_filename(&new_guid)));

    let (level_decoded, mut level_save) = read_sav(&level_path)?;
    let (player_decoded, mut player_save) = read_sav(&old_player_path)?;

    let old_instance_id = migrate_own_save(&mut player_save, &new_guid)?;
    migrate_character_map(&mut level_save, &old_instance_id, &old_guid, &new_guid)?;
    migrate_groups(&mut level_save, &old_guid, &new_guid)?;
    migrate_locker(&mut level_save, &old_guid, &new_guid)?;

    write_sav(&level_path, &level_decoded, &level_save)?;
    write_sav(&new_player_path, &player_decoded, &player_save)?;
    std::fs::remove_file(&old_player_path)?;

    let old_dps_path = output_path
        .join("Players")
        .join(format!("{}_dps.sav", guid_filename(&old_guid)));
    let new_dps_path = output_path
        .join("Players")
        .join(format!("{}_dps.sav", guid_filename(&new_guid)));
    if old_dps_path.exists() {
        let (dps_decoded, mut dps_save) = read_sav(&old_dps_path)?;
        migrate_dps(&mut dps_save, &old_guid, &new_guid)?;
        write_sav(&new_dps_path, &dps_decoded, &dps_save)?;
        std::fs::remove_file(&old_dps_path)?;
    }

    Ok(())
}

/// Palworld save filenames use the plain 32-hex-char form (no dashes) --
/// same convention as `steam_id.rs`'s `PlayerUId` output. `FGuid` has no
/// public accessor for its raw words, so this round-trips through its
/// `Display`/`parse_str` (verified exact, same as `group_raw_data.rs`).
fn guid_filename(guid: &FGuid) -> String {
    guid.to_string().replace('-', "").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_guid_if_match_replaces_only_on_match() {
        let old = FGuid::new(1, 0, 0, 0);
        let new = FGuid::new(2, 0, 0, 0);
        let other = FGuid::new(3, 0, 0, 0);

        let mut matching = old;
        replace_guid_if_match(&mut matching, &old, &new);
        assert_eq!(matching, new);

        let mut non_matching = other;
        replace_guid_if_match(&mut non_matching, &old, &new);
        assert_eq!(non_matching, other);
    }

    #[test]
    fn guid_filename_matches_steam_id_format() {
        let guid = FGuid::new(0xaabbccdd, 0, 0, 0);
        assert_eq!(guid_filename(&guid), "AABBCCDD000000000000000000000000");
    }
}
