use battle_core::{
    Battle, MAX_UNITS, MapKind, Point, ScenarioConfig, UnitCategory, UnitKind, arena_half_extent,
};

#[test]
fn legacy_scenarios_default_to_creatures_and_all_categories_roundtrip() {
    let mut json = serde_json::to_value(ScenarioConfig::default()).unwrap();
    json["blue"].as_object_mut().unwrap().remove("category");
    json["red"].as_object_mut().unwrap().remove("category");
    let legacy = ScenarioConfig::from_json(&json.to_string()).unwrap();
    assert_eq!(legacy.blue.category, UnitCategory::Creature);
    for (index, category) in UnitCategory::ALL.into_iter().enumerate() {
        assert_eq!(category.index(), index);
        let mut config = legacy.clone();
        config.blue.category = category;
        config.red.category = category;
        assert_eq!(
            ScenarioConfig::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
        let definition = category.definition(UnitKind::Infantry);
        assert!(definition.health > 0.0 && definition.damage > 0.0 && definition.range > 0.0);
        assert_eq!(definition.speed == 0.0, category == UnitCategory::Building);
    }
    let million = ScenarioConfig::preset(4);
    assert_eq!(million.total_units(), MAX_UNITS);
    assert!(million.validate().is_ok());
}

#[test]
fn buildings_never_move_even_when_contacted() {
    let mut config = ScenarioConfig::default();
    config.blue.count = 1;
    config.red.count = 1;
    config.blue.category = UnitCategory::Building;
    let mut battle = Battle::new(config).unwrap();
    battle.units.positions[0] = Point::ZERO;
    battle.units.positions[1] = Point::new(0.25, 0.0);
    for _ in 0..80 {
        battle.step();
        assert_eq!(battle.units.positions[0], Point::ZERO);
        assert_eq!(battle.units.velocities[0], Point::ZERO);
    }
}

#[test]
fn elevation_prevents_ground_melee_hitting_space_units() {
    let mut config = ScenarioConfig::default();
    config.blue.count = 1;
    config.red.count = 1;
    config.red.category = UnitCategory::SpaceUnit;
    let mut battle = Battle::new(config).unwrap();
    let space_health = battle.units.health[1];
    battle.units.positions.fill(Point::ZERO);
    for _ in 0..60 {
        battle.step();
    }
    assert_eq!(battle.units.health[1], space_health);
    assert!(battle.units.health[0] < UnitKind::Infantry.definition().health);
    assert_eq!(UnitCategory::AirUnit.elevation(), 8.0);
    assert_eq!(UnitCategory::SpaceUnit.elevation(), 32.0);
    assert_eq!(UnitCategory::UndergroundUnit.elevation(), -3.0);
}

#[test]
fn airborne_units_cross_water_without_the_ground_portal() {
    let mut config = ScenarioConfig {
        map: MapKind::Crossing,
        ..Default::default()
    };
    config.blue.count = 1;
    config.red.count = 1;
    config.blue.category = UnitCategory::AirUnit;
    config.red.category = UnitCategory::AirUnit;
    let mut battle = Battle::new(config).unwrap();
    battle.units.positions[0] = Point::new(-6.0, 20.0);
    battle.units.positions[1] = Point::new(35.0, 20.0);
    for _ in 0..10 {
        battle.step();
    }
    assert!(battle.units.positions[0].x > -5.2);
    assert!(battle.units.positions[0].y > 19.0);
}

#[test]
fn strategic_units_move_each_tick_and_promoted_fronts_really_fight() {
    let mut config = ScenarioConfig::default();
    config.blue.count = 12_000;
    config.red.count = 12_000;
    let mut first = Battle::new(config.clone()).unwrap();
    let mut second = Battle::new(config).unwrap();
    for battle in [&mut first, &mut second] {
        battle.units.positions[0] = Point::new(-0.6, 0.0);
        battle.units.positions[12_000] = Point::new(0.6, 0.0);
        battle.units.health[0] = 1.0;
        battle.units.health[12_000] = 1.0;
    }
    let rear_before = first.units.positions[11_999];
    first.step();
    second.step();
    let rear_after_one = first.units.positions[11_999];
    assert_ne!(rear_before, rear_after_one);
    assert!(first.stats.full_detail_units > 0);
    assert!(first.stats.strategic_units > 0);
    first.step();
    second.step();
    assert_ne!(rear_after_one, first.units.positions[11_999]);
    for _ in 2..24 {
        first.step();
        second.step();
    }
    assert_eq!(first.units.positions, second.units.positions);
    assert_eq!(first.units.health, second.units.health);
    assert!(first.stats.casualties.iter().sum::<u32>() > 0);
    assert_eq!(first.stats.alive[0] + first.stats.casualties[0], 12_000);
    assert_eq!(first.stats.alive[1] + first.stats.casualties[1], 12_000);
    assert!(first.stats.neighbor_checks <= first.stats.full_detail_units as u64 * 112);
}

#[test]
fn million_units_are_independent_real_states_inside_the_arena() {
    let config = ScenarioConfig::preset(4);
    let extent = arena_half_extent(&config);
    let mut battle = Battle::new(config).unwrap();
    assert_eq!(battle.units.len(), 1_000_000);
    assert_eq!(battle.units.categories.len(), battle.units.len());
    assert_eq!(battle.units.health.len(), battle.units.len());
    assert!(battle.stats.estimated_state_bytes > 40_000_000);
    assert!(battle.stats.estimated_state_bytes < 200_000_000);
    for point in &battle.units.positions {
        assert!(point.x.is_finite() && point.y.is_finite());
        assert!(point.x.abs() < extent && point.y.abs() < extent);
    }
    battle.units.health[123_456] -= 1.0;
    assert_ne!(battle.units.health[123_456], battle.units.health[123_457]);
    battle.step();
    assert_eq!(battle.stats.alive, [500_000, 500_000]);
    assert_eq!(battle.stats.strategic_units, 1_000_000);
    assert_eq!(battle.stats.full_detail_units, 0);
    assert_eq!(battle.stats.neighbor_checks, 0);
}
