use super::model::Node;
use bevy_egui::egui::{self, Rect, pos2, vec2};

/// Wires longer than this (world units) draw as short labelled stubs unless
/// they lie on the highlighted path, so the root does not span the canvas.
pub(super) const LONG_EDGE: f32 = 900.0;
pub(super) const STUB_LENGTH: f32 = 70.0;

type Routes = std::sync::Arc<std::collections::BTreeMap<(usize, usize), Vec<egui::Vec2>>>;
#[derive(Clone, Default)]
pub(super) struct WireCache {
    layout: Option<u64>,
    routes: Routes,
}
impl WireCache {
    pub(super) fn resolve(&mut self, nodes: &[Node]) -> Routes {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for node in nodes {
            node.position.x.to_bits().hash(&mut hash);
            node.position.y.to_bits().hash(&mut hash);
            node.prerequisites.hash(&mut hash);
        }
        let fingerprint = hash.finish();
        if self.layout != Some(fingerprint) {
            let mut routes = std::collections::BTreeMap::new();
            for (child, node) in nodes.iter().enumerate() {
                for parent in &node.prerequisites {
                    if *parent < nodes.len() {
                        routes.insert((*parent, child), skill_wire(nodes, *parent, child));
                    }
                }
            }
            self.routes = std::sync::Arc::new(routes);
            self.layout = Some(fingerprint);
        }
        self.routes.clone()
    }
}

pub(super) fn stub_label(parent: &str) -> String {
    // The pixel font has no arrow glyphs; the stub's own arrowhead shows direction.
    format!("from {parent}")
}

/// Whether `a` and `b` share area: rects that only touch do not.
pub(super) fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// Whether the wire segment `a`-`b` runs through `rect` (its bounding box
/// shares area with it; drawn wires are axis-aligned).
pub(super) fn segment_meets(rect: Rect, a: egui::Pos2, b: egui::Pos2) -> bool {
    overlaps(Rect::from_two_pos(a, b), rect)
}

/// Where a stub's label may go, best first, as anchor points and the side of
/// the label each anchors (CF-2-02): beyond the stub's far end along its
/// overall direction, so a short bend never puts it over the child's icon;
/// then beside the stub's longest leg, running from its far end toward the
/// child, centred on the leg, or running from its far end away from the
/// child, on either side, `lift` points further off the leg than the 3-4 pt
/// gap (the spot beyond the far end only at no lift). `points` is the drawn stub in
/// screen points, far end first.
pub(super) fn label_spots(points: &[egui::Pos2], lift: f32) -> Vec<(egui::Pos2, egui::Align2)> {
    use egui::{Align, Align2};
    let start = points[0];
    let tip = *points.last().expect("nonempty stub");
    let lead = (tip - start).normalized();
    // The stub's longest leg, from its end nearer the far end: a stub may
    // start with a sliver of a bend.
    let (from, leg) = points
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .max_by(|a, b| (a.1 - a.0).length().total_cmp(&(b.1 - b.0).length()))
        .unwrap_or((start, tip));
    let middle = from + (leg - from) * 0.5;
    let beyond = if lead.x > 0.5 {
        (start + vec2(-4.0, 0.0), Align2::RIGHT_CENTER)
    } else if lead.x < -0.5 {
        (start + vec2(4.0, 0.0), Align2::LEFT_CENTER)
    } else if lead.y > 0.0 {
        (start + vec2(0.0, -3.0), Align2::CENTER_BOTTOM)
    } else {
        (start + vec2(0.0, 3.0), Align2::CENTER_TOP)
    };
    let mut spots = if lift == 0.0 { vec![beyond] } else { vec![] };
    let along = leg - from;
    let level = along.x.abs() >= along.y.abs();
    // The label's edge at the far end when it runs toward the child, and
    // when it runs away from it.
    let (toward, away) = if (level && along.x < 0.0) || (!level && along.y < 0.0) {
        (Align::Max, Align::Min)
    } else {
        (Align::Min, Align::Max)
    };
    // A level leg: over it, then under it. An upright one: to its left,
    // then to its right.
    for (gap, side) in [(-1.0, Align::Max), (1.0, Align::Min)] {
        for (at, along_leg) in [(from, toward), (middle, Align::Center), (from, away)] {
            spots.push(if level {
                (
                    at + vec2(0.0, (3.0 + lift) * gap),
                    Align2([along_leg, side]),
                )
            } else {
                (
                    at + vec2((4.0 + lift) * gap, 0.0),
                    Align2([side, along_leg]),
                )
            });
        }
    }
    spots
}

pub(super) fn route_length(route: &[egui::Vec2]) -> f32 {
    route.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

/// The last `length` world units of a route, ending at the child.
pub(super) fn wire_stub(route: &[egui::Vec2], length: f32) -> Vec<egui::Vec2> {
    let mut stub = vec![*route.last().expect("nonempty route")];
    let mut left = length;
    for pair in route.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        let segment = (b - a).length();
        if segment >= left {
            stub.push(b + (a - b).normalized() * left);
            break;
        }
        left -= segment;
        stub.push(a);
    }
    stub.reverse();
    stub
}

pub(super) fn skill_obstacle(node: &Node) -> Rect {
    Rect::from_min_max(
        pos2(node.position.x - 92.0, node.position.y - 42.0),
        pos2(node.position.x + 92.0, node.position.y + 116.0),
    )
}
pub(super) fn wire_segment_clear(a: egui::Vec2, b: egui::Vec2, obstacles: &[Rect]) -> bool {
    obstacles.iter().all(|r| {
        if (a.y - b.y).abs() < 0.01 {
            !(a.y > r.top()
                && a.y < r.bottom()
                && a.x.max(b.x) > r.left()
                && a.x.min(b.x) < r.right())
        } else if (a.x - b.x).abs() < 0.01 {
            !(a.x > r.left()
                && a.x < r.right()
                && a.y.max(b.y) > r.top()
                && a.y.min(b.y) < r.bottom())
        } else {
            false
        }
    })
}
/// Orthogonal ports and open channels keep prerequisites out of every unrelated
/// icon/caption envelope. All geometry comes from the authored node positions.
pub(super) fn skill_wire(nodes: &[Node], parent: usize, child: usize) -> Vec<egui::Vec2> {
    let obstacles: Vec<_> = nodes.iter().map(skill_obstacle).collect();
    let source = nodes[parent].position;
    let target = nodes[child].position;
    let ports = |p: egui::Vec2| {
        [
            p + vec2(0.0, -52.0),
            p + vec2(-102.0, 0.0),
            p + vec2(102.0, 0.0),
        ]
    };
    let mut best = Vec::new();
    let mut best_cost = f32::INFINITY;
    {
        let mut consider = |path: Vec<egui::Vec2>| {
            if !path
                .windows(2)
                .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
            {
                return;
            }
            let cost: f32 = route_length(&path) + path.len() as f32 * 8.0;
            if cost < best_cost {
                best_cost = cost;
                best = path;
            }
        };
        for a in ports(source) {
            for b in ports(target) {
                consider(vec![a, vec2(a.x, b.y), b]);
                consider(vec![a, vec2(b.x, a.y), b]);
            }
        }
    }
    if best.is_empty() {
        for a in ports(source) {
            for b in ports(target) {
                for obstacle in &obstacles {
                    let mut consider = |path: Vec<egui::Vec2>| {
                        if !path
                            .windows(2)
                            .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
                        {
                            return;
                        }
                        let cost = route_length(&path) + path.len() as f32 * 8.0;
                        if cost < best_cost {
                            best_cost = cost;
                            best = path;
                        }
                    };
                    for y in [obstacle.top() - 10.0, obstacle.bottom() + 10.0] {
                        consider(vec![a, vec2(a.x, y), vec2(b.x, y), b]);
                    }
                    for x in [obstacle.left() - 10.0, obstacle.right() + 10.0] {
                        consider(vec![a, vec2(x, a.y), vec2(x, b.y), b]);
                    }
                }
            }
        }
    }
    if best.is_empty() {
        return best;
    }
    let first = best[0];
    let last = *best.last().expect("nonempty path");
    best.insert(0, source + (first - source).normalized() * 34.0);
    best.push(target + (last - target).normalized() * 34.0);
    best.dedup_by(|a, b| (*a - *b).length() < 0.01);
    best
}
