//! Flow fields for group moves (spec §5.7, ADR-0015): one field per [`FlowKey`] (the
//! destination cluster and the movement profile), kept in the `Derived` [`FlowCache`]
//! (`"core.path_flow_cache"`).
//!
//! **The abstract integration.** A Dijkstra over the profile's HPA* graph ([`HpaGraph`]) from
//! every node of the destination cluster at cost 0. It pops the smallest `(cost, node index)`;
//! a cost changes only on a strictly lower value. Each node's cost is its cost to the
//! destination cluster: the graph's edges are symmetric, so the search from the destination
//! is also the search toward it. Its settled-node count is a group request's cold cost
//! ([`FlowCache::cold_cost`]).
//!
//! **Tiles.** The field inside one cluster is built lazily, the first time a read needs it:
//! a [`dijkstra_seeded`] inside the cluster, seeded at each node `e` of the cluster with its
//! cheapest exit, `min(cost(q) + edge cost)` over the edges from `e` to a node `q` of another
//! cluster (ties to the lower node index), which remembers `q`. A cell's direction is the cell
//! it was reached from ([`DistanceField::next_toward_source`]), or for a seed nothing cheaper
//! reached, its exit `q` across the border. The destination cluster needs no tile: every
//! passable cell in it is [`FlowDir::Arrived`] (cost 0).
//!
//! Inside a tile the costs are the abstract costs (a node's tile cost is its integration
//! cost), so every step of a direction lowers the cost to the destination by at least one
//! road step: following directions never revisits a cell and ends in the destination cluster.
//! A cell has a direction exactly when a full-map Dijkstra from the destination cluster's
//! passable cells reaches it (`flow_directions_reach_destination`).
//!
//! **Outcome-neutral (ADR-0015, R84).** Every entry is a pure function of the navigation data,
//! and a read never observes a miss: [`FlowCache::direction`] builds a missing integration or
//! tile before it answers, so `None` means only "no route from this cell". Eviction and
//! [`RunConfig::flow_cache_cold`](crate::RunConfig::flow_cache_cold) therefore change only the
//! work done, which [`FlowWork`] counts
//! (`"flowfield.*"` counters, observe-only, R86), never an outcome. Builds run inside the
//! `flowfield_build` span.
//!
//! **Eviction at the tick boundary (R124).** A read never evicts. At the start of a tick
//! ([`FlowCache::begin_tick`], run by the core system [`begin_flow_tick`]) the least recently used entries go down to
//! `Path.flow_cache_max`, but never one read in the previous tick: those are the live set,
//! kept above the cap up to `Path.flow_cache_hard_max`. So a field that units follow is built
//! once, not once per read, however many fields are live; a key is built at most once per
//! tick even past the hard max.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut};
use fl_protocol::Tick;

use super::astar::{CellXY, DistanceField, SearchStats, dijkstra_seeded};
use super::grid::{MoveProfile, NavGrid};
use super::hpa::{ClusterId, HpaGraph, HpaGraphs};
use crate::clock::SimClock;
use crate::path::FlowKey;

/// Where a flow field sends a unit from its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowDir {
    /// Steer to this neighbouring cell next.
    Toward(CellXY),
    /// The cell lies inside the destination cluster.
    Arrived,
}

/// The work flow-field builds did: abstract nodes settled and tile cells settled. Counted as
/// `"flowfield.abstract_nodes"` and `"flowfield.cells"`; it depends on what the cache held
/// (R86), so it is observe-only and never charged to the path budget.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowWork {
    pub abstract_nodes: u32,
    pub cells: u32,
}

/// One cluster's field: the in-cluster costs and the exit of every seed.
#[derive(Clone, Debug)]
struct Tile {
    field: DistanceField,
    /// `(seed cell, exit cell across the border)`, sorted by the seed's `(y, x)`.
    exits: Vec<(CellXY, CellXY)>,
}

/// The flow field of one key.
#[derive(Clone, Debug)]
struct Entry {
    /// The tick of the entry's last read: the LRU order.
    last_used: Tick,
    /// Per abstract node, its cost to the destination cluster; `u32::MAX` if unreached.
    cost: Vec<u32>,
    /// How many nodes the integration settled: the cold cost.
    settled: u32,
    tiles: BTreeMap<ClusterId, Tile>,
}

/// The cached flow fields, by [`FlowKey`]. `Derived` `"core.path_flow_cache"`: emptied by
/// core's rebuild ([`install_nav`](super::install_nav), the last step of `rebuild_core`) and,
/// per profile, by a terrain change ([`clear_profile`](Self::clear_profile)); outcome-neutral
/// (see the module docs).
#[derive(Resource, Default, Debug)]
pub struct FlowCache {
    entries: BTreeMap<FlowKey, Entry>,
    /// The entries kept between ticks (`Path.flow_cache_max`), besides those read in the
    /// previous tick; 0 keeps every entry.
    max: u32,
    /// The most entries kept at a tick start (`Path.flow_cache_hard_max`); 0: no ceiling.
    hard_max: u32,
    /// [`RunConfig::flow_cache_cold`](crate::RunConfig::flow_cache_cold): empty the cache at
    /// the first use of every tick.
    cold: bool,
    /// The tick of the last [`begin_tick`](Self::begin_tick).
    now: Tick,
    /// Eviction's scratch list of `(last read tick, key)`, kept between ticks for its
    /// capacity: empty outside [`begin_tick`](Self::begin_tick).
    lru: Vec<(Tick, FlowKey)>,
}

/// The flow cache's tick start (R124): [`FlowCache::begin_tick`] for the tick about to run.
/// Core, in `InputSet::Housekeeping` after the Transient resets, so it runs every tick before
/// any system that reads flow fields (Think and Act).
pub(crate) fn begin_flow_tick(clock: Res<SimClock>, mut cache: ResMut<FlowCache>) {
    cache.begin_tick(clock.tick);
}

/// `(y, x)`: the order of cells.
fn yx(c: CellXY) -> (u32, u32) {
    (c.1, c.0)
}

/// The abstract integration of `graph` toward cluster `dest` (see the module docs): per node
/// its cost, and the number of nodes settled.
fn integrate(graph: &HpaGraph, dest: ClusterId) -> (Vec<u32>, u32) {
    let n = graph.node_count();
    let mut cost = vec![u32::MAX; n];
    let mut closed = vec![false; n];
    let mut open = BinaryHeap::new();
    for v in graph.cluster_nodes(dest) {
        cost[v] = 0;
        open.push(Reverse((0u32, v)));
    }
    let mut settled: u32 = 0;
    // Keys are unique per live entry: a node is pushed again only with a strictly lower cost.
    while let Some(Reverse((g, v))) = open.pop() {
        if closed[v] {
            continue;
        }
        closed[v] = true;
        settled = settled.saturating_add(1);
        for (to, w) in graph.edges(v) {
            let ng = g.saturating_add(w);
            if !closed[to] && ng < cost[to] {
                cost[to] = ng;
                open.push(Reverse((ng, to)));
            }
        }
    }
    (cost, settled)
}

/// The tile of `cluster` for `graph`'s profile, from the integration `cost` (see the module
/// docs). Adds the cells it settles to `work.cells`.
fn build_tile(
    nav: &NavGrid,
    graph: &HpaGraph,
    cost: &[u32],
    cluster: ClusterId,
    work: &mut FlowWork,
) -> Tile {
    let _span =
        tracing::info_span!("flowfield_build", kind = "tile", cluster = cluster.0).entered();
    let mut seeds: Vec<(CellXY, u32)> = Vec::new();
    let mut exits: Vec<(CellXY, CellXY)> = Vec::new();
    for v in graph.cluster_nodes(cluster) {
        let best = graph
            .edges(v)
            .filter(|&(q, _)| graph.node_cluster(q) != cluster && cost[q] != u32::MAX)
            .map(|(q, w)| (cost[q].saturating_add(w), q))
            .min();
        if let Some((exit_cost, q)) = best {
            let cell = graph.node_cell(v);
            seeds.push((cell, exit_cost));
            exits.push((cell, graph.node_cell(q)));
        }
    }
    exits.sort_unstable_by_key(|&(cell, _)| yx(cell));
    let mut stats = SearchStats::default();
    let field = dijkstra_seeded(
        nav,
        graph.profile(),
        &seeds,
        graph.cluster_rect(cluster),
        &mut stats,
    );
    work.cells = work.cells.saturating_add(stats.expanded);
    tracing::trace!(
        cells = stats.expanded,
        seeds = seeds.len(),
        "flow tile built"
    );
    Tile { field, exits }
}

impl FlowCache {
    /// An empty cache keeping `max` entries between ticks (0: every entry) besides the ones read
    /// in the previous tick, and at most `hard_max` at a tick start (0: no ceiling; it applies
    /// with `max` 0 too); emptied at every tick start when `cold`. See
    /// [`begin_tick`](Self::begin_tick).
    pub fn new(max: u32, hard_max: u32, cold: bool) -> Self {
        FlowCache {
            max,
            hard_max,
            cold,
            ..FlowCache::default()
        }
    }

    /// Whether the cache empties itself every tick
    /// ([`RunConfig::flow_cache_cold`](crate::RunConfig::flow_cache_cold)).
    pub fn is_cold(&self) -> bool {
        self.cold
    }

    /// The tick-start housekeeping (R124): set the tick reads are made in (the LRU order) and,
    /// on a new tick, evict. A cold cache is emptied, so no entry outlives the tick it was built
    /// in. Otherwise, in least-recently-used order (the smallest `(last read tick, key)`
    /// first), entries go while the cache is over `max` and the entry was not read in the
    /// previous tick, then while it is over `hard_max` whatever their last read: the fields
    /// read in the previous tick are the live set, kept above `max` up to `hard_max`.
    ///
    /// The sim calls it once per tick, in [`begin_flow_tick`] (core, `InputSet::Housekeeping`),
    /// before any flow read of the tick; the core systems that read flow fields check that it
    /// ran ([`assert_tick`](Self::assert_tick), debug builds). So eviction never runs mid-tick,
    /// and a key read in a tick stays cached for the rest of it: a key is built at most once per
    /// tick. The cache holds at most `hard_max` entries at a tick start, plus the distinct keys
    /// read in the tick until the next start. Idempotent within a tick. Public for tests that
    /// drive a cache directly; nothing outside the sim needs it.
    #[doc(hidden)]
    pub fn begin_tick(&mut self, tick: Tick) {
        if tick == self.now {
            return;
        }
        self.now = tick;
        if self.cold {
            self.entries.clear();
        } else {
            self.evict(Tick(tick.0.saturating_sub(1)));
        }
    }

    /// Debug builds: panic unless [`begin_tick`](Self::begin_tick) ran for `tick`. Every core
    /// system that reads flow fields checks this before its first read, so a reader can't run
    /// ahead of the tick-start eviction ([`begin_flow_tick`]).
    pub(crate) fn assert_tick(&self, tick: Tick) {
        debug_assert_eq!(
            self.now, tick,
            "the flow cache's tick start (begin_flow_tick) did not run for this tick"
        );
    }

    /// The entry of `key`, built (its integration) if missing; marks it used now. Never evicts
    /// ([`begin_tick`](Self::begin_tick) does).
    fn entry(&mut self, key: FlowKey, graph: &HpaGraph, work: &mut FlowWork) -> &mut Entry {
        if !self.entries.contains_key(&key) {
            let _span = tracing::info_span!(
                "flowfield_build",
                kind = "abstract",
                cluster = key.cluster.0
            )
            .entered();
            let (cost, settled) = integrate(graph, key.cluster);
            work.abstract_nodes = work.abstract_nodes.saturating_add(settled);
            tracing::trace!(settled, "flow integration built");
            self.entries.insert(
                key,
                Entry {
                    last_used: self.now,
                    cost,
                    settled,
                    tiles: BTreeMap::new(),
                },
            );
        }
        let now = self.now;
        let entry = self.entries.get_mut(&key).expect("present or just built");
        entry.last_used = now;
        entry
    }

    /// The eviction of [`begin_tick`](Self::begin_tick); `previous` is the tick before the new
    /// one. Sorts the scratch list `lru`, whose capacity is kept, so it allocates only when the
    /// cache holds more entries than ever before.
    fn evict(&mut self, previous: Tick) {
        let limit = |cap: u32| match usize::try_from(cap) {
            Ok(0) | Err(_) => usize::MAX,
            Ok(cap) => cap,
        };
        let (max, hard_max) = (limit(self.max), limit(self.hard_max));
        if self.entries.len() <= max && self.entries.len() <= hard_max {
            return;
        }
        let mut lru = std::mem::take(&mut self.lru);
        lru.extend(
            self.entries
                .iter()
                .map(|(&key, entry)| (entry.last_used, key)),
        );
        lru.sort_unstable();
        for &(last_used, key) in &lru {
            let len = self.entries.len();
            // Sorted by last read: once an entry read in the previous tick is reached, every
            // later one was read then too, so only the hard max can still evict.
            let stale_over_cap = len > max && last_used < previous;
            if !stale_over_cap && len <= hard_max {
                break;
            }
            self.entries.remove(&key);
        }
        lru.clear();
        self.lru = lru;
    }

    /// The cold cost of a group request to `key`: the number of abstract nodes its
    /// integration settles, the same whether built now or cached. Builds the integration if
    /// missing (its work added to `work`). 0 for a profile without an HPA* graph.
    pub fn cold_cost(
        &mut self,
        key: FlowKey,
        nav: &NavGrid,
        hpa: &HpaGraphs,
        work: &mut FlowWork,
    ) -> u32 {
        let Some(graph) = hpa.graph(key.profile) else {
            return 0;
        };
        debug_assert_eq!(
            nav.cells(),
            graph.grid_cells(),
            "the graph is of another grid"
        );
        self.entry(key, graph, work).settled
    }

    /// Build the tile of `cluster` for `key` if missing, and the integration before it (the
    /// work added to `work`). The destination cluster needs none. A warm-up only: reads build
    /// whatever is missing themselves (R84).
    pub fn ensure_tile(
        &mut self,
        key: FlowKey,
        cluster: ClusterId,
        nav: &NavGrid,
        hpa: &HpaGraphs,
        work: &mut FlowWork,
    ) {
        let Some(graph) = hpa.graph(key.profile) else {
            return;
        };
        self.tile(key, cluster, nav, graph, work);
    }

    /// The tile of `cluster` for `key`, built (with the integration) if missing; `None` for
    /// the destination cluster or an id the graph has no cluster for.
    fn tile(
        &mut self,
        key: FlowKey,
        cluster: ClusterId,
        nav: &NavGrid,
        graph: &HpaGraph,
        work: &mut FlowWork,
    ) -> Option<&Tile> {
        let (cx, cy) = graph.clusters();
        if cluster == key.cluster || cluster.0 >= cx * cy {
            return None;
        }
        let entry = self.entry(key, graph, work);
        if !entry.tiles.contains_key(&cluster) {
            let tile = build_tile(nav, graph, &entry.cost, cluster, work);
            entry.tiles.insert(cluster, tile);
        }
        entry.tiles.get(&cluster)
    }

    /// Where the flow field of `key` sends a unit of the key's profile standing in `cell`:
    /// [`FlowDir::Arrived`] in a passable cell of the destination cluster, else
    /// [`FlowDir::Toward`] the next cell. `None` means only that the cell has no route: it is
    /// blocked or off the map, nothing connects it to the destination, or the profile has no
    /// HPA* graph. A read never observes a miss (R84): a missing integration or tile is built
    /// first, its work added to `work` (never charged to the path budget).
    pub fn direction(
        &mut self,
        key: FlowKey,
        cell: CellXY,
        nav: &NavGrid,
        hpa: &HpaGraphs,
        work: &mut FlowWork,
    ) -> Option<FlowDir> {
        let graph = hpa.graph(key.profile)?;
        if !nav.passable(key.profile, i64::from(cell.0), i64::from(cell.1)) {
            return None;
        }
        let cluster = graph.cluster_of(cell);
        if cluster == key.cluster {
            return Some(FlowDir::Arrived);
        }
        let tile = self.tile(key, cluster, nav, graph, work)?;
        tile.field.cost(cell)?;
        if let Some(next) = tile.field.next_toward_source(cell) {
            return Some(FlowDir::Toward(next));
        }
        let at = tile
            .exits
            .binary_search_by_key(&yx(cell), |&(seed, _)| yx(seed))
            .expect("a reached cell without a parent is a seed");
        Some(FlowDir::Toward(tile.exits[at].1))
    }

    /// Drop every entry of profile `p` (its HPA* graph changed).
    pub fn clear_profile(&mut self, p: MoveProfile) {
        self.entries.retain(|key, _| key.profile != p);
    }

    /// How many entries (keys) the cache holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
