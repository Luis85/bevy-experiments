//! Grid search (ADR-0015, Task 6): A* with pinned tie-breaks and no corner cutting, the
//! bounded Dijkstra distance field, the supercover line-of-sight test, string-pulling and the
//! nearest passable cell.

use std::sync::{Arc, LazyLock};

use fl_content::{ContentDb, MapDef, MoveClass};
use fl_math::{Fx, FxVec2};
use fl_sim::nav::{
    CellXY, MoveProfile, NavGrid, PathRules, SearchStats, astar, cell_centre, content_profiles,
    dijkstra_from, edge_cost, line_clear, nearest_passable, octile, smooth,
};
use fl_sim::{CellRect, MovementRules, TerrainGrid};
use fl_test_utils::ascii_terrain_content;
use proptest::prelude::*;

/// Cell side of every map here, in metres.
const CELL_M: i32 = 4;

const TANK: MoveProfile = MoveProfile {
    class: MoveClass::Tracked,
    size: 1,
};

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

/// The navigation grid of the ASCII map `rows` (row 0 is the north edge, y = rows − 1).
fn nav_of(rows: &[&str]) -> NavGrid {
    let content = ascii_terrain_content(rows, CELL_M);
    let map = content.kind::<MapDef>().get("t.ascii").expect("the map");
    let layers = map.layers.as_ref().expect("decoded layers");
    let terrain = TerrainGrid::from_layers(layers, map.terrain.as_ref().expect("terrain").height_m);
    build_nav(&content, &terrain)
}

fn whole(nav: &NavGrid) -> CellRect {
    let (w, h) = nav.cells();
    CellRect {
        min: (0, 0),
        max: (w - 1, h - 1),
    }
}

fn at(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

#[test]
fn edge_cost_and_octile_are_pinned() {
    let nav = nav_of(&["rrr", "r.r"]);
    // Road 100, grass (pass 0.8, Tracked path factor) costs more than a road cell.
    assert_eq!(edge_cost(&nav, MoveClass::Tracked, (0, 1), (1, 1)), 100);
    assert_eq!(edge_cost(&nav, MoveClass::Tracked, (0, 0), (1, 1)), 141);
    // Grass for Tracked: factor 0.8 + 0.2 × 0.5 = 0.9, cost ceil(100 / 0.9) = 112.
    assert_eq!(nav.cost(MoveClass::Tracked, 1, 0), 112);
    // (100 + 112 + 1) / 2 = 106; diagonal 106 × 141 / 100 = 149.
    assert_eq!(edge_cost(&nav, MoveClass::Tracked, (0, 0), (1, 0)), 106);
    assert_eq!(edge_cost(&nav, MoveClass::Tracked, (0, 1), (1, 0)), 149);
    assert_eq!(octile((0, 0), (3, 1)), 341);
    assert_eq!(octile((3, 1), (0, 0)), 341);
    assert_eq!(octile((2, 2), (2, 2)), 0);
    assert_eq!(octile((0, 5), (0, 0)), 500);
}

#[test]
fn astar_open_grid_pinned() {
    let nav = nav_of(&["rrrr", "rrrr"]);
    let mut stats = SearchStats::default();
    let found = astar(&nav, TANK, (0, 0), (3, 1), whole(&nav), &mut stats);
    assert_eq!(found, Some((vec![(0, 0), (1, 1), (2, 1), (3, 1)], 341)));
    assert_eq!(stats.expanded, 4, "the four path cells, the goal included");
}

#[test]
fn astar_start_is_goal() {
    let nav = nav_of(&["rr"]);
    let mut stats = SearchStats::default();
    let found = astar(&nav, TANK, (1, 0), (1, 0), whole(&nav), &mut stats);
    assert_eq!(found, Some((vec![(1, 0)], 0)));
    assert_eq!(stats.expanded, 1);
}

#[test]
fn no_corner_cutting() {
    // y = 2 is row 0. Water at (1, 0) and (0, 1): (0, 0) is sealed in its corner.
    let sealed = nav_of(&["...", "~..", ".~."]);
    let mut stats = SearchStats::default();
    assert_eq!(
        astar(&sealed, TANK, (0, 0), (1, 1), whole(&sealed), &mut stats),
        None
    );
    assert_eq!(
        stats.expanded, 1,
        "only the start: the diagonal is not an edge"
    );

    // Water at (1, 0) only: the route goes round through (0, 1), never diagonally.
    let open = nav_of(&["...", "...", ".~."]);
    let found = astar(&open, TANK, (0, 0), (1, 1), whole(&open), &mut stats);
    let (path, cost) = found.expect("a route round the water");
    assert_eq!(path, vec![(0, 0), (0, 1), (1, 1)]);
    assert_eq!(
        cost,
        edge_cost(&open, MoveClass::Tracked, (0, 0), (0, 1))
            + edge_cost(&open, MoveClass::Tracked, (0, 1), (1, 1))
    );
}

#[test]
fn astar_respects_bounds() {
    let nav = nav_of(&["rrrrr", "rrrrr", "rrrrr"]);
    let bounds = CellRect {
        min: (0, 0),
        max: (2, 2),
    };
    let mut stats = SearchStats::default();
    assert_eq!(astar(&nav, TANK, (0, 0), (4, 1), bounds, &mut stats), None);
    assert_eq!(astar(&nav, TANK, (4, 1), (0, 0), bounds, &mut stats), None);
    assert_eq!(
        stats.expanded, 0,
        "an endpoint outside the bounds searches nothing"
    );

    // A wall at x = 1 except at y = 2, with bounds that cut the gap off: no route inside.
    let walled = nav_of(&["r.rr", "r~rr", "r~rr"]);
    let low = CellRect {
        min: (0, 0),
        max: (3, 1),
    };
    assert_eq!(astar(&walled, TANK, (0, 0), (2, 0), low, &mut stats), None);
    let (path, _) = astar(&walled, TANK, (0, 0), (2, 0), whole(&walled), &mut stats)
        .expect("a route through the gap");
    assert!(path.contains(&(1, 2)));

    // Bounds past the map edge are clipped to it.
    let past = CellRect {
        min: (0, 0),
        max: (100, 100),
    };
    assert!(astar(&walled, TANK, (0, 0), (2, 0), past, &mut stats).is_some());
}

#[test]
fn astar_breaks_equal_f_and_h_by_y_then_x() {
    // y = 2 is row 0. Water at (1, 1): round the south (y = 0) or the north (y = 2), equal
    // cost and equal h at every step; the smaller y wins.
    let nav = nav_of(&["...", ".~.", "..."]);
    let mut stats = SearchStats::default();
    let (path, cost) =
        astar(&nav, TANK, (0, 1), (2, 1), whole(&nav), &mut stats).expect("round the water");
    assert_eq!(path, vec![(0, 1), (0, 0), (1, 0), (2, 0), (2, 1)]);
    let grass = u32::from(nav.cost(MoveClass::Tracked, 0, 0));
    assert_eq!(cost, 4 * grass);
}

#[test]
fn diagonal_at_the_bounds_edge() {
    // A diagonal's side cells always lie inside the bounds (the bounds are a rectangle
    // holding both ends), so bounds of exactly the two cells' box allow the diagonal...
    let nav = nav_of(&["rrrr", "rrrr", "rrrr", "rrrr"]);
    let tight = CellRect {
        min: (1, 1),
        max: (2, 2),
    };
    let mut stats = SearchStats::default();
    assert_eq!(
        astar(&nav, TANK, (1, 1), (2, 2), tight, &mut stats),
        Some((vec![(1, 1), (2, 2)], 141))
    );
    // ...and a blocked side cell still forbids it. y = 3 is row 0: water at (2, 1).
    let side = nav_of(&["rrrr", "rrrr", "rr~r", "rrrr"]);
    let (path, _) = astar(&side, TANK, (1, 1), (2, 2), tight, &mut stats).expect("round");
    assert_eq!(path, vec![(1, 1), (1, 2), (2, 2)]);
}

/// The smoothing work bound (R93): `6·N·ceil(log2 N) + 12·N` cells for `N` path points.
fn smooth_bound(n: usize) -> u32 {
    let log = n.max(2).next_power_of_two().ilog2() as usize;
    u32::try_from(6 * n * log + 12 * n).expect("small")
}

/// Smooth the A* path from `start` to `goal` (cell centres) and check it.
fn smooth_work(nav: &NavGrid, start: CellXY, goal: CellXY) -> (usize, u32, Vec<FxVec2>) {
    let mut stats = SearchStats::default();
    let (cells, _) = astar(nav, TANK, start, goal, whole(nav), &mut stats).expect("a route");
    let (from, to) = (cell_centre(nav, start), cell_centre(nav, goal));
    let mut checked = 0;
    let corners = smooth(nav, TANK, from, &cells, to, &mut checked);
    assert_eq!(corners.last(), Some(&to));
    let mut prev = from;
    for &corner in &corners {
        assert!(line_clear(nav, TANK, prev, corner, &mut 0));
        prev = corner;
    }
    (cells.len(), checked, corners)
}

#[test]
fn smooth_work_is_bounded_straight() {
    let row = "r".repeat(200);
    let nav = nav_of(&[row.as_str()]);
    let (n, checked, corners) = smooth_work(&nav, (0, 0), (199, 0));
    assert_eq!(n, 200);
    assert_eq!(corners.len(), 1);
    assert!(
        checked <= smooth_bound(n),
        "{checked} > {}",
        smooth_bound(n)
    );
}

#[test]
fn smooth_work_is_bounded_diagonal() {
    let row = ".".repeat(100);
    let rows = vec![row.as_str(); 100];
    let nav = nav_of(&rows);
    let (n, checked, corners) = smooth_work(&nav, (0, 0), (99, 99));
    assert_eq!(n, 100);
    assert_eq!(corners.len(), 1);
    assert!(
        checked <= smooth_bound(n),
        "{checked} > {}",
        smooth_bound(n)
    );
}

#[test]
fn smooth_work_is_bounded_round_a_bend() {
    // An L-shaped corridor: the bottom row (y = 0) and the right column, 120 + 80 cells.
    let mut rows: Vec<String> = (0..80).map(|_| format!("{}r", "~".repeat(119))).collect();
    rows.push("r".repeat(120));
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let nav = nav_of(&rows);
    let (n, checked, corners) = smooth_work(&nav, (0, 0), (119, 80));
    assert_eq!(n, 200);
    assert_eq!(corners.len(), 2, "one bend: {corners:?}");
    assert!(
        checked <= smooth_bound(n),
        "{checked} > {}",
        smooth_bound(n)
    );
    // Greedy first-failure smoothing tests about 0.5·119² + 0.5·80² ≈ 10k cells here; the
    // galloping search needs about 1.5k. The cap pins that for a bend.
    assert!(checked <= 2_000, "{checked} > 2000");
}

#[test]
fn astar_rejects_blocked_endpoints() {
    let nav = nav_of(&["r~r"]);
    let mut stats = SearchStats::default();
    assert_eq!(
        astar(&nav, TANK, (1, 0), (2, 0), whole(&nav), &mut stats),
        None
    );
    assert_eq!(
        astar(&nav, TANK, (0, 0), (1, 0), whole(&nav), &mut stats),
        None
    );
    assert_eq!(stats.expanded, 0);
}

#[test]
fn unreachable_goal_expands_reachable_region() {
    // A closed lake ring around (3, 3) on a 7 × 7 map: 49 − 8 water − 1 inside = 40 cells.
    let nav = nav_of(&[
        ".......", ".......", "..~~~..", "..~.~..", "..~~~..", ".......", ".......",
    ]);
    let mut stats = SearchStats::default();
    assert_eq!(
        astar(&nav, TANK, (0, 0), (3, 3), whole(&nav), &mut stats),
        None
    );
    assert_eq!(stats.expanded, 40);

    let mut dstats = SearchStats::default();
    let field = dijkstra_from(&nav, TANK, (0, 0), whole(&nav), &mut dstats);
    assert_eq!(dstats.expanded, 40);
    assert_eq!(field.cost((3, 3)), None);
    assert_eq!(field.path_to((3, 3)), None);
    assert_eq!(field.cost((0, 0)), Some(0));
    assert_eq!(field.path_to((0, 0)), Some(vec![(0, 0)]));
}

#[test]
fn dijkstra_respects_bounds() {
    let nav = nav_of(&["rrrr", "rrrr"]);
    let bounds = CellRect {
        min: (1, 0),
        max: (2, 1),
    };
    let mut stats = SearchStats::default();
    let field = dijkstra_from(&nav, TANK, (1, 0), bounds, &mut stats);
    assert_eq!(stats.expanded, 4);
    assert_eq!(field.cost((2, 1)), Some(141));
    assert_eq!(field.path_to((2, 1)), Some(vec![(1, 0), (2, 1)]));
    assert_eq!(field.cost((0, 0)), None, "outside the bounds");
    assert_eq!(field.cost((3, 1)), None, "outside the bounds");

    let outside = dijkstra_from(&nav, TANK, (0, 0), bounds, &mut stats);
    assert_eq!(outside.cost((0, 0)), None);
    assert_eq!(
        stats.expanded, 4,
        "a source outside the bounds settles nothing"
    );
}

#[test]
fn line_clear_detects_water_and_corners() {
    // y = 2 is row 0. Water at (1, 0) only.
    let nav = nav_of(&["...", "...", ".~."]);
    let c = |x, y| cell_centre(&nav, (x, y));
    assert_eq!(c(0, 0), at(2, 2));
    assert_eq!(c(2, 1), at(10, 6));

    let mut checked = 0;
    // Centre (0, 0) → centre (1, 1) passes exactly through the corner (4, 4), which touches
    // the water cell (1, 0).
    assert!(!line_clear(&nav, TANK, c(0, 0), c(1, 1), &mut checked));
    assert!(checked > 0);
    // Straight through the water cell.
    assert!(!line_clear(&nav, TANK, c(0, 0), c(2, 0), &mut checked));
    // Just above the corner: through (0, 1), never (1, 0).
    assert!(line_clear(
        &nav,
        TANK,
        c(0, 0),
        FxVec2::new(Fx::from_int(6), Fx::from_ratio(13, 2)),
        &mut checked
    ));
    // Up the open column and back down the far side of the water.
    assert!(line_clear(&nav, TANK, c(0, 0), c(0, 2), &mut checked));
    assert!(line_clear(&nav, TANK, c(2, 2), c(2, 0), &mut checked));

    // A clear segment counts every cell it walks: 3 cells up the column.
    let mut count = 0;
    assert!(line_clear(&nav, TANK, c(0, 0), c(0, 2), &mut count));
    assert_eq!(count, 3);
    // A segment off the map is blocked.
    assert!(!line_clear(&nav, TANK, c(0, 0), at(-2, 2), &mut count));
}

#[test]
fn smooth_pulls_the_string() {
    let nav = nav_of(&["....", "....", "....", "...."]);
    let mut stats = SearchStats::default();
    let (cells, _) = astar(&nav, TANK, (0, 0), (3, 1), whole(&nav), &mut stats).expect("open");
    let mut checked = 0;
    let goal = at(14, 5);
    let corners = smooth(&nav, TANK, at(1, 1), &cells, goal, &mut checked);
    assert_eq!(
        corners,
        vec![goal],
        "an open map pulls straight to the goal"
    );
    assert!(checked > 0);

    // Around a wall: y = 3 is row 0; water at x = 1, y in 1..=3.
    let walled = nav_of(&[".~..", ".~..", ".~..", "...."]);
    let (cells, _) =
        astar(&walled, TANK, (0, 3), (2, 3), whole(&walled), &mut stats).expect("round the wall");
    let start = cell_centre(&walled, (0, 3));
    let goal = cell_centre(&walled, (2, 3));
    let corners = smooth(&walled, TANK, start, &cells, goal, &mut checked);
    assert_eq!(corners.last(), Some(&goal));
    assert!(corners.len() >= 2, "the wall needs a corner: {corners:?}");
    let mut from = start;
    for &corner in &corners {
        assert!(line_clear(&walled, TANK, from, corner, &mut checked));
        from = corner;
    }
}

#[test]
fn nearest_passable_ring_order() {
    // y = 4 is row 0. All water but (4, 2) and (2, 0): both 2 cells from (2, 2).
    let nav = nav_of(&["~~~~~", "~~~~~", "~~~~.", "~~~~~", "~~.~~"]);
    let mut probed = 0;
    assert_eq!(
        nearest_passable(&nav, TANK, (2, 2), 2, &mut probed),
        Some((2, 0))
    );
    assert_eq!(
        probed,
        1 + 8 + 16,
        "every cell of rings 0, 1 and 2 is probed"
    );
    assert_eq!(nearest_passable(&nav, TANK, (2, 2), 1, &mut probed), None);
    assert_eq!(probed, 25 + 9, "probes add up");
    let mut probed = 0;
    assert_eq!(
        nearest_passable(&nav, TANK, (4, 2), 3, &mut probed),
        Some((4, 2))
    );
    assert_eq!(probed, 1, "ring 0 only");
    // A large radius on a closed map ends.
    let sea = nav_of(&["~~", "~~"]);
    assert_eq!(nearest_passable(&sea, TANK, (0, 0), u32::MAX, &mut 0), None);
    // The ring's corner is farther than its side: (0, 0) is 1 diagonal, (2, 1) 1 straight.
    let ring = nav_of(&["~~~", "~~.", ".~~"]);
    assert_eq!(
        nearest_passable(&ring, TANK, (1, 1), 1, &mut 0),
        Some((2, 1))
    );
}

// --- proptests --------------------------------------------------------------------------

/// The content of the ASCII mod; the proptests draw their own terrain over its kinds.
static CONTENT: LazyLock<Arc<ContentDb>> = LazyLock::new(|| ascii_terrain_content(&["."], 4));

/// Palette indices (the ASCII mod's sorted terrain ids) of grass and water.
const IDS: [&str; 7] = [
    "t.bridge",
    "t.deep_forest",
    "t.forest",
    "t.grass",
    "t.road",
    "t.swamp",
    "t.water",
];
const SIDE: u32 = 24;

/// A 24 × 24 map of mixed ground (grass, road, forest) with water rectangles.
fn random_nav(ground: &[u8], lakes: &[(u32, u32, u32, u32)]) -> NavGrid {
    let side = SIDE as usize;
    let mut types: Vec<u8> = ground.to_vec();
    for &(x, y, w, h) in lakes {
        for cy in y..(y + h).min(SIDE) {
            for cx in x..(x + w).min(SIDE) {
                types[cy as usize * side + cx as usize] = 6;
            }
        }
    }
    let terrain = TerrainGrid {
        cells_x: SIDE,
        cells_y: SIDE,
        palette: IDS.iter().map(|&id| id.to_owned()).collect(),
        types,
        heights: Vec::new(),
        height_m: (Fx::ZERO, Fx::from_int(65535)),
    };
    build_nav(&CONTENT, &terrain)
}

fn passable_cells(nav: &NavGrid, p: MoveProfile) -> Vec<CellXY> {
    let (w, h) = nav.cells();
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| nav.passable(p, i64::from(x), i64::from(y)))
        .collect()
}

fn ground() -> impl Strategy<Value = Vec<u8>> {
    // Grass, road or forest: three different costs.
    proptest::collection::vec(
        prop_oneof![Just(3u8), Just(4u8), Just(2u8)],
        (SIDE * SIDE) as usize,
    )
}

fn lakes() -> impl Strategy<Value = Vec<(u32, u32, u32, u32)>> {
    proptest::collection::vec((0..SIDE, 0..SIDE, 1u32..9, 1u32..9), 0..8)
}

fn profile() -> impl Strategy<Value = MoveProfile> {
    (1u8..=2).prop_map(|size| MoveProfile {
        class: MoveClass::Tracked,
        size,
    })
}

/// Panics unless `path` is a valid walk from `start` to `goal` inside `bounds` costing `cost`.
fn check_path(
    nav: &NavGrid,
    p: MoveProfile,
    path: &[CellXY],
    start: CellXY,
    goal: CellXY,
    bounds: CellRect,
    cost: u32,
) {
    assert_eq!(path.first(), Some(&start));
    assert_eq!(path.last(), Some(&goal));
    let ok = |c: CellXY| nav.passable(p, i64::from(c.0), i64::from(c.1));
    let mut sum = 0;
    for &c in path {
        assert!(ok(c), "{c:?} is not passable");
        assert!(
            (bounds.min.0..=bounds.max.0).contains(&c.0)
                && (bounds.min.1..=bounds.max.1).contains(&c.1),
            "{c:?} is outside {bounds:?}"
        );
    }
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            a != b && a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1,
            "{a:?} → {b:?}"
        );
        if a.0 != b.0 && a.1 != b.1 {
            assert!(
                ok((b.0, a.1)) && ok((a.0, b.1)),
                "{a:?} → {b:?} cuts a corner"
            );
        }
        sum += edge_cost(nav, p.class, a, b);
    }
    assert_eq!(sum, cost);
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

/// Like [`offset`], but often exactly on a cell edge (0) or the centre, so points land on
/// edges and corners.
fn edgy_offset() -> impl Strategy<Value = (i64, i64)> {
    let cell = Fx::from_int(CELL_M).raw();
    let axis = || prop_oneof![Just(0i64), Just(cell / 2), 0..cell];
    (axis(), axis())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn astar_matches_dijkstra_cost(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        picks in (any::<usize>(), any::<usize>()),
        sub in (0..SIDE, 0..SIDE, 0..SIDE, 0..SIDE),
    ) {
        let nav = random_nav(&ground, &lakes);
        let cells = passable_cells(&nav, p);
        prop_assume!(!cells.is_empty());
        let (start, goal) = (cells[picks.0 % cells.len()], cells[picks.1 % cells.len()]);
        let sub = CellRect {
            min: (sub.0.min(sub.1), sub.2.min(sub.3)),
            max: (sub.0.max(sub.1), sub.2.max(sub.3)),
        };
        // In the sub-rect, a second pair from its own passable cells (when it has any).
        let inner: Vec<CellXY> = cells
            .iter()
            .copied()
            .filter(|&(x, y)| {
                (sub.min.0..=sub.max.0).contains(&x) && (sub.min.1..=sub.max.1).contains(&y)
            })
            .collect();
        let mut runs = vec![(whole(&nav), start, goal)];
        if !inner.is_empty() {
            runs.push((sub, inner[picks.0 % inner.len()], inner[picks.1 % inner.len()]));
        }
        for (bounds, start, goal) in runs {
            let mut stats = SearchStats::default();
            let found = astar(&nav, p, start, goal, bounds, &mut stats);
            let mut dstats = SearchStats::default();
            let field = dijkstra_from(&nav, p, start, bounds, &mut dstats);
            prop_assert_eq!(found.as_ref().map(|(_, cost)| *cost), field.cost(goal));
            prop_assert!(stats.expanded <= dstats.expanded);
            if let Some((path, cost)) = found {
                check_path(&nav, p, &path, start, goal, bounds, cost);
                let dpath = field.path_to(goal).expect("dijkstra reached the goal");
                check_path(&nav, p, &dpath, start, goal, bounds, cost);
            }
        }
    }

    #[test]
    fn smooth_keeps_path_valid(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        picks in (any::<usize>(), any::<usize>()),
        offsets in (offset(), offset()),
    ) {
        let nav = random_nav(&ground, &lakes);
        let cells = passable_cells(&nav, p);
        prop_assume!(!cells.is_empty());
        let (start, goal) = (cells[picks.0 % cells.len()], cells[picks.1 % cells.len()]);
        let mut stats = SearchStats::default();
        let Some((path, _)) = astar(&nav, p, start, goal, whole(&nav), &mut stats) else {
            // No route: Dijkstra must agree, and there is nothing to smooth.
            let field = dijkstra_from(&nav, p, start, whole(&nav), &mut stats);
            prop_assert_eq!(field.cost(goal), None);
            return Ok(());
        };
        let (from, to) = (inside(start, offsets.0), inside(goal, offsets.1));
        prop_assert_eq!(nav.cell_of(from), start);
        prop_assert_eq!(nav.cell_of(to), goal);
        let mut checked = 0;
        let corners = smooth(&nav, p, from, &path, to, &mut checked);
        prop_assert_eq!(corners.last(), Some(&to));
        prop_assert!(corners.len() < path.len().max(2));
        // The work bound holds where visibility along the path is not monotone too.
        prop_assert!(
            checked <= smooth_bound(path.len().max(2)),
            "{} > {}", checked, smooth_bound(path.len().max(2))
        );
        let mut prev = from;
        for &corner in &corners {
            let mut n = 0;
            prop_assert!(
                line_clear(&nav, p, prev, corner, &mut n),
                "{:?} → {:?} is blocked; corners {:?}", prev, corner, corners
            );
            prev = corner;
        }
    }

    #[test]
    fn line_clear_is_symmetric(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        cells in ((0..SIDE, 0..SIDE), (0..SIDE, 0..SIDE)),
        offsets in (edgy_offset(), edgy_offset()),
    ) {
        let nav = random_nav(&ground, &lakes);
        let (a, b) = (inside(cells.0, offsets.0), inside(cells.1, offsets.1));
        let (mut ab, mut ba) = (0, 0);
        prop_assert_eq!(
            line_clear(&nav, p, a, b, &mut ab),
            line_clear(&nav, p, b, a, &mut ba),
            "{:?} ↔ {:?}", a, b
        );
    }

    #[test]
    fn astar_is_deterministic(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        picks in (any::<usize>(), any::<usize>()),
    ) {
        let nav = random_nav(&ground, &lakes);
        let again = random_nav(&ground, &lakes);
        let cells = passable_cells(&nav, p);
        prop_assume!(!cells.is_empty());
        let (start, goal) = (cells[picks.0 % cells.len()], cells[picks.1 % cells.len()]);
        let (mut a, mut b) = (SearchStats::default(), SearchStats::default());
        let first = astar(&nav, p, start, goal, whole(&nav), &mut a);
        let second = astar(&again, p, start, goal, whole(&again), &mut b);
        prop_assert_eq!(&first, &second);
        prop_assert_eq!(a, b);
        let (mut da, mut db) = (SearchStats::default(), SearchStats::default());
        prop_assert_eq!(
            dijkstra_from(&nav, p, start, whole(&nav), &mut da),
            dijkstra_from(&again, p, start, whole(&again), &mut db)
        );
        prop_assert_eq!(da, db);
        if let Some((path, _)) = first {
            let (mut ca, mut cb) = (0, 0);
            let s = cell_centre(&nav, start);
            let g = cell_centre(&nav, goal);
            prop_assert_eq!(
                smooth(&nav, p, s, &path, g, &mut ca),
                smooth(&again, p, s, &path, g, &mut cb)
            );
            prop_assert_eq!(ca, cb);
        }
    }

    #[test]
    fn nearest_passable_is_nearest(
        ground in ground(),
        lakes in lakes(),
        p in profile(),
        cell in (0..SIDE, 0..SIDE),
        radius in 0u32..6,
    ) {
        let nav = random_nav(&ground, &lakes);
        let found = nearest_passable(&nav, p, cell, radius, &mut 0);
        let ring = |c: CellXY| c.0.abs_diff(cell.0).max(c.1.abs_diff(cell.1));
        let key = |c: CellXY| {
            let (dx, dy) = (u64::from(c.0.abs_diff(cell.0)), u64::from(c.1.abs_diff(cell.1)));
            (ring(c), dx * dx + dy * dy, c.1, c.0)
        };
        let best = passable_cells(&nav, p)
            .into_iter()
            .filter(|&c| ring(c) <= radius)
            .min_by_key(|&c| key(c));
        prop_assert_eq!(found, best);
    }
}
