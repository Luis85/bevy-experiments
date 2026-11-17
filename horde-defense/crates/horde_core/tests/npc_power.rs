//! The Npc defender power readout (W4-16, user decision R62): the Npc model
//! of `Game::defender_power` reads what decides an Npc fight since W4-14,
//! one action per turn, the Aim turns that pay, the range over the arena,
//! the arrivals and their backlog, and gear and training, and its band
//! agrees with W4-14's win table (plan section 9.6, "W4-14 Npc combat
//! balance").
#![allow(clippy::print_stdout)] // prints the calibration table it asserts on
use horde_core::campaign::NpcTraining;
use horde_core::command::{Command, CommandOutcome};
use horde_core::content::Mode;
use horde_core::defenders::{DefenderId, DefenderPower, PowerBand};
use horde_core::progression::FrontPosition;
use horde_core::sim::{DeploymentInputs, Game};
use horde_core::world::world;

/// The player level of W4-14's two arenas: 9 lanes x 18 yards and 17 lanes
/// x 28 yards.
const ARENAS: [(u32, (u8, f32)); 2] = [(10, (9, 18.0)), (25, (17, 28.0))];
/// The campaign rank the recruits leave from (W4-14: 15).
const CAMPAIGN_RANK: u64 = 15;

/// W4-14's campaign (tests/npc_balance.rs): Normal, Germany and Austria
/// liberated, the companion, recruitment and expedition features owned,
/// at player level `level` and threat rank `rank`. Two more liberations
/// raise the advancement allowance to the 60 points the table trains.
fn campaign(level: u32, rank: u64) -> Game {
    let mut g = Game::default();
    g.set_seed(1);
    let c = g.campaign_mut();
    c.choose_start_country("DEU").unwrap();
    for id in ["DEU", "AUT", "FRA", "POL"] {
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
    c.coins = 1_000_000;
    c.player.level = level;
    c.threat_revision = 1;
    c.victories = rank;
    g
}

/// A Marksman recruit trained `points` points, all on Guns, in a campaign
/// of player level `level`: the game, its id and the rank the W4-12 rule
/// gives its expedition.
fn recruit(level: u32, points: u32) -> (Game, u64, u32) {
    let mut g = campaign(level, CAMPAIGN_RANK);
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
    let npc = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .npc
        .clone();
    let rank = g.campaign().expedition_threat_rank(&npc);
    (g, id, rank)
}

/// `defender`'s inputs at `threat` on 0-based `stage` of a 3-stage front
/// (stage 0 is the rank-only reading `defender_power` gives).
fn stage_inputs(g: &Game, defender: DefenderId, threat: u32, stage: u32) -> DeploymentInputs {
    let mut inputs = g.deployment_inputs(defender, threat).unwrap();
    if stage > 0 {
        inputs.snapshot = g
            .campaign()
            .progression_for(
                Mode::Npc,
                threat,
                g.campaign().lane_count(),
                Some(FrontPosition { stage, count: 3 }),
            )
            .unwrap();
        inputs.level = stage as usize;
    }
    inputs
}

/// One row of W4-14's calibration table (plan section 9.6): who fights, at
/// which rank on which stage, and the wins of 18 in each arena.
struct Row {
    what: &'static str,
    /// None: Mara on a main run; Some(points): a Marksman recruit trained
    /// that many points on Guns.
    trained: Option<u32>,
    /// The rank, or None for the recruit's W4-12 expedition rank.
    rank: Option<u32>,
    stage: u32,
    wins: [u32; 2],
}

const TABLE: [Row; 11] = [
    Row {
        what: "Mara starter, stage 1, rank 0",
        trained: None,
        rank: Some(0),
        stage: 0,
        wins: [18, 17],
    },
    Row {
        what: "Marksman recruit starter, stage 1, rank 0",
        trained: Some(0),
        rank: None,
        stage: 0,
        wins: [18, 17],
    },
    Row {
        what: "the same, stage 2 (front ramp)",
        trained: Some(0),
        rank: None,
        stage: 1,
        wins: [5, 5],
    },
    Row {
        what: "the same, stage 3 (Capstone)",
        trained: Some(0),
        rank: None,
        stage: 2,
        wins: [0, 0],
    },
    Row {
        what: "trained 20 (rank 1)",
        trained: Some(20),
        rank: None,
        stage: 0,
        wins: [18, 18],
    },
    Row {
        what: "trained 28 (rank 2)",
        trained: Some(28),
        rank: None,
        stage: 0,
        wins: [16, 18],
    },
    Row {
        what: "trained 36 (rank 3)",
        trained: Some(36),
        rank: None,
        stage: 0,
        wins: [7, 16],
    },
    Row {
        what: "trained 44 (rank 4)",
        trained: Some(44),
        rank: None,
        stage: 0,
        wins: [2, 9],
    },
    Row {
        what: "trained 60 (rank 6)",
        trained: Some(60),
        rank: None,
        stage: 0,
        wins: [0, 1],
    },
    Row {
        what: "starter at rank 5",
        trained: Some(0),
        rank: Some(5),
        stage: 0,
        wins: [0, 0],
    },
    Row {
        what: "starter at rank 10",
        trained: Some(0),
        rank: Some(10),
        stage: 0,
        wins: [0, 0],
    },
];

/// The power of `row` in the arena of player level `level`, and the rank
/// it was read at.
fn row_power(row: &Row, level: u32) -> (DefenderPower, u32) {
    match row.trained {
        None => {
            let rank = row.rank.unwrap_or(0);
            let g = campaign(level, u64::from(rank));
            let inputs = stage_inputs(&g, DefenderId::Mara, rank, row.stage);
            (inputs.power(), rank)
        }
        Some(points) => {
            let (g, id, expedition) = recruit(level, points);
            let rank = row.rank.unwrap_or(expedition);
            let inputs = stage_inputs(&g, DefenderId::Recruit(id), rank, row.stage);
            (inputs.power(), rank)
        }
    }
}

#[test]
fn the_table_personas_match_w4_14() {
    for (level, arena) in ARENAS {
        assert_eq!(campaign(level, 0).campaign().arena_geometry(), arena);
    }
    for (points, rank) in [(0, 0), (20, 1), (28, 2), (36, 3), (44, 4), (60, 6)] {
        assert_eq!(recruit(25, points).2, rank, "trained {points}");
    }
}

/// W4-14's two fail-first symptoms: a recruit trained 20 points on Guns
/// read 98 at its expedition rank 1, below the starter's 100 at rank 0,
/// though it wins more (18 of 18 against 17).
#[test]
fn guns_training_raises_npc_power_at_the_recruits_expedition_rank() {
    for (level, _) in ARENAS {
        let (starter, starter_id, starter_rank) = recruit(level, 0);
        let (trained, trained_id, trained_rank) = recruit(level, 20);
        assert_eq!((starter_rank, trained_rank), (0, 1));
        let low = starter
            .defender_power(DefenderId::Recruit(starter_id), starter_rank)
            .unwrap();
        let high = trained
            .defender_power(DefenderId::Recruit(trained_id), trained_rank)
            .unwrap();
        assert!(
            high.rating > low.rating,
            "level {level}: trained 20 at rank 1 {high:?}, starter at rank 0 {low:?}"
        );
        // More Guns at the same rank never reads weaker.
        let mut more = starter
            .deployment_inputs(DefenderId::Recruit(starter_id), 0)
            .unwrap();
        more.profile.ranged_skill += 5;
        assert!(
            more.power().rating > low.rating,
            "{:?} vs {low:?}",
            more.power()
        );
    }
}

/// The hit chance the model reads is the range-penalised one: a deeper
/// field lowers a starter's kill rate, where the W2-21 model read the raw
/// skill and kept it.
#[test]
fn npc_power_reads_the_range_penalised_hit_chance() {
    let near = campaign(1, 0).defender_power(DefenderId::Mara, 0).unwrap();
    let far = campaign(25, 0).defender_power(DefenderId::Mara, 0).unwrap();
    assert!(
        far.kills_per_min < near.kills_per_min,
        "a 28-yard field {far:?}, a 12-yard field {near:?}"
    );
    // Past the 3d6 ceiling the raw skill adds nothing, but at range it
    // still lands more shots.
    let g = campaign(25, 0);
    let base = g.deployment_inputs(DefenderId::Mara, 0).unwrap();
    let mut sharp = base.clone();
    sharp.profile.ranged_skill += 4;
    assert!(
        sharp.power().kills_per_min > base.power().kills_per_min,
        "{:?} vs {:?}",
        sharp.power(),
        base.power()
    );
}

/// Accuracy counts only through the Aim turns that pay
/// (`sim::npc::aim_turns_that_pay`): a defender who never aims gains
/// nothing from it, one who aims at range does.
#[test]
fn npc_power_counts_only_the_aim_turns_that_pay() {
    let g = campaign(25, 0);
    let base = g.deployment_inputs(DefenderId::Mara, 0).unwrap();
    let with = |skill: i32, accuracy: i32| {
        let mut inputs = base.clone();
        inputs.profile.ranged_skill = skill;
        inputs.profile.npc.accuracy = accuracy;
        inputs.power()
    };
    // Skill 30: every shot lands at any range in the field, so no Aim
    // turn pays and accuracy changes nothing.
    assert_eq!(with(30, 0), with(30, 8));
    // Skill 10: long shots need the aim, so accuracy raises the kill rate.
    assert!(
        with(10, 8).kills_per_min > with(10, 0).kills_per_min,
        "{:?} vs {:?}",
        with(10, 8),
        with(10, 0)
    );
}

/// One action per turn caps the kill rate, and the arrivals are weighed
/// against it: no damage lifts the kill rate past a kill a turn, and more
/// arrivals read weaker.
#[test]
fn npc_power_weighs_arrivals_against_the_turn_limited_kill_rate() {
    let g = campaign(25, 0);
    let base = g.deployment_inputs(DefenderId::Mara, 0).unwrap();
    let mut huge = base.clone();
    huge.profile.npc.damage_dice = 20;
    huge.profile.npc.damage_add = 1_000;
    let power = huge.power();
    assert!(power.kills_per_min <= 60.0, "{power:?}");
    let mut crowded = base.clone();
    crowded.balance.npc_spawn_rate *= 1.5;
    assert!(
        crowded.power().rating < base.power().rating,
        "{:?} vs {:?}",
        crowded.power(),
        base.power()
    );
    // Fewer arrivals read stronger, and the rating is the load held
    // against them: the same kill rate holds twice the stage's own load
    // when the stage brings half (within the bisection's resolution).
    let mut sparse = base.clone();
    sparse.balance.npc_spawn_rate *= 0.5;
    let (sparse, base) = (sparse.power(), base.power());
    assert!(sparse.rating > base.rating);
    assert_eq!(sparse.kills_per_min, base.kills_per_min);
    let ratio = f32::from(sparse.rating) / f32::from(base.rating);
    assert!((1.9..=2.1).contains(&ratio), "{sparse:?} vs {base:?}");
}

/// The band orders W4-14's personas the way their wins do: a persona that
/// wins most of its 18 stages never reads Weak, one that loses most reads
/// Weak, and a persona that wins clearly more rates higher.
#[test]
fn npc_power_band_agrees_with_the_w4_14_win_table() {
    let mut cells = Vec::new();
    let mut problems = Vec::new();
    for (arena, (level, geometry)) in ARENAS.iter().enumerate() {
        for row in &TABLE {
            let (power, rank) = row_power(row, *level);
            let wins = row.wins[arena];
            let cell = format!(
                "{} lanes x {} yd, {} at rank {rank}: {} ({}), wins {wins} of 18",
                geometry.0,
                geometry.1,
                row.what,
                power.rating,
                power.band.label()
            );
            if wins >= 15 && power.band == PowerBand::Weak {
                problems.push(format!("reads Weak but wins most: {cell}"));
            }
            if wins <= 5 && power.band != PowerBand::Weak {
                problems.push(format!("loses most but does not read Weak: {cell}"));
            }
            cells.push((wins, power.rating, cell));
        }
    }
    for (wins_a, rating_a, a) in &cells {
        for (wins_b, rating_b, b) in &cells {
            if *wins_a >= wins_b + 8 && rating_a <= rating_b {
                problems.push(format!("out of order: {a} / {b}"));
            }
        }
    }
    for (_, _, cell) in &cells {
        println!("{cell}");
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
