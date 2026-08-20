//! SteamID64 -> Palworld `PlayerUId` conversion.
//!
//! Reverse-engineered from the minified client-side JS served by
//! hub.tcno.co's Palworld save converter
//! (<https://hub.tcno.co/games/palworld/converter/>), authored by Wesley
//! Pyburn ([@TCNOco](https://github.com/TCNOco)). That JS is not itself
//! attributed to any open-source project or license, so this is an
//! independent reimplementation derived from observing its behavior, not a
//! port of its source.

use std::fmt;

#[derive(Debug)]
pub enum SteamIdError {
    InvalidInput,
    VanityUrlUnsupported,
}

impl fmt::Display for SteamIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SteamIdError::InvalidInput => write!(
                f,
                "Enter a 17-digit SteamID64 or a steamcommunity.com/profiles URL."
            ),
            SteamIdError::VanityUrlUnsupported => write!(
                f,
                "Use a numeric https://steamcommunity.com/profiles/<SteamID64> URL. Vanity /id/ URLs cannot be converted offline."
            ),
        }
    }
}

impl std::error::Error for SteamIdError {}

fn is_steam_id64(s: &str) -> bool {
    s.len() == 17 && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn parse_steam_id64(input: &str) -> Result<String, SteamIdError> {
    let trimmed = input.trim();
    if is_steam_id64(trimmed) {
        return Ok(trimmed.to_string());
    }

    let rest = trimmed
        .strip_prefix("https://")
        .ok_or(SteamIdError::InvalidInput)?;

    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = if let Some(stripped) = host.strip_prefix("www.") {
        stripped.to_string()
    } else {
        host
    };

    let id = path.strip_prefix("profiles/").unwrap_or("");
    let id = id.strip_suffix('/').unwrap_or(id);

    if host == "steamcommunity.com" && is_steam_id64(id) {
        Ok(id.to_string())
    } else {
        Err(SteamIdError::VanityUrlUnsupported)
    }
}

fn utf16le_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for unit in s.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

pub fn steam_id64_to_palworld_player_id(input: &str) -> Result<String, SteamIdError> {
    let steam_id = parse_steam_id64(input)?;
    let bytes = utf16le_bytes(&steam_id);
    let hash: u64 = cityhasher::hash(&bytes);
    let folded = hash.wrapping_add((hash >> 32).wrapping_mul(23)) as u32;
    Ok(format!("{folded:08X}{}", "0".repeat(24)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_steam_ids() {
        assert_eq!(
            steam_id64_to_palworld_player_id("76561198012345678").unwrap(),
            "46CED3EE000000000000000000000000"
        );
        assert_eq!(
            steam_id64_to_palworld_player_id("76561197960287930").unwrap(),
            "3E22F785000000000000000000000000"
        );
        assert_eq!(
            steam_id64_to_palworld_player_id("76561198000000000").unwrap(),
            "A5D406B9000000000000000000000000"
        );
    }

    #[test]
    fn accepts_profile_url() {
        assert_eq!(
            steam_id64_to_palworld_player_id(
                "https://steamcommunity.com/profiles/76561198012345678"
            )
            .unwrap(),
            "46CED3EE000000000000000000000000"
        );
    }

    #[test]
    fn accepts_www_profile_url() {
        assert_eq!(
            steam_id64_to_palworld_player_id(
                "https://www.steamcommunity.com/profiles/76561198012345678/"
            )
            .unwrap(),
            "46CED3EE000000000000000000000000"
        );
    }

    #[test]
    fn rejects_vanity_url() {
        assert!(matches!(
            steam_id64_to_palworld_player_id("https://steamcommunity.com/id/somevanity"),
            Err(SteamIdError::VanityUrlUnsupported)
        ));
    }

    #[test]
    fn rejects_garbage_input() {
        assert!(matches!(
            steam_id64_to_palworld_player_id("not a steam id"),
            Err(SteamIdError::InvalidInput)
        ));
    }
}
