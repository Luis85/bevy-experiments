//! Fair, bounded intersection reservations. Occupancy is always derived from
//! current authority bodies; a permit never overrides terrain or body sweeps.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficRequest {
    pub unit_id: u64,
    pub ticket: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficJunctionState {
    pub junction_id: u64,
    pub queue: Vec<TrafficRequest>,
    pub holder: Option<u64>,
    pub granted_at: f64,
    pub progress_at: f64,
    pub last_position: Option<GeoPosition>,
}
#[derive(Debug, Clone)]
struct NetworkCache {
    roads: Vec<RoadRoute>,
    network: Arc<RoadNetwork>,
}
/// Saved scheduling state only. The compiled geometry cache is private,
/// disposable, and excluded from serialization and semantic equality.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrafficControl {
    pub elapsed_seconds: f64,
    pub next_ticket: u64,
    pub junctions: Vec<TrafficJunctionState>,
    /// NPC handling is saved independently of registered transport vehicles.
    pub vehicle_dynamics: BTreeMap<u64, terra_core::VehicleKinematics>,
    #[serde(skip)]
    cache: Option<NetworkCache>,
}
impl PartialEq for TrafficControl {
    fn eq(&self, other: &Self) -> bool {
        self.elapsed_seconds == other.elapsed_seconds
            && self.next_ticket == other.next_ticket
            && self.junctions == other.junctions
            && self.vehicle_dynamics == other.vehicle_dynamics
    }
}
impl TrafficControl {
    fn network(&mut self, roads: &[RoadRoute]) -> Arc<RoadNetwork> {
        if self.cache.as_ref().is_none_or(|c| c.roads != roads) {
            self.cache = Some(NetworkCache {
                roads: roads.to_vec(),
                network: Arc::new(RoadNetwork::new(roads)),
            });
        }
        self.cache.as_ref().unwrap().network.clone()
    }
    pub fn queued_vehicles(&self) -> usize {
        self.junctions.iter().map(|j| j.queue.len()).sum()
    }
    /// Scheduling hint only. Authority recomputes all physical occupancy each step.
    pub fn is_waiting(&self, unit_id: u64) -> bool {
        self.junctions
            .iter()
            .any(|j| j.holder != Some(unit_id) && j.queue.iter().any(|r| r.unit_id == unit_id))
    }
    pub fn active_permits(&self) -> usize {
        self.junctions.iter().filter(|j| j.holder.is_some()).count()
    }
    pub(super) fn validate(&self, world: &SandboxState) -> Result<(), SandboxError> {
        if !finite_range(self.elapsed_seconds, 0., 1e12)
            || self.junctions.len() > MAX_TRAFFIC_JUNCTIONS
            || self.queued_vehicles() > MAX_LIVING_UNITS
        {
            return Err(SandboxError::Invalid("traffic coordination bounds".into()));
        }
        if self.vehicle_dynamics.len() > MAX_LIVING_UNITS
            || self.vehicle_dynamics.iter().any(|(id, state)| {
                state.validate().is_err()
                    || *id < 1_000_000
                    || *id >= world.living.next_id
                    || world.transport.vehicles.contains_key(id)
                    || world
                        .living_unit_reference(*id)
                        .map_or(!world.is_partial(), |(_, kind, _)| {
                            kind != ActorKind::Vehicle
                        })
            })
        {
            return Err(SandboxError::Invalid("autonomous vehicle handling".into()));
        }
        let mut intersections = BTreeSet::new();
        let mut tickets = BTreeSet::new();
        for junction in &self.junctions {
            let mut units = BTreeSet::new();
            if junction.junction_id == 0
                || !intersections.insert(junction.junction_id)
                || junction.queue.is_empty()
                || !finite_range(junction.granted_at, 0., self.elapsed_seconds)
                || !finite_range(
                    junction.progress_at,
                    junction.granted_at,
                    self.elapsed_seconds,
                )
                || junction
                    .last_position
                    .is_some_and(|p| validate_living_position(p).is_err())
                || junction.holder.is_some() != junction.last_position.is_some()
                || junction
                    .holder
                    .is_some_and(|id| !junction.queue.iter().any(|r| r.unit_id == id))
            {
                return Err(SandboxError::Invalid("traffic junction reservation".into()));
            }
            for request in &junction.queue {
                if !units.insert(request.unit_id)
                    || request.ticket == 0
                    || request.ticket > self.next_ticket
                    || !tickets.insert(request.ticket)
                    || request.unit_id < 1_000_000
                    || request.unit_id >= world.living.next_id
                    || world
                        .living_unit_reference(request.unit_id)
                        .is_some_and(|(_, kind, _)| kind != ActorKind::Vehicle)
                {
                    return Err(SandboxError::Invalid("traffic queue reference".into()));
                }
            }
            if junction
                .queue
                .windows(2)
                .any(|q| q[0].ticket >= q[1].ticket)
            {
                return Err(SandboxError::Invalid("traffic queue order".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TrafficBody {
    id: u64,
    position: GeoPosition,
    heading: f64,
    velocity: [f64; 3],
    half_length: f64,
    half_width: f64,
    manual: bool,
    path: Vec<GeoPosition>,
}
impl TrafficBody {
    fn radius(&self, junction: &RoadJunction) -> f64 {
        junction.radius_m + self.half_length + 0.75
    }
    fn inside(&self, junction: &RoadJunction) -> bool {
        (self.position.altitude_m - junction.position.altitude_m).abs() < 3.
            && self.position.surface_distance_m(junction.position) < self.radius(junction) - 0.05
    }
    fn entry(&self, junction: &RoadJunction) -> Option<f64> {
        let mut from = self.position;
        let mut traveled = 0.;
        for &to in &self.path {
            if let Some(distance) = circle_entry(from, to, junction.position, self.radius(junction))
            {
                return Some(traveled + distance);
            }
            traveled += from.surface_distance_m(to);
            if traveled > 240. {
                break;
            }
            from = to;
        }
        None
    }
    fn exit(&self, junction: &RoadJunction) -> Option<(GeoPosition, f64)> {
        let mut from = self.position;
        let mut entered = self.inside(junction);
        for &to in &self.path {
            if circle_entry(from, to, junction.position, self.radius(junction)).is_some() {
                entered = true;
            }
            if entered {
                if let Some(exit) =
                    circle_exit(from, to, junction.position, self.radius(junction) + 2.)
                {
                    let frame = LocalFrame::new(exit);
                    let a = frame.to_local(from);
                    let b = frame.to_local(to);
                    let heading = (b[0] - a[0])
                        .atan2(a[2] - b[2])
                        .to_degrees()
                        .rem_euclid(360.);
                    return Some((exit, heading));
                }
            }
            from = to;
        }
        None
    }
}

pub(super) struct TrafficFrame {
    pub network: Arc<RoadNetwork>,
    /// Maximum physical travel before a held intersection stop line.
    limits: BTreeMap<u64, f64>,
    blocked_zones: BTreeMap<u64, Vec<(GeoPosition, f64)>>,
}
impl TrafficFrame {
    pub fn limit(&self, id: u64) -> f64 {
        self.limits.get(&id).copied().unwrap_or(f64::INFINITY)
    }
    /// Recheck the actual candidate too: a stationary-obstacle detour must not
    /// cut across an unreserved conflict box after route-distance budgeting.
    pub fn clip(&self, id: u64, from: GeoPosition, to: GeoPosition) -> (GeoPosition, f64) {
        let distance = from.surface_distance_m(to);
        if distance < 1e-8 {
            return (to, 1.);
        }
        let mut allowed = distance;
        if let Some(zones) = self.blocked_zones.get(&id) {
            for &(center, radius) in zones {
                if let Some(entry) = circle_entry(from, to, center, radius) {
                    allowed = allowed.min((entry - 0.25).max(0.));
                }
            }
        }
        let fraction = (allowed / distance).clamp(0., 1.);
        if fraction >= 1. {
            return (to, 1.);
        }
        let frame = LocalFrame::new(from);
        let delta = frame.to_local(to);
        let mut point = frame.to_geo(delta.map(|v| v * fraction)).unwrap_or(from);
        point.altitude_m = from.altitude_m + (to.altitude_m - from.altitude_m) * fraction;
        (point, fraction)
    }
}

impl SandboxState {
    pub(super) fn prepare_traffic(
        &mut self,
        dt: f64,
        possessed: &BTreeSet<u64>,
        enabled: bool,
    ) -> Result<TrafficFrame, SandboxError> {
        let network = self.living.traffic.network(&self.living.roads);
        let mut frame = TrafficFrame {
            network: network.clone(),
            limits: BTreeMap::new(),
            blocked_zones: BTreeMap::new(),
        };
        if dt == 0. || !enabled {
            return Ok(frame);
        }
        let bodies: Vec<_> = self
            .living
            .units
            .values()
            .filter(|u| u.kind == ActorKind::Vehicle)
            .map(|u| {
                let dimensions = self
                    .transport
                    .vehicles
                    .get(&u.id)
                    .map_or([2.2, 1.8, 4.], |v| v.model.dimensions_m());
                let manual = possessed.contains(&u.id);
                let dormant = u.owner.is_some_and(|id| self.is_player_dormant(id));
                let path = if manual || dormant {
                    vec![]
                } else {
                    match u.order {
                        UnitOrder::Move { destination } => {
                            let mut p: Vec<_> = u
                                .path
                                .iter()
                                .skip(u.path_index)
                                .copied()
                                .take(129)
                                .collect();
                            if p.is_empty() {
                                p.push(destination);
                            }
                            p
                        }
                        UnitOrder::Patrol => {
                            // The controller consumes temporary clearance
                            // points first without changing the patrol cursor.
                            // Reservations must inspect that same first leg.
                            let mut path = u.path.clone();
                            if u.patrol_reverse {
                                path.extend(u.patrol.iter().take(u.path_index + 1).rev().copied());
                            } else {
                                path.extend(u.patrol.iter().skip(u.path_index).copied());
                            }
                            path.truncate(129);
                            path
                        }
                        _ => vec![],
                    }
                };
                TrafficBody {
                    id: u.id,
                    position: u.position,
                    heading: f64::from(u.heading_deg),
                    velocity: u.velocity_enu_m_s,
                    half_length: dimensions[2] * 0.5,
                    half_width: dimensions[0] * 0.5,
                    manual,
                    path,
                }
            })
            .collect();
        let control = &mut self.living.traffic;
        control.elapsed_seconds = (control.elapsed_seconds + dt).min(1e12);
        let now = control.elapsed_seconds;
        let mut requests = BTreeMap::<u64, (u64, f64)>::new();
        // Each body requests only its nearest upcoming junction. This bounds
        // all persistent requests by the actual living-body limit.
        for body in bodies.iter().filter(|b| !b.path.is_empty()) {
            if let Some((id, distance)) = network
                .junctions()
                .iter()
                .filter_map(|j| body.entry(j).map(|d| (j.id, d)))
                .filter(|(_, d)| *d <= (10. * dt + 18.).max(30.))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            {
                requests.insert(body.id, (id, distance));
            }
        }
        control.junctions.retain(|state| {
            network
                .junctions()
                .iter()
                .any(|j| j.id == state.junction_id)
        });
        for state in &mut control.junctions {
            state.queue.retain(|r| {
                requests
                    .get(&r.unit_id)
                    .is_some_and(|(id, _)| *id == state.junction_id)
            });
            if state
                .holder
                .is_some_and(|id| !state.queue.iter().any(|r| r.unit_id == id))
            {
                state.holder = None;
                state.last_position = None;
            }
        }
        for (&unit, &(junction, _)) in &requests {
            let index = control
                .junctions
                .iter()
                .position(|j| j.junction_id == junction)
                .unwrap_or_else(|| {
                    control.junctions.push(TrafficJunctionState {
                        junction_id: junction,
                        queue: vec![],
                        holder: None,
                        granted_at: now,
                        progress_at: now,
                        last_position: None,
                    });
                    control.junctions.len() - 1
                });
            if !control.junctions[index]
                .queue
                .iter()
                .any(|r| r.unit_id == unit)
            {
                // Rebase at the representable sequence boundary. Relative FIFO
                // order is preserved, so even saturated clocks remain valid.
                if control.next_ticket == u64::MAX {
                    rebase_tickets(control);
                }
                control.next_ticket += 1;
                control.junctions[index].queue.push(TrafficRequest {
                    unit_id: unit,
                    ticket: control.next_ticket,
                });
            }
        }
        control.junctions.retain(|j| !j.queue.is_empty());
        if control.next_ticket > u64::MAX - MAX_LIVING_UNITS as u64 {
            rebase_tickets(control);
        }
        control.junctions.sort_by_key(|j| j.junction_id);
        let metrics = &mut self.living.metrics;
        for index in 0..control.junctions.len() {
            let id = control.junctions[index].junction_id;
            let junction = network.junctions().iter().find(|j| j.id == id).unwrap();
            let state = &mut control.junctions[index];
            if let Some(holder) = state
                .holder
                .and_then(|id| bodies.iter().find(|b| b.id == id))
            {
                if state
                    .last_position
                    .is_some_and(|p| p.surface_distance_m(holder.position) >= 0.2)
                {
                    state.last_position = Some(holder.position);
                    state.progress_at = now;
                }
                // A vehicle already in the box retains occupancy until it
                // physically clears; timeout never grants passage through it.
                if !holder.inside(junction)
                    && (now - state.progress_at >= 8. || now - state.granted_at >= 30.)
                {
                    let unit = holder.id;
                    state.queue.retain(|r| r.unit_id != unit);
                    state.holder = None;
                    state.last_position = None;
                    metrics.intersection_requeues = metrics.intersection_requeues.saturating_add(1);
                    // The old request joins the tail after other waiting users.
                    if control.next_ticket < u64::MAX {
                        control.next_ticket += 1;
                        state.queue.push(TrafficRequest {
                            unit_id: unit,
                            ticket: control.next_ticket,
                        });
                    }
                }
            }
            if state.holder.is_none() {
                for request in &state.queue {
                    let Some(body) = bodies.iter().find(|b| b.id == request.unit_id) else {
                        continue;
                    };
                    if occupied(junction, body.id, &bodies) {
                        continue;
                    }
                    if exit_blocked(body, junction, &bodies) {
                        metrics.blocked_exits = metrics.blocked_exits.saturating_add(1);
                        continue;
                    }
                    state.holder = Some(body.id);
                    state.granted_at = now;
                    state.progress_at = now;
                    state.last_position = Some(body.position);
                    metrics.intersection_grants = metrics.intersection_grants.saturating_add(1);
                    break;
                }
            }
        }
        for body in bodies.iter().filter(|b| !b.path.is_empty()) {
            let mut limit = f64::INFINITY;
            for junction in network.junctions() {
                if body.inside(junction) {
                    continue;
                } // always allow physical clearing
                let permit = control
                    .junctions
                    .iter()
                    .any(|s| s.junction_id == junction.id && s.holder == Some(body.id));
                if !permit
                    || occupied(junction, body.id, &bodies)
                    || exit_blocked(body, junction, &bodies)
                {
                    if let Some(entry) = body.entry(junction) {
                        limit = limit.min((entry - 0.25).max(0.));
                    }
                    frame
                        .blocked_zones
                        .entry(body.id)
                        .or_default()
                        .push((junction.position, body.radius(junction)));
                }
            }
            if limit.is_finite() {
                frame.limits.insert(body.id, limit);
                if limit < 10. * dt {
                    metrics.intersection_waits = metrics.intersection_waits.saturating_add(1);
                }
            }
        }
        Ok(frame)
    }
}

fn rebase_tickets(control: &mut TrafficControl) {
    let mut tickets: Vec<_> = control
        .junctions
        .iter()
        .flat_map(|j| j.queue.iter().map(|r| r.ticket))
        .collect();
    tickets.sort_unstable();
    tickets.dedup();
    for state in &mut control.junctions {
        for request in &mut state.queue {
            request.ticket = tickets.binary_search(&request.ticket).unwrap() as u64 + 1;
        }
    }
    control.next_ticket = tickets.len() as u64;
}
fn occupied(junction: &RoadJunction, id: u64, bodies: &[TrafficBody]) -> bool {
    bodies.iter().filter(|b| b.id != id).any(|body| {
        body.inside(junction)
            || (body.manual && {
                let f = LocalFrame::new(body.position);
                let predicted = f
                    .to_geo([body.velocity[0] * 1.5, 0., -body.velocity[1] * 1.5])
                    .unwrap_or(body.position);
                circle_entry(
                    body.position,
                    predicted,
                    junction.position,
                    body.radius(junction),
                )
                .is_some()
            })
    })
}
fn exit_blocked(body: &TrafficBody, junction: &RoadJunction, bodies: &[TrafficBody]) -> bool {
    let Some((exit, heading)) = body.exit(junction) else {
        return false;
    };
    // Reserve the actual outgoing footprint, with the same longitudinal and
    // lateral margins as a parallel queue. Applying unrotated half-extents in
    // the other body's axes both misses crosswise nose contacts and blocks
    // physically clear side lanes. The approaching heading is not the heading
    // the vehicle will have after turning through this junction.
    let exiting = terra_core::ContactBody {
        id: body.id,
        position: exit,
        heading_deg: heading,
        dimensions_m: [body.half_width * 2. + 0.7, 1., body.half_length * 2. + 1.5],
    };
    bodies.iter().filter(|b| b.id != body.id).any(|other| {
        if (exit.altitude_m - other.position.altitude_m).abs() >= 3. {
            return false;
        }
        let mut position = other.position;
        // Keep the existing three-metre grade-separation policy. Inside that
        // band this is a horizontal exit reservation, not a new height rule.
        position.altitude_m = exit.altitude_m;
        let obstacle = terra_core::ContactBody {
            id: other.id,
            position,
            heading_deg: other.heading,
            dimensions_m: [other.half_width * 2., 1., other.half_length * 2.],
        };
        terra_core::sweep_body_contact(exiting, exit, heading, obstacle)
            .map_or(true, |contact| contact.is_some())
    })
}
// Follow the actual outgoing lane segment. Projecting radially from the
// junction centre shrinks its lateral offset and falsely calls an opposing
// waiting lane an occupied exit (a four-way deadlock).
fn circle_exit(
    from: GeoPosition,
    to: GeoPosition,
    center: GeoPosition,
    radius: f64,
) -> Option<GeoPosition> {
    let frame = LocalFrame::new(center);
    let a = frame.to_local(from);
    let b = frame.to_local(to);
    let d = [b[0] - a[0], b[2] - a[2]];
    let aa = d[0] * d[0] + d[1] * d[1];
    if aa < 1e-9 {
        return None;
    }
    let bb = 2. * (a[0] * d[0] + a[2] * d[1]);
    let cc = a[0] * a[0] + a[2] * a[2] - radius * radius;
    let discriminant = bb * bb - 4. * aa * cc;
    if discriminant < 0. {
        return None;
    }
    let t = (-bb + discriminant.sqrt()) / (2. * aa);
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let mut p = frame
        .to_geo([a[0] + t * d[0], a[1] + t * (b[1] - a[1]), a[2] + t * d[1]])
        .ok()?;
    p.altitude_m = from.altitude_m + t * (to.altitude_m - from.altitude_m);
    Some(p)
}
fn circle_entry(
    from: GeoPosition,
    to: GeoPosition,
    center: GeoPosition,
    radius: f64,
) -> Option<f64> {
    let frame = LocalFrame::new(center);
    let a = frame.to_local(from);
    let b = frame.to_local(to);
    let d = [b[0] - a[0], b[2] - a[2]];
    let aa = d[0] * d[0] + d[1] * d[1];
    if aa < 1e-9 {
        return None;
    }
    let bb = 2. * (a[0] * d[0] + a[2] * d[1]);
    let cc = a[0] * a[0] + a[2] * a[2] - radius * radius;
    let discriminant = bb * bb - 4. * aa * cc;
    if discriminant < 0. {
        return None;
    }
    let start = (-bb - discriminant.sqrt()) / (2. * aa);
    let end = (-bb + discriminant.sqrt()) / (2. * aa);
    if end < 0. || start > 1. {
        return None;
    }
    let t = start.clamp(0., 1.);
    let altitude = from.altitude_m + (to.altitude_m - from.altitude_m) * t;
    ((altitude - center.altitude_m).abs() < 3.).then_some(t * aa.sqrt())
}

#[cfg(test)]
mod exit_tests {
    use super::*;

    fn turn() -> (TrafficBody, RoadJunction, LocalFrame) {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let frame = LocalFrame::new(origin);
        let point = |east, south| {
            let mut p = frame.to_geo([east, 0., south]).unwrap();
            p.altitude_m = 100.;
            p
        };
        let body = TrafficBody {
            id: 1,
            position: point(-25., 0.),
            heading: 90.,
            velocity: [0.; 3],
            half_length: 4.,
            half_width: 1.,
            manual: false,
            path: vec![origin, point(0., -25.)],
        };
        let junction = RoadJunction {
            id: 1,
            position: origin,
            radius_m: 3.,
            approaches: 4,
        };
        (body, junction, frame)
    }

    #[test]
    fn rotated_exit_clearance_detects_a_crosswise_body_at_the_future_nose() {
        let (body, junction, frame) = turn();
        let exit_north = body.radius(&junction) + 2.;
        let mut other = TrafficBody {
            id: 2,
            position: frame.to_geo([0., 0., -exit_north - 4.]).unwrap(),
            heading: 90.,
            velocity: [0.; 3],
            half_length: 2.,
            half_width: 1.,
            manual: false,
            path: vec![],
        };
        other.position.altitude_m = 100.;
        assert!(
            exit_blocked(&body, &junction, &[other]),
            "the outgoing northbound body overlaps a crosswise vehicle four metres beyond its exit center"
        );
    }

    #[test]
    fn rotated_exit_clearance_does_not_apply_vehicle_length_sideways() {
        let (body, junction, frame) = turn();
        let exit_north = body.radius(&junction) + 2.;
        let mut other = TrafficBody {
            id: 2,
            position: frame.to_geo([5., 0., -exit_north]).unwrap(),
            heading: 90.,
            velocity: [0.; 3],
            half_length: 2.,
            half_width: 1.,
            manual: false,
            path: vec![],
        };
        other.position.altitude_m = 100.;
        assert!(
            !exit_blocked(&body, &junction, &[other]),
            "a clear perpendicular body five metres to the side is outside both expanded footprints"
        );
    }

    #[test]
    fn parallel_exit_clearance_keeps_existing_margins_and_grade_separation() {
        let (body, junction, frame) = turn();
        let exit_north = body.radius(&junction) + 2.;
        for (east, beyond, height, blocked) in [
            (2.3, 0., 100., true),
            (2.4, 0., 100., false),
            (0., 6.7, 100., true),
            (0., 6.8, 100., false),
            (0., 0., 102.9, true),
            (0., 0., 103., false),
        ] {
            let mut position = frame.to_geo([east, 0., -exit_north - beyond]).unwrap();
            position.altitude_m = height;
            let other = TrafficBody {
                id: 2,
                position,
                heading: 0.,
                velocity: [0.; 3],
                half_length: 2.,
                half_width: 1.,
                manual: false,
                path: vec![],
            };
            assert_eq!(
                exit_blocked(&body, &junction, &[other]),
                blocked,
                "offset {east}, {beyond}, height {height}"
            );
        }
    }
}
