use battle_core::{
    Battle, Formation, MAX_UNITS, MapKind, Outcome, Point, SIM_DT, ScenarioConfig, Team, UnitKind,
    arena_half_extent, is_walkable,
};

fn small_config() -> ScenarioConfig {
    let mut config = ScenarioConfig::default();
    config.blue.count = 30;
    config.red.count = 30;
    config.time_limit_secs = 90;
    config
}

#[test]
fn identical_seed_and_commands_reproduce_simulation_state() {
    let mut first = Battle::new(small_config()).unwrap();
    let mut second = Battle::new(small_config()).unwrap();
    for _ in 0..500 {
        first.step();
        second.step();
    }
    assert_eq!(first.units.positions, second.units.positions);
    assert_eq!(first.units.health, second.units.health);
    assert_eq!(first.units.alive, second.units.alive);
    assert_eq!(first.stats.tick, second.stats.tick);
    assert_eq!(first.outcome, second.outcome);
    // step_ms is wall-clock telemetry and intentionally excluded.
}

#[test]
fn different_seed_changes_initial_conditions() {
    let first = Battle::new(small_config()).unwrap();
    let mut config = small_config();
    config.seed += 1;
    let second = Battle::new(config).unwrap();
    assert_ne!(first.units.positions, second.units.positions);
}

#[test]
fn living_and_casualty_counts_are_conserved_through_a_complete_battle() {
    let mut battle = Battle::new(small_config()).unwrap();
    for _ in 0..1800 {
        battle.step();
        for team in Team::ALL {
            let index = team.index();
            assert_eq!(
                battle.stats.alive[index] + battle.stats.casualties[index],
                30
            );
            let alive = battle
                .units
                .alive
                .iter()
                .enumerate()
                .filter(|&(i, value)| *value && battle.units.teams[i] == team)
                .count();
            assert_eq!(alive as u32, battle.stats.alive[index]);
        }
        if battle.outcome.is_some() {
            break;
        }
    }
    assert!(
        battle.stats.casualties.iter().sum::<u32>() > 0,
        "a battle must actually produce combat"
    );
    assert!(
        matches!(
            battle.outcome,
            Some(Outcome::Victory(_)) | Some(Outcome::Draw)
        ),
        "small open-field battle should finish before its time limit: {:?}",
        battle.stats
    );
    let stopped_tick = battle.stats.tick;
    battle.step();
    assert_eq!(
        battle.stats.tick, stopped_tick,
        "finished battles are inert"
    );
}

#[test]
fn simultaneous_lethal_attacks_allow_a_draw() {
    let mut config = small_config();
    config.blue.count = 1;
    config.red.count = 1;
    let mut battle = Battle::new(config).unwrap();
    // Hold at separate out-of-range positions until both cooldowns are ready.
    for _ in 0..25 {
        battle.step();
    }
    battle.units.positions[0] = Point::new(-0.6, 0.0);
    battle.units.positions[1] = Point::new(0.6, 0.0);
    battle.units.health.fill(1.0);
    battle.step();
    assert_eq!(battle.outcome, Some(Outcome::Draw));
    assert_eq!(battle.stats.alive, [0, 0]);
    assert_eq!(battle.stats.casualties, [1, 1]);
}

#[test]
fn crossing_routes_remain_on_walkable_ground_and_reach_combat() {
    let mut config = small_config();
    config.map = MapKind::Crossing;
    config.blue.count = 100;
    config.red.count = 100;
    let mut battle = Battle::new(config).unwrap();
    for _ in 0..900 {
        battle.step();
        for (i, point) in battle.units.positions.iter().enumerate() {
            assert!(
                is_walkable(MapKind::Crossing, point.x, point.y),
                "unit {i} entered water at {point:?}"
            );
            assert!(point.x.is_finite() && point.y.is_finite());
        }
    }
    assert!(
        battle.stats.casualties.iter().sum::<u32>() > 30,
        "both armies must find the bridge and fight: {:?}",
        battle.stats
    );
}

#[test]
fn every_formation_has_distinct_bounded_walkable_spawn_positions() {
    for formation in Formation::ALL {
        let mut config = ScenarioConfig {
            map: MapKind::Crossing,
            ..Default::default()
        };
        config.blue.count = 10_000;
        config.red.count = 10_000;
        config.blue.formation = formation;
        config.red.formation = formation;
        let extent = arena_half_extent(&config);
        let battle = Battle::new(config).unwrap();
        let mut occupied = std::collections::HashSet::new();
        for point in &battle.units.positions {
            assert!(point.x.abs() < extent && point.y.abs() < extent);
            assert!(is_walkable(MapKind::Crossing, point.x, point.y));
            // 0.5m bins are considerably smaller than the 1.15m spawn spacing.
            assert!(
                occupied.insert((
                    (point.x * 2.0).floor() as i32,
                    (point.y * 2.0).floor() as i32
                )),
                "overlapping spawn cell for {formation:?}"
            );
        }
    }
}

#[test]
fn dense_overlap_does_not_make_neighbor_work_quadratic_or_nonfinite() {
    let mut config = small_config();
    config.blue.count = 200;
    config.red.count = 200;
    let mut battle = Battle::new(config).unwrap();
    battle.units.positions.fill(Point::ZERO);
    battle.step();
    assert_eq!(battle.stats.max_cell_occupancy, 400);
    assert!(battle.stats.neighbor_checks <= battle.units.len() as u64 * (64 + 24 + 24));
    assert!(
        battle
            .units
            .positions
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite())
    );
}

#[test]
fn validation_rejects_zero_excessive_overflowing_and_invalid_duration() {
    let mut config = small_config();
    config.blue.count = 0;
    assert!(Battle::new(config.clone()).is_err());
    config.blue.count = MAX_UNITS;
    assert!(config.validate().is_err());
    config.blue.count = u32::MAX;
    assert!(config.validate().is_err());
    config.blue.count = 10;
    config.time_limit_secs = 0;
    assert!(config.validate().is_err());
    config.time_limit_secs = 100;
    config.name.clear();
    assert!(config.validate().is_err());
}

#[test]
fn scenario_json_is_validated_and_roundtrips() {
    for index in 0..5 {
        let config = ScenarioConfig::preset(index);
        let json = config.to_json().unwrap();
        assert_eq!(ScenarioConfig::from_json(&json).unwrap(), config);
    }
    assert!(ScenarioConfig::from_json("{}").is_err());
    let json = ScenarioConfig::default()
        .to_json()
        .unwrap()
        .replace("600", "20000000");
    assert!(ScenarioConfig::from_json(&json).is_err());
}

#[test]
fn ranged_units_attack_outside_melee_range() {
    let mut config = small_config();
    config.blue.count = 1;
    config.red.count = 1;
    config.blue.kind = UnitKind::Ranger;
    let mut battle = Battle::new(config).unwrap();
    for _ in 0..60 {
        battle.step();
        if battle.units.health[1] < UnitKind::Infantry.definition().health {
            let distance = (battle.units.positions[0] - battle.units.positions[1]).length();
            assert!(distance > UnitKind::Infantry.definition().range + 1.0);
            return;
        }
    }
    panic!("ranger failed to attack");
}

#[test]
fn fixed_clock_and_time_limit_are_explicit() {
    let mut config = small_config();
    config.time_limit_secs = 30;
    let mut battle = Battle::new(config).unwrap();
    battle.step();
    assert_eq!(battle.stats.tick, 1);
    assert_eq!(battle.stats.elapsed_secs, SIM_DT);
    // Keep adversaries separated to exercise the limit, not elimination.
    for _ in 1..600 {
        for (i, point) in battle.units.positions.iter_mut().enumerate() {
            point.x = if battle.units.teams[i] == Team::Blue {
                -50.0
            } else {
                50.0
            };
        }
        battle.step();
    }
    assert_eq!(battle.outcome, Some(Outcome::TimeLimit));
    assert_eq!(battle.stats.elapsed_secs, 30.0);
}
