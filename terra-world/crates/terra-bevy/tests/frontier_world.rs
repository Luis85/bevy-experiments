//! Actual engine schedules join ecology, gathering, charters, funded work and
//! persistence. Authority positions are set only to arrange transaction fixtures;
//! walking and collision are covered by the living/procedural integration gates.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use terra_bevy::{
    SandboxIntent, SandboxQueue, SandboxRuntime, TerraLivingPlugin, TerraWorldPlugin, WorldConfig,
    WorldRuntime,
};
use terra_core::{GeoPosition, HeightField, SourceRecord, TileKey, WorldChunk};
use terra_sandbox::{Command, SettlementCommand, SettlementTier};
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};

struct Flat;
impl ChunkProvider for Flat {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        Ok(ChunkPayload {
            cache_hit: false,
            chunk: WorldChunk {
                key,
                heights: HeightField::constant(100.),
                features: vec![],
                warnings: vec![],
                sources: vec![SourceRecord {
                    provider: "frontier fixture".into(),
                    license: "MIT OR Apache-2.0".into(),
                    attribution: "Terra authored integration surface".into(),
                    url: "about:blank".into(),
                    retrieved_unix_seconds: 0,
                    synthetic: true,
                }],
            },
        })
    }
}
fn command(app: &mut App, command: Command) {
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(command));
    app.update();
    let s = app.world().resource::<SandboxRuntime>();
    assert!(
        !s.last_message.starts_with("Action rejected:")
            && !s.last_message.starts_with("Action failed"),
        "{}",
        s.last_message
    );
}
#[test]
fn ecology_gather_charter_supply_contract_and_exact_restore_share_one_app() {
    let directory =
        std::env::temp_dir().join(format!("terra-frontier-integration-{}", std::process::id()));
    let config = WorldConfig {
        focus: GeoPosition::new(48.137, 11.575, 100.).unwrap(),
        chunk_radius: 1,
        terrain_resolution: 17,
        max_resident_chunks: 16,
        max_queued: 16,
        max_in_flight: 2,
        live_weather: false,
        time_scale: 60.,
        base_data_revision: "authored-frontier-integration-v3".into(),
        save_path: directory.join("world.json"),
        cache_dir: directory.join("cache"),
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((
            TerraWorldPlugin::with_provider(config, Arc::new(Flat)),
            TerraLivingPlugin,
        ));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        let s = app.world().resource::<SandboxRuntime>();
        if s.state.living.enabled
            && !s.state.ecology.resources.is_empty()
            && !s.state.settlements.contracts.is_empty()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "frontier did not initialize: {}",
            s.last_message
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..80 {
        app.update();
    }
    assert!(
        app.world()
            .resource::<terra_bevy::EcologyRuntime>()
            .scheduler
            .stats()
            .passes
            > 0
    );
    for stage in [
        terra_bevy::WorldStage::Streaming,
        terra_bevy::WorldStage::Ecology,
        terra_bevy::WorldStage::Living,
        terra_bevy::WorldStage::Economy,
    ] {
        let sample = app
            .world()
            .resource::<terra_bevy::WorldDiagnostics>()
            .stage(stage);
        assert!(sample.window_samples > 0 && sample.p95_ms.is_finite());
    }
    {
        let s = app.world().resource::<SandboxRuntime>();
        assert!(s.state.ecology.animals.len() >= 4);
        assert!(s.state.ecology.metrics.behavior_updates > 0);
        assert!(
            s.state
                .ecology
                .animals
                .values()
                .all(|a| s.state.living.units.contains_key(&a.unit_id))
        );
    }
    // Freeze calendar-driven production/regrowth so exact transaction deltas
    // remain observable while the actual native command queue still runs.
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    let berry = {
        let s = app.world().resource::<SandboxRuntime>();
        s.state
            .resource_nodes
            .values()
            .find(|n| n.item == "berries")
            .unwrap()
            .clone()
    };
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .set_player_position(1, berry.position)
        .unwrap();
    command(
        &mut app,
        Command::Gather {
            node_id: berry.id.clone(),
            amount: 5,
        },
    );
    {
        let s = app.world().resource::<SandboxRuntime>();
        assert_eq!(s.state.players[&1].inventory["berries"], 5);
        assert_eq!(
            s.state.resource_nodes[&berry.id].remaining,
            berry.remaining - 5
        );
    }
    let before_money = app
        .world()
        .resource::<SandboxRuntime>()
        .state
        .living
        .accounts[&1]
        .money;
    command(
        &mut app,
        Command::Settlement {
            action: SettlementCommand::Found {
                name: "Willow Reach".into(),
                center: berry.position,
                faction_owner: None,
            },
        },
    );
    {
        let s = app.world().resource::<SandboxRuntime>();
        let t = s
            .state
            .settlements
            .territories
            .values()
            .next()
            .expect("territory was not created");
        assert_eq!(t.name, "Willow Reach");
        assert_eq!(t.tier, SettlementTier::Neighborhood);
        assert_eq!(before_money, s.state.living.accounts[&1].money + t.treasury);
        assert_eq!(s.state.players[&1].inventory["wood"], 10);
        assert_eq!(s.state.players[&1].inventory["stone"], 10);
    }
    let (offer, shop, price, missing) = {
        let s = app.world().resource::<SandboxRuntime>();
        s.state
            .settlements
            .contracts
            .values()
            .find_map(|c| {
                let owned = s.state.players[&1]
                    .inventory
                    .get(&c.item)
                    .copied()
                    .unwrap_or(0);
                let missing = c.amount.saturating_sub(owned);
                s.state.living.shops.values().find_map(|shop| {
                    let price = shop.prices.get(&c.item)?.buy;
                    (shop.stock.get(&c.item).copied().unwrap_or(0) >= missing
                        && price * missing as u64 <= s.state.living.accounts[&1].money)
                        .then(|| (c.clone(), shop.clone(), price, missing))
                })
            })
            .expect("no affordable supply request")
    };
    if missing > 0 {
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .set_player_position(1, shop.position)
            .unwrap();
        let wallet = app
            .world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .accounts[&1]
            .money;
        command(
            &mut app,
            Command::Living {
                action: terra_sandbox::LivingCommand::Buy {
                    shop_id: shop.id,
                    item: offer.item.clone(),
                    amount: missing,
                },
            },
        );
        assert_eq!(
            app.world()
                .resource::<SandboxRuntime>()
                .state
                .living
                .accounts[&1]
                .money,
            wallet - price * missing as u64
        );
    }
    command(
        &mut app,
        Command::Settlement {
            action: SettlementCommand::AcceptContract {
                contract_id: offer.id,
            },
        },
    );
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .set_player_position(1, offer.target_position)
        .unwrap();
    let (wallet, stock) = {
        let s = app.world().resource::<SandboxRuntime>();
        let stock = s
            .state
            .living
            .producers
            .get(&offer.target_site)
            .map(|p| &p.stock)
            .or_else(|| {
                s.state
                    .living
                    .shops
                    .get(&offer.target_site)
                    .map(|p| &p.stock)
            })
            .unwrap();
        (
            s.state.living.accounts[&1].money,
            stock.get(&offer.item).copied().unwrap_or(0),
        )
    };
    command(
        &mut app,
        Command::Settlement {
            action: SettlementCommand::FulfillContract {
                contract_id: offer.id,
            },
        },
    );
    {
        let s = app.world().resource::<SandboxRuntime>();
        let next = s
            .state
            .living
            .producers
            .get(&offer.target_site)
            .map(|p| &p.stock)
            .or_else(|| {
                s.state
                    .living
                    .shops
                    .get(&offer.target_site)
                    .map(|p| &p.stock)
            })
            .unwrap();
        assert_eq!(next[&offer.item], stock + offer.amount);
        assert_eq!(s.state.living.accounts[&1].money, wallet + offer.reward);
        assert_eq!(s.state.settlements.progress[&1].contracts_completed, 1);
        assert!(!s.state.settlements.contracts.contains_key(&offer.id));
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    app.update();
    let saved = serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap();
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .clear();
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Load);
    app.update();
    assert_eq!(
        saved,
        serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap()
    );
    assert_eq!(
        app.world()
            .resource::<terra_bevy::EcologyRuntime>()
            .scheduler
            .stats()
            .passes,
        0,
        "explicitly loading the same saved clock must reset transient AI decisions"
    );
    assert_eq!(
        app.world()
            .resource::<terra_bevy::LivingDiagnostics>()
            .ecology_work
            .passes,
        0
    );
    let state = &app.world().resource::<SandboxRuntime>().state;
    let view = state.snapshot_for(1, 5000.).unwrap();
    assert!(!view.ecology.animals.is_empty());
    assert!(!view.settlements.territories.is_empty());
    assert_eq!(view.settlements.progress.unwrap().contracts_completed, 1);
    // Floating render origins must not reset geographic scheduler fairness or
    // teleport the active body. Only an actual world replacement resets work.
    app.world_mut().resource_mut::<WorldRuntime>().clock.paused = false;
    for _ in 0..16 {
        app.update();
    }
    let passes = app
        .world()
        .resource::<terra_bevy::EcologyRuntime>()
        .scheduler
        .stats()
        .passes;
    let before = app.world().resource::<SandboxRuntime>().state.players[&1].position;
    let revision = app.world().resource::<WorldRuntime>().origin_revision;
    let focus = terra_core::LocalFrame::new(app.world().resource::<WorldRuntime>().focus)
        .to_geo([3000., 0., 3000.])
        .unwrap();
    app.world_mut().resource_mut::<WorldRuntime>().focus = focus;
    for _ in 0..16 {
        app.update();
    }
    assert!(app.world().resource::<WorldRuntime>().origin_revision > revision);
    assert!(
        app.world()
            .resource::<terra_bevy::EcologyRuntime>()
            .scheduler
            .stats()
            .passes
            > passes
    );
    let state = &app.world().resource::<SandboxRuntime>().state;
    assert!(state.players[&1].position.surface_distance_m(before) < 0.1);
    assert_eq!(state.living.active_unit(1).unwrap().owner, Some(1));
    let _ = std::fs::remove_dir_all(directory);
}
