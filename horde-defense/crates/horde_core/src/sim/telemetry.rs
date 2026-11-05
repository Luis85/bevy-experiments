//! Per-attempt combat telemetry counters.

use super::*;
use std::collections::BTreeMap;

/// The most keys each per-archetype telemetry map may hold (schema v7 §3.12).
pub(crate) const MAX_TELEMETRY_ARCHETYPES: usize = 256;
/// The most boss phase transitions one attempt records (schema v7 §3.12).
pub(crate) const MAX_BOSS_PHASE_TICKS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatTelemetry {
    pub spawned: u32,
    pub skipped_spawns: u32,
    pub peak_enemies: usize,
    pub misses: u32,
    pub defended: u32,
    pub absorbed: u32,
    pub weak_hits: u32,
    pub contact_attacks: u32,
    #[serde(default = "legacy_damage_lanes")]
    pub damage_by_lane: Vec<u64>,
    pub kills_by_kind: KindMap<EnemyKind, u32>,
    pub structure_losses: u32,
    pub structure_damage_taken: u64,
    pub npc_decisions: KindMap<NpcManeuver, u32>,
    pub boss_arrival_tick: Option<u64>,
    pub boss_defeat_tick: Option<u64>,
    pub defender_damage: u64,
    pub trap_damage: u64,
    pub turret_damage: u64,
    pub canceled_reload_ticks: u64,
    /// Non-boss spawns per resolved archetype id. Bosses and unresolved
    /// legacy enemies are not counted. Absent when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spawned_by_archetype: BTreeMap<String, u32>,
    /// Non-boss kills per resolved archetype id; the sum is at most the
    /// attempt's `kills` and each entry at most its spawns. Absent when empty.
    ///
    /// Writer obligation (schema v7 §3.12 keeps the per-archetype rule that
    /// design-progression-tree §10 dropped): an attempt resumed from a save
    /// made before the maps were written has living enemies whose spawns were
    /// never counted, so the writer counts a kill only while that archetype's
    /// kills are below its `spawned_by_archetype` count. Without that rule,
    /// the next load fails with `SaveError::Invalid`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub kills_by_archetype: BTreeMap<String, u32>,
    /// Damage the defenders took per resolved archetype id. Absent when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub damage_taken_by_archetype: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub elites_spawned: u32,
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub elites_killed: u32,
    /// Damage the defenders took from hazards (spit pools, detonations).
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub hazard_damage_taken: u64,
    /// The tick of each boss phase transition, at most two.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boss_phase_ticks: Vec<u64>,
}
impl Default for CombatTelemetry {
    fn default() -> Self {
        Self {
            spawned: Default::default(),
            skipped_spawns: Default::default(),
            peak_enemies: Default::default(),
            misses: Default::default(),
            defended: Default::default(),
            absorbed: Default::default(),
            weak_hits: Default::default(),
            contact_attacks: Default::default(),
            damage_by_lane: legacy_damage_lanes(),
            kills_by_kind: Default::default(),
            structure_losses: Default::default(),
            structure_damage_taken: Default::default(),
            npc_decisions: Default::default(),
            boss_arrival_tick: Default::default(),
            boss_defeat_tick: Default::default(),
            defender_damage: Default::default(),
            trap_damage: Default::default(),
            turret_damage: Default::default(),
            canceled_reload_ticks: Default::default(),
            spawned_by_archetype: BTreeMap::new(),
            kills_by_archetype: BTreeMap::new(),
            damage_taken_by_archetype: BTreeMap::new(),
            elites_spawned: 0,
            elites_killed: 0,
            hazard_damage_taken: 0,
            boss_phase_ticks: Vec::new(),
        }
    }
}
