//! The state hash: one xxh64 over all hashed sim state (spec §5.4, ADR-0004, ADR-0006).
//!
//! Two runs that agree on every hash agree on the state they hashed. Tests compare hashes
//! now; multiplayer and replays compare them later to find the first desynced tick.
//! [`step_tick`](crate::step_tick) computes it every [`HashInterval`](crate::HashInterval)
//! ticks. Registry-backed ids are hashed as their stable name key
//! ([`StateHasher::write_registry_id`]), never as the dense id, so content or features that
//! add names sorting earlier do not change the hash of an unrelated match.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use fl_content::RulesSection;
use fl_math::{Angle, Fx, FxVec2, stream_key};
use fl_protocol::{PlayerId, SimId, StringTables, Tick, UnitTypeId, tables};
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh64::Xxh64;

use crate::builder::SimBuilder;
use crate::classify::StateRegistry;
use crate::clock::SimClock;
use crate::ids::{SimIdAllocator, SimIndex};

/// The name key (`fl_math::stream_key(name)`) of every name of every string table, by table
/// and dense id.
#[derive(Debug, Default)]
pub(crate) struct NameKeys {
    tables: BTreeMap<String, Vec<u64>>,
}

impl NameKeys {
    pub(crate) fn new(tables: &StringTables) -> Self {
        NameKeys {
            tables: tables
                .tables
                .iter()
                .map(|(table, names)| {
                    (table.clone(), names.iter().map(|n| stream_key(n)).collect())
                })
                .collect(),
        }
    }

    fn key(&self, table: &str, id: u32) -> Option<u64> {
        let keys = self.tables.get(table)?;
        keys.get(usize::try_from(id).ok()?).copied()
    }
}

/// The name keys of the running match's string tables (content and every registry), which
/// [`compute_state_hash`] hands to its [`StateHasher`]. External `"core.hash_tables"`.
#[derive(Resource, Clone, Debug)]
pub(crate) struct HashTables(pub(crate) Arc<NameKeys>);

/// A streaming xxh64 (seed 0) over little-endian values, with the match's string tables as
/// context for [`write_registry_id`](Self::write_registry_id).
pub struct StateHasher {
    inner: Xxh64,
    tables: Option<Arc<NameKeys>>,
}

impl Default for StateHasher {
    fn default() -> Self {
        StateHasher {
            inner: Xxh64::new(0),
            tables: None,
        }
    }
}

impl StateHasher {
    /// A hasher without string tables: [`write_registry_id`](Self::write_registry_id) panics.
    pub fn new() -> Self {
        Self::default()
    }

    /// A hasher that resolves registry ids through `tables`.
    pub(crate) fn with_tables(tables: Arc<NameKeys>) -> Self {
        StateHasher {
            inner: Xxh64::new(0),
            tables: Some(tables),
        }
    }

    /// The stable name key of dense id `id` in string table `table` (such as
    /// `fl_protocol::tables::UNITS`), never the dense id itself.
    ///
    /// # Panics
    /// If the hasher has no string tables (it was not made by [`compute_state_hash`]), or
    /// `table` has no name with dense id `id`.
    pub fn write_registry_id(&mut self, table: &'static str, id: u32) {
        let tables = self.tables.as_deref().unwrap_or_else(|| {
            panic!(
                "StateHasher has no string tables to resolve id {id} of '{table}' \
                 (hash through compute_state_hash)"
            )
        });
        let key = tables
            .key(table, id)
            .unwrap_or_else(|| panic!("string table '{table}' has no id {id}"));
        self.write_u64(key);
    }

    pub fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }

    pub fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_bool(&mut self, v: bool) {
        self.write_u8(u8::from(v));
    }

    /// The raw fixed-point bits.
    pub fn write_fx(&mut self, v: Fx) {
        self.write_i64(v.raw());
    }

    /// Raw bytes, with no length prefix: write the length first for variable-size data.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    /// The hash of everything written so far.
    pub fn finish(&self) -> u64 {
        self.inner.digest()
    }
}

/// Sim state that feeds the state hash. Implementations write every field that affects
/// the simulation, in a fixed order.
pub trait SimHash {
    fn sim_hash(&self, h: &mut StateHasher);
}

macro_rules! sim_hash_via {
    ($($ty:ty => $write:ident),* $(,)?) => {
        $(impl SimHash for $ty {
            fn sim_hash(&self, h: &mut StateHasher) {
                h.$write(*self);
            }
        })*
    };
}

sim_hash_via!(u8 => write_u8, u16 => write_u16, u32 => write_u32, u64 => write_u64,
    i64 => write_i64, bool => write_bool, Fx => write_fx);

impl SimHash for i32 {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_bytes(&self.to_le_bytes());
    }
}

impl SimHash for FxVec2 {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_fx(self.x);
        h.write_fx(self.y);
    }
}

impl SimHash for Angle {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u16(self.raw());
    }
}

impl SimHash for SimId {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u32(self.0);
    }
}

impl SimHash for PlayerId {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u8(self.0);
    }
}

/// The unit's name key in [`tables::UNITS`], never the dense id.
impl SimHash for UnitTypeId {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_registry_id(tables::UNITS, u32::from(self.0));
    }
}

impl SimHash for Tick {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u32(self.0);
    }
}

impl SimHash for SimClock {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.tick.sim_hash(h);
    }
}

impl SimHash for SimIdAllocator {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u32(self.issued());
    }
}

/// `0`, or `1` followed by the value.
impl<T: SimHash> SimHash for Option<T> {
    fn sim_hash(&self, h: &mut StateHasher) {
        match self {
            None => h.write_u8(0),
            Some(v) => {
                h.write_u8(1);
                v.sim_hash(h);
            }
        }
    }
}

/// The length as a `u64`, then each element.
impl<T: SimHash> SimHash for Vec<T> {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u64(self.len() as u64);
        for v in self {
            v.sim_hash(h);
        }
    }
}

/// The state hash of `world`:
/// 1. the tick;
/// 2. the number of `Hashed` resources the world has, then each of them in name order: its
///    name key, then its data;
/// 3. the entity count;
/// 4. per [`SimIndex`] entry (ascending [`SimId`]): the id, the number of `Hashed` components
///    the entity has, then each of them in name order: its name key, then its data.
///
/// Absent state writes nothing, so registering a type that the match never uses (a feature
/// it does not exercise) leaves its hashes unchanged; the counts keep the encoding
/// prefix-free (ADR-0004). Registry-backed ids inside the data are hashed as name keys. Bevy
/// `Entity` values never enter it.
///
/// # Panics
/// If `world` was not built by [`build_sim_app`].
///
/// [`build_sim_app`]: crate::build_sim_app
pub fn compute_state_hash(world: &World) -> u64 {
    let mut h = match world.get_resource::<HashTables>() {
        Some(tables) => StateHasher::with_tables(Arc::clone(&tables.0)),
        None => StateHasher::new(),
    };
    world.resource::<SimClock>().tick.sim_hash(&mut h);
    let state = world.resource::<StateRegistry>();
    let resources: Vec<_> = state
        .resources
        .iter()
        .filter_map(|(id, _, entry)| {
            let codec = entry.codec.as_deref()?;
            codec.contains(world).then(|| {
                let key = state
                    .resources
                    .name_key(id)
                    .expect("iter yields registered ids");
                (key, codec)
            })
        })
        .collect();
    h.write_u64(resources.len() as u64);
    for (key, codec) in resources {
        codec.hash(world, key, &mut h);
    }
    let index = world.resource::<SimIndex>();
    h.write_u64(index.len() as u64);
    let codecs: Vec<_> = state
        .components
        .iter()
        .filter_map(|(id, _, entry)| {
            let key = state
                .components
                .name_key(id)
                .expect("iter yields registered ids");
            Some((key, entry.codec.as_deref()?))
        })
        .collect();
    for (id, entity) in index.iter() {
        id.sim_hash(&mut h);
        let present = codecs
            .iter()
            .filter(|(_, codec)| codec.contains(world, entity))
            .count();
        h.write_u64(present as u64);
        for &(key, codec) in &codecs {
            codec.hash(world, entity, key, &mut h);
        }
    }
    h.finish()
}

/// `rules.ron` section `"StateHash"`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StateHashRules {
    /// The default [`HashInterval`](crate::HashInterval): hash every `interval_ticks` ticks.
    pub interval_ticks: u32,
}

impl RulesSection for StateHashRules {}

/// The `"StateHash"` rules section.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_rules_section::<StateHashRules>("StateHash");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy_ecs::world::World;

    use super::*;
    use crate::ids::spawn_with_id;
    use crate::units::Owner;
    use crate::{MatchSetup, RunConfig, build_sim_app, load_content};

    /// A core sim with #1 (with an `Owner`) and #2, spawned in `order`, after `fillers`
    /// spawned-and-despawned entities that shift every `Entity` index.
    fn world_with(order: [u32; 2], fillers: usize) -> World {
        let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod");
        let content = load_content(&[], &[core_mod]).expect("core_mod loads");
        let mut app = build_sim_app(&[], content, MatchSetup::default(), RunConfig::default())
            .expect("the sim builds");
        let world = app.world_mut();
        for _ in 0..fillers {
            let filler = world.spawn_empty().id();
            world.despawn(filler);
        }
        for id in order {
            let entity = spawn_with_id(world, SimId(id), ()).id();
            if id == 1 {
                world.entity_mut(entity).insert(Owner(PlayerId(0)));
            }
        }
        std::mem::take(world)
    }

    #[test]
    fn hash_independent_of_spawn_order() {
        let a = world_with([1, 2], 0);
        let b = world_with([2, 1], 3);
        let index = |w: &World| w.resource::<SimIndex>().get(SimId(1));
        assert_ne!(
            index(&a),
            index(&b),
            "the worlds allocate different entities"
        );
        assert_eq!(compute_state_hash(&a), compute_state_hash(&b));
    }

    fn hash_of(value: &impl SimHash) -> u64 {
        let mut h = StateHasher::new();
        value.sim_hash(&mut h);
        h.finish()
    }

    #[test]
    fn hasher_is_xxh64_seed_0_over_le_bytes() {
        let mut h = StateHasher::new();
        h.write_u32(0x0102_0304);
        h.write_bool(true);
        assert_eq!(h.finish(), xxhash_rust::xxh64::xxh64(&[4, 3, 2, 1, 1], 0));
    }

    #[test]
    fn option_and_vec_encodings_are_distinct() {
        assert_ne!(hash_of(&None::<u8>), hash_of(&Some(0u8)));
        assert_ne!(hash_of(&vec![1u8]), hash_of(&vec![1u8, 0]));
        assert_ne!(hash_of(&Vec::<u8>::new()), hash_of(&vec![0u8]));
    }

    #[test]
    fn fx_hashes_its_raw_bits() {
        assert_eq!(hash_of(&Fx::from_raw(5)), hash_of(&5i64));
    }

    #[test]
    fn registry_ids_hash_as_name_keys() {
        let mut names = StringTables::default();
        names.insert(
            tables::UNITS,
            vec!["a.first".to_owned(), "a.tank".to_owned()],
        );
        let mut h = StateHasher::with_tables(Arc::new(NameKeys::new(&names)));
        UnitTypeId(1).sim_hash(&mut h);
        assert_eq!(h.finish(), hash_of(&stream_key("a.tank")));
    }

    #[test]
    #[should_panic(expected = "has no string tables")]
    fn registry_id_without_tables_panics() {
        hash_of(&UnitTypeId(0));
    }
}
