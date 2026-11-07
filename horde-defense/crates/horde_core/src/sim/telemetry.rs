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
impl CombatTelemetry {
    /// Counts one non-boss spawn of a resolved archetype (W2-05). A key the
    /// map does not hold yet is added only while the map is below its key
    /// limit.
    pub(crate) fn count_archetype_spawn(&mut self, archetype: &str) {
        if let Some(n) = archetype_entry(&mut self.spawned_by_archetype, archetype) {
            *n = n.saturating_add(1);
        }
    }
    /// Counts one non-boss kill of a resolved archetype, only while its kills
    /// are below its counted spawns (the schema v7 §3.12 writer obligation):
    /// an attempt resumed from a save made before the maps were written has
    /// living enemies whose spawns were never counted.
    pub(crate) fn count_archetype_kill(&mut self, archetype: &str) {
        let spawned = self
            .spawned_by_archetype
            .get(archetype)
            .copied()
            .unwrap_or(0);
        let killed = self.kills_by_archetype.get(archetype).copied().unwrap_or(0);
        if killed < spawned
            && let Some(n) = archetype_entry(&mut self.kills_by_archetype, archetype)
        {
            *n += 1;
        }
    }
    /// Adds damage the defender took from a non-boss resolved archetype.
    pub(crate) fn count_archetype_damage_taken(&mut self, archetype: &str, damage: u64) {
        if damage == 0 {
            return;
        }
        if let Some(n) = archetype_entry(&mut self.damage_taken_by_archetype, archetype) {
            *n = n.saturating_add(damage);
        }
    }
}

/// The counter for `archetype`, inserted at zero while the map is below
/// [`MAX_TELEMETRY_ARCHETYPES`] keys; None once a new key would exceed it.
fn archetype_entry<'m, V: Default>(
    map: &'m mut BTreeMap<String, V>,
    archetype: &str,
) -> Option<&'m mut V> {
    if !map.contains_key(archetype) && map.len() >= MAX_TELEMETRY_ARCHETYPES {
        return None;
    }
    Some(map.entry(archetype.to_owned()).or_default())
}
