mod camera;
mod control;
mod interaction;
mod interface;
mod remote;
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
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = WorldConfig::default();
    let mut options = DemoOptions {
        noon: true,
        ..Default::default()
    };
    let mut session_path = PathBuf::from("terra-session.json");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let next = |i: usize| {
            args.get(i + 1)
                .ok_or_else(|| format!("{} requires a value", args[i]))
        };
        match args[i].as_str() {
            "--help" | "-h" => {
                println!(
                    "Terra World 0.2 — living geographic sandbox\n\nUsage: cargo run -p terra-demo -- [options]\n  --config FILE           JSON WorldConfig\n  --lat NUMBER --lon NUMBER  WGS84 geographic focus\n  --live                  Opt in to terrain / OSM / weather demo adapters\n  --no-osm                Skip OpenStreetMap requests\n  --synthetic-weather     Keep deterministic game weather\n  --connect IP:PORT       Join authoritative LAN reference server\n  --session FILE          Private resume identity (default terra-session.json)\n  --time-scale NUMBER     Simulation seconds per real second (0 pauses time)\n  --radius N              Stream (2N+1)^2 chunks, maximum 5\n  --heightmap FILE        Import georeferenced 8/16-bit grayscale PNG manifest\n  --world-pack FILE       Load an explicitly dated immutable world pack\n  --epoch DATE            Source epoch YYYY-MM-DD or signed Unix seconds\n  --politics FILE         Dated political GeoJSON with provenance\n  --atlas                 Start in the political atlas\n  --camp-view             Start close to your player\n  --noon                  Set local daytime without changing the source epoch\n  --save FILE             Sandbox save path\n  --frames N              Exit after N updates\n  --screenshot FILE       Capture rendered scene\n  --smoke                 Exercise local sandbox automatically\n\nF1 RTS: click/drag select, right-click move. F2 third-person, F3 first-person.\nPossessed: WASD move, right-drag look, Shift sprint, R/F flight altitude, E interact, C switch view.\nF10 world settings, F11 inspector, Tab focus, F5 save, F9 load.\nDefault world is synthetic. --live reports provider and fallback provenance."
                );
                return Ok(());
            }
            "--config" => {
                config = serde_json::from_slice(&std::fs::read(next(i)?)?)?;
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
            .any(|a| matches!(a.as_str(), "--radius" | "--config"))
    {
        config.chunk_radius = 1;
        config.terrain_resolution = 33;
    }
    let bundled_politics =
        PathBuf::from("crates/terra-history/data/natural-earth-countries-2026-10-26.geojson");
    if config.politics_file.is_none()
        && config.historical_pack.is_none()
        && config.data_epoch_unix.is_none()
        && options.connect.is_none()
        && bundled_politics.exists()
    {
        config.politics_file = Some(bundled_politics);
    }
    config.validate()?;
    if let Some(path) = &config.heightmap_manifest {
        let provider = terra_stream::HeightmapProvider::open(path, config.terrain_resolution)?;
        let key = terra_core::TileKey::at(config.focus, config.chunk_level)?;
        if !provider.covers(key)
            && !args
                .iter()
                .any(|a| matches!(a.as_str(), "--lat" | "--lon" | "--config"))
        {
            config.focus = provider.manifest.bounds.point(0.5, 0.5, 0.)?;
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
        config.base_data_revision = provider.manifest.revision.clone();
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
    if let Some(path) = &options.screenshot
        && let Some(parent) = std::path::Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    if options.connect.is_some()
        && (config.live_data || config.heightmap_manifest.is_some() || options.smoke)
    {
        return Err("--connect requires matching procedural or historical server data; live APIs, heightmaps and --smoke require local sessions".into());
    }
    let connect = options.connect;
    let focus = config.focus;
    let mut app = App::new();
    app.insert_resource(options)
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
            control::ActorControlPlugin,
            camera::ExplorerCameraPlugin,
            interface::InterfacePlugin,
            interaction::InteractionPlugin,
            remote::RemoteVisualPlugin,
        ));
    if let Some(address) = connect {
        app.insert_resource(NetworkSession { path: session_path });
        app.add_plugins(TerraNetworkPlugin {
            address,
            interest: focus,
            resume_token: None,
        });
    }
    app.run();
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Terra World: {error}");
        std::process::exit(1);
    }
}
