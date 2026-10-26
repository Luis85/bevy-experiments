//! Bounded road-vertex routing for the reference settlement.
use super::*;

impl LivingWorld {
    /// Route through connected road vertices. Walking access legs connect to
    /// the nearest road vertex; disconnected graphs fall back to a direct path.
    fn road_path(&self, from: GeoPosition, to: GeoPosition) -> Vec<GeoPosition> {
        if from.surface_distance_m(to) < 25. || self.roads.is_empty() {
            return vec![to];
        }
        let mut points: Vec<GeoPosition> = vec![];
        let mut links: Vec<(usize, usize)> = vec![];
        for road in self.roads.iter().take(MAX_LIVING_ROADS) {
            let mut prev = None;
            for &p in &road.points {
                let index = points
                    .iter()
                    .position(|q| p.surface_distance_m(*q) < 3.)
                    .unwrap_or_else(|| {
                        points.push(p);
                        points.len() - 1
                    });
                if let Some(a) = prev
                    && a != index
                {
                    links.push((a, index));
                }
                prev = Some(index);
            }
        }
        if points.is_empty() || points.len() > 2048 {
            return vec![to];
        }
        let nearest = |p: GeoPosition| {
            points
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    p.surface_distance_m(**a)
                        .total_cmp(&p.surface_distance_m(**b))
                })
                .map(|(i, _)| i)
                .unwrap()
        };
        let start = nearest(from);
        let goal = nearest(to);
        let mut distances = vec![f64::INFINITY; points.len()];
        let mut previous = vec![None; points.len()];
        let mut visited = vec![false; points.len()];
        distances[start] = 0.;
        for _ in 0..points.len() {
            let Some(i) = (0..points.len())
                .filter(|i| !visited[*i])
                .min_by(|a, b| distances[*a].total_cmp(&distances[*b]))
            else {
                break;
            };
            if !distances[i].is_finite() || i == goal {
                break;
            }
            visited[i] = true;
            for &(a, b) in &links {
                let j = if a == i {
                    b
                } else if b == i {
                    a
                } else {
                    continue;
                };
                let next = distances[i] + points[i].surface_distance_m(points[j]);
                if next < distances[j] {
                    distances[j] = next;
                    previous[j] = Some(i);
                }
            }
        }
        if !distances[goal].is_finite() {
            return vec![to];
        }
        let mut path = vec![points[goal]];
        let mut at = goal;
        while at != start {
            let Some(p) = previous[at] else {
                return vec![to];
            };
            at = p;
            path.push(points[at]);
            if path.len() > 128 {
                return vec![to];
            }
        }
        path.reverse();
        path.push(to);
        path
    }
}

impl LivingWorld {
    /// Road paths include short detours around authored facility footprints, so
    /// warehouses can be reached from either side without walking through them.
    pub fn route_between(&self, from: GeoPosition, to: GeoPosition) -> Vec<GeoPosition> {
        let path = self.road_path(from, to);
        let footprints = self.facility_footprints();
        if footprints.is_empty() {
            return path;
        }
        let mut result = Vec::new();
        let mut previous = from;
        for target in path {
            if !facility_path_clear(ActorKind::Vehicle, previous, target, &footprints) {
                let frame = LocalFrame::new(previous);
                let end = frame.to_local(target);
                let distance = end[0].hypot(end[2]);
                if distance > 0.001 {
                    let direction = [end[0] / distance, end[2] / distance];
                    let mut blockers: Vec<_> = footprints
                        .iter()
                        .filter(|(p, r)| {
                            !facility_path_clear(ActorKind::Vehicle, previous, target, &[(*p, *r)])
                        })
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
                            return vec![to];
                        }
                        result.push(a);
                        result.push(b);
                    }
                }
            }
            result.push(target);
            previous = target;
            if result.len() > 129 {
                return vec![to];
            }
        }
        result
    }
}
