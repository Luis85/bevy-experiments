//! HPA* (ADR-0015, Task 7): 16 × 16-cell clusters per movement profile, entrances by the run
//! rule, the abstract search with refinement and string-pulling, goal and start snapping,
//! partial plans, cluster invalidation after a terrain change, and the `core.clusters` debug
//! query. The graphs are `Derived`: a cold build, the live (invalidated) graphs and the graphs
//! of a loaded save are equal and plan alike.

use std::sync::{Arc, LazyLock};

use fl_content::{ContentDb, MapDef, MoveClass};
use fl_math::{Fx, FxVec2};
use fl_protocol::DebugSetTerrainCmd;
use fl_sim::nav::{
    CellXY, ClusterId, HpaGraphs, MoveProfile, NavGrid, PathPlan, PathRules, PathWork, PlanStatus,
    SearchStats, astar, cell_centre, content_profiles, dijkstra_from, line_clear, nearest_passable,
    octile, plan_path, smooth,
};
use fl_sim::{CellRect, MovementRules, SimCounters, TerrainGrid, debug_query};
use fl_test_utils::{Scenario, ScenarioBuilder, ascii_terrain_content};
use proptest::prelude::*;
use proptest::test_runner::TestRunner;
use serde_json::{Value, json};

/// Cell side of every map here, in metres.
const CELL_M: i32 = 4;
/// `Path.cluster_cells` of the base rules.
const CLUSTER: u32 = 16;

const TANK: MoveProfile = MoveProfile {
    class: MoveClass::Tracked,
    size: 1,
};
/// `t.wide_tank`: 7 m wide on 4 m cells.
const WIDE: MoveProfile = MoveProfile {
    class: MoveClass::Tracked,
    size: 2,
};

/// The ASCII rows of a `w × h` map whose cell `(x, y)` is `f(x, y)` (row 0 is the north
/// edge, `y = h − 1`).
fn draw(w: u32, h: u32, f: impl Fn(u32, u32) -> char) -> Vec<String> {
    (0..h)
        .rev()
        .map(|y| (0..w).map(|x| f(x, y)).collect())
        .collect()
}

fn rows_of(rows: &[String]) -> Vec<&str> {
    rows.iter().map(String::as_str).collect()
}

/// A from-scratch navigation grid of `terrain` for `content`, with 4 m cells.
fn build_nav(content: &ContentDb, terrain: &TerrainGrid) -> NavGrid {
    let max = content.rules::<PathRules>().max_clearance;
    let cell = Fx::from_int(CELL_M);
    NavGrid::build(
        terrain,
        content,
        content.rules::<MovementRules>(),
        &content_profiles(content, cell, max),
        max,
        cell,
    )
}

/// An ASCII map with its navigation grid and HPA* graphs, built from scratch.
struct Map {
    content: Arc<ContentDb>,
    nav: NavGrid,
    hpa: HpaGraphs,
}

fn map_of(rows: &[String]) -> Map {
    let content = ascii_terrain_content(&rows_of(rows), CELL_M);
    let def = content.kind::<MapDef>().get("t.ascii").expect("the map");
    let terrain = TerrainGrid::from_layers(
        def.layers.as_ref().expect("decoded layers"),
        def.terrain.as_ref().expect("terrain").height_m,
    );
    let nav = build_nav(&content, &terrain);
    let hpa = HpaGraphs::build(&nav, CLUSTER);
    Map { content, nav, hpa }
}

impl Map {
    fn rules(&self) -> &PathRules {
        self.content.rules::<PathRules>()
    }

    fn plan(&self, p: MoveProfile, from: FxVec2, to: FxVec2) -> (PathPlan, PathWork) {
        let mut work = PathWork::default();
        let plan = plan_path(&self.nav, &self.hpa, p, from, to, self.rules(), &mut work);
        (plan, work)
    }

    fn centre(&self, c: CellXY) -> FxVec2 {
        cell_centre(&self.nav, c)
    }
}

/// Panics unless every segment `from → corners[0] → corners[1] → …` is clear for `p`.
fn assert_chain(nav: &NavGrid, p: MoveProfile, from: FxVec2, corners: &[FxVec2]) {
    let mut prev = from;
    for &corner in corners {
        assert!(
            line_clear(nav, p, prev, corner, &mut 0),
            "{prev:?} → {corner:?} is blocked; corners {corners:?}"
        );
        prev = corner;
    }
}

/// The reachable cell nearest `goal` from `start` over the whole map: the smallest
/// `(octile, y, x)` a full-map Dijkstra reaches.
fn nearest_reachable(nav: &NavGrid, p: MoveProfile, start: CellXY, goal: CellXY) -> CellXY {
    let (w, h) = nav.cells();
    let whole = CellRect {
        min: (0, 0),
        max: (w - 1, h - 1),
    };
    let field = dijkstra_from(nav, p, start, whole, &mut SearchStats::default());
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&c| field.cost(c).is_some())
        .min_by_key(|&c| (octile(c, goal), c.1, c.0))
        .expect("the start is reached")
}

#[test]
fn entrances_follow_run_rule() {
    // A water wall at x = 15 and 16 (the border of the two clusters) with a gap.
    let wall = |gap: std::ops::RangeInclusive<u32>| {
        map_of(&draw(32, 16, |x, y| {
            if (x == 15 || x == 16) && !gap.contains(&y) {
                '~'
            } else {
                '.'
            }
        }))
    };

    let five = wall(3..=7);
    let g = five.hpa.graph(TANK).expect("a graph for t.tank");
    assert_eq!(g.clusters(), (2, 1));
    assert_eq!(g.cluster_of((15, 9)), ClusterId(0));
    assert_eq!(g.cluster_of((16, 0)), ClusterId(1));
    assert_eq!(
        g.cluster_rect(ClusterId(1)),
        CellRect {
            min: (16, 0),
            max: (31, 15)
        }
    );
    // A 5-cell run: one transition at start + (5 − 1) / 2 = 3 + 2.
    assert_eq!(g.entrances(ClusterId(0)), &[(15, 5)]);
    assert_eq!(g.entrances(ClusterId(1)), &[(16, 5)]);
    assert_eq!(g.node_count(), 2);

    // An 8-cell run: two transitions, at both ends.
    let eight = wall(4..=11);
    let g = eight.hpa.graph(TANK).expect("a graph");
    assert_eq!(g.entrances(ClusterId(0)), &[(15, 4), (15, 11)]);
    assert_eq!(g.entrances(ClusterId(1)), &[(16, 4), (16, 11)]);
    assert_eq!(g.node_count(), 4);

    // The rule's edges: 6 is long (two transitions), a single cell is one at its index 0.
    let six = wall(2..=7);
    assert_eq!(
        six.hpa
            .graph(TANK)
            .expect("a graph")
            .entrances(ClusterId(0)),
        &[(15, 2), (15, 7)]
    );
    let one = wall(9..=9);
    assert_eq!(
        one.hpa
            .graph(TANK)
            .expect("a graph")
            .entrances(ClusterId(1)),
        &[(16, 9)]
    );
}

#[test]
fn border_corridor_detour_is_bounded_and_smoothed_away() {
    // The Task 10b counterexample in small: water at x = 12..=14 below y = 5 leaves x = 15, the
    // last column of cluster 0, a one-cell corridor along the border. The optimal path from
    // (16, 5) crosses the border at y = 5 and heads west, but the 16-pair run has transitions
    // only at its ends (y = 0 and 15). The cell path walks 5 cells down to (16, 0) and 5 back
    // up the corridor: HPA*'s known border detour, at most 2 × REACH (7) cells per crossing (R136).
    // String-pulling removes it from the corners the unit drives.
    let m = map_of(&draw(32, 16, |x, y| {
        if (12..=14).contains(&x) && y < 5 {
            '~'
        } else {
            '.'
        }
    }));
    let g = m.hpa.graph(TANK).expect("a graph");
    assert_eq!(g.entrances(ClusterId(1)), &[(16, 0), (16, 15)]);
    let (start, goal) = ((16, 5), (3, 3));
    let (from, to) = (m.centre(start), m.centre(goal));
    assert!(!line_clear(&m.nav, TANK, from, to, &mut 0));
    let (_, optimal) = astar(
        &m.nav,
        TANK,
        start,
        goal,
        whole(&m.nav),
        &mut SearchStats::default(),
    )
    .expect("a route");
    let (plan, _) = m.plan(TANK, from, to);
    assert_eq!(plan.status, PlanStatus::Found);
    let cell = u32::from(m.nav.cost(TANK.class, 16, 5));
    // 2666 against 1546 when recorded: 10 cells of detour.
    assert!(
        plan.cost > optimal
            && u64::from(plan.cost) <= u64::from(optimal) + 2 * REACH * u64::from(cell),
        "cell path {} against optimal {optimal}",
        plan.cost
    );
    let driven = corner_path_cost(&m.nav, TANK, start, &plan.corners);
    assert_eq!(driven, optimal, "the corners drive the optimal cost");
}

#[test]
fn direct_line_costs_only_los() {
    let m = map_of(&draw(32, 32, |_, _| '.'));
    let from = m.centre((1, 1));
    let to = FxVec2::new(Fx::from_int(30 * 4 + 1), Fx::from_int(28 * 4 + 3));
    let (plan, work) = m.plan(TANK, from, to);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners, vec![to]);
    assert_eq!(plan.cost, octile((1, 1), (30, 28)));
    assert_eq!(work.nodes_expanded, 0);
    assert!(work.los_cells > 0);
    assert_eq!(work.charge(), work.los_cells);
}

#[test]
fn same_cluster_uses_local_astar() {
    // A wall at x = 7 from the south edge up to y = 5, inside cluster 0. The way round it is
    // cheaper than any route leaving the cluster could be, so no abstract search runs.
    let m = map_of(&draw(
        32,
        32,
        |x, y| if x == 7 && y <= 5 { '~' } else { '.' },
    ));
    let (start, goal) = ((3, 3), (11, 3));
    let (from, to) = (m.centre(start), m.centre(goal));
    let rect = m
        .hpa
        .graph(TANK)
        .expect("a graph")
        .cluster_rect(ClusterId(0));
    let mut stats = SearchStats::default();
    let (cells, cost) = astar(&m.nav, TANK, start, goal, rect, &mut stats).expect("over the wall");
    let mut los = 0;
    assert!(!line_clear(&m.nav, TANK, from, to, &mut los));
    let corners = smooth(&m.nav, TANK, from, &cells, to, &mut los);

    let (plan, work) = m.plan(TANK, from, to);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.cost, cost);
    assert_eq!(plan.corners, corners);
    assert_eq!(
        work,
        PathWork {
            nodes_expanded: stats.expanded,
            los_cells: los,
        },
        "the direct line, one in-cluster A* and the smoothing: no abstract search"
    );
}

/// A same-cluster request on `rows` from `start` to `goal` (both in cluster 0) whose local route
/// fails the leave bound: the plan and its work, the in-cluster A* cost and corners, and the
/// work the local route and the two endpoint fields of the abstract search charge.
fn local_beats_abstract(
    rows: &[String],
    start: CellXY,
    goal: CellXY,
) -> (PathPlan, PathWork, u32, Vec<FxVec2>, PathWork) {
    let m = map_of(rows);
    let (from, to) = (m.centre(start), m.centre(goal));
    let rect = m
        .hpa
        .graph(TANK)
        .expect("a graph")
        .cluster_rect(ClusterId(0));
    let mut stats = SearchStats::default();
    let (cells, cost) = astar(&m.nav, TANK, start, goal, rect, &mut stats).expect("a local route");
    let mut los = 0;
    assert!(!line_clear(&m.nav, TANK, from, to, &mut los));
    let corners = smooth(&m.nav, TANK, from, &cells, to, &mut los);
    dijkstra_from(&m.nav, TANK, start, rect, &mut stats);
    dijkstra_from(&m.nav, TANK, goal, rect, &mut stats);
    let (plan, work) = m.plan(TANK, from, to);
    let base = PathWork {
        nodes_expanded: stats.expanded,
        los_cells: los,
    };
    (plan, work, cost, corners, base)
}

#[test]
fn same_cluster_local_route_wins_over_the_abstract_search() {
    // The wall at x = 7 reaches y = 12: going round it inside cluster 0 costs more than the
    // leave bound (1,800, out through x = 16), so the abstract search runs, finds nothing
    // cheaper and the local route stands. The search is charged: its two endpoint fields, and
    // the start, which it settles before every other entry's f reaches the local cost (it
    // settled 5 nodes before the search stopped at that limit).
    let rows = draw(32, 32, |x, y| if x == 7 && y <= 12 { '~' } else { '.' });
    let (plan, work, local, corners, base) = local_beats_abstract(&rows, (3, 3), (11, 3));
    assert!(local > 1800);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.cost, local);
    assert_eq!(plan.corners, corners);
    assert_eq!(work.los_cells, base.los_cells);
    assert_eq!(
        work.nodes_expanded,
        base.nodes_expanded + 1,
        "the start, settled"
    );
}

#[test]
fn same_cluster_local_route_stands_when_the_abstract_search_finds_none() {
    // As above, with water along the east and north edges of cluster 0: it has no entrances,
    // so the abstract search settles only the start and reaches no target.
    let rows = draw(32, 32, |x, y| {
        if (x == 7 && y <= 12) || (x == 15 && y <= 15) || (y == 15 && x <= 15) {
            '~'
        } else {
            '.'
        }
    });
    let (plan, work, local, corners, base) = local_beats_abstract(&rows, (3, 3), (11, 3));
    assert!(local > 1800);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.cost, local);
    assert_eq!(plan.corners, corners);
    assert_eq!(work.los_cells, base.los_cells);
    assert_eq!(
        work.nodes_expanded,
        base.nodes_expanded + 1,
        "the start, settled"
    );
}

#[test]
fn same_cluster_takes_a_cheaper_route_out_of_the_cluster() {
    // Water along y = 8 from x = 1 to the border of cluster 0: inside the cluster the only way
    // from (14, 6) to (14, 10) is round by x = 0, but a route through cluster 1 is short.
    let m = map_of(&draw(32, 32, |x, y| {
        if y == 8 && (1..=15).contains(&x) {
            '~'
        } else {
            '.'
        }
    }));
    let (start, goal) = ((14, 6), (14, 10));
    let (from, to) = (m.centre(start), m.centre(goal));
    let g = m.hpa.graph(TANK).expect("a graph");
    let (_, local) = astar(
        &m.nav,
        TANK,
        start,
        goal,
        g.cluster_rect(ClusterId(0)),
        &mut SearchStats::default(),
    )
    .expect("round by x = 0");
    let (_, optimal) = astar(
        &m.nav,
        TANK,
        start,
        goal,
        whole(&m.nav),
        &mut SearchStats::default(),
    )
    .expect("a route");
    assert!(optimal < local);

    let (plan, _) = m.plan(TANK, from, to);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners.last(), Some(&to));
    assert_eq!(plan.cost, optimal);
    assert_chain(&m.nav, TANK, from, &plan.corners);
}

#[test]
fn goal_in_water_snaps_to_nearest_shore() {
    let lake = map_of(&draw(32, 32, |x, y| {
        if (18..=26).contains(&x) && (18..=26).contains(&y) {
            '~'
        } else {
            '.'
        }
    }));
    let radius = lake.rules().snap_radius_cells;
    let mut probed = 0;
    let shore = nearest_passable(&lake.nav, TANK, (22, 22), radius, &mut probed)
        .expect("a shore within the snap radius");
    assert_eq!(shore, (22, 17));
    assert!(probed > 0);
    let from = lake.centre((2, 2));
    let (plan, work) = lake.plan(TANK, from, lake.centre((22, 22)));
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners.last(), Some(&lake.centre(shore)));
    assert_chain(&lake.nav, TANK, from, &plan.corners);
    assert!(
        work.nodes_expanded >= probed,
        "the snap's probes are charged"
    );

    // No shore within the radius (13 cells away): unreachable.
    let sea = map_of(&draw(32, 32, |x, y| {
        if (3..=29).contains(&x) && (3..=29).contains(&y) {
            '~'
        } else {
            '.'
        }
    }));
    let (plan, work) = sea.plan(TANK, sea.centre((1, 1)), sea.centre((16, 16)));
    assert_eq!(plan.status, PlanStatus::Unreachable);
    assert!(plan.corners.is_empty());
    assert!(work.nodes_expanded > 0);
}

#[test]
fn island_goal_is_partial() {
    // A lake 20..=28 square with an island 23..=25 in it, in cluster 3.
    let m = map_of(&draw(32, 32, |x, y| {
        let lake = (20..=28).contains(&x) && (20..=28).contains(&y);
        let island = (23..=25).contains(&x) && (23..=25).contains(&y);
        if lake && !island { '~' } else { '.' }
    }));
    let (start, goal) = ((2, 2), (24, 24));
    let from = m.centre(start);
    let (plan, _) = m.plan(TANK, from, m.centre(goal));
    assert_eq!(plan.status, PlanStatus::Partial);
    let nearest = nearest_reachable(&m.nav, TANK, start, goal);
    assert_eq!(nearest, (24, 19));
    assert_eq!(plan.corners.last(), Some(&m.centre(nearest)));
    assert_chain(&m.nav, TANK, from, &plan.corners);

    // The same when start and goal share a cluster: the island is in cluster 0 here.
    let near = map_of(&draw(32, 32, |x, y| {
        let lake = (6..=12).contains(&x) && (6..=12).contains(&y);
        let island = x == 9 && y == 9;
        if lake && !island { '~' } else { '.' }
    }));
    let from = near.centre((1, 1));
    let (plan, _) = near.plan(TANK, from, near.centre((9, 9)));
    assert_eq!(plan.status, PlanStatus::Partial);
    let nearest = nearest_reachable(&near.nav, TANK, (1, 1), (9, 9));
    assert_eq!(plan.corners.last(), Some(&near.centre(nearest)));
    assert_chain(&near.nav, TANK, from, &plan.corners);
}

#[test]
fn start_on_blocked_cell_recovers() {
    // A pond 2..=6 square, and a wall at x = 16 with a gap at the top: the goal needs a search.
    let m = map_of(&draw(32, 32, |x, y| {
        let pond = (2..=6).contains(&x) && (2..=6).contains(&y);
        let wall = x == 16 && y <= 28;
        if pond || wall { '~' } else { '.' }
    }));
    let radius = m.rules().snap_radius_cells;
    let snapped = nearest_passable(&m.nav, TANK, (3, 3), radius, &mut 0).expect("a shore");
    assert_eq!(snapped, (3, 1));
    let to = m.centre((24, 3));
    let (plan, work) = m.plan(TANK, m.centre((3, 3)), to);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners.first(), Some(&m.centre(snapped)));
    assert_eq!(plan.corners.last(), Some(&to));
    assert!(
        plan.corners.len() >= 3,
        "round the wall: {:?}",
        plan.corners
    );
    assert_chain(&m.nav, TANK, plan.corners[0], &plan.corners[1..]);
    assert!(work.nodes_expanded > 0);

    // A start off the map snaps into it.
    let off = FxVec2::new(Fx::from_int(-10), Fx::from_int(-10));
    let (plan, _) = m.plan(TANK, off, m.centre((10, 10)));
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners.first(), Some(&m.centre((0, 0))));
    assert_chain(&m.nav, TANK, plan.corners[0], &plan.corners[1..]);

    // No passable cell within the radius of the start: unreachable.
    let sea = map_of(&draw(32, 32, |x, y| {
        if (3..=29).contains(&x) && (3..=29).contains(&y) {
            '~'
        } else {
            '.'
        }
    }));
    let (plan, _) = sea.plan(TANK, sea.centre((16, 16)), sea.centre((1, 1)));
    assert_eq!(plan.status, PlanStatus::Unreachable);
    assert!(plan.corners.is_empty());
}

/// A river at y = 14..=17 across a 32 × 32 map, with one-cell bridges at x = 4 (A) and 27 (B).
fn two_bridges() -> Vec<String> {
    draw(32, 32, |x, y| match (x, y) {
        (4 | 27, 14..=17) => '=',
        (_, 14..=17) => '~',
        _ => '.',
    })
}

fn set_terrain(min: (u32, u32), max: (u32, u32), terrain: &str) -> DebugSetTerrainCmd {
    DebugSetTerrainCmd {
        min_cell: min,
        max_cell: max,
        terrain: terrain.to_owned(),
    }
}

fn scenario(rows: &[String]) -> ScenarioBuilder {
    ScenarioBuilder::new()
        .ascii_map(&rows_of(rows), CELL_M)
        .player("t.red", 0)
}

/// `plan_path` on `s`'s live navigation data.
fn plan_in(s: &Scenario, p: MoveProfile, from: FxVec2, to: FxVec2) -> (PathPlan, PathWork) {
    let world = s.world();
    let content = &world.resource::<fl_sim::Content>().0;
    let mut work = PathWork::default();
    let plan = plan_path(
        world.resource::<NavGrid>(),
        world.resource::<HpaGraphs>(),
        p,
        from,
        to,
        content.rules::<PathRules>(),
        &mut work,
    );
    (plan, work)
}

#[test]
fn bridge_removal_reroutes() {
    let mut s = scenario(&two_bridges()).build();
    let (from, to) = {
        let nav = s.world().resource::<NavGrid>();
        (cell_centre(nav, (4, 4)), cell_centre(nav, (4, 28)))
    };
    let (plan, _) = plan_in(&s, TANK, from, to);
    assert_eq!(plan.corners, vec![to], "straight over bridge A");

    s.issue(0, &[], set_terrain((4, 14), (4, 17), "t.water"), false);
    s.run_ticks(1);
    // Bridge A's rect, grown by the slope (1) and the clearance reach (2 toward −x/−y), then
    // by 1: clusters 0 (south-west) and 2 (north-west).
    assert_eq!(
        s.world()
            .resource::<SimCounters>()
            .get("path.clusters_invalidated"),
        2
    );
    let (plan, _) = plan_in(&s, TANK, from, to);
    assert_eq!(plan.status, PlanStatus::Found);
    assert_eq!(plan.corners.last(), Some(&to));
    let nav = s.world().resource::<NavGrid>();
    assert_chain(nav, TANK, from, &plan.corners);
    let over_b = plan
        .corners
        .iter()
        .any(|c| nav.cell_of(*c).0 >= 24 && nav.cell_of(*c).1 <= 18);
    assert!(over_b, "over bridge B: {:?}", plan.corners);
    assert_eq!(
        s.world().resource::<HpaGraphs>(),
        &HpaGraphs::build(nav, CLUSTER)
    );
}

#[test]
fn wide_profile_avoids_narrow_gap() {
    // A wall at x = 10 with a 1-cell gap at y = 16 and a 4-cell gap at y = 27..=30.
    let m = map_of(&draw(32, 32, |x, y| {
        if x == 10 && y != 16 && !(27..=30).contains(&y) {
            '~'
        } else {
            '.'
        }
    }));
    assert!(m.nav.passable(TANK, 10, 16));
    assert!(!m.nav.passable(WIDE, 10, 16));
    let (from, to) = (m.centre((2, 16)), m.centre((20, 16)));
    let (tank, _) = m.plan(TANK, from, to);
    assert_eq!(tank.status, PlanStatus::Found);
    assert_eq!(
        tank.corners,
        vec![to],
        "t.tank goes straight through the gap"
    );

    let (wide, _) = m.plan(WIDE, from, to);
    assert_eq!(wide.status, PlanStatus::Found);
    assert_eq!(wide.corners.last(), Some(&to));
    assert!(wide.corners.len() >= 2, "a detour: {:?}", wide.corners);
    assert_chain(&m.nav, WIDE, from, &wide.corners);
    assert!(
        wide.corners.iter().any(|c| m.nav.cell_of(*c).1 >= 26),
        "round by the wide gap: {:?}",
        wide.corners
    );
    // The tank's plan is a direct line, whose cost is only an octile lower bound (R99):
    // compare the wide detour with the tank's true cheapest route instead.
    let whole = CellRect {
        min: (0, 0),
        max: (31, 31),
    };
    let (_, tank_optimal) = astar(
        &m.nav,
        TANK,
        (2, 16),
        (20, 16),
        whole,
        &mut SearchStats::default(),
    )
    .expect("through the gap");
    assert!(
        wide.cost > tank_optimal,
        "{} against {tank_optimal}",
        wide.cost
    );
}

#[test]
fn clusters_query_lists_entrances() {
    let rows = draw(32, 32, |_, _| '.');
    let s = scenario(&rows).unit("t.tank", 0, 8, 8).build();
    let ask = |args: Value| debug_query(s.world(), "core.clusters", &args);
    let by_class = ask(json!({ "class": "Tracked", "size": 1 })).expect("answers");
    assert_eq!(by_class["cluster_cells"], json!(16));
    assert_eq!(by_class["clusters"], json!([2, 2]));
    // Two transitions per border (16-cell runs), four borders. A cell at the corner of two
    // borders is one node: each cluster has 3 entrances, 12 in all.
    assert_eq!(
        by_class["entrances"],
        json!([
            [15, 0],
            [0, 15],
            [15, 15],
            [16, 0],
            [16, 15],
            [31, 15],
            [0, 16],
            [15, 16],
            [15, 31],
            [16, 16],
            [31, 16],
            [16, 31]
        ])
    );
    assert_eq!(by_class["nodes"], json!(12));
    let by_unit = ask(json!({ "unit": 1 })).expect("answers");
    assert_eq!(by_unit, by_class, "unit #1 is a size-1 tank");

    // Bad args are errors, never panics.
    for args in [
        json!({}),
        json!({ "class": "Hover" }),
        json!({ "class": "Foot", "size": 3 }),
        json!({ "unit": 99 }),
    ] {
        assert!(ask(args.clone()).is_err(), "{args}");
    }
}

#[test]
fn hpa_cold_warm_and_loaded_agree() {
    // Drown bridge A, restore it, then drown part of bridge B: the live graphs are invalidated
    // three times.
    let make = || {
        scenario(&two_bridges())
            .command(1, 0, &[], set_terrain((4, 14), (4, 17), "t.water"))
            .command(2, 0, &[], set_terrain((4, 14), (4, 17), "t.bridge"))
            .command(3, 0, &[], set_terrain((27, 15), (27, 15), "t.water"))
    };
    let mut warm = make().build();
    warm.run_ticks(5);
    let bytes = warm.save();
    let loaded = warm.reload(&bytes);
    let nav = warm.world().resource::<NavGrid>();
    let cold = HpaGraphs::build(nav, CLUSTER);
    assert_eq!(warm.world().resource::<HpaGraphs>(), &cold);
    assert_eq!(loaded.world().resource::<HpaGraphs>(), &cold);

    let pairs = [((4, 4), (4, 28)), ((30, 2), (2, 30)), ((27, 10), (27, 20))];
    for p in [TANK, WIDE] {
        for (a, b) in pairs {
            let (from, to) = (cell_centre(nav, a), cell_centre(nav, b));
            let live = plan_in(&warm, p, from, to);
            let reloaded = plan_in(&loaded, p, from, to);
            let mut work = PathWork::default();
            let plan = plan_path(
                nav,
                &cold,
                p,
                from,
                to,
                warm.world()
                    .resource::<fl_sim::Content>()
                    .0
                    .rules::<PathRules>(),
                &mut work,
            );
            assert_eq!(live, (plan.clone(), work), "{p:?} {a:?} → {b:?}");
            assert_eq!(reloaded, (plan, work), "{p:?} {a:?} → {b:?}");
        }
    }
}

// --- proptests --------------------------------------------------------------------------

/// The content of the ASCII mod; the proptests draw their own terrain over its kinds.
static CONTENT: LazyLock<Arc<ContentDb>> = LazyLock::new(|| ascii_terrain_content(&["."], 4));

/// The ASCII mod's terrain ids, sorted: the palette of the proptest maps.
const IDS: [&str; 7] = [
    "t.bridge",
    "t.deep_forest",
    "t.forest",
    "t.grass",
    "t.road",
    "t.swamp",
    "t.water",
];
const WATER: u8 = 6;
const SIDE: u32 = 64;

/// A 64 × 64 map of mixed ground (grass, road, forest) with water rectangles.
fn random_terrain(ground: &[u8], lakes: &[(u32, u32, u32, u32)]) -> TerrainGrid {
    let side = SIDE as usize;
    let mut types: Vec<u8> = ground.to_vec();
    for &(x, y, w, h) in lakes {
        for cy in y..(y + h).min(SIDE) {
            for cx in x..(x + w).min(SIDE) {
                types[cy as usize * side + cx as usize] = WATER;
            }
        }
    }
    TerrainGrid {
        cells_x: SIDE,
        cells_y: SIDE,
        palette: IDS.iter().map(|&id| id.to_owned()).collect(),
        types,
        heights: Vec::new(),
        height_m: (Fx::ZERO, Fx::from_int(65535)),
    }
}

fn ground() -> impl Strategy<Value = Vec<u8>> {
    // Grass, road or forest: three different costs.
    proptest::collection::vec(
        prop_oneof![Just(3u8), Just(4u8), Just(2u8)],
        (SIDE * SIDE) as usize,
    )
}

fn lakes() -> impl Strategy<Value = Vec<(u32, u32, u32, u32)>> {
    proptest::collection::vec((0..SIDE, 0..SIDE, 1u32..17, 1u32..17), 0..12)
}

fn profile() -> impl Strategy<Value = MoveProfile> {
    prop_oneof![Just(TANK), Just(WIDE)]
}

fn passable_cells(nav: &NavGrid, p: MoveProfile) -> Vec<CellXY> {
    let (w, h) = nav.cells();
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| nav.passable(p, i64::from(x), i64::from(y)))
        .collect()
}

/// A point inside cell `c` at raw offsets `(ox, oy)` (each below one cell).
fn inside(c: CellXY, (ox, oy): (i64, i64)) -> FxVec2 {
    let cell = Fx::from_int(CELL_M).raw();
    FxVec2::new(
        Fx::from_raw(i64::from(c.0) * cell + ox),
        Fx::from_raw(i64::from(c.1) * cell + oy),
    )
}

fn offset() -> impl Strategy<Value = (i64, i64)> {
    let cell = Fx::from_int(CELL_M).raw();
    (0..cell, 0..cell)
}

fn whole(nav: &NavGrid) -> CellRect {
    let (w, h) = nav.cells();
    CellRect {
        min: (0, 0),
        max: (w - 1, h - 1),
    }
}

/// One random request on a random map: the map, the profile, the start and goal cells and
/// points, HPA*'s plan and the full-map A* answer.
struct Case {
    nav: NavGrid,
    p: MoveProfile,
    start: CellXY,
    from: FxVec2,
    to: FxVec2,
    plan: PathPlan,
    /// The optimal cell path and its cost.
    optimal: Option<(Vec<CellXY>, u32)>,
}

fn case(
    ground: &[u8],
    lakes: &[(u32, u32, u32, u32)],
    p: MoveProfile,
    picks: (usize, usize),
    offsets: ((i64, i64), (i64, i64)),
) -> Option<Case> {
    let nav = build_nav(&CONTENT, &random_terrain(ground, lakes));
    let cells = passable_cells(&nav, p);
    if cells.is_empty() {
        return None;
    }
    let (start, goal) = (cells[picks.0 % cells.len()], cells[picks.1 % cells.len()]);
    let (from, to) = (inside(start, offsets.0), inside(goal, offsets.1));
    let hpa = HpaGraphs::build(&nav, CLUSTER);
    let mut work = PathWork::default();
    let plan = plan_path(
        &nav,
        &hpa,
        p,
        from,
        to,
        CONTENT.rules::<PathRules>(),
        &mut work,
    );
    let optimal = astar(
        &nav,
        p,
        start,
        goal,
        whole(&nav),
        &mut SearchStats::default(),
    );
    Some(Case {
        nav,
        p,
        start,
        from,
        to,
        plan,
        optimal,
    })
}

/// What crossing one cluster border at a transition can add, in costliest-cell steps (R136,
/// ADR-0015): the crossing pair lies in a run whose nearest transition is at most 7 pairs
/// away (`(CLUSTER − 1) / 2`: a 16-pair run has them at its ends), so at most 7 steps along
/// the border on each side, the crossing step, and for a diagonal crossing the step through
/// its side cell. Each piece of the optimal path between crossings stays in one cluster, so
/// an intra edge or the start or goal link costs no more, and the abstract A* is optimal.
const CROSSING_STEPS: u64 = 2 * REACH + 2;

/// The farthest a pair of a border run can be from the run's nearest transition, with
/// `CLUSTER`-cell clusters: the middle of a full run, whose transitions are at its ends.
const REACH: u64 = (CLUSTER as u64 - 1) / 2;

/// The cost of the cheapest cell path from `start` through the cells of `corners` in turn: a
/// lower bound on what driving the straight legs between the corners costs on the grid.
fn corner_path_cost(nav: &NavGrid, p: MoveProfile, start: CellXY, corners: &[FxVec2]) -> u32 {
    let mut at = start;
    let mut cost = 0;
    for &corner in corners {
        let next = nav.cell_of(corner);
        let (_, leg) = astar(nav, p, at, next, whole(nav), &mut SearchStats::default())
            .expect("a corner is reachable from the one before");
        cost += leg;
        at = next;
    }
    cost
}

/// The cluster borders `path` crosses; a diagonal step through a cluster corner crosses two.
fn border_crossings(path: &[CellXY]) -> u64 {
    let cluster = |c: CellXY| (c.0 / CLUSTER, c.1 / CLUSTER);
    path.windows(2)
        .map(|w| {
            let (a, b) = (cluster(w[0]), cluster(w[1]));
            u64::from(a.0 != b.0) + u64::from(a.1 != b.1)
        })
        .sum()
}

/// The cost of the costliest cell passable for `p`.
fn max_cost(nav: &NavGrid, p: MoveProfile) -> u64 {
    passable_cells(nav, p)
        .into_iter()
        .map(|(x, y)| u64::from(nav.cost(p.class, x, y)))
        .max()
        .unwrap_or(0)
}

fn rect() -> impl Strategy<Value = CellRect> {
    (0..SIDE, 0..SIDE, 0..SIDE, 0..SIDE).prop_map(|(a, b, c, d)| CellRect {
        min: (a.min(b), c.min(d)),
        max: (a.max(b), c.max(d)),
    })
}

/// Small rects more often than large ones: a bridge, a crater, a wall.
fn edit_rect() -> impl Strategy<Value = CellRect> {
    prop_oneof![
        3 => (0..SIDE, 0..SIDE, 0u32..4, 0u32..4).prop_map(|(x, y, w, h)| CellRect {
            min: (x, y),
            max: ((x + w).min(SIDE - 1), (y + h).min(SIDE - 1)),
        }),
        1 => rect(),
    ]
}

/// `default`, or `PROPTEST_CASES` when it is set (the nightly gate sets 4096).
#[expect(
    clippy::disallowed_methods,
    reason = "the case count, like proptest's own default, comes from PROPTEST_CASES"
)]
fn cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Maps in [`hpa_cost_ratio_tripwire`], 4 requests each.
const TRIPWIRE_MAPS: u32 = 200;

/// A fixed-seed batch of random requests (the proptest strategies, `TRIPWIRE_MAPS` maps × 4
/// requests): every searched plan within the guaranteed bound, and the worst cell-path ratio at
/// most 1.8, a tripwire just above the 1.775 observed over 20,000 requests (Task 7b report).
#[test]
fn hpa_cost_ratio_tripwire() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng};
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: TRIPWIRE_MAPS,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let strategy = (
        ground(),
        lakes(),
        profile(),
        proptest::collection::vec((any::<usize>(), any::<usize>()), 4),
    );
    let worst = std::cell::Cell::new((0u64, 1u64));
    runner
        .run(&strategy, |(ground, lakes, p, picks)| {
            let nav = build_nav(&CONTENT, &random_terrain(&ground, &lakes));
            let cells = passable_cells(&nav, p);
            if cells.is_empty() {
                return Ok(());
            }
            let hpa = HpaGraphs::build(&nav, CLUSTER);
            let c_max = max_cost(&nav, p);
            for (a, b) in picks {
                let (start, goal) = (cells[a % cells.len()], cells[b % cells.len()]);
                let (from, to) = (cell_centre(&nav, start), cell_centre(&nav, goal));
                if line_clear(&nav, p, from, to, &mut 0) {
                    continue;
                }
                let Some((path, optimal)) = astar(
                    &nav,
                    p,
                    start,
                    goal,
                    whole(&nav),
                    &mut SearchStats::default(),
                ) else {
                    continue;
                };
                let rules = CONTENT.rules::<PathRules>();
                let plan = plan_path(&nav, &hpa, p, from, to, rules, &mut PathWork::default());
                let (cost, optimal) = (u64::from(plan.cost), u64::from(optimal));
                let bound = optimal + border_crossings(&path) * CROSSING_STEPS * c_max;
                prop_assert!(cost <= bound, "cost {} over the bound {}", cost, bound);
                let (wc, wo) = worst.get();
                if cost * wo > wc * optimal {
                    worst.set((cost, optimal));
                }
            }
            Ok(())
        })
        .expect("every plan within the guaranteed bound");
    let (cost, optimal) = worst.get();
    assert!(
        cost * 10 <= optimal * 18,
        "worst cell-path ratio {cost} / {optimal} over 1.8"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(64)))]

    #[test]
    fn hpa_finds_path_iff_astar_does(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        picks in (any::<usize>(), any::<usize>()),
        offsets in (offset(), offset()),
    ) {
        let Some(c) = case(&ground, &lakes, p, picks, offsets) else {
            return Ok(());
        };
        prop_assert_eq!(c.plan.status == PlanStatus::Found, c.optimal.is_some());
        assert_chain(&c.nav, c.p, c.from, &c.plan.corners);
        if c.optimal.is_some() {
            prop_assert_eq!(c.plan.corners.last(), Some(&c.to));
        } else {
            prop_assert_eq!(c.plan.status, PlanStatus::Partial);
            // A partial plan ends in the centre of a cell reachable from the start.
            let end = *c.plan.corners.last().expect("a partial plan has corners");
            let cell = c.nav.cell_of(end);
            prop_assert_eq!(end, cell_centre(&c.nav, cell));
            let field = dijkstra_from(&c.nav, c.p, c.start, whole(&c.nav), &mut SearchStats::default());
            prop_assert!(field.cost(cell).is_some());
        }
    }

    #[test]
    fn hpa_cost_near_optimal(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        picks in (any::<usize>(), any::<usize>()),
        offsets in (offset(), offset()),
    ) {
        let Some(c) = case(&ground, &lakes, p, picks, offsets) else {
            return Ok(());
        };
        if let Some((path, optimal)) = c.optimal {
            // A searched plan's cost is a real route's (a direct line's is an octile lower
            // bound, R99), so never below the optimum.
            if !line_clear(&c.nav, c.p, c.from, c.to, &mut 0) {
                prop_assert!(
                    c.plan.cost >= optimal,
                    "searched cost {} below optimal {}", c.plan.cost, optimal
                );
            }
            // The bound HPA* guarantees (R136, ADR-0015): (2 × REACH + 2) × the costliest
            // passable cell per cluster border the optimal path crosses.
            let crossings = border_crossings(&path);
            let bound = u64::from(optimal) + crossings * CROSSING_STEPS * max_cost(&c.nav, c.p);
            prop_assert!(
                u64::from(c.plan.cost) <= bound,
                "cost {} against optimal {} crossing {} borders", c.plan.cost, optimal, crossings
            );
        }
    }

    #[test]
    fn invalidate_equals_full_rebuild(
        ground in ground(),
        lakes in lakes(),
        edits in proptest::collection::vec((edit_rect(), 0usize..IDS.len()), 1..6),
    ) {
        let mut terrain = random_terrain(&ground, &lakes);
        let mut nav = build_nav(&CONTENT, &terrain);
        let mut hpa = HpaGraphs::build(&nav, CLUSTER);
        let rules = CONTENT.rules::<MovementRules>();
        for (rect, id) in edits {
            terrain.set_rect(rect, IDS[id]);
            let updated = nav.update_rect(&terrain, &CONTENT, rules, rect);
            let rebuilt = hpa.invalidate(&nav, updated);
            prop_assert!(rebuilt.is_sorted() && rebuilt.windows(2).all(|w| w[0] != w[1]));
            let graph = hpa.graph(TANK).expect("a tank graph");
            for corner in [rect.min, rect.max] {
                prop_assert!(rebuilt.contains(&graph.cluster_of(corner)));
            }
            prop_assert_eq!(&hpa, &HpaGraphs::build(&nav, CLUSTER));
        }
    }
}
