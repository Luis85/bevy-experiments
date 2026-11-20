//! Finite storage/working-set evidence; no sockets, renderer or live providers.
use serde_json::json;
use std::{collections::BTreeSet, error::Error, time::Instant};
use terra_core::{GeoPosition, WeatherSample};
use terra_persistence::{RegionKey, RegionStore, RegionWorker};
use terra_sandbox::{Catalog, SandboxState, UnitOrder, UnitRole, ValidatedRegionPartition};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = match args.next().as_deref() {
        None => None,
        Some("--output") => Some(args.next().ok_or("--output requires a path")?),
        _ => return Err("usage: regional_probe [--output report.json]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let origin = GeoPosition::new(48.137, 11.575, 100.)?;
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    world.add_player(1, origin)?;
    world.seed_living_demo(origin, &[], &catalog)?;
    let template = world
        .living
        .units
        .values()
        .find(|u| u.owner.is_none() && u.role == UnitRole::Citizen)
        .ok_or("resident fixture missing")?
        .clone();
    for region in 0..16 {
        let center = GeoPosition::new(
            48.137 + region as f64 * 0.04,
            11.575 + region as f64 * 0.06,
            100.,
        )?;
        for node in 0..400 {
            let id = format!("probe-deposit-{region}-{node}");
            world.seed_resource(
                id.clone(),
                center,
                "stone",
                100,
                if node % 2 == 0 { 0. } else { 30. },
            )?;
            world.resource_nodes.get_mut(&id).unwrap().remaining = 7;
        }
        for _ in 0..12 {
            let mut unit = template.clone();
            unit.id = world.living.next_id;
            world.living.next_id += 1;
            unit.name = format!("Regional resident {}", unit.id);
            unit.position = center;
            unit.home = center;
            unit.order = UnitOrder::Idle;
            unit.path.clear();
            unit.path_index = 0;
            unit.patrol.clear();
            unit.pocket_money = 300;
            world.living.units.insert(unit.id, unit);
        }
    }
    world.validate_with_catalog(&catalog)?;
    let full_bodies = world.living.units.len();
    let full_nodes = world.resource_nodes.len();
    let original = world.clone();
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("probe.db");
    let mut store = RegionStore::open(
        &database,
        json!({"kind":"regional-probe","source":"synthetic-fixture-1"}),
        &catalog,
        16,
    )?;
    let desired = BTreeSet::from([RegionKey::at(origin, 16)?]);
    let start = Instant::now();
    let initial = store.checkpoint(&mut world, json!({"simulation_tick":0}), &desired)?;
    let initial_ms = start.elapsed().as_secs_f64() * 1000.;
    if store.export(&world)? != original {
        return Err("initial regional roundtrip differs".into());
    }
    let active_bodies = world.living.units.len();
    let active_nodes = world.resource_nodes.len();
    let weather = WeatherSample::default();
    let simulate = |state: &mut SandboxState| -> Result<f64, Box<dyn Error>> {
        let started = Instant::now();
        for _ in 0..500 {
            state.advance(0.5, &weather)?;
            state.advance_living(0.05, 0.5, &catalog)?;
        }
        Ok(started.elapsed().as_secs_f64() * 1000.)
    };
    let mut full_simulation = original.clone();
    let full_simulation_ms = simulate(&mut full_simulation)?;
    let active_simulation_ms = simulate(&mut world)?;
    let full_for_scope_probe = store.export(&world)?;
    let proof = ValidatedRegionPartition::from_state(&full_for_scope_probe, 16, &catalog)?;
    let selected = proof.select(&desired)?;
    let mut full_scope_ms = Vec::new();
    let mut certified_scope_ms = Vec::new();
    let mut scope_work = None;
    for sample in 0..16 {
        // Alternate measurement order; canonical equality is checked outside
        // each timed operation. This compares the same fixture and revision.
        let full = || -> Result<_, terra_sandbox::SandboxError> {
            let start = Instant::now();
            let merged = proof.merge_validating_all(&selected.state, &selected.active_regions)?;
            Ok((merged, start.elapsed().as_secs_f64() * 1000.))
        };
        let certified = || -> Result<_, terra_sandbox::SandboxError> {
            let start = Instant::now();
            let merged = proof.merge(&selected.state, &selected.active_regions)?;
            Ok((merged, start.elapsed().as_secs_f64() * 1000.))
        };
        let (full, certified) = if sample % 2 == 0 {
            (full()?, certified()?)
        } else {
            let certified = certified()?;
            (full()?, certified)
        };
        if *full.0 != *certified.0 {
            return Err("certified merge differs from complete reference".into());
        }
        scope_work = Some(certified.0.validation_work());
        full_scope_ms.push(full.1);
        certified_scope_ms.push(certified.1);
    }
    let mut checkpoint_ms = Vec::new();
    for step in 1..=16 {
        world.living.accounts.get_mut(&1).unwrap().money -= 1;
        let expected = store.export(&world)?;
        let started = Instant::now();
        store.checkpoint(
            &mut world,
            json!({"simulation_tick":500,"checkpoint_iteration":step}),
            &desired,
        )?;
        checkpoint_ms.push(started.elapsed().as_secs_f64() * 1000.);
        if store.export(&world)? != expected {
            return Err("checkpoint changed complete logical state".into());
        }
    }
    let mut worker = RegionWorker::new(store)?;
    let mut submission_ms = Vec::new();
    let mut worker_ms = Vec::new();
    let mut checkpoint_stages = Vec::new();
    let mut max_poll_ms: f64 = 0.;
    let mut polls = 0usize;
    for step in 1..=16 {
        world.living.accounts.get_mut(&1).unwrap().money -= 1;
        // Verification is deliberately outside the measured submission/poll
        // path. A real host keeps rendering this revision until completion.
        let expected = worker.export(&world)?;
        let started = Instant::now();
        worker.submit_checkpoint(
            world.clone(),
            json!({"simulation_tick":500,"worker_iteration":step}),
            desired.clone(),
        )?;
        submission_ms.push(started.elapsed().as_secs_f64() * 1000.);
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        let completion = loop {
            let started = Instant::now();
            let ready = worker.try_complete()?;
            max_poll_ms = max_poll_ms.max(started.elapsed().as_secs_f64() * 1000.);
            polls += 1;
            if let Some(completion) = ready {
                break completion;
            }
            if Instant::now() >= deadline {
                return Err("worker completion timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        if let Some(error) = completion.error {
            return Err(error.into());
        }
        worker_ms.push(completion.elapsed_ms);
        checkpoint_stages.push(completion.checkpoint_metrics);
        world = completion.state.ok_or("worker returned no state")?;
        if worker.export(&world)? != expected {
            return Err("worker changed complete logical state".into());
        }
    }
    let last = worker.status();
    let expected = worker.export(&world)?;
    let cache = worker.with_store(|store| Ok(store.cache_stats()))?;
    let dormant_unchanged = expected.resource_nodes["probe-deposit-15-1"]
        == original.resource_nodes["probe-deposit-15-1"];
    if !dormant_unchanged {
        return Err("dormant renewable resource advanced".into());
    }
    let backup = directory.path().join("backup.db");
    worker.backup_to(&backup)?;
    drop(worker);
    let mut reopened = RegionStore::open(
        &database,
        json!({"kind":"regional-probe","source":"synthetic-fixture-1"}),
        &catalog,
        16,
    )?;
    let started = Instant::now();
    let saved = reopened.restore()?.ok_or("missing committed checkpoint")?;
    let restore_ms = started.elapsed().as_secs_f64() * 1000.;
    if saved.state != expected {
        return Err("restart differs from committed world".into());
    }
    let mut backup = RegionStore::open(
        &backup,
        json!({"kind":"regional-probe","source":"synthetic-fixture-1"}),
        &catalog,
        16,
    )?;
    if backup.restore()?.ok_or("empty backup")?.state != expected {
        return Err("online backup differs".into());
    }
    checkpoint_ms.sort_by(f64::total_cmp);
    submission_ms.sort_by(f64::total_cmp);
    worker_ms.sort_by(f64::total_cmp);
    let owner_dormancy = owner_dormancy_probe()?;
    let report = json!({
        "version":env!("CARGO_PKG_VERSION"),"fixture":"synthetic finite regional storage and simulation",
        "environment":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"visible_parallelism":std::thread::available_parallelism().map_or(1,usize::from),"profile":"optimized dev (opt-level=1, debug assertions enabled)","sqlite":rusqlite::version()},
        "scope":{"complete_bodies":full_bodies,"active_bodies":active_bodies,"complete_resource_nodes":full_nodes,"active_resource_nodes":active_nodes,"simulation_steps":500,"physical_seconds":25,"calendar_seconds":250,"region_level":16,"imported_collision":false,"live_data":false,"sockets":0},
        "initial_checkpoint_ms":initial_ms,"initial_status":initial,"final_status":last,
        "checkpoint":{"samples":checkpoint_ms.len(),"mean_ms":checkpoint_ms.iter().sum::<f64>()/checkpoint_ms.len() as f64,"p50_ms":checkpoint_ms[7],"p95_ms":checkpoint_ms[15],"max_ms":checkpoint_ms[15]},
        "background_checkpoint":{"samples":worker_ms.len(),"submission_including_snapshot_clone_p50_ms":submission_ms[7],"submission_p95_ms":submission_ms[15],"worker_p50_ms":worker_ms[7],"worker_p95_ms":worker_ms[15],"poll_calls":polls,"max_nonblocking_poll_ms":max_poll_ms,"policy":"one outstanding revision; authority waits while presentation continues; no simulation mutation between submit and install"},
        "verified_partition_cache":cache,
        "validation_scope_comparison": {
            "samples":16,"same_revision_canonical_equal":true,
            "full_scope_merge":timings(&full_scope_ms),"certified_environment_merge":timings(&certified_scope_ms),
            "work":scope_work,"note":"Alternating same-revision domain merges; excludes storage, encoding, selection and socket work. Full reference is the previous certified-partition merge algorithm with complete merged-state validation."
        },
        "checkpoint_stages": {
            "compose":timings(&checkpoint_stages.iter().map(|m|m.compose_ms).collect::<Vec<_>>()),
            "select":timings(&checkpoint_stages.iter().map(|m|m.select_ms).collect::<Vec<_>>()),
            "encode":timings(&checkpoint_stages.iter().map(|m|m.encode_ms).collect::<Vec<_>>()),
            "transaction":timings(&checkpoint_stages.iter().map(|m|m.transaction_ms).collect::<Vec<_>>()),
            "install":timings(&checkpoint_stages.iter().map(|m|m.install_ms).collect::<Vec<_>>()),
            "last_work":checkpoint_stages.last(),
        },
        "owner_dormancy":owner_dormancy,
        "simulation":{"complete_ms":full_simulation_ms,"active_ms":active_simulation_ms,"note":"Different intended workloads: dormant environment and ambient bodies freeze; global economy and owned bodies advance in both."},
        "restore_complete_world_ms":restore_ms,
        "verification":{"full_roundtrip_exact":true,"every_checkpoint_exact":true,"every_worker_checkpoint_exact":true,"dormant_resource_unchanged":dormant_unchanged,"restart_exact":true,"online_backup_exact":true},
        "limits":"All aggregate domain caps remain. Checkpoint certifies unchanged dependency-closed environmental groups; active/global and dependent cold bodies, housing and ecology are fully validated. Complete restores validate all decoded records. Page encoding is reused only under exact immutable equality and database epoch guards. One background worker remains. Authority pauses while pending, presentation may continue. Explicit startup/restore/final drain are synchronous. Timings are fixture evidence, not a server capacity or latency guarantee."
    });
    let bytes = serde_json::to_string_pretty(&report)?;
    if let Some(output) = output {
        std::fs::write(output, &bytes)?;
    }
    println!("{bytes}");
    Ok(())
}

fn timings(samples: &[f64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"samples":sorted.len(),"mean_ms":sorted.iter().sum::<f64>() / sorted.len() as f64,
        "p50_ms":sorted[(sorted.len()-1)/2],"p95_ms":sorted[((sorted.len() as f64*0.95).ceil() as usize-1).min(sorted.len()-1)],"max_ms":sorted.last()})
}

fn owner_dormancy_probe() -> Result<serde_json::Value, Box<dyn Error>> {
    use terra_core::LocalFrame;
    use terra_sandbox::{LandPlot, Structure};
    let origin = GeoPosition::new(48.137, 11.575, 100.)?;
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    for owner in 1..=16u64 {
        let position = LocalFrame::new(origin).to_geo([(owner - 1) as f64 * 10_000., 0., 0.])?;
        world.add_player(owner, position)?;
    }
    world.seed_living_demo(origin, &[], &catalog)?;
    world.ensure_transport_demo(&catalog)?;
    for owner in 1..=16u64 {
        let position = world.players[&owner].position;
        let plot = world.next_id;
        let house = plot + 1;
        world.next_id += 2;
        world.plots.insert(
            plot,
            LandPlot {
                id: plot,
                owner,
                faction_owner: None,
                center: position,
                radius_m: 50.,
            },
        );
        let blueprint = &catalog.blueprints["house"];
        world.structures.insert(
            house,
            Structure {
                id: house,
                owner,
                faction_owner: None,
                blueprint: "house".into(),
                position,
                hitpoints: blueprint.hitpoints,
                max_hitpoints: blueprint.hitpoints,
                radius_m: blueprint.radius_m,
                housing: blueprint.housing,
                workbench: blueprint.workbench,
            },
        );
        world.players.get_mut(&owner).unwrap().home = Some(house);
        world.seed_resource(
            format!("dormancy-fixture-{owner}"),
            position,
            "stone",
            100,
            30.,
        )?;
        world
            .resource_nodes
            .get_mut(&format!("dormancy-fixture-{owner}"))
            .unwrap()
            .remaining = 7;
    }
    world.validate_with_catalog(&catalog)?;
    let dir = tempfile::tempdir()?;
    let database = dir.path().join("owners.db");
    let identity = json!({"kind":"owner-dormancy-probe","source":"synthetic-fixture-1"});
    let mut store = RegionStore::open(&database, identity.clone(), &catalog, 16)?;
    let requested = BTreeSet::from([RegionKey::at(origin, 16)?]);
    let connected = store.checkpoint(&mut world, json!({}), &requested)?;
    let before_owned = world
        .living
        .units
        .values()
        .filter(|u| u.owner.is_some())
        .count();
    let before_homes = world.structures.len();
    for owner in 2..=16 {
        world.set_player_dormant(owner, true)?;
    }
    let expected = store.export(&world)?;
    let started = Instant::now();
    let dormant = store.checkpoint(&mut world, json!({}), &requested)?;
    let transition_ms = started.elapsed().as_secs_f64() * 1000.;
    let after_owned = world
        .living
        .units
        .values()
        .filter(|u| u.owner.is_some())
        .count();
    let after_homes = world.structures.len();
    if after_owned != terra_sandbox::STARTER_UNIT_COUNT
        || after_homes != 1
        || world.dormant_player_count() != 15
    {
        return Err("offline owners did not retire their body/home records".into());
    }
    for _ in 0..100 {
        world.advance(0.5, &WeatherSample::default())?;
        world.advance_living(0.05, 0.5, &catalog)?;
    }
    let full = store.export(&world)?;
    for (&id, body) in &expected.living.units {
        if body.owner.is_some_and(|owner| owner > 1) && full.living.units.get(&id) != Some(body) {
            return Err("dormant owned body advanced".into());
        }
    }
    for owner in 2..=16 {
        if full.players[&owner] != expected.players[&owner]
            || full.living.accounts[&owner] != expected.living.accounts[&owner]
            || full.resource_nodes[&format!("dormancy-fixture-{owner}")]
                != expected.resource_nodes[&format!("dormancy-fixture-{owner}")]
        {
            return Err("dormant ownership or resources changed".into());
        }
    }
    store.checkpoint(&mut world, json!({}), &requested)?;
    drop(store);
    let mut reopened = RegionStore::open(&database, identity, &catalog, 16)?;
    let mut restored = reopened.restore()?.ok_or("missing owner checkpoint")?.state;
    if restored != full {
        return Err("owner checkpoint restart differs".into());
    }
    for owner in 2..=16 {
        restored.set_player_dormant(owner, false)?;
    }
    restored.validate_with_catalog(&catalog)?;
    Ok(json!({
        "fixture":"16 synthetic owners, 15 dormant; no sockets or renderer",
        "registered_owners":16,"dormant_owners":15,
        "resident_owned_bodies_before":before_owned,"resident_owned_bodies_after":after_owned,
        "resident_homes_before":before_homes,"resident_homes_after":after_homes,
        "before_status":connected,"after_status":dormant,"retirement_checkpoint_ms":transition_ms,
        "simulated_while_dormant":{"steps":100,"physical_seconds":5,"calendar_seconds":50},
        "verification":{"offline_bodies_exact":true,"offline_needs_orders_exact":true,"accounts_inventory_exact":true,"offline_resources_exact":true,"restart_exact":true,"reactivation_valid":true},
        "limit":"Synthetic residency/invariant fixture; global gameplay caps and full dependent cold-record validation remain. Static environment groups may use their immutable proof. Not concurrent-player capacity."
    }))
}
