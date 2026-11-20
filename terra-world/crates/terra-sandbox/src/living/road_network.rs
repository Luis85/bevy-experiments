//! Bounded, source-aware directed road topology. This is reference navigation,
//! not a claim of complete legal routing or inferred bridge elevation.
use super::*;
use std::collections::BinaryHeap;

const MAX_GRAPH_NODES: usize = 8192;
const MAX_GRAPH_SEGMENTS: usize = 8192;
pub const MAX_TRAFFIC_JUNCTIONS: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadDirection {
    #[default]
    Both,
    Forward,
    Reverse,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadStructure {
    #[default]
    Ground,
    Bridge,
    Tunnel,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoadRules {
    pub direction: RoadDirection,
    pub motor_access: bool,
    /// Explicit numeric source limit; unknown or conditional limits remain None.
    pub speed_limit_m_s: Option<f32>,
    /// Relative topology layer, never converted to invented metres of elevation.
    pub layer: i8,
    pub structure: RoadStructure,
}
impl Default for RoadRules {
    fn default() -> Self {
        Self {
            direction: RoadDirection::Both,
            motor_access: true,
            speed_limit_m_s: None,
            layer: 0,
            structure: RoadStructure::Ground,
        }
    }
}
impl RoadRules {
    /// Supported OSM-style metadata is deliberately small and conservative.
    /// Conditional restrictions and jurisdiction defaults need a game provider.
    pub fn from_properties(properties: &BTreeMap<String, String>) -> Self {
        let value = |key: &str| properties.get(key).map_or("", |v| v.trim());
        let oneway = value("oneway");
        let direction = match oneway {
            "yes" | "true" | "1" => RoadDirection::Forward,
            "-1" | "reverse" => RoadDirection::Reverse,
            "no" | "false" | "0" => RoadDirection::Both,
            "" if value("junction") == "roundabout" => RoadDirection::Forward,
            _ => RoadDirection::Both,
        };
        let explicit_access = ["motorcar", "motor_vehicle", "vehicle", "access"]
            .into_iter()
            .map(value)
            .find(|s| !s.is_empty());
        let mut motor_access = match explicit_access {
            Some("no" | "private" | "agricultural" | "forestry") => false,
            Some("yes" | "permissive" | "designated" | "destination") => true,
            _ => !matches!(
                value("highway"),
                "footway" | "pedestrian" | "path" | "steps" | "cycleway" | "bridleway"
            ),
        };
        if !matches!(
            oneway,
            "" | "yes" | "true" | "1" | "-1" | "reverse" | "no" | "false" | "0"
        ) {
            // Reversible/alternating and conditional direction needs a signal
            // policy; autonomous reference routing must not guess both ways.
            motor_access = false;
        }
        let speed = value("maxspeed");
        let (number, multiplier) = if let Some(number) = speed.strip_suffix("mph") {
            (number.trim(), 0.44704)
        } else if let Some(number) = speed.strip_suffix("km/h") {
            (number.trim(), 1. / 3.6)
        } else {
            (speed, 1. / 3.6)
        };
        let speed_limit_m_s = number
            .parse::<f32>()
            .ok()
            .map(|v| v * multiplier)
            .filter(|v| v.is_finite() && (0.5..=100.).contains(v));
        let present = |key: &str| !matches!(value(key), "" | "no" | "false" | "0");
        let structure = if present("bridge") {
            RoadStructure::Bridge
        } else if present("tunnel") {
            RoadStructure::Tunnel
        } else {
            RoadStructure::Ground
        };
        Self {
            direction,
            motor_access,
            speed_limit_m_s,
            layer: value("layer").parse::<i8>().unwrap_or(0).clamp(-10, 10),
            structure,
        }
    }
    pub(super) fn validate(&self) -> bool {
        (-10..=10).contains(&self.layer)
            && self
                .speed_limit_m_s
                .is_none_or(|v| finite_range(f64::from(v), 0.5, 100.))
    }
    fn allows(&self, forward: bool) -> bool {
        self.motor_access
            && matches!(
                (self.direction, forward),
                (RoadDirection::Both, _)
                    | (RoadDirection::Forward, true)
                    | (RoadDirection::Reverse, false)
            )
    }
    fn same_grade(&self, other: &Self) -> bool {
        self.layer == other.layer && self.structure == other.structure
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadPlanKind {
    Connected,
    DirectNoRoads,
    DirectDisconnected,
    BlockedAccess,
    BlockedDirection,
    Unavailable,
}
#[derive(Debug, Clone)]
pub struct RoadPlan {
    pub kind: RoadPlanKind,
    pub points: Vec<GeoPosition>,
}
impl RoadPlan {
    pub fn is_blocked(&self) -> bool {
        matches!(
            self.kind,
            RoadPlanKind::BlockedAccess
                | RoadPlanKind::BlockedDirection
                | RoadPlanKind::Unavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoadJunction {
    pub id: u64,
    pub position: GeoPosition,
    /// Width of the conflict zone, before a body's own clearance is added.
    pub radius_m: f64,
    pub approaches: usize,
}
#[derive(Debug, Clone)]
struct Node {
    point: GeoPosition,
    rules: RoadRules,
}
#[derive(Debug, Clone)]
struct Segment {
    a: usize,
    b: usize,
    width: f32,
    rules: RoadRules,
}
#[derive(Debug, Clone)]
struct SourceSegment {
    a: GeoPosition,
    b: GeoPosition,
    width: f32,
    rules: RoadRules,
    cuts: Vec<f64>,
}
/// Immutable compiled road topology, reusable by game hosts. Graph limits are
/// independent of population limits and never result in an unbounded search.
#[derive(Debug, Clone)]
pub struct RoadNetwork {
    nodes: Vec<Node>,
    segments: Vec<Segment>,
    junctions: Vec<RoadJunction>,
    available: bool,
}
impl RoadNetwork {
    pub fn new(roads: &[RoadRoute]) -> Self {
        let mut result = Self {
            nodes: vec![],
            segments: vec![],
            junctions: vec![],
            available: true,
        };
        if roads.len() > MAX_LIVING_ROADS
            || roads.iter().any(|r| {
                r.points.len() > 64
                    || r.points.len() < 2
                    || !finite_range(f64::from(r.width_m), 2., 30.)
                    || !r.rules.validate()
                    || r.points.iter().any(|p| p.validate().is_err())
            })
        {
            result.available = false;
            return result;
        }
        let mut ordered: Vec<_> = roads.iter().collect();
        ordered.sort_by(|a, b| a.id.cmp(&b.id));
        let mut source: Vec<_> = ordered
            .iter()
            .flat_map(|r| {
                r.points.windows(2).filter_map(|p| {
                    (p[0].surface_distance_m(p[1]) > 0.1).then(|| SourceSegment {
                        a: p[0],
                        b: p[1],
                        width: r.width_m,
                        rules: r.rules.clone(),
                        cuts: vec![0., 1.],
                    })
                })
            })
            .collect();
        let Some(origin) = source.first().map(|s| s.a) else {
            return result;
        };
        let frame = LocalFrame::new(origin);
        let bounds: Vec<_> = source
            .iter()
            .map(|s| {
                let a = frame.to_local(s.a);
                let b = frame.to_local(s.b);
                [
                    a[0].min(b[0]),
                    a[0].max(b[0]),
                    a[2].min(b[2]),
                    a[2].max(b[2]),
                ]
            })
            .collect();
        // Cheap local bounds reject disjoint segments before geodetic crossing
        // work. The worst-case pair count stays bounded by the source budget.
        // At-grade crossings become explicit connected vertices even if a
        // clipped polyline omitted that interior point. Layer/structure and
        // measured vertical separation prevent an invented overpass junction.
        for i in 0..source.len() {
            for j in i + 1..source.len() {
                if !source[i].rules.same_grade(&source[j].rules)
                    || bounds[i][0] > bounds[j][1] + 0.01
                    || bounds[j][0] > bounds[i][1] + 0.01
                    || bounds[i][2] > bounds[j][3] + 0.01
                    || bounds[j][2] > bounds[i][3] + 0.01
                {
                    continue;
                }
                if let Some((a, b)) = crossing(&source[i], &source[j]) {
                    source[i].cuts.push(a);
                    source[j].cuts.push(b);
                }
            }
        }
        let mut cells = BTreeMap::<(i64, i64), Vec<usize>>::new();
        for mut segment in source {
            segment.cuts.sort_by(f64::total_cmp);
            segment.cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
            let mut previous = None;
            for t in segment.cuts {
                let point = interpolate(segment.a, segment.b, t);
                let p = frame.to_local(point);
                let cell = ((p[0] / 1.).floor() as i64, (p[2] / 1.).floor() as i64);
                let mut existing = None;
                'scan: for x in -1..=1 {
                    for z in -1..=1 {
                        if let Some(indices) = cells.get(&(cell.0 + x, cell.1 + z)) {
                            for &index in indices {
                                let node = &result.nodes[index];
                                if node.rules.same_grade(&segment.rules)
                                    && (node.point.altitude_m - point.altitude_m).abs() < 1.5
                                    && node.point.surface_distance_m(point) < 0.75
                                {
                                    existing = Some(index);
                                    break 'scan;
                                }
                            }
                        }
                    }
                }
                let index = existing.unwrap_or_else(|| {
                    let index = result.nodes.len();
                    result.nodes.push(Node {
                        point,
                        rules: segment.rules.clone(),
                    });
                    cells.entry(cell).or_default().push(index);
                    index
                });
                if let Some(a) = previous {
                    if a != index {
                        result.segments.push(Segment {
                            a,
                            b: index,
                            width: segment.width,
                            rules: segment.rules.clone(),
                        });
                    }
                }
                previous = Some(index);
                if result.nodes.len() > MAX_GRAPH_NODES
                    || result.segments.len() > MAX_GRAPH_SEGMENTS
                {
                    result.available = false;
                    result.nodes.clear();
                    result.segments.clear();
                    return result;
                }
            }
        }
        let mut neighbors = vec![BTreeSet::new(); result.nodes.len()];
        let mut widths = vec![2_f32; result.nodes.len()];
        for segment in &result.segments {
            if segment.rules.motor_access {
                neighbors[segment.a].insert(segment.b);
                neighbors[segment.b].insert(segment.a);
                widths[segment.a] = widths[segment.a].max(segment.width);
                widths[segment.b] = widths[segment.b].max(segment.width);
            }
        }
        for (index, adjacent) in neighbors.iter().enumerate() {
            if adjacent.len() < 3 {
                continue;
            }
            let node = &result.nodes[index];
            result.junctions.push(RoadJunction {
                id: junction_id(node),
                position: node.point,
                radius_m: f64::from(widths[index]) * 0.5 + 1.,
                approaches: adjacent.len(),
            });
            if result.junctions.len() > MAX_TRAFFIC_JUNCTIONS {
                result.available = false;
                result.junctions.clear();
                return result;
            }
        }
        result.junctions.sort_by_key(|j| j.id);
        result
    }

    pub fn junctions(&self) -> &[RoadJunction] {
        &self.junctions
    }
    pub fn directed_lane_count(&self) -> usize {
        self.segments
            .iter()
            .map(|s| usize::from(s.rules.allows(true)) + usize::from(s.rules.allows(false)))
            .sum()
    }
    /// Numeric limit only on a nearby, vertically compatible motor road.
    /// Manual drivers may treat it as advisory; autonomous reference drivers obey it.
    pub fn speed_limit_at(&self, position: GeoPosition) -> Option<f64> {
        self.segments
            .iter()
            .filter(|s| s.rules.motor_access)
            .filter_map(|s| {
                let p = super::routing::segment_projection(
                    position,
                    self.nodes[s.a].point,
                    self.nodes[s.b].point,
                );
                let distance = position.surface_distance_m(p);
                ((position.altitude_m - p.altitude_m).abs() < 3.
                    && distance <= f64::from(s.width) * 0.5 + 1.)
                    .then_some((distance, s.rules.speed_limit_m_s))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .and_then(|(_, speed)| speed.map(f64::from))
    }

    pub fn plan(&self, from: GeoPosition, to: GeoPosition, vehicle: bool) -> RoadPlan {
        let direct = |kind| RoadPlan {
            kind,
            points: vec![to],
        };
        let blocked = |kind| RoadPlan {
            kind,
            points: vec![],
        };
        if !self.available || from.validate().is_err() || to.validate().is_err() {
            return blocked(RoadPlanKind::Unavailable);
        }
        if self.segments.is_empty() {
            return direct(RoadPlanKind::DirectNoRoads);
        }
        let nearest = |point: GeoPosition, motor_only: bool| {
            self.segments
                .iter()
                .enumerate()
                .filter(|(_, s)| !motor_only || s.rules.motor_access)
                .map(|(i, s)| {
                    let a = self.nodes[s.a].point;
                    let b = self.nodes[s.b].point;
                    let p = super::routing::segment_projection(point, a, b);
                    let distance = point
                        .surface_distance_m(p)
                        .hypot(point.altitude_m - p.altitude_m);
                    (
                        i,
                        p,
                        distance,
                        a.surface_distance_m(p) / a.surface_distance_m(b).max(0.001),
                    )
                })
                .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)))
        };
        let Some((start_segment, start_point, start_distance, start_t)) = nearest(from, vehicle)
        else {
            return blocked(RoadPlanKind::BlockedAccess);
        };
        let Some((end_segment, end_point, end_distance, end_t)) = nearest(to, vehicle) else {
            return blocked(RoadPlanKind::BlockedAccess);
        };
        if vehicle
            && [(from, start_distance), (to, end_distance)]
                .iter()
                .any(|(p, accepted)| {
                    nearest(*p, false).is_some_and(|(i, _, distance, _)| {
                        !self.segments[i].rules.motor_access && distance < 4. && *accepted > 20.
                    })
                })
        {
            return blocked(RoadPlanKind::BlockedAccess);
        }
        let mut points: Vec<_> = self.nodes.iter().map(|n| n.point).collect();
        let start = points.len();
        points.push(start_point);
        let goal = points.len();
        points.push(end_point);
        let graph = |directed: bool| {
            let mut adjacent = vec![Vec::new(); points.len()];
            let mut connect = |a: usize, b: usize| {
                adjacent[a].push((b, points[a].surface_distance_m(points[b])));
            };
            for s in &self.segments {
                if !vehicle || s.rules.motor_access {
                    if !directed || !vehicle || s.rules.allows(true) {
                        connect(s.a, s.b);
                    }
                    if !directed || !vehicle || s.rules.allows(false) {
                        connect(s.b, s.a);
                    }
                }
            }
            let first = &self.segments[start_segment];
            let last = &self.segments[end_segment];
            if !directed || !vehicle || first.rules.allows(true) {
                connect(start, first.b);
            }
            if !directed || !vehicle || first.rules.allows(false) {
                connect(start, first.a);
            }
            if !directed || !vehicle || last.rules.allows(true) {
                connect(last.a, goal);
            }
            if !directed || !vehicle || last.rules.allows(false) {
                connect(last.b, goal);
            }
            // A virtual node at the exact endpoint may enter/leave an adjoining
            // segment without illegally traversing this segment backwards.
            for (virtual_node, s, t) in [(start, first, start_t), (goal, last, end_t)] {
                let endpoint = if t < 1e-5 {
                    Some(s.a)
                } else if t > 1. - 1e-5 {
                    Some(s.b)
                } else {
                    None
                };
                if let Some(endpoint) = endpoint {
                    connect(virtual_node, endpoint);
                    connect(endpoint, virtual_node);
                }
            }
            if start_segment == end_segment {
                if !directed
                    || !vehicle
                    || (end_t >= start_t && first.rules.allows(true))
                    || (end_t <= start_t && first.rules.allows(false))
                {
                    connect(start, goal);
                }
            }
            adjacent
        };
        let Some(indices) = shortest(&graph(true), start, goal) else {
            if vehicle && shortest(&graph(false), start, goal).is_some() {
                return blocked(RoadPlanKind::BlockedDirection);
            }
            // Preserve the explicitly labelled off-road fallback only where no
            // recorded restriction would be bypassed by it.
            if vehicle
                && self
                    .segments
                    .iter()
                    .any(|s| !s.rules.motor_access || s.rules.direction != RoadDirection::Both)
            {
                return blocked(RoadPlanKind::BlockedDirection);
            }
            return direct(RoadPlanKind::DirectDisconnected);
        };
        if indices.len() > 128 {
            return blocked(RoadPlanKind::Unavailable);
        }
        let mut path: Vec<_> = indices.into_iter().map(|i| points[i]).collect();
        path.push(to);
        RoadPlan {
            kind: RoadPlanKind::Connected,
            points: path,
        }
    }
}

fn interpolate(a: GeoPosition, b: GeoPosition, t: f64) -> GeoPosition {
    let frame = LocalFrame::new(a);
    let delta = frame.to_local(b);
    let mut p = frame.to_geo(delta.map(|v| v * t)).unwrap_or(a);
    p.altitude_m = a.altitude_m + (b.altitude_m - a.altitude_m) * t;
    p
}
fn crossing(a: &SourceSegment, b: &SourceSegment) -> Option<(f64, f64)> {
    let f = LocalFrame::new(a.a);
    let end = f.to_local(a.b);
    let c = f.to_local(b.a);
    let d = f.to_local(b.b);
    let r = [end[0], end[2]];
    let s = [d[0] - c[0], d[2] - c[2]];
    let cross = |x: [f64; 2], y: [f64; 2]| x[0] * y[1] - x[1] * y[0];
    let denominator = cross(r, s);
    if denominator.abs() < 1e-6 {
        return None;
    }
    let t = cross([c[0], c[2]], s) / denominator;
    let u = cross([c[0], c[2]], r) / denominator;
    if !(-1e-7..=1.000_000_1).contains(&t) || !(-1e-7..=1.000_000_1).contains(&u) {
        return None;
    }
    let h1 = a.a.altitude_m + t * (a.b.altitude_m - a.a.altitude_m);
    let h2 = b.a.altitude_m + u * (b.b.altitude_m - b.a.altitude_m);
    ((h1 - h2).abs() < 1.5).then_some((t.clamp(0., 1.), u.clamp(0., 1.)))
}
fn junction_id(node: &Node) -> u64 {
    let mut value = 0xcbf29ce484222325_u64;
    for n in [
        (node.point.latitude * 1e7).round() as i64,
        (node.point.longitude * 1e7).round() as i64,
        (node.point.altitude_m * 10.).round() as i64,
        i64::from(node.rules.layer),
        node.rules.structure as i64,
    ] {
        for byte in n.to_le_bytes() {
            value = (value ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    value.max(1)
}
#[derive(Clone, Copy)]
struct Visit {
    cost: f64,
    node: usize,
}
impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.cost.total_cmp(&other.cost).is_eq()
    }
}
impl Eq for Visit {}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Visit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then(other.node.cmp(&self.node))
    }
}
fn shortest(graph: &[Vec<(usize, f64)>], start: usize, goal: usize) -> Option<Vec<usize>> {
    let mut distance = vec![f64::INFINITY; graph.len()];
    let mut previous = vec![None; graph.len()];
    let mut queue = BinaryHeap::new();
    distance[start] = 0.;
    queue.push(Visit {
        cost: 0.,
        node: start,
    });
    while let Some(Visit { cost, node }) = queue.pop() {
        if cost > distance[node] {
            continue;
        }
        if node == goal {
            break;
        }
        for &(next, length) in &graph[node] {
            let cost = cost + length;
            if cost + 1e-9 < distance[next] {
                distance[next] = cost;
                previous[next] = Some(node);
                queue.push(Visit { cost, node: next });
            }
        }
    }
    if !distance[goal].is_finite() {
        return None;
    }
    let mut result = vec![goal];
    while *result.last()? != start {
        result.push(previous[*result.last()?]?);
        if result.len() > MAX_GRAPH_NODES + 2 {
            return None;
        }
    }
    result.reverse();
    Some(result)
}
