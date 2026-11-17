//! Per-archetype telemetry population (W2-05, enemy-balance; schema v7
//! §3.12). The spawn director, the reap and both defender-damage paths
//! (Personal hit points and the NPC injury resolver) write
//! `spawned_by_archetype`, `kills_by_archetype` and
//! `damage_taken_by_archetype`, keyed by the resolved archetype id. Bosses and
//! unresolved legacy enemies are never counted. A kill counts only while the
//! archetype's kills are below its spawns, so an attempt resumed from a save
//! made before the maps were written still loads after it kills an enemy whose
//! spawn was never counted. Finished runs copy the elite kills, the boss
//! time-to-kill and the boss rank into their `RunRecord`.
mod support;
use horde_core::{
    content::{EnemyKind, Mode},
    rules::InjuryState,
    sim::{Attempt, Game, Input, SAVE_VERSION},
};
use std::collections::BTreeMap;
use support::{legacy_game, smoke_run};

/// The archetype id a legacy kind resolves to.
fn archetype(kind: EnemyKind) -> String {
    horde_core::enemy_catalog::resolve_kind(kind).archetype_id
}

/// A legacy game with an unpaused attempt deployed in `mode`, the attempt
/// taken out of its slot so the test can stage enemies.
fn staged(mode: Mode) -> (Game, Attempt) {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    let a = g.attempt_slot_mut().take().unwrap();
    (g, a)
}

/// Ticks `g` until the defender has taken damage from a hostile (at most
/// `limit` ticks), then returns the live attempt's telemetry maps.
fn tick_until_hurt(g: &mut Game, limit: u32) -> &Attempt {
    for _ in 0..limit {
        g.tick(Input::default());
        let hurt = g
            .attempt()
            .is_some_and(|a| a.telemetry.damage_by_lane.iter().sum::<u64>() > 0);
        if hurt {
            break;
        }
    }
    g.attempt().expect("the defender survives its first wound")
}

/// Fail-first: spawns, kills and defender damage are counted per resolved
/// archetype on both modes; bosses are never counted.
#[test]
fn combat_telemetry_counts_spawns_kills_and_damage_taken_by_archetype() {
    let shambler = archetype(EnemyKind::Shambler);
    let runner = archetype(EnemyKind::Runner);
    assert_ne!(shambler, runner);

    // Spawns and kills (Personal): two shamblers, one runner and the boss.
    let (mut g, mut a) = staged(Mode::Personal);
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 1, false);
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 3, false);
    g.spawn_enemy(&mut a, EnemyKind::Runner, 5, false);
    g.spawn_enemy(&mut a, EnemyKind::Brute, 7, true);
    assert_eq!(a.telemetry.spawned, 4);
    assert_eq!(
        a.telemetry.spawned_by_archetype,
        BTreeMap::from([(shambler.clone(), 2), (runner.clone(), 1)]),
        "the boss is not counted"
    );
    assert!(a.telemetry.kills_by_archetype.is_empty());
    // The first shambler and the runner fall in the next tick's reap.
    a.enemies[0].hp = 0;
    a.enemies[2].hp = 0;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    assert_eq!(a.kills, 2);
    assert_eq!(
        a.telemetry.kills_by_archetype,
        BTreeMap::from([(shambler.clone(), 1), (runner.clone(), 1)])
    );
    // A boss kill adds to `kills` only.
    let mut a = g.attempt_slot_mut().take().unwrap();
    a.enemies.iter_mut().find(|e| e.boss).unwrap().hp = 0;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    assert_eq!(a.kills, 3);
    assert_eq!(
        a.telemetry.kills_by_archetype.values().sum::<u32>(),
        2,
        "the boss kill is not counted"
    );
    assert!(serde_json::to_string(&g).is_ok());

    // Defender damage, Personal hit points: one runner in contact, too tough
    // to fall.
    let (mut g, mut a) = staged(Mode::Personal);
    a.profile.personal.armor = 0;
    g.spawn_enemy(&mut a, EnemyKind::Runner, 5, false);
    let e = &mut a.enemies[0];
    e.x = 1.2;
    e.hp = 100_000;
    e.max_hp = 100_000;
    *g.attempt_slot_mut() = Some(a);
    let a = tick_until_hurt(&mut g, 1200);
    let taken: u64 = a.telemetry.damage_by_lane.iter().sum();
    assert!(taken > 0, "the runner reached and struck the defender");
    assert_eq!(
        a.telemetry.damage_taken_by_archetype,
        BTreeMap::from([(runner.clone(), taken)])
    );

    // Defender damage, the NPC injury path: one shambler in contact that the
    // defender cannot drop, and a boss in contact whose damage is not counted.
    let (mut g, mut a) = staged(Mode::Npc);
    a.ammo = 0;
    a.reload_left = 100_000;
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let e = &mut a.enemies[0];
    e.x = 1.2;
    e.hp = 100_000;
    e.max_hp = 100_000;
    e.injury = InjuryState::new(100_000, 10);
    *g.attempt_slot_mut() = Some(a);
    let a = tick_until_hurt(&mut g, 6000);
    let taken: u64 = a.telemetry.damage_by_lane.iter().sum();
    assert!(taken > 0, "the shambler wounded the NPC defender");
    assert_eq!(
        a.telemetry.damage_taken_by_archetype,
        BTreeMap::from([(shambler.clone(), taken)])
    );

    // A boss's blows are not counted.
    let (mut g, mut a) = staged(Mode::Personal);
    a.profile.personal.armor = 0;
    a.max_hp = 100_000;
    a.hp = 100_000;
    g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
    let e = &mut a.enemies[0];
    e.x = 1.2;
    e.hp = 100_000;
    e.max_hp = 100_000;
    *g.attempt_slot_mut() = Some(a);
    let a = tick_until_hurt(&mut g, 1200);
    assert!(a.telemetry.damage_by_lane.iter().sum::<u64>() > 0);
    assert!(a.telemetry.damage_taken_by_archetype.is_empty());
}

/// Fail-first: over the seeded headless runs the archetype maps account for
/// every non-boss spawn and kill (no run fields a legacy unresolved enemy),
/// each archetype's kills stay within its spawns, and the save loads again.
#[test]
fn kills_by_archetype_sums_to_the_non_boss_kills_of_seeded_runs() {
    for seed in [1, 0x1A57_1167, 0xDEAD_BEEF_2026] {
        for mode in [Mode::Personal, Mode::Npc] {
            let g = smoke_run(seed, mode, 3600, |_| {});
            let (t, kills) = match g.attempt() {
                Some(a) => (&a.telemetry, a.kills),
                None => {
                    let r = g.last_result().unwrap();
                    (&r.telemetry, r.kills)
                }
            };
            let what = format!("seed {seed:#x} {mode:?}");
            let boss_spawned = u32::from(t.boss_arrival_tick.is_some());
            let boss_killed = u32::from(t.boss_defeat_tick.is_some());
            assert!(kills > 0, "{what}: the run kills something");
            assert_eq!(
                t.spawned_by_archetype.values().sum::<u32>(),
                t.spawned - boss_spawned,
                "{what}: every non-boss spawn is counted once"
            );
            assert_eq!(
                t.kills_by_archetype.values().sum::<u32>(),
                kills - boss_killed,
                "{what}: every non-boss kill is counted once"
            );
            for (id, killed) in &t.kills_by_archetype {
                assert!(
                    *killed <= t.spawned_by_archetype[id],
                    "{what}: {id} kills stay within its spawns"
                );
            }
            Game::from_payload(serde_json::to_value(&g).unwrap(), SAVE_VERSION)
                .unwrap_or_else(|e| panic!("{what}: the populated save loads: {e}"));
        }
    }
}

/// Fail-first (writer obligation, schema v7 §3.12): an attempt resumed from a
/// save made before the maps were written kills an enemy whose spawn was
/// never counted. The kill adds to `kills` but not to `kills_by_archetype`,
/// so the save still loads; a later spawn of the same archetype is counted
/// and its kill is too.
#[test]
fn resumed_attempt_counts_a_kill_only_below_its_archetype_spawns() {
    let shambler = archetype(EnemyKind::Shambler);
    let (mut g, mut a) = staged(Mode::Personal);
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 1, false);
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 3, false);
    *g.attempt_slot_mut() = Some(a);
    // The save an older build wrote: live enemies, no archetype maps.
    let mut value = serde_json::to_value(&g).unwrap();
    let telemetry = value["attempt"]["telemetry"].as_object_mut().unwrap();
    telemetry.remove("spawned_by_archetype");
    telemetry.remove("kills_by_archetype");
    telemetry.remove("damage_taken_by_archetype");
    let mut g = Game::from_payload(value, SAVE_VERSION).unwrap();
    assert!(
        g.attempt()
            .unwrap()
            .telemetry
            .spawned_by_archetype
            .is_empty()
    );

    // Kill an enemy spawned before the maps were written.
    g.attempt_mut().unwrap().enemies[0].hp = 0;
    g.tick(Input::default());
    assert_eq!(g.attempt().unwrap().kills, 1);
    let mut g = Game::from_payload(serde_json::to_value(&g).unwrap(), SAVE_VERSION)
        .expect("the resumed attempt still loads after the kill");
    let a = g.attempt().unwrap();
    assert!(
        a.telemetry.kills_by_archetype.is_empty(),
        "an uncounted spawn's kill is not counted: {:?}",
        a.telemetry.kills_by_archetype
    );

    // A new spawn is counted, and so is one kill against it.
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    assert_eq!(a.telemetry.spawned_by_archetype[&shambler], 1);
    for e in &mut a.enemies {
        e.hp = 0;
    }
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    assert_eq!(a.kills, 3);
    assert_eq!(
        a.telemetry.kills_by_archetype,
        BTreeMap::from([(shambler, 1)]),
        "two kills, one counted spawn: one counted kill"
    );
    Game::from_payload(serde_json::to_value(&g).unwrap(), SAVE_VERSION)
        .expect("the save loads with kills at the spawn count");
}

/// Fail-first: a finished run's record carries the boss time-to-kill (defeat
/// tick minus arrival tick), the attempt's elite kills and its boss rank.
#[test]
fn run_record_carries_boss_ttk_elite_kills_and_boss_rank() {
    let (mut g, mut a) = staged(Mode::Personal);
    a.tick = 300;
    g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(300);
    a.tick = 900;
    a.enemies[0].hp = 0;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    assert!(g.attempt().is_none(), "the boss kill ends the run");
    let record = g.campaign().run_history.last().unwrap();
    assert_eq!(record.boss_ttk_ticks, Some(601));
    assert_eq!(record.elite_kills, 0);
    assert_eq!(record.boss_rank, Default::default());

    // A run without a boss kill has no time-to-kill.
    let (mut g, a) = staged(Mode::Personal);
    *g.attempt_slot_mut() = Some(a);
    g.withdraw();
    assert_eq!(
        g.campaign().run_history.last().unwrap().boss_ttk_ticks,
        None
    );

    // Elite kills and the boss rank come from the attempt.
    let (mut g, mut a) = staged(Mode::Personal);
    a.kills = 3;
    a.telemetry.elites_killed = 2;
    a.progression.boss_rank = horde_core::progression::BossRank::Regional;
    *g.attempt_slot_mut() = Some(a);
    g.withdraw();
    let record = g.campaign().run_history.last().unwrap();
    assert_eq!(record.elite_kills, 2);
    assert_eq!(
        record.boss_rank,
        horde_core::progression::BossRank::Regional
    );
    assert_eq!(
        g.last_result().unwrap().boss_rank,
        horde_core::progression::BossRank::Regional
    );
}

/// W2 review: the record's bounds hold for telemetry outside them (a save
/// edited by hand, say). Elite kills are capped at the run's kills and the
/// boss time-to-kill at the run's duration; values inside the bounds are
/// copied unchanged (the test above).
#[test]
fn run_record_clamps_elite_kills_to_kills_and_boss_ttk_to_duration() {
    let (mut g, mut a) = staged(Mode::Personal);
    a.tick = 100;
    a.kills = 2;
    a.telemetry.elites_killed = 5;
    a.telemetry.boss_arrival_tick = Some(10);
    a.telemetry.boss_defeat_tick = Some(5_000);
    *g.attempt_slot_mut() = Some(a);
    g.withdraw();
    let record = g.campaign().run_history.last().unwrap();
    assert_eq!(record.duration_ticks, 100);
    assert_eq!(record.kills, 2);
    assert_eq!(record.elite_kills, 2, "elite kills are at most the kills");
    assert_eq!(
        record.boss_ttk_ticks,
        Some(100),
        "the boss time-to-kill is at most the run's duration"
    );
}
