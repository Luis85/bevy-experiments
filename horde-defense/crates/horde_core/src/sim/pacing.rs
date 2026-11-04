//! Headless pacing measurement: a scripted player walks the fresh-profile
//! loop (survivor → starting country → defenses → camp → continue) and the
//! report records when each early milestone first happens. Un-gated so the
//! `pacing` example and the regression tests share one implementation.
use super::smoke::smoke_buy_gear;
use super::*;
use crate::skill_catalog::{CampaignFeature, skills};
use std::collections::BTreeMap;

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
#[derive(Clone, Debug)]
pub struct PacingProfile {
    pub name: &'static str,
    pub seed: u64,
    pub country: &'static str,
    pub aim: PacingAim,
    pub spend: PacingSpend,
    pub max_runs: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PacingRun {
    pub index: usize,
    pub country: String,
    pub stage: usize,
    pub outcome: Outcome,
    pub elapsed: f32,
    pub coins: u64,
    /// Nodes learned in camp before this run.
    pub learned: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Milestone {
    FirstVictory,
    /// Every node of this prerequisite depth (1-based) is affordable from
    /// the coins earned so far.
    TierAffordable(usize),
    /// The scripted player owns a node of this depth.
    TierFirstNode(usize),
    FirstLiberation,
    Feature(CampaignFeature),
}
#[derive(Clone, Debug)]
pub struct PacingReport {
    pub profile: PacingProfile,
    pub runs: Vec<PacingRun>,
    /// Milestone, 1-based run count when it first held, battle minutes so far.
    pub milestones: Vec<(Milestone, usize, f32)>,
}
impl PacingReport {
    pub fn milestone(&self, m: Milestone) -> Option<(usize, f32)> {
        self.milestones
            .iter()
            .find(|(k, ..)| *k == m)
            .map(|(_, run, minutes)| (*run, *minutes))
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
            if a.tick % 30 == 0 {
                if let Some(e) = a
                    .enemies
                    .iter()
                    .filter(|e| e.in_view())
                    .min_by(|e, f| e.x.total_cmp(&f.x).then(e.id.cmp(&f.id)))
                {
                    *held = [e.x, e.y()];
                }
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
/// Plays `profile` from a fresh profile and records milestones.
pub fn simulate_pacing(profile: &PacingProfile) -> Result<PacingReport, String> {
    let mut g = Game {
        seed: profile.seed,
        ..Game::default()
    };
    g.campaign
        .choose_survivor(crate::survivors::SurvivorId::Rook)?;
    g.campaign.choose_start_country(profile.country)?;
    let depths = node_depths();
    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut runs = Vec::new();
    let mut milestones: Vec<(Milestone, usize, f32)> = Vec::new();
    let mut earned = 0u64;
    let mut minutes = 0.0f32;
    let mut reached = |m: Milestone, run: usize, minutes: f32| {
        if !milestones.iter().any(|(k, ..)| *k == m) {
            milestones.push((m, run, minutes));
        }
    };
    for index in 0..profile.max_runs {
        let learned = match profile.spend {
            PacingSpend::Recommended => {
                let learned = learn_recommended(&mut g)?;
                smoke_buy_gear(&mut g, Mode::Personal)?;
                learned
            }
            PacingSpend::GearFirst => {
                smoke_buy_gear(&mut g, Mode::Personal)?;
                learn_recommended(&mut g)?
            }
        };
        for node in &skills().nodes {
            let d = depths[&node.id];
            if d > 0 && g.campaign.has_node(&node.id) {
                reached(Milestone::TierFirstNode(d), index, minutes);
            }
        }
        for f in CampaignFeature::ALL {
            if g.campaign.has_feature(f) {
                reached(Milestone::Feature(f), index, minutes);
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
                        .min_by(|a, b| {
                            a.difficulty
                                .total_cmp(&b.difficulty)
                                .then(a.id.cmp(&b.id))
                        })
                        .ok_or("No country left to enter")?
                        .id
                        .clone();
                    g.campaign.enter_country(&next)?;
                    let stage = g.campaign.world.next_stage();
                    g.deploy_country_stage(stage, Mode::Personal)?;
                }
            }
        }
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
        earned += r.coins;
        minutes += r.elapsed / 60.0;
        let run = index + 1;
        if r.outcome == Outcome::Victory {
            reached(Milestone::FirstVictory, run, minutes);
        }
        if !g.campaign.world.cleared.is_empty() {
            reached(Milestone::FirstLiberation, run, minutes);
        }
        for d in 1..=max_depth {
            if earned >= tier_cost(d) {
                reached(Milestone::TierAffordable(d), run, minutes);
            }
        }
        runs.push(PacingRun {
            index: run,
            country: r.country_id.clone().unwrap_or_default(),
            stage: r.stage,
            outcome: r.outcome,
            elapsed: r.elapsed,
            coins: r.coins,
            learned,
        });
    }
    milestones.sort_by_key(|(m, ..)| *m);
    Ok(PacingReport {
        profile: profile.clone(),
        runs,
        milestones,
    })
}
/// The profiles the pacing table reports.
pub fn pacing_profiles(max_runs: usize) -> Vec<PacingProfile> {
    vec![
        PacingProfile {
            name: "expert, tree first, Germany",
            seed: 0x1A571167,
            country: "DEU",
            aim: PacingAim::Expert,
            spend: PacingSpend::Recommended,
            max_runs,
        },
        PacingProfile {
            name: "novice, tree first, Germany",
            seed: 0x1A571167,
            country: "DEU",
            aim: PacingAim::Novice,
            spend: PacingSpend::Recommended,
            max_runs,
        },
        PacingProfile {
            name: "novice, gear first, Luxembourg",
            seed: 2026,
            country: "LUX",
            aim: PacingAim::Novice,
            spend: PacingSpend::GearFirst,
            max_runs,
        },
    ]
}
