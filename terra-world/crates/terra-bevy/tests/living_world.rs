//! End-to-end engine schedules: streaming -> population -> commands -> motion,
//! economy -> production/deliveries -> durable save. No renderer or public API.
use bevy::{prelude::*, time::TimeUpdateStrategy};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use terra_bevy::{
    LivingDiagnostics, LivingMotion, SandboxIntent, SandboxQueue, SandboxRuntime,
    TerraLivingPlugin, TerraWorldPlugin, WorldConfig, WorldRuntime,
};
use terra_core::{GeoPosition, HeightField, LocalFrame, SourceRecord, TileKey, WorldChunk};
use terra_sandbox::{Command, LivingCommand, UnitRole};
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};
struct FlatProvider;
impl ChunkProvider for FlatProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        Ok(ChunkPayload {
            cache_hit: false,
            chunk: WorldChunk {
                key,
                heights: HeightField::constant(100.),
                features: vec![],
                warnings: vec![],
                sources: vec![SourceRecord {
                    provider: "integration fixture".into(),
                    license: "MIT OR Apache-2.0".into(),
                    attribution: "Terra authored flat test surface".into(),
                    url: "about:blank".into(),
                    retrieved_unix_seconds: 0,
                    synthetic: true,
                }],
            },
        })
    }
}
fn game() -> App {
    let dir = std::env::temp_dir().join(format!("terra-living-integration-{}", std::process::id()));
    let config = WorldConfig {
        focus: GeoPosition::new(48.137, 11.575, 100.).unwrap(),
        chunk_radius: 1,
        terrain_resolution: 17,
        max_resident_chunks: 16,
        max_queued: 16,
        max_in_flight: 2,
        live_weather: false,
        time_scale: 60.,
        base_data_revision: "authored-flat-integration-v2".into(),
        save_path: dir.join("world.json"),
        cache_dir: dir.join("cache"),
        ..Default::default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((
            TerraWorldPlugin::with_provider(config, Arc::new(FlatProvider)),
            TerraLivingPlugin,
        ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !app
        .world()
        .resource::<SandboxRuntime>()
        .state
        .living
        .enabled
    {
        app.update();
        assert!(
            Instant::now() < deadline,
            "living population did not seed: {:?}",
            app.world().resource::<LivingDiagnostics>().last_error
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    app
}
fn command(app: &mut App, action: LivingCommand) {
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(Command::Living { action }));
    app.update();
}

#[test]
fn inhabit_trade_command_produce_deliver_and_restore_in_one_actual_app() {
    let mut app = game();
    assert!(
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .units
            .len()
            >= 40
    );
    let (shop, avatar, worker, wallet, bread) = {
        let s = app.world().resource::<SandboxRuntime>();
        let life = &s.state.living;
        (
            life.shops.values().next().unwrap().id,
            life.accounts[&1].avatar_unit,
            life.units
                .values()
                .find(|u| {
                    u.owner == Some(1)
                        && u.role == UnitRole::Worker
                        && u.kind == terra_core::ActorKind::Human
                })
                .unwrap()
                .id,
            life.accounts[&1].money,
            s.state.players[&1]
                .inventory
                .get("bread")
                .copied()
                .unwrap_or(0),
        )
    };
    command(
        &mut app,
        LivingCommand::Buy {
            shop_id: shop,
            item: "bread".into(),
            amount: 1,
        },
    );
    {
        let s = app.world().resource::<SandboxRuntime>();
        assert_eq!(
            s.state.living.accounts[&1].money,
            wallet - 250,
            "{}",
            s.last_message
        );
        assert_eq!(s.state.players[&1].inventory["bread"], bread + 1);
    }
    // Physical control of a different owned body preserves the principal account.
    command(&mut app, LivingCommand::Possess { unit_id: worker });
    let before = app.world().resource::<SandboxRuntime>().state.living.units[&worker].position;
    *app.world_mut().resource_mut::<LivingMotion>() = LivingMotion {
        active: true,
        velocity_enu_m_s: [2., 0., 0.],
        heading_deg: 90.,
    };
    for _ in 0..30 {
        app.update();
    }
    *app.world_mut().resource_mut::<LivingMotion>() = LivingMotion::default();
    let after = app.world().resource::<SandboxRuntime>().state.living.units[&worker].position;
    assert!(
        before.surface_distance_m(after) > 2.,
        "possessed worker did not move"
    );
    assert_eq!(app.world().resource::<SandboxRuntime>().player_id, 1);
    command(&mut app, LivingCommand::Release);
    assert_eq!(
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .accounts[&1]
            .active_unit,
        worker
    );
    // RTS selection changes the body used by the SAME trade/build permissions.
    command(&mut app, LivingCommand::SelectActive { unit_id: avatar });
    let target = LocalFrame::new(
        app.world().resource::<SandboxRuntime>().state.living.units[&avatar].position,
    )
    .to_geo([-18., 0., -12.])
    .unwrap();
    command(
        &mut app,
        LivingCommand::MoveUnits {
            unit_ids: vec![avatar],
            destination: target,
        },
    );
    for _ in 0..250 {
        app.update();
    }
    {
        let s = app.world().resource::<SandboxRuntime>();
        assert!(
            s.state.living.units[&avatar]
                .position
                .surface_distance_m(target)
                < 2.,
            "RTS order did not arrive: {:?}",
            s.state.living.units[&avatar].order
        );
        assert_eq!(
            s.state.players[&1].position,
            s.state.living.units[&avatar].position
        );
    }
    for _ in 0..2100 {
        app.update();
    }
    {
        let s = app.world().resource::<SandboxRuntime>();
        let life = &s.state.living;
        assert!(life.metrics.production_cycles > 0);
        assert!(
            life.metrics.deliveries_completed > 0,
            "couriers did not complete: {:?}; motion: {:?}; diagnostics: {:?}",
            life.deliveries,
            life.units
                .values()
                .filter(|u| u.role == UnitRole::Hauler)
                .map(|u| (
                    u.id,
                    LocalFrame::new(life.origin.unwrap()).to_local(u.position),
                    u.velocity_enu_m_s,
                    u.path_index,
                    u.path
                        .get(u.path_index)
                        .map(|p| LocalFrame::new(life.origin.unwrap()).to_local(*p)),
                    u.order.clone()
                ))
                .collect::<Vec<_>>(),
            app.world().resource::<LivingDiagnostics>()
        );
        assert!(
            life.metrics.npc_purchases > 0,
            "residents never purchased supplies"
        );
        assert!(
            life.units
                .values()
                .all(|u| u.position.validate().is_ok() && u.needs.food.is_finite())
        );
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }
    // Explicit local save/load persists bodies, cash, queues, production and cargo.
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    app.update();
    let saved = app.world().resource::<SandboxRuntime>().state.clone();
    let path = app
        .world()
        .resource::<WorldRuntime>()
        .config
        .save_path
        .clone();
    assert!(
        path.is_file(),
        "{}",
        app.world().resource::<SandboxRuntime>().last_message
    );
    for _ in 0..50 {
        app.update();
    }
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Load);
    app.update();
    assert_eq!(app.world().resource::<SandboxRuntime>().state, saved);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
