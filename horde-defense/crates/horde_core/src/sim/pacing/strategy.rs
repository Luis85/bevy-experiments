//! Strategy pacing (W2-43; design-campaign-defenders 1.13, finding CD12,
//! rulings W-1 and P7): the strategy rows on the progression-tree harness
//! and their goal-3 evaluators.
//!
//! A profile whose plan carries a [`StrategyPlan`] is played by a bot that
//! changes the game only through `Game::apply` and the clock (`Game::tick`):
//! [`simulate_strategy_traced`] returns those steps, and [`replay_strategy`]
//! rebuilds the same game from them. The bot recruits the offered archetype
//! with the higher `defender_power`, guards active outbreaks and releases the
//! guards once contained, sends idle recruits to the next enterable 3-stage
//! front (`DeploySurvivor`, or `CountryOperation` once the Operations layer is
//! unlocked), answers an outbreak that would overrun before its next decision
//! with the main run (`SelectCountry` and `BeginDefense` on the final stage),
//! and waits `camp_ticks` in camp before every run after the first. With the
//! `train_companions` plan option (W4-15, ruling R61; S2b sets it) it also
//! trains and equips each Ready recruit in camp before the tree and gear
//! spending (`BuyRecruitPoint`, `EquipRecruit`, `TrainRecruit`;
//! `super::train_companions`), so a recruit leaves trained. The main
//! run plays like the solo loop: with every option off
//! ([`StrategyPlan::OFF`]) the bot plays today's solo profile exactly
//! (`solo_plan_is_todays_profile`).
//!
//! The evaluators are pure, shaped like the goal-5 and goal-6 ones: `Ok`
//! carries the summary the acceptance prints, `Err` the reasons followed by
//! it. Run counts are 1-based runs completed; a missed value counts as the
//! profile's `max_runs + 1` (a missed liberation's minutes count as
//! `missed`, over the same seeds), and quantiles round down (the lower
//! median).
use super::targets::{run_cell, verdict};
use super::{
    CampSpend, Milestone, PacingDefender, PacingFarm, PacingProfile, PacingReport, PacingSell,
    PacingSpend, RunBook, buy_and_equip, lower_quantile, pacing_input, pacing_start,
    structure_placements, sweep_with_threads, train_companions,
};
use crate::balance::BalanceSettings;
use crate::command::{Command, CommandError, CommandOutcome};
use crate::content::Mode;
use crate::defenders::DefenderId;
use crate::sim::smoke::smoke_buy_gear_with;
use crate::sim::{Game, Input, TICKS_PER_SECOND};
use crate::strategy::{OutbreakOutcome, RecruitStatus};
use crate::world::{Country, world};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The strategy rows (design-campaign-defenders 1.13), in table order: S1,
/// S2a, S2b.
pub const STRATEGY_PROFILE_IDS: [&str; 3] = ["strategy", "neighbours-solo", "neighbours-parallel"];
/// S1, the goal-3 strategy profile (G3.10, G3.12).
pub const STRATEGY_PROFILE_ID: &str = "strategy";
/// Germany's seven 3-stage neighbours, the S2a and S2b route.
pub const DEU_THREE_STAGE_NEIGHBOURS: [&str; 7] = ["AUT", "BEL", "CHE", "CZE", "DNK", "LUX", "NLD"];
/// G3.10: the liberations whose runs and minutes are reported.
pub const GOAL3_LIBERATIONS: [u32; 3] = [2, 4, 10];
/// G3.11: K, the route neighbours the S2 rows free.
pub const GOAL3_NEIGHBOURS_FREED: usize = 3;
/// G3.11: parallel frees K neighbours in at most `num / den` (1/1.5) of
/// the runs solo play needs (medians).
pub const GOAL3_PARALLEL_OVER_SOLO: (usize, usize) = (2, 3);
/// G3.12: the liberations after the strategy unlock over which overruns
/// (outbreak reopens) are counted.
pub const GOAL3_REOPEN_WINDOW: u32 = 10;
/// G3.12: the median reopens must stay below this.
pub const GOAL3_REOPEN_LIMIT: u32 = 5;
/// A run's tick budget, as in the solo loop: 10 minutes, then withdraw.
const RUN_TICKS: u32 = 60 * 600;
/// Expeditions take the next enterable front of this many stages.
const EXPEDITION_FRONT_STAGES: usize = 3;

/// Who runs expeditions besides the main defense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingParallel {
    /// Nobody: the main run alone.
    Off,
    /// Idle recruits take single stages (`DeploySurvivor`).
    Single,
    /// Single stages until the Operations layer is unlocked
    /// (`grand_strategy_unlocked`), then whole countries
    /// (`CountryOperation`).
    Operations,
}
/// How the main run picks its next country after a liberation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingRoute {
    /// The lowest difficulty (the solo loop's choice).
    Easiest,
    /// The fewest stages, then the lowest difficulty.
    FewestStages,
}
/// The strategy layer a pacing plan plays (design-campaign-defenders 1.13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrategyPlan {
    /// Recruit (the offered archetype of higher power) while the roster is
    /// smaller than this.
    pub recruit_up_to: usize,
    pub parallel: PacingParallel,
    /// Guard active outbreaks with idle recruits, release the guards once
    /// contained, and answer with the main run an outbreak that would
    /// overrun before the bot decides again.
    pub guards: bool,
    /// Strategic ticks waited in camp before every run after the first.
    pub camp_ticks: u32,
    pub route: PacingRoute,
    /// The countries the main route and the expeditions may take after the
    /// start; empty means any.
    pub countries: &'static [&'static str],
    /// Seed the strategy RNG from the game seed at every seed index
    /// ([`super::pacing_start`]).
    pub seed_strategy: bool,
}
impl StrategyPlan {
    /// Every option off: the bot plays the solo profile through commands.
    pub const OFF: Self = Self {
        recruit_up_to: 0,
        parallel: PacingParallel::Off,
        guards: false,
        camp_ticks: 0,
        route: PacingRoute::Easiest,
        countries: &[],
        seed_strategy: false,
    };
    /// S1: the whole strategy layer, 30 s of camp before each run.
    pub const S1: Self = Self {
        recruit_up_to: 4,
        parallel: PacingParallel::Operations,
        guards: true,
        camp_ticks: 1_800,
        route: PacingRoute::FewestStages,
        countries: &[],
        seed_strategy: true,
    };
    /// S2a: Germany's 3-stage neighbours, the main run alone.
    pub const NEIGHBOURS_SOLO: Self = Self {
        recruit_up_to: 0,
        parallel: PacingParallel::Off,
        guards: false,
        camp_ticks: 1_800,
        route: PacingRoute::Easiest,
        countries: &DEU_THREE_STAGE_NEIGHBOURS,
        seed_strategy: true,
    };
    /// S2b: S2a with three recruits on expeditions, single stages then
    /// country operations.
    pub const NEIGHBOURS_PARALLEL: Self = Self {
        recruit_up_to: 3,
        parallel: PacingParallel::Operations,
        ..Self::NEIGHBOURS_SOLO
    };
}

/// One step of the strategy bot: a command it applied (whatever the
/// answer) or the clock.
#[derive(Clone, Debug)]
pub enum StrategyStep {
    Command(Command),
    /// One battle tick with this input.
    Tick(Input),
    /// This many ticks with no input (camp time).
    Idle(u32),
}
/// The strategy figures after one run. Totals are cumulative.
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyRun {
    /// 1-based run number.
    pub index: usize,
    /// Camp seconds so far.
    pub camp_s: f32,
    /// Coins settled expeditions brought so far (their summaries).
    pub expedition_coins: u64,
    /// Expeditions settled so far, and the stages they cleared.
    pub expeditions_settled: u32,
    pub expedition_stages: u32,
    /// Outbreaks triggered so far.
    pub outbreaks: u32,
    /// Everything earned so far: the run's earnings (`cumulative_coins`,
    /// which also hold the level grants and milestone coins an expedition
    /// victory settled while no main attempt was live) plus the
    /// expeditions'.
    pub coins_total: u64,
    /// Battle and camp minutes so far.
    pub minutes_total: f32,
    /// Countries ever liberated (`Campaign::liberated_count`).
    pub liberated: u32,
    /// Countries of the plan's route list liberated now.
    pub route_freed: u32,
    /// Overruns so far.
    pub overruns: u32,
    /// Roster size, guards on duty and live expeditions after the run.
    pub roster: usize,
    pub guards: usize,
    pub expeditions: usize,
}
/// An outbreak that overran its country (reopened it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overrun {
    /// The run in progress, or the run whose camp it fell in.
    pub run: usize,
    /// Countries ever liberated when it overran.
    pub liberated: u32,
    pub country: String,
}
/// When the Operations layer (outbreaks, guards, country operations;
/// `grand_strategy_unlocked`) first held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrategyUnlock {
    /// Runs completed.
    pub run: usize,
    /// Countries ever liberated.
    pub liberated: u32,
}
/// A strategy profile at one seed: the harness report plus the strategy
/// figures.
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyReport {
    pub pacing: PacingReport,
    /// One per run of `pacing.runs`, in order.
    pub runs: Vec<StrategyRun>,
    pub overruns: Vec<Overrun>,
    pub strategy_unlock: Option<StrategyUnlock>,
}
/// [`simulate_strategy_traced`]'s result: the report, every step the bot
/// took, and the digest of the game it ended with.
#[derive(Clone, Debug)]
pub struct StrategyTrace {
    pub report: StrategyReport,
    pub steps: Vec<StrategyStep>,
    pub digest: u64,
}

/// The plan options the strategy bot cannot combine with a strategy plan.
fn unsupported(profile: &PacingProfile, plan: &StrategyPlan) -> Option<&'static str> {
    let p = profile.plan;
    if p.equip_drops {
        Some("equip_drops")
    } else if p.sell != PacingSell::Never {
        Some("sell with a strategy plan")
    } else if p.defender != PacingDefender::Survivor {
        Some("defender with a strategy plan")
    } else if p.farm != PacingFarm::None {
        Some("farm with a strategy plan")
    } else if plan.countries.iter().any(|c| world().country(c).is_none()) {
        Some("an unknown route country")
    } else {
        None
    }
}

/// The strategy bot: the game, the plan, the optional trace and what it
/// watches tick by tick.
struct Bot<'t> {
    g: Game,
    plan: StrategyPlan,
    trace: Option<&'t mut Vec<StrategyStep>>,
    /// The run in progress (1-based), for overrun attribution.
    run: usize,
    camp_ticks: u64,
    expedition_coins: u64,
    expeditions_settled: u32,
    expedition_stages: u32,
    /// Settled expeditions already counted.
    settled: BTreeSet<u64>,
    /// Run history records already counted for settlement receipts.
    history_seen: usize,
    seen_overruns: BTreeSet<u64>,
    overruns: Vec<Overrun>,
    unlock: Option<StrategyUnlock>,
    /// The country the main run left to answer an outbreak.
    resume: Option<String>,
}
fn record(trace: &mut Option<&mut Vec<StrategyStep>>, step: StrategyStep) {
    if let Some(t) = trace.as_deref_mut() {
        t.push(step);
    }
}
impl<'t> Bot<'t> {
    fn new(g: Game, plan: StrategyPlan, trace: Option<&'t mut Vec<StrategyStep>>) -> Self {
        Self {
            g,
            plan,
            trace,
            run: 1,
            camp_ticks: 0,
            expedition_coins: 0,
            expeditions_settled: 0,
            expedition_stages: 0,
            settled: BTreeSet::new(),
            history_seen: 0,
            seen_overruns: BTreeSet::new(),
            overruns: Vec::new(),
            unlock: None,
            resume: None,
        }
    }
}
impl Bot<'_> {
    fn act(&mut self, command: Command) -> Result<CommandOutcome, CommandError> {
        record(&mut self.trace, StrategyStep::Command(command.clone()));
        self.g.apply(&command)
    }
    fn must(&mut self, command: Command) -> Result<CommandOutcome, String> {
        self.act(command).map_err(|e| e.to_string())
    }
    fn tick(&mut self, input: Input) {
        record(&mut self.trace, StrategyStep::Tick(input.clone()));
        self.g.tick(input);
        self.watch();
    }
    fn idle(&mut self, ticks: u32) {
        record(&mut self.trace, StrategyStep::Idle(ticks));
        for _ in 0..ticks {
            self.g.tick(Input::default());
            self.watch();
        }
        self.camp_ticks += u64::from(ticks);
    }
    /// Counts outbreaks turning Overrun, tick by tick, so pruning cannot
    /// hide one.
    fn watch(&mut self) {
        for o in &self.g.strategy.outbreaks {
            if o.outcome == OutbreakOutcome::Overrun && self.seen_overruns.insert(o.id) {
                self.overruns.push(Overrun {
                    run: self.run,
                    liberated: self.g.campaign.liberated_count(),
                    country: o.country_id.clone(),
                });
            }
        }
    }
    fn check_unlock(&mut self, runs_completed: usize) {
        if self.unlock.is_none() && self.g.strategy.grand_strategy_unlocked(&self.g.campaign) {
            self.unlock = Some(StrategyUnlock {
                run: runs_completed,
                liberated: self.g.campaign.liberated_count(),
            });
        }
    }
    /// Adds the coins of every newly settled expedition, and to `book`'s
    /// earnings the level grants and milestone coins stamped on the new
    /// expedition run records: an expedition victory credited while no main
    /// attempt is live (the camp idle) is settled by `settle_parallel`, so
    /// its receipt is on the worker's record, never on a main run's report.
    /// While a main attempt is live the main run's finish settles and its
    /// report carries the receipt, and the worker's record holds zero.
    fn count_settled(&mut self, book: &mut RunBook) {
        let history = &self.g.campaign.run_history;
        let receipts = history[self.history_seen.min(history.len())..]
            .iter()
            .filter(|r| r.worker_id.is_some())
            .map(|r| r.level_grant_coins.saturating_add(r.milestone_coins))
            .fold(0, u64::saturating_add);
        self.history_seen = history.len();
        book.earned = book.earned.saturating_add(receipts);
        for b in &self.g.battles {
            if let Some(s) = b.summary.as_ref().filter(|_| b.settled)
                && self.settled.insert(b.id)
            {
                self.expedition_coins += s.coins;
                self.expeditions_settled += 1;
                self.expedition_stages += s.stages_cleared;
            }
        }
    }
    fn live_battle_countries(&self) -> BTreeSet<String> {
        self.g
            .battles
            .iter()
            .filter(|b| !b.settled)
            .map(|b| b.country_id.clone())
            .collect()
    }
    fn route_allows(&self, id: &str) -> bool {
        self.plan.countries.is_empty() || self.plan.countries.contains(&id)
    }
    fn route_order(&self, a: &Country, b: &Country) -> Ordering {
        let easiest = a.difficulty.total_cmp(&b.difficulty).then(a.id.cmp(&b.id));
        match self.plan.route {
            PacingRoute::Easiest => easiest,
            PacingRoute::FewestStages => a.level_count().cmp(&b.level_count()).then(easiest),
        }
    }
    /// The enterable, not liberated countries the route allows.
    fn fronts(&self) -> Vec<&'static Country> {
        let w = &self.g.campaign.world;
        world()
            .countries
            .iter()
            .filter(|c| {
                w.can_enter(&c.id) && !w.cleared.contains(&c.id) && self.route_allows(&c.id)
            })
            .collect()
    }
    /// The main run's next country after a liberation: by route, preferring
    /// one no expedition holds.
    fn next_country(&self) -> Result<String, String> {
        let live = self.live_battle_countries();
        let fronts = self.fronts();
        fronts
            .iter()
            .filter(|c| !live.contains(&c.id))
            .min_by(|a, b| self.route_order(a, b))
            .or_else(|| fronts.iter().min_by(|a, b| self.route_order(a, b)))
            .map(|c| c.id.clone())
            .ok_or_else(|| "No country left to enter".to_owned())
    }
    /// The front an expedition takes next: a 3-stage front by route that is
    /// neither the main run's country (the active one, or the one it left
    /// to answer an outbreak and will resume) nor held by another
    /// expedition.
    fn expedition_target(&self) -> Option<String> {
        let live = self.live_battle_countries();
        let main = self.g.campaign.world.active.as_deref();
        let resume = self.resume.as_deref();
        self.fronts()
            .into_iter()
            .filter(|c| {
                let id = Some(c.id.as_str());
                c.level_count() == EXPEDITION_FRONT_STAGES
                    && !live.contains(&c.id)
                    && main != id
                    && resume != id
            })
            .min_by(|a, b| self.route_order(a, b))
            .map(|c| c.id.clone())
    }
    /// The country the main run left to answer an outbreak, taken, while it
    /// is still an enterable, not liberated front that no expedition holds
    /// (else the main run picks its next country afresh).
    fn take_resume(&mut self) -> Option<String> {
        let live = self.live_battle_countries();
        let w = &self.g.campaign.world;
        self.resume
            .take()
            .filter(|c| w.can_enter(c) && !w.cleared.contains(c) && !live.contains(c))
    }
    /// The offered archetype whose recruit would have the higher power on
    /// its expeditions (the first on a tie), tried on copies of the game.
    /// The bot sends recruits only on expeditions, never on main-run
    /// takeovers, so each trial recruit is rated at the threat its
    /// expedition plays at (`Campaign::expedition_threat_rank`, W4-12,
    /// ruling R57), not the campaign rank.
    fn best_archetype(&self, country: &str) -> Option<String> {
        let offer = self.g.recruit_offer(country).ok()?;
        let next = self.g.strategy.next_recruit_id;
        let id = DefenderId::Recruit(next);
        let guard = crate::progression::catalog().threat_guard_rank;
        let mut best: Option<(&'static str, u16)> = None;
        for choice in offer.archetypes {
            let archetype = choice.archetype.id();
            let mut trial = self.g.clone();
            let recruited = trial.apply(&Command::RecruitAs {
                country: country.to_owned(),
                archetype: archetype.to_owned(),
            });
            let threat = trial
                .strategy
                .roster
                .iter()
                .find(|r| r.id == next)
                .map(|r| trial.campaign.expedition_threat_rank(&r.npc).min(guard));
            let rating = recruited
                .ok()
                .and(threat)
                .and_then(|threat| trial.defender_power(id, threat).ok())
                .map(|p| p.rating);
            if let Some(rating) = rating
                && best.is_none_or(|(_, top)| rating > top)
            {
                best = Some((archetype, rating));
            }
        }
        best.map(|(a, _)| a.to_owned())
    }
    /// Camp before spending: read the expedition notices, release guards
    /// whose outbreak is over, and recruit. Returns the coins recruiting
    /// cost.
    fn camp(&mut self) -> Result<u64, String> {
        let notices = self.g.unacknowledged_outcomes();
        if !notices.notices.is_empty() || notices.folded > 0 {
            self.must(Command::AcknowledgeOutcomes {
                battles: notices.notices.iter().map(|n| n.battle).collect(),
                folded: notices.folded,
            })?;
        }
        if self.plan.guards {
            let s = &self.g.strategy;
            let idle: Vec<u64> = s
                .guards
                .iter()
                .filter(|g| {
                    !s.outbreaks.iter().any(|o| {
                        o.country_id == g.country_id && o.outcome == OutbreakOutcome::Active
                    })
                })
                .map(|g| g.recruit_id)
                .collect();
            for id in idle {
                self.must(Command::ReleaseGuard(id))?;
            }
        }
        let mut spent = 0;
        while self.g.strategy.roster.len() < self.plan.recruit_up_to {
            let Some(country) = self
                .g
                .campaign
                .world
                .cleared
                .iter()
                .find(|c| self.g.recruit_offer(c).is_ok())
                .cloned()
            else {
                break;
            };
            let Some(archetype) = self.best_archetype(&country) else {
                break;
            };
            let before = self.g.campaign.coins;
            self.must(Command::RecruitAs { country, archetype })?;
            spent += before.saturating_sub(self.g.campaign.coins);
        }
        Ok(spent)
    }
    fn learn(&mut self) -> Result<Vec<String>, String> {
        let mut learned = Vec::new();
        while let Some(id) = self.g.campaign.recommended_node(Mode::Personal) {
            if self.g.campaign.can_purchase_node(&id).is_err() {
                break;
            }
            self.must(Command::BuySkillNode(id.clone()))?;
            learned.push(id);
        }
        Ok(learned)
    }
    /// The `train_companions` step: trains every Ready recruit before it
    /// leaves. Returns the coins it spent.
    fn train(&mut self) -> u64 {
        let (g, trace) = (&mut self.g, &mut self.trace);
        train_companions(g, |command| {
            record(trace, StrategyStep::Command(command.clone()));
        })
    }
    fn gear(&mut self) -> Result<(), String> {
        let (g, trace) = (&mut self.g, &mut self.trace);
        smoke_buy_gear_with(g, Mode::Personal, |g, definition| {
            buy_and_equip(g, definition, |command| {
                record(trace, StrategyStep::Command(command.clone()));
            })
        })
    }
    /// The profile's camp spending, through commands, in the solo order.
    fn spend(&mut self, spend: PacingSpend, spent_recruit: u64) -> Result<CampSpend, String> {
        let before_tree = self.g.campaign.coins;
        let (learned, spent_tree, spent_gear) = match spend {
            PacingSpend::Recommended => {
                let learned = self.learn()?;
                let before_gear = self.g.campaign.coins;
                self.gear()?;
                (
                    learned,
                    before_tree.saturating_sub(before_gear),
                    before_gear.saturating_sub(self.g.campaign.coins),
                )
            }
            PacingSpend::GearFirst => {
                self.gear()?;
                let before_learn = self.g.campaign.coins;
                let learned = self.learn()?;
                (
                    learned,
                    before_learn.saturating_sub(self.g.campaign.coins),
                    before_tree.saturating_sub(before_learn),
                )
            }
        };
        Ok(CampSpend {
            learned,
            spent_tree,
            spent_gear,
            spent_training: 0,
            spent_recruit,
        })
    }
    /// Camp after spending: guard the active outbreaks, most urgent first,
    /// then send idle recruits on expeditions.
    fn dispatch(&mut self) -> Result<(), String> {
        if self.plan.guards {
            let mut urgent: Vec<(u64, String)> = self
                .g
                .strategy
                .outbreaks
                .iter()
                .filter(|o| o.outcome == OutbreakOutcome::Active)
                .map(|o| (o.deadline_tick, o.country_id.clone()))
                .collect();
            urgent.sort();
            for (_, country) in urgent {
                while let Some(&id) = self
                    .g
                    .strategy
                    .eligible_guards(&self.g.campaign, &country)
                    .first()
                {
                    if self
                        .act(Command::AssignGuard {
                            id,
                            country: country.clone(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        if self.plan.parallel == PacingParallel::Off {
            return Ok(());
        }
        while self.g.strategy.parallel_unlocked(&self.g.campaign) {
            let ready = self
                .g
                .strategy
                .roster
                .iter()
                .filter(|r| r.status == RecruitStatus::Ready)
                .map(|r| r.id)
                .min();
            let (Some(id), Some(country)) = (ready, self.expedition_target()) else {
                break;
            };
            let operation = self.plan.parallel == PacingParallel::Operations
                && self.g.strategy.grand_strategy_unlocked(&self.g.campaign);
            let command = if operation {
                Command::CountryOperation { id, country }
            } else {
                let stage = self.g.campaign.world.completed(&country);
                Command::DeploySurvivor { id, country, stage }
            };
            if self.act(command).is_err() {
                break;
            }
        }
        Ok(())
    }
    /// The active outbreak that would overrun within `budget` ticks (the
    /// most urgent), as its country and final stage.
    fn urgent_outbreak(&self, budget: u64) -> Option<(String, usize)> {
        self.g
            .strategy
            .outbreaks
            .iter()
            .filter(|o| o.outcome == OutbreakOutcome::Active)
            .filter_map(|o| self.g.outbreak_plan(&o.country_id))
            .filter(|p| p.ticks_left < budget)
            .min_by(|a, b| {
                a.ticks_left
                    .cmp(&b.ticks_left)
                    .then(a.country_id.cmp(&b.country_id))
            })
            .map(|p| (p.country_id, p.stage))
    }
    /// Deploys run `index + 1`: the start country first, then an urgent
    /// outbreak response, else the journey's next defense, choosing the
    /// next country after a liberation.
    fn deploy(&mut self, index: usize, country: &str, budget: u64) -> Result<(), String> {
        if index == 0 {
            self.must(Command::ChooseStart(country.to_owned()))?;
            return Ok(());
        }
        if self.plan.guards
            && let Some((outbreak, stage)) = self.urgent_outbreak(budget)
        {
            let w = &self.g.campaign.world;
            let active = w
                .active
                .clone()
                .filter(|a| !w.cleared.contains(a) && *a != outbreak);
            if self.resume.is_none() {
                self.resume = active;
            }
            self.must(Command::SelectCountry(outbreak))?;
            self.must(Command::BeginDefense { stage })?;
            return Ok(());
        }
        if self.must(Command::ContinueJourney)? == CommandOutcome::CountryLiberated {
            let next = match self.take_resume() {
                Some(c) => c,
                None => self.next_country()?,
            };
            self.must(Command::SelectCountry(next))?;
            let stage = self.g.campaign.world.next_stage();
            self.must(Command::BeginDefense { stage })?;
        }
        Ok(())
    }
}

/// Plays `profile` (a strategy plan) at seed index `seed_index`, recording
/// every step into `trace` when given. Returns the report and the game.
fn play(
    profile: &PacingProfile,
    seed_index: usize,
    trace: Option<&mut Vec<StrategyStep>>,
) -> Result<(StrategyReport, Game), String> {
    let plan = profile
        .plan
        .strategy
        .ok_or_else(|| format!("{} has no strategy plan", profile.id))?;
    if let Some(option) = unsupported(profile, &plan) {
        return Err(format!(
            "Pacing plan option {option} is not implemented ({})",
            profile.id
        ));
    }
    let g = pacing_start(profile, seed_index);
    let seed = g.seed;
    let mut bot = Bot::new(g, plan, trace);
    bot.must(Command::ChooseSurvivor(crate::survivors::SurvivorId::Rook))?;
    if bot.g.campaign.balance_settings.preset != profile.difficulty {
        bot.must(Command::SetBalance(BalanceSettings::for_preset(
            profile.difficulty,
        )))?;
    }
    let mut book = RunBook::new();
    let mut runs = Vec::new();
    let mut last_run_ticks = u64::from(RUN_TICKS);
    for index in 0..profile.max_runs {
        bot.run = index + 1;
        if index > 0 && plan.camp_ticks > 0 {
            bot.must(Command::SetPaused(false))?;
            bot.idle(plan.camp_ticks);
        }
        bot.count_settled(&mut book);
        let spent_recruit = bot.camp()?;
        // train_companions (W4-15): the recruits train before the tree and
        // gear take the purse, so a recruit leaves trained.
        let spent_training = if profile.plan.train_companions {
            bot.train()
        } else {
            0
        };
        let mut spend = bot.spend(profile.spend, spent_recruit)?;
        spend.spent_training = spent_training;
        bot.check_unlock(index);
        bot.dispatch()?;
        book.camp(&bot.g, index);
        bot.deploy(
            index,
            profile.country,
            last_run_ticks + u64::from(plan.camp_ticks),
        )?;
        for (kind, lane, column) in structure_placements(&bot.g) {
            // Rejections (supply, an occupied cell) just skip the cell.
            let _ = bot.act(Command::Place { kind, lane, column });
        }
        bot.must(Command::SetPaused(false))?;
        let mut held = [15.0, 5.5];
        for _ in 0..RUN_TICKS {
            if bot.g.attempt.is_none() {
                break;
            }
            let input = pacing_input(&bot.g, profile.aim, &mut held);
            bot.tick(input);
        }
        if bot.g.attempt.is_some() {
            bot.must(Command::Withdraw)?;
        }
        bot.count_settled(&mut book);
        book.record(&bot.g, index, seed, spend)?;
        if let Some(r) = &bot.g.last_result {
            last_run_ticks = (r.elapsed * TICKS_PER_SECOND as f32).round() as u64;
        }
        bot.check_unlock(index + 1);
        let w = &bot.g.campaign.world;
        let camp_s = bot.camp_ticks as f32 / TICKS_PER_SECOND as f32;
        runs.push(StrategyRun {
            index: index + 1,
            camp_s,
            expedition_coins: bot.expedition_coins,
            expeditions_settled: bot.expeditions_settled,
            expedition_stages: bot.expedition_stages,
            outbreaks: bot.g.strategy.next_outbreak_id.saturating_sub(1) as u32,
            coins_total: book.earned + bot.expedition_coins,
            minutes_total: book.minutes + camp_s / 60.0,
            liberated: bot.g.campaign.liberated_count(),
            route_freed: plan
                .countries
                .iter()
                .filter(|c| w.cleared.iter().any(|id| id == *c))
                .count() as u32,
            overruns: bot.overruns.len() as u32,
            roster: bot.g.strategy.roster.len(),
            guards: bot.g.strategy.guards.len(),
            expeditions: bot.g.battles.iter().filter(|b| !b.settled).count(),
        });
        if book.stopped(profile.stop_after) {
            break;
        }
    }
    let report = StrategyReport {
        pacing: book.report(profile, seed_index, seed),
        runs,
        overruns: bot.overruns,
        strategy_unlock: bot.unlock,
    };
    Ok((report, bot.g))
}
/// Plays the strategy `profile` at its own seed (seed index 0).
pub fn simulate_strategy(profile: &PacingProfile) -> Result<StrategyReport, String> {
    simulate_strategy_seed(profile, 0)
}
/// Plays the strategy `profile` at seed index `seed_index` from
/// [`super::pacing_start`].
pub fn simulate_strategy_seed(
    profile: &PacingProfile,
    seed_index: usize,
) -> Result<StrategyReport, String> {
    play(profile, seed_index, None).map(|(report, _)| report)
}
/// [`simulate_strategy_seed`] that also returns every step the bot took
/// and the state digest of the game it ended with.
pub fn simulate_strategy_traced(
    profile: &PacingProfile,
    seed_index: usize,
) -> Result<StrategyTrace, String> {
    let mut steps = Vec::new();
    let (report, g) = play(profile, seed_index, Some(&mut steps))?;
    Ok(StrategyTrace {
        report,
        steps,
        digest: g.state_digest(),
    })
}
/// Rebuilds a game from [`super::pacing_start`] and `steps` alone: every
/// command through `Game::apply` (its answer ignored, as the bot's was
/// recorded either way) and the clock.
pub fn replay_strategy(profile: &PacingProfile, seed_index: usize, steps: &[StrategyStep]) -> Game {
    let mut g = pacing_start(profile, seed_index);
    for step in steps {
        match step {
            StrategyStep::Command(command) => {
                let _ = g.apply(command);
            }
            StrategyStep::Tick(input) => g.tick(input.clone()),
            StrategyStep::Idle(ticks) => {
                for _ in 0..*ticks {
                    g.tick(Input::default());
                }
            }
        }
    }
    g
}
/// Plays every strategy profile at seed indices `0..seeds` on all cores,
/// ordered by (profile position, seed index).
pub fn strategy_sweep(
    profiles: &[PacingProfile],
    seeds: usize,
) -> Result<Vec<StrategyReport>, String> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    sweep_with_threads(profiles, seeds, threads, simulate_strategy_seed)
}

/// Runs from the start country's liberation (the first) until `k` countries
/// of the route list are liberated after a run; None when either never
/// happened.
pub fn runs_to_free_neighbours(report: &StrategyReport, k: usize) -> Option<usize> {
    let (start, _) = report.pacing.milestone(Milestone::Liberation(1))?;
    let freed = report
        .runs
        .iter()
        .find(|r| r.route_freed as usize >= k)?
        .index;
    Some(freed.saturating_sub(start))
}
/// Overruns while the liberation count is in the `n` liberations from the
/// strategy unlock on; None when the unlock never came or the count never
/// reached its end.
pub fn reopens_over_liberations(report: &StrategyReport, n: u32) -> Option<u32> {
    let unlock = report.strategy_unlock?;
    let end = unlock.liberated.saturating_add(n);
    if !report.runs.iter().any(|r| r.liberated >= end) {
        return None;
    }
    Some(
        report
            .overruns
            .iter()
            .filter(|o| (unlock.liberated..end).contains(&o.liberated))
            .count() as u32,
    )
}
/// Coins earned per minute of battle and camp at the report's last run.
fn coins_per_minute(report: &StrategyReport) -> f64 {
    report.runs.last().map_or(0.0, |r| {
        if r.minutes_total > 0.0 {
            r.coins_total as f64 / f64::from(r.minutes_total)
        } else {
            0.0
        }
    })
}
fn max_runs(reports: &[StrategyReport]) -> usize {
    reports
        .iter()
        .map(|r| r.pacing.profile.max_runs)
        .max()
        .unwrap_or(0)
}
/// The lower median and p90 of `values`, or None for none.
fn median_p90<T: Copy + PartialOrd>(mut values: Vec<T>) -> Option<(T, T)> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    Some((
        lower_quantile(&values, 1, 2),
        lower_quantile(&values, 9, 10),
    ))
}
/// A run count's median and p90 over the reports, a missed one counting as
/// `max_runs + 1`, and how many reached it.
fn run_stats(
    reports: &[StrategyReport],
    run: impl Fn(&StrategyReport) -> Option<usize>,
) -> Option<(usize, usize, usize)> {
    let reached = reports.iter().filter(|r| run(r).is_some()).count();
    let (median, p90) = median_p90(
        reports
            .iter()
            .map(|r| run(r).unwrap_or(r.pacing.profile.max_runs + 1))
            .collect(),
    )?;
    Some((median, p90, reached))
}
/// The battle and camp minutes when liberation `n` was first reached.
fn liberation_minutes(report: &StrategyReport, n: u32) -> Option<f32> {
    let (run, _) = report.pacing.milestone(Milestone::Liberation(n))?;
    report
        .runs
        .iter()
        .find(|r| r.index == run)
        .map(|r| r.minutes_total)
}

/// Liberation `n`'s minutes as `median [p90]` cells over every report: a
/// seed that missed it counts as missed (above every reached value), as the
/// run cells count it as `max_runs + 1`, so both cells describe the same
/// seeds. A quantile that falls on a miss reads `missed`.
fn liberation_minutes_cells(reports: &[StrategyReport], n: u32) -> Option<(String, String)> {
    let cell = |m: f32| {
        if m.is_finite() {
            format!("{m:.0}")
        } else {
            "missed".to_owned()
        }
    };
    median_p90(
        reports
            .iter()
            .map(|r| liberation_minutes(r, n).unwrap_or(f32::INFINITY))
            .collect(),
    )
    .map(|(m, p)| (cell(m), cell(p)))
}

/// G3.10: the runs and minutes to liberations 2, 4 and 10 and the coins per
/// minute are reported. Fails when a liberation's median run is missing
/// (most seeds never reached it), since it cannot be reported.
pub fn liberations_and_coins_per_minute(reports: &[StrategyReport]) -> Result<String, String> {
    if reports.is_empty() {
        return Err("no strategy reports to evaluate".to_owned());
    }
    let max = max_runs(reports);
    let seeds = reports.len();
    let mut problems = Vec::new();
    let mut parts = Vec::new();
    for n in GOAL3_LIBERATIONS {
        let m = Milestone::Liberation(n);
        let (median, p90, reached) =
            run_stats(reports, |r| r.pacing.milestone(m).map(|(run, _)| run))
                .expect("reports are not empty");
        let minutes = liberation_minutes_cells(reports, n)
            .map_or_else(|| "n/a".to_owned(), |(m, p)| format!("{m} [p90 {p}]"));
        parts.push(format!(
            "liberation {n}: runs {} [p90 {}] {reached}/{seeds}, minutes {minutes}",
            run_cell(median, max),
            run_cell(p90, max)
        ));
        if median > max {
            problems.push(format!(
                "liberation {n}: the median run is missing ({reached}/{seeds} seeds reached it)"
            ));
        }
    }
    let (rate, rate_p90) =
        median_p90(reports.iter().map(coins_per_minute).collect()).expect("reports are not empty");
    parts.push(format!("coins/min {rate:.1} [p90 {rate_p90:.1}]"));
    verdict(problems, parts.join("; "))
}
/// G3.11: with expeditions, freeing K route neighbours after the start
/// takes at most 2/3 of the solo median runs.
pub fn parallel_frees_neighbours(
    solo: &[StrategyReport],
    parallel: &[StrategyReport],
) -> Result<String, String> {
    let k = GOAL3_NEIGHBOURS_FREED;
    let free = |r: &StrategyReport| runs_to_free_neighbours(r, k);
    let (Some(s), Some(p)) = (run_stats(solo, free), run_stats(parallel, free)) else {
        return Err("no strategy reports to evaluate".to_owned());
    };
    let (solo_max, parallel_max) = (max_runs(solo), max_runs(parallel));
    let summary = format!(
        "runs to free {k} neighbours after the start: parallel {} [p90 {}] {}/{}, solo {} [p90 {}] {}/{}",
        run_cell(p.0, parallel_max),
        run_cell(p.1, parallel_max),
        p.2,
        parallel.len(),
        run_cell(s.0, solo_max),
        run_cell(s.1, solo_max),
        s.2,
        solo.len()
    );
    let (num, den) = GOAL3_PARALLEL_OVER_SOLO;
    let mut problems = Vec::new();
    if s.0 > solo_max {
        problems.push(format!(
            "solo play never frees {k} neighbours in most seeds, so there is no ratio"
        ));
    } else if p.0 * den > s.0 * num {
        problems.push(format!(
            "parallel needs {:.2} of the solo runs, above {num}/{den}",
            p.0 as f64 / s.0.max(1) as f64
        ));
    }
    verdict(problems, summary)
}
/// G3.12: fewer than 5 outbreak reopens (median over seeds) in the 10
/// liberations after the strategy unlock. A seed whose window never
/// completes counts as the limit.
pub fn outbreak_reopens(reports: &[StrategyReport]) -> Result<String, String> {
    let complete = reports
        .iter()
        .filter(|r| reopens_over_liberations(r, GOAL3_REOPEN_WINDOW).is_some())
        .count();
    let Some((median, p90)) = median_p90(
        reports
            .iter()
            .map(|r| reopens_over_liberations(r, GOAL3_REOPEN_WINDOW).unwrap_or(GOAL3_REOPEN_LIMIT))
            .collect(),
    ) else {
        return Err("no strategy reports to evaluate".to_owned());
    };
    let summary = format!(
        "reopens over {GOAL3_REOPEN_WINDOW} liberations after the strategy unlock: median {median} [p90 {p90}], {complete}/{} seeds completed the window",
        reports.len()
    );
    let mut problems = Vec::new();
    if median >= GOAL3_REOPEN_LIMIT {
        problems.push(format!("median {median} is not below {GOAL3_REOPEN_LIMIT}"));
    }
    verdict(problems, summary)
}

/// The goal-3 table of a strategy sweep: one column per profile (reports of
/// one profile are adjacent), cells `median [p90]` over seeds, then the
/// parallel-over-solo line when both neighbour rows are present.
pub fn strategy_table(reports: &[StrategyReport]) -> String {
    let mut groups: Vec<&[StrategyReport]> = Vec::new();
    let mut start = 0;
    for i in 1..=reports.len() {
        if i == reports.len() || reports[i].pacing.profile.id != reports[start].pacing.profile.id {
            if i > start {
                groups.push(&reports[start..i]);
            }
            start = i;
        }
    }
    let mut s = String::from("| Strategy (median [p90], runs reached/seeds) |");
    for g in &groups {
        let _ = write!(s, " {} |", g[0].pacing.profile.name);
    }
    s.push('\n');
    let _ = writeln!(s, "|---|{}", "---|".repeat(groups.len()));
    let mut line = |label: &str, cell: &dyn Fn(&[StrategyReport]) -> String| {
        let _ = write!(s, "| {label} |");
        for g in &groups {
            let _ = write!(s, " {} |", cell(g));
        }
        s.push('\n');
    };
    let runs_cell = |g: &[StrategyReport], run: &dyn Fn(&StrategyReport) -> Option<usize>| {
        let max = max_runs(g);
        run_stats(g, run).map_or_else(
            || "n/a".to_owned(),
            |(m, p, reached)| {
                format!(
                    "{} [{}] {reached}/{}",
                    run_cell(m, max),
                    run_cell(p, max),
                    g.len()
                )
            },
        )
    };
    let float_cell = |values: Vec<f64>, digits: usize| {
        median_p90(values).map_or_else(
            || "n/a".to_owned(),
            |(m, p)| format!("{m:.digits$} [{p:.digits$}]"),
        )
    };
    for n in GOAL3_LIBERATIONS {
        let m = Milestone::Liberation(n);
        line(&format!("Liberation {n} (runs)"), &|g| {
            runs_cell(g, &|r| r.pacing.milestone(m).map(|(run, _)| run))
        });
        line(&format!("Liberation {n} (minutes)"), &|g| {
            liberation_minutes_cells(g, n)
                .map_or_else(|| "n/a".to_owned(), |(m, p)| format!("{m} [{p}]"))
        });
    }
    line("Coins per minute", &|g| {
        float_cell(g.iter().map(coins_per_minute).collect(), 1)
    });
    let last = |r: &StrategyReport, f: fn(&StrategyRun) -> f64| r.runs.last().map_or(0.0, f);
    line("Coins earned (with expeditions)", &|g| {
        float_cell(
            g.iter()
                .map(|r| last(r, |x| x.coins_total as f64))
                .collect(),
            0,
        )
    });
    line("Expedition coins", &|g| {
        float_cell(
            g.iter()
                .map(|r| last(r, |x| x.expedition_coins as f64))
                .collect(),
            0,
        )
    });
    line("Expeditions settled", &|g| {
        float_cell(
            g.iter()
                .map(|r| last(r, |x| f64::from(x.expeditions_settled)))
                .collect(),
            0,
        )
    });
    line("Expedition stages cleared", &|g| {
        float_cell(
            g.iter()
                .map(|r| last(r, |x| f64::from(x.expedition_stages)))
                .collect(),
            0,
        )
    });
    line("Outbreaks / overruns (whole profile)", &|g| {
        let outbreaks = float_cell(
            g.iter()
                .map(|r| last(r, |x| f64::from(x.outbreaks)))
                .collect(),
            0,
        );
        let overruns = float_cell(
            g.iter()
                .map(|r| last(r, |x| f64::from(x.overruns)))
                .collect(),
            0,
        );
        format!("{outbreaks} / {overruns}")
    });
    line("Battle minutes", &|g| {
        float_cell(
            g.iter()
                .map(|r| {
                    r.pacing
                        .runs
                        .iter()
                        .map(|x| f64::from(x.elapsed))
                        .sum::<f64>()
                        / 60.0
                })
                .collect(),
            0,
        )
    });
    line("Camp minutes", &|g| {
        float_cell(
            g.iter()
                .map(|r| last(r, |x| f64::from(x.camp_s) / 60.0))
                .collect(),
            0,
        )
    });
    line("Strategy unlock (runs)", &|g| {
        runs_cell(g, &|r| r.strategy_unlock.map(|u| u.run))
    });
    let k = GOAL3_NEIGHBOURS_FREED;
    line(
        &format!("Runs to free {k} neighbours (after the start)"),
        &|g| {
            if g[0]
                .pacing
                .profile
                .plan
                .strategy
                .is_some_and(|p| p.countries.is_empty())
            {
                "n/a".to_owned()
            } else {
                runs_cell(g, &|r| runs_to_free_neighbours(r, k))
            }
        },
    );
    line(
        &format!("Reopens over {GOAL3_REOPEN_WINDOW} liberations after the unlock"),
        &|g| {
            let complete = g
                .iter()
                .filter(|r| reopens_over_liberations(r, GOAL3_REOPEN_WINDOW).is_some())
                .count();
            // A missing window counts as the limit, as in the acceptance;
            // a row where no seed completes one has nothing to report.
            median_p90(
                g.iter()
                    .map(|r| {
                        reopens_over_liberations(r, GOAL3_REOPEN_WINDOW)
                            .unwrap_or(GOAL3_REOPEN_LIMIT)
                    })
                    .collect(),
            )
            .filter(|_| complete > 0)
            .map_or_else(
                || format!("n/a 0/{}", g.len()),
                |(m, p)| format!("{m} [{p}] {complete}/{}", g.len()),
            )
        },
    );
    let group = |id: &str| {
        groups
            .iter()
            .find(|g| g[0].pacing.profile.id == id)
            .copied()
    };
    if let (Some(solo), Some(parallel)) = (
        group(STRATEGY_PROFILE_IDS[1]),
        group(STRATEGY_PROFILE_IDS[2]),
    ) {
        let free = |r: &StrategyReport| runs_to_free_neighbours(r, k);
        if let (Some(p), Some(so)) = (run_stats(parallel, free), run_stats(solo, free)) {
            let (num, den) = GOAL3_PARALLEL_OVER_SOLO;
            let _ = writeln!(
                s,
                "\nParallel / solo runs to free {k} neighbours (medians): {} / {} = {:.2} (target <= {:.2})",
                run_cell(p.0, max_runs(parallel)),
                run_cell(so.0, max_runs(solo)),
                p.0 as f64 / so.0.max(1) as f64,
                num as f64 / den as f64
            );
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::ParallelBattle;

    /// S1's bot on a campaign that started in and liberated Germany, so its
    /// neighbours are open fronts.
    fn bot_after_germany() -> Bot<'static> {
        let mut g = Game::default();
        g.campaign.choose_start_country("DEU").unwrap();
        let levels = world().country("DEU").unwrap().level_count();
        g.campaign.world.country_levels.insert("DEU".into(), levels);
        g.campaign.world.cleared.push("DEU".into());
        Bot::new(g, StrategyPlan::S1, None)
    }
    fn live_expedition(country: &str) -> ParallelBattle {
        ParallelBattle {
            id: 1,
            recruit_id: 1,
            country_id: country.to_owned(),
            stage: 0,
            game: Box::new(Game::default()),
            synced_coins: 0,
            synced_item_id: 0,
            synced_item_index: 0,
            settled: false,
            loadout: [0; 4],
            country_operation: false,
            summary: None,
            pending_auto_sell: vec![],
        }
    }

    #[test]
    fn expeditions_leave_the_interrupted_main_country_alone() {
        let mut bot = bot_after_germany();
        let first = bot.expedition_target().expect("a 3-stage front");
        // The main run left `first` to answer an outbreak elsewhere: the
        // run will resume there, so no expedition takes it meanwhile.
        bot.g.campaign.world.active = Some("FRA".into());
        bot.resume = Some(first.clone());
        let next = bot.expedition_target().expect("another 3-stage front");
        assert_ne!(next, first, "the expedition takes the resume country");
    }

    #[test]
    fn the_main_run_does_not_resume_a_country_an_expedition_holds() {
        let mut bot = bot_after_germany();
        let held = bot.expedition_target().expect("a 3-stage front");
        bot.resume = Some(held.clone());
        bot.g.battles.push(live_expedition(&held));
        assert_eq!(bot.take_resume(), None, "{held} is an expedition's front");
        assert_eq!(bot.resume, None, "taken");
        // A front no expedition holds is resumed.
        bot.g.battles[0].settled = true;
        bot.resume = Some(held.clone());
        assert_eq!(bot.take_resume(), Some(held));
    }

    #[test]
    fn idle_pass_expedition_rewards_count_in_the_earnings() {
        // A campaign one victory short of rank 5 with a recruit on an
        // expedition to the start country's first stage; no main attempt.
        let mut g = Game::default();
        g.campaign.legacy_features = true;
        g.campaign.balance_settings = BalanceSettings::legacy();
        // Four liberated countries unlock parallel expeditions.
        let country = world().countries[0].id.clone();
        g.campaign.choose_start_country(&country).unwrap();
        for c in world().countries.iter().take(4) {
            g.campaign
                .world
                .country_levels
                .insert(c.id.clone(), c.level_count());
            g.campaign.world.cleared.push(c.id.clone());
        }
        g.campaign.coins = 10_000;
        g.campaign.attempts = 1;
        g.campaign.victories = 4;
        let recruit = g.strategy.recruit(&mut g.campaign, &country).unwrap();
        let b = g.deploy_survivor(recruit, &country, 0).unwrap();
        let mut bot = Bot::new(g, StrategyPlan::S1, None);
        bot.must(Command::SetPaused(false)).unwrap();
        // The expedition wins its stage on the next tick of the idle pass.
        let w = &mut bot.g.battles.iter_mut().find(|x| x.id == b).unwrap().game;
        let mut a = w.attempt.take().unwrap();
        w.spawn_enemy(&mut a, crate::content::EnemyKind::Shambler, 5, true);
        a.enemies.last_mut().unwrap().x = 20.0;
        a.boss_spawned = true;
        a.enemies[0].injury.unconscious = true;
        w.attempt = Some(a);
        bot.idle(1);
        assert!(bot.g.attempt.is_none());
        assert_eq!(bot.g.campaign.progression_rank(), 5);
        let record = bot
            .g
            .campaign
            .run_history
            .iter()
            .find(|r| r.worker_id == Some(b))
            .expect("the expedition's run record")
            .clone();
        let paid = record.level_grant_coins + record.milestone_coins;
        assert!(record.milestone_coins >= 500, "{record:?}");
        let mut book = RunBook::new();
        bot.count_settled(&mut book);
        assert_eq!(book.earned, paid, "the settlement receipt is earned");
        // Counted once.
        bot.idle(3);
        bot.count_settled(&mut book);
        assert_eq!(book.earned, paid);
    }

    /// Closing (W4-12, R57, R62): the bot's recruits only go on
    /// expeditions, which play at the recruit's own threat
    /// (`Campaign::expedition_threat_rank`), so it rates each offered
    /// archetype there, not at the campaign rank a fresh recruit never
    /// meets. Deep in a campaign (rank 15) the campaign-rank ratings
    /// collapse to the clamp 1 and tie, and the first offer won by default
    /// even where the second is the stronger expedition recruit.
    #[test]
    fn best_archetype_rates_each_recruit_at_its_expedition_rank() {
        let mut bot = bot_after_germany();
        bot.g
            .campaign
            .skill_nodes
            .insert(crate::sim::defense::feature_node(
                crate::skill_catalog::CampaignFeature::Recruitment,
            ));
        bot.g.campaign.coins = 1_000_000;
        bot.g.campaign.victories = 15;
        let campaign_rank = bot.g.campaign.progression_rank();
        let mut second_wins = 0;
        for next in 1..=12 {
            bot.g.strategy.next_recruit_id = next;
            let rated = |archetype: &str| {
                let mut trial = bot.g.clone();
                trial
                    .apply(&Command::RecruitAs {
                        country: "DEU".into(),
                        archetype: archetype.to_owned(),
                    })
                    .unwrap();
                let roster = &trial.strategy.roster;
                let npc = &roster.iter().find(|r| r.id == next).unwrap().npc;
                let rank = trial.campaign.expedition_threat_rank(npc);
                let power = |rank| {
                    trial
                        .defender_power(DefenderId::Recruit(next), rank)
                        .unwrap()
                        .rating
                };
                (rank, power(rank), power(campaign_rank))
            };
            let offer = bot.g.recruit_offer("DEU").unwrap();
            let mut expected: Option<(&str, u16)> = None;
            for (index, choice) in offer.archetypes.iter().enumerate() {
                let archetype = choice.archetype.id();
                let (rank, rating, at_campaign) = rated(archetype);
                assert!(rank < campaign_rank, "{archetype} plays at rank {rank}");
                assert_eq!(at_campaign, 1, "{archetype} at rank {campaign_rank}");
                if expected.is_none_or(|(_, top)| rating > top) {
                    expected = Some((archetype, rating));
                    second_wins += index;
                }
            }
            assert_eq!(
                bot.best_archetype("DEU").as_deref(),
                expected.map(|(a, _)| a),
                "recruit {next}"
            );
        }
        assert!(second_wins > 0, "no offer's second archetype is stronger");
    }
}
