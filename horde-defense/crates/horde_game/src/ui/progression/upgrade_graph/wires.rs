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
            separate_stubs(nodes, &mut routes);
            self.routes = std::sync::Arc::new(routes);
            self.layout = Some(fingerprint);
        }
        self.routes.clone()
    }
}

/// A wire as it draws off the highlighted path: a long edge is its stub.
fn drawn_wire(route: &[egui::Vec2]) -> Vec<egui::Vec2> {
    if route.len() >= 2 && route_length(route) > LONG_EDGE {
        wire_stub(route, STUB_LENGTH)
    } else {
        route.to_vec()
    }
}

/// CF-2-01: a long edge draws only its last `STUB_LENGTH` units, so a stub
/// that arrives along another drawn wire reads as that wire with an
/// arrowhead at each end (the `pistol_training` stub entered SMG proficiency
/// from the right along the start of the edge to Spray control). Each such
/// long edge is routed again to the cheapest route whose stub runs along no
/// other drawn wire, in edge order; it keeps its route when there is none.
fn separate_stubs(
    nodes: &[Node],
    routes: &mut std::collections::BTreeMap<(usize, usize), Vec<egui::Vec2>>,
) {
    let long: Vec<_> = routes
        .iter()
        .filter(|(_, route)| route.len() >= 2 && route_length(route) > LONG_EDGE)
        .map(|(edge, _)| *edge)
        .collect();
    for edge in long {
        let others: Vec<_> = routes
            .iter()
            .filter(|(other, _)| **other != edge)
            .map(|(_, route)| drawn_wire(route))
            .collect();
        let apart = |route: &[egui::Vec2]| {
            let stub = drawn_wire(route);
            !others.iter().any(|other| shares_segment(&stub, other))
        };
        if apart(&routes[&edge]) {
            continue;
        }
        // The run the edge shares with its siblings (its longest leg): of
        // two routes at one cost, the one that keeps it wins.
        let trunk = routes[&edge]
            .windows(2)
            .max_by(|a, b| (a[1] - a[0]).length().total_cmp(&(b[1] - b[0]).length()))
            .expect("a long route has a leg")
            .to_vec();
        let route = skill_wire_where(nodes, edge.0, edge.1, &|route| {
            (route_length(route) > LONG_EDGE && apart(route))
                .then(|| f32::from(u8::from(!shares_segment(route, &trunk))))
        });
        if route.len() >= 2 {
            routes.insert(edge, route);
        }
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

/// Whether two wires run along each other: a leg of `a` and a leg of `b`
/// lie on one line and overlap for more than half a world unit. Legs that
/// cross or only meet end to end do not count (drawn wires are
/// axis-aligned).
pub(super) fn shares_segment(a: &[egui::Vec2], b: &[egui::Vec2]) -> bool {
    let along = |p: egui::Vec2, q: egui::Vec2, r: egui::Vec2, s: egui::Vec2| {
        let level = |u: egui::Vec2, v: egui::Vec2| (u.y - v.y).abs() < 0.01;
        let upright = |u: egui::Vec2, v: egui::Vec2| (u.x - v.x).abs() < 0.01;
        let overlap =
            |p: f32, q: f32, r: f32, s: f32| p.max(q).min(r.max(s)) - p.min(q).max(r.min(s)) > 0.5;
        (level(p, q) && level(r, s) && level(p, r) && overlap(p.x, q.x, r.x, s.x))
            || (upright(p, q) && upright(r, s) && upright(p, r) && overlap(p.y, q.y, r.y, s.y))
    };
    a.windows(2)
        .any(|v| b.windows(2).any(|w| along(v[0], v[1], w[0], w[1])))
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
    skill_wire_where(nodes, parent, child, &|_| Some(0.0))
}

/// `skill_wire` over the finished routes `extra` allows, each at its cost
/// plus that extra: the cheapest direct one, else the cheapest that detours
/// round one obstacle, else none.
fn skill_wire_where(
    nodes: &[Node],
    parent: usize,
    child: usize,
    extra: &dyn Fn(&[egui::Vec2]) -> Option<f32>,
) -> Vec<egui::Vec2> {
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
    let best = std::cell::RefCell::new(Vec::new());
    let best_cost = std::cell::Cell::new(f32::INFINITY);
    let consider = |mut path: Vec<egui::Vec2>| {
        if !path
            .windows(2)
            .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
        {
            return;
        }
        let cost = route_length(&path) + path.len() as f32 * 8.0;
        if cost >= best_cost.get() {
            return;
        }
        // The legs from the ports to the icons' edges.
        let first = path[0];
        let last = *path.last().expect("nonempty path");
        path.insert(0, source + (first - source).normalized() * 34.0);
        path.push(target + (last - target).normalized() * 34.0);
        path.dedup_by(|a, b| (*a - *b).length() < 0.01);
        if let Some(cost) = extra(&path).map(|extra| cost + extra)
            && cost < best_cost.get()
        {
            best_cost.set(cost);
            *best.borrow_mut() = path;
        }
    };
    for a in ports(source) {
        for b in ports(target) {
            consider(vec![a, vec2(a.x, b.y), b]);
            consider(vec![a, vec2(b.x, a.y), b]);
        }
    }
    if best_cost.get().is_finite() {
        return best.into_inner();
    }
    for a in ports(source) {
        for b in ports(target) {
            for obstacle in &obstacles {
                for y in [obstacle.top() - 10.0, obstacle.bottom() + 10.0] {
                    consider(vec![a, vec2(a.x, y), vec2(b.x, y), b]);
                }
                for x in [obstacle.left() - 10.0, obstacle.right() + 10.0] {
                    consider(vec![a, vec2(x, a.y), vec2(x, b.y), b]);
                }
            }
        }
    }
    best.into_inner()
}
