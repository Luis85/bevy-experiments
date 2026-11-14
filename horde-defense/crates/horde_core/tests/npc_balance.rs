#![allow(clippy::print_stdout)] // prints the calibration table it asserts on
//! The Npc combat balance pass (W4-14, user decision R59): under the Npc
//! rules Mara and recruits hold stage 1 of a fresh country at low ranks.
//!
//! Each persona plays stage 1 of the six 3-stage neighbours of a liberated
//! Germany on three seeds (18 runs), on the Normal preset, in the arena a
//! campaign has when its expeditions leave (player level 25: 17 lanes,
//! 28 yards). Mara defends a main run at threat rank 0. The recruits go on
//! real expeditions (`Command::DeploySurvivor`) from a campaign at threat
//! rank 15, so the W4-12 rule (R57) sets their rank: a starter recruit plays
//! rank 0, a recruit trained 20 points (advancement level 2, all on Guns)
//! rank 1. Release-only, like `pacing_targets`: 54 full stages.
mod support;
use horde_core::campaign::NpcTraining;
use horde_core::command::{Command, CommandOutcome};
use horde_core::content::Mode;
use horde_core::defenders::DefenderId;
use horde_core::sim::{Game, Input, Outcome};
use horde_core::world::world;

/// The 3-stage neighbours of Germany an expedition may take (Austria is the
/// second liberation, which opens expeditions).
const NEIGHBOURS: [&str; 6] = ["BEL", "CHE", "CZE", "DNK", "LUX", "NLD"];
const SEEDS: [u64; 3] = [1, 2, 3];
/// The player level of the campaign: 17 lanes and a 28-yard field.
const PLAYER_LEVEL: u32 = 25;
/// The campaign threat rank the expeditions leave at (13-17 in the W4-c
/// strategy sweep).
const EXPEDITION_CAMPAIGN_RANK: u64 = 15;
/// The longest a stage may run before the probe gives up (ticks).
const MAX_TICKS: u32 = 60 * 1_500;
/// The wins of 18 each persona needs.
const MIN_WINS: usize = 15;

/// A Normal campaign that started in Germany and liberated Germany and
/// Austria, owns the companion, recruitment and expedition features and
/// stands at threat rank `rank`.
fn campaign(seed: u64, rank: u64) -> Game {
    let mut g = Game::default();
    g.set_seed(seed);
    let c = g.campaign_mut();
    c.choose_start_country("DEU").unwrap();
    for id in ["DEU", "AUT"] {
        let stages = world().country(id).unwrap().level_count();
        c.world.country_levels.insert(id.into(), stages);
        c.world.cleared.push(id.into());
    }
    for node in [
        "companion_contact",
        "recruitment_network",
        "parallel_command",
    ] {
        c.skill_nodes.insert(node.into());
    }
    c.attempts = 1;
    c.coins = 100_000;
    c.player.level = PLAYER_LEVEL;
    c.threat_revision = 1;
    c.victories = rank;
    assert_eq!(c.progression_rank(), rank as u32);
    assert_eq!(c.arena_geometry(), (17, 28.0));
    g
}

/// Mara's main run on stage 1 of `country` at threat rank 0.
fn mara(seed: u64, country: &str) -> Outcome {
    let mut g = campaign(seed, 0);
    g.campaign_mut().enter_country(country).unwrap();
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    g.set_paused(false);
    for _ in 0..MAX_TICKS {
        if g.attempt().is_none() {
            break;
        }
        g.tick(Input::default());
    }
    g.last_result().expect("Mara's stage ends").outcome
}

/// A recruit's expedition to stage 1 of `country`, trained `points` points
/// on Guns first, from a campaign at [`EXPEDITION_CAMPAIGN_RANK`]; returns
/// the outcome and the rank the W4-12 rule gave it.
fn expedition(seed: u64, country: &str, points: u32) -> (Outcome, u32) {
    let mut g = campaign(seed, EXPEDITION_CAMPAIGN_RANK);
    let id = match g.apply(&Command::RecruitAs {
        country: "DEU".into(),
        archetype: "Marksman".into(),
    }) {
        Ok(CommandOutcome::Id(id)) => id,
        other => panic!("recruiting failed: {other:?}"),
    };
    for _ in 0..points {
        g.apply(&Command::BuyRecruitPoint(id)).unwrap();
    }
    for _ in 0..points / NpcTraining::Guns.cost() {
        g.apply(&Command::TrainRecruit {
            id,
            training: NpcTraining::Guns,
        })
        .unwrap();
    }
    let battle = match g.apply(&Command::DeploySurvivor {
        id,
        country: country.into(),
        stage: 0,
    }) {
        Ok(CommandOutcome::Id(battle)) => battle,
        other => panic!("the expedition did not leave: {other:?}"),
    };
    let find = |g: &Game| g.battles().iter().find(|b| b.id == battle).cloned();
    let rank = find(&g)
        .and_then(|b| b.game().attempt().map(|a| a.progression.rank))
        .expect("a live expedition");
    g.set_paused(false);
    for _ in 0..MAX_TICKS {
        if find(&g).is_none_or(|b| b.settled) {
            break;
        }
        g.tick(Input::default());
    }
    let summary = find(&g)
        .and_then(|b| b.summary)
        .expect("the expedition settles with a notice");
    (summary.outcome.expect("a settled outcome"), rank)
}

/// The Npc rules hold stage 1 of a fresh neighbour at low ranks: Mara at
/// rank 0, a starter recruit at its W4-12 rank 0 and a recruit trained 20
/// points at rank 1 each win at least 15 of the 18 runs. The table prints
/// `Game::defender_power` beside the wins.
#[test]
#[ignore = "release-only"]
fn npc_defenders_hold_stage_one_of_a_fresh_neighbour_at_low_ranks() {
    let mut problems = Vec::new();
    let rating = |points: u32, threat: u32| {
        let mut g = campaign(1, EXPEDITION_CAMPAIGN_RANK);
        let id = match g.apply(&Command::RecruitAs {
            country: "DEU".into(),
            archetype: "Marksman".into(),
        }) {
            Ok(CommandOutcome::Id(id)) => id,
            other => panic!("recruiting failed: {other:?}"),
        };
        for _ in 0..points {
            g.apply(&Command::BuyRecruitPoint(id)).unwrap();
        }
        for _ in 0..points / NpcTraining::Guns.cost() {
            g.apply(&Command::TrainRecruit {
                id,
                training: NpcTraining::Guns,
            })
            .unwrap();
        }
        g.defender_power(DefenderId::Recruit(id), threat)
            .unwrap()
            .rating
    };
    let mara_rating = campaign(1, 0)
        .defender_power(DefenderId::Mara, 0)
        .unwrap()
        .rating;
    let mut row = |what: &str, wins: usize, ranks: &[u32], power: u16| {
        println!("{what}: won {wins} of 18 at ranks {ranks:?}; defender_power {power}");
        if wins < MIN_WINS {
            problems.push(format!("{what} won {wins} of 18 (needs {MIN_WINS})"));
        }
    };
    let runs = || {
        NEIGHBOURS
            .iter()
            .flat_map(|country| SEEDS.map(|seed| (seed, *country)))
    };
    let wins = runs()
        .filter(|(seed, country)| mara(*seed, country) == Outcome::Victory)
        .count();
    row("Mara, main run", wins, &[0], mara_rating);
    for points in [0, 20] {
        let results: Vec<(Outcome, u32)> = runs()
            .map(|(seed, country)| expedition(seed, country, points))
            .collect();
        let wins = results
            .iter()
            .filter(|(outcome, _)| *outcome == Outcome::Victory)
            .count();
        let mut ranks: Vec<u32> = results.iter().map(|(_, rank)| *rank).collect();
        ranks.dedup();
        row(
            &format!("recruit trained {points} points, expedition"),
            wins,
            &ranks,
            rating(points, ranks[0]),
        );
    }
    assert!(problems.is_empty(), "{}", problems.join("; "));
}
