//! The expedition threat rule (W4-12, ruling R57): an expedition's threat
//! rank follows the recruit, its training and level and its gear, never the
//! campaign rank. It is set when the expedition attempt is created and saved
//! where every attempt saves its threat (`Attempt.progression.rank`); the
//! player's and Mara's main runs, and a recruit's main-run takeover, keep
//! the campaign rank.
mod support;
use horde_core::{
    command::{Command, CommandOutcome},
    content::Mode,
    defenders::{
        DefenderId, EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL, EXPEDITION_RANKS_PER_ITEM_LEVEL,
        expedition_threat_rank,
    },
    sim::{Game, SAVE_VERSION},
    world::world,
};
use support::{advance, enemy, strategic_game};

/// A strategic game (four liberated countries, recruitment and parallel
/// expeditions open) at campaign threat rank `rank`.
fn ranked_game(rank: u64) -> Game {
    let mut g = strategic_game();
    let c = g.campaign_mut();
    c.threat_revision = 1;
    c.victories = rank;
    assert_eq!(c.progression_rank(), rank as u32);
    g
}

fn home(g: &Game) -> String {
    g.campaign().world.cleared[0].clone()
}

/// An enterable front of more than one stage that is not liberated.
fn fronts(g: &Game) -> Vec<String> {
    world()
        .countries
        .iter()
        .filter(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .map(|c| c.id.clone())
        .collect()
}

fn recruit(g: &mut Game) -> u64 {
    let country = home(g);
    match g.apply(&Command::Recruit(country)) {
        Ok(CommandOutcome::Id(id)) => id,
        other => panic!("recruiting failed: {other:?}"),
    }
}

/// Raises recruit `id` to advancement level 4 (30 awarded points, unspent).
fn train(g: &mut Game, id: u64) {
    let npc = &mut g
        .staging()
        .strategy()
        .roster
        .iter_mut()
        .find(|r| r.id == id)
        .unwrap()
        .npc;
    npc.awarded = 30;
    npc.unspent = 30;
    assert_eq!(npc.advancement_level(), 4);
}

/// Equips recruit `id` with a level-`level` item in every slot, by command.
fn gear(g: &mut Game, id: u64, level: u32) {
    for definition in 0..4 {
        let item = g
            .campaign_mut()
            .add_item_at_level(definition, level)
            .unwrap();
        g.apply(&Command::EquipRecruit { id, item })
            .unwrap_or_else(|e| panic!("equip slot {definition}: {e}"));
    }
}

fn send(g: &mut Game, id: u64, country: &str) -> u64 {
    match g.apply(&Command::DeploySurvivor {
        id,
        country: country.to_owned(),
        stage: 0,
    }) {
        Ok(CommandOutcome::Id(battle)) => battle,
        other => panic!("expedition failed: {other:?}"),
    }
}

fn expedition_rank(g: &Game, battle: u64) -> u32 {
    g.battles()
        .iter()
        .find(|b| b.id == battle)
        .unwrap()
        .game()
        .attempt()
        .expect("a live expedition attempt")
        .progression
        .rank
}

fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e}"))
}

#[test]
fn the_rule_reads_training_and_gear_only() {
    assert_eq!(expedition_threat_rank(1, [1; 4]), 0, "a starter recruit");
    assert_eq!(
        expedition_threat_rank(4, [1; 4]),
        3 * EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL
    );
    assert_eq!(
        expedition_threat_rank(1, [9; 4]),
        8 * EXPEDITION_RANKS_PER_ITEM_LEVEL
    );
    // The gear term averages the four slots (rounded down).
    assert_eq!(
        expedition_threat_rank(1, [9, 1, 1, 1]),
        8 * EXPEDITION_RANKS_PER_ITEM_LEVEL / 4
    );
    assert_eq!(
        expedition_threat_rank(4, [9; 4]),
        3 * EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL + 8 * EXPEDITION_RANKS_PER_ITEM_LEVEL
    );
    // Level 0 never occurs on a valid npc; it reads as level 1.
    assert_eq!(expedition_threat_rank(0, [1; 4]), 0);
    assert_eq!(
        expedition_threat_rank(u8::MAX, [u32::MAX; 4]),
        expedition_threat_rank(u8::MAX, [u32::MAX; 4]),
        "total and deterministic at the extremes"
    );
}

#[test]
fn expedition_threat_rank_follows_the_recruit_not_the_campaign_rank() {
    let expected = expedition_threat_rank(4, [9; 4]);
    assert!(expected > 0 && expected < 15);
    let mut ranks = Vec::new();
    for campaign_rank in [15, 25] {
        let mut g = ranked_game(campaign_rank);
        let targets = fronts(&g);
        let starter = recruit(&mut g);
        let veteran = recruit(&mut g);
        train(&mut g, veteran);
        gear(&mut g, veteran, 9);
        let a = send(&mut g, starter, &targets[0]);
        let b = send(&mut g, veteran, &targets[1]);
        assert_eq!(expedition_rank(&g, a), 0, "a starter recruit plays rank 0");
        assert_eq!(expedition_rank(&g, b), expected, "training and gear");
        assert_eq!(g.campaign().progression_rank(), campaign_rank as u32);
        ranks.push((expedition_rank(&g, a), expedition_rank(&g, b)));
    }
    assert_eq!(ranks[0], ranks[1], "the campaign rank does not move it");

    // A recruit whose own rank is above the campaign's plays the campaign
    // rank: an expedition is never harder than the main run.
    let mut g = ranked_game(2);
    let targets = fronts(&g);
    let veteran = recruit(&mut g);
    train(&mut g, veteran);
    gear(&mut g, veteran, 9);
    let b = send(&mut g, veteran, &targets[0]);
    assert_eq!(expedition_rank(&g, b), 2);
    reload(&g);
}

#[test]
fn expedition_threat_rank_is_set_when_the_attempt_is_created() {
    let mut g = ranked_game(15);
    let country = fronts(&g)
        .into_iter()
        .find(|c| world().country(c).unwrap().level_count() >= 3)
        .expect("a front of three stages or more");
    let id = recruit(&mut g);
    let operation = match g.apply(&Command::CountryOperation {
        id,
        country: country.clone(),
    }) {
        Ok(CommandOutcome::Id(battle)) => battle,
        other => panic!("operation failed: {other:?}"),
    };
    assert_eq!(expedition_rank(&g, operation), 0);
    let snapshot = |g: &Game| {
        g.battles()
            .iter()
            .find(|b| b.id == operation)
            .unwrap()
            .game()
            .attempt()
            .unwrap()
            .progression
            .clone()
    };
    let created = snapshot(&g);

    // The rank is frozen in the attempt: the recruit levelling up inside
    // the worker, or the campaign climbing, changes nothing on this stage,
    // and the save keeps it.
    let worker = g
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == operation)
        .unwrap()
        .game_mut();
    worker.campaign_mut().npc.awarded = 30;
    worker.campaign_mut().npc.unspent = 30;
    g.campaign_mut().victories += 3;
    advance(&mut g, 2);
    assert_eq!(snapshot(&g), created, "frozen at creation");
    let loaded = reload(&g);
    let saved = loaded
        .battles()
        .iter()
        .find(|b| b.id == operation)
        .unwrap()
        .game()
        .attempt()
        .unwrap()
        .progression
        .clone();
    assert_eq!(saved, created, "saved with the attempt");

    // The operation's next stage is a new expedition attempt, created with
    // the recruit as it is then: advancement level 4.
    let b = g
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == operation)
        .unwrap();
    enemy(b.game_mut(), 0, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().injury.unconscious = true;
    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    let b = g.battles().iter().find(|b| b.id == operation).unwrap();
    assert!(!b.settled && b.stage == 1, "the operation continues");
    let level = b.game().campaign().npc.advancement_level();
    assert!(level >= 4, "the worker's recruit kept its level ({level})");
    assert_eq!(
        expedition_rank(&g, operation),
        expedition_threat_rank(level, [1; 4]),
        "the next stage reads the recruit when it is created"
    );
    assert!(g.campaign().progression_rank() > 15, "the campaign climbed");
    reload(&g);
}

#[test]
fn main_run_threat_rank_still_follows_the_campaign_rank() {
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = ranked_game(15);
        let id = recruit(&mut g);
        train(&mut g, id);
        let country = fronts(&g).remove(0);
        g.apply(&Command::SelectCountry(country)).unwrap();
        g.apply(&Command::Deploy { stage: 0, mode }).unwrap();
        let a = g.attempt().unwrap();
        assert_eq!(a.progression.rank, 15, "{mode:?} main run");
    }
    // A recruit taking the main run over fights at the campaign rank too.
    let mut g = ranked_game(15);
    let id = recruit(&mut g);
    let country = fronts(&g).remove(0);
    g.apply(&Command::SelectCountry(country)).unwrap();
    g.apply(&Command::SetNextDefender(DefenderId::Recruit(id)))
        .unwrap();
    g.apply(&Command::BeginDefense { stage: 0 }).unwrap();
    let a = g.attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.progression.rank, 15, "takeover main run");
}
