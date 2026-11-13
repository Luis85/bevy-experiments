//! Headless pacing measurement: a scripted player walks the fresh-profile
//! loop (survivor → starting country → defenses → camp → continue) and the
//! report records when each milestone first happens. Un-gated so the
//! `pacing` example and the regression tests share one implementation.
//!
//! Harness v2 (W2-01): named profiles in a registry ([`by_id`]), per-seed
//! variation ([`pacing_seed`]), a threaded sweep ([`pacing_sweep`]) and
//! milestone statistics over seeds ([`MilestoneStats`]). Other remits add
//! registry rows and evaluators on this API. The goal-5 evaluators (W2-03)
//! live in [`targets`], the goal-6 rows' evaluators (W2-04) in [`goal6`].
//! Every plan takes its camp steps through `Game::apply`: the difficulty
//! (SetBalance), the survivor (ChooseSurvivor), the skill nodes
//! (BuySkillNode) and the gear (BuyItem and Equip). The plans that sell,
//! farm or hand the main run to a recruit (W2-42) also equip, sell, recruit,
//! set the next defender and take the replay or frontier deploy through it.
//! As built (W2 review), the non-strategy loop still calls the rest
//! directly: `Campaign::choose_start_country` (ChooseStart would also deploy
//! stage 0 and pause), `Game::continue_journey`, `Campaign::enter_country`,
//! the Survivor plan's own `Game::deploy_country_stage` (Deploy would also
//! unpause before the structures are placed), and the battle controls
//! (`Game::place`, `set_paused`, `withdraw`). Routing those through
//! commands could move pacing and would need plan §9.6 rows (R28). The
//! strategy rows and their goal-3 evaluators (W2-43, ruling P7) live in
//! [`strategy`]: a plan with a [`StrategyPlan`] is played by a bot that
//! acts only through commands.
mod goal6;
#[cfg(test)]
mod plan_tests;
mod strategy;
mod targets;
pub use goal6::{
    DefeatWindow, GOAL6_BOSS_TTK_S, GOAL6_CLAMPED_FIRST_VICTORY, GOAL6_DEFEAT_RATE_PCT,
    GOAL6_LUX_EASY_MAX_RUNS, GOAL6_LUX_HARD_OVER_NORMAL, GOAL6_LUX_PROFILE_IDS,
    GOAL6_START_PROFILE_IDS, GOAL6_STREAK_FROM_RUN, GOAL6_STREAK_LIMIT, GOAL6_WINDOW_RUNS,
    RunsToLiberate, archetype_coverage, boss_ttk_median_s, clamped_start_first_victory,
    defeat_rate_windows, defeat_windows, longest_win_streak, max_win_streak, runs_to_liberate,
};
pub use strategy::{
    DEU_THREE_STAGE_NEIGHBOURS, GOAL3_LIBERATIONS, GOAL3_NEIGHBOURS_FREED,
    GOAL3_PARALLEL_OVER_SOLO, GOAL3_REOPEN_LIMIT, GOAL3_REOPEN_WINDOW, Overrun, PacingParallel,
    PacingRoute, STRATEGY_PROFILE_ID, STRATEGY_PROFILE_IDS, StrategyPlan, StrategyReport,
    StrategyRun, StrategyStep, StrategyTrace, StrategyUnlock, liberations_and_coins_per_minute,
    outbreak_reopens, parallel_frees_neighbours, reopens_over_liberations, replay_strategy,
    runs_to_free_neighbours, simulate_strategy, simulate_strategy_seed, simulate_strategy_traced,
    strategy_sweep, strategy_table,
};
pub use targets::{
    GOAL5_FIRST_VICTORY, GOAL5_TIER_WINDOWS, UNLOCK_ORDER, median_run_within, tier_affordable_runs,
    tree_outlasts_liberation, unlock_order,
};

use super::smoke::smoke_buy_gear_with;
use super::*;
use crate::balance::{BalancePreset, BalanceSettings};
use crate::command::{Command, CommandOutcome};
use crate::content::ItemDefId;
use crate::enemy_catalog::enemy_catalog;
use crate::skill_catalog::{CampaignFeature, SkillNode, skills};
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
/// What the scripted player does with spare items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingSell {
    /// Never sells.
    Never,
    /// Sells every item `SellUnprotected` would in camp, before spending.
    /// The coins reach the wallet; like any sale outside settlement they
    /// are not part of `cumulative_coins`.
    ManualExtras,
    /// Stores the default auto-sell rule, enabled (Poor and Common drops,
    /// every slot, upgrades kept), before the first run, so settlement
    /// sells each run's matching drops (hook H1).
    Auto,
}
/// Who defends the main run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingDefender {
    /// The chosen survivor defends every run.
    Survivor,
    /// Recruits once the roster is empty and a liberated country offers a
    /// recruit (the first offer), then sends the lowest-id recruit that can
    /// defend into every run. The survivor defends otherwise, and retries
    /// every stage a recruit lost, so the journey keeps moving.
    RecruitWhenReady,
}
/// Whether the scripted player replays stages to farm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingFarm {
    /// Always continues the journey.
    None,
    /// After this many lost runs (defeats or withdrawals) in a row on the
    /// country's frontier stage, replays the stage before it once, then
    /// returns to the frontier.
    ReplayAfterDefeats(u32),
}
/// Where the next run of a country goes under a farm plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FarmStep {
    /// `ContinueJourney` as usual.
    Continue,
    /// Replay this cleared stage.
    Replay(usize),
    /// Back to this frontier stage after a replay.
    Frontier(usize),
}
/// The farm plan's next step: `frontier` is the active country's next
/// stage, `defeats` the frontier defeats in a row since the last replay,
/// and `replayed` whether the last run was a replay.
fn farm_step(farm: PacingFarm, frontier: usize, defeats: u32, replayed: bool) -> FarmStep {
    match farm {
        PacingFarm::None => FarmStep::Continue,
        PacingFarm::ReplayAfterDefeats(_) if replayed => FarmStep::Frontier(frontier),
        PacingFarm::ReplayAfterDefeats(n) if n > 0 && defeats >= n && frontier > 0 => {
            FarmStep::Replay(frontier - 1)
        }
        PacingFarm::ReplayAfterDefeats(_) => FarmStep::Continue,
    }
}
/// The camp plan a profile follows besides aim and spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacingPlan {
    /// Equip dropped items that beat the current loadout, in camp before
    /// any sale or purchase ([`equip_dropped_upgrades`]). The strategy bot
    /// does not combine it with a strategy plan.
    pub equip_drops: bool,
    pub sell: PacingSell,
    pub defender: PacingDefender,
    pub farm: PacingFarm,
    /// Train Mara and recruits in camp. Not implemented yet:
    /// [`simulate_pacing`] rejects a plan that sets it.
    pub train_companions: bool,
    /// The strategy layer (W2-43): recruits, expeditions, guards and camp
    /// time, played by the command-only strategy bot. None is the solo loop.
    pub strategy: Option<StrategyPlan>,
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
        strategy: None,
    };
    fn unsupported(&self) -> Option<&'static str> {
        self.train_companions.then_some("train_companions")
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
    /// End the profile once this milestone is held, checked after each run
    /// (at most `max_runs` runs either way). A post-run milestone recorded
    /// at `n` (first held after run `n`) stops right after run `n`. A camp
    /// milestone (Feature, TierFirstNode, TierOwned, OperationsOpen) is
    /// recorded at `n` when first held in camp before run `n + 1`, but the
    /// check only follows a run, so run `n + 1` still plays and the report
    /// has `n + 1` runs.
    pub stop_after: Option<Milestone>,
}
/// The registry key of the acceptance profile (goals 5 and 6).
pub const DEFAULT_PROFILE_ID: &str = "expert-deu";
/// The profiles the default table prints, in column order. Their output at
/// one seed is the pre-v2 table.
pub const LEGACY_PROFILE_IDS: [&str; 3] = ["expert-deu", "novice-deu", "novice-gear-lux"];
/// The expert, tree-first player of the acceptance profile, on Normal, for
/// the goal-6 rows to vary by country, preset and stop.
const GOAL6_PLAYER: PacingProfile = PacingProfile {
    id: "",
    name: "",
    seed: 0x1A571167,
    country: "",
    aim: PacingAim::Expert,
    spend: PacingSpend::Recommended,
    max_runs: 20,
    difficulty: BalancePreset::Normal,
    plan: PacingPlan::SOLO,
    stop_after: None,
};
/// The acceptance profile, which the W2-42 rows vary only in their named
/// fields (see the registry comment above `manual-sell`).
const EXPERT_DEU: PacingProfile = PacingProfile {
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
};
/// Every named profile. Ids are stable and unique; rows are appended.
const REGISTRY: [PacingProfile; 19] = [
    EXPERT_DEU,
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
    // Goal 6 (W2-04, ruling W-4): Luxembourg runs to liberate per preset,
    // each stopping at the first liberation within 20 runs.
    PacingProfile {
        id: "lux-easy",
        name: "expert, tree first, Luxembourg, Easy",
        country: "LUX",
        difficulty: BalancePreset::Easy,
        stop_after: Some(Milestone::Liberation(1)),
        ..GOAL6_PLAYER
    },
    PacingProfile {
        id: "lux-normal",
        name: "expert, tree first, Luxembourg, Normal",
        country: "LUX",
        stop_after: Some(Milestone::Liberation(1)),
        ..GOAL6_PLAYER
    },
    PacingProfile {
        id: "lux-hard",
        name: "expert, tree first, Luxembourg, Hard",
        country: "LUX",
        difficulty: BalancePreset::Hard,
        stop_after: Some(Milestone::Liberation(1)),
        ..GOAL6_PLAYER
    },
    // The clamped starts: only the first victory is checked, so each stops
    // there within 20 runs.
    PacingProfile {
        id: "start-aut",
        name: "expert, tree first, Austria start",
        country: "AUT",
        stop_after: Some(Milestone::FirstVictory),
        ..GOAL6_PLAYER
    },
    PacingProfile {
        id: "start-bgd",
        name: "expert, tree first, Bangladesh start",
        country: "BGD",
        stop_after: Some(Milestone::FirstVictory),
        ..GOAL6_PLAYER
    },
    // W2-42 (W2-I): the rows that need other remits' commands. Each varies
    // the acceptance profile only in the fields it names. manual-sell,
    // auto-sell, easy, hard and long change one field (the sale plan, the
    // preset or the run limit), so their table columns read against
    // expert-deu's. takeover changes two (the defender plan and a Luxembourg
    // start) and farm two (the farm plan and the novice aim), so those two
    // columns do not isolate their plan (R51).
    PacingProfile {
        id: "manual-sell",
        name: "expert, tree first, Germany, sells extras",
        plan: PacingPlan {
            sell: PacingSell::ManualExtras,
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
    },
    PacingProfile {
        id: "auto-sell",
        name: "expert, tree first, Germany, auto-sell",
        plan: PacingPlan {
            sell: PacingSell::Auto,
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
    },
    // A Luxembourg start is liberated within a few runs (lux-normal), so a
    // recruit takes over early enough to measure.
    PacingProfile {
        id: "takeover",
        name: "expert, tree first, Luxembourg, recruit takes over",
        country: "LUX",
        plan: PacingPlan {
            defender: PacingDefender::RecruitWhenReady,
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
    },
    // The novice loses often enough on the frontier to farm.
    PacingProfile {
        id: "farm",
        name: "novice, tree first, Germany, farms after 2 defeats",
        aim: PacingAim::Novice,
        plan: PacingPlan {
            farm: PacingFarm::ReplayAfterDefeats(2),
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
    },
    PacingProfile {
        id: "easy",
        name: "expert, tree first, Germany, Easy",
        difficulty: BalancePreset::Easy,
        ..EXPERT_DEU
    },
    PacingProfile {
        id: "hard",
        name: "expert, tree first, Germany, Hard",
        difficulty: BalancePreset::Hard,
        ..EXPERT_DEU
    },
    // Informational: 5 seeds (design-progression-tree §11).
    PacingProfile {
        id: "long",
        name: "expert, tree first, Germany, 150 runs",
        max_runs: 150,
        ..EXPERT_DEU
    },
    // W2-43 (design-campaign-defenders 1.13): the strategy rows. S1 plays
    // the whole strategy layer from Luxembourg; S2a and S2b free Germany's
    // 3-stage neighbours alone and with expeditions.
    PacingProfile {
        id: "strategy",
        name: "strategy, novice, gear first, Luxembourg",
        seed: 2026,
        country: "LUX",
        aim: PacingAim::Novice,
        spend: PacingSpend::GearFirst,
        max_runs: 120,
        plan: PacingPlan {
            strategy: Some(StrategyPlan::S1),
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
    },
    PacingProfile {
        id: "neighbours-solo",
        name: "neighbours solo, Germany",
        plan: PacingPlan {
            strategy: Some(StrategyPlan::NEIGHBOURS_SOLO),
            ..PacingPlan::SOLO
        },
        stop_after: Some(Milestone::Liberation(1 + GOAL3_NEIGHBOURS_FREED as u32)),
        ..EXPERT_DEU
    },
    PacingProfile {
        id: "neighbours-parallel",
        name: "neighbours parallel, Germany",
        plan: PacingPlan {
            strategy: Some(StrategyPlan::NEIGHBOURS_PARALLEL),
            ..PacingPlan::SOLO
        },
        stop_after: Some(Milestone::Liberation(1 + GOAL3_NEIGHBOURS_FREED as u32)),
        ..EXPERT_DEU
    },
    // Acceptance G5.6 (W2 review): the acceptance profile that also wears
    // the drops that beat its loadout.
    PacingProfile {
        id: "equip-drops",
        name: "expert, tree first, Germany, equips drops",
        plan: PacingPlan {
            equip_drops: true,
            ..PacingPlan::SOLO
        },
        ..EXPERT_DEU
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
    /// and auto-sold coins (for the strategy bot, also the level grants and
    /// milestone coins settled on expedition records in camp).
    pub cumulative_coins: u64,
    /// Progression rank and player level after the run.
    pub rank: u32,
    pub level: u32,
    pub lanes: u8,
    pub defender: DefenderId,
    pub kills: u32,
    /// Nodes learned in camp before this run.
    pub learned: Vec<String>,
    /// Coins spent in camp before this run, by purpose. Training stays 0
    /// until a plan trains; recruiting is paid by a takeover plan
    /// (`recruit_when_ready`) and by a strategy plan that recruits
    /// (`StrategyPlan.recruit_up_to` > 0).
    pub spent_tree: u64,
    pub spent_gear: u64,
    pub spent_training: u64,
    pub spent_recruit: u64,
    pub auto_sold_count: u32,
    pub auto_sold_coins: u64,
    /// Skill nodes owned after the run, and those neither owned nor retired
    /// ([`nodes_left`]).
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
    /// No unowned, non-retired skill node is left ([`tree_exhausted`]).
    TreeExhausted,
    /// The progression rank is at least this.
    Rank(u32),
    /// A recruit defended the main run.
    FirstTakeover,
    /// The scripted player owns every node of this depth (report only).
    TierOwned(usize),
    /// The Operations page is reachable ([`Game::operations_available`]).
    OperationsOpen,
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
    /// `TierOwned`, `OperationsOpen`) are checked before run `i` (1-based)
    /// and count `i - 1`; the others are checked after run `i` and count `i`.
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
/// earned up to and including it; None when no report played it. A seed
/// that stopped earlier (`stop_after` or `max_runs`) is left out, so the
/// median of a stopping row leans toward slow seeds; [`EarnedAfter`]
/// also says how many seeds it covers.
pub fn median_earned_after(reports: &[PacingReport], run: usize) -> Option<u64> {
    EarnedAfter::of(reports, run).map(|e| e.median)
}
/// The coins earned up to and including one run, over the reports that
/// played it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EarnedAfter {
    /// The lower median over the `sampled` reports.
    pub median: u64,
    /// The reports that played the run.
    pub sampled: usize,
    /// Every report.
    pub seeds: usize,
}
impl EarnedAfter {
    /// None when no report played run `run`.
    pub fn of(reports: &[PacingReport], run: usize) -> Option<Self> {
        let mut earned: Vec<u64> = reports
            .iter()
            .filter_map(|r| r.runs.iter().find(|x| x.index == run))
            .map(|x| x.cumulative_coins)
            .collect();
        if earned.is_empty() {
            return None;
        }
        earned.sort_unstable();
        Some(Self {
            median: lower_quantile(&earned, 1, 2),
            sampled: earned.len(),
            seeds: reports.len(),
        })
    }
    /// Some seeds stopped before the run, so the median covers only the
    /// slower ones.
    pub fn partial(&self) -> bool {
        self.sampled < self.seeds
    }
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
/// A node retired from sale: the append-only catalogue keeps its id, but
/// no campaign can buy it, so the tree is exhausted without it (ruling
/// W-3). Today that is only the innate starter root: price 0, granted at
/// campaign creation and never sold, and the only price-0 node the
/// catalogue validator allows.
pub fn node_retired(node: &SkillNode) -> bool {
    node.price == 0
}
/// The nodes of `nodes` that are neither owned nor retired.
pub fn nodes_left<'a>(
    nodes: impl IntoIterator<Item = &'a SkillNode>,
    owned: impl Fn(&SkillNode) -> bool,
    retired: impl Fn(&SkillNode) -> bool,
) -> usize {
    nodes
        .into_iter()
        .filter(|n| !owned(n) && !retired(n))
        .count()
}
/// [`Milestone::TreeExhausted`] holds: no unowned, non-retired node is
/// left (ruling W-3).
pub fn tree_exhausted<'a>(
    nodes: impl IntoIterator<Item = &'a SkillNode>,
    owned: impl Fn(&SkillNode) -> bool,
    retired: impl Fn(&SkillNode) -> bool,
) -> bool {
    nodes_left(nodes, owned, retired) == 0
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
        g.apply(&Command::BuySkillNode(id.clone()))
            .map_err(|e| e.to_string())?;
        learned.push(id);
    }
    Ok(learned)
}
/// Buys catalogue item `definition` and equips it on the survivor through
/// `BuyItem` and `Equip`, handing each command to `seen` before it is
/// applied (the strategy bot records it).
fn buy_and_equip(
    g: &mut Game,
    definition: usize,
    mut seen: impl FnMut(&Command),
) -> Result<(), String> {
    let buy = Command::BuyItem(ItemDefId::at(definition));
    seen(&buy);
    let CommandOutcome::Id(id) = g.apply(&buy).map_err(|e| e.to_string())? else {
        return Err("BuyItem answered without an item id".to_owned());
    };
    let equip = Command::Equip {
        mode: Mode::Personal,
        id,
    };
    seen(&equip);
    g.apply(&equip).map(|_| ()).map_err(|e| e.to_string())
}
/// The smoke bot's gear choice for the survivor, bought through commands.
fn buy_gear(g: &mut Game) -> Result<(), String> {
    smoke_buy_gear_with(g, Mode::Personal, |g, definition| {
        buy_and_equip(g, definition, |_| {})
    })
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
/// Every placement a player who bought fortifications tries, in order: each
/// owned structure kind across the lanes, traps nearest the line.
fn structure_placements(g: &Game) -> Vec<(StructureKind, u8, u8)> {
    let lanes = g.attempt.as_ref().map_or(0, |a| a.lane_count);
    let mut out = Vec::new();
    for (column, kind) in [
        (0, StructureKind::Trap),
        (1, StructureKind::Wall),
        (2, StructureKind::Turret),
    ] {
        if g.campaign.blueprints[kind] {
            out.extend(
                construction_lane_order(lanes)
                    .into_iter()
                    .map(|lane| (kind, lane, column)),
            );
        }
    }
    out
}
/// Places every owned structure kind across the lanes while supply lasts,
/// traps nearest the line, as a player who bought fortifications would.
fn build_owned_structures(g: &mut Game) {
    for (kind, lane, column) in structure_placements(g) {
        // Rejections (supply, an occupied cell) just skip the cell.
        let _ = g.place(kind, lane, column);
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
/// The equip-drops plan's camp step: for each slot in [`Slot::ALL`] order,
/// equips through `Command::Equip` the owned item that raises the
/// survivor's slot score the most by the recommender's own test
/// ([`Campaign::upgrade_gain`], so only an item the survivor may use and
/// that beats the slot by more than the margin; ties keep the lowest id).
/// Items a recruit or an expedition holds are skipped. Purchases are
/// equipped when bought, so what this finds is loot. Returns the ids
/// equipped, in slot order.
///
/// [`Campaign::upgrade_gain`]: crate::campaign::Campaign::upgrade_gain
pub fn equip_dropped_upgrades(g: &mut Game) -> Result<Vec<u64>, String> {
    let mut equipped = Vec::new();
    for slot in crate::content::Slot::ALL {
        let c = &g.campaign;
        let best = c
            .items
            .iter()
            .filter(|item| {
                !c.item_reserved(item.id)
                    && item.definition.definition().is_some_and(|d| d.slot == slot)
            })
            .filter_map(|item| {
                c.upgrade_gain(Mode::Personal, c.player.loadout, &c.npc, item.id)
                    .map(|gain| (item.id, gain))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        if let Some((id, _)) = best {
            g.apply(&Command::Equip {
                mode: Mode::Personal,
                id,
            })
            .map_err(|e| e.to_string())?;
            equipped.push(id);
        }
    }
    Ok(equipped)
}
/// The camp sale of `sell`, before spending: `ManualExtras` sells what
/// `SellUnprotected` would (nothing is `Ok(0)`); the other plans sell
/// nothing in camp.
fn camp_sale(g: &mut Game, sell: PacingSell) -> Result<(), String> {
    if sell == PacingSell::ManualExtras {
        g.apply(&Command::SellUnprotected)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// A takeover plan's recruiting: while the roster is empty, recruits the
/// first offer of the first liberated country (in liberation order) that
/// offers one. Returns the coins paid.
fn recruit_when_ready(g: &mut Game) -> Result<u64, String> {
    if !g.strategy.roster.is_empty() {
        return Ok(0);
    }
    let Some(country) = g
        .campaign
        .world
        .cleared
        .iter()
        .find(|c| g.recruit_offer(c).is_ok())
        .cloned()
    else {
        return Ok(0);
    };
    let before = g.campaign.coins;
    g.apply(&Command::Recruit(country))
        .map_err(|e| e.to_string())?;
    Ok(before.saturating_sub(g.campaign.coins))
}
/// Who a takeover plan sends next: the survivor retries a stage a recruit
/// lost; otherwise the lowest-id recruit that can defend now, or else the
/// survivor.
fn takeover_defender(g: &Game) -> DefenderId {
    if g.last_result.as_ref().is_some_and(|r| {
        matches!(r.defender(), DefenderId::Recruit(_)) && r.outcome != Outcome::Victory
    }) {
        return DefenderId::Player;
    }
    let mut ids: Vec<u64> = g.strategy.roster.iter().map(|r| r.id).collect();
    ids.sort_unstable();
    ids.into_iter()
        .map(DefenderId::Recruit)
        .find(|d| g.defender_selectable(*d).is_ok())
        .unwrap_or(DefenderId::Player)
}
/// Deploys on the active country's `stage` through a command: the survivor
/// with `Deploy`, a takeover plan's next defender with `BeginDefense`.
fn deploy_stage(g: &mut Game, defender: PacingDefender, stage: usize) -> Result<(), String> {
    let command = match defender {
        PacingDefender::Survivor => Command::Deploy {
            stage,
            mode: Mode::Personal,
        },
        PacingDefender::RecruitWhenReady => Command::BeginDefense { stage },
    };
    g.apply(&command).map(|_| ()).map_err(|e| e.to_string())
}
/// Plays `profile` at its own seed (seed index 0) and records milestones.
pub fn simulate_pacing(profile: &PacingProfile) -> Result<PacingReport, String> {
    simulate_pacing_seed(profile, 0)
}
/// The fresh game seed index `seed_index` of `profile` starts from, before
/// any command: the game seed [`pacing_seed`] gives, and the strategy RNG
/// `Rng::new(seed ^ "PLAGUE")` for every later index, or for every index
/// when the plan seeds the strategy RNG (W2-43). Index 0 of a plan that
/// does not keeps the default strategy RNG, so one seed reproduces the
/// pre-v2 table. No per-journey seed ships (design-campaign-defenders 1.13).
pub fn pacing_start(profile: &PacingProfile, seed_index: usize) -> Game {
    let seed = pacing_seed(profile.seed, seed_index);
    let mut g = Game {
        seed,
        ..Game::default()
    };
    let seeded = profile.plan.strategy.is_some_and(|s| s.seed_strategy);
    if seed_index > 0 || seeded {
        g.strategy = crate::strategy::StrategyState::seeded(seed);
    }
    g
}
/// Plays `profile` at seed index `seed_index` from a fresh profile
/// ([`pacing_start`]). A plan with a [`StrategyPlan`] is played by the
/// strategy bot ([`simulate_strategy_seed`]); this returns its runs and
/// milestones.
pub fn simulate_pacing_seed(
    profile: &PacingProfile,
    seed_index: usize,
) -> Result<PacingReport, String> {
    if profile.plan.strategy.is_some() {
        return simulate_strategy_seed(profile, seed_index).map(|r| r.pacing);
    }
    if let Some(option) = profile.plan.unsupported() {
        return Err(format!(
            "Pacing plan option {option} is not implemented yet ({})",
            profile.id
        ));
    }
    let mut g = pacing_start(profile, seed_index);
    let seed = g.seed;
    g.apply(&Command::ChooseSurvivor(crate::survivors::SurvivorId::Rook))
        .map_err(|e| e.to_string())?;
    if g.campaign.balance_settings.preset != profile.difficulty {
        g.apply(&Command::SetBalance(BalanceSettings::for_preset(
            profile.difficulty,
        )))
        .map_err(|e| e.to_string())?;
    }
    g.campaign.choose_start_country(profile.country)?;
    let plan = profile.plan;
    if plan.sell == PacingSell::Auto {
        g.apply(&Command::SetAutoSell(crate::campaign::AutoSellRule {
            enabled: true,
            ..crate::campaign::AutoSellRule::default()
        }))
        .map_err(|e| e.to_string())?;
    }
    let mut book = RunBook::new();
    // The farm plan's frontier defeats in a row and whether the last run
    // was a replay.
    let mut defeats = 0u32;
    let mut replayed = false;
    for index in 0..profile.max_runs {
        if plan.equip_drops {
            equip_dropped_upgrades(&mut g)?;
        }
        camp_sale(&mut g, plan.sell)?;
        let spent_recruit = match plan.defender {
            PacingDefender::Survivor => 0,
            PacingDefender::RecruitWhenReady => recruit_when_ready(&mut g)?,
        };
        let before_tree = g.campaign.coins;
        let (learned, spent_tree, spent_gear) = match profile.spend {
            PacingSpend::Recommended => {
                let learned = learn_recommended(&mut g)?;
                let before_gear = g.campaign.coins;
                buy_gear(&mut g)?;
                (
                    learned,
                    before_tree.saturating_sub(before_gear),
                    before_gear.saturating_sub(g.campaign.coins),
                )
            }
            PacingSpend::GearFirst => {
                buy_gear(&mut g)?;
                let before_learn = g.campaign.coins;
                let learned = learn_recommended(&mut g)?;
                (
                    learned,
                    before_learn.saturating_sub(g.campaign.coins),
                    before_tree.saturating_sub(before_learn),
                )
            }
        };
        book.camp(&g, index);
        if plan.defender == PacingDefender::RecruitWhenReady {
            let defender = takeover_defender(&g);
            if g.next_defender() != defender {
                g.apply(&Command::SetNextDefender(defender))
                    .map_err(|e| e.to_string())?;
            }
        }
        let step = if index == 0 {
            FarmStep::Continue
        } else {
            farm_step(plan.farm, g.campaign.world.next_stage(), defeats, replayed)
        };
        if index == 0 {
            if plan.defender == PacingDefender::Survivor {
                g.deploy_country_stage(0, Mode::Personal)?;
            } else {
                deploy_stage(&mut g, plan.defender, 0)?;
            }
        } else if let FarmStep::Replay(stage) | FarmStep::Frontier(stage) = step {
            deploy_stage(&mut g, plan.defender, stage)?;
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
                    if plan.defender == PacingDefender::Survivor {
                        g.deploy_country_stage(stage, Mode::Personal)?;
                    } else {
                        deploy_stage(&mut g, plan.defender, stage)?;
                    }
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
        let outcome = book.record(
            &g,
            index,
            seed,
            CampSpend {
                learned,
                spent_tree,
                spent_gear,
                spent_recruit,
            },
        )?;
        replayed = matches!(step, FarmStep::Replay(_));
        if replayed || outcome == Outcome::Victory {
            defeats = 0;
        } else {
            defeats += 1;
        }
        if book.stopped(profile.stop_after) {
            break;
        }
    }
    Ok(book.report(profile, seed_index, seed))
}
/// What one camp spent before a run, for its [`PacingRun`].
#[derive(Clone, Debug, Default)]
struct CampSpend {
    learned: Vec<String>,
    spent_tree: u64,
    spent_gear: u64,
    spent_recruit: u64,
}
/// The bookkeeping every plan shares (split out of the solo loop by W2-43
/// for the strategy bot): the runs, the milestones, the coins earned and
/// the battle minutes so far.
struct RunBook {
    depths: BTreeMap<String, usize>,
    max_depth: usize,
    runs: Vec<PacingRun>,
    milestones: Vec<(Milestone, usize, f32)>,
    earned: u64,
    minutes: f32,
}
impl RunBook {
    fn new() -> Self {
        let depths = node_depths();
        let max_depth = depths.values().copied().max().unwrap_or(0);
        Self {
            depths,
            max_depth,
            runs: Vec::new(),
            milestones: Vec::new(),
            earned: 0,
            minutes: 0.0,
        }
    }
    /// The camp milestones, checked in camp after spending, before run
    /// `index + 1`: they count `index` runs completed.
    fn camp(&mut self, g: &Game, index: usize) {
        let minutes = self.minutes;
        for d in 1..=self.max_depth {
            let mut tier = skills().nodes.iter().filter(|n| self.depths[&n.id] == d);
            if tier.clone().any(|n| g.campaign.has_node(&n.id)) {
                reached(
                    &mut self.milestones,
                    Milestone::TierFirstNode(d),
                    index,
                    minutes,
                );
            }
            if tier.all(|n| g.campaign.has_node(&n.id)) {
                reached(
                    &mut self.milestones,
                    Milestone::TierOwned(d),
                    index,
                    minutes,
                );
            }
        }
        for f in CampaignFeature::ALL {
            if g.campaign.has_feature(f) {
                reached(&mut self.milestones, Milestone::Feature(f), index, minutes);
            }
        }
        if g.operations_available() {
            reached(
                &mut self.milestones,
                Milestone::OperationsOpen,
                index,
                minutes,
            );
        }
    }
    /// Records run `index + 1` from its settled report and checks the
    /// post-run milestones. Returns the run's outcome.
    fn record(
        &mut self,
        g: &Game,
        index: usize,
        seed: u64,
        spend: CampSpend,
    ) -> Result<Outcome, String> {
        let r = g.last_result.as_ref().ok_or("Run did not settle")?;
        let level_grant_coins = r.progression_rewards.level_coins;
        let milestone_coins: u64 = r
            .progression_rewards
            .milestones
            .iter()
            .map(|m| m.coins)
            .sum();
        self.earned += r.coins + level_grant_coins + milestone_coins + r.auto_sold.coins;
        self.minutes += r.elapsed / 60.0;
        let (run, minutes, milestones) = (index + 1, self.minutes, &mut self.milestones);
        if r.outcome == Outcome::Victory {
            reached(milestones, Milestone::FirstVictory, run, minutes);
        }
        let liberated = g.campaign.world.cleared.len() as u32;
        for n in 1..=liberated {
            reached(milestones, Milestone::Liberation(n), run, minutes);
        }
        for d in 1..=self.max_depth {
            if self.earned >= tier_cost(d) {
                reached(milestones, Milestone::TierAffordable(d), run, minutes);
            }
        }
        let nodes_owned = skills()
            .nodes
            .iter()
            .filter(|n| g.campaign.has_node(&n.id))
            .count();
        let left = nodes_left(
            &skills().nodes,
            |n| g.campaign.has_node(&n.id),
            node_retired,
        );
        if left == 0 {
            reached(milestones, Milestone::TreeExhausted, run, minutes);
        }
        let rank = g.campaign.progression_rank();
        for n in 1..=rank {
            reached(milestones, Milestone::Rank(n), run, minutes);
        }
        let defender = r.defender();
        if matches!(defender, DefenderId::Recruit(_)) {
            reached(milestones, Milestone::FirstTakeover, run, minutes);
        }
        let t = &r.telemetry;
        self.runs.push(PacingRun {
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
            cumulative_coins: self.earned,
            rank,
            level: g.campaign.player.level,
            lanes: r.lane_count,
            defender,
            kills: r.kills,
            learned: spend.learned,
            spent_tree: spend.spent_tree,
            spent_gear: spend.spent_gear,
            spent_training: 0,
            spent_recruit: spend.spent_recruit,
            auto_sold_count: r.auto_sold.count,
            auto_sold_coins: r.auto_sold.coins,
            nodes_owned,
            nodes_left: left,
            wallet: g.campaign.coins,
            boss_ttk_s: t
                .boss_arrival_tick
                .zip(t.boss_defeat_tick)
                .map(|(arrived, fell)| {
                    fell.saturating_sub(arrived) as f32 / TICKS_PER_SECOND as f32
                }),
            archetypes: archetype_runs(t),
        });
        Ok(r.outcome)
    }
    /// The profile's stop milestone has been reached.
    fn stopped(&self, stop_after: Option<Milestone>) -> bool {
        stop_after.is_some_and(|m| self.milestones.iter().any(|(k, ..)| *k == m))
    }
    fn report(mut self, profile: &PacingProfile, seed_index: usize, seed: u64) -> PacingReport {
        self.milestones.sort_by_key(|(m, ..)| *m);
        PacingReport {
            profile: *profile,
            seed_index,
            seed,
            runs: self.runs,
            milestones: self.milestones,
        }
    }
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
    sweep_with_threads(profiles, seeds, threads, simulate_pacing_seed)
}
/// Runs `play` for every (profile, seed index) job on at most `threads`
/// threads; results come back in (profile position, seed index) order.
fn sweep_with_threads<T: Send>(
    profiles: &[PacingProfile],
    seeds: usize,
    threads: usize,
    play: fn(&PacingProfile, usize) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let jobs: Vec<(usize, usize)> = (0..profiles.len())
        .flat_map(|p| (0..seeds).map(move |s| (p, s)))
        .collect();
    let next = AtomicUsize::new(0);
    let threads = threads.clamp(1, jobs.len().max(1));
    let mut done: Vec<(usize, Result<T, String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let job = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(p, s)) = jobs.get(job) else {
                            break;
                        };
                        out.push((job, play(&profiles[p], s)));
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
