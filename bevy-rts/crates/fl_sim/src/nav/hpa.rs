//! HPA* over the [`NavGrid`] (ADR-0015): one abstract graph per movement profile
//! ([`HpaGraphs`], `Derived` `"core.nav_hpa"`), path planning on it ([`plan_path`]) and the
//! invalidation of only the clusters a terrain change touches ([`HpaGraphs::invalidate`]).
//!
//! **Clusters.** The grid is cut into `Path.cluster_cells`-square clusters, row-major from
//! the south-west: [`ClusterId`] `cy × clusters_x + cx`. The last row and column may be
//! narrower.
//!
//! **Entrances.** Along each border between two adjacent clusters, each maximal run of cell
//! pairs `(a, b)` (`a` west or south of the border, `b` across it) passable on both sides gets
//! one transition at index `start + (len − 1) / 2` if the run is shorter than 6, and two, at
//! both ends, otherwise. Each transition makes a node at `a` in its cluster and one at `b` in
//! the other, joined by an inter edge costed with [`edge_cost`]. A cell is one node however
//! many transitions it ends (a cluster corner can end one per border). Intra edges come from
//! one in-cluster [`dijkstra_from`] per node and keep their cell paths.
//!
//! **Cost bound (R136).** A found route costs at most the optimal route plus `16 × c_max` per
//! cluster border the optimal route crosses (a diagonal step through a cluster corner crosses two),
//! `c_max` being the costliest passable cell. A crossing pair of the optimal route lies in a run
//! whose nearest transition is at most 7 pairs away (with 16-cell clusters; in general
//! `(cluster_cells − 1) / 2`, the middle of a full run with transitions at its ends): walk at most
//! 7 steps along the border on one side, cross, walk back at most 7 on the other, and a diagonal
//! crossing first steps through its side cell (passable, as no corner is cut). That is at most 16
//! steps of at most `c_max` each. Each piece between two crossings lies in one cluster, so an intra
//! edge (an in-cluster Dijkstra) or the start or goal link costs no more than it, and the abstract
//! A* (octile is consistent: every edge is a real route) finds the cheapest abstract route. A route
//! inside one cluster stands only if no route leaving the cluster can be cheaper (see [`plan_path`]
//! step 4).
//!
//! **The graph.** Nodes are kept in a `Vec` sorted by `(cluster, y, x)`; each node's
//! adjacency list is sorted by target node. Everything is a pure function of the grid, so a
//! graph invalidated in place equals a graph built from scratch (`invalidate_equals_full_rebuild`),
//! and a cache never changes an outcome.
//!
//! **Work.** [`plan_path`] adds every cell or abstract node it settles, and every
//! [`nearest_passable`] probe, to [`PathWork::nodes_expanded`], and every line-of-sight cell
//! to [`PathWork::los_cells`]; the sum is the request's cold cost ([`PathWork::charge`]).
//! Building the graphs is not charged to any request.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use bevy_ecs::resource::Resource;
use fl_math::FxVec2;
use serde::{Deserialize, Serialize};

use super::PathRules;
use super::astar::{
    CellXY, DistanceField, SearchStats, astar, cell_centre, dijkstra_from, edge_cost,
    nearest_passable, octile,
};
use super::grid::{MoveProfile, NavGrid};
use super::los::{line_clear, smooth};
use crate::terrain::CellRect;

/// A run of passable cell pairs at least this long gets two transitions, at both ends.
const LONG_RUN: usize = 6;

/// "No parent" in the abstract search.
const NONE: u32 = u32::MAX;

/// A cluster of an [`HpaGraph`]: `cy × clusters_x + cx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClusterId(pub u32);

/// One transition across a border: `a` in the west or south cluster, `b` in the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Transition {
    a: CellXY,
    b: CellXY,
    /// [`edge_cost`] of the step `a → b`.
    cost: u32,
}

/// An intra edge to the cluster's entrance `to` (a local index), with its cell path.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Intra {
    to: u32,
    cost: u32,
    /// From the edge's node to `to`, both included.
    path: Vec<CellXY>,
}

/// The per-cluster data the graph is assembled from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Cluster {
    /// The transitions on the border with the cluster to the east (+x); none in the last
    /// column.
    east: Vec<Transition>,
    /// The transitions on the border with the cluster to the north (+y); none in the last row.
    north: Vec<Transition>,
    /// The node cells, sorted by `(y, x)`, each once.
    entrances: Vec<CellXY>,
    /// Per entrance, its intra edges, sorted by target.
    intra: Vec<Vec<Intra>>,
}

/// A node of the abstract graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Node {
    cluster: u32,
    cell: CellXY,
}

/// An abstract edge to node `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Edge {
    to: u32,
    cost: u32,
}

/// Which border of a cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Border {
    East,
    North,
}

/// The HPA* graph of one movement profile (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HpaGraph {
    profile: MoveProfile,
    cluster_cells: u32,
    /// The grid size the graph was built for, in cells.
    cells: (u32, u32),
    /// The cluster counts along x and y.
    clusters: (u32, u32),
    data: Vec<Cluster>,
    /// Sorted by `(cluster, y, x)`.
    nodes: Vec<Node>,
    /// Per cluster, the index of its first node; one more entry holds the node count.
    first: Vec<u32>,
    /// Per node, its edges sorted by target.
    adjacency: Vec<Vec<Edge>>,
}

/// `(y, x)`: the order of cells within a cluster.
fn yx(c: CellXY) -> (u32, u32) {
    (c.1, c.0)
}

fn passable(nav: &NavGrid, p: MoveProfile, c: CellXY) -> bool {
    nav.passable(p, i64::from(c.0), i64::from(c.1))
}

/// The cells of `rect`, row by row from the south.
fn rect_cells(rect: CellRect) -> impl Iterator<Item = CellXY> {
    (rect.min.1..=rect.max.1).flat_map(move |y| (rect.min.0..=rect.max.0).map(move |x| (x, y)))
}

/// The transitions of one border, given its cell pairs in order along it.
fn transitions(nav: &NavGrid, p: MoveProfile, pairs: &[(CellXY, CellXY)]) -> Vec<Transition> {
    let open: Vec<bool> = pairs
        .iter()
        .map(|&(a, b)| passable(nav, p, a) && passable(nav, p, b))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if !open[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < pairs.len() && open[i] {
            i += 1;
        }
        let len = i - start;
        let picks = if len < LONG_RUN {
            [Some(start + (len - 1) / 2), None]
        } else {
            [Some(start), Some(start + len - 1)]
        };
        for k in picks.into_iter().flatten() {
            let (a, b) = pairs[k];
            out.push(Transition {
                a,
                b,
                cost: edge_cost(nav, p.class, a, b),
            });
        }
    }
    out
}

impl HpaGraph {
    /// The graph of profile `p` over `nav` with `cluster_cells`-square clusters.
    fn build(nav: &NavGrid, profile: MoveProfile, cluster_cells: u32) -> Self {
        let cluster_cells = cluster_cells.max(1);
        let cells = nav.cells();
        let clusters = (
            cells.0.div_ceil(cluster_cells),
            cells.1.div_ceil(cluster_cells),
        );
        let count = clusters.0 as usize * clusters.1 as usize;
        let mut graph = HpaGraph {
            profile,
            cluster_cells,
            cells,
            clusters,
            data: vec![Cluster::default(); count],
            nodes: Vec::new(),
            first: Vec::new(),
            adjacency: Vec::new(),
        };
        for i in 0..count {
            graph.compute_border(nav, i, Border::East);
            graph.compute_border(nav, i, Border::North);
        }
        for i in 0..count {
            graph.build_cluster(nav, i);
        }
        graph.link();
        graph
    }

    fn count(&self) -> usize {
        self.data.len()
    }

    /// `(cx, cy)` of cluster index `i`.
    fn coords(&self, i: usize) -> (u32, u32) {
        let i = u32::try_from(i).expect("a cluster index fits u32");
        (i % self.clusters.0, i / self.clusters.0)
    }

    fn index(&self, cx: u32, cy: u32) -> usize {
        cy as usize * self.clusters.0 as usize + cx as usize
    }

    /// The cluster holding cell `c` (clamped into the grid).
    pub fn cluster_of(&self, c: CellXY) -> ClusterId {
        let cx = (c.0 / self.cluster_cells).min(self.clusters.0.saturating_sub(1));
        let cy = (c.1 / self.cluster_cells).min(self.clusters.1.saturating_sub(1));
        ClusterId(cy * self.clusters.0 + cx)
    }

    /// The cells of cluster `id`, clipped to the grid.
    ///
    /// # Panics
    /// If `id` is not a cluster of the graph.
    pub fn cluster_rect(&self, id: ClusterId) -> CellRect {
        let i = id.0 as usize;
        assert!(i < self.count(), "{id:?} is not a cluster of the graph");
        let (cx, cy) = self.coords(i);
        let cc = self.cluster_cells;
        CellRect {
            min: (cx * cc, cy * cc),
            max: (
                (cx * cc + cc - 1).min(self.cells.0 - 1),
                (cy * cc + cc - 1).min(self.cells.1 - 1),
            ),
        }
    }

    /// The node cells of cluster `id`, sorted by `(y, x)`.
    ///
    /// # Panics
    /// If `id` is not a cluster of the graph.
    pub fn entrances(&self, id: ClusterId) -> &[CellXY] {
        &self.data[id.0 as usize].entrances
    }

    /// How many abstract nodes the graph has.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The cluster counts `(clusters_x, clusters_y)`.
    pub fn clusters(&self) -> (u32, u32) {
        self.clusters
    }

    /// The profile the graph is for.
    pub fn profile(&self) -> MoveProfile {
        self.profile
    }

    /// The grid size the graph was built for, in cells.
    pub(crate) fn grid_cells(&self) -> (u32, u32) {
        self.cells
    }

    /// The node indices of cluster `id` (none for an id the graph has no cluster for).
    pub(crate) fn cluster_nodes(&self, id: ClusterId) -> std::ops::Range<usize> {
        let i = id.0 as usize;
        if i < self.count() {
            self.node_range(i)
        } else {
            0..0
        }
    }

    /// The cell of node `v`.
    pub(crate) fn node_cell(&self, v: usize) -> CellXY {
        self.nodes[v].cell
    }

    /// The cluster of node `v`.
    pub(crate) fn node_cluster(&self, v: usize) -> ClusterId {
        ClusterId(self.nodes[v].cluster)
    }

    /// The abstract edges of node `v`, `(target node, cost)`, sorted by target. Every edge
    /// has its reverse with the same cost ([`edge_cost`] and the corner rule are symmetric).
    pub(crate) fn edges(&self, v: usize) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.adjacency[v].iter().map(|e| (e.to as usize, e.cost))
    }

    /// Recompute the transitions on cluster `i`'s `border`.
    fn compute_border(&mut self, nav: &NavGrid, i: usize, border: Border) {
        let (cx, cy) = self.coords(i);
        let rect = self.cluster_rect(ClusterId(cx + cy * self.clusters.0));
        let pairs: Vec<(CellXY, CellXY)> = match border {
            Border::East if cx + 1 < self.clusters.0 => {
                let x = rect.max.0;
                (rect.min.1..=rect.max.1)
                    .map(|y| ((x, y), (x + 1, y)))
                    .collect()
            }
            Border::North if cy + 1 < self.clusters.1 => {
                let y = rect.max.1;
                (rect.min.0..=rect.max.0)
                    .map(|x| ((x, y), (x, y + 1)))
                    .collect()
            }
            _ => Vec::new(),
        };
        let found = transitions(nav, self.profile, &pairs);
        match border {
            Border::East => self.data[i].east = found,
            Border::North => self.data[i].north = found,
        }
    }

    /// Recompute cluster `i`'s entrances, from the transitions of its four borders, and its
    /// intra edges, one in-cluster Dijkstra per entrance.
    fn build_cluster(&mut self, nav: &NavGrid, i: usize) {
        let (cx, cy) = self.coords(i);
        let _span =
            tracing::info_span!("hpa_cluster_build", cx, cy, size = self.profile.size).entered();
        let mut entrances: Vec<CellXY> = Vec::new();
        let here = &self.data[i];
        entrances.extend(here.east.iter().chain(&here.north).map(|t| t.a));
        if cx > 0 {
            entrances.extend(self.data[i - 1].east.iter().map(|t| t.b));
        }
        if cy > 0 {
            let south = i - self.clusters.0 as usize;
            entrances.extend(self.data[south].north.iter().map(|t| t.b));
        }
        entrances.sort_unstable_by_key(|&c| yx(c));
        entrances.dedup();

        let rect = self.cluster_rect(ClusterId(cx + cy * self.clusters.0));
        let mut stats = SearchStats::default();
        let intra = entrances
            .iter()
            .enumerate()
            .map(|(k, &from)| {
                let field = dijkstra_from(nav, self.profile, from, rect, &mut stats);
                entrances
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != k)
                    .filter_map(|(j, &to)| {
                        Some(Intra {
                            to: u32::try_from(j).expect("a cluster has few entrances"),
                            cost: field.cost(to)?,
                            path: field.path_to(to)?,
                        })
                    })
                    .collect()
            })
            .collect();
        self.data[i].entrances = entrances;
        self.data[i].intra = intra;
    }

    /// The node index of entrance `cell` of cluster `i`.
    fn node_at(&self, i: usize, cell: CellXY) -> u32 {
        let local = self.data[i]
            .entrances
            .binary_search_by_key(&yx(cell), |&c| yx(c))
            .expect("a transition's cells are entrances");
        self.first[i] + u32::try_from(local).expect("a cluster has few entrances")
    }

    /// Assemble the node list and the adjacency lists from the per-cluster data.
    fn link(&mut self) {
        self.nodes.clear();
        self.first.clear();
        for (i, cluster) in self.data.iter().enumerate() {
            let at = u32::try_from(self.nodes.len()).expect("node indices fit u32");
            self.first.push(at);
            let id = u32::try_from(i).expect("a cluster index fits u32");
            self.nodes.extend(
                cluster
                    .entrances
                    .iter()
                    .map(|&cell| Node { cluster: id, cell }),
            );
        }
        self.first
            .push(u32::try_from(self.nodes.len()).expect("node indices fit u32"));

        let mut adjacency = vec![Vec::new(); self.nodes.len()];
        for (i, cluster) in self.data.iter().enumerate() {
            let base = self.first[i] as usize;
            for (k, edges) in cluster.intra.iter().enumerate() {
                adjacency[base + k].extend(edges.iter().map(|e| Edge {
                    to: self.first[i] + e.to,
                    cost: e.cost,
                }));
            }
            let across = [
                (&cluster.east, i + 1),
                (&cluster.north, i + self.clusters.0 as usize),
            ];
            for (list, other) in across {
                for t in list {
                    let (a, b) = (self.node_at(i, t.a), self.node_at(other, t.b));
                    adjacency[a as usize].push(Edge {
                        to: b,
                        cost: t.cost,
                    });
                    adjacency[b as usize].push(Edge {
                        to: a,
                        cost: t.cost,
                    });
                }
            }
        }
        for list in &mut adjacency {
            list.sort_by_key(|e| e.to);
        }
        self.adjacency = adjacency;
    }

    /// The node indices of cluster `i`.
    fn node_range(&self, i: usize) -> std::ops::Range<usize> {
        self.first[i] as usize..self.first[i + 1] as usize
    }

    /// Bring the graph up to date after the cells of `rect` changed (see
    /// [`HpaGraphs::invalidate`]). Returns the rebuilt clusters, sorted.
    fn invalidate(&mut self, nav: &NavGrid, rect: CellRect) -> Vec<ClusterId> {
        if self.cells != nav.cells() {
            *self = HpaGraph::build(nav, self.profile, self.cluster_cells);
            let count = u32::try_from(self.count()).expect("a cluster index fits u32");
            return (0..count).map(ClusterId).collect();
        }
        let (w, h) = self.cells;
        if w == 0 || h == 0 {
            return Vec::new();
        }
        // Passability for this profile reads the clearance `back` cells toward −x/−y.
        let back = u32::from(self.profile.size.max(1) - 1) / 2;
        let (last_x, last_y) = (w - 1, h - 1);
        let min = (
            rect.min.0.min(last_x).saturating_sub(1),
            rect.min.1.min(last_y).saturating_sub(1),
        );
        let max = (
            rect.max.0.saturating_add(back + 1).min(last_x),
            rect.max.1.saturating_add(back + 1).min(last_y),
        );
        let cc = self.cluster_cells;
        let mut touched = Vec::new();
        for cy in min.1 / cc..=max.1 / cc {
            for cx in min.0 / cc..=max.0 / cc {
                touched.push((cx, cy));
            }
        }

        let mut borders = BTreeSet::new();
        let mut affected = BTreeSet::new();
        for &(cx, cy) in &touched {
            let i = self.index(cx, cy);
            borders.insert((i, Border::East));
            borders.insert((i, Border::North));
            affected.insert(i);
            if cx > 0 {
                borders.insert((i - 1, Border::East));
                affected.insert(i - 1);
            }
            if cy > 0 {
                let south = self.index(cx, cy - 1);
                borders.insert((south, Border::North));
                affected.insert(south);
            }
            if cx + 1 < self.clusters.0 {
                affected.insert(i + 1);
            }
            if cy + 1 < self.clusters.1 {
                affected.insert(self.index(cx, cy + 1));
            }
        }
        for (i, border) in borders {
            self.compute_border(nav, i, border);
        }
        for i in affected {
            self.build_cluster(nav, i);
        }
        self.link();
        touched
            .into_iter()
            .map(|(cx, cy)| ClusterId(cy * self.clusters.0 + cx))
            .collect()
    }

    /// Append to `cells` (which ends at node `nodes[0]`'s cell) the refined path along the
    /// abstract nodes `nodes`.
    fn refine(&self, cells: &mut Vec<CellXY>, nodes: &[usize]) {
        for pair in nodes.windows(2) {
            let (u, v) = (self.nodes[pair[0]], self.nodes[pair[1]]);
            if u.cluster == v.cluster {
                let i = u.cluster as usize;
                let local = |n: usize| {
                    u32::try_from(n - self.first[i] as usize).expect("a local index fits u32")
                };
                let edges = &self.data[i].intra[local(pair[0]) as usize];
                let at = edges
                    .binary_search_by_key(&local(pair[1]), |e| e.to)
                    .expect("an abstract edge in a cluster is an intra edge");
                cells.extend_from_slice(&edges[at].path[1..]);
            } else {
                cells.push(v.cell);
            }
        }
    }

    /// A lower bound on the cost of any route from `start` to `goal` (both in cluster `i`) that
    /// leaves the cluster: the least `octile(start, o) + octile(o, goal)` over the cells `o` just
    /// outside it, as such a route enters one of them first and [`octile`] never overestimates.
    /// `u32::MAX` for a cluster that is the whole grid.
    fn leave_bound(&self, i: usize, start: CellXY, goal: CellXY) -> u32 {
        let id = ClusterId(u32::try_from(i).expect("a cluster index fits u32"));
        let rect = self.cluster_rect(id);
        let (w, h) = self.cells;
        let xs = rect.min.0.saturating_sub(1)..=(rect.max.0 + 1).min(w - 1);
        let ys = rect.min.1.saturating_sub(1)..=(rect.max.1 + 1).min(h - 1);
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| x < rect.min.0 || x > rect.max.0 || y < rect.min.1 || y > rect.max.1)
            .map(|o| octile(start, o).saturating_add(octile(o, goal)))
            .min()
            .unwrap_or(u32::MAX)
    }

    /// The cell route from `start` to `goal` (both passable). When they share a cluster,
    /// in-cluster A*; it stands if no route leaving the cluster can be cheaper
    /// ([`Self::leave_bound`]), else the abstract search's route replaces it if strictly
    /// cheaper. Otherwise (or if in-cluster A* finds nothing) the abstract search; with no
    /// route, the route to the reached node or cell nearest the goal.
    fn route(&self, nav: &NavGrid, start: CellXY, goal: CellXY, stats: &mut SearchStats) -> Route {
        let p = self.profile;
        let (sc, gc) = (
            self.cluster_of(start).0 as usize,
            self.cluster_of(goal).0 as usize,
        );
        let mut local = None;
        if sc == gc
            && let Some((cells, cost)) = astar(
                nav,
                p,
                start,
                goal,
                self.cluster_rect(ClusterId(sc as u32)),
                stats,
            )
        {
            let route = Route {
                cells,
                cost,
                found: true,
            };
            if cost <= self.leave_bound(sc, start, goal) {
                return route;
            }
            local = Some(route);
        }
        let srect = self.cluster_rect(ClusterId(sc as u32));
        let sfield = dijkstra_from(nav, p, start, srect, stats);
        let gfield = dijkstra_from(nav, p, goal, self.cluster_rect(ClusterId(gc as u32)), stats);
        // With a local route, nothing costing as much can replace it (ties keep it).
        let limit = local.as_ref().map_or(u32::MAX, |r: &Route| r.cost);
        let search = self.search(start, goal, sc, gc, &sfield, &gfield, limit, stats);
        let n = self.nodes.len();
        let target = n + 1;

        if search.closed[target] {
            let chain = search.chain(target);
            let nodes = &chain[1..chain.len() - 1];
            let (first, last) = (nodes[0], nodes[nodes.len() - 1]);
            let mut cells = sfield
                .path_to(self.nodes[first].cell)
                .expect("the start reached its first node");
            self.refine(&mut cells, nodes);
            let mut back = gfield
                .path_to(self.nodes[last].cell)
                .expect("the goal reached its last node");
            back.reverse();
            cells.extend_from_slice(&back[1..]);
            let cost = search.g[target];
            return match local {
                Some(local) if local.cost <= cost => local,
                _ => Route {
                    cells,
                    cost,
                    found: true,
                },
            };
        }
        if let Some(local) = local {
            return local;
        }

        // No route: the settled node or start-cluster cell nearest the goal, ties by node
        // index (nodes first), then (y, x).
        let nodes = (0..n)
            .filter(|&v| search.closed[v])
            .map(|v| ((octile(self.nodes[v].cell, goal), 0, v, 0, 0), Some(v)));
        let cells = rect_cells(srect)
            .filter(|&c| sfield.cost(c).is_some())
            .map(|c| ((octile(c, goal), 1, 0, c.1, c.0), None));
        let (key, node) = nodes
            .chain(cells)
            .min_by_key(|&(key, _)| key)
            .expect("the start cell is reached");
        let Some(v) = node else {
            let cell = (key.4, key.3);
            return Route {
                cells: sfield.path_to(cell).expect("a reached cell"),
                cost: sfield.cost(cell).expect("a reached cell"),
                found: false,
            };
        };
        let chain = search.chain(v);
        let nodes = &chain[1..];
        let mut cells = sfield
            .path_to(self.nodes[nodes[0]].cell)
            .expect("the start reached its first node");
        self.refine(&mut cells, nodes);
        // Within the node's cluster, go on to the reached cell nearest the goal.
        let at = self.nodes[v];
        let rect = self.cluster_rect(ClusterId(at.cluster));
        let field = dijkstra_from(nav, p, at.cell, rect, stats);
        let end = rect_cells(rect)
            .filter(|&c| field.cost(c).is_some())
            .min_by_key(|&c| (octile(c, goal), c.1, c.0))
            .expect("the node's own cell is reached");
        let tail = field.path_to(end).expect("a reached cell");
        cells.extend_from_slice(&tail[1..]);
        Route {
            cells,
            cost: search.g[v].saturating_add(field.cost(end).expect("a reached cell")),
            found: false,
        }
    }

    /// A* over the abstract nodes plus the start (index `n`) and the goal (`n + 1`): the start
    /// links to the start cluster's nodes `sfield` reaches, the goal cluster's nodes that
    /// `gfield` reaches link to the goal. The [`octile`] heuristic; pops the smallest `(f, h,
    /// node index)`; a parent changes only on a strictly lower g. Stops, before settling it,
    /// at the first entry whose f is at least `limit`: as the heuristic is consistent, no route
    /// cheaper than `limit` remains. Adds every node settled to `stats.expanded`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the endpoints, their clusters and fields, and the cost limit"
    )]
    fn search(
        &self,
        start: CellXY,
        goal: CellXY,
        sc: usize,
        gc: usize,
        sfield: &DistanceField,
        gfield: &DistanceField,
        limit: u32,
        stats: &mut SearchStats,
    ) -> Search {
        let n = self.nodes.len();
        let (source, target) = (n, n + 1);
        let cell = |v: usize| match v {
            v if v < n => self.nodes[v].cell,
            v if v == source => start,
            _ => goal,
        };
        let h = |v: usize| {
            if v == target {
                0
            } else {
                octile(cell(v), goal)
            }
        };
        let mut search = Search {
            g: vec![u32::MAX; n + 2],
            parent: vec![NONE; n + 2],
            closed: vec![false; n + 2],
        };
        let goal_nodes = self.node_range(gc);
        let mut open = BinaryHeap::new();
        search.g[source] = 0;
        open.push(Reverse((h(source), h(source), source)));
        while let Some(Reverse((f, _, v))) = open.pop() {
            if f >= limit {
                break;
            }
            if search.closed[v] {
                // A stale entry: the node was settled from a cheaper one.
                continue;
            }
            search.closed[v] = true;
            stats.expanded = stats.expanded.saturating_add(1);
            if v == target {
                break;
            }
            let gv = search.g[v];
            let mut relax = |u: usize, w: u32| {
                if search.closed[u] {
                    return;
                }
                let ng = gv.saturating_add(w);
                if ng < search.g[u] {
                    search.g[u] = ng;
                    search.parent[u] = u32::try_from(v).expect("node indices fit u32");
                    let hu = h(u);
                    open.push(Reverse((ng.saturating_add(hu), hu, u)));
                }
            };
            if v == source {
                for u in self.node_range(sc) {
                    if let Some(d) = sfield.cost(self.nodes[u].cell) {
                        relax(u, d);
                    }
                }
            } else {
                for e in &self.adjacency[v] {
                    relax(e.to as usize, e.cost);
                }
                if goal_nodes.contains(&v)
                    && let Some(d) = gfield.cost(self.nodes[v].cell)
                {
                    relax(target, d);
                }
            }
        }
        search
    }
}

/// The state of an abstract search, per node (the start and goal last).
struct Search {
    g: Vec<u32>,
    parent: Vec<u32>,
    closed: Vec<bool>,
}

impl Search {
    /// The nodes from the start to `v`, both included.
    fn chain(&self, v: usize) -> Vec<usize> {
        let mut chain = vec![v];
        let mut at = self.parent[v];
        while at != NONE {
            chain.push(at as usize);
            at = self.parent[at as usize];
        }
        chain.reverse();
        chain
    }
}

/// A cell route and its cost; `found` is false for a route to the nearest reachable point.
struct Route {
    cells: Vec<CellXY>,
    cost: u32,
    found: bool,
}

/// The HPA* graph of every profile of the [`NavGrid`]. `Derived` `"core.nav_hpa"`, built by
/// core's rebuild after the grid ([`rebuild_nav`](super::rebuild_nav)) and invalidated in
/// place when the terrain changes ([`on_terrain_changed`](super::on_terrain_changed)).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct HpaGraphs {
    cluster_cells: u32,
    graphs: BTreeMap<MoveProfile, HpaGraph>,
}

impl HpaGraphs {
    /// One [`HpaGraph`] per profile of `nav` (`nav.profiles()`), over `cluster_cells`-square
    /// clusters (`Path.cluster_cells`; at least 1).
    pub fn build(nav: &NavGrid, cluster_cells: u32) -> Self {
        let cluster_cells = cluster_cells.max(1);
        HpaGraphs {
            cluster_cells,
            graphs: nav
                .profiles()
                .iter()
                .map(|&p| (p, HpaGraph::build(nav, p, cluster_cells)))
                .collect(),
        }
    }

    /// The graph of profile `p`; `None` if the grid has no such profile.
    pub fn graph(&self, p: MoveProfile) -> Option<&HpaGraph> {
        self.graphs.get(&p)
    }

    /// The side of a cluster, in cells.
    pub fn cluster_cells(&self) -> u32 {
        self.cluster_cells
    }

    /// The cells of cluster `id` (every graph cuts the grid into the same clusters); `None`
    /// without a graph or for an id the grid has no cluster for.
    pub fn cluster_rect(&self, id: ClusterId) -> Option<CellRect> {
        let graph = self.graphs.values().next()?;
        (id.0 < graph.clusters.0 * graph.clusters.1).then(|| graph.cluster_rect(id))
    }

    /// Bring every graph up to date after the navigation data of `rect` changed, `nav` being
    /// the updated grid: `rect` is the rect [`NavGrid::update_rect`] recomputed.
    ///
    /// Per graph, `rect` is first extended toward +x/+y by the `(size − 1) / 2` cells a
    /// profile's passability reads toward −x/−y ([`NavGrid::passable`]), then grown by 1 cell.
    /// The clusters it intersects are rebuilt: the transitions on all four of their borders,
    /// their entrances and intra edges. Their 4-neighbours get their entrances and intra
    /// edges recomputed, as the shared borders may have moved them. A graph whose grid size
    /// changed is rebuilt whole. Returns the rebuilt clusters of every graph, sorted and
    /// unique; the graphs then equal [`HpaGraphs::build`] of `nav`.
    pub fn invalidate(&mut self, nav: &NavGrid, rect: CellRect) -> Vec<ClusterId> {
        let mut rebuilt = BTreeSet::new();
        for graph in self.graphs.values_mut() {
            rebuilt.extend(graph.invalidate(nav, rect));
        }
        rebuilt.into_iter().collect()
    }
}

/// How a [`PathPlan`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    /// At the goal (or the passable cell it snapped to).
    Found,
    /// At the reachable point nearest the goal: no route reaches it.
    Partial,
    /// Nowhere: the goal or the start has no passable cell within `Path.snap_radius_cells`.
    Unreachable,
}

/// A planned path: the corners to drive through after the start, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathPlan {
    /// Empty when `Unreachable`.
    pub corners: Vec<FxVec2>,
    /// The cost of the cell path the corners were pulled from; for a direct line (no search)
    /// the [`octile`] distance of its end cells, which never exceeds the cheapest route.
    pub cost: u32,
    pub status: PlanStatus,
}

impl PathPlan {
    fn unreachable() -> Self {
        PathPlan {
            corners: Vec::new(),
            cost: 0,
            status: PlanStatus::Unreachable,
        }
    }
}

/// The work path planning did, the measure the per-tick path budget charges.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathWork {
    /// Cells and abstract nodes settled, and cells probed for a snap.
    pub nodes_expanded: u32,
    /// Cells line-of-sight tests walked.
    pub los_cells: u32,
}

impl PathWork {
    /// `nodes_expanded + los_cells`: the request's cold cost (ADR-0015).
    pub fn charge(&self) -> u32 {
        self.nodes_expanded.saturating_add(self.los_cells)
    }
}

/// A path endpoint: its cell, the point the path starts or ends at, and whether it snapped.
struct Endpoint {
    cell: CellXY,
    point: FxVec2,
    snapped: bool,
}

/// The cell holding `pos`, or `None` off the map (no clamp, as [`line_clear`] maps points).
fn cell_holding(nav: &NavGrid, pos: FxVec2) -> Option<CellXY> {
    let cell = nav.cell_m().raw().max(1);
    let (w, h) = nav.cells();
    let axis = |v: i64, count: u32| {
        u32::try_from(v.div_euclid(cell))
            .ok()
            .filter(|&i| i < count)
    };
    Some((axis(pos.x.raw(), w)?, axis(pos.y.raw(), h)?))
}

/// `pos` itself if it lies in a cell passable for `p`; else the centre of the passable cell
/// nearest its (clamped) cell within `radius`, with the probes charged; `None` if none.
fn endpoint(
    nav: &NavGrid,
    p: MoveProfile,
    pos: FxVec2,
    radius: u32,
    work: &mut PathWork,
) -> Option<Endpoint> {
    if let Some(cell) = cell_holding(nav, pos)
        && passable(nav, p, cell)
    {
        return Some(Endpoint {
            cell,
            point: pos,
            snapped: false,
        });
    }
    let mut probed = 0;
    let found = nearest_passable(nav, p, nav.cell_of(pos), radius, &mut probed);
    work.nodes_expanded = work.nodes_expanded.saturating_add(probed);
    found.map(|cell| Endpoint {
        cell,
        point: cell_centre(nav, cell),
        snapped: true,
    })
}

/// Plan a path for profile `p` from `from` to `to` on `nav` with the HPA* graphs `hpa`, adding
/// the work done to `work`:
///
/// 1. A goal outside a passable cell snaps to the centre of the [`nearest_passable`] cell
///    within `rules.snap_radius_cells`; if there is none, the plan is `Unreachable`.
/// 2. A start outside a passable cell snaps the same way. The snapped cell's centre is then
///    the first corner, and the rest of the plan starts from it, so [`smooth`]'s
///    preconditions hold (its start lies in the first cell of the path; R95).
/// 3. If [`line_clear`] from the start to the goal, the corners end with just the goal:
///    `Found`.
/// 4. Start and goal in one cluster: in-cluster [`astar`], then [`smooth`], if its cost is at
///    most the least `octile(start, o) + octile(o, goal)` over the cells `o` just outside the
///    cluster (a lower bound on any route leaving it). If not, step 5 runs too, and its route
///    replaces the in-cluster one if strictly cheaper (R136).
/// 5. Otherwise (or if step 4 found nothing) the start and goal join the abstract graph by
///    in-cluster [`dijkstra_from`]; A* over the abstract nodes (octile heuristic, `(f, h,
///    node index)` ties) finds the route, refined by concatenating the stored cell paths,
///    then smoothed: `Found`.
/// 6. With no route, the plan goes to the settled abstract node or start-cluster cell with
///    the smallest octile distance to the goal (ties by node index, nodes first, then `(y,
///    x)`); from a node it goes on, inside the node's cluster, to the cell the node reaches
///    nearest the goal (smallest `(octile, y, x)`), and ends at that cell's centre:
///    `Partial`.
///
/// A profile without a graph is `Unreachable`. Runs inside the `hpa_search` span.
pub fn plan_path(
    nav: &NavGrid,
    hpa: &HpaGraphs,
    p: MoveProfile,
    from: FxVec2,
    to: FxVec2,
    rules: &PathRules,
    work: &mut PathWork,
) -> PathPlan {
    let _span = tracing::info_span!("hpa_search", class = ?p.class, size = p.size).entered();
    let plan = plan(nav, hpa, p, from, to, rules, work);
    tracing::trace!(
        status = ?plan.status,
        corners = plan.corners.len(),
        cost = plan.cost,
        nodes_expanded = work.nodes_expanded,
        los_cells = work.los_cells,
        "path planned"
    );
    plan
}

fn plan(
    nav: &NavGrid,
    hpa: &HpaGraphs,
    p: MoveProfile,
    from: FxVec2,
    to: FxVec2,
    rules: &PathRules,
    work: &mut PathWork,
) -> PathPlan {
    let Some(graph) = hpa.graph(p) else {
        return PathPlan::unreachable();
    };
    let radius = rules.snap_radius_cells;
    let Some(goal) = endpoint(nav, p, to, radius, work) else {
        return PathPlan::unreachable();
    };
    let Some(start) = endpoint(nav, p, from, radius, work) else {
        return PathPlan::unreachable();
    };
    let mut corners = Vec::new();
    if start.snapped {
        corners.push(start.point);
    }
    if line_clear(nav, p, start.point, goal.point, &mut work.los_cells) {
        corners.push(goal.point);
        return PathPlan {
            corners,
            cost: octile(start.cell, goal.cell),
            status: PlanStatus::Found,
        };
    }
    let mut stats = SearchStats::default();
    let route = graph.route(nav, start.cell, goal.cell, &mut stats);
    work.nodes_expanded = work.nodes_expanded.saturating_add(stats.expanded);
    let end = if route.found {
        goal.point
    } else {
        cell_centre(nav, *route.cells.last().expect("a route has cells"))
    };
    corners.extend(smooth(
        nav,
        p,
        start.point,
        &route.cells,
        end,
        &mut work.los_cells,
    ));
    PathPlan {
        corners,
        cost: route.cost,
        status: if route.found {
            PlanStatus::Found
        } else {
            PlanStatus::Partial
        },
    }
}
