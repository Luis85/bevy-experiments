//! Finite, reproducible representation and simulation-work probe.
//! Run from the workspace root: cargo run -p terra-sandbox --example scaling_probe --locked
use serde_json::json;
use std::{collections::BTreeSet, time::Instant};
use terra_core::{ActorKind, GeoPosition, LocalFrame, WeatherSample};
use terra_sandbox::*;

fn at(frame: &LocalFrame, east: f64, north: f64) -> GeoPosition {
    frame
        .to_geo([east, 0., -north])
        .expect("finite fixture coordinate")
}
fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = GeoPosition::new(48.137, 11.575, 100.)?;
    let frame = LocalFrame::new(origin);
    let points: Vec<_> = (0..100_000_u64)
        .map(|i| {
            (
                i,
                at(&frame, (i % 320) as f64 * 75., (i / 320) as f64 * 75.),
            )
        })
        .collect();
    let centers: Vec<_> = (0..64_u64)
        .map(|i| {
            at(
                &frame,
                (i * 29 % 310) as f64 * 75.,
                (i * 47 % 300) as f64 * 75.,
            )
        })
        .collect();
    let started = Instant::now();
    let index = GeoSpatialIndex::from_positions(128., points.iter().copied())?;
    let index_build_us = micros(started);
    let started = Instant::now();
    let queries: Vec<_> = centers
        .iter()
        .map(|center| index.query(*center, 250.))
        .collect::<Result<_, _>>()?;
    let index_query_us = micros(started);
    let candidates: usize = queries.iter().map(|q| q.candidates_tested).sum();
    let started = Instant::now();
    for (center, result) in centers.iter().zip(&queries) {
        let scan: Vec<_> = points
            .iter()
            .filter(|(_, p)| center.surface_distance_m(*p) <= 250.)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(scan, result.ids);
    }
    let linear_query_us = micros(started);
    assert!(candidates < points.len() * centers.len() / 100);

    let catalog = Catalog::builtin();
    let mut state = SandboxState::new();
    for player in 1..=64_u64 {
        state.add_player(
            player,
            at(
                &frame,
                ((player - 1) % 8) as f64 * 350.,
                ((player - 1) / 8) as f64 * 350.,
            ),
        )?;
    }
    state.seed_living_demo(origin, &[], &catalog)?;
    state.seed_ecology_near(origin, &catalog)?;
    let template = state
        .living
        .units
        .values()
        .find(|u| u.role == UnitRole::Citizen)
        .unwrap()
        .clone();
    while state.living.units.len() < MAX_LIVING_UNITS {
        let id = state.living.next_id;
        state.living.next_id += 1;
        let mut unit = template.clone();
        unit.id = id;
        unit.name = format!("Probe resident {id}");
        unit.position = at(
            &frame,
            (id % 32) as f64 * 250.,
            (id / 32 % 32) as f64 * 250.,
        );
        unit.home = unit.position;
        unit.order = UnitOrder::Idle;
        unit.path.clear();
        unit.patrol.clear();
        unit.path_index = 0;
        state.living.units.insert(id, unit);
    }
    for i in 0..MAX_ENTITIES - state.resource_nodes.len() {
        state.seed_resource(
            format!("probe:{i:05}"),
            at(&frame, (i % 100) as f64 * 75., (i / 100) as f64 * 75.),
            "stone",
            20,
            0.,
        )?;
    }
    state.validate_with_catalog(&catalog)?;
    let started = Instant::now();
    let baseline: Vec<_> = (1..=64)
        .map(|player| state.snapshot_for(player, 250.))
        .collect::<Result<_, _>>()?;
    let linear_snapshot_us = micros(started);
    let started = Instant::now();
    let interest = SandboxInterestIndex::new(&state)?;
    let interest_build_us = micros(started);
    let started = Instant::now();
    let indexed: Vec<_> = (1..=64)
        .map(|player| state.snapshot_for_indexed(player, 250., &interest))
        .collect::<Result<_, _>>()?;
    let indexed_snapshot_us = micros(started);
    let mut total_bytes = 0;
    for (left, right) in baseline.iter().zip(indexed.iter()) {
        let left = serde_json::to_vec(left)?;
        let right = serde_json::to_vec(right)?;
        assert_eq!(left, right);
        total_bytes += right.len();
    }
    drop(interest);

    // A separate real domain fixture demonstrates budgeted distant behavior.
    // Physical movement, needs, resources and production still advance each step.
    let mut ecology = SandboxState::new();
    ecology.add_player(1, origin)?;
    ecology.seed_living_demo(origin, &[], &catalog)?;
    ecology.seed_ecology_near(origin, &catalog)?;
    for (i, animal) in ecology.ecology.animals.values_mut().enumerate() {
        ecology
            .living
            .units
            .get_mut(&animal.unit_id)
            .unwrap()
            .position = at(
            &frame,
            700. + (i % 6) as f64 * 90.,
            -450. + (i / 6) as f64 * 90.,
        );
        animal.decision_remaining_seconds = 0.;
    }
    let mut scheduler = EcologyScheduler::default();
    let weather = WeatherSample::default();
    let observers: Vec<_> = ecology
        .living
        .units
        .values()
        .filter(|u| u.owner.is_some())
        .map(|u| u.position)
        .collect();
    let started = Instant::now();
    for _ in 0..1000 {
        ecology.advance(3., &weather)?;
        ecology.advance_ecology_scheduled_with_environment(
            0.05,
            3.,
            12.,
            &mut scheduler,
            &observers,
            Some,
        )?;
        ecology.advance_living_with_environment(0.05, 3., &catalog, |_, _, candidate| {
            Some(candidate)
        })?;
        ecology.advance_settlements(3., &catalog)?;
    }
    let domain_steps_us = micros(started);
    ecology.validate_with_catalog(&catalog)?;
    let animal_count = ecology.ecology.animals.len();
    let stats = scheduler.stats();
    let owned: BTreeSet<_> = ecology
        .living
        .units
        .values()
        .filter(|u| u.owner.is_some())
        .map(|u| u.id)
        .collect();
    assert_eq!(owned.len(), 5);
    assert!(
        ecology
            .living
            .units
            .values()
            .all(|u| u.position.validate().is_ok())
    );
    assert!(
        ecology
            .living
            .units
            .values()
            .any(|u| u.kind == ActorKind::Animal)
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope": "finite domain and index probe; not a network concurrency, collision-world, or MMO benchmark",
            "build": if cfg!(debug_assertions) { "workspace dev profile, opt-level=1" } else { "release profile" },
            "index_only": {
                "entries": points.len(), "queries": centers.len(), "radius_m": 250,
                "build_us": index_build_us, "indexed_query_us": index_query_us,
                "linear_query_us": linear_query_us, "candidate_tests": candidates,
                "linear_distance_tests": points.len() * centers.len(), "identical_results": true
            },
            "authoritative_snapshot_batch": {
                "registered_players": state.players.len(), "living_bodies": state.living.units.len(),
                "resource_nodes": state.resource_nodes.len(), "views": 64,
                "linear_snapshot_us": linear_snapshot_us, "index_build_us": interest_build_us,
                "indexed_snapshot_us": indexed_snapshot_us, "snapshot_bytes": total_bytes,
                "byte_identical_views": true, "includes_wire_budget_and_private_filtering": true,
                "network_connections": 0
            },
            "ecology_domain": {
                "fixed_steps": 1000, "physical_seconds": 50, "calendar_seconds": 3000,
                "observed_bodies": ecology.living.units.len(), "managed_animals": animal_count,
                "elapsed_us": domain_steps_us, "schedule": stats,
                "full_pass_animal_evaluations": stats.passes * animal_count as u64,
                "production_cycles": ecology.living.metrics.production_cycles,
                "physical_deliveries": ecology.living.metrics.deliveries_completed,
                "traffic_queries": ecology.living.metrics.traffic_queries,
                "traffic_candidate_tests": ecology.living.metrics.traffic_candidate_tests,
                "environment": "permissive finite-coordinate motion resolver; imported collision not included"
            }
        }))?
    );
    Ok(())
}
