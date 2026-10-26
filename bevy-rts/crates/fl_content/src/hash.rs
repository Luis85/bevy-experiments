//! The rules hash: one number that identifies every sim-relevant piece of loaded content.
//!
//! Two machines may play together, or replay each other's games, only with the same hash.
//! It is xxh64 (seed 0) over [`RULES_HASH_FORMAT`] (little-endian) followed by the postcard
//! bytes of a canonical tree (ADR-0007). Postcard writes `Fx` and `Angle` as their raw
//! integers, so every bit of a value counts, while file formatting and field order in the
//! source files do not. Hashed content types must not use `#[serde(flatten)]`.

use std::collections::BTreeMap;

use serde::Serialize;
use xxhash_rust::xxh64::xxh64;

use crate::db::ContentDb;
use crate::def::DefComponent;

/// The version of the canonical tree's layout. Bump it whenever the tree's shape changes,
/// so hashes from different layouts never compare equal by accident.
pub const RULES_HASH_FORMAT: u32 = 1;

/// One concrete unit: `(id, name, tags, components by name)`.
type CanonicalUnit<'a> = (
    &'a str,
    &'a str,
    &'a [String],
    BTreeMap<&'a str, &'a dyn DefComponent>,
);

/// Everything the rules hash covers, in canonical order.
#[derive(Serialize)]
struct Canonical<'a> {
    /// `(id, version)` in load order.
    mods: Vec<(&'a str, &'a str)>,
    /// Concrete units sorted by id.
    units: Vec<CanonicalUnit<'a>>,
    /// Rules-hashed kinds sorted by extension, each with its items' postcard bytes in id order.
    kinds: Vec<(&'static str, Vec<Vec<u8>>)>,
    /// Rules sections sorted by name, with their postcard bytes.
    rules: Vec<(&'static str, Vec<u8>)>,
}

fn postcard_bytes<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("loaded content serializes to postcard")
}

/// The canonical tree of `db`, built from the database's sorted tables.
fn canonical(db: &ContentDb) -> Canonical<'_> {
    let mods = db
        .mods()
        .iter()
        .map(|m| (m.id.as_str(), m.version.as_str()))
        .collect();
    let units = db
        .units()
        .iter()
        .map(|unit| {
            let components = unit
                .components
                .iter()
                .map(|(name, component)| (name.as_str(), &**component))
                .collect();
            (
                unit.id.as_str(),
                unit.name.as_str(),
                unit.tags.as_slice(),
                components,
            )
        })
        .collect();
    let kinds = db
        .registry()
        .kind_entries()
        .filter(|entry| entry.in_rules_hash)
        .map(|entry| {
            let items = db.kinds[entry.ext]
                .erased_items()
                .map(postcard_bytes)
                .collect();
            (entry.ext, items)
        })
        .collect();
    let rules = db
        .rules
        .iter()
        .map(|(name, section)| (*name, postcard_bytes(section.as_ref())))
        .collect();
    Canonical {
        mods,
        units,
        kinds,
        rules,
    }
}

/// xxh64 (seed 0) over `RULES_HASH_FORMAT.to_le_bytes()` followed by the postcard bytes of
/// the canonical tree.
pub(crate) fn rules_hash(db: &ContentDb) -> u64 {
    let mut bytes = RULES_HASH_FORMAT.to_le_bytes().to_vec();
    bytes.extend(postcard_bytes(&canonical(db)));
    xxh64(&bytes, 0)
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::path::PathBuf;
    use std::sync::Arc;

    use bevy_ecs::world::EntityWorldMut;
    use fl_math::Fx;
    use serde::Deserialize;

    use super::*;
    use crate::def::SpawnCtx;
    use crate::inherit::ResolvedObject;
    use crate::registry::DefRegistry;

    #[derive(Debug, Serialize, Deserialize)]
    struct Big(Fx);

    impl DefComponent for Big {
        fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn hash_with_big(raw: i64) -> u64 {
        let mut registry = DefRegistry::default();
        registry.register_component::<Big>("Big");
        let mut db = ContentDb::empty(registry);
        let big: Arc<dyn DefComponent> = Arc::new(Big(Fx::from_raw(raw)));
        db.units.push(ResolvedObject {
            kind: "unit",
            id: "t.big".into(),
            name: "Big".into(),
            tags: Vec::new(),
            components: BTreeMap::from([("Big".to_owned(), big)]),
            file: PathBuf::from("big.unit.ron"),
        });
        rules_hash(&db)
    }

    #[test]
    fn rules_hash_sees_low_bits_at_large_magnitude() {
        assert_ne!(hash_with_big((1 << 53) | 1), hash_with_big((1 << 53) | 2));
        // 2^22 + 2^-32 and 2^22 + 2^-31 round to the same f64, so a text hash of the
        // human-readable form cannot tell these two apart.
        assert_ne!(hash_with_big((1 << 54) | 1), hash_with_big((1 << 54) | 2));
    }
}
