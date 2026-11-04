mod support;
use horde_core::{
    content::{EnemyKind, Mode, StructureKind},
    sim::{DEFENSE_LINE, EnemyPhase, Game, Input, MAX_PASS_OFFSET},
};
use support::legacy_game;

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

#[test]
fn faster_enemy_smoothly_passes_without_stopping_the_slow_one_or_changing_lanes() {
    for lane in [0, 5, 10] {
        let (mut g, slow, fast) = fixture(lane);
        let mut crossed = false;
        let mut previous_offset = 0.0;
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
            assert_ne!(slow.id, fast.id);
            assert!((fast.pass_offset - previous_offset).abs() <= 2.0 / 60.0 + 0.00001);
            previous_offset = fast.pass_offset;
            assert!(fast.pass_offset.abs() <= MAX_PASS_OFFSET);
            assert!((0.5..=10.5).contains(&fast.y()));
            assert_eq!(fast.y().floor() as u8, lane);
            if (fast.x - slow.x).abs() < 0.025 {
                assert!(
                    (fast.y() - slow.y()).abs() >= if lane == 5 { 0.5 } else { 0.25 },
                    "passing must have real lateral separation"
                );
                crossed = true;
            }
        }
        let a = g.attempt().unwrap();
        assert!(crossed);
        assert!(
            a.enemies.iter().find(|e| e.id == fast).unwrap().x
                < a.enemies.iter().find(|e| e.id == slow).unwrap().x
        );
    }
}

#[test]
fn wall_and_contact_queues_still_block_fast_runners_and_traps_cover_the_passing_width() {
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
    assert_eq!(
        a.enemies
            .iter()
            .filter(|e| e.target_structure == Some(a.structures[0].id))
            .count(),
        1
    );
    assert!(
        a.enemies
            .iter()
            .any(|e| e.attack_phase == EnemyPhase::Queued)
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

#[test]
fn passing_state_roundtrips_and_produces_identical_fixed_ticks_at_1_2_4x() {
    let (mut original, _, _) = fixture(5);
    step(&mut original, 20, 1);
    let path = std::env::temp_dir().join(format!("lastlight-pass-{}.json", std::process::id()));
    original.set_paused(true);
    let paused = serde_json::to_string(&original).unwrap();
    step(&mut original, 120, 4);
    assert_eq!(serde_json::to_string(&original).unwrap(), paused);
    original.save(&path).unwrap();
    let mut reference = None;
    for batch in [1, 2, 4] {
        let mut g = Game::load(&path).unwrap();
        g.set_paused(false);
        step(&mut g, 180, batch);
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
