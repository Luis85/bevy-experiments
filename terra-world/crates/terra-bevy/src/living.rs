//! Optional settlement adapter. The same pure rules drive a local Bevy world
//! and the authoritative server; clients render a replica without advancing it.
use crate::{
    ResidentChunks, SandboxRuntime, WorldConfig, WorldEnvironment, WorldRuntime, WorldSet,
    environment::{physical_surface, source_building_at_with_clearance},
};
use bevy::prelude::*;
use std::{cell::Cell, time::Instant};
use terra_core::{
    ActorKind, ActorState, GeoPosition, LocalFrame, TileKey, TravelMode, WeatherSample,
};

pub struct TerraLivingPlugin;

/// The stream scheduler's clipped/wrapped startup neighborhood. Readiness
/// callers count relevant resident or integrated keys and retain the existing
/// nine-page cap. Invalid positions or levels cannot admit startup.
pub fn startup_neighborhood(config: &WorldConfig, position: GeoPosition) -> Vec<TileKey> {
    TileKey::at(position, config.chunk_level)
        .map(|center| center.neighbors(config.chunk_radius))
        .unwrap_or_default()
}

/// Insert before adding TerraLivingPlugin to use the adapter with game-owned
/// population instead of the reference settlement.
#[derive(Resource, Debug, Clone)]
pub struct LivingSettings {
    pub seed_demo: bool,
    pub simulate: bool,
    pub collision: bool,
}
impl Default for LivingSettings {
    fn default() -> Self {
        Self {
            seed_demo: true,
            simulate: true,
            collision: true,
        }
    }
}

/// Host-generated input. This is not a network command carrying a position.
/// Network hosts receive velocity intent and perform their own environment checks.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct LivingMotion {
    pub active: bool,
    pub velocity_enu_m_s: [f64; 3],
    pub heading_deg: f32,
    /// Road vehicles accept steering and pedals, never a lateral velocity.
    pub drive: Option<terra_core::DriveInput>,
}

#[derive(Resource, Debug, Default)]
pub struct LivingDiagnostics {
    pub seeded: bool,
    pub population: usize,
    pub moving: usize,
    pub blocked_moves: u64,
    pub simulation_ms: f64,
    pub peak_simulation_ms: f64,
    pub fixed_steps: u64,
    pub last_error: Option<String>,
    pub ecology_work: terra_sandbox::EcologyScheduleStats,
}

/// Host-local work policy; never serialized into gameplay saves or advanced by clients.
#[derive(Resource, Debug, Default)]
pub struct EcologyRuntime {
    pub scheduler: terra_sandbox::EcologyScheduler,
}

impl Plugin for TerraLivingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LivingSettings>()
            .init_resource::<LivingMotion>()
            .init_resource::<LivingDiagnostics>()
            .init_resource::<EcologyRuntime>()
            .add_systems(
                Update,
                seed_population
                    .after(WorldSet::Stream)
                    .before(WorldSet::Sandbox),
            )
            .add_systems(FixedUpdate, advance_population.after(WorldSet::Simulate));
        app.add_systems(
            Update,
            seed_frontier
                .after(seed_population)
                .before(WorldSet::Sandbox),
        );
        app.add_systems(
            Update,
            seed_transport
                .after(seed_frontier)
                .before(WorldSet::Sandbox),
        );
    }
}

fn seed_transport(
    settings: Res<LivingSettings>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    persistence: Res<crate::RegionalPersistence>,
) {
    if runtime.network_authority
        || (persistence.blocked() || persistence.pending() || sandbox.state.is_partial())
        || !settings.seed_demo
        || !sandbox.state.living.enabled
        || (sandbox.state.transport.enabled
            && sandbox
                .state
                .living
                .accounts
                .keys()
                .all(|id| sandbox.state.transport.drivers.contains_key(id)))
    {
        return;
    }
    let first = !sandbox.state.transport.enabled;
    let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
    match state.ensure_transport_demo(catalog) {
        Ok(()) => {
            if first {
                for facility in state.transport.facilities.values_mut() {
                    if let Some(surface) =
                        physical_surface(&resident, facility.position, &runtime.effective_weather())
                    {
                        facility.position.altitude_m = surface.terrain_height_m;
                    }
                }
            }
            sandbox.dirty = true;
            sandbox.revision = sandbox.revision.wrapping_add(1);
        }
        Err(error) => sandbox.last_message = format!("Transport could not start: {error}"),
    }
}

/// Idempotent additive initialization also supports older saves whose living
/// population exists but whose ecology layer has not yet been authored.
fn seed_frontier(
    settings: Res<LivingSettings>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut diagnostics: ResMut<LivingDiagnostics>,
    persistence: Res<crate::RegionalPersistence>,
) {
    if runtime.network_authority
        || (persistence.blocked() || persistence.pending() || sandbox.state.is_partial())
        || !settings.seed_demo
        || !sandbox.state.living.enabled
    {
        return;
    }
    if !sandbox.state.ecology.config.enabled
        || sandbox
            .state
            .ecology
            .seeded_regions
            .contains("ecology:starter:v1")
    {
        return;
    }
    let Some(origin) = sandbox.state.living.origin else {
        return;
    };
    let weather = runtime.effective_weather();
    let mut obstacles: Vec<_> = sandbox
        .state
        .structures
        .values()
        .map(|s| (s.position, s.radius_m))
        .collect();
    obstacles.extend(sandbox.state.living.facility_footprints());
    let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
    if let Err(error) = state.seed_ecology_near_with_environment(origin, catalog, |p| {
        ground_candidate(&resident, p, ActorKind::Animal, &weather, true, &obstacles)
    }) {
        diagnostics.last_error = Some(format!("Ecology: {error}"));
    }
}

fn seed_population(
    settings: Res<LivingSettings>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut diagnostics: ResMut<LivingDiagnostics>,
    mut attempted: Local<bool>,
    persistence: Res<crate::RegionalPersistence>,
) {
    diagnostics.population = sandbox.state.living.units.len();
    diagnostics.seeded = sandbox.state.living.enabled;
    if runtime.network_authority
        || (persistence.blocked() || persistence.pending() || sandbox.state.is_partial())
        || sandbox.state.living.enabled
        || *attempted
        || !settings.seed_demo
    {
        return;
    }
    let Some(start) = sandbox
        .state
        .players
        .get(&sandbox.player_id)
        .map(|p| p.position)
    else {
        return;
    };
    // Use the scheduler's clipped/wrapped neighborhood: a radius-one polar
    // request has six valid tiles, not nine. Unrelated resident pages cannot
    // satisfy the player's startup readiness. Retain the nine-page startup cap.
    let neighborhood = startup_neighborhood(&runtime.config, start);
    let wanted = neighborhood.len().min(9);
    if wanted == 0
        || neighborhood
            .iter()
            .filter(|key| resident.chunks.contains_key(key))
            .count()
            < wanted
    {
        return;
    }
    let weather = runtime.effective_weather();
    let Some(origin) = find_safe_ground(&resident, start, ActorKind::Human, &weather) else {
        diagnostics.last_error =
            Some("The settlement needs loaded, dry, walkable land near its spawn.".into());
        return;
    };
    let features: Vec<_> = resident
        .chunks
        .values()
        .flat_map(|c| &c.features)
        .filter(|f| {
            f.points
                .iter()
                .any(|p| p.surface_distance_m(origin) < 1500.)
        })
        .take(8000)
        .cloned()
        .collect();
    let player = sandbox.player_id;
    let result = {
        let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
        state
            .set_player_position(player, origin)
            .and_then(|()| state.seed_living_demo(origin, &features, catalog))
    };
    *attempted = true;
    match result {
        Ok(()) => {
            ground_initial_population(&mut sandbox, &resident, &weather);
            sandbox.revision = sandbox.revision.wrapping_add(1);
            sandbox.dirty = true;
            sandbox.last_message =
                "Settlement ready. Select an actor, issue an order, or press F2 to take control."
                    .into();
            diagnostics.seeded = true;
            diagnostics.population = sandbox.state.living.units.len();
            diagnostics.last_error = None;
        }
        Err(error) => {
            diagnostics.last_error = Some(error.to_string());
            sandbox.last_message = format!("Living world could not start: {error}");
        }
    }
}

fn find_safe_ground(
    resident: &ResidentChunks,
    position: GeoPosition,
    kind: ActorKind,
    weather: &WeatherSample,
) -> Option<GeoPosition> {
    let frame = LocalFrame::new(position);
    for radius in [0., 3., 7., 15., 30., 60., 100.] {
        for angle in 0..if radius == 0. { 1 } else { 12 } {
            let theta = angle as f64 * std::f64::consts::TAU / 12.;
            let candidate = frame
                .to_geo([radius * theta.cos(), 0., radius * theta.sin()])
                .ok()?;
            if let Some(grounded) = ground_candidate(resident, candidate, kind, weather, true, &[])
            {
                return Some(grounded);
            }
        }
    }
    None
}

fn ground_initial_population(
    sandbox: &mut SandboxRuntime,
    resident: &ResidentChunks,
    weather: &WeatherSample,
) {
    let mut moved_facilities = Vec::new();
    for shop in sandbox.state.living.shops.values_mut() {
        if let Some(position) = find_safe_ground(resident, shop.position, ActorKind::Human, weather)
        {
            moved_facilities.push((shop.position, position));
            shop.position = position;
        }
    }
    for site in sandbox.state.living.producers.values_mut() {
        if let Some(position) = find_safe_ground(resident, site.position, ActorKind::Human, weather)
        {
            moved_facilities.push((site.position, position));
            site.position = position;
        }
    }
    let remap = |position: GeoPosition| {
        moved_facilities
            .iter()
            .find(|(old, _)| old.surface_distance_m(position) < 0.5)
            .map(|(_, new)| *new)
            .unwrap_or_else(|| {
                let mut p = position;
                if let Some(h) = resident.terrain_height(p) {
                    p.altitude_m = h;
                }
                p
            })
    };
    for unit in sandbox.state.living.units.values_mut() {
        if let Some(position) = find_safe_ground(resident, unit.position, unit.kind, weather) {
            unit.position = position;
        }
        unit.home = remap(unit.home);
        for point in &mut unit.path {
            *point = remap(*point);
        }
        for point in &mut unit.patrol {
            *point = remap(*point);
        }
    }
    for road in &mut sandbox.state.living.roads {
        for point in &mut road.points {
            *point = remap(*point);
        }
    }
    for account in sandbox.state.living.accounts.values() {
        if let Some(unit) = sandbox.state.living.units.get(&account.active_unit) {
            if let Some(player) = sandbox.state.players.get_mut(&account.player_id) {
                player.position = unit.position;
            }
        }
    }
}

pub(crate) fn ground_candidate(
    resident: &ResidentChunks,
    mut candidate: GeoPosition,
    kind: ActorKind,
    weather: &WeatherSample,
    collision: bool,
    structures: &[(GeoPosition, f64)],
) -> Option<GeoPosition> {
    let surface = physical_surface(resident, candidate, weather)?;
    match kind.travel_mode() {
        TravelMode::Ground => {
            let radius = if kind == ActorKind::Vehicle {
                1.15
            } else {
                0.32
            };
            if surface.water
                || surface.slope_degrees > if kind == ActorKind::Vehicle { 32. } else { 44. }
            {
                return None;
            }
            if collision
                && (source_building_at_with_clearance(
                    resident,
                    candidate,
                    radius,
                    match kind {
                        ActorKind::Vehicle => 8.,
                        ActorKind::Animal => 1.3,
                        ActorKind::Creature => 2.,
                        _ => 1.8,
                    },
                ) || structures
                    .iter()
                    .any(|(p, r)| candidate.surface_distance_m(*p) < r + radius))
            {
                return None;
            }
            candidate.altitude_m = surface.terrain_height_m + 0.025;
        }
        TravelMode::Water => {
            if !surface.water {
                return None;
            }
            candidate.altitude_m = surface.terrain_height_m.max(0.) + 0.35;
        }
        TravelMode::Air => {
            candidate.altitude_m = candidate.altitude_m.max(surface.terrain_height_m + 3.);
        }
    }
    Some(candidate)
}

/// Bounded local steering around a static footprint. It is intentionally a
/// reference surface controller, not a replacement for a game's rigid bodies
/// or full navigation mesh. All alternatives still require loaded valid ground.
fn resolve_step(
    resident: &ResidentChunks,
    previous: GeoPosition,
    candidate: GeoPosition,
    kind: ActorKind,
    weather: &WeatherSample,
    collision: bool,
    structures: &[(GeoPosition, f64)],
) -> Option<GeoPosition> {
    let accept = |candidate| {
        if collision && !terra_sandbox::facility_path_clear(kind, previous, candidate, structures) {
            return None;
        }
        // Swept validation permits outward escape from an older overlapping
        // save. An endpoint-only collider would trap that body permanently.
        ground_candidate(resident, candidate, kind, weather, collision, &[])
    };
    if let Some(valid) = accept(candidate) {
        return Some(valid);
    }
    if kind.travel_mode() != TravelMode::Ground {
        return None;
    }
    let frame = LocalFrame::new(previous);
    let delta = frame.to_local(candidate);
    // East-only/north-only sliding preserves movement along a wall. The two
    // perpendicular attempts let road-following couriers round a nearby corner.
    for trial in [
        [delta[0], 0., 0.],
        [0., 0., delta[2]],
        [delta[2] * 0.7, 0., -delta[0] * 0.7],
        [-delta[2] * 0.7, 0., delta[0] * 0.7],
    ] {
        if trial[0].hypot(trial[2]) < 0.0001 {
            continue;
        }
        let Some(p) = frame.to_geo(trial).ok() else {
            continue;
        };
        if let Some(valid) = accept(p) {
            return Some(valid);
        }
    }
    None
}

fn advance_population(
    time: Res<Time<Fixed>>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    settings: Res<LivingSettings>,
    motion: Res<LivingMotion>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut diagnostics: ResMut<LivingDiagnostics>,
    mut ecology: ResMut<EcologyRuntime>,
    mut timings: ResMut<crate::WorldDiagnostics>,
    persistence: Res<crate::RegionalPersistence>,
) {
    if runtime.network_authority
        || persistence.authority_paused()
        || !sandbox.state.living.enabled
        || !settings.simulate
    {
        return;
    }
    let start = Instant::now();
    let dt = time.delta_secs_f64().clamp(0., 0.25);
    if dt == 0. || runtime.clock.paused {
        return;
    }
    diagnostics.last_error = None;
    let weather = runtime.effective_weather();
    let mut structures: Vec<_> = sandbox
        .state
        .structures
        .values()
        .map(|s| (s.position, s.radius_m))
        .collect();
    structures.extend(sandbox.state.living.facility_footprints());
    let blocked = Cell::new(0_u64);
    let resolver = |kind, previous, candidate| {
        let resolved = resolve_step(
            &resident,
            previous,
            candidate,
            kind,
            &weather,
            settings.collision,
            &structures,
        );
        if resolved.is_none() {
            blocked.set(blocked.get().saturating_add(1));
        }
        resolved
    };
    let sim_dt = dt * runtime.clock.time_scale;
    // Retained offline bodies are durable occupancy, not active observers.
    // Match the server's connected-owner policy without moving or deleting them.
    let observers: Vec<_> = sandbox
        .state
        .living
        .units
        .values()
        .filter(|unit| {
            unit.owner
                .is_some_and(|owner| !sandbox.state.is_player_dormant(owner))
        })
        .map(|unit| unit.position)
        .collect();
    {
        let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
        let hour = (runtime.clock.utc_hour() + runtime.focus.longitude / 15.).rem_euclid(24.);
        let stage = Instant::now();
        if let Err(error) = state.advance_ecology_scheduled_with_environment(
            dt,
            sim_dt,
            hour,
            &mut ecology.scheduler,
            &observers,
            |p| {
                ground_candidate(
                    &resident,
                    p,
                    ActorKind::Animal,
                    &weather,
                    settings.collision,
                    &structures,
                )
            },
        ) {
            diagnostics.last_error = Some(format!("Ecology: {error}"));
        }
        timings.record(
            crate::WorldStage::Ecology,
            stage.elapsed().as_secs_f64() * 1000.,
        );
        diagnostics.ecology_work = ecology.scheduler.stats().clone();
        let stage = Instant::now();
        if let Err(error) = state.advance_living_with_vehicle_environment(
            dt,
            sim_dt,
            catalog,
            settings.collision,
            &resolver,
            terra_sandbox::AutonomousVehicleEnvironment {
                sample: &|p| crate::environment::vehicle_ground_sample(&resident, p, &weather),
                resolve: &|from, to, dimensions| {
                    resolve_autonomous_vehicle(
                        &resident,
                        &weather,
                        &structures,
                        settings.collision,
                        from,
                        to,
                        dimensions,
                    )
                },
                weather_grip: terra_core::road_weather_grip(
                    weather.precipitation_mm_h,
                    weather.temperature_c,
                ),
            },
        ) {
            diagnostics.last_error = Some(error.to_string());
        }
        timings.record(
            crate::WorldStage::Living,
            stage.elapsed().as_secs_f64() * 1000.,
        );
        let stage = Instant::now();
        if let Err(error) = state.advance_settlements(sim_dt, catalog) {
            diagnostics.last_error = Some(format!("Settlements: {error}"));
        }
        timings.record(
            crate::WorldStage::Economy,
            stage.elapsed().as_secs_f64() * 1000.,
        );
    }
    let stage = Instant::now();
    let player = sandbox.player_id;
    if sandbox
        .state
        .living
        .accounts
        .get(&player)
        .is_some_and(|a| a.possessed)
    {
        if let Some(unit) = sandbox.state.living.active_unit(player).cloned() {
            if sandbox.state.transport.vehicles.contains_key(&unit.id) {
                if let Err(error) = drive_controlled_vehicle(
                    &mut sandbox,
                    &resident,
                    &weather,
                    &structures,
                    settings.collision,
                    if motion.active {
                        motion.drive.unwrap_or_else(terra_core::DriveInput::stopped)
                    } else {
                        terra_core::DriveInput::stopped()
                    },
                    dt,
                ) {
                    diagnostics.last_error = Some(error);
                }
            } else if motion.active {
                let speed = motion
                    .velocity_enu_m_s
                    .iter()
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt();
                let cap = unit.speed_limit_m_s();
                let velocity = if speed > cap {
                    motion.velocity_enu_m_s.map(|v| v * cap / speed)
                } else {
                    motion.velocity_enu_m_s
                };
                let mut actor = ActorState {
                    id: unit.id,
                    kind: unit.kind,
                    position: unit.position,
                    velocity_enu_m_s: velocity,
                    heading_deg: motion.heading_deg,
                };
                if actor.advance(dt).is_ok() {
                    let (mut position, mut velocity) =
                        match resolver(unit.kind, unit.position, actor.position) {
                            Some(position) => (position, velocity),
                            None => (unit.position, [0.; 3]),
                        };
                    if settings.collision {
                        match terra_sandbox::LivingContacts::new(&sandbox.state).and_then(
                            |contacts| {
                                contacts.resolve(
                                    unit.id,
                                    unit.position,
                                    unit.heading_deg,
                                    position,
                                    motion.heading_deg,
                                    velocity,
                                )
                            },
                        ) {
                            Ok(admitted) => {
                                position = admitted.position;
                                if admitted.contact.is_some() {
                                    velocity = [0.; 3];
                                }
                            }
                            Err(error) => {
                                position = unit.position;
                                velocity = [0.; 3];
                                diagnostics.last_error = Some(error.to_string());
                            }
                        }
                    }
                    if let Err(error) = sandbox.state.set_controlled_motion(
                        player,
                        position,
                        velocity,
                        motion.heading_deg,
                    ) {
                        diagnostics.last_error = Some(error.to_string());
                    }
                }
            }
        }
    }
    timings.record(
        crate::WorldStage::ControlledMotion,
        stage.elapsed().as_secs_f64() * 1000.,
    );
    diagnostics.population = sandbox.state.living.units.len();
    diagnostics.moving = sandbox
        .state
        .living
        .units
        .values()
        .filter(|u| u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) > 0.1)
        .count();
    diagnostics.blocked_moves = diagnostics.blocked_moves.saturating_add(blocked.get());
    diagnostics.simulation_ms = start.elapsed().as_secs_f64() * 1000.;
    diagnostics.peak_simulation_ms = diagnostics
        .peak_simulation_ms
        .max(diagnostics.simulation_ms);
    diagnostics.fixed_steps = diagnostics.fixed_steps.saturating_add(1);
    sandbox.dirty = true;
}

/// Full source envelope for autonomous vehicles. Like manual admission it
/// never slides a vehicle sideways around an obstruction or invents support.
#[allow(clippy::too_many_arguments)]
fn resolve_autonomous_vehicle(
    resident: &ResidentChunks,
    weather: &WeatherSample,
    structures: &[(GeoPosition, f64)],
    collision: bool,
    from: &ActorState,
    to: &ActorState,
    dimensions: [f64; 3],
) -> Option<GeoPosition> {
    let frame = LocalFrame::new(from.position);
    let delta = frame.to_local(to.position);
    let angle = (f64::from(to.heading_deg - from.heading_deg) + 540.).rem_euclid(360.) - 180.;
    let required = ((from.position.surface_distance_m(to.position)
        + dimensions[0].hypot(dimensions[2]) * 0.5 * angle.abs().to_radians())
        / 0.25)
        .ceil() as usize;
    if required > 512
        || (collision
            && !terra_sandbox::facility_path_clear(
                ActorKind::Vehicle,
                from.position,
                to.position,
                structures,
            ))
    {
        return None;
    }
    let count = required.max(1);
    let mut previous =
        terra_core::vehicle_support_points(from.position, f64::from(from.heading_deg), dimensions)
            .ok()?;
    for n in 1..=count {
        let fraction = n as f64 / count as f64;
        let p = frame.to_geo(delta.map(|x| x * fraction)).ok()?;
        let points = terra_core::vehicle_support_points(
            p,
            f64::from(from.heading_deg) + angle * fraction,
            dimensions,
        )
        .ok()?;
        for (&old, &point) in previous.iter().zip(&points) {
            ground_candidate(resident, point, ActorKind::Vehicle, weather, false, &[])?;
            if collision
                && (source_building_at_with_clearance(resident, point, 0.2, dimensions[1])
                    || !terra_sandbox::facility_path_clear(
                        ActorKind::Human,
                        old,
                        point,
                        structures,
                    ))
            {
                return None;
            }
        }
        previous = points;
    }
    ground_candidate(
        resident,
        to.position,
        ActorKind::Vehicle,
        weather,
        false,
        &[],
    )
}

/// Surface admission bounds translation and corner travel to quarter-metre steps. A car cannot slide
/// sideways around a wall, tunnel through an imported building, or enter an
/// unloaded tile. Missing coverage stops it without charging collision damage.
fn drive_controlled_vehicle(
    sandbox: &mut SandboxRuntime,
    resident: &ResidentChunks,
    weather: &WeatherSample,
    structures: &[(GeoPosition, f64)],
    collision: bool,
    input: terra_core::DriveInput,
    dt: f64,
) -> Result<(), String> {
    let player = sandbox.player_id;
    let Some(unit) = sandbox.state.living.active_unit(player).cloned() else {
        return Ok(());
    };
    let Some(vehicle) = sandbox.state.transport.vehicles.get(&unit.id).cloned() else {
        return Ok(());
    };
    let mut dynamics = vehicle.dynamics;
    let config = sandbox.state.transport.vehicle_dynamics_config(unit.id);
    let support = config.and_then(|config| {
        terra_core::sample_vehicle_ground(
            unit.position,
            f64::from(unit.heading_deg),
            config.wheelbase_m,
            vehicle.model.dimensions_m()[0] * 0.86,
            |p| {
                crate::environment::vehicle_ground_sample(resident, p, weather)
                    .map(|s| sandbox.state.living.apply_authored_road_surface(p, s))
            },
        )
        .ok()
    });
    let cap = sandbox
        .state
        .vehicle_speed_limit_m_s(unit.id)
        .unwrap_or(0.)
        .min(unit.speed_limit_m_s());
    if cap <= 0. || config.is_none() || support.is_none() {
        dynamics.stop();
        if vehicle.fuel_l <= 0. {
            dynamics.engine_rpm = 0.;
        }
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&unit.id)
            .unwrap()
            .dynamics = dynamics;
        return sandbox
            .state
            .set_controlled_motion(player, unit.position, [0.; 3], unit.heading_deg)
            .map_err(|e| e.to_string());
    }
    let mut config = config.unwrap();
    let support = support.unwrap();
    config.max_speed_m_s = cap.max(1.);
    let frame = LocalFrame::new(unit.position);
    let actor = ActorState {
        id: unit.id,
        kind: unit.kind,
        position: unit.position,
        velocity_enu_m_s: unit.velocity_enu_m_s,
        heading_deg: unit.heading_deg,
    };
    let mut candidate = terra_core::drive_actor(
        &actor,
        &mut dynamics,
        &config,
        input,
        dt,
        support.grip_scale(terra_core::road_weather_grip(
            weather.precipitation_mm_h,
            weather.temperature_c,
        )),
        support.pitch_radians,
    )
    .map_err(|e| e.to_string())?;
    let delta = frame.to_local(candidate.position);
    let angle =
        (f64::from(candidate.heading_deg - unit.heading_deg) + 540.).rem_euclid(360.) - 180.;
    let dimensions = vehicle.model.dimensions_m();
    let corner_radius = dimensions[0].hypot(dimensions[2]) * 0.5;
    let required = ((unit.position.surface_distance_m(candidate.position)
        + corner_radius * angle.abs().to_radians())
        / 0.25)
        .ceil() as usize;
    if required > 512 {
        return Err("vehicle source envelope budget exceeded".into());
    }
    let steps = required.max(1);
    let mut previous = terra_core::vehicle_support_points(
        unit.position,
        f64::from(unit.heading_deg),
        vehicle.model.dimensions_m(),
    )
    .map_err(|e| e.to_string())?;
    let envelope: Vec<_> = (1..=steps)
        .map(|n| {
            let position = frame
                .to_geo(delta.map(|x| x * n as f64 / steps as f64))
                .map_err(|e| e.to_string())?;
            terra_core::vehicle_support_points(
                position,
                f64::from(unit.heading_deg) + angle * n as f64 / steps as f64,
                vehicle.model.dimensions_m(),
            )
            .map_err(|e| e.to_string())
        })
        .collect::<Result<_, String>>()?;
    // Check the complete swept envelope before classifying a rejected step as
    // an impact. A loaded centre with an unknown front/corner is a streaming
    // boundary, not evidence of a wall or other physical collision.
    let covered = envelope
        .iter()
        .flatten()
        .all(|p| physical_surface(resident, *p, weather).is_some());
    let admitted = covered
        && (!collision
            || terra_sandbox::facility_path_clear(
                unit.kind,
                unit.position,
                candidate.position,
                structures,
            ))
        && envelope.iter().all(|points| {
            let clear = previous.iter().zip(points).all(|(from, p)| {
                ground_candidate(resident, *p, unit.kind, weather, false, &[]).is_some()
                    && (!collision
                        || (!source_building_at_with_clearance(
                            resident,
                            *p,
                            0.2,
                            vehicle.model.dimensions_m()[1],
                        ) && terra_sandbox::facility_path_clear(
                            ActorKind::Human,
                            *from,
                            *p,
                            structures,
                        )))
            });
            previous = *points;
            clear
        });
    let mut impact = 0.;
    if admitted {
        candidate.position =
            ground_candidate(resident, candidate.position, unit.kind, weather, false, &[]).unwrap();
        if collision {
            let accepted = terra_sandbox::LivingContacts::new(&sandbox.state)
                .and_then(|contacts| {
                    contacts.resolve(
                        unit.id,
                        unit.position,
                        unit.heading_deg,
                        candidate.position,
                        candidate.heading_deg,
                        candidate.velocity_enu_m_s,
                    )
                })
                .map_err(|e| e.to_string())?;
            candidate.position = accepted.position;
            candidate.heading_deg = accepted.heading_deg;
            if accepted.contact.is_some() {
                impact = accepted.impact_speed_m_s;
                candidate.velocity_enu_m_s = [0.; 3];
                dynamics.stop();
            }
        }
    } else {
        if covered {
            impact = dynamics.speed_m_s.abs();
        }
        candidate.position = unit.position;
        candidate.velocity_enu_m_s = [0.; 3];
        candidate.heading_deg = unit.heading_deg;
        dynamics.stop();
    }
    sandbox
        .state
        .set_controlled_motion(
            player,
            candidate.position,
            candidate.velocity_enu_m_s,
            candidate.heading_deg,
        )
        .map_err(|e| e.to_string())?;
    sandbox
        .state
        .transport
        .vehicles
        .get_mut(&unit.id)
        .unwrap()
        .dynamics = dynamics;
    sandbox
        .state
        .record_vehicle_motion(
            player,
            unit.id,
            unit.position.surface_distance_m(candidate.position),
            dt,
            impact,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::{collections::BTreeMap, sync::Arc};
    use terra_core::{FeatureKind, HeightField, TileKey, WorldChunk, WorldFeature};
    use terra_sandbox::{Catalog, Command, LivingCommand, SandboxState};

    fn flat() -> (ResidentChunks, GeoPosition) {
        let origin = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let key = TileKey::at(origin, 15).unwrap();
        let chunk = WorldChunk {
            key,
            heights: HeightField::constant(10.),
            features: vec![],
            sources: vec![],
            warnings: vec![],
        };
        (
            ResidentChunks {
                level: 15,
                chunks: BTreeMap::from([(key, Arc::new(chunk))]),
            },
            origin,
        )
    }

    fn moving_driver(position: GeoPosition) -> (SandboxRuntime, u64) {
        let mut sandbox = SandboxRuntime::new(position);
        sandbox
            .state
            .seed_living_demo(position, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let id = *sandbox.state.transport.vehicles.keys().next().unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::Possess { unit_id: id },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let unit = sandbox.state.living.units.get_mut(&id).unwrap();
        unit.position = position;
        unit.heading_deg = 90.;
        unit.velocity_enu_m_s = [10., 0., 0.];
        sandbox.state.players.get_mut(&1).unwrap().position = position;
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .dynamics
            .speed_m_s = 10.;
        (sandbox, id)
    }

    #[test]
    fn saved_loaded_fleet_clears_perpendicular_traffic_and_delivers_on_imported_heightmap() {
        use terra_sandbox::{
            CompanyCommand, FleetService, FleetStatus, TransportCommand, UnitOrder, UnitRole,
        };
        use terra_stream::{ChunkProvider, HeightmapProvider};

        let mut origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let provider = HeightmapProvider::open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../terra-stream/data/heightmap-demo/manifest.json"),
            65,
        )
        .unwrap();
        let mut resident = ResidentChunks {
            level: 15,
            ..Default::default()
        };
        for key in TileKey::at(origin, 15).unwrap().neighbors(3) {
            resident
                .chunks
                .insert(key, Arc::new(provider.load(key).unwrap().chunk));
        }
        let weather = WeatherSample::default();
        origin.altitude_m = physical_surface(&resident, origin, &weather)
            .unwrap()
            .terrain_height_m
            + 0.025;
        let frame = LocalFrame::new(origin);
        let ground = |east, south| {
            ground_candidate(
                &resident,
                frame.to_geo([east, 0., south]).unwrap(),
                ActorKind::Vehicle,
                &weather,
                true,
                &[],
            )
            .unwrap()
        };
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        ground_initial_population(&mut sandbox, &resident, &weather);
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        for facility in sandbox.state.transport.facilities.values_mut() {
            facility.position.altitude_m = physical_surface(&resident, facility.position, &weather)
                .unwrap()
                .terrain_height_m;
        }
        let vehicle = *sandbox.state.transport.vehicles.keys().next().unwrap();
        let traffic = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.role == UnitRole::Traffic)
            .unwrap()
            .id;
        sandbox
            .state
            .living
            .units
            .retain(|id, u| u.owner.is_some() || *id == traffic);
        let company = |sandbox: &mut SandboxRuntime, action| {
            sandbox
                .state
                .apply(
                    1,
                    Command::Transport {
                        action: TransportCommand::Company { action },
                    },
                    &sandbox.catalog,
                )
                .unwrap()
        };
        company(
            &mut sandbox,
            CompanyCommand::Create {
                name: "Heightmap transit".into(),
            },
        );
        company(&mut sandbox, CompanyCommand::Deposit { amount: 10_000 });
        let driver = company(
            &mut sandbox,
            CompanyCommand::HireDriver { name: "Ari".into() },
        )
        .created_id
        .unwrap();
        let route = company(
            &mut sandbox,
            CompanyCommand::CreateRoute {
                name: "Town to Hillside".into(),
                stops: vec![2, 3],
                service: FleetService::Passengers { count: 1 },
            },
        )
        .created_id
        .unwrap();
        company(
            &mut sandbox,
            CompanyCommand::Assign {
                driver_id: driver,
                vehicle_id: vehicle,
                route_id: route,
            },
        );
        company(
            &mut sandbox,
            CompanyCommand::SetDriverRunning {
                driver_id: driver,
                running: true,
            },
        );
        sandbox
            .state
            .living
            .units
            .get_mut(&vehicle)
            .unwrap()
            .position = ground(-6., 1.);
        let structures = sandbox.state.living.facility_footprints();
        let resolver =
            |kind, from, to| resolve_step(&resident, from, to, kind, &weather, true, &structures);
        let waiting = sandbox.state.transport.facilities[&3].passengers_waiting;
        sandbox
            .state
            .advance_living_with_environment(0.05, 0., &sandbox.catalog, resolver)
            .unwrap();
        assert!(
            sandbox.state.transport.companies[&1].drivers[0]
                .trip
                .is_some()
        );

        // Reproduce the saved native blockage: the loaded pickup faces north
        // immediately beside an eastbound traffic nose and must turn west.
        let unit = sandbox.state.living.units.get_mut(&vehicle).unwrap();
        unit.position = ground(-6., -1.);
        unit.heading_deg = 0.;
        unit.velocity_enu_m_s = [0.; 3];
        unit.order = UnitOrder::Idle;
        unit.path.clear();
        unit.path_index = 0;
        let other = sandbox.state.living.units.get_mut(&traffic).unwrap();
        other.position = ground(-9., 1.5);
        other.heading_deg = 90.;
        other.velocity_enu_m_s = [0.; 3];
        other.order = UnitOrder::Patrol;
        other.patrol = vec![other.position, ground(40., 1.5)];
        other.path_index = 1;
        other.patrol_reverse = false;
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&vehicle)
            .unwrap()
            .parking_brake = true;
        let driver_state = &mut sandbox
            .state
            .transport
            .companies
            .get_mut(&1)
            .unwrap()
            .drivers[0];
        driver_state.status = FleetStatus::RouteBlocked;
        driver_state.stalled_seconds = 15.;
        let committed = driver_state.trip.clone().unwrap();
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
        sandbox.state =
            serde_json::from_slice(&serde_json::to_vec(&sandbox.state).unwrap()).unwrap();
        assert_eq!(
            sandbox.state.transport.companies[&1].drivers[0]
                .trip
                .as_ref(),
            Some(&committed)
        );
        company(
            &mut sandbox,
            CompanyCommand::SetDriverRunning {
                driver_id: driver,
                running: true,
            },
        );
        sandbox.state.prepare_company_drivers(0.05).unwrap();

        let unit = &sandbox.state.living.units[&vehicle];
        let west = ground(-6.5, -1.);
        assert!(resolver(ActorKind::Vehicle, unit.position, west).is_some());
        let contact = terra_sandbox::LivingContacts::new(&sandbox.state)
            .unwrap()
            .resolve(vehicle, unit.position, 0., west, 270., [-10., 0., 0.])
            .unwrap()
            .contact
            .unwrap();
        assert_eq!(contact.other_id, traffic);
        assert!(
            contact.fraction < 0.1,
            "turn blocked near its start: {contact:?}"
        );

        let money = |state: &SandboxState| {
            let c = &state.transport.companies[&1];
            state.transport.treasury
                + c.cash
                + c.payroll_paid()
                + c.drivers
                    .iter()
                    .filter_map(|d| d.trip.as_ref())
                    .map(|t| t.reward_escrow)
                    .sum::<u64>()
        };
        let before_money = money(&sandbox.state);
        for _ in 0..900 {
            let before = sandbox.state.living.units[&vehicle].position;
            sandbox
                .state
                .advance_living_with_environment(0.05, 0., &sandbox.catalog, resolver)
                .unwrap();
            let unit = &sandbox.state.living.units[&vehicle];
            assert!(before.surface_distance_m(unit.position) < 0.501);
            let surface = physical_surface(&resident, unit.position, &weather).unwrap();
            assert!(!surface.water && surface.slope_degrees < 32.);
            assert!((unit.position.altitude_m - surface.terrain_height_m - 0.025).abs() < 0.0001);
            assert_eq!(money(&sandbox.state), before_money);
            if sandbox.state.transport.companies[&1].drivers[0].deliveries_completed > 0 {
                break;
            }
        }
        let c = &sandbox.state.transport.companies[&1];
        assert_eq!(c.drivers[0].deliveries_completed, 1);
        assert!(c.drivers[0].trip.is_none());
        assert_eq!(c.revenue, committed.reward_escrow);
        assert_eq!(
            sandbox.state.transport.facilities[&3].passengers_waiting,
            waiting + 1
        );
        assert!(sandbox.state.transport.vehicles[&vehicle].odometer_m > 150.);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
    }

    #[test]
    fn absent_manual_input_brakes_possessed_vehicle_but_true_pause_freezes_it() {
        let (resident, original) = flat();
        let mut origin = TileKey::at(original, 15).unwrap().center();
        origin.altitude_m = 10.;
        let (sandbox, id) = moving_driver(origin);
        let mut world = World::new();
        let mut time = Time::<Fixed>::from_hz(20.);
        time.advance_by(std::time::Duration::from_millis(50));
        world.insert_resource(time);
        world.insert_resource(WorldRuntime {
            config: crate::WorldConfig::default(),
            focus: origin,
            frame: LocalFrame::new(origin),
            clock: terra_core::WorldClock {
                time_scale: 0.,
                ..Default::default()
            },
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            origin_revision: 0,
            rebases: 0,
            network_authority: false,
        });
        world.insert_resource(resident);
        world.insert_resource(sandbox);
        world.insert_resource(LivingSettings {
            seed_demo: false,
            simulate: true,
            collision: false,
        });
        // Inactive intent must ignore any stale pedals left in the resource.
        world.insert_resource(LivingMotion {
            active: false,
            drive: Some(terra_core::DriveInput {
                throttle: 1.,
                ..Default::default()
            }),
            ..Default::default()
        });
        world.init_resource::<LivingDiagnostics>();
        world.init_resource::<EcologyRuntime>();
        world.init_resource::<crate::WorldDiagnostics>();
        world.init_resource::<crate::RegionalPersistence>();
        world.run_system_once(advance_population).unwrap();
        let sandbox = world.resource::<SandboxRuntime>();
        assert!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s < 10.);
        assert!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s > 0.);
        assert!(sandbox.state.transport.vehicles[&id].odometer_m > 0.);
        assert!(
            sandbox.state.living.units[&id]
                .position
                .surface_distance_m(origin)
                > 0.1
        );
        assert!(world.resource::<LivingDiagnostics>().last_error.is_none());
        world.resource_mut::<WorldRuntime>().clock.paused = true;
        let paused = world.resource::<SandboxRuntime>().state.clone();
        world.run_system_once(advance_population).unwrap();
        assert_eq!(world.resource::<SandboxRuntime>().state, paused);
        world.resource_mut::<WorldRuntime>().clock.paused = false;
        for _ in 0..40 {
            world.run_system_once(advance_population).unwrap();
        }
        assert_eq!(
            world.resource::<SandboxRuntime>().state.transport.vehicles[&id]
                .dynamics
                .speed_m_s,
            0.
        );
    }

    #[test]
    fn dormant_owned_bodies_do_not_activate_native_ecology_but_resumed_owners_do() {
        let (resident, origin) = flat();
        let far = GeoPosition::new(50., 14., 10.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .seed_ecology_near(origin, &sandbox.catalog)
            .unwrap();
        sandbox.state.set_player_position(1, far).unwrap();
        for unit in sandbox.state.living.units.values_mut() {
            if unit.owner == Some(1) {
                unit.position = far;
            }
        }
        sandbox.state.add_player(2, origin).unwrap();
        sandbox.state.ensure_living_player(2).unwrap();
        sandbox.state.set_player_dormant(2, true).unwrap();
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
        let retained: BTreeMap<_, _> = sandbox
            .state
            .living
            .units
            .iter()
            .filter(|(_, unit)| unit.owner == Some(2))
            .map(|(id, unit)| (*id, unit.clone()))
            .collect();
        let population = sandbox.state.ecology.animals.len();
        assert!(population > 0);
        let mut world = World::new();
        let mut time = Time::<Fixed>::from_hz(4.);
        time.advance_by(std::time::Duration::from_millis(250));
        world.insert_resource(time);
        world.insert_resource(WorldRuntime {
            config: crate::WorldConfig::default(),
            focus: origin,
            frame: LocalFrame::new(origin),
            clock: terra_core::WorldClock {
                time_scale: 1.,
                ..Default::default()
            },
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            origin_revision: 0,
            rebases: 0,
            network_authority: false,
        });
        world.insert_resource(resident);
        world.insert_resource(sandbox);
        world.insert_resource(LivingSettings {
            seed_demo: false,
            simulate: true,
            collision: false,
        });
        world.init_resource::<LivingMotion>();
        world.init_resource::<LivingDiagnostics>();
        world.init_resource::<EcologyRuntime>();
        world.init_resource::<crate::WorldDiagnostics>();
        world.init_resource::<crate::RegionalPersistence>();
        for _ in 0..4 {
            world.run_system_once(advance_population).unwrap();
        }
        let stats = &world.resource::<LivingDiagnostics>().ecology_work;
        assert_eq!(stats.passes, 1, "the real ecology adapter must execute");
        assert_eq!(
            stats.eligible, 0,
            "offline occupancy cannot activate wildlife"
        );
        assert_eq!(stats.evaluated, 0);
        let state = &world.resource::<SandboxRuntime>().state;
        assert_eq!(state.ecology.animals.len(), population);
        assert_eq!(state.ecology.elapsed_seconds, 1.);
        for (id, unit) in &retained {
            assert_eq!(
                &state.living.units[id], unit,
                "offline bodies remain unchanged"
            );
        }
        assert!(world.resource::<LivingDiagnostics>().last_error.is_none());

        world
            .resource_mut::<SandboxRuntime>()
            .state
            .set_player_dormant(2, false)
            .unwrap();
        for _ in 0..4 {
            world.run_system_once(advance_population).unwrap();
        }
        let stats = &world.resource::<LivingDiagnostics>().ecology_work;
        assert_eq!(stats.passes, 2);
        assert!(
            stats.eligible > 0,
            "resumed observers must activate existing wildlife"
        );
        assert!(
            stats.near_decisions > 0,
            "near active bodies retain timely decisions"
        );
        assert!(world.resource::<LivingDiagnostics>().last_error.is_none());
        world
            .resource_mut::<SandboxRuntime>()
            .state
            .set_player_dormant(2, true)
            .unwrap();
        for _ in 0..4 {
            world.run_system_once(advance_population).unwrap();
        }
        assert_eq!(
            world.resource::<LivingDiagnostics>().ecology_work.eligible,
            0
        );
        let sandbox = world.resource::<SandboxRuntime>();
        assert_eq!(sandbox.state.ecology.elapsed_seconds, 3.);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
    }

    #[test]
    fn missing_vehicle_corner_stops_without_wear_but_solid_obstacle_causes_impact() {
        let (mut resident, original) = flat();
        let key = TileKey::at(original, 15).unwrap();
        let bounds = key.bounds();
        let edge = GeoPosition::new((bounds.north + bounds.south) * 0.5, bounds.east, 10.).unwrap();
        let position = LocalFrame::new(edge).to_geo([-3., 0., 0.]).unwrap();
        let (mut sandbox, id) = moving_driver(position);
        let before = sandbox.state.transport.vehicles[&id].condition;
        drive_controlled_vehicle(
            &mut sandbox,
            &resident,
            &WeatherSample::default(),
            &[],
            true,
            terra_core::DriveInput::default(),
            0.25,
        )
        .unwrap();
        assert_eq!(sandbox.state.living.units[&id].position, position);
        assert_eq!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s, 0.);
        assert_eq!(sandbox.state.transport.vehicles[&id].condition, before);
        assert_eq!(sandbox.state.transport.vehicles[&id].odometer_m, 0.);

        let mut center = key.center();
        center.altitude_m = 10.;
        let frame = LocalFrame::new(center);
        Arc::make_mut(resident.chunks.get_mut(&key).unwrap())
            .features
            .push(WorldFeature {
                id: "vehicle-impact-wall".into(),
                kind: FeatureKind::Building,
                height_m: 8.,
                properties: BTreeMap::new(),
                points: [[3.5, 0., -5.], [8., 0., -5.], [8., 0., 5.], [3.5, 0., 5.]]
                    .map(|p| frame.to_geo(p).unwrap())
                    .to_vec(),
            });
        let (mut sandbox, id) = moving_driver(center);
        drive_controlled_vehicle(
            &mut sandbox,
            &resident,
            &WeatherSample::default(),
            &[],
            true,
            terra_core::DriveInput::default(),
            0.25,
        )
        .unwrap();
        assert_eq!(sandbox.state.living.units[&id].position, center);
        assert_eq!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s, 0.);
        assert!(sandbox.state.transport.vehicles[&id].condition < 1.);
        assert_eq!(sandbox.state.transport.vehicles[&id].odometer_m, 0.);
    }

    #[test]
    fn vehicle_on_loaded_heightmap_holds_service_and_parking_brakes_then_rolls_on_release() {
        let (mut resident, original) = flat();
        let key = TileKey::at(original, 15).unwrap();
        let mut origin = key.center();
        origin.altitude_m = 1000.;
        let frame = LocalFrame::new(origin);
        let bounds = key.bounds();
        let west = GeoPosition::new(origin.latitude, bounds.west, 1000.).unwrap();
        let east = GeoPosition::new(origin.latitude, bounds.east, 1000.).unwrap();
        let low = (1000. + frame.to_local(west)[0] * 0.15) as f32;
        let high = (1000. + frame.to_local(east)[0] * 0.15) as f32;
        Arc::make_mut(resident.chunks.get_mut(&key).unwrap()).heights = HeightField {
            width: 2,
            height: 2,
            samples_m: vec![low, high, low, high],
        };
        let (mut sandbox, id) = moving_driver(origin);
        sandbox.state.living.roads.clear();
        sandbox
            .state
            .living
            .units
            .get_mut(&id)
            .unwrap()
            .velocity_enu_m_s = [0.; 3];
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .dynamics
            .stop();
        let weather = WeatherSample::default();
        for _ in 0..40 {
            drive_controlled_vehicle(
                &mut sandbox,
                &resident,
                &weather,
                &[],
                false,
                terra_core::DriveInput::stopped(),
                0.05,
            )
            .unwrap();
        }
        assert_eq!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s, 0.);
        assert!(
            sandbox.state.living.units[&id]
                .position
                .surface_distance_m(origin)
                < 1e-5
        );
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .parking_brake = true;
        let position = sandbox.state.living.units[&id].position;
        let fuel = sandbox.state.transport.vehicles[&id].fuel_l;
        for _ in 0..20 {
            drive_controlled_vehicle(
                &mut sandbox,
                &resident,
                &weather,
                &[],
                false,
                terra_core::DriveInput {
                    throttle: 1.,
                    ..Default::default()
                },
                0.05,
            )
            .unwrap();
        }
        assert_eq!(sandbox.state.living.units[&id].position, position);
        assert_eq!(sandbox.state.living.units[&id].velocity_enu_m_s, [0.; 3]);
        assert_eq!(sandbox.state.transport.vehicles[&id].fuel_l, fuel);
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .parking_brake = false;
        for _ in 0..20 {
            drive_controlled_vehicle(
                &mut sandbox,
                &resident,
                &weather,
                &[],
                false,
                terra_core::DriveInput::default(),
                0.05,
            )
            .unwrap();
        }
        assert!(sandbox.state.transport.vehicles[&id].dynamics.speed_m_s < -1.);
        assert!(sandbox.state.living.units[&id].position.longitude < origin.longitude);
        assert!(sandbox.state.transport.vehicles[&id].odometer_m > 0.4);
    }

    #[test]
    fn manual_vehicle_stops_before_pedestrian_without_repeated_resting_damage() {
        let (resident, original) = flat();
        let mut origin = TileKey::at(original, 15).unwrap().center();
        origin.altitude_m = 10.;
        let (mut sandbox, id) = moving_driver(origin);
        let mut pedestrian = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.owner.is_none() && u.kind == ActorKind::Human)
            .unwrap()
            .clone();
        pedestrian.position = LocalFrame::new(origin).to_geo([4., 0., 0.]).unwrap();
        pedestrian.velocity_enu_m_s = [0.; 3];
        let obstacle = pedestrian.position;
        sandbox.state.living.units.insert(pedestrian.id, pedestrian);
        drive_controlled_vehicle(
            &mut sandbox,
            &resident,
            &WeatherSample::default(),
            &[],
            true,
            terra_core::DriveInput::default(),
            0.25,
        )
        .unwrap();
        let unit = &sandbox.state.living.units[&id];
        assert!(origin.surface_distance_m(unit.position) > 0.5);
        assert!(unit.position.surface_distance_m(obstacle) > 2.8);
        assert_eq!(unit.velocity_enu_m_s, [0.; 3]);
        let condition = sandbox.state.transport.vehicles[&id].condition;
        assert!(condition < 0.99);
        for _ in 0..10 {
            drive_controlled_vehicle(
                &mut sandbox,
                &resident,
                &WeatherSample::default(),
                &[],
                true,
                terra_core::DriveInput::default(),
                0.05,
            )
            .unwrap();
        }
        assert_eq!(sandbox.state.transport.vehicles[&id].condition, condition);
    }

    #[test]
    fn unknown_terrain_and_static_footprints_reject_surface_motion() {
        let (resident, origin) = flat();
        let weather = WeatherSample::default();
        let frame = LocalFrame::new(origin);
        let next = frame.to_geo([1., 0., 0.]).unwrap();
        let grounded = resolve_step(
            &resident,
            origin,
            next,
            ActorKind::Human,
            &weather,
            true,
            &[],
        )
        .unwrap();
        assert!((grounded.altitude_m - 10.025).abs() < 1e-6);
        assert!(
            ground_candidate(
                &resident,
                next,
                ActorKind::Human,
                &weather,
                true,
                &[(origin, 5.)]
            )
            .is_none()
        );
        assert!(
            resolve_step(
                &resident,
                origin,
                next,
                ActorKind::Human,
                &weather,
                true,
                &[(origin, 5.)]
            )
            .is_some(),
            "a body restored inside a structure must be able to step outward"
        );
        let distant = frame.to_geo([10000., 0., 0.]).unwrap();
        assert!(
            ground_candidate(&resident, distant, ActorKind::Human, &weather, true, &[]).is_none()
        );
    }

    #[test]
    fn water_blocks_a_person_but_accepts_a_boat() {
        let (mut resident, origin) = flat();
        let frame = LocalFrame::new(origin);
        let key = TileKey::at(origin, 15).unwrap();
        Arc::make_mut(resident.chunks.get_mut(&key).unwrap())
            .features
            .push(WorldFeature {
                id: "test-water".into(),
                kind: FeatureKind::Water,
                height_m: 0.,
                properties: BTreeMap::new(),
                points: [
                    [-20., 0., -20.],
                    [20., 0., -20.],
                    [20., 0., 20.],
                    [-20., 0., 20.],
                ]
                .map(|v| frame.to_geo(v).unwrap())
                .to_vec(),
            });
        assert!(
            ground_candidate(
                &resident,
                origin,
                ActorKind::Human,
                &WeatherSample::default(),
                true,
                &[]
            )
            .is_none()
        );
        assert!(
            ground_candidate(
                &resident,
                origin,
                ActorKind::Ship,
                &WeatherSample::default(),
                true,
                &[]
            )
            .is_some()
        );
    }

    #[test]
    fn rts_motion_uses_resolver_before_reaching_destination() {
        let (resident, origin) = flat();
        let mut state = SandboxState::default();
        let catalog = Catalog::builtin();
        state.add_player(1, origin).unwrap();
        state.seed_living_demo(origin, &[], &catalog).unwrap();
        let id = state.living.accounts[&1].avatar_unit;
        let destination = LocalFrame::new(origin).to_geo([15., 0., 0.]).unwrap();
        state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::MoveUnits {
                        unit_ids: vec![id],
                        destination,
                    },
                },
                &catalog,
            )
            .unwrap();
        let previous = state.living.units[&id].position;
        state
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, _| None)
            .unwrap();
        assert_eq!(state.living.units[&id].position, previous);
        state
            .advance_living_with_environment(0.1, 0., &catalog, |kind, old, new| {
                resolve_step(
                    &resident,
                    old,
                    new,
                    kind,
                    &WeatherSample::default(),
                    true,
                    &[],
                )
            })
            .unwrap();
        assert!(
            state.living.units[&id]
                .position
                .surface_distance_m(previous)
                > 0.1
        );
    }
}
