//! Per-attempt combat telemetry counters.

use super::*;
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
        }
    }
}
