mod events;
mod journey;
mod migrations;
mod parallel;
mod persistence;
pub use events::{Actor, BattleId, SimEvent, SimEventRecord, Strike};
pub use journey::JourneyContinuation;
pub use migrations::SAVE_VERSION;
pub use parallel::SettlementIssue;
pub(crate) use parallel::UNKNOWN_EXPEDITION;
use persistence::validate_game;
pub use persistence::{RULESET_VERSION, SaveError};

use crate::campaign::{Campaign, DefenderProfile};
use crate::content::{
    BossKind, BossTiming, EnemyKind, ItemDefId, Mode, Policy, StructureKind, content,
};
use crate::kind_map::KindMap;
use crate::ordnance::{self, FireZone, OrdnanceKind, OrdnanceState, ThrowRequest, ThrownOrdnance};
use crate::rules::{self, Attack, Damage, DamageKind, InjuryState, Rng};
use crate::status::{StatusId, Statuses};
use crate::world::Modifiers;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use tracing::{debug, debug_span, info, info_span, trace};

pub const TICKS_PER_SECOND: u32 = 60;
pub const ENEMY_HIT_STOP_TICKS: u32 = 6;
pub const BRACE_DURATION_TICKS: u32 = 300;
pub const MAX_PASS_OFFSET: f32 = 0.32;
pub const LANES: u8 = 11;
pub const MIN_LANES: u8 = 3;
pub const MAX_LANES: u8 = 17;
pub fn legacy_width() -> f32 {
    30.0
}
pub fn legacy_lane_count() -> u8 {
    LANES
}
fn legacy_damage_lanes() -> Vec<u64> {
    vec![0; usize::from(LANES)]
}
pub const DEFENDER: [f32; 2] = [0.5, 5.5];
pub const DEFENSE_LINE: f32 = 1.5;
const DT: f32 = 1.0 / 60.0;
const MAX_LOG: usize = 80;
pub const MAX_EVENTS: usize = 512;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Input {
    pub aim: [f32; 2],
    pub fire: bool,
    pub melee: bool,
    pub reload: bool,
    pub heal: bool,
    pub brace: bool,
    pub auto_reload: bool,
    #[serde(default)]
    pub throw: Option<ThrowRequest>,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            aim: [15.0, 5.5],
            fire: false,
            melee: false,
            reload: false,
            heal: false,
            brace: false,
            auto_reload: true,
            throw: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    #[serde(default = "legacy_width")]
    pub arena_width: f32,
    /// Saved within-lane passing displacement; real hit and area geometry use y().
    #[serde(default)]
    pub pass_offset: f32,
    pub id: u64,
    pub kind: EnemyKind,
    pub lane: u8,
    pub x: f32,
    pub hp: i32,
    pub max_hp: i32,
    pub boss: bool,
    pub attack_left: u32,
    pub armor: i32,
    pub injury: InjuryState,
    pub born_tick: u64,
    pub contact_since: Option<u64>,
    pub target_structure: Option<u64>,
    #[serde(default)]
    pub ready_turns: u32,
    #[serde(default)]
    pub boss_kind: Option<BossKind>,
    #[serde(default)]
    pub boss_timing: BossTiming,
    #[serde(default)]
    pub attack_phase: EnemyPhase,
    #[serde(default)]
    pub windup_total: u32,
    /// Timed effects; `StatusId::HitStop` holds movement steps after positive
    /// injury while attacks keep their cadence.
    #[serde(default, skip_serializing_if = "Statuses::is_empty")]
    pub statuses: Statuses,
    #[serde(default)]
    pub resolved: Option<crate::enemy_catalog::ResolvedEnemy>,
}
/// Permanent battlefield remains for this attempt, independent of transient effects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorpseRecord {
    pub id: u64,
    pub kind: EnemyKind,
    pub boss_kind: Option<BossKind>,
    pub at: [f32; 2],
    pub visual: Option<crate::enemy_catalog::EnemyVisual>,
    pub tick: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyPhase {
    #[default]
    Advance,
    Queued,
    Windup,
    Recovery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyCondition {
    Active,
    Critical,
    Defeated,
}
impl Enemy {
    pub fn condition(&self, mode: Mode) -> EnemyCondition {
        if (mode == Mode::Personal && self.hp <= 0) || (mode == Mode::Npc && self.injury.terminal())
        {
            EnemyCondition::Defeated
        } else if mode == Mode::Npc && self.injury.hp <= 0 {
            EnemyCondition::Critical
        } else {
            EnemyCondition::Active
        }
    }
    pub fn critically_wounded(&self, mode: Mode) -> bool {
        self.condition(mode) == EnemyCondition::Critical
    }
    fn apply_damage(&mut self, damage: i32, mode: Mode) {
        if mode == Mode::Personal {
            self.hp -= damage;
        } else {
            // The rules resolver has already applied NPC injury exactly once.
            self.hp = self.injury.hp;
        }
        if damage > 0 {
            self.statuses
                .refresh(StatusId::HitStop, ENEMY_HIT_STOP_TICKS);
        }
    }
    /// Movement steps still held by hit-stop; 0 when the enemy may move.
    pub fn hit_stop_remaining(&self) -> u32 {
        self.statuses.remaining(StatusId::HitStop)
    }
    pub fn y(&self) -> f32 {
        self.lane as f32 + 0.5 + self.pass_offset
    }
    pub fn contact(&self) -> bool {
        self.x <= DEFENSE_LINE + 0.01
    }
    pub fn in_view(&self) -> bool {
        self.x <= self.arena_width + if self.boss { 0.55 } else { 0.34 }
    }
    pub fn exposed_at(&self, tick: u64) -> bool {
        let cycle = seconds(self.boss_timing.cycle).max(1);
        let phase = tick.saturating_sub(self.born_tick) % u64::from(cycle);
        self.boss
            && phase >= u64::from(seconds(self.boss_timing.exposure_delay))
            && phase
                < u64::from(seconds(
                    self.boss_timing.exposure_delay + self.boss_timing.exposure_duration,
                ))
    }
    pub fn windup_progress(&self) -> f32 {
        if self.attack_phase == EnemyPhase::Windup && self.windup_total > 0 {
            1.0 - self.attack_left as f32 / self.windup_total as f32
        } else {
            0.0
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Structure {
    #[serde(default)]
    pub position_x: Option<f32>,
    #[serde(default)]
    pub build_width: Option<f32>,
    pub id: u64,
    pub kind: StructureKind,
    pub lane: u8,
    pub column: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub cooldown: u32,
    pub charges: u32,
    pub cost: u32,
    #[serde(default)]
    pub ready_turns: u32,
}
impl Structure {
    pub fn footprint_width(&self) -> f32 {
        self.build_width.unwrap_or(2.4)
    }
    pub fn blocking_radius(&self) -> f32 {
        0.7_f32.min(self.footprint_width() * 0.5 + 0.25)
    }
    pub fn x(&self) -> f32 {
        self.position_x.unwrap_or(4.2 + self.column as f32 * 2.4)
    }
    pub fn y(&self) -> f32 {
        self.lane as f32 + 0.5
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Shot,
    Hit,
    WeakPoint,
    Armor,
    Kill,
    Damage,
    Reload,
    Melee,
    Spawn,
    Boss,
    StructureLost,
    Ability,
    Contact,
    Windup,
    BossWarning,
    BossExposure,
    Reinforcements,
    Miss,
    Defended,
    Throw,
    Explosion,
    Burn,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub value: i32,
    pub tick: u64,
    #[serde(default)]
    pub actor_id: Option<u64>,
    #[serde(default)]
    pub target_id: Option<u64>,
    #[serde(default)]
    pub lane: Option<u8>,
    #[serde(default)]
    pub duration_ticks: u32,
    /// Shared with the matching `SimEventRecord`; zero only in legacy saves
    /// before load-time renumbering.
    #[serde(default)]
    pub seq: u64,
}
#[derive(Clone, Copy, Debug, Default)]
struct EventMetadata {
    actor_id: Option<u64>,
    target_id: Option<u64>,
    lane: Option<u8>,
    duration_ticks: u32,
    ordnance: Option<OrdnanceKind>,
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum NpcManeuver {
    #[default]
    DoNothing,
    Aim,
    Attack,
    Ready,
    Reload,
    Recover,
    Posture,
    AllOutDefense,
}
impl crate::kind_map::StableKind for NpcManeuver {
    const ALL: &'static [Self] = &[
        Self::DoNothing,
        Self::Aim,
        Self::Attack,
        Self::Ready,
        Self::Reload,
        Self::Recover,
        Self::Posture,
        Self::AllOutDefense,
    ];
}
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    #[serde(default = "legacy_width")]
    pub width_yards: f32,
    #[serde(default = "crate::balance::BalanceSettings::legacy")]
    pub balance: crate::balance::BalanceSettings,
    #[serde(default)]
    pub dps: crate::stats::DpsTracker,
    #[serde(default)]
    pub gear_snapshot: Vec<crate::stats::GearRecord>,
    #[serde(default)]
    pub kill_xp: u64,
    #[serde(default)]
    pub loot_quality_counts: crate::kind_map::KindMap<crate::quality::Quality, u32>,
    #[serde(default = "legacy_lane_count")]
    pub lane_count: u8,
    #[serde(default)]
    pub loot_events: Vec<crate::quality::LootEvent>,
    #[serde(default)]
    pub progression: crate::progression::ProgressionSnapshot,
    pub id: u64,
    pub level: usize,
    pub mode: Mode,
    pub tick: u64,
    pub paused: bool,
    pub country_id: Option<String>,
    pub country_name: String,
    pub stage: usize,
    pub modifiers: Modifiers,
    pub enemies: Vec<Enemy>,
    #[serde(default)]
    pub corpses: Vec<CorpseRecord>,
    pub structures: Vec<Structure>,
    /// Saved plan entries omitted because this stage cannot afford the full prefix.
    #[serde(default)]
    pub layout_omissions: usize,
    pub hp: i32,
    pub max_hp: i32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub ammo: u32,
    pub magazine: u32,
    pub reload_left: u32,
    pub shot_cooldown: u32,
    pub melee_cooldown: u32,
    pub melee_commit: u32,
    pub supply: u32,
    pub boss_spawned: bool,
    pub kills: u32,
    pub coins: u64,
    pub loot: u32,
    pub decision: String,
    /// Bounded presentation window for scene replay; the typed stream is
    /// `Game::drain_events`.
    pub events: VecDeque<Event>,
    pub log: Vec<String>,
    /// Latest rules resolution (attack roll, defense, injury) for inspection.
    #[serde(default)]
    pub latest_resolution: Option<String>,
    pub last_damage_lane: Option<u8>,
    pub profile: DefenderProfile,
    pub rng: Rng,
    pub injury: InjuryState,
    pub reward_ledger: BTreeSet<u64>,
    pub first_contact: Option<f32>,
    pub shots: u32,
    pub hits: u32,
    pub active_damage: u64,
    pub structure_damage: u64,
    pub reload_ticks: u64,
    pub aim_ticks: u64,
    pub next_id: u64,
    #[serde(default)]
    pub telemetry: CombatTelemetry,
    #[serde(default)]
    pub maneuver: NpcManeuver,
    #[serde(default)]
    pub decision_target: Option<u64>,
    #[serde(default)]
    pub defense_bonus: i32,
    #[serde(default)]
    pub ordnance: OrdnanceState,
    spawn_credit: f32,
    lane_history: Vec<u8>,
    aim_target: Option<u64>,
    aim_seconds: u8,
    ready_turns: u32,
    #[serde(default)]
    npc_holding_melee: bool,
    heal_used: bool,
    #[serde(default, skip_serializing_if = "Statuses::is_empty")]
    statuses: Statuses,
    caller_tick: u64,
    #[serde(default)]
    boss_warning_sent: bool,
    #[serde(default)]
    caller_lanes: Option<[u8; 2]>,
    /// Next event sequence number; the live attempt owns the game's cursor.
    #[serde(default)]
    event_seq: u64,
    #[serde(skip)]
    emitted: Vec<SimEventRecord>,
}
impl Attempt {
    /// The surviving structures as a reusable country layout plan.
    pub fn structure_layout(&self) -> Vec<crate::campaign::Placement> {
        self.structures
            .iter()
            .map(|s| crate::campaign::Placement {
                kind: s.kind,
                lane: usize::from(s.lane),
                column: usize::from(s.column),
            })
            .collect()
    }
    /// Saved authoritative clock; transient presentation events may expire first.
    pub fn brace_remaining_ticks(&self) -> u32 {
        self.statuses.remaining(StatusId::Brace)
    }
    /// Whether the brace stance covers `tick` (at or before the current tick).
    /// The saved status clock is authoritative; past ticks after the status
    /// ended fall back to retained brace ability events (`value == 0`).
    pub fn brace_active_at(&self, tick: u64) -> bool {
        if tick > self.tick {
            return false;
        }
        let duration = u64::from(BRACE_DURATION_TICKS);
        let remaining = self.statuses.remaining(StatusId::Brace);
        if remaining > 0 {
            let end = self.tick.saturating_add(u64::from(remaining));
            let start = end.saturating_sub(duration);
            if tick >= start && tick < end {
                return true;
            }
        }
        if tick == self.tick {
            return false;
        }
        self.events.iter().rev().any(|event| {
            event.kind == EventKind::Ability
                && event.value == 0
                && event.tick <= tick
                && tick - event.tick < duration
        })
    }
    /// The defender's timed status effects (saved, authoritative).
    pub fn statuses(&self) -> &Statuses {
        &self.statuses
    }
    pub fn live_dps(&self) -> f32 {
        self.dps.live(self.tick)
    }
    pub fn average_dps(&self) -> f32 {
        self.dps.average(self.tick)
    }
    pub fn peak_dps(&self) -> f32 {
        self.dps.peak
    }
    pub fn construction_bounds(&self) -> [f32; 2] {
        [
            2.0 + (self.width_yards - 12.0) / 18.0,
            self.width_yards * 0.5,
        ]
    }
    pub fn construction_cell_width(&self) -> f32 {
        let b = self.construction_bounds();
        (b[1] - b[0]) / 5.0
    }
    pub fn build_cell_center(&self, lane: u8, column: u8) -> Option<[f32; 2]> {
        if lane >= self.lane_count || column >= 5 {
            return None;
        }
        let b = self.construction_bounds();
        Some([
            b[0] + (f32::from(column) + 0.5) * self.construction_cell_width(),
            f32::from(lane) + 0.5,
        ])
    }
    pub fn build_cell_at(&self, point: [f32; 2]) -> Option<[u8; 2]> {
        let b = self.construction_bounds();
        if point.iter().any(|v| !v.is_finite())
            || point[0] < b[0]
            || point[0] >= b[1]
            || point[1] < 0.0
            || point[1] >= f32::from(self.lane_count)
        {
            return None;
        }
        Some([
            point[1].floor() as u8,
            ((point[0] - b[0]) / self.construction_cell_width()).floor() as u8,
        ])
    }

    pub fn defender_position(&self) -> [f32; 2] {
        [0.5, f32::from(self.lane_count) * 0.5]
    }
    /// A zero snapshot cap is the legacy-save adapter for authored level caps.
    pub fn enemy_cap(&self) -> usize {
        if self.progression.enemy_cap == 0 {
            content().levels[self.level].enemy_cap
        } else {
            self.progression.enemy_cap
        }
    }
    pub fn enemy_protection(&self, enemy: &Enemy) -> i32 {
        enemy_armor(self, enemy)
    }
    pub fn elapsed(&self) -> f32 {
        self.tick as f32 / 60.0
    }
    pub fn next_decision_seconds(&self) -> f32 {
        (60 - self.tick % 60) as f32 / 60.0
    }
    pub fn boss_countdown(&self) -> f32 {
        (self.boss_time() - self.elapsed()).max(0.0)
    }
    pub fn boss_time(&self) -> f32 {
        content().levels[self.level]
            .boss_time
            .max(self.progression.boss_time_floor)
            * self.balance.boss_time
    }
    pub fn arrival_rate(&self) -> f32 {
        let l = &content().levels[self.level];
        ((l.start_rate.max(self.progression.start_rate_floor)
            * (1.0 + l.growth * self.elapsed() / self.boss_time()))
        .min(l.rate_cap.max(self.progression.rate_cap_floor))
            * self.progression.arrival_multiplier
            * self.balance.spawn_rate
            * if self.mode == Mode::Npc {
                self.balance.npc_spawn_rate
            } else {
                1.0
            })
        .min(30.0)
    }
    pub fn action(&self) -> String {
        if self.melee_commit > 0 {
            format!("Melee recovery · {:.1}s", self.melee_commit as f32 / 60.0)
        } else if self.reload_left > 0 {
            format!("Reloading · {:.1}s", self.reload_left as f32 / 60.0)
        } else if self.mode == Mode::Npc {
            self.decision.clone()
        } else {
            "Ready · hold primary to fire".into()
        }
    }
    fn event(&mut self, kind: EventKind, from: [f32; 2], to: [f32; 2], value: i32) {
        self.event_with(kind, from, to, value, EventMetadata::default());
    }
    fn note(&mut self, text: String) {
        self.log.push(text);
        if self.log.len() > MAX_LOG {
            self.log.remove(0);
        }
    }
    /// Log a rules resolution and publish it as typed, inspectable data.
    fn resolved(
        &mut self,
        attacker_id: Option<u64>,
        target_id: Option<u64>,
        result: &rules::AttackResult,
        line: String,
    ) {
        debug!(
            attacker = ?attacker_id,
            target = ?target_id,
            roll = result.attack_roll,
            skill = result.effective_skill,
            hit = result.hit,
            defended = result.defended,
            injury = result.injury,
            "rule resolution"
        );
        let attacker = self.actor(attacker_id, None);
        self.latest_resolution = Some(line.clone());
        self.emit(SimEvent::RuleResolved {
            attacker,
            target_id,
            summary: line.clone(),
        });
        self.note(line);
    }
    fn resolution_event(
        &mut self,
        result: &rules::AttackResult,
        from: [f32; 2],
        to: [f32; 2],
        meta: EventMetadata,
        friendly: bool,
    ) {
        let kind = if result.defended {
            if friendly {
                self.telemetry.defended += 1;
            }
            EventKind::Defended
        } else if !result.hit {
            if friendly {
                self.telemetry.misses += 1;
            }
            EventKind::Miss
        } else if result.injury == 0 {
            if friendly {
                self.telemetry.absorbed += 1;
            }
            EventKind::Armor
        } else if friendly {
            EventKind::Hit
        } else {
            EventKind::Damage
        };
        self.event_with(kind, from, to, result.injury, meta);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Victory,
    Defeat,
    Withdrawal,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    #[serde(default)]
    pub attempt_id: u64,
    #[serde(default = "legacy_width")]
    pub width_yards: f32,
    #[serde(default = "legacy_lane_count")]
    pub lane_count: u8,
    #[serde(default)]
    pub completion_xp: u64,
    #[serde(default)]
    pub loot_events: Vec<crate::quality::LootEvent>,
    pub outcome: Outcome,
    pub level: usize,
    pub mode: Mode,
    pub elapsed: f32,
    pub kills: u32,
    pub country_id: Option<String>,
    pub country_name: String,
    pub stage: usize,
    pub country_freed: bool,
    pub coins: u64,
    pub loot: u32,
    pub cause: String,
    pub tip: String,
    pub first_clear: bool,
    pub shots: u32,
    pub hits: u32,
    pub first_contact: Option<f32>,
    pub active_damage: u64,
    pub structure_damage: u64,
    pub log: Vec<String>,
    #[serde(default)]
    pub telemetry: CombatTelemetry,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    #[serde(default)]
    pending_runs: Vec<crate::stats::RunRecord>,
    pub campaign: Campaign,
    pub attempt: Option<Attempt>,
    pub last_result: Option<Report>,
    pub next_attempt_id: u64,
    pub seed: u64,
    pub repeat_remaining: u32,
    #[serde(default)]
    pub strategy: crate::strategy::StrategyState,
    #[serde(default)]
    pub battles: Vec<ParallelBattle>,
    #[serde(default)]
    pub global_paused: bool,
    #[serde(default)]
    worker: bool,
    /// Transient settlement failures awaiting a frontend; never persisted.
    #[serde(skip)]
    settlement_issues: Vec<SettlementIssue>,
    /// Event sequence cursor between attempts; a live attempt owns it.
    #[serde(default)]
    next_event_seq: u64,
    /// Transient typed event outbox; see `drain_events`.
    #[serde(skip)]
    event_outbox: Vec<SimEventRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelBattle {
    pub id: u64,
    pub recruit_id: u64,
    pub country_id: String,
    pub stage: usize,
    pub game: Box<Game>,
    pub synced_coins: u64,
    pub synced_item_id: u64,
    #[serde(default)]
    pub synced_item_index: usize,
    pub settled: bool,
    pub loadout: [u64; 4],
    #[serde(default)]
    pub country_operation: bool,
}
impl Default for Game {
    fn default() -> Self {
        Self {
            pending_runs: Vec::new(),
            campaign: Campaign::default(),
            attempt: None,
            last_result: None,
            next_attempt_id: 1,
            seed: 0x1A571167,
            repeat_remaining: 0,
            strategy: crate::strategy::StrategyState::default(),
            battles: vec![],
            global_paused: false,
            worker: false,
            settlement_issues: Vec::new(),
            next_event_seq: 0,
            event_outbox: Vec::new(),
        }
    }
}
impl Game {
    /// An expedition that has not yet settled back into the shared campaign.
    pub fn active_battle(&self, id: u64) -> Option<&ParallelBattle> {
        self.battles.iter().find(|b| b.id == id && !b.settled)
    }
    pub fn active_battle_mut(&mut self, id: u64) -> Option<&mut ParallelBattle> {
        self.battles.iter_mut().find(|b| b.id == id && !b.settled)
    }
    /// Drains settlement failures recorded since the last call.
    pub fn take_settlement_issues(&mut self) -> Vec<SettlementIssue> {
        std::mem::take(&mut self.settlement_issues)
    }
    pub fn sale_preview(&self, id: u64) -> Result<crate::strategy::StrategySaleQuote, String> {
        self.strategy.sale_preview(&self.campaign, id)
    }
    pub fn sell_item(&mut self, id: u64) -> Result<u64, String> {
        self.sale_preview(id)?;
        let coins = self.campaign.sell(id)?;
        self.strategy.reconcile_loadouts(&self.campaign);
        Ok(coins)
    }
    pub fn sell_extras_quote(&self) -> u64 {
        self.strategy.sell_extras_quote(&self.campaign)
    }
    pub fn sell_extras(&mut self) -> Result<u64, String> {
        self.strategy.sell_extras(&mut self.campaign)
    }
    pub fn sell_unprotected(&mut self) -> Result<u64, String> {
        self.sell_extras()
    }
    pub fn deploy(&mut self, level: usize, mode: Mode) -> Result<(), String> {
        self.deploy_internal(level, mode, None)
    }
    pub fn deploy_country_stage(&mut self, stage: usize, mode: Mode) -> Result<(), String> {
        let country = self
            .campaign
            .world
            .active_country()
            .ok_or("Choose an infected country on the world map first")?;
        if stage >= country.level_count() || stage > self.campaign.world.next_stage() {
            return Err("Secure the preceding country stage first".into());
        }
        self.deploy_internal(stage % content().levels.len(), mode, Some(stage))
    }
    fn deploy_internal(
        &mut self,
        level: usize,
        mode: Mode,
        country_stage: Option<usize>,
    ) -> Result<(), String> {
        if self.attempt.is_some() {
            return Err("Finish or recall the active deployment first".into());
        }
        if level >= content().levels.len()
            || (country_stage.is_none() && level >= self.campaign.unlocked)
        {
            return Err("This stage is not unlocked".into());
        }
        self.campaign.validate_loadout(if mode == Mode::Personal {
            self.campaign.player.loadout
        } else {
            self.campaign.npc.loadout
        })?;
        let mut profile = self.campaign.profile(mode);
        let country = if country_stage.is_some() {
            self.campaign.world.active_country()
        } else {
            None
        };
        let (lane_count, width_yards) = self.campaign.arena_geometry();
        let balance = self.campaign.balance_settings.clone();
        let progression = self.campaign.deployment_progression(mode)?;
        let mut modifiers = country.map_or_else(Modifiers::default, |c| {
            let mut m = c.modifiers;
            m.enemy_health *= c.difficulty;
            m.enemy_speed *= c.difficulty.sqrt();
            m.reward *= c.difficulty;
            m
        });
        modifiers.enemy_health *= progression.enemy_health * balance.enemy_health;
        modifiers.enemy_speed *= progression.enemy_speed * balance.enemy_speed;
        modifiers.reward *= progression.reward_multiplier * balance.reward;
        profile.personal.damage = (profile.personal.damage as f32 * balance.player_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.melee_damage = (profile.personal.melee_damage as f32
            * balance.player_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.health = (profile.personal.health as f32 * balance.defender_health)
            .round()
            .max(1.0) as i32;
        profile.max_hp = (profile.max_hp as f32 * balance.defender_health)
            .round()
            .max(1.0) as i32;
        let expected = crate::campaign::mean_basic_damage(Damage {
            dice: profile.npc.damage_dice,
            adds: profile.npc.damage_add,
            kind: profile.npc.damage_kind,
        });
        profile.npc.damage_add = (profile.npc.damage_add as f32
            + expected * (balance.player_damage - 1.0))
            .round()
            .clamp(-30.0, 1_000_000.0) as i32;
        let expected = crate::campaign::mean_basic_damage(Damage {
            dice: profile.npc_melee_damage_dice,
            adds: profile.npc_melee_damage_add,
            kind: profile.npc_melee_kind,
        });
        profile.npc_melee_damage_add = (profile.npc_melee_damage_add as f32
            + expected * (balance.player_damage - 1.0))
            .round()
            .clamp(-30.0, 1_000_000.0) as i32;
        let boss_base = if mode == Mode::Npc {
            content().levels[level]
                .boss_npc_hp
                .max(progression.npc_boss_health_floor)
        } else {
            content().levels[level]
                .boss_health
                .max(progression.boss_health_floor)
        };
        let maximum_base =
            boss_base.max(crate::enemy_catalog::enemy_catalog().maximum_health(mode));
        if maximum_base as f32 * modifiers.enemy_health > 10_000_000.0 {
            return Err("Encounter health exceeds supported numerical safety bounds".into());
        }
        profile.personal.damage = (profile.personal.damage as f32 * modifiers.personal_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.melee_damage = (profile.personal.melee_damage as f32
            * modifiers.personal_damage)
            .round()
            .max(1.0) as i32;
        profile.ranged_skill += modifiers.npc_skill;
        profile.melee_skill += modifiers.npc_skill;
        let max_hp = if mode == Mode::Personal {
            profile.personal.health
        } else {
            profile.max_hp
        };
        let stamina = if mode == Mode::Personal {
            profile.personal.stamina as f32
        } else {
            profile.max_fp as f32
        };
        let magazine = if mode == Mode::Personal {
            profile.personal.magazine
        } else {
            profile.npc.magazine
        };
        let supply = (content().levels[level].supply as i32
            + modifiers.supply
            + progression.supply_bonus as i32
            + balance.supply)
            .max(0) as u32;
        let layout = country
            .map(|c| self.campaign.layout_for(&c.id).to_vec())
            .unwrap_or_default();
        self.campaign.reserve(mode)?;
        let id = self.next_attempt_id;
        self.next_attempt_id += 1;
        let loadout = if mode == Mode::Personal {
            self.campaign.player.loadout
        } else {
            self.campaign.npc.loadout
        };
        let gear_snapshot = crate::content::Slot::ALL
            .into_iter()
            .map(|slot| {
                let item = self
                    .campaign
                    .item(loadout[slot.index()])
                    .expect("validated loadout");
                let definition = item.definition();
                crate::stats::GearRecord {
                    slot,
                    definition: item.definition.clone(),
                    name: definition.name.clone(),
                    level: item.level,
                    quality: self.campaign.item_quality(item.id).expect("owned"),
                    weapon_class: (slot == crate::content::Slot::Ranged)
                        .then_some(profile.weapon.class),
                }
            })
            .collect();
        let a = Attempt {
            width_yards,
            balance,
            dps: crate::stats::DpsTracker::new(),
            gear_snapshot,
            kill_xp: 0,
            loot_quality_counts: Default::default(),
            lane_count,
            loot_events: Vec::new(),
            progression,
            id,
            level,
            mode,
            tick: 0,
            paused: false,
            country_id: country.map(|c| c.id.clone()),
            country_name: country.map_or_else(|| "Local defense".into(), |c| c.name.clone()),
            stage: country_stage.unwrap_or(level),
            modifiers,
            enemies: vec![],
            corpses: vec![],
            structures: vec![],
            layout_omissions: 0,
            hp: max_hp,
            max_hp,
            stamina,
            max_stamina: stamina,
            ammo: magazine,
            magazine,
            reload_left: 0,
            shot_cooldown: 0,
            melee_cooldown: 0,
            melee_commit: 0,
            supply,
            boss_spawned: false,
            kills: 0,
            coins: 0,
            loot: 0,
            decision: "Ready to deploy".into(),
            events: VecDeque::new(),
            log: vec![],
            latest_resolution: None,
            last_damage_lane: None,
            ordnance: OrdnanceState::for_deployment(mode, &profile.modifiers),
            profile,
            rng: Rng::new(self.seed ^ id.wrapping_mul(0x9E3779B97F4A7C15)),
            injury: InjuryState::new(max_hp, stamina as i32),
            reward_ledger: BTreeSet::new(),
            first_contact: None,
            shots: 0,
            hits: 0,
            active_damage: 0,
            structure_damage: 0,
            reload_ticks: 0,
            aim_ticks: 0,
            next_id: 1,
            telemetry: CombatTelemetry {
                damage_by_lane: vec![0; usize::from(lane_count)],
                ..Default::default()
            },
            maneuver: NpcManeuver::DoNothing,
            decision_target: None,
            defense_bonus: 0,
            spawn_credit: 0.0,
            lane_history: vec![],
            aim_target: None,
            aim_seconds: 0,
            ready_turns: 0,
            npc_holding_melee: false,
            heal_used: false,
            statuses: Statuses::default(),
            caller_tick: 0,
            boss_warning_sent: false,
            caller_lanes: None,
            event_seq: self.next_event_seq,
            emitted: Vec::new(),
        };
        info_span!("battle", attempt = id, mode = ?mode, seed = self.seed).in_scope(|| {
            info!(
                worker = self.worker,
                country = ?a.country_id,
                stage = a.stage,
                level,
                "attempt started"
            );
        });
        self.last_result = None;
        self.attempt = Some(a);
        if !self.worker {
            self.campaign.journey_active = true;
            self.campaign.journey_revision = 1;
        }
        self.global_paused = false;
        for (index, p) in layout.iter().enumerate() {
            if let Err(error) = self.place(p.kind, p.lane as u8, p.column as u8) {
                let a = self.attempt.as_mut().expect("new attempt");
                a.layout_omissions = layout.len() - index;
                a.note(format!(
                    "Saved country plan: {} placements omitted ({error}); full plan preserved.",
                    a.layout_omissions
                ));
                break;
            }
        }
        Ok(())
    }
    pub fn retry(&mut self) -> Result<(), String> {
        let r = self.last_result.clone().ok_or("No previous deployment")?;
        self.deploy_report(&r)
    }
    fn deploy_report(&mut self, r: &Report) -> Result<(), String> {
        if let Some(country) = &r.country_id {
            self.campaign.enter_country(country)?;
            self.deploy_country_stage(r.stage, r.mode)
        } else {
            self.deploy(r.level, r.mode)
        }
    }
    pub fn withdraw(&mut self) {
        if let Some(a) = self.attempt.take() {
            self.finish(
                a,
                Outcome::Withdrawal,
                "Recalled to camp with all banked rewards".into(),
            );
        }
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.global_paused = paused;
        if let Some(a) = &mut self.attempt {
            a.paused = paused;
        }
    }
    pub fn preview_placement(
        &self,
        kind: StructureKind,
        lane: u8,
        column: u8,
    ) -> Result<(), String> {
        let a = self.attempt.as_ref().ok_or("Deploy before building")?;
        if !self
            .campaign
            .has_feature(crate::skill_catalog::CampaignFeature::Construction)
        {
            return Err("Learn construction in the skill tree".into());
        }
        if lane >= a.lane_count || column >= 5 {
            return Err("Footprint must remain wholly in the left-half build zone".into());
        }
        if !self.campaign.blueprints[kind] {
            return Err("Purchase the unlocked blueprint in camp".into());
        }
        if a.supply < content().structure(kind).supply {
            return Err("Not enough build supply".into());
        }
        if a.structures
            .iter()
            .any(|s| s.lane == lane && s.column == column)
        {
            return Err("This cell is occupied".into());
        }
        let x = a.build_cell_center(lane, column).expect("validated cell")[0];
        if a.enemies
            .iter()
            .any(|e| e.lane == lane && (e.x - x).abs() < a.construction_cell_width() * 0.5)
        {
            return Err("An enemy occupies this footprint".into());
        }
        Ok(())
    }
    pub fn place(&mut self, kind: StructureKind, lane: u8, column: u8) -> Result<(), String> {
        self.preview_placement(kind, lane, column)?;
        let a = self.attempt.as_mut().expect("preview checked attempt");
        let d = content().structure(kind);
        a.supply -= d.supply;
        let id = a.next_id;
        a.next_id += 1;
        let durability = (d.health as f32
            * structure_bonus(a, self.campaign.structure_upgrades[kind]))
        .round() as i32;
        a.structures.push(Structure {
            position_x: Some(a.build_cell_center(lane, column).expect("valid cell")[0]),
            build_width: Some(a.construction_cell_width()),
            id,
            kind,
            lane,
            column,
            hp: durability,
            max_hp: durability,
            cooldown: 0,
            charges: d.charges,
            cost: d.supply,
            ready_turns: 0,
        });
        Ok(())
    }
    /// Save the current country's surviving structures without stopping combat.
    pub fn save_country_layout(&mut self) -> Result<(), String> {
        let a = self
            .attempt
            .as_ref()
            .ok_or("Deploy before saving a layout")?;
        let country = a
            .country_id
            .as_deref()
            .ok_or("This is not a country deployment")?;
        let layout = a.structure_layout();
        self.campaign.save_country_layout(country, layout)
    }
    /// Clear the selected country's plan without removing deployed structures.
    pub fn clear_country_layout(&mut self) -> Result<(), String> {
        let country = self
            .attempt
            .as_ref()
            .and_then(|a| a.country_id.as_deref())
            .or(self.campaign.world.active.as_deref())
            .ok_or("Choose a country first")?
            .to_owned();
        self.campaign.clear_country_layout(&country)
    }
    pub fn salvage(&mut self, id: u64) -> Result<u32, String> {
        let a = self.attempt.as_mut().ok_or("No deployment")?;
        let n = a
            .structures
            .iter()
            .position(|s| s.id == id)
            .ok_or("No structure selected")?;
        let s = a.structures.remove(n);
        for e in &mut a.enemies {
            if e.target_structure == Some(id) {
                e.target_structure = None;
                e.contact_since = None;
                e.attack_left = 0;
                e.attack_phase = EnemyPhase::Advance;
                e.windup_total = 0;
            }
        }
        let refund = (s.cost as f32 * 0.5 * s.hp.max(0) as f32 / s.max_hp as f32).floor() as u32;
        a.supply += refund;
        Ok(refund)
    }
    /// Advance one fixed tick. Events it emits, including those of a finishing
    /// attempt and of expeditions, are available from `drain_events`.
    pub fn tick(&mut self, input: Input) {
        self.advance_tick(input);
        self.collect_attempt_events();
    }
    fn advance_tick(&mut self, input: Input) {
        if self.global_paused || self.attempt.as_ref().is_some_and(|a| a.paused) {
            return;
        }
        if !self.worker {
            self.tick_parallel();
            self.strategy.tick_world(&mut self.campaign);
        }
        let Some(mut a) = self.attempt.take() else {
            return;
        };
        if a.paused {
            self.attempt = Some(a);
            return;
        }
        // Per-tick span stays below the shipped `info` filter; attempt start
        // and finish keep their info-level spans.
        let _battle =
            debug_span!("battle", attempt = a.id, mode = ?a.mode, seed = self.seed).entered();
        a.event_seq = a.event_seq.max(self.next_event_seq);
        a.tick += 1;
        trace!(tick = a.tick, enemies = a.enemies.len(), hp = a.hp, "tick");
        a.events
            .retain(|e| a.tick.saturating_sub(e.tick) < u64::from(e.duration_ticks.max(30)));
        a.shot_cooldown = a.shot_cooldown.saturating_sub(1);
        if a.melee_cooldown == 1 {
            a.emit(SimEvent::CooldownReady);
        }
        a.melee_cooldown = a.melee_cooldown.saturating_sub(1);
        a.melee_commit = a.melee_commit.saturating_sub(1);
        a.statuses.tick();
        if a.mode == Mode::Personal {
            self.advance_ordnance(&mut a);
            self.reap(&mut a);
            if a.boss_spawned && !a.enemies.iter().any(|e| e.boss) {
                self.finish(a, Outcome::Victory, "The sector boss was defeated".into());
                return;
            }
            a.stamina = (a.stamina + (12.0 + a.profile.endurance as f32) * DT).min(a.max_stamina);
            if a.reload_left > 0 {
                a.reload_ticks += 1;
                a.reload_left -= 1;
                if a.reload_left == 0 {
                    a.ammo = a.magazine;
                    a.event(
                        EventKind::Reload,
                        a.defender_position(),
                        a.defender_position(),
                        0,
                    );
                }
            }
            self.manual_actions(&mut a, &input);
            self.reap(&mut a);
            if a.boss_spawned && !a.enemies.iter().any(|e| e.boss) {
                self.finish(a, Outcome::Victory, "The sector boss was defeated".into());
                return;
            }
        }
        self.spawn_director(&mut a);
        self.move_enemies(&mut a);
        self.structures_act(&mut a);
        self.reap(&mut a);
        let boss_defeated = a.boss_spawned && !a.enemies.iter().any(|e| e.boss);
        if boss_defeated {
            self.finish(a, Outcome::Victory, "The sector boss was defeated".into());
            return;
        }
        if a.mode == Mode::Npc
            && a.tick.is_multiple_of(60)
            && self.campaign.npc.recall_percent > 0
            && a.hp * 100 <= a.max_hp * self.campaign.npc.recall_percent as i32
        {
            self.finish(
                a,
                Outcome::Withdrawal,
                "Survivor recalled at the configured health threshold".into(),
            );
            return;
        }
        if a.mode == Mode::Npc && a.tick.is_multiple_of(60) {
            self.campaign.record_npc_progress(0, 60);
            self.npc_turn(&mut a);
            debug!(
                maneuver = ?a.maneuver,
                target = ?a.decision_target,
                decision = %a.decision,
                "npc decision"
            );
            a.telemetry.npc_decisions[a.maneuver] += 1;
            self.reap(&mut a);
        }
        if a.boss_spawned && !a.enemies.iter().any(|e| e.boss) {
            self.finish(a, Outcome::Victory, "The sector boss was defeated".into());
            return;
        }
        if a.mode == Mode::Npc && a.injury.terminal() {
            self.finish(
                a,
                Outcome::Defeat,
                "Survivor forced to retreat; rest and train in camp before retrying".into(),
            );
            return;
        }
        self.enemy_attacks(&mut a);
        self.reap(&mut a);
        if a.boss_spawned && !a.enemies.iter().any(|e| e.boss) {
            self.finish(a, Outcome::Victory, "The sector boss was defeated".into());
            return;
        }
        if a.hp <= 0 && a.mode == Mode::Personal {
            let lane = a.last_damage_lane.map_or(6, |l| l + 1);
            self.finish(
                a,
                Outcome::Defeat,
                format!(
                    "Retreated from the overrun line in lane {lane}; rest and train before retrying"
                ),
            );
            return;
        }
        if a.mode == Mode::Npc && a.injury.terminal() {
            let cause = "Survivor evacuated to camp; rest and train before retrying";
            self.finish(a, Outcome::Defeat, cause.into());
            return;
        }
        self.attempt = Some(a);
    }
    fn manual_actions(&mut self, a: &mut Attempt, input: &Input) {
        if input.heal && !a.heal_used && a.profile.heal && a.hp < a.max_hp {
            a.heal_used = true;
            let healed = (a.max_hp - a.hp).min(35);
            a.hp = (a.hp + 35).min(a.max_hp);
            a.event(
                EventKind::Ability,
                a.defender_position(),
                a.defender_position(),
                healed,
            );
        }
        if input.aim.iter().any(|value| !value.is_finite()) {
            return;
        }
        if input.brace
            && a.profile.brace
            && !a.statuses.active(StatusId::Brace)
            && a.stamina >= 30.0
        {
            a.stamina -= 30.0;
            a.statuses.refresh(StatusId::Brace, BRACE_DURATION_TICKS);
            a.event(
                EventKind::Ability,
                a.defender_position(),
                a.defender_position(),
                0,
            );
        }
        let lane = input.aim[1].floor().clamp(0.0, f32::from(a.lane_count - 1)) as u8;
        let target = a.enemies.iter().position(|e| e.lane == lane && e.contact());
        if input.melee
            && a.melee_cooldown == 0
            && a.melee_commit == 0
            && a.stamina >= 25.0
            && let Some(n) = target
        {
            a.telemetry.canceled_reload_ticks += u64::from(a.reload_left);
            a.reload_left = 0;
            a.stamina -= 25.0;
            a.melee_cooldown = 72;
            a.melee_commit = 30;
            let damage = (a.profile.personal.melee_damage - enemy_armor(a, &a.enemies[n])).max(0);
            let point = [a.enemies[n].x, a.enemies[n].y()];
            a.enemies[n].apply_damage(damage, a.mode);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Defender, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            let meta = EventMetadata {
                target_id: Some(a.enemies[n].id),
                lane: Some(lane),
                ..Default::default()
            };
            a.event_with(EventKind::Melee, a.defender_position(), point, damage, meta);
            if damage == 0 {
                a.telemetry.absorbed += 1;
                a.event_with(EventKind::Armor, a.defender_position(), point, 0, meta);
            }
            if a.enemies[n].boss && enemy_defeated(a, &a.enemies[n]) {
                return;
            }
            if a.profile.sweep {
                let mut total = 0;
                let targets: Vec<_> = a
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.contact() && e.lane.abs_diff(lane) == 1)
                    .map(|(n, _)| n)
                    .collect();
                for n in targets {
                    let secondary = (a.profile.personal.melee_damage / 2
                        - enemy_armor(a, &a.enemies[n]))
                    .max(0);
                    let e = &mut a.enemies[n];
                    if e.contact() && e.lane.abs_diff(lane) == 1 {
                        e.apply_damage(secondary, a.mode);
                        total += secondary;
                    }
                }
                a.active_damage += total as u64;
                a.dps
                    .record(a.tick, crate::stats::DamageSource::Defender, total as u64);
                a.telemetry.defender_damage += total as u64;
            }
            return;
        }
        if a.melee_commit > 0 {
            return;
        }
        if let Some(request) = input.throw {
            self.throw_ordnance(a, request);
            return;
        }
        if (input.reload || (input.auto_reload && a.ammo == 0))
            && a.reload_left == 0
            && a.ammo < a.magazine
        {
            a.reload_left = seconds(a.profile.personal.reload.max(0.5));
            return;
        }
        if !input.fire || a.shot_cooldown > 0 || a.reload_left > 0 || a.ammo == 0 {
            return;
        }
        let dx = input.aim[0] - a.defender_position()[0];
        let dy = input.aim[1] - a.defender_position()[1];
        let length = dx.hypot(dy);
        if length < 0.01 {
            return;
        }
        let base_angle = dy.atan2(dx);
        let pellets = a.profile.weapon.pellets;
        let count = if a.profile.pierce { 2 } else { 1 };
        // A deterministic fan consumes one shell. Damage is aggregated by stable
        // enemy ID before applying the per-trigger pistol cap and injury feedback.
        let mut impacts = std::collections::BTreeMap::<u64, (usize, i32, bool)>::new();
        a.ammo -= 1;
        a.shots += 1;
        a.shot_cooldown = seconds(a.profile.personal.cadence.max(0.08));
        for pellet in 0..pellets {
            let offset = if pellets == 1 {
                0.0
            } else {
                pellet as f32 / (pellets - 1) as f32 - 0.5
            } * a.profile.weapon.spread_radians;
            let angle = base_angle + offset;
            let direction = [angle.cos(), angle.sin()];
            let mut candidates = Vec::new();
            for (n, e) in a.enemies.iter().enumerate() {
                if !e.in_view() || e.hp <= 0 {
                    continue;
                }
                let v = [
                    e.x - a.defender_position()[0],
                    e.y() - a.defender_position()[1],
                ];
                let along = v[0] * direction[0] + v[1] * direction[1];
                let perpendicular = (v[0] * direction[1] - v[1] * direction[0]).abs();
                let radius = if e.boss { 0.55 } else { 0.34 };
                if along > 0.0 && along < 45.0 && perpendicular <= radius {
                    candidates.push((n, along, perpendicular));
                }
            }
            candidates.sort_by(|x, y| {
                x.1.total_cmp(&y.1)
                    .then(a.enemies[x.0].id.cmp(&a.enemies[y.0].id))
            });
            let endpoint = candidates.iter().take(count).next_back().map_or(
                [
                    a.defender_position()[0] + direction[0] * 40.0,
                    a.defender_position()[1] + direction[1] * 40.0,
                ],
                |&(n, _, _)| [a.enemies[n].x, a.enemies[n].y()],
            );
            a.event(EventKind::Shot, a.defender_position(), endpoint, 0);
            if candidates.is_empty() {
                a.event(EventKind::Miss, a.defender_position(), endpoint, 0);
            }
            for &(n, _, perpendicular) in candidates.iter().take(count) {
                let weak = perpendicular < 0.12;
                let raw = (a.profile.personal.damage as f32
                    * if weak {
                        1.0 + a.profile.personal.weak_bonus
                    } else {
                        1.0
                    })
                .round() as i32;
                let damage = (raw - enemy_armor(a, &a.enemies[n])).max(0);
                let impact = impacts.entry(a.enemies[n].id).or_insert((n, 0, false));
                impact.1 = impact.1.saturating_add(damage);
                impact.2 |= weak;
            }
        }
        if impacts.is_empty() {
            a.telemetry.misses += 1;
        }
        let mut impacts: Vec<_> = impacts.into_iter().collect();
        impacts.sort_by(|(left_id, (left, _, _)), (right_id, (right, _, _))| {
            let distance = |index: usize| {
                (a.enemies[index].x - a.defender_position()[0])
                    .hypot(a.enemies[index].y() - a.defender_position()[1])
            };
            distance(*left)
                .total_cmp(&distance(*right))
                .then(left_id.cmp(right_id))
        });
        for (_, (n, damage, weak)) in impacts {
            let damage = damage.min(a.profile.weapon.hit_cap(a.enemies[n].max_hp));
            a.enemies[n].apply_damage(damage, a.mode);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Defender, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            if damage == 0 {
                a.telemetry.absorbed += 1;
            } else if weak {
                a.telemetry.weak_hits += 1;
            }
            a.hits += 1;
            let pos = [a.enemies[n].x, a.enemies[n].y()];
            let meta = EventMetadata {
                target_id: Some(a.enemies[n].id),
                lane: Some(a.enemies[n].lane),
                ..Default::default()
            };
            a.event_with(
                if damage == 0 {
                    EventKind::Armor
                } else if weak {
                    EventKind::WeakPoint
                } else {
                    EventKind::Hit
                },
                a.defender_position(),
                pos,
                damage,
                meta,
            );
            if a.enemies[n].boss && enemy_defeated(a, &a.enemies[n]) {
                break;
            }
        }
    }

    fn throw_ordnance(&mut self, a: &mut Attempt, request: ThrowRequest) {
        let index = request.kind.index();
        if a.mode != Mode::Personal
            || !ordnance::valid_cell_for_arena(request.cell, a.lane_count, a.width_yards)
            || a.ordnance.charges[index] == 0
            || a.ordnance.cooldowns[index] > 0
            || a.reload_left > 0
            || a.melee_commit > 0
            || a.ordnance.projectiles.len() + a.ordnance.fire_zones.len()
                >= ordnance::MAX_ACTIVE_ORDNANCE
            || !a.profile.modifiers.unlocked(request.kind.unlock())
        {
            return;
        }
        let definition = ordnance::catalog().definition(request.kind);
        let modifiers = &a.profile.modifiers;
        let damage_multiplier = modifiers.ordnance_damage_multiplier
            * a.modifiers.personal_damage
            * a.progression.fortification_multiplier;
        let projectile = ThrownOrdnance {
            id: a.next_id,
            kind: request.kind,
            cell: request.cell,
            flight_left: definition.flight_ticks,
            flight_ticks: definition.flight_ticks,
            damage: (definition.damage as f32 * damage_multiplier)
                .round()
                .clamp(1.0, 100_000.0) as i32,
            radius: (definition.radius * modifiers.ordnance_radius_multiplier).clamp(0.1, 15.0),
            edge_damage_fraction: definition.edge_damage_fraction,
            burn_damage: if definition.burn_damage > 0 {
                (definition.burn_damage as f32 * damage_multiplier)
                    .round()
                    .clamp(1.0, 100_000.0) as i32
            } else {
                0
            },
            burn_ticks: if definition.burn_ticks > 0 {
                (definition.burn_ticks as f32 * modifiers.burn_duration_multiplier)
                    .round()
                    .clamp(1.0, 3600.0) as u32
            } else {
                0
            },
            burn_interval: definition.burn_interval,
            release_cues: Some(ordnance::ReleaseCues {
                tick: a.tick,
                brace: a.statuses.active(StatusId::Brace),
                last_shot_tick: a
                    .events
                    .iter()
                    .rev()
                    .find(|event| {
                        event.kind == EventKind::Shot
                            && event.actor_id.is_none()
                            && event.tick <= a.tick
                    })
                    .map(|event| event.tick),
                last_hit_tick: a
                    .events
                    .iter()
                    .rev()
                    .find(|event| {
                        event.kind == EventKind::Damage
                            && event.target_id.is_none()
                            && event.value > 0
                            && event.tick <= a.tick
                    })
                    .map(|event| event.tick),
            }),
        };
        a.next_id += 1;
        a.ordnance.next_id += 1;
        a.ordnance.charges[index] -= 1;
        a.ordnance.spent[index] += 1;
        a.ordnance.cooldowns[index] = (definition.cooldown_ticks as f32
            * modifiers.ordnance_cooldown_multiplier)
            .round()
            .clamp(1.0, 3600.0) as u32;
        a.event_with(
            EventKind::Throw,
            a.defender_position(),
            ordnance::cell_center(request.cell),
            index as i32,
            EventMetadata {
                actor_id: Some(projectile.id),
                lane: Some(request.cell[1]),
                duration_ticks: projectile.flight_ticks,
                ordnance: Some(projectile.kind),
                ..Default::default()
            },
        );
        a.ordnance.projectiles.push(projectile);
    }
    fn advance_ordnance(&mut self, a: &mut Attempt) {
        for cooldown in &mut a.ordnance.cooldowns {
            *cooldown = cooldown.saturating_sub(1);
        }
        let mut pulses = Vec::new();
        for zone in &mut a.ordnance.fire_zones {
            zone.ticks_left = zone.ticks_left.saturating_sub(1);
            zone.tick_left = zone.tick_left.saturating_sub(1);
            if zone.tick_left == 0 {
                pulses.push((zone.id, zone.cell, zone.radius, zone.damage));
                zone.tick_left = zone.tick_interval;
            }
        }
        a.ordnance.fire_zones.retain(|z| z.ticks_left > 0);
        for (id, cell, radius, damage) in pulses {
            self.ordnance_area_damage(a, (id, OrdnanceKind::Molotov), cell, radius, damage, 1.0);
        }
        let mut impacts = Vec::new();
        for projectile in &mut a.ordnance.projectiles {
            projectile.flight_left = projectile.flight_left.saturating_sub(1);
            if projectile.flight_left == 0 {
                impacts.push(projectile.clone());
            }
        }
        a.ordnance.projectiles.retain(|p| p.flight_left > 0);
        for projectile in impacts {
            let point = ordnance::cell_center(projectile.cell);
            // Area-event value is radius in hundredths of a yard. Individual
            // injury events below carry damage and the resolved hostile ID.
            a.event_with(
                EventKind::Explosion,
                a.defender_position(),
                point,
                (projectile.radius * 100.0).round() as i32,
                EventMetadata {
                    actor_id: Some(projectile.id),
                    lane: Some(projectile.cell[1]),
                    duration_ticks: 30,
                    ordnance: Some(projectile.kind),
                    ..Default::default()
                },
            );
            self.ordnance_area_damage(
                a,
                (projectile.id, projectile.kind),
                projectile.cell,
                projectile.radius,
                projectile.damage,
                projectile.edge_damage_fraction,
            );
            if projectile.kind == OrdnanceKind::Molotov {
                a.event_with(
                    EventKind::Burn,
                    a.defender_position(),
                    point,
                    (projectile.radius * 100.0).round() as i32,
                    EventMetadata {
                        actor_id: Some(projectile.id),
                        lane: Some(projectile.cell[1]),
                        duration_ticks: projectile.burn_ticks,
                        ordnance: Some(OrdnanceKind::Molotov),
                        ..Default::default()
                    },
                );
                a.ordnance.fire_zones.push(FireZone {
                    id: projectile.id,
                    cell: projectile.cell,
                    radius: projectile.radius,
                    damage: projectile.burn_damage,
                    ticks_left: projectile.burn_ticks,
                    tick_left: projectile.burn_interval,
                    tick_interval: projectile.burn_interval,
                });
            }
        }
    }
    fn ordnance_area_damage(
        &mut self,
        a: &mut Attempt,
        (source, ordnance): (u64, OrdnanceKind),
        cell: [u8; 2],
        radius: f32,
        raw_damage: i32,
        edge_fraction: f32,
    ) {
        let center = ordnance::cell_center(cell);
        let mut targets: Vec<_> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| {
                enemy.hp > 0
                    && enemy.in_view()
                    && (enemy.x - center[0]).hypot(enemy.y() - center[1]) <= radius
            })
            .map(|(index, enemy)| (enemy.id, index))
            .collect();
        targets.sort_unstable_by_key(|(id, _)| *id);
        for (_, index) in targets {
            let enemy = &a.enemies[index];
            let point = [enemy.x, enemy.y()];
            let distance = (point[0] - center[0]).hypot(point[1] - center[1]);
            let raw = (raw_damage as f32 * (1.0 - (1.0 - edge_fraction) * (distance / radius)))
                .round() as i32;
            let damage = (raw - enemy_armor(a, enemy)).max(0);
            let meta = EventMetadata {
                actor_id: Some(source),
                target_id: Some(enemy.id),
                lane: Some(enemy.lane),
                ordnance: Some(ordnance),
                ..Default::default()
            };
            a.enemies[index].apply_damage(damage, Mode::Personal);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Ordnance, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            if damage == 0 {
                a.telemetry.absorbed += 1;
            }
            a.event_with(
                if damage == 0 {
                    EventKind::Armor
                } else {
                    EventKind::Hit
                },
                center,
                point,
                damage,
                meta,
            );
        }
    }
    fn spawn_director(&mut self, a: &mut Attempt) {
        let level = &content().levels[a.level];
        let boss_tick = u64::from(seconds(a.boss_time()));
        let warning = u64::from(seconds(level.boss_timing.warning));
        if !a.boss_spawned && !a.boss_warning_sent && a.tick >= boss_tick.saturating_sub(warning) {
            a.boss_warning_sent = true;
            a.event_with(
                EventKind::BossWarning,
                [a.width_yards, a.defender_position()[1]],
                [a.width_yards, a.defender_position()[1]],
                (boss_tick.saturating_sub(a.tick) / 60) as i32,
                EventMetadata {
                    lane: Some(a.lane_count / 2),
                    duration_ticks: warning as u32,
                    ..Default::default()
                },
            );
            a.note(format!(
                "{} approaches in {:.1}s",
                level.boss_name,
                a.boss_countdown()
            ));
        }
        if !a.boss_spawned && a.elapsed() >= a.boss_time() {
            a.boss_spawned = true;
            self.spawn_enemy(a, EnemyKind::Brute, a.lane_count / 2, true);
            a.caller_tick = a.tick + u64::from(seconds(level.boss_timing.initial_call));
            a.telemetry.boss_arrival_tick = Some(a.tick);
            a.note(format!(
                "{} has arrived. Defeat it to secure the sector.",
                level.boss_name
            ));
        }
        // Credits count groups, preserving the authored mean enemy rate while
        // higher ranks deliver a larger simultaneous, independently laned horde.
        a.spawn_credit += a.arrival_rate() * DT / a.progression.batch_size as f32;
        if a.spawn_credit >= 1.0 {
            a.spawn_credit -= 1.0;
            for _ in 0..a.progression.batch_size {
                if a.enemies.iter().filter(|e| !e.boss).count() < a.enemy_cap() {
                    let lane = choose_lane(a);
                    let pool = crate::enemy_catalog::enemy_catalog().wave_pool(
                        a.level.max(a.progression.wave_tier),
                        &self.campaign.unlocked_bosses(),
                    );
                    let total = pool.iter().map(|entry| u64::from(entry.weight)).sum();
                    let mut roll = a.rng.below(total);
                    for entry in &pool {
                        if roll < u64::from(entry.weight) {
                            self.spawn_archetype(a, &entry.archetype_id, lane)
                                .expect("validated eligible wave archetype");
                            break;
                        }
                        roll -= u64::from(entry.weight);
                    }
                } else {
                    a.telemetry.skipped_spawns += 1;
                }
            }
        }
        if a.boss_spawned && level.boss == BossKind::Caller {
            if a.caller_lanes.is_none()
                && a.tick
                    >= a.caller_tick
                        .saturating_sub(u64::from(seconds(level.boss_timing.call_warning)))
            {
                let lane = a.rng.below(u64::from(a.lane_count)) as u8;
                let lanes = [lane, (lane + a.lane_count / 2) % a.lane_count];
                a.caller_lanes = Some(lanes);
                for lane in lanes {
                    a.event_with(
                        EventKind::Reinforcements,
                        [a.width_yards, lane as f32 + 0.5],
                        [a.width_yards, lane as f32 + 0.5],
                        1,
                        EventMetadata {
                            lane: Some(lane),
                            duration_ticks: seconds(level.boss_timing.call_warning),
                            ..Default::default()
                        },
                    );
                }
                a.note(format!(
                    "Caller signals lanes {} and {} · reinforcements in {:.1}s",
                    lanes[0] + 1,
                    lanes[1] + 1,
                    level.boss_timing.call_warning
                ));
            }
            if a.tick >= a.caller_tick {
                a.caller_tick = a.tick + u64::from(seconds(level.boss_timing.call_interval));
                for l in a.caller_lanes.take().unwrap_or([0, a.lane_count / 2]) {
                    if a.enemies.iter().filter(|e| !e.boss).count() < a.enemy_cap() {
                        self.spawn_enemy(a, EnemyKind::Runner, l, false);
                    } else {
                        a.telemetry.skipped_spawns += 1;
                    }
                }
            }
        }
        if let Some(boss) = a.enemies.iter().find(|e| e.boss) {
            let exposed = boss.exposed_at(a.tick);
            if exposed != boss.exposed_at(a.tick.saturating_sub(1))
                || (exposed && boss.born_tick == a.tick)
            {
                let point = [boss.x, boss.y()];
                let id = boss.id;
                let lane = boss.lane;
                let duration = if exposed {
                    boss.boss_timing.exposure_duration
                } else {
                    boss.boss_timing.cycle - boss.boss_timing.exposure_duration
                };
                a.event_with(
                    EventKind::BossExposure,
                    point,
                    point,
                    i32::from(exposed),
                    EventMetadata {
                        actor_id: Some(id),
                        lane: Some(lane),
                        duration_ticks: seconds(duration),
                        ..Default::default()
                    },
                );
            }
        }
    }
    pub fn spawn_enemy(&mut self, a: &mut Attempt, kind: EnemyKind, lane: u8, boss: bool) {
        let resolved = crate::enemy_catalog::resolve_kind(kind);
        self.spawn_resolved_enemy(a, resolved, lane, boss);
    }
    pub fn spawn_archetype(&mut self, a: &mut Attempt, id: &str, lane: u8) -> Result<(), String> {
        let resolved = crate::enemy_catalog::enemy_catalog().resolve(id)?;
        if resolved
            .unlocked_by
            .is_some_and(|boss| !self.campaign.unlocked_bosses().contains(&boss))
        {
            return Err("Defeat this boss before its ordinary variant joins the horde".into());
        }
        self.spawn_resolved_enemy(a, resolved, lane, false);
        Ok(())
    }
    fn spawn_resolved_enemy(
        &mut self,
        a: &mut Attempt,
        resolved: crate::enemy_catalog::ResolvedEnemy,
        lane: u8,
        boss: bool,
    ) {
        let kind = resolved.kind;
        let d = &resolved;
        let level = &content().levels[a.level];
        if (boss && a.enemies.iter().any(|e| e.boss))
            || (!boss && a.enemies.iter().filter(|e| !e.boss).count() >= a.enemy_cap())
        {
            return;
        }
        let lane = lane.min(a.lane_count - 1);
        let spawn_x = a
            .enemies
            .iter()
            .filter(|e| e.lane == lane)
            .fold(a.width_yards + 0.7, |tail, e| tail.max(e.x + 0.65));
        let base_hp = if a.mode == Mode::Personal {
            if boss {
                level.boss_health.max(a.progression.boss_health_floor)
            } else {
                d.health
            }
        } else if boss {
            level.boss_npc_hp.max(a.progression.npc_boss_health_floor)
        } else {
            d.npc_hp
        };
        let max_hp = (base_hp as f32 * a.modifiers.enemy_health).round().max(1.0) as i32;
        let id = a.next_id;
        a.next_id += 1;
        a.enemies.push(Enemy {
            arena_width: a.width_yards,
            pass_offset: 0.0,
            id,
            kind,
            lane,
            x: spawn_x,
            hp: max_hp,
            max_hp,
            boss,
            attack_left: 0,
            armor: if a.mode == Mode::Npc {
                if boss {
                    level.boss_npc_armor
                } else {
                    d.npc_armor
                }
            } else if boss {
                level.boss_armor
            } else {
                d.armor
            },
            injury: InjuryState::new(max_hp, 10),
            born_tick: a.tick,
            contact_since: None,
            target_structure: None,
            ready_turns: 0,
            boss_kind: if boss { Some(level.boss) } else { None },
            boss_timing: level.boss_timing.clone(),
            attack_phase: EnemyPhase::Advance,
            windup_total: 0,
            statuses: Statuses::default(),
            resolved: (!boss).then_some(resolved),
        });
        a.telemetry.spawned += 1;
        a.telemetry.peak_enemies = a.telemetry.peak_enemies.max(a.enemies.len());
        a.event_with(
            if boss {
                EventKind::Boss
            } else {
                EventKind::Spawn
            },
            [a.width_yards, lane as f32 + 0.5],
            [a.width_yards, lane as f32 + 0.5],
            0,
            EventMetadata {
                actor_id: Some(id),
                lane: Some(lane),
                duration_ticks: 72,
                ..Default::default()
            },
        );
    }
    fn move_enemies(&mut self, a: &mut Attempt) {
        for lane in 0..a.lane_count {
            let mut order: Vec<usize> = a
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| e.lane == lane)
                .map(|(n, _)| n)
                .collect();
            order.sort_by(|&n, &m| {
                a.enemies[n]
                    .x
                    .total_cmp(&a.enemies[m].x)
                    .then(a.enemies[n].id.cmp(&a.enemies[m].id))
            });
            let mut offsets = std::collections::BTreeMap::<usize, f32>::new();
            for pair in order.windows(2) {
                let (front, back) = (pair[0], pair[1]);
                let ahead = &a.enemies[front];
                let behind = &a.enemies[back];
                if ahead.condition(a.mode) == EnemyCondition::Defeated
                    || behind.condition(a.mode) == EnemyCondition::Defeated
                {
                    continue;
                }
                let gap = behind.x - ahead.x;
                let front_speed = enemy_step_speed(a, ahead);
                let back_speed = enemy_step_speed(a, behind);
                let passing =
                    (back_speed > front_speed + 0.01 && gap < 1.4 && ahead.contact_since.is_none())
                        || (front_speed > back_speed + 0.01
                            && gap < 0.9
                            && (ahead.pass_offset.abs() > 0.01 || behind.pass_offset.abs() > 0.01));
                if passing {
                    let (fast, slow) = if back_speed > front_speed {
                        (back, front)
                    } else {
                        (front, back)
                    };
                    let sign = if lane + 1 == a.lane_count { -1.0 } else { 1.0 };
                    offsets.insert(fast, sign * MAX_PASS_OFFSET);
                    offsets.insert(
                        slow,
                        if lane == 0 || lane + 1 == a.lane_count {
                            0.0
                        } else {
                            -MAX_PASS_OFFSET
                        },
                    );
                } else if gap < 0.65
                    && (ahead.pass_offset.abs() > 0.01 || behind.pass_offset.abs() > 0.01)
                {
                    offsets.entry(front).or_insert(ahead.pass_offset);
                    offsets.entry(back).or_insert(behind.pass_offset);
                }
            }
            let mut queue_front: Option<f32> = None;
            for n in order {
                let e = &a.enemies[n];
                if e.condition(a.mode) == EnemyCondition::Defeated {
                    continue;
                }
                let previous_target = e.target_structure;
                let mut stop = DEFENSE_LINE;
                let mut blocker = None;
                for s in &a.structures {
                    if s.lane == lane
                        && s.kind != StructureKind::Trap
                        && s.hp > 0
                        && s.x() <= e.x + s.blocking_radius()
                        && s.x() + s.blocking_radius() > stop
                    {
                        stop = s.x() + s.blocking_radius();
                        blocker = Some(s.id);
                    }
                }
                if let Some(front) = queue_front
                    && front + 0.65 > stop
                {
                    stop = front + 0.65;
                    blocker = None;
                }
                let speed = if e.boss { 0.6 } else { enemy_profile(e).speed };
                let injury = &a.enemies[n].injury;
                let slow = if a.enemies[n].statuses.active(StatusId::HitStop)
                    || (a.mode == Mode::Npc && (injury.prone || injury.stunned))
                {
                    0.0
                } else if a.mode == Mode::Npc {
                    let hp = if injury.hp * 3 < injury.max_hp {
                        0.5
                    } else {
                        1.0
                    };
                    let fp = if injury.fp * 3 < injury.max_fp {
                        0.5
                    } else {
                        1.0
                    };
                    hp * fp
                } else {
                    1.0
                };
                a.enemies[n].x = (a.enemies[n].x - speed * DT * slow * a.modifiers.enemy_speed)
                    .max(stop.min(a.enemies[n].x));
                a.enemies[n].statuses.tick();
                let blocked = a.enemies[n].x <= stop + 0.01;
                a.enemies[n].target_structure = if blocked { blocker } else { None };
                if blocked && (stop <= DEFENSE_LINE + 0.01 || blocker.is_some()) {
                    if a.enemies[n].contact_since.is_none() || previous_target != blocker {
                        a.enemies[n].contact_since = Some(a.tick);
                        let duration = seconds(enemy_profile(&a.enemies[n]).windup);
                        a.enemies[n].attack_left = duration;
                        a.enemies[n].windup_total = duration;
                        a.enemies[n].attack_phase = EnemyPhase::Windup;
                        let enemy = &a.enemies[n];
                        let point = [enemy.x, enemy.y()];
                        let meta = EventMetadata {
                            actor_id: Some(enemy.id),
                            target_id: blocker,
                            lane: Some(lane),
                            duration_ticks: duration,
                            ..Default::default()
                        };
                        a.event_with(EventKind::Contact, point, point, 0, meta);
                        a.event_with(
                            EventKind::Windup,
                            point,
                            if blocker.is_none() {
                                a.defender_position()
                            } else {
                                point
                            },
                            0,
                            meta,
                        );
                        if stop <= DEFENSE_LINE + 0.01 && a.first_contact.is_none() {
                            a.first_contact = Some(a.elapsed());
                        }
                    }
                } else {
                    a.enemies[n].contact_since = None;
                    a.enemies[n].attack_left = 0;
                    a.enemies[n].windup_total = 0;
                    a.enemies[n].attack_phase = if blocked {
                        EnemyPhase::Queued
                    } else {
                        EnemyPhase::Advance
                    };
                }
                queue_front = blocked.then_some(a.enemies[n].x);
                let target = offsets.get(&n).copied().unwrap_or(0.0);
                let difference = target - a.enemies[n].pass_offset;
                a.enemies[n].pass_offset += difference.clamp(-2.0 * DT * slow, 2.0 * DT * slow);
            }
        }
    }
    fn structures_act(&mut self, a: &mut Attempt) {
        for n in 0..a.structures.len() {
            a.structures[n].cooldown = a.structures[n].cooldown.saturating_sub(1);
            if a.mode == Mode::Npc && a.tick.is_multiple_of(60) && a.structures[n].ready_turns > 0 {
                a.structures[n].ready_turns -= 1;
                continue;
            }
            if a.structures[n].kind == StructureKind::Wall || a.structures[n].cooldown > 0 {
                continue;
            }
            if a.mode == Mode::Npc && !a.tick.is_multiple_of(60) {
                continue;
            }
            let s = &a.structures[n];
            let d = content().structure(s.kind);
            let origin = [s.x(), s.y()];
            let mut candidates: Vec<usize> = a
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if (a.mode == Mode::Personal && e.hp <= 0)
                        || (a.mode == Mode::Npc && e.injury.terminal())
                    {
                        return false;
                    }
                    if s.kind == StructureKind::Trap {
                        s.charges > 0 && e.lane == s.lane && (e.x - s.x()).abs() <= 0.65
                    } else {
                        (e.x - s.x()).hypot(e.y() - s.y()) <= d.range
                    }
                })
                .map(|(n, _)| n)
                .collect();
            candidates.sort_by(|&n, &m| {
                a.enemies[n]
                    .x
                    .total_cmp(&a.enemies[m].x)
                    .then(a.enemies[n].id.cmp(&a.enemies[m].id))
            });
            let Some(&target) = candidates.first() else {
                continue;
            };
            a.structures[n].cooldown =
                seconds(d.cooldown).max(if a.mode == Mode::Npc { 60 } else { 1 });
            if a.structures[n].kind == StructureKind::Trap {
                a.structures[n].charges -= 1;
            }
            let point = [a.enemies[target].x, a.enemies[target].y()];
            let meta = EventMetadata {
                actor_id: Some(a.structures[n].id),
                target_id: Some(a.enemies[target].id),
                lane: Some(a.enemies[target].lane),
                ..Default::default()
            };
            a.event_with(EventKind::Shot, origin, point, 0, meta);
            let bonus = structure_bonus(a, self.campaign.structure_upgrades[d.kind]);
            let damage = if a.mode == Mode::Personal {
                let damage = ((d.damage as f32 * bonus).round() as i32
                    - enemy_armor(a, &a.enemies[target]))
                .max(0);
                if damage == 0 {
                    a.telemetry.absorbed += 1;
                }
                a.event_with(
                    if damage == 0 {
                        EventKind::Armor
                    } else {
                        EventKind::Hit
                    },
                    origin,
                    point,
                    damage,
                    meta,
                );
                damage
            } else {
                let e = &a.enemies[target];
                let distance = (point[0] - origin[0]).hypot(point[1] - origin[1]);
                let attack = Attack {
                    skill: d.npc_skill + rules::range_penalty(distance),
                    damage: Damage {
                        dice: d.npc.damage_dice,
                        adds: d.npc.damage_add + ((bonus - 1.0) * 4.0).round() as i32,
                        kind: d.npc.damage_kind,
                    },
                    armor_divisor: 1,
                };
                let dodge = e.injury.dodge(enemy_dodge(a, e));
                let armor = enemy_armor(a, e);
                let ht = enemy_ht(a, e);
                let result = rules::resolve_attack(
                    &mut a.rng,
                    attack,
                    Some(dodge),
                    armor,
                    &mut a.enemies[target].injury,
                    ht,
                );
                if result.requires_ready {
                    a.structures[n].ready_turns = 1;
                }
                a.resolution_event(&result, origin, point, meta, true);
                a.resolved(
                    meta.actor_id,
                    meta.target_id,
                    &result,
                    format!(
                        "{} → lane {}: {}",
                        d.name,
                        a.enemies[target].lane + 1,
                        result.explanation()
                    ),
                );
                result.injury
            };
            a.enemies[target].apply_damage(damage, a.mode);
            a.structure_damage += damage.max(0) as u64;
            a.dps.record(
                a.tick,
                crate::stats::DamageSource::Structures,
                damage.max(0) as u64,
            );
            if a.structures[n].kind == StructureKind::Trap {
                a.telemetry.trap_damage += damage.max(0) as u64;
            } else {
                a.telemetry.turret_damage += damage.max(0) as u64;
            }
            if a.enemies[target].boss && enemy_defeated(a, &a.enemies[target]) {
                return;
            }
        }
    }
    fn npc_turn(&mut self, a: &mut Attempt) {
        a.defense_bonus = 0;
        a.maneuver = NpcManeuver::DoNothing;
        a.decision_target = None;
        if !a.injury.start_turn(&mut a.rng, a.profile.stats.ht) {
            a.hp = a.injury.hp;
            a.decision = "Incapacitated · unable to begin a maneuver".into();
            return;
        }
        if a.injury.stunned {
            a.maneuver = NpcManeuver::Recover;
            a.injury.recover_stun(&mut a.rng, a.profile.stats.ht);
            a.decision = "Do Nothing · recover from stun".into();
            a.injury.clear_shock();
            return;
        }
        if a.injury.prone {
            a.maneuver = NpcManeuver::Posture;
            a.injury.prone = false;
            a.decision = "Change Posture · stand in place".into();
            a.injury.clear_shock();
            return;
        }
        if a.injury.weapon_dropped {
            a.maneuver = NpcManeuver::Ready;
            a.injury.weapon_dropped = false;
            a.decision = "Ready · retrieve dropped weapon".into();
            a.injury.clear_shock();
            return;
        }
        if !a
            .injury
            .fatigue_allows_action(&mut a.rng, a.profile.stats.iq)
        {
            a.maneuver = NpcManeuver::Recover;
            a.decision = "Do Nothing · exhausted".into();
            a.injury.clear_shock();
            return;
        }
        if a.ready_turns > 0 {
            a.maneuver = NpcManeuver::Ready;
            a.ready_turns -= 1;
            a.decision = "Ready · recover weapon".into();
            a.injury.clear_shock();
            return;
        }
        if a.reload_left > 0 {
            a.maneuver = NpcManeuver::Reload;
            a.reload_ticks += 60;
            a.reload_left = a.reload_left.saturating_sub(60);
            a.decision = format!(
                "Ready · reload, {} turns remaining",
                a.reload_left.div_ceil(60)
            );
            if a.reload_left == 0 {
                a.ammo = a.magazine;
                a.event(
                    EventKind::Reload,
                    a.defender_position(),
                    a.defender_position(),
                    0,
                );
            }
            a.injury.clear_shock();
            return;
        }
        let policy = self.campaign.npc.policy;
        let target = select_npc_target(a, policy);
        let Some(target) = target else {
            a.decision = "Do Nothing · watching the approach".into();
            a.aim_target = None;
            a.aim_seconds = 0;
            a.injury.clear_shock();
            return;
        };
        let e = &a.enemies[target];
        a.decision_target = Some(e.id);
        let contact = e.contact();
        let id = e.id;
        let lane = e.lane;
        let imminent = a.enemies.iter().any(|e| e.x < 5.0);
        let melee = contact && (a.ammo == 0 || policy == Policy::Guardian);
        if melee != a.npc_holding_melee {
            a.maneuver = NpcManeuver::Ready;
            a.npc_holding_melee = melee;
            a.aim_target = None;
            a.aim_seconds = 0;
            a.decision = if melee {
                "Ready · sling rifle and draw melee weapon".into()
            } else {
                "Ready · ready rifle".into()
            };
            a.injury.clear_shock();
            return;
        }
        if (a.ammo == 0
            || (self.campaign.npc.early_reload && a.ammo <= a.magazine / 4 && !imminent))
            && !melee
        {
            a.maneuver = NpcManeuver::Reload;
            a.reload_left = a.profile.npc.reload_turns.saturating_sub(1) * 60;
            a.reload_ticks += 60;
            a.decision = format!("Ready · reload for {} turns", a.profile.npc.reload_turns);
            if a.reload_left == 0 {
                a.ammo = a.magazine;
            }
            a.aim_target = None;
            a.aim_seconds = 0;
            a.injury.clear_shock();
            return;
        }
        let current_range = if contact {
            1.0
        } else {
            (e.x - a.defender_position()[0]).hypot(e.y() - a.defender_position()[1])
        };
        if !melee {
            // This preference limits action selection, never the dice. Even a
            // risk-taking policy cannot make an illegal skill-below-3 attack.
            let minimum_skill = if self.campaign.npc.allow_risk { 3 } else { 8 };
            let aimed_seconds = if a.aim_target == Some(id) {
                a.aim_seconds
            } else {
                0
            };
            let base_skill = a.profile.ranged_skill
                + a.profile.npc.skill_bonus
                + rules::range_penalty(current_range)
                - a.injury.shock;
            let effective_skill =
                base_skill + rules::aim_bonus(a.profile.npc.accuracy, aimed_seconds);
            if effective_skill < minimum_skill {
                let fully_aimed_skill = base_skill + rules::aim_bonus(a.profile.npc.accuracy, 3);
                if aimed_seconds < 3 && fully_aimed_skill >= minimum_skill {
                    a.maneuver = NpcManeuver::Aim;
                    a.aim_target = Some(id);
                    a.aim_seconds = aimed_seconds + 1;
                    a.aim_ticks += 60;
                    a.decision = format!(
                        "Aim at {} in lane {} · cautious policy: attack skill {} below {}",
                        enemy_name(a, e),
                        lane + 1,
                        effective_skill,
                        minimum_skill
                    );
                } else {
                    a.aim_target = None;
                    a.aim_seconds = 0;
                    a.decision = format!(
                        "Do Nothing · wait for a safer shot in lane {}: attack skill {} below {}",
                        lane + 1,
                        effective_skill,
                        minimum_skill
                    );
                    if a.enemies
                        .iter()
                        .any(|e| e.contact() && !e.injury.terminal())
                    {
                        a.maneuver = NpcManeuver::AllOutDefense;
                        a.defense_bonus = 2;
                        a.decision = format!(
                            "All-Out Defense · Dodge +2 until next turn; wait for a safer shot in lane {} (skill {})",
                            lane + 1,
                            effective_skill
                        );
                    }
                }
                a.injury.clear_shock();
                return;
            }
        }
        let useful_aim = a.profile.ranged_skill
            + a.profile.npc.skill_bonus
            + rules::range_penalty(current_range)
            < 14;
        let prefer_aim = self.campaign.npc.prefer_aim
            && (policy == Policy::Marksman || (!imminent && !contact))
            && (policy == Policy::Marksman || useful_aim);
        let desired_aim = if policy == Policy::Marksman && !imminent {
            2
        } else {
            1
        };
        if !melee && prefer_aim && (a.aim_target != Some(id) || a.aim_seconds < desired_aim) {
            a.maneuver = NpcManeuver::Aim;
            if a.aim_target != Some(id) {
                a.aim_seconds = 0;
            }
            a.aim_target = Some(id);
            a.aim_seconds += 1;
            a.aim_ticks += 60;
            a.decision = format!(
                "Aim at {} in lane {} · {}s",
                enemy_name(a, e),
                lane + 1,
                a.aim_seconds
            );
            a.injury.clear_shock();
            return;
        }
        let range = if contact {
            1.0
        } else {
            (e.x - a.defender_position()[0]).hypot(e.y() - a.defender_position()[1])
        };
        let skill = if melee {
            a.profile.melee_skill
        } else {
            a.profile.ranged_skill
                + a.profile.npc.skill_bonus
                + rules::range_penalty(range)
                + rules::aim_bonus(
                    a.profile.npc.accuracy,
                    if a.aim_target == Some(id) {
                        a.aim_seconds
                    } else {
                        0
                    },
                )
        };
        let skill = skill - a.injury.shock;
        let damage = if melee {
            let full = Damage {
                dice: a.profile.npc_melee_damage_dice,
                adds: a.profile.npc_melee_damage_add,
                kind: a.profile.npc_melee_kind,
            };
            if a.injury.fp * 3 < a.injury.max_fp {
                rules::rescale_swing(full, a.profile.stats.st, (a.profile.stats.st + 1) / 2)
                    .expect("validated human ST and melee profile")
            } else {
                full
            }
        } else {
            Damage {
                dice: a.profile.npc.damage_dice,
                adds: a.profile.npc.damage_add,
                kind: a.profile.npc.damage_kind,
            }
        };
        let attack = Attack {
            skill,
            damage,
            armor_divisor: 1,
        };
        let armor = enemy_armor(a, e);
        let dodge = e.injury.dodge(enemy_dodge(a, e));
        let ht = enemy_ht(a, e);
        let point = [e.x, e.y()];
        let target_name = enemy_name(a, e);
        a.maneuver = NpcManeuver::Attack;
        let result = rules::resolve_attack(
            &mut a.rng,
            attack,
            Some(dodge),
            armor,
            &mut a.enemies[target].injury,
            ht,
        );
        if !melee {
            a.ammo -= 1;
            a.shots += 1;
        }
        if result.hit {
            a.hits += 1;
            if !melee {
                self.campaign.record_npc_progress(1, 0);
            }
        }
        if result.requires_ready {
            a.ready_turns = 1;
        }
        a.enemies[target].apply_damage(result.injury, a.mode);
        a.active_damage += result.injury.max(0) as u64;
        a.dps.record(
            a.tick,
            crate::stats::DamageSource::Defender,
            result.injury.max(0) as u64,
        );
        a.telemetry.defender_damage += result.injury.max(0) as u64;
        a.resolved(
            None,
            Some(id),
            &result,
            format!(
                "{} {} in lane {}: {}",
                if melee { "Strike" } else { "Shoot" },
                target_name,
                lane + 1,
                result.explanation()
            ),
        );
        a.decision = format!(
            "{} {} in lane {} · {}",
            if melee {
                "Attack: strike"
            } else {
                "Attack: shoot"
            },
            target_name,
            lane + 1,
            if contact {
                "close engagement"
            } else {
                "nearest visible threat"
            }
        );
        let meta = EventMetadata {
            target_id: Some(id),
            lane: Some(lane),
            ..Default::default()
        };
        a.event_with(
            if melee {
                EventKind::Melee
            } else {
                EventKind::Shot
            },
            a.defender_position(),
            point,
            0,
            meta,
        );
        a.resolution_event(&result, a.defender_position(), point, meta, true);
        a.aim_target = None;
        a.aim_seconds = 0;
        a.injury.clear_shock();
        a.hp = a.injury.hp;
        a.stamina = a.injury.fp as f32;
    }
    fn enemy_attacks(&mut self, a: &mut Attempt) {
        for n in 0..a.enemies.len() {
            let e = &a.enemies[n];
            if (a.mode == Mode::Personal && e.hp <= 0)
                || (a.mode == Mode::Npc && e.injury.terminal())
            {
                continue;
            }
            if a.enemies[n].contact_since.is_some_and(|tick| a.tick > tick) {
                a.enemies[n].attack_left = a.enemies[n].attack_left.saturating_sub(1);
                if a.enemies[n].attack_phase == EnemyPhase::Recovery
                    && a.enemies[n].attack_left == 0
                {
                    let duration = seconds(enemy_profile(&a.enemies[n]).windup);
                    a.enemies[n].attack_phase = EnemyPhase::Windup;
                    a.enemies[n].attack_left = duration;
                    a.enemies[n].windup_total = duration;
                    let e = &a.enemies[n];
                    let point = [e.x, e.y()];
                    let meta = EventMetadata {
                        actor_id: Some(e.id),
                        target_id: e.target_structure,
                        lane: Some(e.lane),
                        duration_ticks: duration,
                        ..Default::default()
                    };
                    a.event_with(
                        EventKind::Windup,
                        point,
                        if meta.target_id.is_none() {
                            a.defender_position()
                        } else {
                            point
                        },
                        0,
                        meta,
                    );
                }
            }
            let mut turn_shock = 0;
            if a.mode == Mode::Npc && a.tick.is_multiple_of(60) {
                turn_shock = a.enemies[n].injury.shock;
                a.enemies[n].injury.clear_shock();
                let ht = enemy_ht(a, &a.enemies[n]);
                if !a.enemies[n].injury.start_turn(&mut a.rng, ht) {
                    if a.enemies[n].boss {
                        break;
                    }
                    continue;
                }
                if a.enemies[n].injury.stunned {
                    a.enemies[n].injury.recover_stun(&mut a.rng, ht);
                    continue;
                }
                if a.enemies[n].injury.prone {
                    a.enemies[n].injury.prone = false;
                    continue;
                }
                // Hostiles use natural strikes; knockdown cannot drop a claw.
                a.enemies[n].injury.weapon_dropped = false;
                if a.enemies[n].ready_turns > 0 {
                    a.enemies[n].ready_turns -= 1;
                    continue;
                }
            }
            let e = &a.enemies[n];
            let Some(contact_tick) = e.contact_since else {
                if a.mode == Mode::Npc && a.tick.is_multiple_of(60) {
                    a.enemies[n].injury.clear_shock();
                }
                continue;
            };
            if a.tick <= contact_tick {
                continue;
            }
            if a.enemies[n].attack_left > 0 {
                continue;
            }
            if a.mode == Mode::Npc && !a.tick.is_multiple_of(60) {
                continue;
            }
            let e = &a.enemies[n];
            let d = enemy_profile(e).clone();
            let origin = [e.x, e.y()];
            let lane = e.lane;
            let structure = e
                .target_structure
                .and_then(|id| a.structures.iter().position(|s| s.id == id));
            let actor_id = e.id;
            let target_id = e.target_structure;
            let meta = EventMetadata {
                actor_id: Some(actor_id),
                target_id,
                lane: Some(lane),
                ..Default::default()
            };
            let mut damage = if e.boss {
                content().levels[a.level].boss_damage
            } else {
                d.damage
            };
            damage = (damage as f32 * a.progression.enemy_damage * a.balance.enemy_damage).round()
                as i32;
            if structure.is_some() {
                damage *= if e.boss && content().levels[a.level].boss == BossKind::SiegeBrute {
                    2
                } else {
                    i32::from(d.structure_damage_multiplier)
                };
            }
            let siege = e.boss && content().levels[a.level].boss == BossKind::SiegeBrute;
            a.enemies[n].attack_phase = if siege {
                EnemyPhase::Recovery
            } else {
                EnemyPhase::Windup
            };
            a.enemies[n].attack_left = if siege {
                seconds(content().levels[a.level].boss_timing.siege_recovery)
            } else if a.mode == Mode::Personal {
                seconds(d.windup)
            } else {
                60
            };
            a.enemies[n].windup_total = a.enemies[n].attack_left;
            if !siege {
                a.event_with(
                    EventKind::Windup,
                    origin,
                    if target_id.is_none() {
                        a.defender_position()
                    } else {
                        origin
                    },
                    0,
                    EventMetadata {
                        duration_ticks: a.enemies[n].attack_left,
                        ..meta
                    },
                );
            }
            if structure.is_none() {
                a.telemetry.contact_attacks += 1;
            }
            if a.mode == Mode::Npc {
                let l = &content().levels[a.level];
                let boss = a.enemies[n].boss;
                let demolition = if structure.is_some() {
                    if boss && l.boss == BossKind::SiegeBrute {
                        2
                    } else {
                        d.structure_damage_multiplier
                    }
                } else {
                    1
                };
                let mut attack = Attack {
                    skill: if boss { l.boss_npc_skill } else { d.npc_skill } - turn_shock,
                    damage: Damage {
                        dice: (if boss {
                            l.boss_npc_damage_dice
                        } else {
                            d.npc_damage_dice
                        })
                        .saturating_mul(demolition),
                        adds: (if boss {
                            l.boss_npc_damage_add
                        } else {
                            d.npc_damage_add
                        }) * i32::from(demolition)
                            + ((a.progression.enemy_damage - 1.0) * 6.0).floor() as i32,
                        kind: DamageKind::Crushing,
                    },
                    armor_divisor: 1,
                };
                let expected = crate::campaign::mean_basic_damage(attack.damage);
                attack.damage.adds = (attack.damage.adds as f32
                    + expected * (a.balance.enemy_damage - 1.0))
                    .round()
                    .clamp(-30.0, 1_000_000.0) as i32;
                if let Some(s) = structure {
                    let protection = content().structure(a.structures[s].kind).npc.armor;
                    let result = rules::resolve_object_attack(&mut a.rng, attack, protection);
                    if result.requires_ready {
                        a.enemies[n].ready_turns = 1;
                    }
                    damage = result.injury * 5;
                    a.structures[s].hp -= damage;
                    let mut feedback = result.clone();
                    feedback.injury = damage;
                    a.resolution_event(
                        &feedback,
                        origin,
                        [a.structures[s].x(), a.structures[s].y()],
                        meta,
                        false,
                    );
                    a.resolved(
                        Some(actor_id),
                        target_id,
                        &result,
                        format!(
                            "{} → structure in lane {}: {}",
                            d.name,
                            lane + 1,
                            result.explanation()
                        ),
                    );
                } else {
                    let dodge = a
                        .injury
                        .dodge((a.profile.stats.dx + a.profile.stats.ht) / 4 + 3)
                        + a.defense_bonus;
                    let result = rules::resolve_attack(
                        &mut a.rng,
                        attack,
                        Some(dodge),
                        a.profile.npc.armor,
                        &mut a.injury,
                        a.profile.stats.ht,
                    );
                    if result.requires_ready {
                        a.enemies[n].ready_turns = 1;
                    }
                    if result.defense_roll.is_some() || result.injury > 0 {
                        a.aim_target = None;
                        a.aim_seconds = 0;
                    }
                    damage = result.injury;
                    a.hp = a.injury.hp;
                    a.resolution_event(&result, origin, a.defender_position(), meta, false);
                    a.resolved(
                        Some(actor_id),
                        None,
                        &result,
                        format!(
                            "{} → survivor from lane {}: {}",
                            d.name,
                            lane + 1,
                            result.explanation()
                        ),
                    );
                }
                a.enemies[n].injury.clear_shock();
            } else if let Some(s) = structure {
                a.structures[s].hp -= damage;
            } else {
                damage = (damage - a.profile.personal.armor).max(0);
                if a.statuses.active(StatusId::Brace) {
                    damage /= 2;
                }
                a.hp -= damage;
            }
            let point = structure.map_or(a.defender_position(), |s| {
                [a.structures[s].x(), a.structures[s].y()]
            });
            if a.mode == Mode::Personal {
                a.event_with(
                    if damage > 0 {
                        EventKind::Damage
                    } else {
                        EventKind::Armor
                    },
                    origin,
                    point,
                    damage,
                    meta,
                );
            }
            if structure.is_none() && damage > 0 {
                a.last_damage_lane = Some(lane);
                a.telemetry.damage_by_lane[lane as usize] += damage as u64;
            }
            if structure.is_some() {
                a.telemetry.structure_damage_taken += damage.max(0) as u64;
            }
            if a.mode == Mode::Npc && a.injury.terminal() {
                break;
            }
        }
        let destroyed: Vec<_> = a
            .structures
            .iter()
            .filter(|s| s.hp <= 0)
            .map(|s| [s.x(), s.y()])
            .collect();
        for point in destroyed {
            a.telemetry.structure_losses += 1;
            a.event(EventKind::StructureLost, point, point, 0);
        }
        a.structures.retain(|s| s.hp > 0);
        for e in &mut a.enemies {
            if e.target_structure
                .is_some_and(|id| !a.structures.iter().any(|s| s.id == id))
            {
                e.target_structure = None;
                e.contact_since = None;
                e.attack_left = 0;
                e.attack_phase = EnemyPhase::Advance;
                e.windup_total = 0;
            }
        }
    }
    fn reap(&mut self, a: &mut Attempt) {
        let defeated: Vec<usize> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if a.mode == Mode::Personal {
                    e.hp <= 0
                } else {
                    e.injury.terminal()
                }
            })
            .map(|(n, _)| n)
            .collect();
        for n in defeated.into_iter().rev() {
            let e = a.enemies.remove(n);
            if !a.reward_ledger.insert(e.id) {
                continue;
            }
            a.corpses.push(CorpseRecord {
                id: e.id,
                kind: e.kind,
                boss_kind: e.boss_kind,
                at: [e.x, e.y()],
                visual: e.resolved.as_ref().map(|profile| profile.visual.clone()),
                tick: a.tick,
            });
            let d = enemy_profile(&e);
            let level = &content().levels[a.level];
            let base_coins = if e.boss {
                level.boss_reward + level.completion_bonus
            } else {
                d.reward
            };
            let coins = (base_coins as f32 * a.modifiers.reward).round().max(1.0) as u64;
            let xp = if e.boss {
                40 + a.level as u64 * 15
            } else {
                d.xp
            };
            a.kill_xp = a
                .kill_xp
                .saturating_add(self.campaign.reward(coins, xp, a.mode));
            a.telemetry.kills_by_kind[e.kind] += 1;
            if e.boss {
                a.telemetry.boss_defeat_tick = Some(a.tick);
            }
            a.coins += coins;
            a.kills += 1;
            a.event_with(
                EventKind::Kill,
                [e.x, e.y()],
                [e.x, e.y()],
                coins as i32,
                EventMetadata {
                    actor_id: Some(e.id),
                    lane: Some(e.lane),
                    ..Default::default()
                },
            );
            let drop = e.boss || a.rng.below(100) < 3 || a.kills.is_multiple_of(20);
            if drop {
                let available: Vec<usize> = content()
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(n, i)| *n >= 4 && i.tier > 0 && i.tier <= a.level + 1)
                    .map(|(n, _)| n)
                    .collect();
                if !available.is_empty() {
                    let definition = available[a.rng.below(available.len() as u64) as usize];
                    let quality =
                        crate::quality::catalog().roll(self.seed, a.id, e.id, a.progression.rank);
                    if let Ok(item_id) = self.campaign.add_item_with_quality(
                        definition,
                        a.progression.item_level,
                        Some(quality),
                    ) {
                        a.loot += 1;
                        a.loot_quality_counts[quality] += 1;
                        if a.loot_events.len() == crate::quality::MAX_LOOT_EVENTS {
                            a.loot_events.remove(0);
                        }
                        a.loot_events.push(crate::quality::LootEvent {
                            at: [e.x, e.y()],
                            item_id,
                            definition: ItemDefId::at(definition),
                            level: a.progression.item_level,
                            quality,
                            name: content().items[definition].name.clone(),
                            tick: a.tick,
                        });
                        a.note(format!(
                            "Recovered {} {} · level {} · banked in the shared armory",
                            quality.name(),
                            content().items[definition].name,
                            a.progression.item_level
                        ));
                    }
                }
            }
            if a.mode == Mode::Npc {
                a.note(format!(
                    "{} in lane {} {} · +{} coins",
                    enemy_name(a, &e),
                    e.lane + 1,
                    if e.injury.dead {
                        "dead"
                    } else if e.injury.unconscious {
                        "unconscious; removed from the encounter"
                    } else {
                        "incapacitated"
                    },
                    coins
                ));
            }
        }
    }
    fn finish(&mut self, mut a: Attempt, outcome: Outcome, cause: String) {
        info_span!("battle", attempt = a.id, mode = ?a.mode, seed = self.seed).in_scope(|| {
            info!(
                outcome = ?outcome,
                ticks = a.tick,
                kills = a.kills,
                %cause,
                "attempt finished"
            );
        });
        self.finish_events(&mut a, outcome, &cause);
        if !self.worker {
            self.repeat_remaining = 0;
        }
        let completion_xp = if outcome == Outcome::Victory {
            self.campaign.completion_xp_preview()
        } else {
            0
        };
        let first_clear = outcome == Outcome::Victory && self.campaign.first_clear(a.level, a.mode);
        let country_freed = if outcome == Outcome::Victory && a.country_id.is_some() {
            self.campaign
                .record_country_stage_clear(a.stage)
                .unwrap_or(false)
        } else {
            false
        };
        if outcome == Outcome::Victory
            && let Some(country) = &a.country_id
        {
            self.strategy.resolve_outbreak(country);
        }
        self.campaign.finish_attempt();
        let record = crate::stats::RunRecord {
            id: 0,
            attempt_id: a.id,
            worker_id: None,
            recruit_id: None,
            outcome,
            mode: a.mode,
            country_id: a.country_id.clone(),
            country_name: a.country_name.clone(),
            defender_name: if a.mode == Mode::Npc {
                self.campaign.npc.name.clone()
            } else {
                self.campaign
                    .selected_survivor
                    .unwrap_or(crate::survivors::SurvivorId::Rook)
                    .name()
                    .into()
            },
            cause: cause.clone(),
            stage: a.stage,
            template: a.level,
            survivor: if a.mode == Mode::Personal {
                Some(
                    self.campaign
                        .selected_survivor
                        .unwrap_or(crate::survivors::SurvivorId::Rook),
                )
            } else {
                None
            },
            lane_count: a.lane_count,
            width_yards: a.width_yards,
            threat_rank: a.progression.rank,
            balance: a.balance.clone(),
            gear: a.gear_snapshot.clone(),
            duration_ticks: a.tick,
            kills: a.kills,
            shots: a.shots,
            hits: a.hits,
            misses: a.telemetry.misses,
            loot: a.loot,
            damage_by_source: a.dps.totals,
            average_dps: a.average_dps(),
            peak_dps: a.peak_dps(),
            coins: a.coins,
            kill_xp: a.kill_xp,
            completion_xp,
            loot_by_quality: a.loot_quality_counts.clone(),
            end_hp: a.hp,
            max_hp: a.max_hp,
            legacy: a.gear_snapshot.is_empty(),
        };
        if self.worker {
            self.pending_runs.push(record);
        } else {
            self.campaign.append_run(record);
        }
        self.last_result = Some(Report {
            attempt_id: a.id,
            width_yards: a.width_yards,
            lane_count: a.lane_count,
            completion_xp,
            loot_events: a.loot_events.clone(),
            outcome,
            level: a.level,
            mode: a.mode,
            elapsed: a.elapsed(),
            kills: a.kills,
            country_id: a.country_id.clone(),
            country_name: a.country_name.clone(),
            stage: a.stage,
            country_freed,
            coins: a.coins,
            loot: a.loot,
            cause,
            tip: if outcome == Outcome::Victory {
                "Spend retained rewards, review your loadout, then secure the next sector.".into()
            } else if a.mode == Mode::Npc {
                "Train ranged skill or buy a reliable rifle; review attack rolls and protection in the combat log.".into()
            } else if a.reload_ticks > a.tick / 4 {
                "Coordination and a larger magazine reduce dangerous reload windows.".into()
            } else {
                "Aim at the nearest runner first. Melee answers enemies at any contact lane. Vitality and armor buy recovery time.".into()
            },
            first_clear,
            shots: a.shots,
            hits: a.hits,
            first_contact: a.first_contact,
            active_damage: a.active_damage,
            structure_damage: a.structure_damage,
            log: a.log,
            telemetry: a.telemetry,
        });
        if outcome != Outcome::Victory || first_clear || country_freed {
            self.repeat_remaining = 0;
        }
    }
    pub fn start_repeat(&mut self, count: u32) -> Result<(), String> {
        if count == 0 || count > 10 {
            return Err("Repeat count must be between one and ten".into());
        }
        let report = self
            .last_result
            .clone()
            .ok_or("Complete a survivor deployment first")?;
        if report.mode != Mode::Npc || report.outcome != Outcome::Victory {
            return Err("Repeats require a cleared survivor deployment".into());
        }
        self.deploy_report(&report)?;
        self.repeat_remaining = count;
        self.set_paused(false);
        Ok(())
    }
    pub fn continue_repeat(&mut self) -> Result<bool, String> {
        if self.attempt.is_some() || self.repeat_remaining == 0 {
            return Ok(false);
        }
        let r = self
            .last_result
            .clone()
            .ok_or("Missing deployment result")?;
        if r.outcome != Outcome::Victory || r.first_clear {
            self.repeat_remaining = 0;
            return Ok(false);
        }
        self.repeat_remaining -= 1;
        if self.repeat_remaining == 0 {
            return Ok(false);
        }
        if let Err(error) = self.deploy_report(&r) {
            self.repeat_remaining = 0;
            return Err(error);
        }
        self.set_paused(false);
        Ok(true)
    }
}
fn seconds(value: f32) -> u32 {
    (value * 60.0).ceil() as u32
}
pub fn construction_lane_order(lanes: u8) -> Vec<u8> {
    let mut result: Vec<_> = (0..lanes).collect();
    result.sort_by_key(|lane| (lane.abs_diff(lanes / 2), *lane));
    result
}
fn choose_lane(a: &mut Attempt) -> u8 {
    let mut candidates: Vec<u8> = (0..a.lane_count)
        .filter(|lane| a.lane_history.iter().filter(|&&old| old == *lane).count() < 2)
        .collect();
    if candidates.is_empty() {
        candidates.extend(0..a.lane_count);
    }
    let lane = candidates[a.rng.below(candidates.len() as u64) as usize];
    a.lane_history.push(lane);
    if a.lane_history.len() > 5 {
        a.lane_history.remove(0);
    }
    lane
}
fn enemy_profile(e: &Enemy) -> &crate::enemy_catalog::ResolvedEnemy {
    e.resolved
        .as_ref()
        .unwrap_or_else(|| crate::enemy_catalog::enemy_catalog().default_profile(e.kind))
}
fn enemy_step_speed(a: &Attempt, e: &Enemy) -> f32 {
    if e.statuses.active(StatusId::HitStop)
        || (a.mode == Mode::Npc && (e.injury.prone || e.injury.stunned))
    {
        return 0.0;
    }
    let mut multiplier = a.modifiers.enemy_speed;
    if a.mode == Mode::Npc {
        if e.injury.hp * 3 < e.injury.max_hp {
            multiplier *= 0.5;
        }
        if e.injury.fp * 3 < e.injury.max_fp {
            multiplier *= 0.5;
        }
    }
    (if e.boss { 0.6 } else { enemy_profile(e).speed }) * multiplier
}
fn enemy_armor(a: &Attempt, e: &Enemy) -> i32 {
    if e.boss {
        if e.exposed_at(a.tick) { 0 } else { e.armor }
    } else if e.contact() {
        (e.armor as f32 * enemy_profile(e).contact_armor_fraction).floor() as i32
    } else {
        e.armor
    }
}
fn enemy_defeated(a: &Attempt, e: &Enemy) -> bool {
    e.condition(a.mode) == EnemyCondition::Defeated
}
fn enemy_name(a: &Attempt, e: &Enemy) -> String {
    if e.boss {
        content().levels[a.level].boss_name.clone()
    } else {
        enemy_profile(e).name.clone()
    }
}
fn select_npc_target(a: &Attempt, policy: Policy) -> Option<usize> {
    if !a
        .enemies
        .iter()
        .any(|e| e.contact() && !e.injury.terminal())
        && let Some(n) = a
            .enemies
            .iter()
            .position(|e| Some(e.id) == a.aim_target && !e.injury.terminal() && e.in_view())
    {
        return Some(n);
    }
    let mut targets: Vec<usize> = a
        .enemies
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.injury.terminal() && e.in_view())
        .map(|(n, _)| n)
        .collect();
    targets.sort_by(|&n, &m| {
        let score = |e: &Enemy| {
            let mut urgency = e.x;
            if e.contact() {
                urgency -= 50.0;
            }
            if e.injury.stunned || e.injury.prone {
                urgency += 15.0;
            }
            if e.injury.hp <= 0 {
                urgency += 8.0;
            }
            // Skip a zero bias so the score keeps its exact float value.
            let bias = enemy_profile(e).target_urgency;
            if bias != 0.0 {
                urgency += bias;
            }
            if e.boss {
                urgency -= if policy == Policy::BossFocus && a.hp * 2 > a.max_hp {
                    65.0
                } else if policy == Policy::BossFocus {
                    30.0
                } else {
                    12.0
                };
            }
            if policy == Policy::Guardian && e.target_structure.is_some() {
                urgency -= 10.0;
            }
            urgency
        };
        score(&a.enemies[n])
            .total_cmp(&score(&a.enemies[m]))
            .then(a.enemies[n].id.cmp(&a.enemies[m].id))
    });
    targets.first().copied()
}

fn structure_bonus(a: &Attempt, rank: u32) -> f32 {
    (1.0 + rank as f32 * 0.2)
        * a.progression.fortification_multiplier
        * if a.profile.engineering && !a.profile.modifiers.engineering {
            1.25
        } else {
            a.profile.modifiers.engineering_multiplier
        }
}

fn enemy_ht(a: &Attempt, e: &Enemy) -> i32 {
    if e.boss {
        content().levels[a.level].boss_npc_ht
    } else {
        enemy_profile(e).npc_ht
    }
}
fn enemy_dodge(a: &Attempt, e: &Enemy) -> i32 {
    if e.boss {
        content().levels[a.level].boss_npc_dodge
    } else {
        enemy_profile(e).npc_dodge
    }
}

/// Seeded, renderer-free full encounter exercise used by the desktop --smoke
/// command. The manual bot supplies ordinary geometric aiming inputs only.
pub fn smoke() -> Result<String, String> {
    smoke_seed(0x1A571167)
}
pub fn smoke_seed(seed: u64) -> Result<String, String> {
    let mut lines = Vec::new();
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = Game {
            seed,
            ..Game::default()
        };
        g.campaign.attempts = 1;
        if mode == Mode::Npc {
            g.campaign.legacy_features = true;
            g.campaign.npc.policy = Policy::BossFocus;
        }
        for level in 0..5 {
            let mut cleared = false;
            for retry in 0..6 {
                smoke_prepare(&mut g, mode)?;
                g.deploy(level, mode)?;
                if mode == Mode::Npc && g.campaign.blueprints[StructureKind::Trap] {
                    for lane in
                        construction_lane_order(g.attempt.as_ref().expect("deployed").lane_count)
                    {
                        if g.attempt.as_ref().is_some_and(|a| {
                            a.supply >= content().structure(StructureKind::Trap).supply
                        }) {
                            g.place(StructureKind::Trap, lane, 0)?;
                        }
                    }
                }
                g.set_paused(false);
                for _ in 0..60 * 600 {
                    if g.attempt.is_none() {
                        break;
                    }
                    let input = smoke_input(&g);
                    g.tick(input);
                }
                let r = g
                    .last_result
                    .as_ref()
                    .ok_or("Smoke encounter exceeded ten simulated minutes")?;
                lines.push(format!(
                    "{mode:?} stage{} attempt{} {:?} {:.1}s · {} kills · {} coins retained",
                    level + 1,
                    retry + 1,
                    r.outcome,
                    r.elapsed,
                    r.kills,
                    r.coins
                ));
                if r.outcome == Outcome::Victory {
                    cleared = true;
                    break;
                }
            }
            if !cleared {
                return Err(format!(
                    "No campaign clear within six earned-reward retries:\n{}",
                    lines.join("\n")
                ));
            }
        }
    }
    Ok(lines.join("\n"))
}
/// Legal earned-reward preparation used by deterministic campaign proofs.
pub fn smoke_prepare(g: &mut Game, mode: Mode) -> Result<(), String> {
    if mode == Mode::Npc
        && g.campaign.blueprint_available(StructureKind::Trap)
        && !g.campaign.blueprints[StructureKind::Trap]
        && g.campaign.coins >= content().structure(StructureKind::Trap).blueprint_price
    {
        g.campaign.purchase_blueprint(StructureKind::Trap)?;
    }
    if mode == Mode::Personal {
        for id in [
            "quick_reload",
            "rifle_training",
            "steady_hands",
            "rapid_cycle",
            "stopping_power",
            "piercing",
            "field_fitness",
            "second_wind",
            "brace",
        ] {
            let node = crate::skill_catalog::skills()
                .node(id)
                .expect("shipped smoke purchase route");
            if !g.campaign.has_node(id)
                && node.price <= g.campaign.coins
                && node.prerequisites.iter().all(|p| g.campaign.has_node(p))
            {
                g.campaign.purchase_node(id)?;
            }
        }
    }
    let mut market: Vec<usize> = content()
        .items
        .iter()
        .enumerate()
        .filter(|(definition, item)| {
            item.tier > 0
                && item.tier <= g.campaign.unlocked
                && g.campaign
                    .buy_price(*definition)
                    .is_ok_and(|price| price <= g.campaign.coins)
                && (mode == Mode::Npc || g.campaign.can_use_weapon(mode, *definition))
        })
        .map(|(n, _)| n)
        .collect();
    market.sort_by_key(|n| {
        (
            match content().items[*n].slot {
                crate::content::Slot::Ranged => 0,
                crate::content::Slot::Outfit => 1,
                crate::content::Slot::Accessory => 2,
                crate::content::Slot::Melee => 3,
            },
            std::cmp::Reverse(content().items[*n].tier),
            std::cmp::Reverse(
                if mode == Mode::Npc && content().items[*n].slot == crate::content::Slot::Ranged {
                    // Compare guaranteed catalogue profiles within a
                    // tier, including sustained magazine capacity.
                    let profile = &content().items[*n].npc;
                    i64::from(profile.damage_dice) * 28
                        + i64::from(profile.damage_add) * 8
                        + i64::from(profile.accuracy) * 3
                        + i64::from(profile.magazine)
                } else {
                    0
                },
            ),
        )
    });
    let mut bought = [false; 4];
    for n in market {
        let def = &content().items[n];
        let slot = def.slot.index();
        if !bought[slot]
            && (g.campaign.equipped(mode, def.slot).tier < def.tier
                || (g.campaign.equipped(mode, def.slot).tier == def.tier
                    && g.campaign
                        .item(if mode == Mode::Personal {
                            g.campaign.player.loadout[slot]
                        } else {
                            g.campaign.npc.loadout[slot]
                        })
                        .is_some_and(|item| item.level < g.campaign.shop_item_level())))
            && g.campaign
                .buy_price(n)
                .is_ok_and(|price| price <= g.campaign.coins)
        {
            let id = g.campaign.buy_item(n)?;
            g.campaign.equip(mode, id)?;
            bought[slot] = true;
        }
    }
    if mode == Mode::Npc {
        while g.campaign.npc.unspent < 4
            && g.campaign.coins >= 25
            && g.campaign.npc.awarded < g.campaign.allowance()
        {
            g.campaign.buy_training_point()?;
        }
        if g.campaign.npc.unspent >= 4 {
            g.campaign.train_npc(crate::campaign::NpcTraining::Guns)?;
        }
        if g.campaign.npc.guns >= 17 {
            // A trained defender facing a dense horde trades
            // optional Aim turns for more legal attack attempts.
            g.campaign.npc.prefer_aim = false;
            g.campaign.npc.allow_risk = true;
        }
    } else {
        while g.campaign.player.points > 0
            && g.campaign.player.body[crate::campaign::BodyAttribute::Vitality]
                < g.campaign.body_cap()
        {
            g.campaign
                .allocate_body(crate::campaign::BodyAttribute::Vitality)?;
        }
    }
    Ok(())
}
pub fn smoke_input(g: &Game) -> Input {
    let Some(a) = &g.attempt else {
        return Input::default();
    };
    let target = a
        .enemies
        .iter()
        .filter(|e| e.in_view())
        .min_by(|e, f| e.x.total_cmp(&f.x).then(e.id.cmp(&f.id)));
    let Some(e) = target else {
        return Input::default();
    };
    Input {
        aim: [e.x, e.y()],
        fire: true,
        melee: e.contact() && a.stamina >= 25.0,
        heal: a.hp < a.max_hp / 2,
        brace: a.hp < a.max_hp / 2,
        ..Input::default()
    }
}

/// Deterministic, isolated review scenario. It uses legal purchases, real
/// footprints and the authored population ceiling; it never touches disk.
pub fn peak_render_fixture() -> Result<Game, String> {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.choose_start_country("USA")?;
    for level in 0..4 {
        g.campaign.first_clear(level, Mode::Personal);
        g.campaign.finish_attempt();
        g.campaign.record_country_stage_clear(level)?;
    }
    g.campaign.reward(50_000, 0, Mode::Personal);
    g.campaign
        .purchase_skill(crate::campaign::Skill::Engineering)?;
    for kind in StructureKind::ALL {
        g.campaign.purchase_blueprint(kind)?;
    }
    for slot in crate::content::Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.slot == slot && item.tier <= g.campaign.unlocked)
            .max_by_key(|(_, item)| item.tier)
            .map(|(n, _)| n)
            .ok_or("Missing fixture equipment")?;
        let id = g.campaign.buy_item(definition)?;
        g.campaign.equip(Mode::Npc, id)?;
    }
    g.deploy_country_stage(4, Mode::Npc)?;
    let center = g.attempt.as_ref().expect("deployed").lane_count / 2;
    let secondary = (center + 4).min(g.attempt.as_ref().unwrap().lane_count - 1);
    g.place(StructureKind::Turret, center, 1)?;
    g.place(StructureKind::Turret, secondary, 2)?;
    g.place(StructureKind::Trap, 2, 3)?;
    for lane in 0..g.attempt.as_ref().expect("deployed").lane_count {
        if g.attempt.as_ref().unwrap().supply >= content().structure(StructureKind::Wall).supply {
            g.place(StructureKind::Wall, lane, 0)?;
        }
    }
    let mut a = g.attempt.take().expect("fixture deployment");
    a.tick = u64::from(seconds(content().levels[a.level].boss_time));
    let cap = a.enemy_cap();
    for n in 0..cap {
        let lane = (n % usize::from(a.lane_count)) as u8;
        g.spawn_enemy(&mut a, EnemyKind::ALL[n % 5], lane, false);
        let front = a
            .structures
            .iter()
            .filter(|structure| structure.lane == lane && structure.kind != StructureKind::Trap)
            .map(|structure| structure.x() + structure.blocking_radius() + 0.1)
            .fold(2.5_f32, f32::max);
        let rows = cap.div_ceil(usize::from(a.lane_count)).max(2);
        let gap = ((a.width_yards - 0.4 - front) / (rows - 1) as f32).max(0.65);
        a.enemies.last_mut().expect("spawn below cap").x =
            front + (n / usize::from(a.lane_count)) as f32 * gap;
    }
    let center = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
    a.enemies.last_mut().expect("fixture boss").x = a.width_yards - 0.6;
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(a.tick);
    a.events.clear();
    g.attempt = Some(a);
    g.set_paused(true);
    validate_game(&g, 0)?;
    Ok(g)
}

/// Synthetic maximum-rank crowd for rendering/performance review. Progress and
/// funds are staged deliberately; this is never an earned campaign proof.
pub fn maximum_progression_render_fixture() -> Result<Game, String> {
    let mut g = peak_render_fixture()?;
    g.attempt = None;
    g.campaign.reserved = None;
    let mut remaining = crate::progression::catalog().max_rank as usize;
    for country in crate::world::world()
        .countries
        .iter()
        .filter(|c| c.id != "USA")
    {
        if remaining == 0 {
            break;
        }
        let count = remaining.min(country.level_count());
        remaining -= count;
        g.campaign
            .world
            .country_levels
            .insert(country.id.clone(), count);
        g.campaign
            .world
            .clear_high_water
            .insert(country.id.clone(), count);
        if !g.campaign.world.unlocked.contains(&country.id) {
            g.campaign.world.unlocked.push(country.id.clone());
        }
        if count == country.level_count() && !g.campaign.world.cleared.contains(&country.id) {
            g.campaign.world.cleared.push(country.id.clone());
        }
    }
    g.campaign.player.level = g.campaign.player_level_cap();
    g.campaign.player.xp = 0;
    g.campaign.player.body = KindMap::default();
    g.campaign.player.points = g.campaign.player.level - 1;
    g.campaign.first_clear(4, Mode::Npc);
    g.campaign.victories = u64::from(crate::progression::catalog().max_rank);
    for slot in crate::content::Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.slot == slot && i.tier > 0 && i.tier <= g.campaign.unlocked)
            .max_by_key(|(_, i)| i.tier)
            .map(|(n, _)| n)
            .ok_or("Missing maximum fixture equipment")?;
        let id = g.campaign.buy_item(definition)?;
        g.campaign.equip(Mode::Npc, id)?;
    }
    g.deploy_country_stage(4, Mode::Npc)?;
    let center = g.attempt.as_ref().expect("deployed").lane_count / 2;
    let secondary = (center + 4).min(g.attempt.as_ref().unwrap().lane_count - 1);
    g.place(StructureKind::Turret, center, 1)?;
    g.place(StructureKind::Turret, secondary, 2)?;
    g.place(StructureKind::Trap, 2, 3)?;
    for lane in 0..g.attempt.as_ref().expect("deployed").lane_count {
        if g.attempt
            .as_ref()
            .is_some_and(|a| a.supply >= content().structure(StructureKind::Wall).supply)
        {
            g.place(StructureKind::Wall, lane, 0)?;
        }
    }
    let mut a = g.attempt.take().expect("fixture deployed");
    a.tick = u64::from(seconds(a.boss_time()));
    for n in 0..a.enemy_cap() {
        let lane = (n % usize::from(a.lane_count)) as u8;
        g.spawn_enemy(&mut a, EnemyKind::ALL[n % 5], lane, false);
        a.enemies.last_mut().expect("within cap").x = 5.0
            + (n / usize::from(a.lane_count)) as f32
                * (22.0 / (a.enemy_cap().div_ceil(usize::from(a.lane_count)).max(2) - 1) as f32);
    }
    let center = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
    a.enemies.last_mut().expect("boss").x = 28.8;
    a.enemies.last_mut().expect("fixture boss").x = a.width_yards - 0.6;
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(a.tick);
    a.events.clear();
    g.attempt = Some(a);
    g.set_paused(true);
    validate_game(&g, 0)?;
    Ok(g)
}

#[cfg(test)]
mod brace_tests {
    use super::*;

    #[test]
    fn brace_active_at_reads_the_status_clock_then_retained_ability_events() {
        let mut game = Game::default();
        game.deploy(0, Mode::Personal).unwrap();
        game.attempt.as_mut().unwrap().profile.brace = true;
        game.tick(Input {
            brace: true,
            ..Default::default()
        });
        let born = game.attempt.as_ref().unwrap().tick;
        for _ in 0..3 {
            game.tick(Input::default());
        }
        let a = game.attempt.as_mut().unwrap();
        assert!(a.brace_remaining_ticks() > 0);
        assert!(a.brace_active_at(a.tick));
        assert!(a.brace_active_at(born));
        assert!(!a.brace_active_at(born.saturating_sub(1)));
        assert!(!a.brace_active_at(a.tick + 1));
        // Without the status clock only past ticks covered by the event qualify.
        a.statuses.remove(StatusId::Brace);
        assert!(!a.brace_active_at(a.tick));
        let event_tick = a
            .events
            .iter()
            .find(|e| e.kind == EventKind::Ability && e.value == 0)
            .expect("brace event retained")
            .tick;
        assert!(a.brace_active_at(event_tick));
        assert!(!a.brace_active_at(event_tick.saturating_sub(1)));
    }
}
