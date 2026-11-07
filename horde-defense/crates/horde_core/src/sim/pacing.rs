//! Headless pacing measurement: a scripted player walks the fresh-profile
//! loop (survivor → starting country → defenses → camp → continue) and the
//! report records when each milestone first happens. Un-gated so the
//! `pacing` example and the regression tests share one implementation.
//!
//! Harness v2 (W2-01): named profiles in a registry ([`by_id`]), per-seed
//! variation ([`pacing_seed`]), a threaded sweep ([`pacing_sweep`]) and
//! milestone statistics over seeds ([`MilestoneStats`]). Other remits add
//! registry rows and evaluators on this API.
use super::smoke::smoke_buy_gear;
use super::*;
use crate::balance::{BalancePreset, BalanceSettings};
use crate::command::Command;
use crate::enemy_catalog::enemy_catalog;
use crate::skill_catalog::{CampaignFeature, skills};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How the scripted player aims during a defense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingAim {
    /// The smoke bot: re-aims at the nearest enemy every tick, melees,
    /// heals and braces.
    Expert,
    /// A slower player: re-aims every half second, only shoots and melees.
    Novice,
}
/// How the scripted player spends coins in camp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingSpend {
    /// Learn the tree's recommended node while affordable, then buy gear.
    Recommended,
    /// Buy gear first, then learn recommended nodes with what is left.
    GearFirst,
}
/// What the scripted player does with spare items. Further variants
/// (manual and automatic selling) arrive with the profiles that use them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingSell {
    /// Never sells.
    Never,
}
/// Who defends the main run. Further variants (Mara or a recruit taking
/// over) arrive with the profiles that use them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingDefender {
    /// The chosen survivor defends every run.
    Survivor,
}
/// Whether the scripted player replays stages to farm. Further variants
/// arrive with the profiles that use them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingFarm {
    /// Always continues the journey.
    None,
}
/// The camp plan a profile follows besides aim and spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacingPlan {
    /// Equip dropped items that beat the current loadout. Not implemented
    /// yet: [`simulate_pacing`] rejects a plan that sets it.
    pub equip_drops: bool,
    pub sell: PacingSell,
    pub defender: PacingDefender,
    pub farm: PacingFarm,
    /// Train Mara and recruits in camp. Not implemented yet:
    /// [`simulate_pacing`] rejects a plan that sets it.
    pub train_companions: bool,
}
impl PacingPlan {
    /// The survivor alone, never selling, farming or training: the plan of
    /// every pre-v2 profile.
    pub const SOLO: Self = Self {
        equip_drops: false,
        sell: PacingSell::Never,
        defender: PacingDefender::Survivor,
        farm: PacingFarm::None,
        train_companions: false,
    };
    fn unsupported(&self) -> Option<&'static str> {
        if self.equip_drops {
            Some("equip_drops")
        } else if self.train_companions {
            Some("train_companions")
        } else {
            None
        }
    }
}
/// One scripted player. `id` is the stable registry key; `name` is the
/// table heading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PacingProfile {
    pub id: &'static str,
    pub name: &'static str,
    /// The game seed of seed index 0; later indices derive theirs with
    /// [`pacing_seed`].
    pub seed: u64,
    pub country: &'static str,
    pub aim: PacingAim,
    pub spend: PacingSpend,
    pub max_runs: usize,
    /// The balance preset, set before the start country is chosen. The
    /// campaign default (Normal) issues no command.
    pub difficulty: BalancePreset,
    pub plan: PacingPlan,
    /// End the profile after the run in which this milestone is first
    /// reached (at most `max_runs` runs either way).
    pub stop_after: Option<Milestone>,
}
/// The registry key of the acceptance profile (goals 5 and 6).
pub const DEFAULT_PROFILE_ID: &str = "expert-deu";
/// The profiles the default table prints, in column order. Their output at
/// one seed is the pre-v2 table.
pub const LEGACY_PROFILE_IDS: [&str; 3] = ["expert-deu", "novice-deu", "novice-gear-lux"];
/// Every named profile. Ids are stable and unique; rows are appended.
const REGISTRY: [PacingProfile; 3] = [
    PacingProfile {
        id: "expert-deu",
        name: "expert, tree first, Germany",
        seed: 0x1A571167,
        country: "DEU",
        aim: PacingAim::Expert,
        spend: PacingSpend::Recommended,
        max_runs: 60,
        difficulty: BalancePreset::Normal,
        plan: PacingPlan::SOLO,
        stop_after: None,
    },
    PacingProfile {
        id: "novice-deu",
        name: "novice, tree first, Germany",
        seed: 0x1A571167,
        country: "DEU",
        aim: PacingAim::Novice,
        spend: PacingSpend::Recommended,
        max_runs: 60,
        difficulty: BalancePreset::Normal,
        plan: PacingPlan::SOLO,
        stop_after: None,
    },
    PacingProfile {
        id: "novice-gear-lux",
        name: "novice, gear first, Luxembourg",
        seed: 2026,
        country: "LUX",
        aim: PacingAim::Novice,
        spend: PacingSpend::GearFirst,
        max_runs: 60,
        difficulty: BalancePreset::Normal,
        plan: PacingPlan::SOLO,
        stop_after: None,
    },
];
/// Every registered profile, in registry order.
pub fn pacing_registry() -> &'static [PacingProfile] {
    &REGISTRY
}
/// The registered profile `id`, if any.
pub fn by_id(id: &str) -> Option<PacingProfile> {
    REGISTRY.iter().find(|p| p.id == id).copied()
}
/// The game seed of seed index `index`: `base` itself for index 0 (so one
/// seed reproduces the pre-v2 table), otherwise a SplitMix of `base ^ index`.
pub fn pacing_seed(base: u64, index: usize) -> u64 {
    if index == 0 {
        base
    } else {
        crate::hash::mix(base ^ index as u64)
    }
}
/// What one archetype did in one run (from the report's telemetry).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchetypeRun {
    pub spawned: u32,
    pub killed: u32,
    pub damage_taken: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PacingRun {
    /// 1-based run number.
    pub index: usize,
    pub seed: u64,
    pub difficulty: BalancePreset,
    pub country: String,
    pub stage: usize,
    pub outcome: Outcome,
    /// The run liberated its country.
    pub country_freed: bool,
    pub elapsed: f32,
    pub coins: u64,
    pub level_grant_coins: u64,
    pub milestone_coins: u64,
    /// Everything earned so far: run coins, level grants, milestone coins
    /// and auto-sold coins.
    pub cumulative_coins: u64,
    /// Progression rank and player level after the run.
    pub rank: u32,
    pub level: u32,
    pub lanes: u8,
    pub defender: DefenderId,
    pub kills: u32,
    /// Nodes learned in camp before this run.
    pub learned: Vec<String>,
    /// Coins spent in camp before this run, by purpose. Training and
    /// recruiting stay 0 until a plan trains or recruits.
    pub spent_tree: u64,
    pub spent_gear: u64,
    pub spent_training: u64,
    pub spent_recruit: u64,
    pub auto_sold_count: u32,
    pub auto_sold_coins: u64,
    /// Skill nodes owned and not yet owned after the run.
    pub nodes_owned: usize,
    pub nodes_left: usize,
    /// Coins held after the run.
    pub wallet: u64,
    /// Seconds from the boss's arrival to its defeat, when it fell.
    pub boss_ttk_s: Option<f32>,
    /// Per-archetype telemetry, keyed by archetype id.
    pub archetypes: BTreeMap<String, ArchetypeRun>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Milestone {
    FirstVictory,
    /// Every node of this prerequisite depth (1-based) is affordable from
    /// the coins earned so far.
    TierAffordable(usize),
    /// The scripted player owns a node of this depth.
    TierFirstNode(usize),
    Feature(CampaignFeature),
    /// At least this many countries are liberated.
    Liberation(u32),
    /// Every skill node is owned.
    TreeExhausted,
    /// The progression rank is at least this.
    Rank(u32),
    /// A recruit defended the main run.
    FirstTakeover,
    /// The scripted player owns every node of this depth (report only).
    TierOwned(usize),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PacingReport {
    pub profile: PacingProfile,
    /// The sweep's seed index (0 is the profile's own seed).
    pub seed_index: usize,
    pub seed: u64,
    pub runs: Vec<PacingRun>,
    /// Milestone, the number of runs completed when it first held, and the
    /// battle minutes so far. Camp milestones (`Feature`, `TierFirstNode`,
    /// `TierOwned`) are checked before run `i` (1-based) and count `i - 1`;
    /// the others are checked after run `i` and count `i`.
    pub milestones: Vec<(Milestone, usize, f32)>,
}
impl PacingReport {
    pub fn milestone(&self, m: Milestone) -> Option<(usize, f32)> {
        self.milestones
            .iter()
            .find(|(k, ..)| *k == m)
            .map(|(_, run, minutes)| (*run, *minutes))
    }
    /// The milestone's run count, or `max_runs + 1` when it was missed.
    pub fn milestone_or_missing(&self, m: Milestone) -> usize {
        self.milestone(m)
            .map_or(self.profile.max_runs + 1, |(run, _)| run)
    }
}
/// The element at fraction `num / den` of `sorted`, rounding the position
/// down (so `1 / 2` is the lower median).
fn lower_quantile<T: Copy>(sorted: &[T], num: usize, den: usize) -> T {
    sorted[(sorted.len() - 1) * num / den]
}
/// One milestone over the seeds of a sweep. A seed that missed it counts as
/// its profile's `max_runs + 1`; quantiles round down (the lower median).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MilestoneStats {
    pub median: usize,
    pub p25: usize,
    pub p75: usize,
    /// Seeds that reached the milestone.
    pub reached: usize,
    pub seeds: usize,
}
impl MilestoneStats {
    /// The statistics of `m` over `reports`, or None for no reports.
    pub fn of(reports: &[PacingReport], m: Milestone) -> Option<Self> {
        if reports.is_empty() {
            return None;
        }
        let mut runs: Vec<usize> = reports.iter().map(|r| r.milestone_or_missing(m)).collect();
        runs.sort_unstable();
        Some(Self {
            median: lower_quantile(&runs, 1, 2),
            p25: lower_quantile(&runs, 1, 4),
            p75: lower_quantile(&runs, 3, 4),
            reached: reports.iter().filter(|r| r.milestone(m).is_some()).count(),
            seeds: reports.len(),
        })
    }
    /// A table cell, `median [p25, p75] reached/seeds`, with every value
    /// above `max_runs` printed as `>max_runs`.
    pub fn cell(&self, max_runs: usize) -> String {
        let run = |v: usize| {
            if v > max_runs {
                format!(">{max_runs}")
            } else {
                v.to_string()
            }
        };
        format!(
            "{} [{}, {}] {}/{}",
            run(self.median),
            run(self.p25),
            run(self.p75),
            self.reached,
            self.seeds
        )
    }
}
/// The lower median, over the reports that played run `run`, of the coins
/// earned up to and including it; None when no report played it.
pub fn median_earned_after(reports: &[PacingReport], run: usize) -> Option<u64> {
    let mut earned: Vec<u64> = reports
        .iter()
        .filter_map(|r| r.runs.iter().find(|x| x.index == run))
        .map(|x| x.cumulative_coins)
        .collect();
    if earned.is_empty() {
        return None;
    }
    earned.sort_unstable();
    Some(lower_quantile(&earned, 1, 2))
}
/// Prerequisite depth of every node: the root is 0, its children 1.
pub fn node_depths() -> BTreeMap<String, usize> {
    fn depth(id: &str, out: &mut BTreeMap<String, usize>) -> usize {
        if let Some(d) = out.get(id) {
            return *d;
        }
        let node = skills().node(id).expect("validated prerequisite");
        let d = node
            .prerequisites
            .iter()
            .map(|p| depth(p, out) + 1)
            .max()
            .unwrap_or(0);
        out.insert(id.to_owned(), d);
        d
    }
    let mut out = BTreeMap::new();
    for node in &skills().nodes {
        depth(&node.id, &mut out);
    }
    out
}
/// Coins needed to learn every node up to and including `depth`.
pub fn tier_cost(depth: usize) -> u64 {
    let depths = node_depths();
    skills()
        .nodes
        .iter()
        .filter(|n| depths[&n.id] <= depth)
        .map(|n| n.price)
        .sum()
}
fn learn_recommended(g: &mut Game) -> Result<Vec<String>, String> {
    let mut learned = Vec::new();
    while let Some(id) = g.campaign.recommended_node(Mode::Personal) {
        if g.campaign.can_purchase_node(&id).is_err() {
            break;
        }
        g.campaign.purchase_node(&id)?;
        learned.push(id);
    }
    Ok(learned)
}
fn pacing_input(g: &Game, aim: PacingAim, held: &mut [f32; 2]) -> Input {
    match aim {
        PacingAim::Expert => smoke_input(g),
        PacingAim::Novice => {
            let Some(a) = &g.attempt else {
                return Input::default();
            };
            if a.tick % 30 == 0
                && let Some(e) = a
                    .enemies
                    .iter()
                    .filter(|e| e.in_view())
                    .min_by(|e, f| e.x.total_cmp(&f.x).then(e.id.cmp(&f.id)))
            {
                *held = [e.x, e.y()];
            }
            Input {
                aim: *held,
                fire: true,
                melee: a.enemies.iter().any(|e| e.contact()) && a.stamina >= 25.0,
                ..Input::default()
            }
        }
    }
}
/// Places every owned structure kind across the lanes while supply lasts,
/// traps nearest the line, as a player who bought fortifications would.
fn build_owned_structures(g: &mut Game) {
    for (column, kind) in [
        (0, StructureKind::Trap),
        (1, StructureKind::Wall),
        (2, StructureKind::Turret),
    ] {
        if !g.campaign.blueprints[kind] {
            continue;
        }
        let lanes = g.attempt.as_ref().map_or(0, |a| a.lane_count);
        for lane in construction_lane_order(lanes) {
            // Rejections (supply, an occupied cell) just skip the cell.
            let _ = g.place(kind, lane, column);
        }
    }
}
/// Per-archetype telemetry of one report.
fn archetype_runs(t: &CombatTelemetry) -> BTreeMap<String, ArchetypeRun> {
    let mut out: BTreeMap<String, ArchetypeRun> = BTreeMap::new();
    for (id, n) in &t.spawned_by_archetype {
        out.entry(id.clone()).or_default().spawned = *n;
    }
    for (id, n) in &t.kills_by_archetype {
        out.entry(id.clone()).or_default().killed = *n;
    }
    for (id, n) in &t.damage_taken_by_archetype {
        out.entry(id.clone()).or_default().damage_taken = *n;
    }
    out
}
/// The archetype ids in catalogue order, for per-archetype CSV columns.
pub fn pacing_archetype_ids() -> Vec<String> {
    enemy_catalog().archetype_ids().map(str::to_owned).collect()
}
/// Records `m` at `run` unless it was reached before.
fn reached(milestones: &mut Vec<(Milestone, usize, f32)>, m: Milestone, run: usize, minutes: f32) {
    if !milestones.iter().any(|(k, ..)| *k == m) {
        milestones.push((m, run, minutes));
    }
}
/// Plays `profile` at its own seed (seed index 0) and records milestones.
pub fn simulate_pacing(profile: &PacingProfile) -> Result<PacingReport, String> {
    simulate_pacing_seed(profile, 0)
}
/// Plays `profile` at seed index `seed_index` from a fresh profile. Index
/// 0 is the profile's own game seed with the default strategy RNG; later
/// indices vary both through [`pacing_seed`].
pub fn simulate_pacing_seed(
    profile: &PacingProfile,
    seed_index: usize,
) -> Result<PacingReport, String> {
    if let Some(option) = profile.plan.unsupported() {
        return Err(format!(
            "Pacing plan option {option} is not implemented yet ({})",
            profile.id
        ));
    }
    let seed = pacing_seed(profile.seed, seed_index);
    let mut g = Game {
        seed,
        ..Game::default()
    };
    if seed_index > 0 {
        g.strategy = crate::strategy::StrategyState::seeded(seed);
    }
    g.campaign
        .choose_survivor(crate::survivors::SurvivorId::Rook)?;
    if g.campaign.balance_settings.preset != profile.difficulty {
        g.apply(&Command::SetBalance(BalanceSettings::for_preset(
            profile.difficulty,
        )))
        .map_err(|e| e.to_string())?;
    }
    g.campaign.choose_start_country(profile.country)?;
    let depths = node_depths();
    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut runs = Vec::new();
    let mut milestones: Vec<(Milestone, usize, f32)> = Vec::new();
    let mut earned = 0u64;
    let mut minutes = 0.0f32;
    for index in 0..profile.max_runs {
        let before_tree = g.campaign.coins;
        let (learned, spent_tree, spent_gear) = match profile.spend {
            PacingSpend::Recommended => {
                let learned = learn_recommended(&mut g)?;
                let before_gear = g.campaign.coins;
                smoke_buy_gear(&mut g, Mode::Personal)?;
                (
                    learned,
                    before_tree.saturating_sub(before_gear),
                    before_gear.saturating_sub(g.campaign.coins),
                )
            }
            PacingSpend::GearFirst => {
                smoke_buy_gear(&mut g, Mode::Personal)?;
                let before_learn = g.campaign.coins;
                let learned = learn_recommended(&mut g)?;
                (
                    learned,
                    before_learn.saturating_sub(g.campaign.coins),
                    before_tree.saturating_sub(before_learn),
                )
            }
        };
        for d in 1..=max_depth {
            let mut tier = skills().nodes.iter().filter(|n| depths[&n.id] == d);
            if tier.clone().any(|n| g.campaign.has_node(&n.id)) {
                reached(&mut milestones, Milestone::TierFirstNode(d), index, minutes);
            }
            if tier.all(|n| g.campaign.has_node(&n.id)) {
                reached(&mut milestones, Milestone::TierOwned(d), index, minutes);
            }
        }
        for f in CampaignFeature::ALL {
            if g.campaign.has_feature(f) {
                reached(&mut milestones, Milestone::Feature(f), index, minutes);
            }
        }
        if index == 0 {
            g.deploy_country_stage(0, Mode::Personal)?;
        } else {
            match g.continue_journey()? {
                JourneyContinuation::Deployed => {}
                JourneyContinuation::CountryLiberated => {
                    let next = crate::world::world()
                        .countries
                        .iter()
                        .filter(|c| {
                            g.campaign.world.can_enter(&c.id)
                                && !g.campaign.world.cleared.contains(&c.id)
                        })
                        .min_by(|a, b| a.difficulty.total_cmp(&b.difficulty).then(a.id.cmp(&b.id)))
                        .ok_or("No country left to enter")?
                        .id
                        .clone();
                    g.campaign.enter_country(&next)?;
                    let stage = g.campaign.world.next_stage();
                    g.deploy_country_stage(stage, Mode::Personal)?;
                }
            }
        }
        build_owned_structures(&mut g);
        g.set_paused(false);
        let mut held = [15.0, 5.5];
        for _ in 0..60 * 600 {
            if g.attempt.is_none() {
                break;
            }
            let input = pacing_input(&g, profile.aim, &mut held);
            g.tick(input);
        }
        if g.attempt.is_some() {
            g.withdraw();
        }
        let r = g.last_result.as_ref().ok_or("Run did not settle")?;
        let level_grant_coins = r.progression_rewards.level_coins;
        let milestone_coins: u64 = r
            .progression_rewards
            .milestones
            .iter()
            .map(|m| m.coins)
            .sum();
        earned += r.coins + level_grant_coins + milestone_coins + r.auto_sold.coins;
        minutes += r.elapsed / 60.0;
        let run = index + 1;
        if r.outcome == Outcome::Victory {
            reached(&mut milestones, Milestone::FirstVictory, run, minutes);
        }
        let liberated = g.campaign.world.cleared.len() as u32;
        for n in 1..=liberated {
            reached(&mut milestones, Milestone::Liberation(n), run, minutes);
        }
        for d in 1..=max_depth {
            if earned >= tier_cost(d) {
                reached(&mut milestones, Milestone::TierAffordable(d), run, minutes);
            }
        }
        let nodes_owned = skills()
            .nodes
            .iter()
            .filter(|n| g.campaign.has_node(&n.id))
            .count();
        let nodes_left = skills().nodes.len() - nodes_owned;
        if nodes_left == 0 {
            reached(&mut milestones, Milestone::TreeExhausted, run, minutes);
        }
        let rank = g.campaign.progression_rank();
        for n in 1..=rank {
            reached(&mut milestones, Milestone::Rank(n), run, minutes);
        }
        let defender = r.defender();
        if matches!(defender, DefenderId::Recruit(_)) {
            reached(&mut milestones, Milestone::FirstTakeover, run, minutes);
        }
        let t = &r.telemetry;
        runs.push(PacingRun {
            index: run,
            seed,
            difficulty: g.campaign.balance_settings.preset,
            country: r.country_id.clone().unwrap_or_default(),
            stage: r.stage,
            outcome: r.outcome,
            country_freed: r.country_freed,
            elapsed: r.elapsed,
            coins: r.coins,
            level_grant_coins,
            milestone_coins,
            cumulative_coins: earned,
            rank,
            level: g.campaign.player.level,
            lanes: r.lane_count,
            defender,
            kills: r.kills,
            learned,
            spent_tree,
            spent_gear,
            spent_training: 0,
            spent_recruit: 0,
            auto_sold_count: r.auto_sold.count,
            auto_sold_coins: r.auto_sold.coins,
            nodes_owned,
            nodes_left,
            wallet: g.campaign.coins,
            boss_ttk_s: t
                .boss_arrival_tick
                .zip(t.boss_defeat_tick)
                .map(|(arrived, fell)| {
                    fell.saturating_sub(arrived) as f32 / TICKS_PER_SECOND as f32
                }),
            archetypes: archetype_runs(t),
        });
        if profile
            .stop_after
            .is_some_and(|m| milestones.iter().any(|(k, ..)| *k == m))
        {
            break;
        }
    }
    milestones.sort_by_key(|(m, ..)| *m);
    Ok(PacingReport {
        profile: *profile,
        seed_index,
        seed,
        runs,
        milestones,
    })
}
/// Plays every profile at seed indices `0..seeds` on all available cores.
/// Reports are ordered by (profile position, seed index), whatever the
/// thread count.
pub fn pacing_sweep(profiles: &[PacingProfile], seeds: usize) -> Result<Vec<PacingReport>, String> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    pacing_sweep_with_threads(profiles, seeds, threads)
}
/// [`pacing_sweep`] on at most `threads` threads (at least one). Each job
/// builds its own game, so the result does not depend on the thread count.
pub fn pacing_sweep_with_threads(
    profiles: &[PacingProfile],
    seeds: usize,
    threads: usize,
) -> Result<Vec<PacingReport>, String> {
    let jobs: Vec<(usize, usize)> = (0..profiles.len())
        .flat_map(|p| (0..seeds).map(move |s| (p, s)))
        .collect();
    let next = AtomicUsize::new(0);
    let threads = threads.clamp(1, jobs.len().max(1));
    let mut done: Vec<(usize, Result<PacingReport, String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let job = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(p, s)) = jobs.get(job) else {
                            break;
                        };
                        out.push((job, simulate_pacing_seed(&profiles[p], s)));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });
    done.sort_by_key(|(job, _)| *job);
    done.into_iter().map(|(_, report)| report).collect()
}
/// The legacy profiles the pre-v2 table reports, each limited to
/// `max_runs`.
pub fn pacing_profiles(max_runs: usize) -> Vec<PacingProfile> {
    LEGACY_PROFILE_IDS
        .iter()
        .filter_map(|id| by_id(id))
        .map(|p| PacingProfile { max_runs, ..p })
        .collect()
}
