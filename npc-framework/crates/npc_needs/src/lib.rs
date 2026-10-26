//! Data-driven needs with hysteresis thresholds.
//!
//! Content registers each need once as a [`NeedDef`] (name, decay per tick,
//! thresholds). At freeze time the defs are sorted by name and get dense
//! [`NeedId`]s; every agent's [`Needs`] holds one [`Need`] per def in id
//! order. Values are satisfaction in milli-units: [`NEED_MAX`] = fully
//! satisfied, 0 = depleted.
//!
//! Each tick (`NpcSet::Needs`, before the day/night system) every need of
//! every agent decays by its def's `decay_per_tick` (unless paused by
//! [`PauseDecay`]), its level is recomputed with hysteresis, and agents
//! whose level changed get a `ThinkRequest`; changes are journaled
//! (`NeedLevel`) sorted by `(SimId, def order)`.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::ThinkRequest;
use npc_kernel::commands::{CommandError, SetNeedHandler, invalid};
use npc_kernel::hash::Fnv64;
use npc_kernel::journal::{Journal, JournalKind};
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::snapshot::SnapshotAppExt;
use npc_kernel::{NpcSet, SimClock, SimHash, SimId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use npc_kernel::NeedLevel;

/// Full satisfaction.
pub const NEED_MAX: i32 = 100_000;

/// Thresholds of one need. Enter a worse level when value < `*_enter`, leave
/// it back towards Ok only when value > `*_exit` (exit > enter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NeedThresholds {
    pub low_enter: i32,
    pub low_exit: i32,
    pub urgent_enter: i32,
    pub urgent_exit: i32,
}

/// One need, registered by content (`register_need`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NeedDef {
    /// Stable name: journal, commands (`SetNeed.need`), id order (`"Hunger"`).
    pub name: &'static str,
    /// Field name in JSON views of `Needs` (`"hunger"`).
    pub key: &'static str,
    /// Presentation order (JSON views, journal order of simultaneous level
    /// changes); ties by name. Never affects ids or the state hash.
    pub order: i32,
    /// Decay per tick (milli-units).
    pub decay_per_tick: i32,
    pub thresholds: NeedThresholds,
}

/// Index of a need in [`NeedRegistry::defs`] and in [`Needs::values`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NeedId(pub u16);

/// All need defs; sorted by name (index = id) once frozen.
#[derive(Resource, Clone, Debug, Default)]
pub struct NeedRegistry {
    pub defs: Vec<NeedDef>,
    frozen: bool,
}

impl NeedRegistry {
    /// Registers `def`. Panics on a duplicate name or after freeze.
    pub fn add(&mut self, def: NeedDef) {
        assert!(
            !self.frozen,
            "cannot register need `{}` after freeze",
            def.name
        );
        assert!(
            !self.defs.iter().any(|d| d.name == def.name),
            "need `{}` is registered twice",
            def.name
        );
        self.defs.push(def);
    }

    /// Sorts by name and fixes the ids. Idempotent.
    pub fn freeze(&mut self) {
        if !self.frozen {
            self.defs.sort_by_key(|d| d.name);
            u16::try_from(self.defs.len()).unwrap_or_else(|_| {
                panic!("too many needs: {} does not fit in u16", self.defs.len())
            });
            self.frozen = true;
        }
    }

    /// Id of need `name` (frozen registries only).
    pub fn id(&self, name: &str) -> Option<NeedId> {
        assert!(self.frozen, "NeedIds exist only after freeze");
        self.defs
            .iter()
            .position(|d| d.name == name)
            .and_then(|i| u16::try_from(i).ok())
            .map(NeedId)
    }

    pub fn def(&self, id: NeedId) -> &NeedDef {
        &self.defs[id.0 as usize]
    }

    /// Fresh needs (all full, level Ok) for an agent.
    pub fn full(&self) -> Needs {
        Needs {
            values: vec![Need::default(); self.defs.len()],
        }
    }

    /// Ids in presentation order (`order`, then name).
    pub fn display_order(&self) -> Vec<NeedId> {
        let mut ids: Vec<NeedId> = (0..self.defs.len())
            .filter_map(|i| u16::try_from(i).ok())
            .map(NeedId)
            .collect();
        ids.sort_by_key(|&id| (self.def(id).order, self.def(id).name));
        ids
    }

    /// Names of every need, sorted (error messages).
    pub fn names(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.defs.iter().map(|d| d.name).collect();
        v.sort_unstable();
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Need {
    pub value: i32,
    pub level: NeedLevel,
}

impl Default for Need {
    fn default() -> Self {
        Self {
            value: NEED_MAX,
            level: NeedLevel::Ok,
        }
    }
}

/// An agent's needs, one per [`NeedDef`], indexed by [`NeedId`].
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Needs {
    pub values: Vec<Need>,
}

impl Needs {
    pub fn get(&self, id: NeedId) -> &Need {
        &self.values[id.0 as usize]
    }
    pub fn get_mut(&mut self, id: NeedId) -> &mut Need {
        &mut self.values[id.0 as usize]
    }
}

impl SimHash for Needs {
    const NAME: &'static str = "needs";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.values.len()).expect("too many needs"));
        for n in &self.values {
            h.i32(n.value);
            h.u8(match n.level {
                NeedLevel::Ok => 0,
                NeedLevel::Low => 1,
                NeedLevel::Urgent => 2,
            });
        }
    }
}

/// `{"<key>": {"value":..,"level":..}, ..}` for `Needs` (diffs, generic inspect).
pub fn needs_json(world: &World, entity: Entity) -> Option<Value> {
    let needs = world.get::<Needs>(entity)?;
    let registry = world.get_resource::<NeedRegistry>()?;
    let mut map = Map::new();
    for (i, need) in needs.values.iter().enumerate() {
        let key = registry
            .defs
            .get(i)
            .map_or_else(|| format!("need{i}"), |d| d.key.to_owned());
        map.insert(key, serde_json::to_value(need).ok()?);
    }
    Some(Value::Object(map))
}

/// Recomputes `need.level` from `need.value` with hysteresis. Returns true when the level changed.
///
/// - From `Ok`: `v < urgent_enter` gives Urgent; else `v < low_enter` gives Low.
/// - From `Low`: `v < urgent_enter` gives Urgent; else `v > low_exit` gives Ok.
/// - From `Urgent`: `v > urgent_exit` gives Ok when `v > low_exit`, otherwise Low.
pub fn update_level(need: &mut Need, t: &NeedThresholds) -> bool {
    let v = need.value;
    let next = match need.level {
        NeedLevel::Ok if v < t.urgent_enter => NeedLevel::Urgent,
        NeedLevel::Ok if v < t.low_enter => NeedLevel::Low,
        NeedLevel::Low if v < t.urgent_enter => NeedLevel::Urgent,
        NeedLevel::Low if v > t.low_exit => NeedLevel::Ok,
        NeedLevel::Urgent if v > t.urgent_exit => {
            if v > t.low_exit {
                NeedLevel::Ok
            } else {
                NeedLevel::Low
            }
        }
        unchanged => unchanged,
    };
    let changed = next != need.level;
    need.level = next;
    changed
}

/// Pauses decay of the needs whose id bit is set (bit `id.0`), e.g. energy
/// while asleep. Inserted and removed by content actions; not sim state of
/// its own (it only gates decay).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(storage = "SparseSet")]
pub struct PauseDecay {
    pub mask: u64,
}

impl PauseDecay {
    pub fn of(ids: &[NeedId]) -> Self {
        let mut mask = 0;
        for id in ids {
            assert!(id.0 < 64, "PauseDecay covers the first 64 needs");
            mask |= 1u64 << id.0;
        }
        Self { mask }
    }
    pub fn pauses(&self, id: NeedId) -> bool {
        id.0 < 64 && self.mask & (1u64 << id.0) != 0
    }
}

/// `NpcSet::Needs` system: decay (clamped to `0..=NEED_MAX`), level update,
/// and a `ThinkRequest` on every agent whose level changed (on any need).
/// Purely per-entity, so iteration order does not matter. Level changes are
/// journaled (`NeedLevel`) sorted by `(SimId, def order, name)`, so the
/// journal does not depend on query order either.
pub fn decay_needs_system(
    mut agents: Query<(Entity, &mut Needs, Option<&PauseDecay>, Option<&SimId>)>,
    registry: Res<NeedRegistry>,
    journal: Option<ResMut<Journal>>,
    clock: Option<Res<SimClock>>,
    mut commands: Commands,
) {
    let mut journal = journal.filter(|j| j.enabled);
    let defs = &registry.defs;
    let mut changes: Vec<(SimId, i32, &'static str, NeedLevel, NeedLevel)> = Vec::new();
    for (entity, mut needs, pause, id) in &mut agents {
        let mut any = false;
        for (i, def) in defs.iter().enumerate() {
            let Some(need) = needs.values.get_mut(i) else {
                continue;
            };
            let nid = NeedId(u16::try_from(i).expect("need ids fit in u16"));
            if !pause.is_some_and(|p| p.pauses(nid)) {
                need.value = (need.value - def.decay_per_tick).clamp(0, NEED_MAX);
            }
            let before = need.level;
            if update_level(need, &def.thresholds) {
                any = true;
                if journal.is_some()
                    && let Some(&id) = id
                {
                    changes.push((id, def.order, def.name, before, need.level));
                }
            }
        }
        if any {
            commands.entity(entity).insert(ThinkRequest);
        }
    }
    if let Some(journal) = &mut journal
        && !changes.is_empty()
    {
        changes.sort_unstable();
        let tick = clock.map_or(0, |c| c.tick);
        for (id, _, need, from, to) in changes {
            journal.push(
                tick,
                Some(id),
                JournalKind::NeedLevel {
                    need: need.to_owned(),
                    from,
                    to,
                },
            );
        }
    }
}

fn set_need_validate(world: &World, e: Entity, need: &str, value: i32) -> Result<(), CommandError> {
    let unit = world.get::<SimId>(e).map_or(0, |id| id.0);
    if world.get::<Needs>(e).is_none() {
        return Err(CommandError::WrongKind {
            unit,
            expected: "unit with needs".to_owned(),
        });
    }
    let registry = world
        .get_resource::<NeedRegistry>()
        .ok_or_else(|| CommandError::NotReady {
            missing: "NeedRegistry".to_owned(),
        })?;
    if registry.id(need).is_none() {
        return Err(invalid(
            "need",
            format!(
                "unknown need `{need}` (known: {})",
                registry.names().join(", ")
            ),
        ));
    }
    if !(0..=NEED_MAX).contains(&value) {
        return Err(invalid(
            "value",
            format!("{value} is outside 0..={NEED_MAX}"),
        ));
    }
    Ok(())
}

fn set_need_apply(world: &mut World, e: Entity, need: &str, value: i32) {
    let Some(id) = world
        .get_resource::<NeedRegistry>()
        .and_then(|r| r.id(need))
    else {
        return;
    };
    if let Some(mut needs) = world.get_mut::<Needs>(e)
        && let Some(n) = needs.values.get_mut(id.0 as usize)
    {
        n.value = value;
    }
}

fn freeze_needs(world: &mut World) -> Vec<(String, String)> {
    let mut registry = world.get_resource_or_init::<NeedRegistry>();
    registry.freeze();
    registry
        .defs
        .iter()
        .map(|d| ("need".to_owned(), d.name.to_owned()))
        .collect()
}

/// Registration of needs on `App`.
pub trait NeedsAppExt {
    /// Registers a need (any order; ids are frozen sorted by name).
    fn register_need(&mut self, def: NeedDef) -> &mut Self;
}

impl NeedsAppExt for App {
    fn register_need(&mut self, def: NeedDef) -> &mut Self {
        assert!(
            !is_frozen(self.world()),
            "cannot register need `{}`: content is already frozen",
            def.name
        );
        self.world_mut()
            .get_resource_or_init::<NeedRegistry>()
            .add(def);
        self
    }
}

/// Installs the needs mechanic: [`NeedRegistry`], the `needs` sim component,
/// the decay system, the `SetNeed` command handler and the freeze step.
pub struct NpcNeedsPlugin;

impl Plugin for NpcNeedsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NeedRegistry>()
            .insert_resource(SetNeedHandler {
                validate: set_need_validate,
                apply: set_need_apply,
            })
            .register_sim_component_with::<Needs>(needs_json)
            .register_snapshot_component_named::<PauseDecay>("pause_decay")
            .add_freeze_hook("npc_needs", freeze_needs)
            .add_systems(
                npc_kernel::NpcSimTick,
                decay_needs_system
                    .in_set(NpcSet::Needs)
                    .before(npc_kernel::clock::day_night_system),
            );
    }
}
