//! Path requests, the order queue and path following (ADR-0015, Task 8): moves and wander
//! targets go through the budgeted [`PathQueue`], results apply in request order, stale and
//! orphaned requests are discarded, units follow the planned corners (turning in place first
//! when the heading is far off), a terrain change mid-route re-plans, and `MoveOrder` v1
//! saves migrate.

use std::collections::BTreeMap;

use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{DebugSetTerrainCmd, MoveCmd, SimId, StopCmd, Tick};
use fl_sim::nav::{
    CellXY, HpaGraphs, MoveProfile, NavGrid, PathRules, SearchStats, dijkstra_from, octile,
    profile_of,
};
use fl_sim::{
    CellRect, Footprint, LoadError, MAX_WAYPOINTS, Mobility, MoveOrder, MovementRules, Order,
    PathFollow, PathQueue, PathState, SaveFile, SimCounters, SimIndex, TerrainGrid, debug_query,
    setup_despawn,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, assert_branch, assert_deterministic, assert_shuffle_invariant,
};
use fl_view::JournalEntry;
use serde_json::{Value, json};

/// Cell side of every map here, in metres.
const CELL_M: i32 = 4;

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

/// The centre of cell `c`, in metres.
fn at(c: CellXY) -> FxVec2 {
    let m = |i: u32| Fx::from_int(i32::try_from(i).expect("a small map") * CELL_M + CELL_M / 2);
    FxVec2::new(m(c.0), m(c.1))
}

/// `(x, y)` metres of the centre of cell `c`, for [`ScenarioBuilder::unit`].
fn metres(c: CellXY) -> (i32, i32) {
    let m = |i: u32| i32::try_from(i).expect("a small map") * CELL_M + CELL_M / 2;
    (m(c.0), m(c.1))
}

fn move_to(c: CellXY) -> MoveCmd {
    MoveCmd { target: at(c) }
}

/// The base rules with `Path.budget_per_tick` set to `budget`, and `Path.flow_min_group` above
/// any group here (64), so every move asks for single requests: these tests pin the queue,
/// not flow fields (Task 10).
fn rules_with_budget(budget: u32) -> String {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron");
    let base = std::fs::read_to_string(&path).expect("the base rules");
    let rules = base.replace(
        "budget_per_tick: 8000,",
        &format!("budget_per_tick: {budget},"),
    );
    assert_ne!(rules, base, "the base rules name budget_per_tick: 8000");
    let singles = rules.replace("flow_min_group: 6,", "flow_min_group: 64,");
    assert_ne!(singles, rules, "the base rules name flow_min_group: 6");
    singles
}

/// A scenario on the ASCII map `rows`, player 0 (`t.red`) and `units` of `unit` at the
/// centres of the given cells.
fn on_map(rows: &[String], unit: &str, cells: &[CellXY]) -> ScenarioBuilder {
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows_of(rows), CELL_M)
        .player("t.red", 0);
    for &c in cells {
        let (x, y) = metres(c);
        b = b.unit(unit, 0, x, y);
    }
    b
}

fn entity(s: &Scenario, id: u32) -> bevy_ecs::entity::Entity {
    s.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .unwrap_or_else(|| panic!("#{id} is alive"))
}

fn follow(s: &Scenario, id: u32) -> PathFollow {
    s.world()
        .get::<PathFollow>(entity(s, id))
        .cloned()
        .expect("a mobile unit has a PathFollow")
}

fn orders(s: &Scenario, id: u32) -> Vec<Order> {
    s.world()
        .get::<MoveOrder>(entity(s, id))
        .expect("a mobile unit has a MoveOrder")
        .orders
        .clone()
}

fn pos(s: &Scenario, id: u32) -> FxVec2 {
    s.pose(id).expect("alive").pos
}

fn cell_of(s: &Scenario, id: u32) -> CellXY {
    s.world().resource::<NavGrid>().cell_of(pos(s, id))
}

fn profile(s: &Scenario, id: u32) -> MoveProfile {
    let world = s.world();
    let nav = world.resource::<NavGrid>();
    let e = entity(s, id);
    profile_of(
        world.get::<Mobility>(e).expect("mobile"),
        world.get::<Footprint>(e),
        nav.cell_m(),
        nav.max_clearance(),
    )
}

fn arrive_m(s: &Scenario) -> Fx {
    s.world()
        .resource::<fl_sim::Content>()
        .0
        .rules::<MovementRules>()
        .arrive_m
}

/// Whether #`id` has finished its orders within `arrive_m` of `goal`.
fn arrived(s: &Scenario, id: u32, goal: FxVec2) -> bool {
    orders(s, id).is_empty() && (pos(s, id) - goal).length() <= arrive_m(s)
}

fn entries<'a>(s: &'a Scenario, kind: &str) -> Vec<&'a JournalEntry> {
    s.journal().iter().filter(|e| e.kind == kind).collect()
}

/// The `"core.path_result"` entries of request `request`.
fn result_of(s: &Scenario, request: u32) -> &JournalEntry {
    let found: Vec<&JournalEntry> = entries(s, "core.path_result")
        .into_iter()
        .filter(|e| e.data["request"] == request)
        .collect();
    assert_eq!(found.len(), 1, "one result for request {request}");
    found[0]
}

/// The `"core.order_changed"` reasons of #`id`, in journal order.
fn order_reasons(s: &Scenario, id: u32) -> Vec<String> {
    entries(s, "core.order_changed")
        .into_iter()
        .filter(|e| e.sim_id == Some(SimId(id)))
        .map(|e| e.data["reason"].as_str().expect("a reason").to_owned())
        .collect()
}

/// The terrain id of cell `c`.
fn terrain(s: &Scenario, c: CellXY) -> String {
    s.world()
        .resource::<TerrainGrid>()
        .terrain_at(c.0, c.1)
        .unwrap_or("open")
        .to_owned()
}

/// A 32 × 32 map with a lake at x = 10..=21, y = 6..=25.
fn lake() -> Vec<String> {
    draw(32, 32, |x, y| {
        if (10..=21).contains(&x) && (6..=25).contains(&y) {
            '~'
        } else {
            '.'
        }
    })
}

fn lake_detour() -> ScenarioBuilder {
    on_map(&lake(), "t.tank", &[(4, 16)]).command(0, 0, &[1], move_to((27, 16)))
}

#[test]
fn tank_paths_around_lake() {
    let mut s = lake_detour().build();
    let goal = at((27, 16));
    let p = profile(&s, 1);
    for _ in 0..600 {
        s.run_ticks(1);
        let c = cell_of(&s, 1);
        assert!(
            s.world()
                .resource::<NavGrid>()
                .passable(p, i64::from(c.0), i64::from(c.1)),
            "tick {}: #1 in blocked cell {c:?}",
            s.tick().0
        );
        if arrived(&s, 1, goal) {
            return;
        }
    }
    panic!("#1 did not arrive in 600 ticks: at {:?}", pos(&s, 1));
}

#[test]
fn crosses_river_only_by_bridge() {
    // A river at y = 7..=8 with a bridge at x = 12.
    let rows = draw(16, 16, |x, y| match (x, y) {
        (12, 7..=8) => '=',
        (_, 7..=8) => '~',
        _ => '.',
    });
    let goal = (2, 13);
    let mut s = on_map(&rows, "t.truck", &[(2, 2)])
        .command(0, 0, &[1], move_to(goal))
        .build();
    let mut on_bridge = false;
    for _ in 0..400 {
        s.run_ticks(1);
        let here = terrain(&s, cell_of(&s, 1));
        assert_ne!(here, "t.water", "tick {}: in the river", s.tick().0);
        on_bridge |= here == "t.bridge";
        if arrived(&s, 1, at(goal)) {
            assert!(on_bridge, "crossed by the bridge");
            return;
        }
    }
    panic!("#1 did not arrive: at {:?}", pos(&s, 1));
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
fn unreachable_goal_goes_to_closest_point() {
    // A lake inside the north-east cluster (16..=31 on both axes; clusters are 16 cells) with
    // a 2 × 2 island. A partial plan scans the start cluster and the cluster of the settled
    // node nearest the goal (R97); here that cluster holds the reachable cell nearest the
    // island, so the plan's end is the nearest reachable cell of the whole map.
    let rows = draw(32, 32, |x, y| {
        let lake = (18..=29).contains(&x) && (18..=29).contains(&y);
        let island = (23..=24).contains(&x) && (23..=24).contains(&y);
        if lake && !island { '~' } else { '.' }
    });
    let (start, goal) = ((4, 4), (23, 23));
    let mut s = on_map(&rows, "t.tank", &[start])
        .command(0, 0, &[1], move_to(goal))
        .build();
    s.run_ticks(1);
    assert_eq!(result_of(&s, 0).data["status"], "partial");
    let nearest = nearest_reachable(s.world().resource::<NavGrid>(), profile(&s, 1), start, goal);
    for _ in 0..1500 {
        if orders(&s, 1).is_empty() {
            break;
        }
        s.run_ticks(1);
    }
    assert!(orders(&s, 1).is_empty(), "the order ended");
    assert_eq!(cell_of(&s, 1), nearest);
    assert_eq!(order_reasons(&s, 1), ["command", "completed"]);
}

#[test]
fn hull_rotates_before_moving() {
    let open = draw(32, 32, |_, _| '.');
    let goal = at((4, 16));
    let mut s = on_map(&open, "t.tank", &[(16, 16)])
        .command(0, 0, &[1], MoveCmd { target: goal })
        .build();
    let start = pos(&s, 1);
    assert_eq!(s.pose(1).expect("alive").heading, Angle::ZERO);
    let threshold = i32::from(Angle::from_degrees(Fx::from_int(30)).raw());
    for _ in 0..60 {
        s.run_ticks(1);
        let pose = s.pose(1).expect("alive");
        let error = pose.heading.signed_diff((goal - pose.pos).angle()).abs();
        if error > threshold {
            assert_eq!(pose.pos, start, "tick {}: turning in place", s.tick().0);
        } else {
            assert_ne!(
                pose.pos,
                start,
                "tick {}: within 30°, it drives",
                s.tick().0
            );
            assert!(pose.pos.x < start.x, "west");
            return;
        }
    }
    panic!("the tank never turned within 30° of west");
}

#[test]
fn queued_moves_run_in_order() {
    let open = draw(32, 32, |_, _| '.');
    let (a, b) = (at((15, 5)), at((15, 15)));
    let mut s = on_map(&open, "t.tank", &[(5, 5)]).build();
    s.issue(0, &[1], MoveCmd { target: a }, false);
    s.issue(0, &[1], MoveCmd { target: b }, true);
    s.run_ticks(1);
    assert_eq!(
        orders(&s, 1),
        [Order::Move { target: a }, Order::Move { target: b }]
    );
    let (mut at_a, mut at_b) = (None, None);
    for _ in 0..600 {
        s.run_ticks(1);
        let here = pos(&s, 1);
        if at_a.is_none() && (here - a).length() <= arrive_m(&s) {
            at_a = Some(s.tick());
        }
        if at_b.is_none() && (here - b).length() <= arrive_m(&s) {
            at_b = Some(s.tick());
        }
        if at_b.is_some() && orders(&s, 1).is_empty() {
            break;
        }
    }
    let (at_a, at_b) = (at_a.expect("reached A"), at_b.expect("reached B"));
    assert!(at_a < at_b, "A ({at_a:?}) before B ({at_b:?})");
    assert!(orders(&s, 1).is_empty());
    assert_eq!(
        order_reasons(&s, 1),
        ["command", "command", "completed", "completed"]
    );
}

/// Two tanks on an open map with a path budget of 1: one request per tick.
fn two_tanks_budget_one() -> Scenario {
    let open = draw(32, 32, |_, _| '.');
    let mut s = on_map(&open, "t.tank", &[(4, 4), (8, 4)])
        .ascii_rules(&rules_with_budget(1))
        .build();
    s.issue(0, &[1, 2], move_to((20, 20)), false);
    s.run_ticks(1);
    assert_eq!(result_of(&s, 0).data["status"], "found");
    assert_eq!(
        s.world().resource::<PathQueue>().pending.len(),
        1,
        "#2's request waits"
    );
    s
}

#[test]
fn stop_clears_orders_and_pending_request() {
    let mut s = two_tanks_budget_one();
    s.issue(0, &[2], StopCmd, false);
    s.run_ticks(1);
    let result = result_of(&s, 1);
    assert_eq!(result.sim_id, Some(SimId(2)));
    assert_eq!(result.data["status"], "stale");
    assert_eq!(result.data["charge"], 0);
    let f = follow(&s, 2);
    assert_eq!(f.state, PathState::Idle);
    assert!(f.corners.is_empty());
    assert!(orders(&s, 2).is_empty());
    assert!(s.world().resource::<PathQueue>().pending.is_empty());
    assert_eq!(order_reasons(&s, 2), ["command", "stopped"]);
    let stopped = pos(&s, 2);
    s.run_ticks(5);
    assert_eq!(pos(&s, 2), stopped);
}

#[test]
fn stale_request_is_discarded() {
    let open = draw(32, 32, |_, _| '.');
    let mut s = on_map(&open, "t.tank", &[(4, 4)]).build();
    s.issue(0, &[1], move_to((20, 4)), false);
    s.issue(0, &[1], move_to((4, 20)), false);
    s.run_ticks(1);
    let first = result_of(&s, 0);
    assert_eq!(first.data["status"], "stale");
    assert_eq!(first.data["charge"], 0);
    let second = result_of(&s, 1);
    assert_eq!(second.data["status"], "found");
    let f = follow(&s, 1);
    assert_eq!(f.state, PathState::Following);
    assert_eq!(f.corners.last(), Some(&at((4, 20))));
    assert_eq!(
        orders(&s, 1),
        [Order::Move {
            target: at((4, 20))
        }]
    );
}

#[test]
fn despawned_unit_request_discarded() {
    let mut s = two_tanks_budget_one();
    assert!(setup_despawn(s.runner.world_mut(), SimId(2)));
    s.run_ticks(1);
    let result = result_of(&s, 1);
    assert_eq!(result.data["status"], "stale");
    assert_eq!(result.data["charge"], 0);
    assert!(s.world().resource::<PathQueue>().pending.is_empty());
}

/// 48 × 32 cells with a pond at x = 16..=24, y = 6..=7; 40 tanks along y = 2 (#1 at x = 1 …
/// #40 at x = 40), all ordered at tick 1 to cell (20, 10) beyond the pond, with a path budget
/// of 300: most see the goal (cheap straight lines), the ones behind the pond search.
fn forty_movers() -> ScenarioBuilder {
    let ids: Vec<u32> = (1..=40).collect();
    forty_movers_map().command(1, 0, &ids, move_to((20, 10)))
}

/// The map, the 40 tanks and the budget of [`forty_movers`], without the order.
fn forty_movers_map() -> ScenarioBuilder {
    let rows = draw(48, 32, |x, y| {
        if (16..=24).contains(&x) && (6..=7).contains(&y) {
            '~'
        } else {
            '.'
        }
    });
    let cells: Vec<CellXY> = (1..=40).map(|x| (x, 2)).collect();
    on_map(&rows, "t.tank", &cells).ascii_rules(&rules_with_budget(300))
}

/// Per tick, the `(request, charge)` of every path result, sorted by request id. The journal
/// orders a tick's results by sim id; here request ids follow sim ids, and
/// `queue_pops_in_request_order_not_sim_id_order` pins the pop order itself.
fn results_by_tick(s: &Scenario) -> BTreeMap<Tick, Vec<(u64, u64)>> {
    let mut by_tick: BTreeMap<Tick, Vec<(u64, u64)>> = BTreeMap::new();
    for e in entries(s, "core.path_result") {
        let request = e.data["request"].as_u64().expect("a request id");
        let charge = e.data["charge"].as_u64().expect("a charge");
        by_tick.entry(e.tick).or_default().push((request, charge));
    }
    for results in by_tick.values_mut() {
        results.sort_unstable();
    }
    by_tick
}

#[test]
fn queue_respects_budget_and_order() {
    let mut s = forty_movers().build();
    s.run_ticks(40);
    let by_tick = results_by_tick(&s);
    let all: Vec<u64> = by_tick.values().flatten().map(|&(r, _)| r).collect();
    assert_eq!(all, (0..40).collect::<Vec<u64>>(), "request order, all 40");
    assert!(
        by_tick.len() > 2,
        "the budget spread the requests: {by_tick:?}"
    );
    assert!(
        by_tick.values().any(|results| results.len() > 1),
        "several cheap requests share a tick: {by_tick:?}"
    );
    for (tick, results) in &by_tick {
        let total: u64 = results.iter().map(|&(_, c)| c).sum();
        let last = results.last().expect("a result").1;
        assert!(
            total - last < 300,
            "tick {}: {results:?} spent past the budget",
            tick.0
        );
    }
}

/// [`forty_movers`] with the moves issued one unit per command from #40 down to #1, so the
/// request ids run against the sim ids: #40 gets request 0, #1 request 39.
fn forty_movers_reversed() -> ScenarioBuilder {
    let mut b = forty_movers_map();
    for id in (1..=40).rev() {
        b = b.command(1, 0, &[id], move_to((20, 10)));
    }
    b
}

#[test]
fn queue_pops_in_request_order_not_sim_id_order() {
    // The journal orders a tick's results by sim id, so the pop order inside a tick is not
    // visible there. What is: each tick processes a prefix of the queue. With request ids
    // running against sim ids, popping by sim id would take other sets.
    let mut s = forty_movers_reversed().build();
    s.run_ticks(40);
    let mut by_tick: BTreeMap<Tick, Vec<u64>> = BTreeMap::new();
    for e in entries(&s, "core.path_result") {
        let request = e.data["request"].as_u64().expect("a request id");
        let unit = u64::from(e.sim_id.expect("a unit").0);
        assert_eq!(request, 40 - unit, "#{unit}'s request");
        by_tick.entry(e.tick).or_default().push(request);
    }
    let mut next = 0;
    for (tick, ids) in &mut by_tick {
        ids.sort_unstable();
        let expected: Vec<u64> = (next..next + ids.len() as u64).collect();
        assert_eq!(
            *ids, expected,
            "tick {}: the next requests in queue order",
            tick.0
        );
        next += ids.len() as u64;
    }
    assert_eq!(next, 40);
    assert!(by_tick.values().any(|ids| ids.len() > 1), "{by_tick:?}");
}

#[test]
fn no_request_starves() {
    let mut s = forty_movers().build();
    s.run_ticks(40);
    let results = entries(&s, "core.path_result");
    assert_eq!(results.len(), 40);
    let total: u64 = results
        .iter()
        .map(|e| e.data["charge"].as_u64().expect("a charge"))
        .sum();
    let bound = total.div_ceil(300) + 1;
    for e in results {
        let waited = e.data["waited"].as_u64().expect("waited");
        assert!(waited <= bound, "{e:?} waited past {bound} ticks");
    }
}

/// A river at y = 14..=17 across a 32 × 32 map, with one-cell bridges at x = 4 (A) and 27 (B).
fn two_bridges() -> Vec<String> {
    draw(32, 32, |x, y| match (x, y) {
        (4 | 27, 14..=17) => '=',
        (_, 14..=17) => '~',
        _ => '.',
    })
}

#[test]
fn bridge_destroyed_mid_route_repaths() {
    let goal = (4, 28);
    let mut s = on_map(&two_bridges(), "t.truck", &[(4, 4)])
        .command(0, 0, &[1], move_to(goal))
        .command(
            5,
            0,
            &[],
            DebugSetTerrainCmd {
                min_cell: (4, 14),
                max_cell: (4, 17),
                terrain: "t.water".to_owned(),
            },
        )
        .build();
    let mut over_b = false;
    for _ in 0..800 {
        s.run_ticks(1);
        let c = cell_of(&s, 1);
        assert_ne!(
            terrain(&s, c),
            "t.water",
            "tick {}: in the river",
            s.tick().0
        );
        over_b |= c.0 >= 24 && (14..=17).contains(&c.1);
        if arrived(&s, 1, at(goal)) {
            break;
        }
    }
    assert!(arrived(&s, 1, at(goal)), "#1 arrived: at {:?}", pos(&s, 1));
    assert!(over_b, "#1 crossed bridge B");
    let reasons = order_reasons(&s, 1);
    assert_eq!(reasons, ["command", "replanned", "completed"]);
    let replanned = entries(&s, "core.order_changed")
        .into_iter()
        .find(|e| e.data["reason"] == "replanned")
        .expect("replanned");
    assert_eq!(replanned.tick, Tick(5));
}

#[test]
fn wander_goes_through_the_queue() {
    let rows = draw(32, 32, |x, y| {
        if (12..=18).contains(&x) && (12..=18).contains(&y) {
            '~'
        } else {
            '.'
        }
    });
    let cells = [(4, 4), (26, 4), (4, 26), (26, 26)];
    let mut s = on_map(&rows, "t.tank", &cells).wander_all(40).build();
    s.run_ticks(50);
    for id in 1..=4 {
        let requests = entries(&s, "core.path_request")
            .into_iter()
            .filter(|e| e.sim_id == Some(SimId(id)))
            .count();
        assert!(requests > 0, "#{id} requested a path");
    }
}

#[test]
fn branch_with_queued_requests() {
    let mut s = forty_movers().build();
    s.run_ticks(2);
    assert!(
        !s.world().resource::<PathQueue>().pending.is_empty(),
        "requests are queued at tick 2"
    );
    assert_branch(forty_movers, 2, 300);
}

mod v1 {
    use fl_math::FxVec2;
    use serde::Serialize;

    /// `MoveOrder` as saved at version 1.
    #[derive(Serialize)]
    pub struct MoveOrder {
        pub waypoints: Vec<FxVec2>,
    }
}

#[test]
fn move_order_v1_migrates() {
    let open = draw(32, 32, |_, _| '.');
    let mut s = on_map(&open, "t.tank", &[(4, 4)]).build();
    let (a, b) = (at((10, 4)), at((10, 10)));
    let mut save: SaveFile = postcard::from_bytes(&s.save()).expect("a SaveFile");
    let (_, components) = &mut save.sim.entities[0];
    let entry = components
        .iter_mut()
        .find(|(name, _, _)| name == "core.move_order")
        .expect("#1 has core.move_order");
    assert_eq!(entry.1, 2, "saved at version 2");
    entry.1 = 1;
    entry.2 = postcard::to_allocvec(&v1::MoveOrder {
        waypoints: vec![a, b],
    })
    .expect("encodes");
    let bytes = postcard::to_allocvec(&save).expect("encodes");
    let mut loaded = s.reload(&bytes);
    assert_eq!(
        orders(&loaded, 1),
        [Order::Move { target: a }, Order::Move { target: b }]
    );
    loaded.run_ticks(1);
    assert_eq!(follow(&loaded, 1).state, PathState::Following);
}

/// `s`'s save with #1's `core.move_order` replaced by `bytes` saved at `version`.
fn save_with_move_order(s: &mut Scenario, version: u32, bytes: Vec<u8>) -> Vec<u8> {
    let mut save: SaveFile = postcard::from_bytes(&s.save()).expect("a SaveFile");
    let (_, components) = &mut save.sim.entities[0];
    let entry = components
        .iter_mut()
        .find(|(name, _, _)| name == "core.move_order")
        .expect("#1 has core.move_order");
    entry.1 = version;
    entry.2 = bytes;
    postcard::to_allocvec(&save).expect("encodes")
}

#[test]
fn over_cap_move_order_fails_to_load() {
    // R77 holds for loaded state too: a save whose orders exceed MAX_WAYPOINTS, at v2 or as
    // v1 waypoints, is a codec error naming the state.
    let open = draw(32, 32, |_, _| '.');
    let mut s = on_map(&open, "t.tank", &[(4, 4)]).build();
    let content = s.world().resource::<fl_sim::Content>().0.clone();
    let features = s.features().to_vec();
    let target = at((10, 4));
    let load = |bytes: &[u8]| {
        fl_sim::load_world(
            &features,
            content.clone(),
            bytes,
            fl_sim::RunConfig::default(),
        )
    };
    let v2 = |n: usize| {
        let order = MoveOrder {
            orders: vec![Order::Move { target }; n],
        };
        postcard::to_allocvec(&order).expect("encodes")
    };
    let v1 = |n: usize| {
        postcard::to_allocvec(&v1::MoveOrder {
            waypoints: vec![target; n],
        })
        .expect("encodes")
    };
    for (version, bytes) in [(2, v2(MAX_WAYPOINTS + 1)), (1, v1(MAX_WAYPOINTS + 1))] {
        let save = save_with_move_order(&mut s, version, bytes);
        match load(&save) {
            Err(LoadError::Codec { name, .. }) => assert_eq!(name, "core.move_order"),
            Err(other) => panic!("v{version}: expected a codec error, got {other}"),
            Ok(_) => panic!("v{version}: an over-cap order queue loaded"),
        }
    }
    for (version, bytes) in [(2, v2(MAX_WAYPOINTS)), (1, v1(MAX_WAYPOINTS))] {
        let save = save_with_move_order(&mut s, version, bytes);
        let loaded = load(&save).unwrap_or_else(|err| panic!("v{version} at the cap: {err}"));
        let world = loaded.app.world();
        let unit = world.resource::<SimIndex>().get(SimId(1)).expect("#1");
        let orders = &world.get::<MoveOrder>(unit).expect("orders").orders;
        assert_eq!(orders.len(), MAX_WAYPOINTS, "v{version}");
    }
}

#[test]
fn path_counters_and_journal() {
    let mut s = lake_detour().build();
    s.run_ticks(1);
    let counters = s.world().resource::<SimCounters>();
    assert_eq!(counters.get("path.requests"), 1);
    assert!(counters.get("path.nodes_expanded") > 0, "a search");
    assert!(counters.get("path.budget_spent") > 0);
    let request = entries(&s, "core.path_request");
    assert_eq!(request.len(), 1);
    assert_eq!(request[0].sim_id, Some(SimId(1)));
    assert_eq!(request[0].data["request"], 0);
    assert_eq!(request[0].data["kind"], "single");
    assert_eq!(
        request[0].data["profile"],
        json!({ "class": "Tracked", "size": 1 })
    );
    let result = result_of(&s, 0);
    assert_eq!(result.data["status"], "found");
    assert_eq!(result.data["waited"], 0);
    assert!(
        result.data["corners"].as_u64().expect("corners") >= 2,
        "a detour"
    );

    let mut backlog = forty_movers().build();
    backlog.run_ticks(2);
    let counters = backlog.world().resource::<SimCounters>();
    assert!(counters.get("path.queue_len") > 0, "backlogged");
    assert!(counters.get("path.requests") > 0);
}

#[test]
fn paths_debug_query() {
    let mut s = lake_detour().build();
    s.run_ticks(1);
    let answer =
        debug_query(s.world(), "core.paths", &json!({ "units": [1] })).expect("core.paths answers");
    let one = &answer["1"];
    assert_eq!(one["state"], "following", "{answer}");
    let f = follow(&s, 1);
    let corners: Vec<Value> = f
        .corners
        .iter()
        .map(|c| json!([c.x.raw(), c.y.raw()]))
        .collect();
    assert_eq!(one["corners"], json!(corners));
    assert_eq!(one["next"], json!(f.next));
    let target = at((27, 16));
    assert_eq!(one["goal"], json!([target.x.raw(), target.y.raw()]));
    assert_eq!(
        one["orders"],
        json!([{ "move": [target.x.raw(), target.y.raw()] }])
    );
    assert_eq!(one["profile"], json!({ "class": "Tracked", "size": 1 }));

    let names = debug_query(s.world(), "core.debug_queries", &Value::Null).expect("lists");
    assert!(
        names["queries"]
            .as_array()
            .unwrap()
            .contains(&json!("core.paths"))
    );

    // Bad args are errors, never panics.
    for args in [
        Value::Null,
        json!({}),
        json!({ "units": 1 }),
        json!({ "units": ["one"] }),
        json!({ "units": [99] }),
        json!({ "units": [-1] }),
    ] {
        assert!(
            debug_query(s.world(), "core.paths", &args).is_err(),
            "{args}"
        );
    }
}

#[test]
fn scenario_setup_builds_the_hpa_graphs() {
    // A scenario's live navigation data equals what a load rebuilds (R53), HPA* included:
    // without the graphs, plans in a live match and in its loaded save would differ.
    let s = ScenarioBuilder::new().scenario("base.testbed").build();
    let world = s.world();
    let cluster_cells = world
        .resource::<fl_sim::Content>()
        .0
        .rules::<PathRules>()
        .cluster_cells;
    let built = HpaGraphs::build(world.resource::<NavGrid>(), cluster_cells);
    assert_eq!(world.get_resource::<HpaGraphs>(), Some(&built));
}

/// A 12 × 8 map with a river (y = 3..=4) and two bridges; six movers ordered across, one
/// order replaced and one stopped on the way.
fn river_movers() -> ScenarioBuilder {
    let rows = [
        "....rrrr....",
        "....r..f....",
        "..F.r.....9.",
        "~~~~=~~~~=~~",
        "~~~~=~~~~=~~",
        ".fff........",
        "....rrrrrrr.",
        "......s.....",
    ];
    let rows: Vec<String> = rows.iter().map(|&r| r.to_owned()).collect();
    let v = |x: i32, y: i32| FxVec2::new(Fx::from_int(x), Fx::from_int(y));
    ScenarioBuilder::new()
        .ascii_map(&rows_of(&rows), CELL_M)
        .player("t.red", 0)
        .player("t.blue", 1)
        .unit("t.tank", 0, 6, 2)
        .unit("t.truck", 0, 18, 26)
        .unit("t.rifleman", 0, 30, 6)
        .unit("t.tank", 1, 40, 30)
        .unit("t.rifleman", 1, 10, 30)
        .unit("t.truck", 1, 46, 2)
        .command(0, 0, &[1, 2, 3], MoveCmd { target: v(18, 30) })
        .command(3, 1, &[4, 6], MoveCmd { target: v(38, 2) })
        .command(10, 1, &[5], MoveCmd { target: v(18, 2) })
        .command(40, 0, &[1, 2], MoveCmd { target: v(46, 30) })
        .command(60, 1, &[6], StopCmd)
}

#[test]
fn smoke_pathing_twin_and_shuffle() {
    assert_deterministic(river_movers, 300);
    assert_shuffle_invariant(river_movers, 300);
}
