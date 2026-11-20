//! Shared dynamic-body admission for local, remote and autonomous movement.
//! A broad phase is built once per population step and relocated after each
//! accepted move, so later actors see current positions without an all-pairs scan.
use super::*;
use crate::GeoSpatialIndex;
use terra_core::{BodyContact, ContactBody, TravelMode, body_overlaps_column, sweep_body_contact};

pub(super) const MAX_AVOIDANCE_CLUSTER_BODIES: usize = 4;

/// One shape mapping for movement contacts and construction admission. These
/// reference dimensions are unchanged for existing ground-body motion.
fn contact_body(state: &SandboxState, unit: &LivingUnit) -> ContactBody {
    let dimensions_m = state.transport.vehicles.get(&unit.id).map_or_else(
        || match unit.kind {
            ActorKind::Vehicle if unit.role == UnitRole::Hauler => [1.95, 1.8, 4.0],
            ActorKind::Vehicle => [2.2, 1.8, 3.9],
            ActorKind::Human => [0.55, 1.8, 0.55],
            ActorKind::Animal => [0.65, 1.1, 1.25],
            ActorKind::Ship => [2.4, 2., 2.4],
            _ => [0.85, 2., 0.85],
        },
        |vehicle| vehicle.model.dimensions_m(),
    );
    ContactBody {
        id: unit.id,
        position: unit.position,
        heading_deg: if unit.kind == ActorKind::Vehicle {
            f64::from(unit.heading_deg)
        } else {
            0.
        },
        dimensions_m,
    }
}

impl SandboxState {
    /// Dismount planning ignores only the occupied vehicle while sweeping the
    /// avatar from its seat. Other bodies, including people and dormant owners,
    /// remain obstacles. A pre-existing overlap with one of them cannot escape
    /// through this new-placement query.
    pub(crate) fn dismount_path_obstruction(
        &self,
        avatar: &LivingUnit,
        from: GeoPosition,
        to: GeoPosition,
        vehicle_id: u64,
    ) -> Result<bool, SandboxError> {
        let moving = ContactBody {
            position: from,
            ..contact_body(self, avatar)
        };
        for unit in self
            .living
            .units
            .values()
            .filter(|unit| unit.id != avatar.id && unit.id != vehicle_id)
        {
            let other = contact_body(self, unit);
            for destination in [from, to] {
                if sweep_body_contact(moving, destination, moving.heading_deg, other)
                    .map_err(|error| SandboxError::Invalid(error.to_string()))?
                    .is_some()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// New bodies require an unoccupied placement, including people that do
    /// not collide during walking. Unlike movement, this admits no overlap
    /// escape exception and includes dormant resident bodies.
    pub fn spawn_obstruction(&self, candidate: &LivingUnit) -> Result<Option<u64>, SandboxError> {
        self.spawn_shape_obstruction(contact_body(self, candidate))
    }

    pub(crate) fn spawn_shape_obstruction(
        &self,
        shape: ContactBody,
    ) -> Result<Option<u64>, SandboxError> {
        for unit in self
            .living
            .units
            .values()
            .filter(|unit| unit.id != shape.id)
        {
            if sweep_body_contact(
                shape,
                shape.position,
                shape.heading_deg,
                contact_body(self, unit),
            )
            .map_err(|error| SandboxError::Invalid(error.to_string()))?
            .is_some()
            {
                return Ok(Some(unit.id));
            }
        }
        Ok(None)
    }

    /// First resident actor intersecting a proposed cylindrical structure.
    /// Construction checks every body, including parked/dormant ones and other
    /// owners' vehicles. Hosts hydrate command dependencies before this query.
    pub fn construction_obstruction(
        &self,
        position: GeoPosition,
        radius_m: f64,
        height_m: f64,
    ) -> Result<Option<u64>, SandboxError> {
        position.validate()?;
        if !finite_range(radius_m, 0., 100.) || !finite_range(height_m, 0.1, 100.) {
            return Err(SandboxError::Invalid("construction footprint".into()));
        }
        if !self.living.enabled {
            return Ok(None);
        }
        for unit in self.living.units.values() {
            if body_overlaps_column(contact_body(self, unit), position, radius_m, height_m)
                .map_err(|error| SandboxError::Invalid(error.to_string()))?
            {
                return Ok(Some(unit.id));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
struct Entry {
    body: ContactBody,
    kind: ActorKind,
    velocity: [f64; 3],
    route_active: bool,
}

/// A bounded runtime collision snapshot. It owns no persistent or private
/// account data. Rebuild after spawn/despawn/possession, and update after every
/// accepted move within the step. Dormant resident bodies remain obstacles.
#[derive(Debug, Clone)]
pub struct LivingContacts {
    bodies: BTreeMap<u64, Entry>,
    index: GeoSpatialIndex,
    radius: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ContactMotion {
    pub position: GeoPosition,
    pub heading_deg: f32,
    pub contact: Option<BodyContact>,
    pub impact_speed_m_s: f64,
    pub candidates_tested: usize,
}

impl LivingContacts {
    pub fn new(state: &SandboxState) -> Result<Self, SandboxError> {
        let seated: BTreeSet<_> = state
            .living
            .accounts
            .values()
            .filter(|a| a.possessed && a.active_unit != a.avatar_unit)
            .filter(|a| {
                state
                    .living
                    .units
                    .get(&a.active_unit)
                    .is_some_and(|u| u.kind == ActorKind::Vehicle)
            })
            .map(|a| a.avatar_unit)
            .collect();
        let bodies: BTreeMap<_, _> = state
            .living
            .units
            .values()
            .filter(|u| u.kind.travel_mode() == TravelMode::Ground && !seated.contains(&u.id))
            .map(|u| {
                (
                    u.id,
                    Entry {
                        body: contact_body(state, u),
                        kind: u.kind,
                        velocity: u.velocity_enu_m_s,
                        route_active: matches!(u.order, UnitOrder::Move { .. } | UnitOrder::Patrol),
                    },
                )
            })
            .collect();
        for entry in bodies.values() {
            entry
                .body
                .validate()
                .map_err(|e| SandboxError::Invalid(e.to_string()))?;
        }
        let index = GeoSpatialIndex::from_positions(
            16.,
            bodies.iter().map(|(id, e)| (*id, e.body.position)),
        )?;
        let radius = bodies
            .values()
            .map(|e| e.body.horizontal_radius_m())
            .fold(0., f64::max);
        Ok(Self {
            bodies,
            index,
            radius,
        })
    }

    /// Collision-independent terrain validation must happen first. This method
    /// clips only a known, admitted path and never invents an impact against an
    /// unloaded tile. People/animals collide with vehicles, not each other.
    pub fn resolve(
        &self,
        id: u64,
        from: GeoPosition,
        from_heading: f32,
        destination: GeoPosition,
        destination_heading: f32,
        velocity_enu_m_s: [f64; 3],
    ) -> Result<ContactMotion, SandboxError> {
        let mut result = ContactMotion {
            position: destination,
            heading_deg: destination_heading,
            contact: None,
            impact_speed_m_s: 0.,
            candidates_tested: 0,
        };
        from.validate()?;
        destination.validate()?;
        if !from_heading.is_finite()
            || !destination_heading.is_finite()
            || velocity_enu_m_s.iter().any(|v| !v.is_finite())
        {
            return Err(SandboxError::Invalid("contact motion".into()));
        }
        let Some(entry) = self.bodies.get(&id) else {
            return Ok(result);
        };
        let moving = ContactBody {
            position: from,
            heading_deg: if entry.kind == ActorKind::Vehicle {
                f64::from(from_heading)
            } else {
                0.
            },
            ..entry.body
        };
        let shape_heading = if entry.kind == ActorKind::Vehicle {
            f64::from(destination_heading)
        } else {
            0.
        };
        let distance = from.surface_distance_m(destination);
        if distance > 250. || (destination.altitude_m - from.altitude_m).abs() > 250. {
            return Err(SandboxError::Invalid(
                "contact motion exceeds bounded ground step".into(),
            ));
        }
        let query = self.index.query(
            from,
            distance + moving.horizontal_radius_m() + self.radius + 1.,
        )?;
        let ids = query.ids;
        let mut impact = 0.;
        for other_id in ids {
            if other_id == id {
                continue;
            }
            let other = &self.bodies[&other_id];
            if entry.kind != ActorKind::Vehicle && other.kind != ActorKind::Vehicle {
                continue;
            }
            result.candidates_tested += 1;
            let Some(contact) = sweep_body_contact(moving, destination, shape_heading, other.body)
                .map_err(|e| SandboxError::Invalid(e.to_string()))?
            else {
                continue;
            };
            if result.contact.is_none_or(|old| {
                contact.fraction < old.fraction
                    || (contact.fraction == old.fraction && contact.other_id < old.other_id)
            }) {
                result.contact = Some(contact);
                impact = (0..3)
                    .map(|i| -(velocity_enu_m_s[i] - other.velocity[i]) * contact.normal_enu[i])
                    .sum::<f64>()
                    .max(0.)
                    .min(100.);
            }
        }
        if let Some(contact) = result.contact {
            // Leave a two-centimetre clearance; subsequent stopped frames must
            // not repeatedly charge an impact at the exact touching boundary.
            let fraction = (contact.fraction - 0.02 / distance.max(0.02)).max(0.);
            if fraction <= 0. {
                result.position = from;
                result.heading_deg = from_heading;
            } else {
                let frame = LocalFrame::new(from);
                result.position =
                    frame.to_geo(frame.to_local(destination).map(|v| v * fraction))?;
                result.position.altitude_m =
                    from.altitude_m + (destination.altitude_m - from.altitude_m) * fraction;
                let start_heading = f64::from(from_heading).rem_euclid(360.);
                let end_heading = f64::from(destination_heading).rem_euclid(360.);
                let angle = (end_heading - start_heading + 180.).rem_euclid(360.) - 180.;
                result.heading_deg = (start_heading + angle * fraction).rem_euclid(360.) as f32;
            }
            // Initial overlap is a legacy placement/entry issue, not a new
            // impact. Slow repeated pressure is likewise not collision damage.
            result.impact_speed_m_s = if contact.fraction > 1e-8 && impact >= 1. {
                impact
            } else {
                0.
            };
        }
        Ok(result)
    }

    pub fn update(
        &mut self,
        id: u64,
        position: GeoPosition,
        heading_deg: f32,
        velocity: [f64; 3],
    ) -> Result<(), SandboxError> {
        position.validate()?;
        if !heading_deg.is_finite() || velocity.iter().any(|v| !v.is_finite()) {
            return Err(SandboxError::Invalid("contact body update".into()));
        }
        if let Some(entry) = self.bodies.get_mut(&id) {
            self.index.relocate(&id, entry.body.position, position)?;
            entry.body.position = position;
            entry.body.heading_deg = if entry.kind == ActorKind::Vehicle {
                f64::from(heading_deg)
            } else {
                0.
            };
            entry.velocity = velocity;
        }
        Ok(())
    }

    /// A bounded physical clearance maneuver may temporarily diverge from the
    /// route while moving away from pedestrians, wildlife or a parked vehicle.
    /// Moving vehicles retain yielding priority. This grants no contact
    /// exemption: the resulting movement must pass the full ordinary sweep.
    pub(super) fn clears_blocking_corner(
        &self,
        id: u64,
        obstacle_id: u64,
        from: GeoPosition,
        candidate: GeoPosition,
    ) -> bool {
        let Some(moving) = self.bodies.get(&id) else {
            return false;
        };
        let Some(obstacle) = self.bodies.get(&obstacle_id) else {
            return false;
        };
        if (obstacle.kind == ActorKind::Vehicle
            && obstacle.velocity[0].hypot(obstacle.velocity[1]) > 0.1)
            || from.surface_distance_m(candidate) > 1.01
        {
            return false;
        }
        let distance = from.surface_distance_m(obstacle.body.position);
        distance < moving.body.horizontal_radius_m() + obstacle.body.horizontal_radius_m() + 0.25
            && candidate.surface_distance_m(obstacle.body.position) > distance + 0.001
    }

    pub(super) fn moving_walker(&self, id: u64) -> bool {
        self.bodies.get(&id).is_some_and(|body| {
            body.kind != ActorKind::Vehicle && body.velocity[0].hypot(body.velocity[1]) > 0.1
        })
    }

    pub(super) fn stopped_routed_vehicle(&self, id: u64) -> bool {
        self.bodies.get(&id).is_some_and(|entry| {
            entry.kind == ActorKind::Vehicle
                && entry.route_active
                && entry.velocity[0].hypot(entry.velocity[1]) <= 0.1
        })
    }

    /// A local route detour is appropriate for an obstruction ahead. A rear
    /// corner encountered while turning needs clearance, not a route back
    /// around the body that the vehicle is already leaving behind.
    pub(super) fn obstacle_ahead(
        &self,
        obstacle_id: u64,
        from: GeoPosition,
        target: GeoPosition,
    ) -> bool {
        let Some(obstacle) = self.bodies.get(&obstacle_id) else {
            return false;
        };
        let frame = LocalFrame::new(from);
        let goal = frame.to_local(target);
        let delta = frame.to_local(obstacle.body.position);
        let distance = goal[0].hypot(goal[2]);
        distance > 0.01 && (delta[0] * goal[0] + delta[2] * goal[2]) / distance > 0.5
    }

    /// Conservative local detour around one dynamic body. The caller still
    /// validates both legs against terrain and all bodies before inserting it.
    pub fn avoidance_corners(
        &self,
        id: u64,
        obstacle_id: u64,
        from: GeoPosition,
        target: GeoPosition,
        side: f64,
    ) -> Option<[GeoPosition; 2]> {
        let moving = self.bodies.get(&id)?;
        let obstacle = self.bodies.get(&obstacle_id)?;
        // A moving road user is handled by stopping/yielding, not by continually
        // rewriting a route around the location it occupied this instant.
        if obstacle.velocity[0].hypot(obstacle.velocity[1]) > 0.1 {
            return None;
        }
        let frame = LocalFrame::new(from);
        let end = frame.to_local(target);
        let length = end[0].hypot(end[2]);
        if length < 0.01 || !matches!(side, -1. | 1.) {
            return None;
        }
        let direction = [end[0] / length, end[2] / length];
        let center = frame.to_local(obstacle.body.position);
        let turning_clearance = if moving.kind == ActorKind::Vehicle {
            4.
        } else {
            2.
        };
        let clearance = moving.body.horizontal_radius_m()
            + obstacle.body.horizontal_radius_m()
            + turning_clearance;
        let normal = [-direction[1] * side, direction[0] * side];
        let make = |sign: f64| {
            let mut p = frame
                .to_geo([
                    center[0] + sign * direction[0] * clearance + normal[0] * clearance,
                    0.,
                    center[2] + sign * direction[1] * clearance + normal[1] * clearance,
                ])
                .ok()?;
            p.altitude_m = from.altitude_m;
            Some(p)
        };
        Some([make(-1.)?, make(1.)?])
    }

    /// A narrower alternative for a parallel parked road vehicle. The normal
    /// turning-clearance proposal remains preferred. This proposal uses the
    /// actual lateral body projections, and callers must admit every leg and
    /// the later physical curve; a centre that fits is not sufficient.
    pub(super) fn corridor_avoidance_corners(
        &self,
        id: u64,
        obstacle_id: u64,
        from: GeoPosition,
        target: GeoPosition,
        side: f64,
    ) -> Option<[GeoPosition; 2]> {
        let moving = self.bodies.get(&id)?;
        let obstacle = self.bodies.get(&obstacle_id)?;
        if id == obstacle_id
            || moving.kind != ActorKind::Vehicle
            || obstacle.kind != ActorKind::Vehicle
            || obstacle.route_active
            || obstacle.velocity[0].hypot(obstacle.velocity[1]) > 0.1
            || !matches!(side, -1. | 1.)
        {
            return None;
        }
        let frame = LocalFrame::new(from);
        let end = frame.to_local(target);
        let length = end[0].hypot(end[2]);
        if length < 0.01 {
            return None;
        }
        let direction = [end[0] / length, end[2] / length];
        let bearing = direction[0].atan2(-direction[1]).to_degrees();
        let relative = (obstacle.body.heading_deg - bearing).to_radians();
        // Crossing and turning bodies keep the existing conservative proposal.
        if relative.sin().abs() > 0.15 {
            return None;
        }
        let normal = [-direction[1] * side, direction[0] * side];
        let center = frame.to_local(obstacle.body.position);
        let half_width = obstacle.body.dimensions_m[0] * 0.5 * relative.cos().abs()
            + obstacle.body.dimensions_m[2] * 0.5 * relative.sin().abs();
        let lateral = half_width + moving.body.dimensions_m[0] * 0.5 + 0.8;
        let longitudinal =
            moving.body.horizontal_radius_m() + obstacle.body.horizontal_radius_m() + 3.5;
        let make = |sign: f64| {
            let mut point = frame
                .to_geo([
                    center[0] + sign * direction[0] * longitudinal + normal[0] * lateral,
                    0.,
                    center[2] + sign * direction[1] * longitudinal + normal[1] * lateral,
                ])
                .ok()?;
            point.altitude_m = from.altitude_m;
            Some(point)
        };
        Some([make(-1.)?, make(1.)?])
    }

    /// Expand a failed one-body proposal around a small set of stopped bodies.
    /// Every member was encountered by an ordinary corridor sweep; this does
    /// not scan or route around the whole population. The route-aligned bounds
    /// include each complete body and the existing vehicle turning clearance.
    /// Callers must admit all three legs, including the original target, before
    /// committing the two corners. A moving member invalidates the proposal.
    pub(super) fn avoidance_cluster_corners(
        &self,
        id: u64,
        obstacle_ids: &[u64],
        from: GeoPosition,
        target: GeoPosition,
        side: f64,
    ) -> Option<[GeoPosition; 2]> {
        if !(2..=MAX_AVOIDANCE_CLUSTER_BODIES).contains(&obstacle_ids.len())
            || !matches!(side, -1. | 1.)
        {
            return None;
        }
        let moving = self.bodies.get(&id)?;
        if moving.kind != ActorKind::Vehicle {
            return None;
        }
        let frame = LocalFrame::new(from);
        let end = frame.to_local(target);
        let length = end[0].hypot(end[2]);
        if length < 0.01 {
            return None;
        }
        let direction = [end[0] / length, end[2] / length];
        let normal = [-direction[1] * side, direction[0] * side];
        let mut first = f64::INFINITY;
        let mut last = f64::NEG_INFINITY;
        let mut outside = f64::NEG_INFINITY;
        for (index, obstacle_id) in obstacle_ids.iter().enumerate() {
            if *obstacle_id == id || obstacle_ids[..index].contains(obstacle_id) {
                return None;
            }
            let obstacle = self.bodies.get(obstacle_id)?;
            if obstacle.velocity[0].hypot(obstacle.velocity[1]) > 0.1 {
                return None;
            }
            let center = frame.to_local(obstacle.body.position);
            let clearance =
                moving.body.horizontal_radius_m() + obstacle.body.horizontal_radius_m() + 4.;
            let along = center[0] * direction[0] + center[2] * direction[1];
            let across = center[0] * normal[0] + center[2] * normal[1];
            first = first.min(along - clearance);
            last = last.max(along + clearance);
            outside = outside.max(across + clearance);
        }
        let make = |along: f64| {
            let mut point = frame
                .to_geo([
                    direction[0] * along + normal[0] * outside,
                    0.,
                    direction[1] * along + normal[1] * outside,
                ])
                .ok()?;
            point.altitude_m = from.altitude_m;
            Some(point)
        };
        Some([make(first)?, make(last)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> (SandboxState, GeoPosition, Catalog, u64) {
        let origin = GeoPosition::new(48., 11., 10.).unwrap();
        let catalog = Catalog::default();
        let mut state = SandboxState::new();
        state.add_player(1, origin).unwrap();
        state.seed_living_demo(origin, &[], &catalog).unwrap();
        state.ensure_transport_demo(&catalog).unwrap();
        let vehicle = *state.transport.vehicles.keys().next().unwrap();
        (state, origin, catalog, vehicle)
    }
    #[test]
    fn narrow_corridors_require_an_identified_parallel_parked_vehicle() {
        let (mut state, origin, _, vehicle) = world();
        state.living.units.retain(|id, _| *id == vehicle);
        let frame = LocalFrame::new(origin);
        let target = frame.to_geo([0., 0., -60.]).unwrap();
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        unit.velocity_enu_m_s = [0.; 3];
        let mut parked = unit.clone();
        parked.id += 100;
        parked.position = frame.to_geo([0., 0., -16.]).unwrap();
        parked.order = UnitOrder::Idle;
        parked.path.clear();
        let obstacle = parked.id;
        state.living.units.insert(obstacle, parked);
        for side in [-1., 1.] {
            assert!(
                LivingContacts::new(&state)
                    .unwrap()
                    .corridor_avoidance_corners(vehicle, obstacle, origin, target, side)
                    .is_some()
            );
        }
        let contacts = LivingContacts::new(&state).unwrap();
        for (id, side) in [
            (vehicle, 1.),
            (u64::MAX, 1.),
            (obstacle, 0.),
            (obstacle, f64::NAN),
        ] {
            assert!(
                contacts
                    .corridor_avoidance_corners(vehicle, id, origin, target, side)
                    .is_none()
            );
        }
        for variant in 0..4 {
            let mut rejected = state.clone();
            let parked = rejected.living.units.get_mut(&obstacle).unwrap();
            match variant {
                0 => parked.velocity_enu_m_s = [0.2, 0., 0.],
                1 => {
                    parked.order = UnitOrder::Move {
                        destination: target,
                    }
                }
                2 => parked.heading_deg = 90.,
                _ => parked.kind = ActorKind::Human,
            }
            assert!(
                LivingContacts::new(&rejected)
                    .unwrap()
                    .corridor_avoidance_corners(vehicle, obstacle, origin, target, 1.)
                    .is_none()
            );
        }
    }
    #[test]
    fn cluster_proposals_reject_moving_unknown_duplicate_and_excess_members() {
        let (mut state, origin, _, vehicle) = world();
        state.living.units.retain(|id, _| *id == vehicle);
        let frame = LocalFrame::new(origin);
        let target = frame.to_geo([0., 0., -60.]).unwrap();
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        unit.velocity_enu_m_s = [0.; 3];
        let template = unit.clone();
        let mut ids = Vec::new();
        for index in 0..5 {
            let mut parked = template.clone();
            parked.id = vehicle + 100 + index;
            parked.position = frame.to_geo([index as f64 * 6., 0., -16.]).unwrap();
            parked.order = UnitOrder::Idle;
            ids.push(parked.id);
            state.living.units.insert(parked.id, parked);
        }
        let mut contacts = LivingContacts::new(&state).unwrap();
        for count in 2..=4 {
            assert!(
                contacts
                    .avoidance_cluster_corners(vehicle, &ids[..count], origin, target, 1.)
                    .is_some()
            );
        }
        for rejected in [
            ids.clone(),
            vec![ids[0]],
            vec![ids[0], ids[0]],
            vec![ids[0], vehicle],
            vec![ids[0], u64::MAX],
        ] {
            assert!(
                contacts
                    .avoidance_cluster_corners(vehicle, &rejected, origin, target, 1.)
                    .is_none()
            );
        }
        contacts
            .update(
                ids[1],
                state.living.units[&ids[1]].position,
                90.,
                [0.2, 0., 0.],
            )
            .unwrap();
        assert!(
            contacts
                .avoidance_cluster_corners(vehicle, &ids[..2], origin, target, 1.)
                .is_none()
        );
    }
    #[test]
    fn occupied_avatar_is_not_a_vehicle_obstacle_and_parked_bodies_are() {
        let (mut state, origin, catalog, vehicle) = world();
        let avatar = state.living.accounts[&1].avatar_unit;
        state
            .living
            .units
            .retain(|id, _| *id == vehicle || *id == avatar);
        for unit in state.living.units.values_mut() {
            unit.position = origin;
            unit.heading_deg = 0.;
        }
        state
            .apply(
                1,
                crate::Command::Living {
                    action: LivingCommand::Possess { unit_id: vehicle },
                },
                &catalog,
            )
            .unwrap();
        let contacts = LivingContacts::new(&state).unwrap();
        let end = LocalFrame::new(origin).to_geo([0., 0., -20.]).unwrap();
        assert!(
            contacts
                .resolve(vehicle, origin, 0., end, 0., [0., 20., 0.])
                .unwrap()
                .contact
                .is_none()
        );
        let mut body = state.living.units[&vehicle].clone();
        body.id += 100;
        body.owner = None;
        body.position = LocalFrame::new(origin).to_geo([0., 0., -10.]).unwrap();
        state.living.units.insert(body.id, body);
        let contacts = LivingContacts::new(&state).unwrap();
        let hit = contacts
            .resolve(vehicle, origin, 0., end, 0., [0., 20., 0.])
            .unwrap();
        assert!(hit.contact.is_some());
        assert!(hit.impact_speed_m_s > 19.9);
        assert!(origin.surface_distance_m(hit.position) < 7.);
        let extreme = contacts
            .resolve(vehicle, origin, f32::MAX, end, -f32::MAX, [0., 20., 0.])
            .unwrap();
        assert!(extreme.contact.is_some());
        assert!(extreme.heading_deg.is_finite());
        extreme.position.validate().unwrap();
    }
    #[test]
    fn updated_bodies_outside_original_index_are_still_obstacles() {
        let (mut state, origin, _, vehicle) = world();
        state.living.units.retain(|id, _| *id == vehicle);
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        let mut body = unit.clone();
        body.id += 100;
        body.owner = None;
        let other_id = body.id;
        body.position = LocalFrame::new(origin).to_geo([200., 0., 0.]).unwrap();
        state.living.units.insert(body.id, body);
        let mut contacts = LivingContacts::new(&state).unwrap();
        contacts
            .update(
                other_id,
                LocalFrame::new(origin).to_geo([0., 0., -10.]).unwrap(),
                0.,
                [0.; 3],
            )
            .unwrap();
        let end = LocalFrame::new(origin).to_geo([0., 0., -20.]).unwrap();
        assert_eq!(
            contacts
                .resolve(vehicle, origin, 0., end, 0., [0., 20., 0.])
                .unwrap()
                .contact
                .unwrap()
                .other_id,
            other_id
        );
    }

    #[test]
    fn walking_avatar_can_exit_a_vehicle_overlap_in_any_direction() {
        let (mut state, origin, _, vehicle) = world();
        let avatar = state.living.accounts[&1].avatar_unit;
        state
            .living
            .units
            .retain(|id, _| *id == vehicle || *id == avatar);
        let account = state.living.accounts.get_mut(&1).unwrap();
        account.active_unit = avatar;
        account.possessed = true;
        for unit in state.living.units.values_mut() {
            unit.position = origin;
            unit.heading_deg = 0.;
        }
        let contacts = LivingContacts::new(&state).unwrap();
        for (east, north, heading) in [(0.2, 0., 90.), (-0.2, 0., 270.), (0., -0.2, 180.)] {
            let end = LocalFrame::new(origin).to_geo([east, 0., -north]).unwrap();
            let admitted = contacts
                .resolve(avatar, origin, 0., end, heading, [east, north, 0.])
                .unwrap();
            assert!(
                admitted.contact.is_none(),
                "avatar remained trapped on exit heading {heading}"
            );
        }
    }

    #[test]
    fn autonomous_vehicle_cannot_tunnel_through_a_parked_vehicle() {
        let (mut state, origin, catalog, vehicle) = world();
        let avatar = state.living.accounts[&1].avatar_unit;
        state
            .living
            .units
            .retain(|id, _| *id == vehicle || *id == avatar);
        state.living.accounts.get_mut(&1).unwrap().possessed = false;
        state
            .transport
            .vehicles
            .get_mut(&vehicle)
            .unwrap()
            .parking_brake = false;
        let far = LocalFrame::new(origin).to_geo([100., 0., 100.]).unwrap();
        state.living.units.get_mut(&avatar).unwrap().position = far;
        let destination = LocalFrame::new(origin).to_geo([0., 0., -40.]).unwrap();
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        unit.path = vec![destination];
        unit.path_index = 0;
        unit.order = UnitOrder::Move { destination };
        let mut parked = unit.clone();
        parked.id += 100;
        parked.owner = None;
        parked.order = UnitOrder::Idle;
        parked.path.clear();
        parked.position = LocalFrame::new(origin).to_geo([0., 0., -20.]).unwrap();
        let obstacle = parked.position;
        state.living.units.insert(parked.id, parked);
        for _ in 0..50 {
            let before = state.living.units[&vehicle].clone();
            let contacts = LivingContacts::new(&state).unwrap();
            state.advance_living(0.1, 0., &catalog).unwrap();
            let after = &state.living.units[&vehicle];
            assert!(before.position.surface_distance_m(after.position) <= 1.001);
            assert!(
                contacts
                    .resolve(
                        vehicle,
                        before.position,
                        before.heading_deg,
                        after.position,
                        after.heading_deg,
                        after.velocity_enu_m_s
                    )
                    .unwrap()
                    .contact
                    .is_none()
            );
        }
        let unit = &state.living.units[&vehicle];
        assert!(origin.surface_distance_m(unit.position) > 1.);
        assert!(unit.position.surface_distance_m(obstacle) > 4.);
    }

    #[test]
    fn sequentially_moving_sparse_bodies_keep_contact_work_local() {
        let (mut state, origin, _, vehicle) = world();
        let mut template = state.living.units[&vehicle].clone();
        template.owner = None;
        state.living.units.clear();
        let frame = LocalFrame::new(origin);
        for i in 0..512 {
            let mut unit = template.clone();
            unit.id = 2_000_000 + i;
            unit.position = frame
                .to_geo([(i % 32) as f64 * 100., 0., (i / 32) as f64 * 100.])
                .unwrap();
            unit.heading_deg = 90.;
            state.living.units.insert(unit.id, unit);
        }
        let mut contacts = LivingContacts::new(&state).unwrap();
        let mut tests = 0;
        for unit in state.living.units.values() {
            let to = LocalFrame::new(unit.position)
                .to_geo([0.4, 0., 0.])
                .unwrap();
            let motion = contacts
                .resolve(unit.id, unit.position, 90., to, 90., [8., 0., 0.])
                .unwrap();
            tests += motion.candidates_tested;
            assert!(motion.contact.is_none());
            contacts
                .update(unit.id, motion.position, 90., [8., 0., 0.])
                .unwrap();
        }
        assert!(
            tests < 512 * 3,
            "sparse moved-body queries degraded to {tests} pair tests"
        );
    }

    #[test]
    fn vehicle_clears_its_rear_before_turning_without_bypassing_terrain_or_speed() {
        for allow_forward_clearance in [false, true] {
            let (mut state, origin, catalog, vehicle) = world();
            let frame = LocalFrame::new(origin);
            let avatar = state.living.accounts[&1].avatar_unit;
            state
                .living
                .units
                .retain(|id, _| *id == vehicle || *id == avatar);
            state.living.accounts.get_mut(&1).unwrap().possessed = false;
            state.living.units.get_mut(&avatar).unwrap().position =
                frame.to_geo([100., 0., 100.]).unwrap();
            let destination = frame.to_geo([-0.8289, 0., -12.476]).unwrap();
            let unit = state.living.units.get_mut(&vehicle).unwrap();
            unit.position = origin;
            unit.heading_deg = 14.5447;
            unit.path = vec![destination];
            unit.path_index = 0;
            unit.order = UnitOrder::Move { destination };
            let mut parked = unit.clone();
            parked.id += 100;
            parked.position = frame.to_geo([1.6846, 0., 4.3269]).unwrap();
            parked.heading_deg = 0.;
            parked.order = UnitOrder::Idle;
            parked.path.clear();
            let mut parked_vehicle = state.transport.vehicles[&vehicle].clone();
            parked_vehicle.unit_id = parked.id;
            state.transport.vehicles.insert(parked.id, parked_vehicle);
            state.living.units.insert(parked.id, parked);
            state
                .transport
                .vehicles
                .get_mut(&vehicle)
                .unwrap()
                .parking_brake = false;

            for _ in 0..80 {
                let before = state.living.units[&vehicle].clone();
                let contacts = LivingContacts::new(&state).unwrap();
                state
                    .advance_living_with_environment(0.05, 0., &catalog, |_, _, candidate| {
                        (allow_forward_clearance || frame.to_local(candidate)[0] <= 0.0001)
                            .then_some(candidate)
                    })
                    .unwrap();
                let after = &state.living.units[&vehicle];
                assert!(before.position.surface_distance_m(after.position) <= 0.501);
                assert!(
                    contacts
                        .resolve(
                            vehicle,
                            before.position,
                            before.heading_deg,
                            after.position,
                            after.heading_deg,
                            [0.; 3]
                        )
                        .unwrap()
                        .contact
                        .is_none()
                );
            }
            let final_position = state.living.units[&vehicle].position;
            if allow_forward_clearance {
                assert!(final_position.surface_distance_m(destination) < 1.5);
            } else {
                assert!(final_position.surface_distance_m(origin) < 0.001);
            }
        }
    }

    #[test]
    fn turnaround_clears_a_front_obstruction_behind_the_route_destination() {
        let (mut state, origin, catalog, vehicle) = world();
        let frame = LocalFrame::new(origin);
        let avatar = state.living.accounts[&1].avatar_unit;
        state
            .living
            .units
            .retain(|id, _| *id == vehicle || *id == avatar);
        state.living.accounts.get_mut(&1).unwrap().possessed = false;
        state.living.units.get_mut(&avatar).unwrap().position =
            frame.to_geo([-1.895, 0., 2.273]).unwrap();
        let destination = frame.to_geo([77.86, 0., 2.282]).unwrap();
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 245.45665;
        unit.path = vec![destination];
        unit.path_index = 0;
        unit.order = UnitOrder::Move { destination };
        state
            .transport
            .vehicles
            .get_mut(&vehicle)
            .unwrap()
            .parking_brake = false;

        for _ in 0..1200 {
            let before = state.living.units[&vehicle].clone();
            let contacts = LivingContacts::new(&state).unwrap();
            state.advance_living(0.1, 0., &catalog).unwrap();
            let after = &state.living.units[&vehicle];
            assert!(before.position.surface_distance_m(after.position) <= 1.001);
            assert!(
                contacts
                    .resolve(
                        vehicle,
                        before.position,
                        before.heading_deg,
                        after.position,
                        after.heading_deg,
                        after.velocity_enu_m_s
                    )
                    .unwrap()
                    .contact
                    .is_none()
            );
            if matches!(after.order, UnitOrder::Idle) {
                break;
            }
        }
        let arrived = &state.living.units[&vehicle];
        assert!(matches!(arrived.order, UnitOrder::Idle), "{arrived:?}");
        assert!(arrived.position.surface_distance_m(destination) < 1.5);
    }

    #[test]
    fn moving_pedestrians_allow_swept_clearance_away_but_never_toward_them() {
        let (mut state, origin, _, vehicle) = world();
        state.living.units.retain(|id, _| *id == vehicle);
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        let mut person = unit.clone();
        person.id += 100;
        person.owner = None;
        person.kind = ActorKind::Human;
        person.role = UnitRole::Citizen;
        person.position = LocalFrame::new(origin).to_geo([0., 0., -2.5]).unwrap();
        person.velocity_enu_m_s = [1.4, 0., 0.];
        let other = person.id;
        state.living.units.insert(other, person);
        let contacts = LivingContacts::new(&state).unwrap();
        let away = LocalFrame::new(origin).to_geo([0., 0., 0.01]).unwrap();
        let toward = LocalFrame::new(origin).to_geo([0., 0., -0.01]).unwrap();
        assert!(contacts.clears_blocking_corner(vehicle, other, origin, away));
        assert!(!contacts.clears_blocking_corner(vehicle, other, origin, toward));
        assert!(
            contacts
                .resolve(vehicle, origin, 0., away, 0., [0., -0.2, 0.])
                .unwrap()
                .contact
                .is_none()
        );
    }

    #[test]
    fn perpendicular_clearance_is_bounded_to_a_stationary_outward_corner() {
        let (mut state, origin, _, vehicle) = world();
        state.living.units.retain(|id, _| *id == vehicle);
        let unit = state.living.units.get_mut(&vehicle).unwrap();
        unit.position = origin;
        unit.heading_deg = 0.;
        let mut parked = unit.clone();
        parked.id += 100;
        parked.owner = None;
        parked.position = LocalFrame::new(origin).to_geo([-3., 0., 2.5]).unwrap();
        parked.heading_deg = 90.;
        parked.velocity_enu_m_s = [0.; 3];
        let other = parked.id;
        state.living.units.insert(other, parked);
        let mut contacts = LivingContacts::new(&state).unwrap();
        let p = |east, south| LocalFrame::new(origin).to_geo([east, 0., south]).unwrap();
        assert!(contacts.clears_blocking_corner(vehicle, other, origin, p(0., -0.5)));
        assert!(!contacts.clears_blocking_corner(vehicle, other, origin, p(0., 0.5)));
        assert!(!contacts.clears_blocking_corner(vehicle, other, origin, p(0., -2.)));
        assert!(!contacts.clears_blocking_corner(vehicle, other, p(0., -10.), p(0., -10.5)));
        contacts
            .update(other, p(-3., 2.5), 90., [1., 0., 0.])
            .unwrap();
        assert!(!contacts.clears_blocking_corner(vehicle, other, origin, p(0., -0.5)));
    }

    #[test]
    fn legacy_walking_path_goes_around_a_vehicle_with_bounded_admitted_steps() {
        for allow_sidestep in [false, true] {
            let (mut state, origin, catalog, vehicle) = world();
            let avatar = state.living.accounts[&1].avatar_unit;
            let mut pedestrian = state.living.units[&avatar].clone();
            pedestrian.id += 200;
            pedestrian.owner = None;
            pedestrian.role = UnitRole::Citizen;
            let frame = LocalFrame::new(origin);
            let start = frame.to_geo([0., 0., -6.]).unwrap();
            let destination = frame.to_geo([0., 0., 6.]).unwrap();
            pedestrian.position = start;
            pedestrian.heading_deg = 180.;
            pedestrian.order = UnitOrder::Move { destination };
            pedestrian.path = vec![destination];
            pedestrian.path_index = 0;
            let id = pedestrian.id;
            state
                .living
                .units
                .retain(|id, _| *id == vehicle || *id == avatar);
            state.living.units.get_mut(&avatar).unwrap().position =
                frame.to_geo([50., 0., 50.]).unwrap();
            state.living.units.get_mut(&vehicle).unwrap().position = origin;
            state.living.units.get_mut(&vehicle).unwrap().heading_deg = 0.;
            state.living.units.insert(id, pedestrian);
            for _ in 0..180 {
                let before = state.living.units[&id].position;
                state
                    .advance_living_with_environment(0.1, 0., &catalog, |_, _, candidate| {
                        (allow_sidestep || frame.to_local(candidate)[0].abs() < 0.05)
                            .then_some(candidate)
                    })
                    .unwrap();
                assert!(before.surface_distance_m(state.living.units[&id].position) < 0.15);
            }
            let end = state.living.units[&id].position;
            if allow_sidestep {
                assert!(end.surface_distance_m(destination) < 1.6);
            } else {
                assert!(frame.to_local(end)[2] < -2.8);
            }
        }
    }
}
