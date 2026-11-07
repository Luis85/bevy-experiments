//! Enemies never block each other (user decision, 2026-11-06, W1-69).
//!
//! Until W1-69 a lane was traffic: an enemy stopped 0.65 yd behind the one in
//! front (`EnemyPhase::Queued`) and a faster one swung out by
//! `pass_offset` to overtake. The user ruled that enemies overlap instead:
//! there is no lane queue, every enemy walks until it reaches the defense
//! line or a live non-trap structure in its lane, and every enemy there
//! attacks. These tests pin that rule; they replace the queueing and passing
//! assertions on purpose, they are not weakened versions of them.
mod support;
use horde_core::{
    content::{EnemyKind, Mode, StructureKind},
    kind_map::KindMap,
    sim::{DEFENSE_LINE, EnemyPhase, Game, Input, MAX_PASS_OFFSET},
};
use support::{game, legacy_game};

fn fixture(lane: u8) -> (Game, u64, u64) {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, false);
    a.enemies.last_mut().unwrap().x = 14.0;
    let slow = a.enemies.last().unwrap().id;
    g.spawn_enemy(&mut a, EnemyKind::Runner, lane, false);
    a.enemies.last_mut().unwrap().x = 14.8;
    let fast = a.enemies.last().unwrap().id;
    *g.attempt_slot_mut() = Some(a);
    (g, slow, fast)
}
fn step(g: &mut Game, count: u32, batch: u32) {
    let mut left = count;
    while left > 0 {
        let n = left.min(batch);
        for _ in 0..n {
            g.tick(Input::default());
        }
        left -= n;
    }
}

/// Six hostiles of mixed speeds, slowest in front, all in `lane`, with a
/// defender that cannot fall. Returns the game and `(id, start x, speed)`.
fn crowd(lane: u8) -> (Game, Vec<(u64, f32, f32)>) {
    let mut g = game(Mode::Personal);
    let mut a = g.attempt_slot_mut().take().unwrap();
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    let mut crowd = Vec::new();
    for (kind, x) in [
        (EnemyKind::Brute, 3.0),
        (EnemyKind::Demolisher, 3.4),
        (EnemyKind::Armored, 3.8),
        (EnemyKind::Shambler, 4.2),
        (EnemyKind::Runner, 4.6),
        (EnemyKind::Runner, 5.0),
    ] {
        g.spawn_enemy(&mut a, kind, lane, false);
        let e = a.enemies.last_mut().unwrap();
        e.x = x;
        let speed = e.resolved.as_ref().unwrap().speed * a.modifiers.enemy_speed;
        crowd.push((e.id, x, speed));
    }
    *g.attempt_slot_mut() = Some(a);
    (g, crowd)
}

#[test]
fn enemies_in_one_lane_never_block_each_other() {
    let (mut g, crowd) = crowd(5);
    let (brute, runner) = (crowd[0].0, crowd[4].0);
    // Ticks each hostile needs to walk from its start to the line, plus one
    // for the contact step.
    let due: Vec<u64> = crowd
        .iter()
        .map(|(_, x, speed)| ((x - DEFENSE_LINE) / speed * 60.0).ceil() as u64 + 1)
        .collect();
    let start = g.attempt().unwrap().tick;
    let mut swapped = false;
    let mut contact_at = vec![None; crowd.len()];
    for _ in 0..=*due.iter().max().unwrap() {
        g.tick(Input::default());
        let a = g.attempt().expect("the defender cannot fall");
        let find = |id: u64| a.enemies.iter().find(|e| e.id == id).unwrap();
        for (n, (id, _, _)) in crowd.iter().enumerate() {
            let e = find(*id);
            assert_ne!(
                e.attack_phase,
                EnemyPhase::Queued,
                "hostile {id} queued behind another"
            );
            assert_eq!(e.pass_offset, 0.0, "hostile {id} swung out to pass");
            if contact_at[n].is_none() && e.contact_since.is_some() {
                assert!((e.x - DEFENSE_LINE).abs() <= 0.01, "contact off the line");
                assert_eq!(e.attack_phase, EnemyPhase::Windup);
                assert_eq!(e.target_structure, None);
                contact_at[n] = Some(a.tick - start);
            }
        }
        swapped |= find(runner).x < find(brute).x;
    }
    assert!(
        swapped,
        "the runner walked through the brute in front of it"
    );
    let a = g.attempt().unwrap();
    for (n, (id, _, _)) in crowd.iter().enumerate() {
        let e = a.enemies.iter().find(|e| e.id == *id).unwrap();
        assert!(
            (e.x - DEFENSE_LINE).abs() <= 0.01,
            "hostile {id} stopped at {} short of the line",
            e.x
        );
        let reached = contact_at[n].unwrap_or_else(|| panic!("hostile {id} never made contact"));
        assert!(
            reached <= due[n],
            "hostile {id}: contact at {reached} > {}",
            due[n]
        );
    }
}

#[test]
fn enemies_still_stop_at_structures() {
    let mut g = game(Mode::Personal);
    g.campaign_mut().blueprints = KindMap::filled(true);
    g.set_paused(true);
    g.place(StructureKind::Wall, 5, 0).unwrap();
    let (wall_id, stop) = {
        let s = &g.attempt().unwrap().structures[0];
        (s.id, s.x() + s.blocking_radius())
    };
    g.attempt_mut().unwrap().structures[0].hp = 1_000_000;
    g.attempt_mut().unwrap().structures[0].max_hp = 1_000_000;
    let mut a = g.attempt_slot_mut().take().unwrap();
    let mut ids = Vec::new();
    for (kind, x) in [
        (EnemyKind::Brute, stop + 0.4),
        (EnemyKind::Shambler, stop + 0.8),
        (EnemyKind::Runner, stop + 1.2),
        (EnemyKind::Runner, stop + 1.6),
    ] {
        g.spawn_enemy(&mut a, kind, 5, false);
        a.enemies.last_mut().unwrap().x = x;
        ids.push(a.enemies.last().unwrap().id);
    }
    *g.attempt_slot_mut() = Some(a);
    g.set_paused(false);
    step(&mut g, 300, 4);
    let a = g.attempt().unwrap();
    assert!(a.structures[0].hp > 0);
    for id in ids {
        let e = a.enemies.iter().find(|e| e.id == id).unwrap();
        assert!(
            (e.x - stop).abs() <= 0.01,
            "hostile {id} at {} instead of the wall face {stop}",
            e.x
        );
        assert_eq!(e.target_structure, Some(wall_id), "hostile {id}");
        assert!(e.contact_since.is_some(), "hostile {id} is not attacking");
        assert_ne!(e.attack_phase, EnemyPhase::Queued);
    }
}

#[test]
fn a_saved_queued_enemy_resolves_on_the_next_tick() {
    // Saves written before W1-69 can hold `"attack_phase": "Queued"`. The
    // variant stays for them; the next movement step resolves it.
    for (x, contact) in [(6.0, false), (DEFENSE_LINE, true)] {
        let mut g = game(Mode::Personal);
        let mut a = g.attempt_slot_mut().take().unwrap();
        g.spawn_enemy(&mut a, EnemyKind::Shambler, 3, false);
        let e = a.enemies.last_mut().unwrap();
        e.x = x;
        e.attack_phase = EnemyPhase::Queued;
        *g.attempt_slot_mut() = Some(a);
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains("\"Queued\""));
        let path = std::env::temp_dir().join(format!(
            "lastlight-queued-{}-{contact}.json",
            std::process::id()
        ));
        g.save(&path).unwrap();
        let mut g = Game::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            g.attempt().unwrap().enemies[0].attack_phase,
            EnemyPhase::Queued
        );
        g.set_paused(false);
        g.tick(Input::default());
        let e = &g.attempt().unwrap().enemies[0];
        if contact {
            assert_eq!(e.attack_phase, EnemyPhase::Windup);
            assert!(e.contact_since.is_some());
        } else {
            assert_eq!(e.attack_phase, EnemyPhase::Advance);
            assert!(e.x < x);
        }
    }
}

#[test]
fn faster_enemy_walks_through_the_slow_one_without_stopping_it_or_leaving_the_lane() {
    for lane in [0, 5, 10] {
        let (mut g, slow, fast) = fixture(lane);
        let mut crossed = false;
        for _ in 0..150 {
            let a = g.attempt().unwrap();
            let before = a.enemies.iter().find(|e| e.id == slow).unwrap().x;
            g.tick(Input::default());
            let a = g.attempt().unwrap();
            let slow = a.enemies.iter().find(|e| e.id == slow).unwrap();
            let fast = a.enemies.iter().find(|e| e.id == fast).unwrap();
            assert!(
                slow.x < before,
                "ordinary moving body must never stop its slower peer"
            );
            assert_eq!(slow.lane, lane);
            assert_eq!(fast.lane, lane);
            // No lateral passing: both stay on the lane centre and overlap.
            assert_eq!(fast.pass_offset, 0.0);
            assert_eq!(fast.y(), lane as f32 + 0.5);
            assert_eq!(slow.y(), fast.y());
            crossed |= fast.x < slow.x;
        }
        assert!(crossed);
    }
}

/// Both signs: pre-W1-69 saves hold negative offsets too (the slow body
/// swung to -MAX_PASS_OFFSET on middle lanes), and they must glide back to
/// the centre as smoothly as positive ones, never snap or overshoot.
#[test]
fn a_saved_passing_offset_eases_back_to_the_lane_centre() {
    for start in [0.32f32, -0.32] {
        let (mut g, _, fast) = fixture(5);
        let a = g.attempt_mut().unwrap();
        let e = a.enemies.iter_mut().find(|e| e.id == fast).unwrap();
        e.pass_offset = start;
        let mut previous = start;
        for _ in 0..30 {
            g.tick(Input::default());
            let e = g
                .attempt()
                .unwrap()
                .enemies
                .iter()
                .find(|e| e.id == fast)
                .unwrap()
                .pass_offset;
            assert!(
                e.abs() <= previous.abs()
                    && (previous - e).abs() <= 2.0 / 60.0 + 0.00001
                    && e * previous >= 0.0,
                "start {start}: {previous} -> {e}"
            );
            previous = e;
        }
        assert_eq!(previous, 0.0, "start {start}");
    }
}

#[test]
fn walls_hold_every_enemy_and_traps_still_trigger() {
    let mut g = legacy_game();
    g.campaign_mut().first_clear(0, Mode::Personal);
    g.campaign_mut().first_clear(1, Mode::Personal);
    g.campaign_mut().reward(1000, 0, Mode::Personal);
    g.campaign_mut()
        .purchase_blueprint(StructureKind::Wall)
        .unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Wall, 5, 0).unwrap();
    let wall = g.attempt().unwrap().structures[0].x() + 0.7;
    let mut a = g.attempt_slot_mut().take().unwrap();
    for (kind, x) in [(EnemyKind::Shambler, 6.3), (EnemyKind::Runner, 7.1)] {
        g.spawn_enemy(&mut a, kind, 5, false);
        a.enemies.last_mut().unwrap().x = x;
    }
    *g.attempt_slot_mut() = Some(a);
    step(&mut g, 240, 4);
    let a = g.attempt().unwrap();
    assert!(a.structures[0].hp > 0);
    assert!(
        a.enemies
            .iter()
            .filter(|e| e.lane == 5)
            .all(|e| e.x >= wall - 0.001)
    );
    // Both bodies reach the wall and attack it; neither waits behind the other.
    assert_eq!(
        a.enemies
            .iter()
            .filter(|e| e.target_structure == Some(a.structures[0].id))
            .count(),
        2
    );
    assert!(
        a.enemies
            .iter()
            .all(|e| e.attack_phase != EnemyPhase::Queued)
    );

    let mut g = legacy_game();
    g.campaign_mut().first_clear(0, Mode::Personal);
    g.campaign_mut().reward(25, 0, Mode::Personal);
    g.campaign_mut()
        .purchase_blueprint(StructureKind::Trap)
        .unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Trap, 5, 0).unwrap();
    let mut a = g.attempt_slot_mut().take().unwrap();
    for (kind, x) in [(EnemyKind::Shambler, 5.5), (EnemyKind::Runner, 6.2)] {
        g.spawn_enemy(&mut a, kind, 5, false);
        a.enemies.last_mut().unwrap().x = x;
    }
    *g.attempt_slot_mut() = Some(a);
    step(&mut g, 180, 4);
    let a = g.attempt().unwrap();
    assert_eq!(a.kills, 2);
    assert_eq!(a.structures[0].charges, 1);
    assert!(a.corpses.iter().all(|e| e.at[0] >= DEFENSE_LINE));
}

/// A save taken mid-easing (an old save's side step, W1-69) keeps its
/// `pass_offset`, and the easing back to the lane centre is identical at 1, 2
/// and 4 ticks per batch.
#[test]
fn a_saved_pass_offset_roundtrips_and_eases_identically_at_1_2_4x() {
    let (mut original, _, fast) = fixture(5);
    step(&mut original, 20, 1);
    let offset = |g: &Game| {
        g.attempt()
            .unwrap()
            .enemies
            .iter()
            .find(|e| e.id == fast)
            .unwrap()
            .pass_offset
    };
    let a = original.attempt_mut().unwrap();
    assert!(a.lane_count > 6, "lane 5 may step out either way");
    let e = a.enemies.iter_mut().find(|e| e.id == fast).unwrap();
    e.pass_offset = MAX_PASS_OFFSET;
    let path = std::env::temp_dir().join(format!("lastlight-pass-{}.json", std::process::id()));
    original.set_paused(true);
    let paused = serde_json::to_string(&original).unwrap();
    step(&mut original, 120, 4);
    assert_eq!(serde_json::to_string(&original).unwrap(), paused);
    original.save(&path).unwrap();
    let mut reference = None;
    for batch in [1, 2, 4] {
        let mut g = Game::load(&path).unwrap();
        assert_eq!(offset(&g), MAX_PASS_OFFSET, "saved mid-easing");
        g.set_paused(false);
        step(&mut g, 4, batch);
        assert!(
            (0.0..MAX_PASS_OFFSET).contains(&offset(&g)),
            "easing at batch {batch}: {}",
            offset(&g)
        );
        step(&mut g, 176, batch);
        assert_eq!(offset(&g), 0.0, "eased back at batch {batch}");
        let actual = serde_json::to_string(&g).unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&actual, expected);
        } else {
            reference = Some(actual);
        }
    }
    let mut invalid = original.clone();
    invalid.attempt_mut().unwrap().enemies[0].pass_offset = 1.0;
    assert!(invalid.save(&path).is_err());
    let mut old = serde_json::to_value(original.attempt().unwrap().enemies[0].clone()).unwrap();
    old.as_object_mut().unwrap().remove("pass_offset");
    assert_eq!(
        serde_json::from_value::<horde_core::sim::Enemy>(old)
            .unwrap()
            .pass_offset,
        0.0
    );
    std::fs::remove_file(path).unwrap();
}
