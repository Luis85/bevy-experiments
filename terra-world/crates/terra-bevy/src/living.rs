//! Optional settlement adapter. The same pure rules drive a local Bevy world
//! and the authoritative server; clients render a replica without advancing it.
use crate::{
    ResidentChunks, SandboxRuntime, WorldEnvironment, WorldRuntime, WorldSet,
    environment::{physical_surface, source_building_at},
};
use bevy::prelude::*;
use std::{cell::Cell, time::Instant};
use terra_core::{ActorKind, ActorState, GeoPosition, LocalFrame, TravelMode, WeatherSample};

pub struct TerraLivingPlugin;

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
}

#[derive(Resource, Debug, Default)]
pub struct LivingDiagnostics {
    pub seeded: bool,
    pub population: usize,
    pub moving: usize,
    pub blocked_moves: u64,
    pub simulation_ms: f64,
    pub last_error: Option<String>,
}

impl Plugin for TerraLivingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LivingSettings>()
            .init_resource::<LivingMotion>()
            .init_resource::<LivingDiagnostics>()
            .add_systems(
                Update,
                seed_population
                    .after(WorldSet::Stream)
                    .before(WorldSet::Sandbox),
            )
            .add_systems(FixedUpdate, advance_population.after(WorldSet::Simulate));
    }
}

fn seed_population(
    settings: Res<LivingSettings>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut diagnostics: ResMut<LivingDiagnostics>,
    mut attempted: Local<bool>,
) {
    diagnostics.population = sandbox.state.living.units.len();
    diagnostics.seeded = sandbox.state.living.enabled;
    if runtime.network_authority
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
    // Prefer a completed nearby neighborhood so source-derived roads and service
    // POIs are available before population is authored. A one-tile host still works.
    let wanted = ((runtime.config.chunk_radius * 2 + 1).pow(2) as usize).min(9);
    if resident.chunks.len() < wanted {
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
        if let Some(unit) = sandbox.state.living.units.get(&account.active_unit)
            && let Some(player) = sandbox.state.players.get_mut(&account.player_id)
        {
            player.position = unit.position;
        }
    }
}

fn ground_candidate(
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
                && (source_building_at(resident, candidate, radius)
                    || structures
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
) {
    if runtime.network_authority || !sandbox.state.living.enabled || !settings.simulate {
        return;
    }
    let start = Instant::now();
    let dt = time.delta_secs_f64().clamp(0., 0.25);
    if dt == 0. || runtime.clock.paused {
        return;
    }
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
    {
        let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
        if let Err(error) = state.advance_living_with_environment(dt, sim_dt, catalog, resolver) {
            diagnostics.last_error = Some(error.to_string());
        }
    }
    let player = sandbox.player_id;
    if motion.active
        && sandbox
            .state
            .living
            .accounts
            .get(&player)
            .is_some_and(|a| a.possessed)
        && let Some(unit) = sandbox.state.living.active_unit(player).cloned()
    {
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
            let (position, velocity) = match resolver(unit.kind, unit.position, actor.position) {
                Some(position) => (position, velocity),
                None => (unit.position, [0.; 3]),
            };
            if let Err(error) =
                sandbox
                    .state
                    .set_controlled_motion(player, position, velocity, motion.heading_deg)
            {
                diagnostics.last_error = Some(error.to_string());
            }
        }
    }
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
    sandbox.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;
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
