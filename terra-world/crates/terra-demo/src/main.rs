mod accessibility;
mod actor_summary;
mod building_review;
mod camera;
mod character;
mod command_input;
mod communities;
mod control;
mod driving;
mod fleet;
mod flow;
mod frontier;
mod industry;
mod interaction;
mod interface;
mod launcher;
mod market;
mod output;
mod performance;
mod player_input;
mod remote;
mod reviewed_action;
mod telemetry_capture;
mod vehicle_models;
mod vehicle_presentation;
mod wheel_input;
use bevy::prelude::*;
use std::{net::SocketAddr, path::PathBuf};
use terra_bevy::{
    NetworkSession, TerraLivingPlugin, TerraNetworkPlugin, TerraRenderPlugin, TerraWorldPlugin,
    WorldConfig,
};

#[derive(Resource, Default)]
pub struct DemoOptions {
    pub screenshot: Option<String>,
    pub exit_frames: Option<u64>,
    pub smoke: bool,
    pub atlas: bool,
    pub camp_view: bool,
    pub noon: bool,
    pub connect: Option<SocketAddr>,
    pub network_security: Option<terra_bevy::NetworkSecurity>,
    pub allow_insecure_remote: bool,
    pub telemetry: Option<PathBuf>,
    pub telemetry_interval_seconds: Option<u64>,
    pub character: Option<terra_bevy::rules::CharacterProfile>,
    pub world_profile_path: Option<PathBuf>,
    /// A managed launch witness requires restore, even when an earlier abrupt
    /// exit prevented presentation settings from recording checkpoint_present.
    pub require_existing_progress: bool,
    pub world_title: Option<String>,
    pub user_data: PathBuf,
    pub start_hour: Option<u8>,
    pub menu_supervised: bool,
    pub asset_pack: Option<PathBuf>,
    pub procedural_models: bool,
    pub no_default_politics: bool,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|argument| argument == "--menu") {
        return launcher::run(&args);
    }
    if args.iter().any(|argument| argument == "--world-profile")
        && args.iter().any(|argument| {
            matches!(
                argument.as_str(),
                "--config"
                    | "--region-store"
                    | "--save"
                    | "--connect"
                    | "--lat"
                    | "--lon"
                    | "--live"
                    | "--heightmap"
                    | "--world-pack"
                    | "--epoch"
                    | "--politics"
            )
        })
    {
        return Err("--world-profile already selects its source, character and save. Use a separate direct-play command for another world.".into());
    }
    let mut config = WorldConfig::default();
    let mut options = DemoOptions {
        noon: true,
        user_data: std::env::current_dir()?.join("saves"),
        ..Default::default()
    };
    let mut session_path = PathBuf::from("terra-session.json");
    let mut quality = terra_bevy::render::RenderQualitySettings::default();
    let mut tls_ca = None;
    let mut tls_server_name = None;
    let mut controls_override = None;
    let mut i = 0;
    while i < args.len() {
        let next = |i: usize| {
            args.get(i + 1)
                .ok_or_else(|| format!("{} requires a value", args[i]))
        };
        match args[i].as_str() {
            "--play" => {}
            "--asset-pack" => {
                options.asset_pack = Some(launcher::model::absolute_path(std::path::Path::new(
                    next(i)?,
                ))?);
                i += 1;
            }
            "--procedural-models" => options.procedural_models = true,
            "--no-default-politics" => options.no_default_politics = true,
            "--controls-json" => {
                controls_override = Some(player_input::InputSettings::from_handoff(next(i)?)?);
                i += 1;
            }
            "--menu-supervised" => options.menu_supervised = true,
            "--user-data" => {
                options.user_data = launcher::model::absolute_path(std::path::Path::new(next(i)?))?;
                i += 1;
            }
            "--world-profile" => {
                let path = launcher::model::absolute_path(std::path::Path::new(next(i)?))?;
                let profile = launcher::model::WorldProfile::read(&path)?;
                config = profile.config;
                quality = profile.quality;
                options.world_title = Some(profile.title);
                options.character = Some(profile.character);
                options.start_hour = Some(profile.start_hour);
                options.world_profile_path = Some(path);
                // Managed profiles already carry their resolved political
                // source, including an explicitly politics-free recovery.
                options.no_default_politics = true;
                options.noon = false;
                i += 1;
            }
            "--help" | "-h" => {
                println!(
                    "Terra World — living geographic sandbox\n\nUsage: cargo run -p terra-demo -- [options]\nNo arguments opens the native main menu. Explicit options start the game directly.\n  --menu                  Open main menu, character creation and world setup\n  --play                  Direct play with default world settings\n  --world-profile FILE    Resume a world created through the native menu\n  --user-data FOLDER      Menu worlds, preferences and multiplayer identities\n  --config FILE           JSON WorldConfig\n  --quality NAME          performance, balanced (default), or high\n  --no-atmosphere         Disable atmospheric sky on this client\n  --asset-pack FILE       Validate and use a presentation pack JSON\n  --procedural-models     Use geometric actors without external model packs\n  --telemetry FILE        Retain bounded diagnostics and a final exit capture\n  --telemetry-interval-seconds N  Also capture periodically, 1..3600 (16 retained)\n  --lat NUMBER --lon NUMBER  WGS84 geographic focus\n  --live                  Opt in to terrain / OSM / weather demo adapters\n  --no-osm                Skip OpenStreetMap requests\n  --synthetic-weather     Keep deterministic game weather\n  --connect IP:PORT       Join authoritative LAN reference server\n  --tls-ca FILE           Trusted CA certificate PEM for verified TLS\n  --tls-server-name NAME  Hostname on the server certificate\n  --allow-insecure-remote Explicitly allow trusted-network remote plaintext\n  --session FILE          Private resume identity (default terra-session.json)\n  --time-scale NUMBER     Calendar/economy seconds per real second (0 freezes calendar)\n  --radius N              Stream (2N+1)^2 chunks, maximum 5\n  --heightmap FILE        Import georeferenced 8/16-bit grayscale PNG manifest\n  --world-pack FILE       Load an explicitly dated immutable world pack\n  --epoch DATE            Source epoch YYYY-MM-DD or signed Unix seconds\n  --politics FILE         Dated political GeoJSON with provenance\n  --no-default-politics   Keep worlds without an implicit political dataset\n  --atlas                 Start in the political atlas\n  --camp-view             Start close to your player\n  --noon                  Start new worlds at noon; preserve saved clocks\n  --save FILE             Explicit JSON save path (F5 / F9)\n  --region-store FILE     Resume / create a durable regional SQLite world\n  --checkpoint-seconds N  Regional autosave cadence, 1..3600 (default 30)\n  --frames N              Exit after N updates\n  --screenshot FILE       Capture rendered scene\n  --smoke                 Exercise local sandbox automatically\n\nF1 RTS: click/drag select, right-click move. F2 third-person, F3 first-person.\nOn foot: WASD move, Shift sprint. Possession: right-drag look, R/F flight altitude, E interact, C view.\nRegistered vehicles: W/S throttle/reverse, A/D steer, Shift brake, Space handbrake.\nEscape cancels editing or opens the session menu. J activities, N nature, T territory, V driving, G garage. F6 management focus, F10 world settings, F11 inspector, F12 performance, Tab focus, F5 save, F9 load.\nWith --region-store, F5 checkpoints and F9 restores the latest checkpoint.\nDefault world is synthetic. --live reports provider and fallback provenance."
                );
                return Ok(());
            }
            "--config" => {
                config = serde_json::from_slice(&std::fs::read(next(i)?)?)?;
                i += 1;
            }
            "--quality" => {
                quality.quality = terra_bevy::render::RenderQuality::from_name(next(i)?)
                    .ok_or("--quality must be performance, balanced or high")?;
                i += 1;
            }
            "--no-atmosphere" => quality.atmospheric_sky = false,
            "--telemetry" => {
                options.telemetry = Some(PathBuf::from(next(i)?));
                i += 1;
            }
            "--telemetry-interval-seconds" => {
                let seconds: u64 = next(i)?.parse()?;
                if !(1..=3600).contains(&seconds) {
                    return Err("--telemetry-interval-seconds must be 1..=3600.".into());
                }
                options.telemetry_interval_seconds = Some(seconds);
                i += 1;
            }
            "--region-store" => {
                config.region_store = Some(PathBuf::from(next(i)?));
                i += 1;
            }
            "--checkpoint-seconds" => {
                config.region_checkpoint_seconds = next(i)?.parse()?;
                i += 1;
            }
            "--lat" => {
                config.focus.latitude = next(i)?.parse()?;
                i += 1;
            }
            "--lon" => {
                config.focus.longitude = next(i)?.parse()?;
                i += 1;
            }
            "--live" => config.live_data = true,
            "--no-osm" => config.osm_features = false,
            "--synthetic-weather" => config.live_weather = false,
            "--connect" => {
                options.connect = Some(next(i)?.parse()?);
                i += 1;
            }
            "--tls-ca" => {
                tls_ca = Some(launcher::model::absolute_path(std::path::Path::new(next(
                    i,
                )?))?);
                i += 1;
            }
            "--tls-server-name" => {
                tls_server_name = Some(next(i)?.clone());
                i += 1;
            }
            "--allow-insecure-remote" => options.allow_insecure_remote = true,
            "--session" => {
                session_path = PathBuf::from(next(i)?);
                i += 1;
            }
            "--time-scale" => {
                config.time_scale = next(i)?.parse()?;
                i += 1;
            }
            "--world-pack" => {
                config.historical_pack = Some(PathBuf::from(next(i)?));
                i += 1;
            }
            "--heightmap" => {
                config.heightmap_manifest = Some(PathBuf::from(next(i)?));
                i += 1;
            }
            "--epoch" => {
                config.data_epoch_unix = Some(terra_bevy::history::parse_date(next(i)?)?);
                i += 1;
            }
            "--politics" => {
                config.politics_file = Some(PathBuf::from(next(i)?));
                i += 1;
            }
            "--atlas" => options.atlas = true,
            "--noon" => options.noon = true,
            "--camp-view" => options.camp_view = true,
            "--radius" => {
                config.chunk_radius = next(i)?.parse()?;
                i += 1;
            }
            "--save" => {
                config.save_path = PathBuf::from(next(i)?);
                i += 1;
            }
            "--frames" => {
                options.exit_frames = Some(next(i)?.parse()?);
                i += 1;
            }
            "--screenshot" => {
                options.screenshot = Some(next(i)?.clone());
                i += 1;
            }
            "--smoke" => options.smoke = true,
            x => return Err(format!("unknown option: {x}").into()),
        }
        i += 1;
    }
    options.network_security = match (tls_ca, tls_server_name) {
        (Some(ca_path), Some(server_name)) => Some(terra_bevy::NetworkSecurity {
            ca_path,
            server_name,
        }),
        (None, None) => None,
        _ => return Err("TLS requires both --tls-ca FILE and --tls-server-name HOSTNAME.".into()),
    };
    if options.connect.is_none()
        && (options.network_security.is_some() || options.allow_insecure_remote)
    {
        return Err("Connection security options require --connect IP:PORT.".into());
    }
    if options
        .connect
        .is_some_and(|address| !address.ip().is_loopback())
        && options.network_security.is_none()
        && !options.allow_insecure_remote
    {
        return Err("Remote connections require verified TLS (--tls-ca and --tls-server-name) or an explicit --allow-insecure-remote for a trusted plaintext network.".into());
    }
    config.focus = terra_core::GeoPosition::new(
        config.focus.latitude,
        config.focus.longitude,
        config.focus.altitude_m,
    )?;
    // Public services receive a small default neighborhood; prepared or offline
    // data can use the wider 49-chunk playground without multiplying requests.
    if config.live_data
        && !args
            .iter()
            .any(|a| matches!(a.as_str(), "--radius" | "--config" | "--world-profile"))
    {
        config.chunk_radius = 1;
        config.terrain_resolution = 33;
    }
    // A radius-only CLI request also supplies a fitting default cache budget.
    // Explicit configuration files retain their author-chosen validated limits.
    if config.chunk_radius <= 5
        && args.iter().any(|a| a == "--radius")
        && !args
            .iter()
            .any(|a| matches!(a.as_str(), "--config" | "--world-profile"))
    {
        let needed = (2 * config.chunk_radius + 1).pow(2) as usize;
        config.max_resident_chunks = config.max_resident_chunks.max(needed);
        config.max_queued = config.max_queued.max(needed);
    }
    let bundled_politics =
        PathBuf::from("crates/terra-history/data/natural-earth-countries-2026-10-26.geojson");
    if config.politics_file.is_none()
        && !options.no_default_politics
        && config.historical_pack.is_none()
        && config.data_epoch_unix.is_none()
        && options.connect.is_none()
        && bundled_politics.exists()
    {
        config.politics_file = Some(bundled_politics);
    }
    config.validate()?;
    if config.region_store.is_some() && options.connect.is_some() {
        return Err(
            "--region-store is local authority storage; a multiplayer client uses its host's store"
                .into(),
        );
    }
    if config.region_store.is_some() && args.iter().any(|arg| arg == "--save") {
        return Err(
            "--save and --region-store select different persistence modes; choose one".into(),
        );
    }
    if options.telemetry_interval_seconds.is_some() && options.telemetry.is_none() {
        return Err("--telemetry-interval-seconds requires --telemetry FILE.".into());
    }
    output::validate_session_outputs(&config, &options, &session_path)?;
    output::validate(&config, &options)?;
    if options.procedural_models && options.asset_pack.is_some() {
        return Err("Choose --asset-pack or --procedural-models, not both.".into());
    }
    // Import before creating a native window or saving any world. These bytes
    // are presentation only and never participate in authoritative world identity.
    let asset_pack = if options.procedural_models {
        None
    } else if let Some(path) = &options.asset_pack {
        Some(terra_bevy::asset_pack::ValidatedAssetPack::read(path)
            .map_err(|error| format!("Asset pack could not be loaded: {error}. Fix the pack or use --procedural-models."))?)
    } else {
        Some(terra_bevy::render::bundled_wildlife_pack()?)
    };
    if let Some(path) = &config.heightmap_manifest {
        let provider =
            terra_stream::PreparedHeightmapProvider::open(path, config.terrain_resolution)?;
        let key = terra_core::TileKey::at(config.focus, config.chunk_level)?;
        if !provider.covers(key)
            && !args.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "--lat" | "--lon" | "--config" | "--world-profile"
                )
            })
        {
            config.focus = provider
                .covered_focus(config.chunk_level)
                .ok_or("heightmap set has no completely covered tile at this level")?;
        }
        let focus_key = terra_core::TileKey::at(config.focus, config.chunk_level)?;
        if !provider.covers(focus_key) {
            return Err(
                "heightmap does not cover the focus tile; choose coordinates inside its bounds"
                    .into(),
            );
        }
        while config.chunk_radius > 0
            && !focus_key
                .neighbors(config.chunk_radius)
                .iter()
                .all(|key| provider.covers(*key))
        {
            config.chunk_radius -= 1;
        }
        config.base_data_revision = provider.source_revision().to_owned();
        config.live_weather = false;
    }
    if let Some(path) = &config.politics_file {
        let data = terra_bevy::history::PoliticsDataset::load(path)?;
        config.political_dataset_revision = Some(data.id);
    }
    if let Some(path) = &config.historical_pack {
        let pack = terra_bevy::history::HistoricalPack::load(path)?;
        let epoch = config.data_epoch_unix.ok_or("missing --epoch")?;
        let keys = pack.keys();
        let first = pack.chunk_ref(keys[0], epoch)?;
        config.chunk_level = keys[0].level;
        config.terrain_resolution = first.heights.width;
        for key in &keys {
            let chunk = pack.chunk_ref(*key, epoch)?;
            if key.level != config.chunk_level
                || chunk.heights.width != config.terrain_resolution
                || chunk.heights.height != config.terrain_resolution
            {
                return Err(
                    "desktop pack requires one tile level and one square terrain resolution".into(),
                );
            }
        }
        let focus_key = terra_core::TileKey::at(config.focus, config.chunk_level)?;
        let _ = pack.chunk_ref(focus_key, epoch)?;
        // Initial focus requests only the complete neighborhood present in this bounded pack.
        while config.chunk_radius > 0
            && !focus_key
                .neighbors(config.chunk_radius)
                .iter()
                .all(|k| keys.contains(k))
        {
            config.chunk_radius -= 1;
        }
        config.base_data_revision = pack.manifest.id;
        config.validate()?;
    }
    if options.exit_frames.is_some_and(|n| n < 100) {
        return Err("--frames must be at least 100 to allow initial rendering".into());
    }
    if let Some(path) = &options.screenshot {
        if let Some(parent) = std::path::Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
    }
    if options.connect.is_some() && (config.live_data || options.smoke) {
        return Err("--connect requires matching procedural, heightmap or historical server data; live APIs and --smoke require local sessions".into());
    }
    let connect = options.connect;
    let focus = config.focus;
    let (exit_handoff, exit_receiver) = flow::exit_handoff();
    let input_settings =
        controls_override.unwrap_or_else(|| player_input::InputSettings::read(&options.user_data));
    let mut app = App::new();
    if let Some(pack) = asset_pack {
        app.add_plugins(terra_bevy::render::AssetPackPlugin::new(pack));
    }
    app.insert_resource(options)
        .insert_resource(exit_handoff)
        .insert_resource(input_settings)
        .insert_resource(quality)
        .insert_resource(ClearColor(Color::srgb(0.46, 0.64, 0.71)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Terra World · Geospatial Sandbox".into(),
                resolution: (1440, 900).into(),
                resize_constraints: WindowResizeConstraints {
                    min_width: 1100.,
                    min_height: 700.,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            TerraWorldPlugin::new(config),
            TerraLivingPlugin,
            TerraRenderPlugin,
            player_input::PlayerInputPlugin,
            control::ActorControlPlugin,
            driving::DrivingPlugin,
            fleet::FleetPlugin,
            vehicle_presentation::VehiclePresentationPlugin,
            camera::ExplorerCameraPlugin,
            interface::InterfacePlugin,
            interaction::InteractionPlugin,
            remote::RemoteVisualPlugin,
            performance::DemoTelemetryPlugin,
            flow::GameFlowPlugin,
        ));
    if let Some(address) = connect {
        app.insert_resource(NetworkSession { path: session_path });
        let plugin = TerraNetworkPlugin {
            address,
            interest: focus,
            resume_token: None,
        };
        let security = app
            .world()
            .resource::<DemoOptions>()
            .network_security
            .clone();
        if let Some(security) = security {
            app.add_plugins(plugin.with_tls(security));
        } else {
            app.add_plugins(plugin);
        }
    }
    let mut managed_session = None;
    if connect.is_none() {
        // Install the chosen profile before the first regional checkpoint.
        // PreStartup restoration replaces this provisional state, so existing
        // worlds retain the character actually saved in their authoritative DB.
        let character = app.world().resource::<DemoOptions>().character.clone();
        if let Some(character) = character {
            let mut sandbox = app.world_mut().resource_mut::<terra_bevy::SandboxRuntime>();
            let player = sandbox.player_id;
            sandbox.state.set_character_profile(player, character)?;
        }
        if let Some(hour) = app.world().resource::<DemoOptions>().start_hour {
            let mut runtime = app.world_mut().resource_mut::<terra_bevy::WorldRuntime>();
            runtime.clock.unix_seconds =
                runtime.clock.unix_seconds.div_euclid(86400.) * 86400. + f64::from(hour) * 3600.;
        }
        if let Some(profile_path) = app
            .world()
            .resource::<DemoOptions>()
            .world_profile_path
            .clone()
        {
            let config = &app.world().resource::<terra_bevy::WorldRuntime>().config;
            let regional = config.region_store.is_some();
            let lease = launcher::model::begin_managed_session(&profile_path, config)?;
            let require_existing = lease.require_existing_progress;
            managed_session = Some(lease);
            app.world_mut()
                .resource_mut::<DemoOptions>()
                .require_existing_progress = require_existing;
            if require_existing && regional {
                app.world_mut()
                    .resource_mut::<terra_bevy::RegionalPersistence>()
                    .require_existing_checkpoint()?;
            }
        }
        restore_managed_json(&mut app)?;
    }
    let result = app.run();
    // The runner owns and drops the actual native world. App::run leaves this
    // handle empty; final data was captured after the checkpoint, before drop.
    drop(app);
    if result.is_error() {
        return Err("native runtime or requested verification failed".into());
    }
    let flow::ExitSnapshot {
        return_to_menu,
        user_data,
        menu_supervised,
        profile,
        settings,
        controls,
        config,
    } = exit_receiver
        .try_recv()
        .map_err(|_| "The world exited without completing its final session handoff.")?;
    let notice = profile
        .as_deref()
        .and_then(|path| launcher::model::persist_settings(path, settings, &config).err());
    // Retain exclusive managed ownership through both gameplay and the final
    // metadata update, then release before another menu/session can be launched.
    drop(managed_session);
    if return_to_menu {
        if menu_supervised {
            // Only the supervisor launches the next menu, after this game's
            // captured stderr closes. This avoids retaining an old supervisor
            // through an inherited pipe after every return-to-menu cycle.
            eprintln!(
                "{}{}",
                launcher::MENU_CONTROLS_PREFIX,
                controls.to_handoff()?
            );
            eprintln!(
                "{}{}",
                launcher::MENU_RETURN_PREFIX,
                serde_json::to_string(&notice)?
            );
        } else {
            launcher::spawn_menu(&user_data, notice.as_deref(), Some(&controls))?;
        }
    } else if let Some(notice) = notice {
        eprintln!(
            "Terra World: progress saved, but presentation settings could not be updated: {notice}"
        );
    }
    Ok(())
}

fn restore_managed_json(app: &mut App) -> Result<(), String> {
    let options = app.world().resource::<DemoOptions>();
    let managed = options.world_profile_path.is_some();
    let runtime = app.world().resource::<terra_bevy::WorldRuntime>();
    if !managed || runtime.config.region_store.is_some() {
        return Ok(());
    }
    let metadata = match std::fs::symlink_metadata(&runtime.config.save_path) {
        Ok(metadata) => metadata,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && !options.require_existing_progress =>
        {
            return Ok(());
        }
        Err(error) => {
            return Err(format!(
                "This managed world was already started, but its portable checkpoint cannot be restored: {error}. Restore a verified backup or create a separate new world; no fresh world was initialized."
            ));
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Managed JSON progress must be a regular file.".into());
    }
    let save = terra_bevy::sandbox::LocalWorldSave::read(
        &runtime.config.save_path,
        &runtime.config,
        &app.world().resource::<terra_bevy::SandboxRuntime>().catalog,
    )?;
    // Complete validation precedes replacement. A corrupt or mismatched snapshot
    // leaves the file intact and returns a visible launch failure to the menu.
    {
        let mut sandbox = app.world_mut().resource_mut::<terra_bevy::SandboxRuntime>();
        sandbox.state = save.state;
        sandbox.player_id = save.player_id;
        sandbox.revision += 1;
        sandbox.restore_revision = sandbox.restore_revision.wrapping_add(1);
        sandbox.dirty = false;
        sandbox.last_message = "Saved portable world restored.".into();
    }
    app.world_mut()
        .resource_mut::<terra_bevy::WorldRuntime>()
        .clock = save.clock;
    let mut politics = app.world_mut().resource_mut::<terra_bevy::PoliticalWorld>();
    politics.dataset = save.politics;
    politics.revision += 1;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Terra World: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod managed_startup_tests {
    use super::*;

    #[test]
    fn admitted_managed_json_restore_rejects_a_checkpoint_lost_before_runtime_startup() {
        let directory = std::env::temp_dir().join(format!(
            "terra-managed-json-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir(&directory).unwrap();
        let config = WorldConfig {
            save_path: directory.join("world-save.json"),
            ..Default::default()
        };
        let sandbox = terra_bevy::SandboxRuntime::new(config.focus);
        let original = serde_json::to_value(&sandbox.state).unwrap();
        let runtime = terra_bevy::WorldRuntime {
            focus: config.focus,
            frame: terra_core::LocalFrame::new(config.focus),
            config: config.clone(),
            clock: terra_core::WorldClock::default(),
            origin_revision: 0,
            weather: terra_core::WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let mut app = App::new();
        app.insert_resource(sandbox)
            .insert_resource(runtime)
            .insert_resource(terra_bevy::PoliticalWorld::load(None).unwrap())
            .insert_resource(DemoOptions {
                world_profile_path: Some(directory.join("world.json")),
                require_existing_progress: true,
                ..Default::default()
            });
        assert!(
            restore_managed_json(&mut app)
                .unwrap_err()
                .contains("no fresh world was initialized")
        );
        assert!(!config.save_path.exists());
        assert_eq!(
            serde_json::to_value(&app.world().resource::<terra_bevy::SandboxRuntime>().state)
                .unwrap(),
            original
        );
        assert_eq!(
            app.world()
                .resource::<terra_bevy::SandboxRuntime>()
                .restore_revision,
            0
        );
        std::fs::write(&config.save_path, b"damaged after launch review").unwrap();
        assert!(restore_managed_json(&mut app).is_err());
        assert_eq!(
            std::fs::read(&config.save_path).unwrap(),
            b"damaged after launch review"
        );
        assert_eq!(
            serde_json::to_value(&app.world().resource::<terra_bevy::SandboxRuntime>().state)
                .unwrap(),
            original
        );
        std::fs::remove_file(&config.save_path).unwrap();
        app.world_mut()
            .resource_mut::<DemoOptions>()
            .require_existing_progress = false;
        assert!(
            restore_managed_json(&mut app).is_ok(),
            "the exclusive first initializer still admits genuinely new progress"
        );
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
