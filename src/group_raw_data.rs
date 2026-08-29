//! `GroupSaveDataMap` `RawData` decoding (guild/organization membership).
//!
//! Ported from `group.py` in `deafdudecomputers/PalworldSaveTools` (MIT).
//! Unlike `character_raw_data.rs`, this `RawData` blob is a fully custom
//! binary format (GUIDs, length-prefixed strings, count-prefixed arrays in
//! a fixed sequence), not a generic Unreal property bag, so there's no
//! `uesave` parser to reuse here -- this is a direct hand-rolled port.
//!
//! One correctness fix versus the Python reference: its `encode_bytes` for
//! `EPalGroupType::IndependentGuild` omits `base_camp_level`/
//! `map_object_instance_ids_base_camp_points`/`guild_name` entirely, even
//! though `decode_bytes` expects to read them back -- confirmed empirically
//! (`decode(encode(x))` fails with a short read). This never appears in the
//! real save used to validate this port (0 of 13 real groups), but our
//! encoder writes those fields in the order `decode_bytes` expects, since
//! that's the actual on-disk format `decode_bytes` was written against.
//!
//! FString payloads (`group_name`, `guild_name`, `player_name`, ...) are
//! kept as raw bytes rather than decoded to `String` -- a real save
//! contained a non-ASCII byte in an in-game name that crashes the Python
//! reference's naive ASCII-decode path. We never need to read or write
//! string *content* for membership migration, so raw bytes sidestep the
//! encoding question entirely.
//!
//! Not wired into the CLI yet -- see module doc comment on `palworld_types`.

#![allow(dead_code)]

use std::io::{Cursor, Read, Write};

use uesave::FGuid;

#[derive(Debug)]
pub enum GroupRawDataError {
    UnexpectedEof,
    NotFullyConsumed { remaining: usize },
    UnknownGroupType(String),
}

impl std::fmt::Display for GroupRawDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupRawDataError::UnexpectedEof => write!(f, "unexpected end of group RawData"),
            GroupRawDataError::NotFullyConsumed { remaining } => write!(
                f,
                "{remaining} trailing bytes left after decoding group RawData"
            ),
            GroupRawDataError::UnknownGroupType(group_type) => {
                write!(f, "unknown group type: {group_type:?}")
            }
        }
    }
}

impl std::error::Error for GroupRawDataError {}

impl From<std::io::Error> for GroupRawDataError {
    fn from(_: std::io::Error) -> Self {
        GroupRawDataError::UnexpectedEof
    }
}

type Result<T> = std::result::Result<T, GroupRawDataError>;

/// A raw Unreal `FString` payload, kept undecoded (see module doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FStringRaw {
    Empty,
    /// Single-byte-encoded content, without the trailing null terminator.
    Ascii(Vec<u8>),
    /// UTF-16LE-encoded content, without the trailing null terminator.
    Utf16(Vec<u8>),
}

struct Reader<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    fn eof(&self) -> bool {
        self.cursor.position() >= self.cursor.get_ref().len() as u64
    }

    fn remaining(&self) -> usize {
        self.cursor.get_ref().len() - self.cursor.position() as usize
    }

    fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn byte_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.byte_array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.byte_array()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.byte_array()?))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.byte_array()?))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.byte_array()?))
    }

    fn guid(&mut self) -> Result<FGuid> {
        Ok(FGuid::new(
            self.u32()?,
            self.u32()?,
            self.u32()?,
            self.u32()?,
        ))
    }

    /// Unreal `FString`: an `i32` length prefix, then payload per sign
    /// (negative = UTF-16LE, positive = single-byte), each including a
    /// trailing null terminator that's stripped here.
    fn fstring(&mut self) -> Result<FStringRaw> {
        let size = self.i32()?;
        if size == 0 {
            return Ok(FStringRaw::Empty);
        }
        if size < 0 {
            let n = (-size) as usize;
            let mut data = self.bytes(n * 2)?;
            data.truncate(data.len() - 2); // drop the UTF-16 null terminator
            Ok(FStringRaw::Utf16(data))
        } else {
            let n = size as usize;
            let mut data = self.bytes(n)?;
            data.truncate(data.len() - 1); // drop the single-byte null terminator
            Ok(FStringRaw::Ascii(data))
        }
    }

    fn tarray<T>(&mut self, mut read: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let count = self.u32()?;
        (0..count).map(|_| read(self)).collect()
    }
}

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn bytes(self) -> Vec<u8> {
        self.buf
    }

    fn raw(&mut self, data: &[u8]) {
        self.buf.write_all(data).unwrap();
    }

    fn u8(&mut self, v: u8) {
        self.raw(&[v]);
    }

    fn u32(&mut self, v: u32) {
        self.raw(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.raw(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.raw(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.raw(&v.to_le_bytes());
    }

    fn guid(&mut self, g: &FGuid) {
        // FGuid's a/b/c/d fields are private with no accessors; round-trip
        // through its Display/parse_str (verified exact) to recover them.
        let s = g.to_string().replace('-', "");
        let word = |start: usize| u32::from_str_radix(&s[start..start + 8], 16).unwrap();
        self.u32(word(0));
        self.u32(word(8));
        self.u32(word(16));
        self.u32(word(24));
    }

    fn fstring(&mut self, s: &FStringRaw) {
        match s {
            FStringRaw::Empty => self.i32(0),
            FStringRaw::Ascii(data) => {
                self.i32(data.len() as i32 + 1);
                self.raw(data);
                self.u8(0);
            }
            FStringRaw::Utf16(data) => {
                self.i32(-(data.len() as i32 / 2 + 1));
                self.raw(data);
                self.raw(&[0, 0]);
            }
        }
    }

    fn tarray<T>(&mut self, items: &[T], mut write: impl FnMut(&mut Self, &T)) {
        self.u32(items.len() as u32);
        for item in items {
            write(self, item);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceHandle {
    pub guid: FGuid,
    pub instance_id: FGuid,
}

fn read_instance_handle(r: &mut Reader) -> Result<InstanceHandle> {
    Ok(InstanceHandle {
        guid: r.guid()?,
        instance_id: r.guid()?,
    })
}

fn write_instance_handle(w: &mut Writer, h: &InstanceHandle) {
    w.guid(&h.guid);
    w.guid(&h.instance_id);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub player_uid: FGuid,
    pub last_online_real_time: i64,
    pub player_name: FStringRaw,
}

fn read_player_info(r: &mut Reader) -> Result<PlayerInfo> {
    Ok(PlayerInfo {
        player_uid: r.guid()?,
        last_online_real_time: r.i64()?,
        player_name: r.fstring()?,
    })
}

fn write_player_info(w: &mut Writer, p: &PlayerInfo) {
    w.guid(&p.player_uid);
    w.i64(p.last_online_real_time);
    w.fstring(&p.player_name);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildPlayerInfo {
    pub info: PlayerInfo,
    /// `EPalGuildRole`
    pub role: u8,
}

fn read_guild_player_info(r: &mut Reader) -> Result<GuildPlayerInfo> {
    Ok(GuildPlayerInfo {
        info: read_player_info(r)?,
        role: r.u8()?,
    })
}

fn write_guild_player_info(w: &mut Writer, p: &GuildPlayerInfo) {
    write_player_info(w, &p.info);
    w.u8(p.role);
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuildMarker {
    pub marker_id: FGuid,
    pub icon_location: (f64, f64, f64),
    pub icon_type: i32,
    pub owner_player_uid: FGuid,
}

fn read_guild_marker(r: &mut Reader) -> Result<GuildMarker> {
    Ok(GuildMarker {
        marker_id: r.guid()?,
        icon_location: (r.f64()?, r.f64()?, r.f64()?),
        icon_type: r.i32()?,
        owner_player_uid: r.guid()?,
    })
}

fn write_guild_marker(w: &mut Writer, m: &GuildMarker) {
    w.guid(&m.marker_id);
    w.f64(m.icon_location.0);
    w.f64(m.icon_location.1);
    w.f64(m.icon_location.2);
    w.i32(m.icon_type);
    w.guid(&m.owner_player_uid);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePermission {
    /// `EPalGuildRole`
    pub role: u8,
    /// `EPalGuildPermission` values
    pub permissions: Vec<u8>,
}

fn read_role_permission(r: &mut Reader) -> Result<RolePermission> {
    Ok(RolePermission {
        role: r.u8()?,
        permissions: r.tarray(|r| r.u8())?,
    })
}

fn write_role_permission(w: &mut Writer, p: &RolePermission) {
    w.u8(p.role);
    w.tarray(&p.permissions, |w, v| w.u8(*v));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildTail {
    /// Guild tail before the 2026-07 update.
    V1 {
        admin_player_uid: FGuid,
        players: Vec<PlayerInfo>,
        trailing_bytes: [u8; 4],
    },
    /// Guild tail as of the 2026-07 update: chest roles, per-player role,
    /// permissions.
    V2 {
        guild_chest_allowed_roles: Vec<u8>,
        unknown_i32: i32,
        admin_player_uid: FGuid,
        players: Vec<GuildPlayerInfo>,
        role_permissions: Vec<RolePermission>,
        trailing_bytes: [u8; 4],
    },
}

fn read_guild_tail_v1(r: &mut Reader) -> Result<GuildTail> {
    Ok(GuildTail::V1 {
        admin_player_uid: r.guid()?,
        players: r.tarray(read_player_info)?,
        trailing_bytes: r.byte_array()?,
    })
}

fn read_guild_tail_v2(r: &mut Reader) -> Result<GuildTail> {
    Ok(GuildTail::V2 {
        guild_chest_allowed_roles: r.tarray(|r| r.u8())?,
        unknown_i32: r.i32()?,
        admin_player_uid: r.guid()?,
        players: r.tarray(read_guild_player_info)?,
        role_permissions: r.tarray(read_role_permission)?,
        trailing_bytes: r.byte_array()?,
    })
}

/// There is no version flag in the blob, so the two layouts are told apart
/// by trial. Landing precisely on EOF is the discriminator; a v2 attempt
/// that over- or under-reads is rejected and v1 is tried instead.
fn read_guild_tail(data: &[u8], start: usize) -> Result<GuildTail> {
    let mut r = Reader::new(data);
    r.cursor.set_position(start as u64);
    if let Ok(tail) = read_guild_tail_v2(&mut r) {
        if r.eof() {
            return Ok(tail);
        }
    }
    let mut r = Reader::new(data);
    r.cursor.set_position(start as u64);
    read_guild_tail_v1(&mut r)
}

fn write_guild_tail(w: &mut Writer, tail: &GuildTail) {
    match tail {
        GuildTail::V1 {
            admin_player_uid,
            players,
            trailing_bytes,
        } => {
            w.guid(admin_player_uid);
            w.tarray(players, write_player_info);
            w.raw(trailing_bytes);
        }
        GuildTail::V2 {
            guild_chest_allowed_roles,
            unknown_i32,
            admin_player_uid,
            players,
            role_permissions,
            trailing_bytes,
        } => {
            w.tarray(guild_chest_allowed_roles, |w, v| w.u8(*v));
            w.i32(*unknown_i32);
            w.guid(admin_player_uid);
            w.tarray(players, write_guild_player_info);
            w.tarray(role_permissions, write_role_permission);
            w.raw(trailing_bytes);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroupRawData {
    Guild {
        group_id: FGuid,
        group_name: FStringRaw,
        individual_character_handle_ids: Vec<InstanceHandle>,
        org_type: u8,
        leading_bytes: [u8; 4],
        base_ids: Vec<FGuid>,
        unknown_1: i32,
        base_camp_level: i32,
        map_object_instance_ids_base_camp_points: Vec<FGuid>,
        guild_name: FStringRaw,
        last_guild_name_modifier_player_uid: FGuid,
        guild_markers: Vec<GuildMarker>,
        tail: GuildTail,
    },
    IndependentGuild {
        group_id: FGuid,
        group_name: FStringRaw,
        individual_character_handle_ids: Vec<InstanceHandle>,
        org_type: u8,
        base_camp_level: i32,
        map_object_instance_ids_base_camp_points: Vec<FGuid>,
        guild_name: FStringRaw,
        player_uid: FGuid,
        guild_name_2: FStringRaw,
        last_online_real_time: i64,
        player_name: FStringRaw,
    },
    Organization {
        group_id: FGuid,
        group_name: FStringRaw,
        individual_character_handle_ids: Vec<InstanceHandle>,
        org_type: u8,
        trailing_bytes: [u8; 12],
    },
}

const GUILD: &str = "EPalGroupType::Guild";
const INDEPENDENT_GUILD: &str = "EPalGroupType::IndependentGuild";
const ORGANIZATION: &str = "EPalGroupType::Organization";

pub fn decode_group_raw_data(group_bytes: &[u8], group_type: &str) -> Result<GroupRawData> {
    let mut r = Reader::new(group_bytes);
    let group_id = r.guid()?;
    let group_name = r.fstring()?;
    let individual_character_handle_ids = r.tarray(read_instance_handle)?;

    let data = match group_type {
        GUILD => {
            let org_type = r.u8()?;
            let leading_bytes = r.byte_array()?;
            let base_ids = r.tarray(|r| r.guid())?;
            let unknown_1 = r.i32()?;
            let base_camp_level = r.i32()?;
            let map_object_instance_ids_base_camp_points = r.tarray(|r| r.guid())?;
            let guild_name = r.fstring()?;
            let last_guild_name_modifier_player_uid = r.guid()?;
            let guild_markers = r.tarray(read_guild_marker)?;
            let tail_start = r.cursor.position() as usize;
            let tail = read_guild_tail(group_bytes, tail_start)?;
            // read_guild_tail re-parses from `group_bytes`/`tail_start`
            // independently (it needs to be able to rewind past a failed v2
            // attempt); advance our own cursor to match so the final EOF
            // check below is accurate.
            r.cursor.set_position(group_bytes.len() as u64);
            GroupRawData::Guild {
                group_id,
                group_name,
                individual_character_handle_ids,
                org_type,
                leading_bytes,
                base_ids,
                unknown_1,
                base_camp_level,
                map_object_instance_ids_base_camp_points,
                guild_name,
                last_guild_name_modifier_player_uid,
                guild_markers,
                tail,
            }
        }
        INDEPENDENT_GUILD => {
            let org_type = r.u8()?;
            let base_camp_level = r.i32()?;
            let map_object_instance_ids_base_camp_points = r.tarray(|r| r.guid())?;
            let guild_name = r.fstring()?;
            let player_uid = r.guid()?;
            let guild_name_2 = r.fstring()?;
            let last_online_real_time = r.i64()?;
            let player_name = r.fstring()?;
            GroupRawData::IndependentGuild {
                group_id,
                group_name,
                individual_character_handle_ids,
                org_type,
                base_camp_level,
                map_object_instance_ids_base_camp_points,
                guild_name,
                player_uid,
                guild_name_2,
                last_online_real_time,
                player_name,
            }
        }
        ORGANIZATION => {
            let org_type = r.u8()?;
            let trailing_bytes = r.byte_array()?;
            GroupRawData::Organization {
                group_id,
                group_name,
                individual_character_handle_ids,
                org_type,
                trailing_bytes,
            }
        }
        other => return Err(GroupRawDataError::UnknownGroupType(other.to_string())),
    };

    if !r.eof() {
        return Err(GroupRawDataError::NotFullyConsumed {
            remaining: r.remaining(),
        });
    }
    Ok(data)
}

pub fn encode_group_raw_data(data: &GroupRawData) -> Vec<u8> {
    let mut w = Writer::new();
    match data {
        GroupRawData::Guild {
            group_id,
            group_name,
            individual_character_handle_ids,
            org_type,
            leading_bytes,
            base_ids,
            unknown_1,
            base_camp_level,
            map_object_instance_ids_base_camp_points,
            guild_name,
            last_guild_name_modifier_player_uid,
            guild_markers,
            tail,
        } => {
            w.guid(group_id);
            w.fstring(group_name);
            w.tarray(individual_character_handle_ids, write_instance_handle);
            w.u8(*org_type);
            w.raw(leading_bytes);
            w.tarray(base_ids, |w, g| w.guid(g));
            w.i32(*unknown_1);
            w.i32(*base_camp_level);
            w.tarray(map_object_instance_ids_base_camp_points, |w, g| w.guid(g));
            w.fstring(guild_name);
            w.guid(last_guild_name_modifier_player_uid);
            w.tarray(guild_markers, write_guild_marker);
            write_guild_tail(&mut w, tail);
        }
        GroupRawData::IndependentGuild {
            group_id,
            group_name,
            individual_character_handle_ids,
            org_type,
            base_camp_level,
            map_object_instance_ids_base_camp_points,
            guild_name,
            player_uid,
            guild_name_2,
            last_online_real_time,
            player_name,
        } => {
            w.guid(group_id);
            w.fstring(group_name);
            w.tarray(individual_character_handle_ids, write_instance_handle);
            w.u8(*org_type);
            w.i32(*base_camp_level);
            w.tarray(map_object_instance_ids_base_camp_points, |w, g| w.guid(g));
            w.fstring(guild_name);
            w.guid(player_uid);
            w.fstring(guild_name_2);
            w.i64(*last_online_real_time);
            w.fstring(player_name);
        }
        GroupRawData::Organization {
            group_id,
            group_name,
            individual_character_handle_ids,
            org_type,
            trailing_bytes,
        } => {
            w.guid(group_id);
            w.fstring(group_name);
            w.tarray(individual_character_handle_ids, write_instance_handle);
            w.u8(*org_type);
            w.raw(trailing_bytes);
        }
    }
    w.bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Synthetic fixtures authored with the Python reference's own
    // group.encode_bytes() against fake data -- not real save data. The
    // IndependentGuild fixture was hand-built instead (see module doc
    // comment: encode_bytes is broken for that type upstream).
    const ORGANIZATION_FIXTURE: &str = "00000010000000000000000001000000080000004f72674e616d650001000000000000200000000000000000020000000000003000000000000000000300000001000102030405060708090a0b";
    const GUILD_V1_FIXTURE: &str = "000000400000000000000000040000000c0000004775696c644e616d65563100010000000000005000000000000000000500000000000060000000000000000006000000020909090901000000000000700000000000000000070000000b000000160000000100000000000080000000000000000008000000110000004775696c644e616d655631496e6e6572000000009000000000000000000900000000000000000000a000000000000000000a00000001000000000000b000000000000000000b00000015cd5b070000000006000000416c6963650001010101";
    const GUILD_V2_FIXTURE: &str = "000000400000000000000000040000000c0000004775696c644e616d65563100010000000000005000000000000000000500000000000060000000000000000006000000020909090901000000000000700000000000000000070000000b000000160000000100000000000080000000000000000008000000110000004775696c644e616d655632496e6e6572000000009000000000000000000900000001000000000000c000000000000000000c000000000000000000f83f00000000000004c0000000000000084004000000000000d000000000000000000d0000000300000000010263000000000000a000000000000000000a00000001000000000000b000000000000000000b00000015cd5b070000000006000000416c6963650003020000000003000000010203010000000001010101";
    const INDEPENDENT_GUILD_FIXTURE: &str = "000000100000000000000000010000000f000000496e64696547726f75704e616d6500000000000503000000000000000a000000496e6469654e616d6500000000f000000000000000000f00000010000000496e6469654775696c644e616d653200b168de3a0000000004000000426f6200";

    #[test]
    fn roundtrips_organization() {
        let fixture = hex_decode(ORGANIZATION_FIXTURE);
        let decoded = decode_group_raw_data(&fixture, ORGANIZATION).unwrap();
        assert!(matches!(decoded, GroupRawData::Organization { .. }));
        assert_eq!(encode_group_raw_data(&decoded), fixture);
    }

    #[test]
    fn roundtrips_guild_v1() {
        let fixture = hex_decode(GUILD_V1_FIXTURE);
        let decoded = decode_group_raw_data(&fixture, GUILD).unwrap();
        match &decoded {
            GroupRawData::Guild { tail, .. } => assert!(matches!(tail, GuildTail::V1 { .. })),
            _ => panic!("expected Guild"),
        }
        assert_eq!(encode_group_raw_data(&decoded), fixture);
    }

    #[test]
    fn roundtrips_guild_v2() {
        let fixture = hex_decode(GUILD_V2_FIXTURE);
        let decoded = decode_group_raw_data(&fixture, GUILD).unwrap();
        match &decoded {
            GroupRawData::Guild {
                tail,
                guild_markers,
                ..
            } => {
                assert!(matches!(tail, GuildTail::V2 { .. }));
                assert_eq!(guild_markers.len(), 1);
            }
            _ => panic!("expected Guild"),
        }
        assert_eq!(encode_group_raw_data(&decoded), fixture);
    }

    #[test]
    fn roundtrips_independent_guild() {
        let fixture = hex_decode(INDEPENDENT_GUILD_FIXTURE);
        let decoded = decode_group_raw_data(&fixture, INDEPENDENT_GUILD).unwrap();
        assert!(matches!(decoded, GroupRawData::IndependentGuild { .. }));
        assert_eq!(encode_group_raw_data(&decoded), fixture);
    }

    #[test]
    fn rejects_unknown_group_type() {
        let fixture = hex_decode(ORGANIZATION_FIXTURE);
        let err = decode_group_raw_data(&fixture, "EPalGroupType::Nonsense").unwrap_err();
        assert!(matches!(err, GroupRawDataError::UnknownGroupType(_)));
    }

    #[test]
    fn rejects_truncated_input() {
        let fixture = hex_decode(ORGANIZATION_FIXTURE);
        let truncated = &fixture[..fixture.len() - 5];
        assert!(decode_group_raw_data(truncated, ORGANIZATION).is_err());
    }

    #[test]
    fn rejects_trailing_garbage() {
        let mut fixture = hex_decode(ORGANIZATION_FIXTURE);
        fixture.push(0xff);
        let err = decode_group_raw_data(&fixture, ORGANIZATION).unwrap_err();
        assert!(matches!(
            err,
            GroupRawDataError::NotFullyConsumed { remaining: 1 }
        ));
    }
}
