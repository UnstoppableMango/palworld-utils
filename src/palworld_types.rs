//! Palworld's GVAS struct-type hints for `uesave`.
//!
//! `PALWORLD_TYPE_HINTS` (in `palworld_type_hints.rs`) is generated from
//! `deafdudecomputers/PalworldSaveTools`
//! (<https://github.com/deafdudecomputers/PalworldSaveTools/blob/main/src/palsav/palsav/paltypes.py>,
//! MIT), itself a fork of the community `palworld-save-tools` -- run `make
//! gen-palworld-types` to regenerate it. `uesave` (our GVAS crate) has no
//! per-path custom-parser hook of its own -- its only type-disambiguation
//! mechanism is a [`uesave::Types`] map used to resolve the key/value
//! struct type of `MapProperty`/`SetProperty` entries, which is the direct
//! Rust analog of this table. `uesave`'s path convention matches Python's
//! dot-joined property paths (minus the leading `.`), and it pushes
//! `"Key"`/`"Value"` onto the scope for map key/value lookups the same way
//! Python's `.Key`/`.Value` suffixes do.
//!
//! This is data-only groundwork: it isn't wired into a CLI command yet,
//! since the custom `RawData` binary decoders it depends on (guild
//! membership, character params -- Python's `PALWORLD_CUSTOM_PROPERTIES`)
//! haven't been ported.

// Not wired into the CLI yet -- see module doc comment.
#![allow(dead_code)]

use uesave::{StructType, Types};

use crate::palworld_type_hints::PALWORLD_TYPE_HINTS;

fn struct_type(hint: &str) -> StructType {
    match hint {
        "Guid" => StructType::Guid,
        "StructProperty" => StructType::Struct(None),
        other => unreachable!("unknown Palworld type hint: {other:?}"),
    }
}

pub fn palworld_types() -> Types {
    let mut types = Types::new();
    for (path, hint) in PALWORLD_TYPE_HINTS {
        types.add((*path).to_string(), struct_type(hint));
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_all_known_hints() {
        assert_eq!(PALWORLD_TYPE_HINTS.len(), 40);
    }

    #[test]
    fn hint_values_are_known() {
        for (path, hint) in PALWORLD_TYPE_HINTS {
            assert!(
                *hint == "StructProperty" || *hint == "Guid",
                "unexpected hint {hint:?} for path {path:?}"
            );
        }
    }

    #[test]
    fn spot_check_entries() {
        let map: std::collections::HashMap<_, _> = PALWORLD_TYPE_HINTS.iter().copied().collect();
        assert_eq!(map["worldSaveData.GroupSaveDataMap.Key"], "Guid");
        assert_eq!(
            map["worldSaveData.GroupSaveDataMap.Value"],
            "StructProperty"
        );
        assert_eq!(
            map["worldSaveData.CharacterSaveParameterMap.Key"],
            "StructProperty"
        );
        assert_eq!(
            map["worldSaveData.CharacterSaveParameterMap.Value"],
            "StructProperty"
        );
        assert_eq!(map["worldSaveData.BaseCampSaveData.Key"], "Guid");
    }

    #[test]
    fn builds_types_without_panicking() {
        let _ = palworld_types();
    }
}
