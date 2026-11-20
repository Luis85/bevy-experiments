//! The world rules in force: the island's immutable preset (`ActiveWorld`),
//! its day length and the system switches that gate verbs.
use crate::worlds::WorldPreset;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;

/// The world preset this island was built from: its rules (needs, day and
/// night, the living world's catalog and behaviour). Hashed and saved
/// (`island_world_rules_v1`), so a save carries the rules it was made under
/// and Continue never depends on an edited world file (review fix S1.3).
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveWorld(pub WorldPreset);

impl ActiveWorld {
    /// Canonical JSON of the rules: structs and `BTreeMap`s only, so the
    /// text is the same on every run and platform.
    pub fn canonical(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }
}

impl npc_framework::kernel::hash::SimHashResource for ActiveWorld {
    const NAME: &'static str = "island_world_rules_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.canonical());
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Installs the world rules before content freezes: the AI scheduler policy,
/// [`ActiveWorld`] and the day/night clock.
pub fn prepare(app: &mut App, p: &WorldPreset) {
    app.insert_resource(p.ai_scheduler.clone());
    if app.world().contains_resource::<ActiveWorld>() {
        return;
    }
    app.insert_resource(ActiveWorld(p.clone()));
    app.world_mut().resource_mut::<SimConfig>().day_length = p.day_night.cycle_ticks;
    app.world_mut().resource_mut::<SimConfig>().night_permille =
        if p.day_night.on { 300 } else { 0 };
}

/// Whether a system switch of the active world is on (on without a world).
pub fn enabled(world: &World, field: fn(&crate::worlds::SystemsSpec) -> bool) -> bool {
    world
        .get_resource::<ActiveWorld>()
        .is_none_or(|p| field(&p.0.systems))
}

/// Applies the world to the played actor after spawn.
pub fn apply(world: &mut World, played: SimId, _p: &WorldPreset) {
    crate::village::apply(world, played);
}

/// Refuses verbs of a system the world switches off.
pub fn verb_gate(world: &World, _e: Entity, verb: &str) -> Option<String> {
    let p = &world.get_resource::<ActiveWorld>()?.0;
    let base_verb = verb.split_once(':').map_or(verb, |(base, _)| base);
    ((!p.systems.crafting && base_verb.eq_ignore_ascii_case("craft"))
        || (!p.systems.locked_doors && matches!(verb, "lock" | "unlock"))
        || (!p.systems.vehicles && matches!(verb, "mount" | "dismount" | "drive_to" | "unload"))
        || (!p.systems.progression && matches!(verb, "study" | "teach" | "spend_points")))
    .then(|| "this system is disabled in this world".into())
}
