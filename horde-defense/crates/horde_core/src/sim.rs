mod combat;
mod director;
mod events;
#[cfg(feature = "test-support")]
mod fixtures;
mod journey;
mod lifecycle;
mod manual;
mod migrations;
mod movement;
mod npc;
mod parallel;
mod persistence;
mod smoke;
mod structures;
mod telemetry;
pub use events::{Actor, BattleId, SimEvent, SimEventRecord, Strike};
#[cfg(feature = "test-support")]
pub use fixtures::{maximum_progression_render_fixture, peak_render_fixture};
pub use journey::JourneyContinuation;
pub use migrations::SAVE_VERSION;
use npc::structure_bonus;
pub use parallel::SettlementIssue;
pub(crate) use parallel::UNKNOWN_EXPEDITION;
pub use persistence::{RULESET_VERSION, SaveError};
pub use smoke::{smoke, smoke_input, smoke_prepare, smoke_seed};
pub use telemetry::CombatTelemetry;

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
