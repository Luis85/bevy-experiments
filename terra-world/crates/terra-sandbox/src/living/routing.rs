//! Bounded road routing with segment access and reference right-hand lanes.
use super::*;

impl LivingWorld {
    fn road_path(&self, from: GeoPosition, to: GeoPosition, threshold_m: f64) -> Vec<GeoPosition> {
        if from.surface_distance_m(to) < threshold_m || self.roads.is_empty() {
            return vec![to];
        }
        let plan = RoadNetwork::new(&self.roads).plan(from, to, false);
        if plan.is_blocked() {
            vec![from]
        } else {
            plan.points
        }
    }
}

impl LivingWorld {
    /// Road paths include short detours around authored facility footprints, so
    /// warehouses can be reached from either side without walking through them.
    pub fn route_between(&self, from: GeoPosition, to: GeoPosition) -> Vec<GeoPosition> {
        self.detour_facilities(ActorKind::Vehicle, from, self.road_path(from, to, 25.))
    }

    /// Directed, grade-aware reference lanes. A blocked source restriction
    /// holds position instead of silently taking a wrong-way direct fallback.
    pub fn vehicle_route_between(&self, from: GeoPosition, to: GeoPosition) -> Vec<GeoPosition> {
        let plan = self.vehicle_route_plan(from, to);
        if plan.is_blocked() {
            vec![from]
        } else {
            plan.points
        }
    }

    /// Exposes the difference between a connected route, explicit off-road
    /// fallback and a blocked restriction to game UI and logistics policies.
    pub fn vehicle_route_plan(&self, from: GeoPosition, to: GeoPosition) -> RoadPlan {
        let mut plan = RoadNetwork::new(&self.roads).plan(from, to, true);
        if !plan.is_blocked() {
            plan.points = self.offset_path(from, to, plan.points, false);
            if plan.points == [from] && from.surface_distance_m(to) > 0.001 {
                plan.kind = RoadPlanKind::Unavailable;
                plan.points.clear();
            }
        }
        plan
    }

    /// Authored pedestrian edge paths keep routine residents out of the traffic
    /// lane. This offset is scenario navigation, not observed sidewalk geometry.
    pub fn pedestrian_route_between(&self, from: GeoPosition, to: GeoPosition) -> Vec<GeoPosition> {
        self.offset_route_between(from, to, true)
    }

    fn offset_route_between(
        &self,
        from: GeoPosition,
        to: GeoPosition,
        pedestrian: bool,
    ) -> Vec<GeoPosition> {
        let path = self.road_path(from, to, if pedestrian { 25. } else { 2. });
        self.offset_path(from, to, path, pedestrian)
    }

    fn offset_path(
        &self,
        from: GeoPosition,
        to: GeoPosition,
        path: Vec<GeoPosition>,
        pedestrian: bool,
    ) -> Vec<GeoPosition> {
        let kind = if pedestrian {
            ActorKind::Human
        } else {
            ActorKind::Vehicle
        };
        if path.len() < 2 {
            return self.detour_facilities(kind, from, path);
        }
        let mut lane = Vec::with_capacity(path.len() + 1);
        for (i, &point) in path.iter().enumerate() {
            let width = self
                .roads
                .iter()
                .flat_map(|road| road.points.windows(2).map(move |pair| (road.width_m, pair)))
                .map(|(width, pair)| {
                    (
                        width,
                        point.surface_distance_m(segment_projection(point, pair[0], pair[1])),
                    )
                })
                .filter(|(_, distance)| *distance < 1.)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(width, _)| width);
            let previous = i.checked_sub(1).map_or(from, |i| path[i]);
            let next = path.get(i + 1).copied().unwrap_or(point);
            let offset = width.map_or(0., |width| {
                if pedestrian {
                    f64::from(width) * 0.5 + 0.6
                } else {
                    lane_offset(width)
                }
            });
            lane.push(offset_waypoint(previous, point, next, offset));
        }
        if lane.last().copied() != Some(to) {
            if lane.len() == 129 {
                lane.pop();
            }
            lane.push(to);
        }
        self.detour_facilities(kind, from, lane)
    }

    fn detour_facilities(
        &self,
        kind: ActorKind,
        from: GeoPosition,
        path: Vec<GeoPosition>,
    ) -> Vec<GeoPosition> {
        let footprints = self.facility_footprints();
        if footprints.is_empty() {
            return path;
        }
        let mut result = Vec::new();
        let mut previous = from;
        for target in path {
            if !facility_path_clear(kind, previous, target, &footprints) {
                let frame = LocalFrame::new(previous);
                let end = frame.to_local(target);
                let distance = end[0].hypot(end[2]);
                if distance > 0.001 {
                    let direction = [end[0] / distance, end[2] / distance];
                    let mut blockers: Vec<_> = footprints
                        .iter()
                        .filter(|(p, r)| !facility_path_clear(kind, previous, target, &[(*p, *r)]))
                        .collect();
                    blockers.sort_by(|a, b| {
                        previous
                            .surface_distance_m(a.0)
                            .total_cmp(&previous.surface_distance_m(b.0))
                    });
                    for &(center, radius) in blockers.into_iter().take(8) {
                        let c = frame.to_local(center);
                        let margin = radius + 3.;
                        let make = |side: f64| {
                            let normal = [-direction[1] * side, direction[0] * side];
                            let a = frame
                                .to_geo([
                                    c[0] - direction[0] * margin + normal[0] * margin,
                                    0.,
                                    c[2] - direction[1] * margin + normal[1] * margin,
                                ])
                                .unwrap_or(previous);
                            let b = frame
                                .to_geo([
                                    c[0] + direction[0] * margin + normal[0] * margin,
                                    0.,
                                    c[2] + direction[1] * margin + normal[1] * margin,
                                ])
                                .unwrap_or(target);
                            (a, b)
                        };
                        let left = make(1.);
                        let right = make(-1.);
                        let cost = |(a, b): (GeoPosition, GeoPosition)| {
                            previous.surface_distance_m(a)
                                + a.surface_distance_m(b)
                                + b.surface_distance_m(target)
                        };
                        let (a, b) = if cost(left) <= cost(right) {
                            left
                        } else {
                            right
                        };
                        if result.len() + 3 > 129 {
                            return vec![from];
                        }
                        result.push(a);
                        result.push(b);
                    }
                }
            }
            result.push(target);
            previous = target;
            if result.len() > 129 {
                return vec![from];
            }
        }
        result
    }
}

pub(super) fn segment_projection(
    point: GeoPosition,
    a: GeoPosition,
    b: GeoPosition,
) -> GeoPosition {
    let frame = LocalFrame::new(a);
    let p = frame.to_local(point);
    let end = frame.to_local(b);
    let length_sq = end[0] * end[0] + end[2] * end[2];
    let t = if length_sq > 1e-8 {
        ((p[0] * end[0] + p[2] * end[2]) / length_sq).clamp(0., 1.)
    } else {
        0.
    };
    let mut result = frame.to_geo(end.map(|v| v * t)).unwrap_or(a);
    result.altitude_m = a.altitude_m + (b.altitude_m - a.altitude_m) * t;
    result
}
fn lane_offset(width: f32) -> f64 {
    if width < 4.5 {
        0.
    } else {
        // Keep a modest centre clearance for bodies standing on the road
        // centreline while retaining a one-metre ordinary-car edge margin.
        (f64::from(width) * 0.25 + 0.25)
            .min(f64::from(width) * 0.5 - 1.)
            .clamp(1.1, 1.8)
    }
}
fn offset_waypoint(
    previous: GeoPosition,
    point: GeoPosition,
    next: GeoPosition,
    offset: f64,
) -> GeoPosition {
    if offset == 0. {
        return point;
    }
    let frame = LocalFrame::new(point);
    let prior = frame.to_local(previous);
    let later = frame.to_local(next);
    let incoming = [-prior[0], -prior[2]];
    let outgoing = [later[0], later[2]];
    let normalize = |v: [f64; 2]| {
        let length = v[0].hypot(v[1]);
        if length > 0.01 {
            [v[0] / length, v[1] / length]
        } else {
            [0., 0.]
        }
    };
    let a = normalize(incoming);
    let b = normalize(outgoing);
    let mut direction = normalize([a[0] + b[0], a[1] + b[1]]);
    if direction == [0., 0.] {
        direction = if b == [0., 0.] { a } else { b };
    }
    // Local axes are east/up/south: right of forward (x,z) is (-z,x).
    let mut result = frame
        .to_geo([-direction[1] * offset, 0., direction[0] * offset])
        .unwrap_or(point);
    result.altitude_m = point.altitude_m;
    result
}

/// A closed directed traffic patrol, with opposite travel directions occupying
/// separate reference lanes. A closed source ring wraps rather than reversing.
pub(crate) fn traffic_lane_patrol(road: &RoadRoute) -> Vec<GeoPosition> {
    let offset = lane_offset(road.width_m);
    let closed = road.points.len() > 2
        && road.points[0].surface_distance_m(*road.points.last().unwrap()) < 2.;
    let make = |points: &[GeoPosition]| {
        points
            .iter()
            .enumerate()
            .map(|(i, &point)| {
                let previous = if i > 0 {
                    points[i - 1]
                } else if closed {
                    points[points.len() - 2]
                } else {
                    point
                };
                let next = points
                    .get(i + 1)
                    .copied()
                    .unwrap_or_else(|| if closed { points[1] } else { point });
                offset_waypoint(previous, point, next, offset)
            })
            .collect::<Vec<_>>()
    };
    let ordered = if road.rules.direction == RoadDirection::Reverse {
        road.points.iter().rev().copied().collect::<Vec<_>>()
    } else {
        road.points.clone()
    };
    let mut patrol = make(&ordered);
    if !closed && road.rules.direction == RoadDirection::Both {
        let reversed: Vec<_> = road.points.iter().rev().copied().collect();
        patrol.extend(make(&reversed));
        if let Some(first) = patrol.first().copied() {
            patrol.push(first);
        }
    } else if closed {
        if let Some(first) = patrol.first().copied() {
            *patrol.last_mut().unwrap() = first;
        }
    }
    patrol
}

#[cfg(test)]
mod tests {
    use super::*;
    fn straight() -> (LivingWorld, LocalFrame) {
        let origin = GeoPosition::new(48., 11., 10.).unwrap();
        let frame = LocalFrame::new(origin);
        let mut world = LivingWorld {
            origin: Some(origin),
            ..Default::default()
        };
        world.roads.push(RoadRoute {
            id: "test:road".into(),
            rules: Default::default(),
            width_m: 6.,
            points: vec![origin, frame.to_geo([0., 0., -100.]).unwrap()],
        });
        (world, frame)
    }

    #[test]
    fn road_access_projects_to_segment_without_detouring_to_its_endpoint() {
        let (world, frame) = straight();
        let from = frame.to_geo([0., 0., -20.]).unwrap();
        let to = frame.to_geo([0., 0., -80.]).unwrap();
        let path = world.route_between(from, to);
        assert!(path[0].surface_distance_m(from) < 0.01);
        assert_eq!(path.last(), Some(&to));
        let mut previous = from;
        let length: f64 = path
            .iter()
            .map(|p| {
                let distance = previous.surface_distance_m(*p);
                previous = *p;
                distance
            })
            .sum();
        assert!(length < 61., "mid-segment route detoured {length} metres");
    }

    #[test]
    fn opposite_vehicle_directions_use_separate_lanes_and_exact_stops() {
        let (world, frame) = straight();
        let from = frame.to_geo([0., 0., -20.]).unwrap();
        let to = frame.to_geo([0., 0., -80.]).unwrap();
        let northbound = world.vehicle_route_between(from, to);
        let southbound = world.vehicle_route_between(to, from);
        assert!(frame.to_local(northbound[0])[0] > 1.4);
        assert!(frame.to_local(southbound[0])[0] < -1.4);
        assert_eq!(northbound.last(), Some(&to));
        assert_eq!(southbound.last(), Some(&from));
    }

    #[test]
    fn open_road_patrol_returns_in_opposite_lane_and_closes_without_reversal() {
        let (world, frame) = straight();
        let path = traffic_lane_patrol(&world.roads[0]);
        assert_eq!(path.len(), 5);
        assert_eq!(path.first(), path.last());
        assert!(frame.to_local(path[0])[0] > 1.4);
        assert!(frame.to_local(path[2])[0] < -1.4);
        let narrow = RoadRoute {
            width_m: 3.,
            ..world.roads[0].clone()
        };
        assert_eq!(traffic_lane_patrol(&narrow)[0], narrow.points[0]);
    }
}
