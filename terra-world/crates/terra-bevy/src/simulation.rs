use crate::{config::WorldConfig, environment::WorldEnvironment, sandbox::SandboxRuntime};
use bevy::prelude::*;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};
use terra_core::{
    ActorKind, ActorState, GeoPosition, Layer, LocalFrame, TileKey, WeatherSample, WorldChunk,
    WorldClock, synthetic_weather,
};
use terra_stream::{ChunkStreamer, StreamEvent, StreamStats};

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum WorldSet {
    Stream,
    Simulate,
    Sandbox,
    Render,
}

#[derive(Resource)]
pub struct WorldRuntime {
    pub config: WorldConfig,
    pub focus: GeoPosition,
    pub frame: LocalFrame,
    pub origin_revision: u64,
    pub clock: WorldClock,
    pub weather: WeatherSample,
    pub stream_stats: StreamStats,
    pub last_error: Option<String>,
    pub stream_poll_ms: f64,
    pub simulation_ms: f64,
    pub rebases: u64,
    pub network_authority: bool,
}

impl WorldRuntime {
    /// Effective local gameplay/render weather. Disabled weather is a neutral,
    /// explicitly synthetic sample; cached source observations stay available in `weather`.
    pub fn effective_weather(&self) -> WeatherSample {
        if self.config.layers.contains(Layer::Weather) {
            self.weather
        } else {
            WeatherSample {
                temperature_c: 20.,
                precipitation_mm_h: 0.,
                cloud_cover: 0.,
                wind_speed_m_s: 0.,
                wind_direction_deg: 0.,
                observed_unix_seconds: self.clock.unix_seconds as i64,
                synthetic: true,
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct ResidentChunks {
    pub level: u8,
    pub chunks: BTreeMap<TileKey, Arc<WorldChunk>>,
}

#[derive(Resource)]
struct StreamResource(Mutex<ChunkStreamer>);

#[derive(Resource, Default)]
pub(crate) struct RenderQueue(pub VecDeque<StreamEvent>);

/// Absolute location for any game object. Kept accurate when the render origin shifts.
#[derive(Component, Clone, Copy, Debug)]
pub struct GeoAnchor(pub GeoPosition);

/// Optional simple locomotion. Games may replace this with their own controller.
#[derive(Component, Clone, Debug)]
pub struct GeoActor(pub ActorState);

#[derive(Resource, Default)]
struct WeatherTask {
    receiver: Option<Mutex<mpsc::Receiver<(GeoPosition, Result<WeatherSample, String>)>>>,
    last_requested: Option<GeoPosition>,
    last_fetch: Option<Instant>,
    last_sim_bucket: i64,
    last_synthetic_focus: Option<GeoPosition>,
}

pub struct TerraWorldPlugin {
    pub config: WorldConfig,
    provider: Option<Arc<dyn terra_stream::ChunkProvider>>,
}
impl TerraWorldPlugin {
    pub fn new(config: WorldConfig) -> Self {
        Self {
            config,
            provider: None,
        }
    }
}
impl TerraWorldPlugin {
    /// Supply game-owned data adapters while keeping bounded scheduling/cache integration.
    /// Set `base_data_revision` to a stable revision ID for save and network consistency.
    pub fn with_provider(
        config: WorldConfig,
        provider: Arc<dyn terra_stream::ChunkProvider>,
    ) -> Self {
        Self {
            config,
            provider: Some(provider),
        }
    }
}
impl Plugin for TerraWorldPlugin {
    fn build(&self, app: &mut App) {
        self.config
            .validate()
            .expect("invalid Terra World configuration");
        let mut config = self.config.clone();
        let mut clock = WorldClock::default();
        clock.time_scale = config.time_scale;
        if config.live_data {
            clock.unix_seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
        }
        if let Some(epoch) = config.data_epoch_unix {
            clock.unix_seconds = epoch as f64;
        }
        let politics = crate::politics::PoliticalWorld::load(config.politics_file.as_deref())
            .expect("invalid political dataset");
        if let Some(data) = &politics.dataset {
            config.political_dataset_revision = Some(data.id.clone());
            if config.data_epoch_unix.is_none()
                && !config.live_data
                && clock.unix_seconds < data.valid_from_unix as f64
            {
                clock.unix_seconds = data.valid_from_unix as f64 + 43200.;
            }
        }
        let weather = synthetic_weather(&config.focus, &clock, config.seed);
        let streamer = if let Some(provider) = &self.provider {
            ChunkStreamer::with_provider(config.stream_config(), provider.clone())
        } else {
            crate::history::build_streamer(&mut config)
        }
        .expect("cannot initialize world data");
        app.insert_resource(Time::<Fixed>::from_hz(config.fixed_hz))
            .insert_resource(WorldRuntime {
                focus: config.focus,
                frame: LocalFrame::new(config.focus),
                clock,
                weather,
                stream_stats: StreamStats::default(),
                last_error: None,
                stream_poll_ms: 0.0,
                simulation_ms: 0.0,
                origin_revision: 0,
                rebases: 0,
                network_authority: false,
                config: config.clone(),
            })
            .insert_resource(StreamResource(Mutex::new(streamer)))
            .insert_resource(ResidentChunks {
                level: config.chunk_level,
                chunks: BTreeMap::new(),
            })
            .init_resource::<WeatherTask>()
            .insert_resource(SandboxRuntime::new(config.focus))
            .insert_resource(politics)
            .init_resource::<crate::sandbox::SandboxQueue>()
            .configure_sets(
                Update,
                (WorldSet::Stream, WorldSet::Sandbox, WorldSet::Render).chain(),
            )
            .add_systems(
                Update,
                (stream_world, update_weather)
                    .chain()
                    .in_set(WorldSet::Stream),
            )
            .add_systems(FixedUpdate, advance_world.in_set(WorldSet::Simulate))
            .add_systems(
                Update,
                crate::sandbox::apply_sandbox_commands.in_set(WorldSet::Sandbox),
            );
    }
}

fn stream_world(
    mut runtime: ResMut<WorldRuntime>,
    stream: Res<StreamResource>,
    mut resident: ResMut<ResidentChunks>,
    mut queue: Option<ResMut<RenderQueue>>,
    mut sandbox: ResMut<SandboxRuntime>,
) {
    let start = Instant::now();
    if runtime.frame.origin.surface_distance_m(runtime.focus) > runtime.config.rebase_distance_m {
        runtime.frame = LocalFrame::new(GeoPosition {
            altitude_m: 0.0,
            ..runtime.focus
        });
        runtime.origin_revision += 1;
        runtime.rebases += 1;
    }
    let Ok(mut s) = stream.0.lock() else {
        runtime.last_error = Some("stream worker lock poisoned".into());
        return;
    };
    if let Err(e) = s.set_focus(
        runtime.focus,
        runtime.config.chunk_level,
        runtime.config.chunk_radius,
    ) {
        runtime.last_error = Some(e.to_string());
    }
    for event in s.poll(runtime.config.integrate_per_frame) {
        match &event {
            StreamEvent::Loaded(chunk) => {
                resident.chunks.insert(chunk.key, chunk.clone());
                if !runtime.network_authority {
                    let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
                    state.register_chunk(chunk);
                    // Tree-to-wood is the sample catalog's mapping, not a
                    // requirement imposed on a hosting game's item vocabulary.
                    // Preserve source metadata but never seed unsupported items.
                    state
                        .resource_nodes
                        .retain(|_, node| catalog.items.contains(&node.item));
                }
            }
            StreamEvent::Evicted(key) => {
                resident.chunks.remove(key);
            }
            StreamEvent::Failed { error, .. } => {
                runtime.last_error = Some(error.to_string());
            }
        }
        if let Some(queue) = queue.as_mut() {
            queue.0.push_back(event);
        }
    }
    runtime.stream_stats = s.stats();
    runtime.stream_poll_ms = start.elapsed().as_secs_f64() * 1000.0;
}

fn advance_world(
    time: Res<Time<Fixed>>,
    mut runtime: ResMut<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut actors: Query<&mut GeoActor>,
    mut sandbox: ResMut<SandboxRuntime>,
) {
    let start = Instant::now();
    let dt = time.delta_secs_f64();
    let weather = runtime.effective_weather();
    if !runtime.network_authority {
        let before = runtime.clock.unix_seconds;
        runtime.clock.advance(dt);
        let elapsed = runtime.clock.unix_seconds - before;
        if elapsed > 0.0 {
            if let Err(e) = sandbox.state.advance(elapsed, &weather) {
                runtime.last_error = Some(e.to_string());
            }
        }
        if runtime.config.layers.contains(Layer::Actors) && !runtime.clock.paused {
            for mut actor in &mut actors {
                let old = actor.0.position;
                if actor.0.kind == ActorKind::Balloon {
                    let angle = (weather.wind_direction_deg as f64).to_radians();
                    actor.0.velocity_enu_m_s[0] = -angle.sin() * weather.wind_speed_m_s as f64;
                    actor.0.velocity_enu_m_s[1] = -angle.cos() * weather.wind_speed_m_s as f64;
                }
                if actor.0.advance(dt).is_err() {
                    actor.0.position = old;
                    continue;
                }
                if let Some(env) = resident.sample_environment(actor.0.position, &weather) {
                    match actor.0.kind {
                        ActorKind::Human
                        | ActorKind::Animal
                        | ActorKind::Creature
                        | ActorKind::Vehicle => {
                            if env.water || env.slope_degrees > 45.0 {
                                actor.0.position = old;
                            } else {
                                actor.0.position.altitude_m = env.terrain_height_m + 0.05;
                            }
                        }
                        ActorKind::Ship => {
                            if !env.water {
                                actor.0.position = old;
                            } else {
                                actor.0.position.altitude_m = env.terrain_height_m.max(0.0) + 1.0;
                            }
                        }
                        _ => {
                            actor.0.position.altitude_m =
                                actor.0.position.altitude_m.max(env.terrain_height_m + 20.0);
                        }
                    }
                } else {
                    // A surface actor cannot run into unloaded/unknown terrain.
                    if matches!(
                        actor.0.kind,
                        ActorKind::Human
                            | ActorKind::Animal
                            | ActorKind::Creature
                            | ActorKind::Vehicle
                            | ActorKind::Ship
                    ) {
                        actor.0.position = old;
                    }
                }
            }
        }
    }
    runtime.simulation_ms = start.elapsed().as_secs_f64() * 1000.0;
}

fn update_weather(mut runtime: ResMut<WorldRuntime>, mut task: ResMut<WeatherTask>) {
    if !runtime.config.layers.contains(Layer::Weather) {
        return;
    }
    if runtime.network_authority {
        return;
    }
    if let Some(receiver) = &task.receiver {
        let message = receiver.lock().ok().and_then(|r| r.try_recv().ok());
        if let Some((at, result)) = message {
            task.receiver = None;
            if geo_distance(at, runtime.focus) < 20_000.0 {
                match result {
                    Ok(weather) => runtime.weather = weather,
                    Err(e) => runtime.last_error = Some(e),
                }
            }
        }
    }
    if !(runtime.config.live_data && runtime.config.live_weather) {
        let bucket = (runtime.clock.unix_seconds / 60.0) as i64;
        // Synthetic conditions depend on location as well as simulation time.
        // A paused clock must not pin the previous continent's weather after a
        // focus jump. This branch does no I/O; live-request spacing stays below.
        if bucket != task.last_sim_bucket || task.last_synthetic_focus != Some(runtime.focus) {
            runtime.weather =
                synthetic_weather(&runtime.focus, &runtime.clock, runtime.config.seed);
            task.last_sim_bucket = bucket;
            task.last_synthetic_focus = Some(runtime.focus);
        }
        return;
    }
    let due = task.last_fetch.is_none_or(|t| t.elapsed().as_secs() >= 600);
    let moved = task
        .last_requested
        .is_none_or(|p| geo_distance(p, runtime.focus) > 10_000.0)
        && task.last_fetch.is_none_or(|t| t.elapsed().as_secs() >= 10);
    if task.receiver.is_none() && (due || moved) {
        let focus = runtime.focus;
        let config = runtime.config.stream_config();
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let value = terra_stream::fetch_weather(focus, &config).map_err(|e| e.to_string());
            let _ = send.send((focus, value));
        });
        task.receiver = Some(Mutex::new(receive));
        task.last_requested = Some(focus);
        task.last_fetch = Some(Instant::now());
    }
}

pub fn geo_distance(a: GeoPosition, b: GeoPosition) -> f64 {
    a.surface_distance_m(b)
}

#[cfg(test)]
mod geo_tests {
    use super::*;
    #[test]
    fn whole_world_distance_does_not_collapse_at_antipodes() {
        let a = GeoPosition::new(0., 0., 0.).unwrap();
        let b = GeoPosition::new(0., 180., 0.).unwrap();
        assert!(geo_distance(a, b) > 20_000_000.);
    }
}
