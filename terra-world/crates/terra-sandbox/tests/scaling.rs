use std::collections::{BTreeMap, BTreeSet};
use terra_core::{ActorKind, FeatureKind, GeoPosition, LocalFrame, WeatherSample};
use terra_sandbox::*;

fn origin() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 100.).unwrap()
}
fn at(east: f64, north: f64) -> GeoPosition {
    LocalFrame::new(origin())
        .to_geo([east, 0., -north])
        .unwrap()
}
fn world() -> (SandboxState, Catalog) {
    let mut state = SandboxState::new();
    let catalog = Catalog::builtin();
    state.add_player(1, origin()).unwrap();
    state.add_player(2, at(20., 10.)).unwrap();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    state.seed_ecology_near(origin(), &catalog).unwrap();
    (state, catalog)
}
fn far_wildlife() -> (SandboxState, Catalog) {
    let (mut state, catalog) = world();
    for (i, animal) in state.ecology.animals.values_mut().enumerate() {
        let p = at(700. + (i % 6) as f64 * 90., -450. + (i / 6) as f64 * 90.);
        state
            .living
            .units
            .get_mut(&animal.unit_id)
            .unwrap()
            .position = p;
        animal.decision_remaining_seconds = 0.;
    }
    state.ecology.config.update_interval_seconds = 0.25;
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog)
}

#[test]
fn sphere_grid_matches_scan_across_dateline_poles_and_altitudes() {
    let mut positions = vec![
        (0, GeoPosition::new(0., 179.9999, -200.).unwrap()),
        (1, GeoPosition::new(0., -179.9999, 90_000.).unwrap()),
        (2, GeoPosition::new(89.9999, 90., 0.).unwrap()),
        (3, GeoPosition::new(89.9999, -90., 20_000.).unwrap()),
        (4, GeoPosition::new(-89.9999, -179., 0.).unwrap()),
    ];
    for i in 5..500_u64 {
        positions.push((
            i,
            GeoPosition::new(
                (i * 37 % 180) as f64 - 90.,
                (i * 73 % 360) as f64 - 180.,
                500.,
            )
            .unwrap(),
        ));
    }
    let index = GeoSpatialIndex::from_positions(64., positions.iter().copied()).unwrap();
    for center in positions.iter().take(5).map(|(_, p)| *p) {
        for radius in [0., 32., 500., 5_000., 22_000_000.] {
            let wanted: Vec<_> = positions
                .iter()
                .filter(|(_, p)| center.surface_distance_m(*p) <= radius)
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(index.query(center, radius).unwrap().ids, wanted);
        }
    }
    assert!(index.query(positions[0].1, 32.).unwrap().ids.contains(&1));
    assert!(index.query(positions[2].1, 32.).unwrap().ids.contains(&3));
}

#[test]
fn spatial_inputs_and_duplicate_keys_reject_without_unbounded_grid_walks() {
    assert!(GeoSpatialIndex::<u64>::from_positions(0., []).is_err());
    assert!(GeoSpatialIndex::from_positions(32., [(1, origin()), (1, at(1., 1.))]).is_err());
    let index = GeoSpatialIndex::from_positions(32., [(1, origin())]).unwrap();
    assert!(index.query(origin(), f64::NAN).is_err());
    assert!(index.query(origin(), -1.).is_err());
    assert_eq!(
        index.query(origin(), f64::MAX).unwrap().candidates_tested,
        1
    );
    assert_eq!(index.query(origin(), f64::MAX).unwrap().ids, vec![1]);
}

#[test]
fn index_reduces_candidates_without_changing_large_point_query_results() {
    // An index-only representation probe, not a claim about full actor scale.
    let points: Vec<_> = (0..10_000_u64)
        .map(|i| (i, at((i % 100) as f64 * 75., (i / 100) as f64 * 75.)))
        .collect();
    let index = GeoSpatialIndex::from_positions(128., points.iter().copied()).unwrap();
    let center = at(3750., 3750.);
    let query = index.query(center, 160.).unwrap();
    let scan: Vec<_> = points
        .iter()
        .filter(|(_, p)| center.surface_distance_m(*p) <= 160.)
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(query.ids, scan);
    assert!(query.candidates_tested < points.len() / 100);
}

#[test]
fn indexed_snapshot_matches_all_overlay_layers_and_private_actor_rules() {
    let (mut state, catalog) = world();
    for i in 0..400 {
        state
            .seed_resource(
                format!("probe:{i:04}"),
                at((i % 20) as f64 * 4., (i / 20) as f64 * 4.),
                "stone",
                20,
                0.,
            )
            .unwrap();
    }
    state
        .resource_nodes
        .get_mut("probe:0000")
        .unwrap()
        .remaining = 3;
    state.plots.insert(
        30,
        LandPlot {
            id: 30,
            owner: 1,
            faction_owner: None,
            center: origin(),
            radius_m: 10.,
        },
    );
    let bp = &catalog.blueprints["house"];
    state.structures.insert(
        31,
        Structure {
            id: 31,
            owner: 1,
            faction_owner: None,
            blueprint: "house".into(),
            position: origin(),
            hitpoints: bp.hitpoints,
            max_hitpoints: bp.hitpoints,
            radius_m: bp.radius_m,
            housing: bp.housing,
            workbench: bp.workbench,
        },
    );
    let crop = &catalog.crops["wheat"];
    state.farms.insert(
        32,
        FarmPlot {
            id: 32,
            owner: 1,
            faction_owner: None,
            plot_id: 30,
            position: origin(),
            crop: "wheat".into(),
            growth_seconds: 0.,
            required_growth_seconds: crop.growth_seconds,
            moisture: 0.5,
            harvest_item: crop.harvest_item.clone(),
            harvest_amount: crop.harvest_amount,
        },
    );
    state.world_features.insert(
        "removed-tree".into(),
        FeatureReference {
            position: origin(),
            radius_m: 0.,
            kind: FeatureKind::Tree,
        },
    );
    state.feature_edits.insert("removed-tree".into());
    let owned = state.living.accounts[&1].avatar_unit;
    state.set_player_position(1, origin()).unwrap();
    let crew = state
        .living
        .units
        .values()
        .find(|u| u.owner == Some(1) && u.id != owned)
        .unwrap()
        .id;
    state.living.units.get_mut(&crew).unwrap().position = at(12_000., 0.);
    let before = state.clone();
    let index = SandboxInterestIndex::new(&state).unwrap();
    for radius in [0., 40., 500., 5000.] {
        let legacy = state.snapshot_for(1, radius).unwrap();
        let indexed = state.snapshot_for_indexed(1, radius, &index).unwrap();
        assert_eq!(
            serde_json::to_value(&legacy).unwrap(),
            serde_json::to_value(&indexed).unwrap()
        );
        assert!(indexed.living.units.iter().any(|u| u.id == crew));
        assert!(
            indexed
                .living
                .units
                .iter()
                .all(|u| u.pocket_money == 0 && u.supplies.is_empty() && u.path.is_empty())
        );
        assert_eq!(indexed.living.account.unwrap().player_id, 1);
    }
    assert_eq!(state, before);
}

#[test]
fn interest_index_rejects_other_world_and_rebuild_tracks_removal() {
    let (mut state, _) = world();
    let other = state.clone();
    {
        let index = SandboxInterestIndex::new(&state).unwrap();
        assert!(other.snapshot_for_indexed(1, 500., &index).is_err());
        assert!(state.snapshot_for_indexed(1, f64::NAN, &index).is_err());
    }
    let id = state
        .living
        .units
        .values()
        .find(|u| u.owner.is_none() && u.kind == ActorKind::Human)
        .unwrap()
        .id;
    state.living.units.remove(&id);
    let index = SandboxInterestIndex::new(&state).unwrap();
    let view = state.snapshot_for_indexed(1, 5000., &index).unwrap();
    assert!(view.living.units.iter().all(|u| u.id != id));
}

#[test]
fn far_decision_budget_is_fair_and_avoids_scanning_every_threat() {
    let (mut state, _) = far_wildlife();
    let mut scheduler = EcologyScheduler::new(EcologyScheduleConfig {
        near_radius_m: 20.,
        far_interval_seconds: 2.,
        max_far_decisions_per_pass: 2,
    })
    .unwrap();
    let mut evaluated = BTreeSet::new();
    let count = state.ecology.animals.len();
    for _ in 0..16 {
        state
            .advance_ecology_scheduled_with_environment(
                0.25,
                0.,
                12.,
                &mut scheduler,
                &[origin()],
                Some,
            )
            .unwrap();
        let stats = scheduler.stats();
        assert_eq!(stats.near_decisions, 0);
        assert!(stats.far_decisions <= 2);
        assert!(stats.evaluated < count);
        assert!(stats.threat_candidates < count * 3);
        evaluated.extend(
            state
                .ecology
                .animals
                .values()
                .filter(|a| a.decision_remaining_seconds > 0.)
                .map(|a| a.unit_id),
        );
    }
    assert_eq!(evaluated.len(), count, "far identities must not starve");
    assert_eq!(scheduler.stats().passes, 16);
    assert!(scheduler.stats().deferred_total > scheduler.stats().evaluated_total);
}

#[test]
fn approaching_observer_and_vehicle_threat_bypass_far_budget() {
    let (mut state, _) = far_wildlife();
    let mut scheduler = EcologyScheduler::new(EcologyScheduleConfig {
        near_radius_m: 20.,
        far_interval_seconds: 30.,
        max_far_decisions_per_pass: 1,
    })
    .unwrap();
    let ids: Vec<_> = state.ecology.animals.keys().copied().collect();
    let id = *ids.last().unwrap();
    let position = state.living.units[&id].position;
    state
        .advance_ecology_scheduled_with_environment(
            0.25,
            0.,
            12.,
            &mut scheduler,
            &[origin()],
            Some,
        )
        .unwrap();
    assert_eq!(state.ecology.animals[&id].decision_remaining_seconds, 0.);
    state
        .advance_ecology_scheduled_with_environment(
            0.25,
            0.,
            12.,
            &mut scheduler,
            &[position],
            Some,
        )
        .unwrap();
    assert!(state.ecology.animals[&id].decision_remaining_seconds > 0.);
    assert!(scheduler.stats().near_decisions > 0);
    let vehicle = state
        .living
        .units
        .values()
        .find(|u| u.owner.is_none() && u.kind == ActorKind::Vehicle)
        .unwrap()
        .id;
    state.living.units.get_mut(&vehicle).unwrap().position =
        LocalFrame::new(position).to_geo([4., 0., 0.]).unwrap();
    state
        .ecology
        .animals
        .get_mut(&id)
        .unwrap()
        .decision_remaining_seconds = 120.;
    state
        .advance_ecology_scheduled_with_environment(
            0.25,
            0.,
            12.,
            &mut scheduler,
            &[origin()],
            Some,
        )
        .unwrap();
    assert_eq!(
        state.ecology.animals[&id].behavior,
        WildlifeBehavior::Fleeing
    );
    assert!(scheduler.stats().near_decisions > 0);
}

#[test]
fn persisted_player_does_not_grant_near_priority_without_an_active_observer() {
    let (mut state, _) = far_wildlife();
    // The body is within the near tier but outside every species' flee range.
    let player_position = at(580., -450.);
    state.set_player_position(1, player_position).unwrap();
    let mut scheduler = EcologyScheduler::new(EcologyScheduleConfig {
        max_far_decisions_per_pass: 1,
        ..Default::default()
    })
    .unwrap();
    state
        .advance_ecology_scheduled_with_environment(0.25, 0., 12., &mut scheduler, &[], Some)
        .unwrap();
    assert_eq!(scheduler.stats().near_decisions, 0);
    assert_eq!(scheduler.stats().far_decisions, 1);
    state
        .advance_ecology_scheduled_with_environment(
            0.25,
            0.,
            12.,
            &mut scheduler,
            &[player_position],
            Some,
        )
        .unwrap();
    assert!(scheduler.stats().near_decisions > 0);
}

#[test]
fn scheduling_preserves_full_rate_resources_needs_production_and_physical_motion() {
    let (mut scheduled, catalog) = far_wildlife();
    let mut baseline = scheduled.clone();
    let mut scheduler = EcologyScheduler::new(EcologyScheduleConfig {
        max_far_decisions_per_pass: 1,
        ..Default::default()
    })
    .unwrap();
    let node = "ecology:starter:berries";
    scheduled.resource_nodes.get_mut(node).unwrap().remaining = 1;
    baseline.resource_nodes.get_mut(node).unwrap().remaining = 1;
    let weather = WeatherSample::default();
    for _ in 0..100 {
        for world in [&mut scheduled, &mut baseline] {
            world.advance(6., &weather).unwrap();
        }
        scheduled
            .advance_ecology_scheduled_with_environment(
                0.05,
                6.,
                12.,
                &mut scheduler,
                &[origin()],
                Some,
            )
            .unwrap();
        baseline.advance_ecology(0.05, 6., 12.).unwrap();
        for world in [&mut scheduled, &mut baseline] {
            // The same fixed-step physical resolver still checks every body.
            world
                .advance_living_with_environment(0.05, 6., &catalog, |_, _, p| Some(p))
                .unwrap();
        }
    }
    assert_eq!(scheduled.resource_nodes, baseline.resource_nodes);
    assert_eq!(scheduled.farms, baseline.farms);
    assert_eq!(scheduled.living.producers, baseline.living.producers);
    assert_eq!(scheduled.living.shops, baseline.living.shops);
    assert_eq!(scheduled.living.accounts, baseline.living.accounts);
    assert_eq!(scheduled.living.deliveries, baseline.living.deliveries);
    for (id, body) in &scheduled.living.units {
        if body.kind != ActorKind::Animal {
            assert_eq!(body, &baseline.living.units[id]);
        }
    }
    assert_eq!(
        scheduled.ecology.elapsed_seconds,
        baseline.ecology.elapsed_seconds
    );
    for (id, animal) in &scheduled.ecology.animals {
        assert_eq!(animal.age_seconds, baseline.ecology.animals[id].age_seconds);
    }
}

#[test]
fn scheduled_unknown_ground_rejects_targets_and_births() {
    let (mut state, _) = far_wildlife();
    let removed = *state.ecology.animals.keys().next().unwrap();
    state.living.units.remove(&removed);
    state.ecology.animals.remove(&removed);
    for habitat in state.ecology.habitats.values_mut() {
        habitat.initial_population_seeded = false;
    }
    let observers: Vec<_> = state
        .ecology
        .animals
        .keys()
        .map(|id| state.living.units[id].position)
        .collect();
    let count = state.living.units.len();
    let mut scheduler = EcologyScheduler::default();
    state
        .advance_ecology_scheduled_with_environment(
            1.,
            100.,
            12.,
            &mut scheduler,
            &observers,
            |_| None,
        )
        .unwrap();
    assert_eq!(state.living.units.len(), count);
    for id in state.ecology.animals.keys() {
        let unit = &state.living.units[id];
        assert!(unit.path.is_empty());
        assert!(!matches!(unit.order, UnitOrder::Move { .. }));
    }
}

#[test]
fn schedule_inputs_reject_atomically_and_clock_rewind_resets_runtime_budget() {
    assert!(
        EcologyScheduler::new(EcologyScheduleConfig {
            max_far_decisions_per_pass: 0,
            ..Default::default()
        })
        .is_err()
    );
    let (mut state, _) = far_wildlife();
    let mut scheduler = EcologyScheduler::default();
    let before = state.clone();
    let bad = GeoPosition {
        latitude: f64::NAN,
        ..origin()
    };
    assert!(
        state
            .advance_ecology_scheduled_with_environment(1., 1., 12., &mut scheduler, &[bad], Some)
            .is_err()
    );
    assert_eq!(state, before);
    state
        .advance_ecology_scheduled_with_environment(1., 20., 12., &mut scheduler, &[origin()], Some)
        .unwrap();
    state
        .advance_ecology_scheduled_with_environment(1., 20., 12., &mut scheduler, &[origin()], Some)
        .unwrap();
    assert_eq!(scheduler.stats().passes, 2);
    state = before;
    state
        .advance_ecology_scheduled_with_environment(1., 0., 12., &mut scheduler, &[origin()], Some)
        .unwrap();
    assert_eq!(scheduler.stats().passes, 1);
    scheduler.reset();
    assert_eq!(scheduler.stats(), &EcologyScheduleStats::default());
}

fn vehicle(id: u64, position: GeoPosition) -> LivingUnit {
    let destination = LocalFrame::new(position).to_geo([100., 0., 0.]).unwrap();
    LivingUnit {
        id,
        name: format!("Vehicle {id}"),
        kind: ActorKind::Vehicle,
        owner: None,
        role: UnitRole::Traffic,
        position,
        heading_deg: 90.,
        velocity_enu_m_s: [8., 0., 0.],
        needs: Needs::default(),
        order: UnitOrder::Move { destination },
        home: position,
        path: vec![destination],
        path_index: 0,
        patrol: vec![],
        patrol_reverse: false,
        pocket_money: 0,
        supplies: BTreeMap::new(),
    }
}

#[test]
fn traffic_broad_phase_preserves_yielding_and_bounds_separated_vehicle_work() {
    let mut state = SandboxState::new();
    state.living.enabled = true;
    state.living.origin = Some(origin());
    state.living.next_id = 1_001_000;
    for i in 0..MAX_LIVING_UNITS {
        let id = 1_000_000 + i as u64;
        state.living.units.insert(
            id,
            vehicle(id, at((i % 32) as f64 * 100., (i / 32) as f64 * 100.)),
        );
    }
    state.living.units.get_mut(&1_000_001).unwrap().position = at(4., 0.);
    state.living.units.get_mut(&1_000_001).unwrap().path = vec![at(104., 0.)];
    state.validate().unwrap();
    let before = state.living.units[&1_000_000].position;
    state.advance_living(0.05, 0., &Catalog::builtin()).unwrap();
    assert_eq!(state.living.units[&1_000_000].position, before);
    assert_eq!(state.living.units[&1_000_000].velocity_enu_m_s, [0.; 3]);
    assert_eq!(
        state.living.metrics.traffic_queries,
        MAX_LIVING_UNITS as u64
    );
    assert!(state.living.metrics.traffic_candidate_tests < MAX_LIVING_UNITS as u64 * 4);
    assert!(
        state.living.units[&1_000_002]
            .position
            .surface_distance_m(at(200., 0.))
            > 0.3
    );
}

#[test]
fn prior_living_metrics_load_with_zero_spatial_work_counters() {
    let (state, catalog) = world();
    let mut value = serde_json::to_value(&state).unwrap();
    value["living"]["metrics"]
        .as_object_mut()
        .unwrap()
        .remove("traffic_queries");
    value["living"]["metrics"]
        .as_object_mut()
        .unwrap()
        .remove("traffic_candidate_tests");
    let restored: SandboxState = serde_json::from_value(value).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    assert_eq!(restored.living.metrics.traffic_queries, 0);
    assert_eq!(restored.living.metrics.traffic_candidate_tests, 0);
}
