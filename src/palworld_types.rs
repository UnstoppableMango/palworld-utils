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
//! Rust analog of this table.
//!
//! `uesave`'s path convention only matches Python's dot-joined paths
//! (minus the leading `.`) up to a point: both push `"Key"`/`"Value"` onto
//! the scope for a map's *own* key/value type resolution, but Python's
//! paths additionally carry that segment (or a repeated container-name
//! segment) into deeper nested lookups within the resolved value struct,
//! while `uesave` pops it immediately after resolving the map itself and
//! continues with plain `.<field name>` from there. For most entries this
//! is harmless -- `uesave`'s own defaults (`Guid` for keys, `Struct(None)`
//! for values) happen to already match what the hint would have said. It
//! stopped being harmless for two `Key` positions, confirmed against a
//! real save (see [`EXTRA_TYPE_HINTS`]): `uesave` defaulted them to `Guid`,
//! but they're really generic structs, which desynced parsing of
//! everything after them in the same containing struct and cascaded into
//! `worldSaveData` silently falling back to opaque raw bytes.
//!
//! Consumed by `inspect.rs`; the custom `RawData` binary decoders (guild
//! membership, character params -- Python's `PALWORLD_CUSTOM_PROPERTIES`)
//! that would let us fully interpret those properties haven't been ported
//! yet.

use uesave::{StructType, Types};

use crate::palworld_type_hints::PALWORLD_TYPE_HINTS;

/// Corrections/additions beyond the generated table -- see the module doc
/// comment. `InLockerCharacterInstanceIDArray` isn't in Python's own hint
/// table at all (its elements' real shape, `{PlayerUId, InstanceId,
/// DebugName}`, was confirmed directly against a real save).
const EXTRA_TYPE_HINTS: &[(&str, &str)] = &[
    (
        "worldSaveData.FoliageGridSaveDataMap.ModelMap.InstanceDataMap.Key",
        "StructProperty",
    ),
    (
        "worldSaveData.InLockerCharacterInstanceIDArray",
        "StructProperty",
    ),
];

fn struct_type(hint: &str) -> StructType {
    match hint {
        "Guid" => StructType::Guid,
        "StructProperty" => StructType::Struct(None),
        other => unreachable!("unknown Palworld type hint: {other:?}"),
    }
}

pub fn palworld_types() -> Types {
    let mut types = Types::new();
    for (path, hint) in PALWORLD_TYPE_HINTS.iter().chain(EXTRA_TYPE_HINTS) {
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

    #[test]
    fn extra_hints_are_struct_property() {
        // Both corrections are Key-position overrides of uesave's Guid
        // default -- see the module doc comment.
        for (path, hint) in EXTRA_TYPE_HINTS {
            assert_eq!(*hint, "StructProperty", "unexpected hint for {path:?}");
        }
    }
}
