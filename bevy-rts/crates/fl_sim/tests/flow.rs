//! Flow fields for group moves (ADR-0015, Task 10): a non-queued move of at least
//! `Path.flow_min_group` same-profile units makes one group request, charged the cold cost of
//! its field; the members follow the field per (destination cluster, profile) to their
//! formation slots; the field cache is outcome-neutral (cold, warm and overflowing caches give
//! the same hashes, R84); a terrain change rebuilds the field; formation slots keep a minimum
//! spacing (Task 9 review Minor 2).

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};

use fl_content::{ContentDb, MoveClass};
use fl_math::{Fx, FxVec2};
use fl_protocol::{DebugSetTerrainCmd, MoveCmd, SimId, StopCmd, Tick};
use fl_sim::nav::{
    CellXY, ClusterId, FlowCache, FlowDir, FlowWork, HpaGraphs, MoveProfile, NavGrid, PathRules,
    SearchStats, content_profiles, dijkstra_seeded,
};
use fl_sim::{
    CellRect, FlowKey, MapInfo, MoveOrder, MovementRules, Order, PathFollow, PathQueue, PathState,
    RequestKind, SimCounters, SimIndex, TerrainGrid, formation_targets, space_slots,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, ascii_terrain_content, assert_branch, assert_deterministic,
    assert_shuffle_invariant,
};
use fl_view::JournalEntry;
use proptest::prelude::*;
use serde_json::json;

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

/// The base rules with `from` replaced by `to`.
fn rules_with(from: &str, to: &str) -> String {
    rules_edited(&[(from, to)])
}

/// The base rules with each `(from, to)` replaced in turn (`from` must occur, and the rules
/// must change).
fn rules_edited(edits: &[(&str, &str)]) -> String {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron");
    let base = std::fs::read_to_string(&path).expect("the base rules");
    let mut rules = base.clone();
    for &(from, to) in edits {
        assert!(rules.contains(from), "the base rules name {from}");
        rules = rules.replace(from, to);
    }
    assert_ne!(rules, base, "the edits change the base rules");
    rules
}

/// Player 0 (`t.red`) with `(unit, cell)` units on the ASCII map `rows`, in order (#1 first).
fn on_map(rows: &[String], units: &[(&str, CellXY)]) -> ScenarioBuilder {
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows_of(rows), CELL_M)
        .player("t.red", 0);
    for &(unit, c) in units {
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

/// The target of #`id`'s one move order.
fn slot(s: &Scenario, id: u32) -> FxVec2 {
    match orders(s, id)[..] {
        [Order::Move { target }] => target,
        ref other => panic!("#{id}: one move, not {other:?}"),
    }
}

fn pos(s: &Scenario, id: u32) -> FxVec2 {
    s.pose(id).expect("alive").pos
}

fn cell_of(s: &Scenario, id: u32) -> CellXY {
    s.world().resource::<NavGrid>().cell_of(pos(s, id))
}

fn entries<'a>(s: &'a Scenario, kind: &str) -> Vec<&'a JournalEntry> {
    s.journal().iter().filter(|e| e.kind == kind).collect()
}

/// The terrain id of cell `c`.
fn terrain(s: &Scenario, c: CellXY) -> String {
    s.world()
        .resource::<TerrainGrid>()
        .terrain_at(c.0, c.1)
        .unwrap_or("open")
        .to_owned()
}

/// The ids of the units on a flow field.
fn on_flow(s: &Scenario, ids: impl IntoIterator<Item = u32>) -> Vec<u32> {
    ids.into_iter()
        .filter(|&id| matches!(follow(s, id).state, PathState::Flow { .. }))
        .collect()
}

/// `n` cells 2 apart (8 m) from `from` eastward, in rows of four going north.
fn block(from: CellXY, n: u32) -> Vec<CellXY> {
    (0..n)
        .map(|i| (from.0 + 2 * (i % 4), from.1 + 2 * (i / 4)))
        .collect()
}

// --- the river scenario -------------------------------------------------------------------

/// A 48 × 48 map with a river at y = 22..=25 crossed by two 2-cell bridges, A at x = 6..=7
/// and B at x = 40..=41.
fn river() -> Vec<String> {
    draw(48, 48, |x, y| match (x, y) {
        (6..=7 | 40..=41, 22..=25) => '=',
        (_, 22..=25) => '~',
        _ => '.',
    })
}

const RIVER_ROWS: std::ops::RangeInclusive<u32> = 22..=25;
const BRIDGE_A: std::ops::RangeInclusive<u32> = 6..=7;
const BRIDGE_B: std::ops::RangeInclusive<u32> = 40..=41;
/// The group's target: north of the river, above bridge A's side.
const RIVER_GOAL: CellXY = (11, 40);

/// Eight tanks (#1–#8) 8 m apart south of the river, nearer bridge A, ordered at tick 0 across
/// it.
fn river_group() -> ScenarioBuilder {
    let tanks: Vec<(&str, CellXY)> = block((8, 4), 8)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    let ids: Vec<u32> = (1..=8).collect();
    on_map(&river(), &tanks).command(0, 0, &ids, move_to(RIVER_GOAL))
}

/// [`river_group`] with bridge A turned to water at tick 10, before any tank reaches it.
fn river_group_bridge_lost() -> ScenarioBuilder {
    river_group().command(
        10,
        0,
        &[],
        DebugSetTerrainCmd {
            min_cell: (*BRIDGE_A.start(), *RIVER_ROWS.start()),
            max_cell: (*BRIDGE_A.end(), *RIVER_ROWS.end()),
            terrain: "t.water".to_owned(),
        },
    )
}

#[test]
fn group_move_uses_one_flow_request() {
    let mut s = river_group().build();
    s.run_ticks(1);
    let requests = entries(&s, "core.path_request");
    assert_eq!(requests.len(), 1, "{requests:?}");
    let request = requests[0];
    assert_eq!(request.data["kind"], "group");
    assert_eq!(request.sim_id, Some(SimId(1)));
    assert_eq!(request.data["units"], json!([1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(
        request.data["profile"],
        json!({ "class": "Tracked", "size": 1 })
    );
    let results = entries(&s, "core.path_result");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].data["status"], "flow");
    let charge = results[0].data["charge"].as_u64().expect("a charge");
    assert!(charge > 0, "the integration settles nodes");
    assert_eq!(
        s.world().resource::<SimCounters>().get("path.budget_spent"),
        charge
    );
    for id in 1..=8 {
        let f = follow(&s, id);
        assert!(
            matches!(f.state, PathState::Flow { key } if key.profile == TANK),
            "#{id}: {f:?}"
        );
        assert_eq!(f.goal, slot(&s, id), "#{id} heads for its own slot");
        assert!(f.corners.is_empty());
    }
    let PathState::Flow { key } = follow(&s, 1).state else {
        unreachable!()
    };
    let hpa = s.world().resource::<HpaGraphs>();
    let graph = hpa.graph(TANK).expect("the tank graph");
    assert_eq!(key.cluster, graph.cluster_of(RIVER_GOAL));
}

#[test]
fn small_group_uses_single_requests() {
    let tanks: Vec<(&str, CellXY)> = block((8, 4), 5)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    let mut s = on_map(&river(), &tanks)
        .command(0, 0, &[1, 2, 3, 4, 5], move_to(RIVER_GOAL))
        .build();
    s.run_ticks(1);
    let requests = entries(&s, "core.path_request");
    assert_eq!(requests.len(), 5);
    assert!(requests.iter().all(|e| e.data["kind"] == "single"));
    let mut units: Vec<u32> = requests
        .iter()
        .map(|e| e.sim_id.expect("a unit").0)
        .collect();
    units.sort_unstable();
    assert_eq!(units, [1, 2, 3, 4, 5]);
    assert!(
        entries(&s, "core.path_result")
            .iter()
            .all(|e| e.data["status"] != "flow")
    );
}

#[test]
fn mixed_profiles_split() {
    // Six tanks (#1–#6), then six riflemen (#7–#12), and one truck (#13): two groups, one
    // single request.
    let mut units: Vec<(&str, CellXY)> = block((8, 2), 6)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    units.extend(block((8, 8), 6).into_iter().map(|c| ("t.rifleman", c)));
    units.push(("t.truck", (20, 2)));
    let ids: Vec<u32> = (1..=13).collect();
    let mut s = on_map(&river(), &units)
        .command(0, 0, &ids, move_to(RIVER_GOAL))
        .build();
    s.run_ticks(1);
    let mut requests: Vec<&JournalEntry> = entries(&s, "core.path_request");
    requests.sort_by_key(|e| e.data["request"].as_u64());
    let summary: Vec<(u64, String, String)> = requests
        .iter()
        .map(|e| {
            (
                e.data["request"].as_u64().expect("an id"),
                e.data["kind"].as_str().expect("a kind").to_owned(),
                e.data["profile"]["class"]
                    .as_str()
                    .expect("a class")
                    .to_owned(),
            )
        })
        .collect();
    // Groups first, in profile order (Foot before Tracked), then the single requests.
    assert_eq!(
        summary,
        [
            (0, "group".to_owned(), "Foot".to_owned()),
            (1, "group".to_owned(), "Tracked".to_owned()),
            (2, "single".to_owned(), "Wheeled".to_owned()),
        ]
    );
    assert_eq!(requests[0].data["units"], json!([7, 8, 9, 10, 11, 12]));
    assert_eq!(requests[1].data["units"], json!([1, 2, 3, 4, 5, 6]));
    let flow_results = entries(&s, "core.path_result")
        .into_iter()
        .filter(|e| e.data["status"] == "flow")
        .count();
    assert_eq!(flow_results, 2);
}

#[test]
fn queued_group_move_uses_single_requests() {
    // Shift-queued, a move becomes current only for units without orders: each asks alone.
    let tanks: Vec<(&str, CellXY)> = block((8, 4), 8)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    let ids: Vec<u32> = (1..=8).collect();
    let mut s = on_map(&river(), &tanks).build();
    s.issue(0, &ids, move_to(RIVER_GOAL), true);
    s.run_ticks(1);
    let requests = entries(&s, "core.path_request");
    assert_eq!(requests.len(), 8);
    assert!(requests.iter().all(|e| e.data["kind"] == "single"));
}

/// Run `s` until every unit of `ids` has no orders left (at most `max` ticks), checking every
/// tick that none stands on water; returns, per unit, the bridges it was seen on (`'A'`,
/// `'B'`).
fn run_crossing(s: &mut Scenario, ids: &[u32], max: u32) -> Vec<BTreeSet<char>> {
    let mut bridges = vec![BTreeSet::new(); ids.len()];
    for _ in 0..max {
        s.run_ticks(1);
        for (i, &id) in ids.iter().enumerate() {
            let c = cell_of(s, id);
            assert_ne!(
                terrain(s, c),
                "t.water",
                "tick {}: #{id} on water at {c:?}",
                s.tick().0
            );
            if RIVER_ROWS.contains(&c.1) {
                if BRIDGE_A.contains(&c.0) {
                    bridges[i].insert('A');
                }
                if BRIDGE_B.contains(&c.0) {
                    bridges[i].insert('B');
                }
            }
        }
        if ids.iter().all(|&id| orders(s, id).is_empty()) {
            break;
        }
    }
    bridges
}

#[test]
fn flow_units_reach_their_slots() {
    let mut s = river_group().build();
    s.run_ticks(1);
    let ids: Vec<u32> = (1..=8).collect();
    let slots: Vec<FxVec2> = ids.iter().map(|&id| slot(&s, id)).collect();
    let bridges = run_crossing(&mut s, &ids, 899);
    assert!(s.tick() <= Tick(900), "arrived by tick 900");
    for (i, &id) in ids.iter().enumerate() {
        assert!(orders(&s, id).is_empty(), "#{id} still has orders");
        let off = (pos(&s, id) - slots[i]).length();
        assert!(off <= Fx::from_int(2), "#{id} is {off} m from its slot");
        assert_eq!(
            bridges[i],
            BTreeSet::from(['A']),
            "#{id} crossed by the near bridge"
        );
    }
}

#[test]
fn flow_invalidated_by_terrain_change() {
    let mut s = river_group_bridge_lost().build();
    s.run_ticks(1);
    let ids: Vec<u32> = (1..=8).collect();
    let slots: Vec<FxVec2> = ids.iter().map(|&id| slot(&s, id)).collect();
    s.run_ticks(10);
    assert_eq!(terrain(&s, (6, 23)), "t.water", "bridge A is gone");
    assert!(
        on_flow(&s, ids.iter().copied()).len() == ids.len(),
        "the group stays on its field: the next reads rebuild it"
    );
    let bridges = run_crossing(&mut s, &ids, 2000);
    for (i, &id) in ids.iter().enumerate() {
        assert!(orders(&s, id).is_empty(), "#{id} still has orders");
        let off = (pos(&s, id) - slots[i]).length();
        assert!(off <= Fx::from_int(2), "#{id} is {off} m from its slot");
        assert_eq!(
            bridges[i],
            BTreeSet::from(['B']),
            "#{id} crossed by bridge B"
        );
    }
}

/// Run `make` for `ticks` ticks: its hash stream and the sum of counter `name` over the run.
fn run_summing(make: ScenarioBuilder, ticks: u32, name: &str) -> (Vec<(Tick, u64)>, u64) {
    let mut s = make.build();
    let mut sum = 0;
    for _ in 0..ticks {
        s.run_ticks(1);
        sum += s.world().resource::<SimCounters>().get(name);
    }
    (s.hashes(), sum)
}

#[test]
fn cache_does_not_change_outcomes() {
    let (warm, warm_nodes) =
        run_summing(river_group_bridge_lost(), 600, "flowfield.abstract_nodes");
    let (cold, cold_nodes) = run_summing(
        river_group_bridge_lost().flow_cache_cold(true),
        600,
        "flowfield.abstract_nodes",
    );
    assert_eq!(warm.len(), 600);
    assert!(warm == cold, "a cold flow cache changed the hashes");
    // Not vacuous: the cold run rebuilt what the warm one reused.
    assert!(
        cold_nodes > warm_nodes,
        "cold {cold_nodes} vs warm {warm_nodes} abstract nodes built"
    );
}

/// Members per group of [`flow_groups`] (its `Path.flow_min_group`).
const GROUP_UNITS: u32 = 2;

/// `groups` flow fields followed at once (at most 60): a 64 × 64 grass map in 8-cell clusters
/// (8 × 8 of them) with two ponds (cells x 14–19, y 44–50 and x 38–42, y 14–20) for the fields
/// to route around, `Path.flow_min_group` 2, `Path.flow_cache_max` `max` and
/// `Path.flow_cache_hard_max` `hard_max`. Group `g` is two tanks, member `m` at cell
/// `(2 + 2 (g % 30), 28 + 4 (g / 30) + 2 m)`, spawned member by member (the first of every
/// group, then the second), so consecutive `SimId`s are of different groups and the flow reads
/// cycle through the keys; member `m` of group `g` is `#(1 + m · groups + g)`. At tick 0 group
/// `g` is ordered (one `core.move`) to the centre of cluster `(g / 8, g % 8)`, never the one it
/// starts in: one destination cluster, so one flow field, per group.
fn flow_groups(groups: u32, max: u32, hard_max: u32) -> ScenarioBuilder {
    assert!(groups <= 60, "at most 60 groups fit the rows");
    let rows = draw(64, 64, |x, y| {
        let pond = ((14..=19).contains(&x) && (44..=50).contains(&y))
            || ((38..=42).contains(&x) && (14..=20).contains(&y));
        if pond { '~' } else { '.' }
    });
    let mut units = Vec::new();
    for m in 0..GROUP_UNITS {
        for g in 0..groups {
            units.push(("t.tank", (2 + 2 * (g % 30), 28 + 4 * (g / 30) + 2 * m)));
        }
    }
    let rules = rules_edited(&[
        ("cluster_cells: 16,", "cluster_cells: 8,"),
        ("flow_min_group: 6,", "flow_min_group: 2,"),
        ("flow_cache_max: 32,", &format!("flow_cache_max: {max},")),
        (
            "flow_cache_hard_max: 128,",
            &format!("flow_cache_hard_max: {hard_max},"),
        ),
    ]);
    let mut b = on_map(&rows, &units).ascii_rules(&rules);
    for g in 0..groups {
        let ids: Vec<u32> = (0..GROUP_UNITS).map(|m| 1 + m * groups + g).collect();
        b = b.command(0, 0, &ids, move_to((8 * (g / 8) + 4, 8 * (g % 8) + 4)));
    }
    b
}

/// The distinct flow keys the units of `s` follow.
fn live_keys(s: &Scenario) -> BTreeSet<FlowKey> {
    let world = s.world();
    world
        .resource::<SimIndex>()
        .iter()
        .filter_map(|(_, entity)| match world.get::<PathFollow>(entity)?.state {
            PathState::Flow { key } => Some(key),
            _ => None,
        })
        .collect()
}

/// Run `s` for `ticks` ticks: per tick, `(flowfield.abstract_nodes, flowfield.cells, keys
/// live at its end)`.
fn flow_work_per_tick(s: &mut Scenario, ticks: u32) -> Vec<(u64, u64, usize)> {
    (0..ticks)
        .map(|_| {
            s.run_ticks(1);
            let counters = s.world().resource::<SimCounters>();
            (
                counters.get("flowfield.abstract_nodes"),
                counters.get("flowfield.cells"),
                live_keys(s).len(),
            )
        })
        .collect()
}

#[test]
fn flow_cache_live_keys_never_thrash() {
    // 36 live keys against Path.flow_cache_max 32 (R124): the keys read in the previous tick
    // stay, so after the tick the group requests ran no integration is built again, and no
    // tile.
    let mut s = flow_groups(36, 32, 128).build();
    let work = flow_work_per_tick(&mut s, 150);
    let last_request = entries(&s, "core.path_result")
        .into_iter()
        .filter(|e| e.data["status"] == "flow")
        .map(|e| e.tick.0)
        .max()
        .expect("the groups were planned");
    assert!(
        last_request < 10,
        "the group requests ran by tick {last_request}"
    );
    let mut roomy = flow_groups(36, 64, 256).build();
    let roomy_work = flow_work_per_tick(&mut roomy, 150);
    assert!(
        s.hashes() == roomy.hashes(),
        "the cache cap changed the hashes"
    );
    // Ticks run from 0, so work[t] is tick t.
    let after = usize::try_from(last_request).expect("a small tick") + 1;
    assert!(work.len() - after >= 100, "at least 100 ticks measured");
    let mut over_cap = 0;
    for (t, (&(nodes, cells, live), &(_, roomy_cells, _))) in
        work.iter().zip(&roomy_work).enumerate().skip(after)
    {
        assert_eq!(nodes, 0, "tick {t}: {nodes} abstract nodes rebuilt");
        // Tiles of clusters entered for the first time are built in both runs alike; any
        // more would be a rebuild.
        assert_eq!(cells, roomy_cells, "tick {t}: tile cells built");
        over_cap += usize::from(live > 32);
    }
    // Not vacuous: the live keys outnumbered the cap for 100 ticks and more.
    assert!(
        over_cap >= 100,
        "more than 32 keys live on only {over_cap} ticks"
    );
}

/// The navigation data of a 48 × 48 grass map in 4-cell clusters (12 × 12), and its first
/// `n` flow keys: [`TANK`] to every cluster in id order, then [`WIDE`].
fn grass_keys(n: usize) -> (NavGrid, HpaGraphs, Vec<FlowKey>) {
    let grass = u8::try_from(IDS.iter().position(|&id| id == "t.grass").expect("grass"))
        .expect("a small palette");
    let ground = vec![grass; (SIDE * SIDE) as usize];
    let nav = build_nav(&CONTENT, &random_terrain(&ground, &[]));
    let hpa = HpaGraphs::build(&nav, 4);
    let clusters = (SIDE / 4) * (SIDE / 4);
    let keys: Vec<FlowKey> = [TANK, WIDE]
        .into_iter()
        .flat_map(|profile| {
            (0..clusters).map(move |c| FlowKey {
                cluster: ClusterId(c),
                profile,
            })
        })
        .take(n)
        .collect();
    assert_eq!(keys.len(), n, "the map has {n} keys");
    (nav, hpa, keys)
}

/// Read every key of `keys` from `cache`: its cold cost, and the direction from cell (1, 1),
/// which builds that tile. The cold costs.
fn read_all(
    cache: &mut FlowCache,
    keys: &[FlowKey],
    nav: &NavGrid,
    hpa: &HpaGraphs,
    work: &mut FlowWork,
) -> Vec<u32> {
    keys.iter()
        .map(|&key| {
            let cost = cache.cold_cost(key, nav, hpa, work);
            assert!(cache.direction(key, (1, 1), nav, hpa, work).is_some());
            cost
        })
        .collect()
}

#[test]
fn flow_cache_evicts_only_at_tick_start() {
    let (nav, hpa, keys) = grass_keys(6);
    let mut cache = FlowCache::new(2, 8, false);
    let mut work = FlowWork::default();
    cache.begin_tick(Tick(1));
    let costs = read_all(&mut cache, &keys, &nav, &hpa, &mut work);
    assert_eq!(
        work.abstract_nodes,
        costs.iter().sum::<u32>(),
        "each integration built once"
    );
    assert_eq!(
        cache.len(),
        6,
        "a read never evicts, even over the cap of 2"
    );
    // Read again in the same tick: nothing was evicted, so nothing is built again.
    let built = work;
    read_all(&mut cache, &keys, &nav, &hpa, &mut work);
    assert_eq!(
        work, built,
        "a key read this tick is still cached this tick"
    );

    // All six were read in tick 1, so they stay at the start of tick 2 (6 is within the hard
    // max of 8). Tick 2 reads keys 4 and 5 only.
    cache.begin_tick(Tick(2));
    assert_eq!(
        cache.len(),
        6,
        "the keys read in the previous tick stay above the cap"
    );
    read_all(&mut cache, &keys[4..], &nav, &hpa, &mut work);
    assert_eq!(work, built, "nothing evicted, nothing rebuilt");

    // Tick 3 starts: keys 0 to 3 were not read in tick 2, so they go, down to the cap.
    cache.begin_tick(Tick(3));
    assert_eq!(cache.len(), 2);
    read_all(&mut cache, &keys[4..], &nav, &hpa, &mut work);
    assert_eq!(work, built, "keys 4 and 5 were kept");
    // An evicted key is rebuilt on its next read (R84), with the same cold cost.
    assert_eq!(
        read_all(&mut cache, &keys[..1], &nav, &hpa, &mut work),
        costs[..1]
    );
    assert_eq!(
        work.abstract_nodes,
        built.abstract_nodes + costs[0],
        "key 0 was evicted"
    );

    // Keys 0, 4 and 5 were all read in tick 3, so all three stay at the start of tick 4.
    // Tick 4 reads key 5 only; at the start of tick 5, keys 0 and 4 (both last read in tick 3)
    // are over the cap, and the lower key, key 0, goes first.
    cache.begin_tick(Tick(4));
    assert_eq!(cache.len(), 3);
    read_all(&mut cache, &keys[5..], &nav, &hpa, &mut work);
    cache.begin_tick(Tick(5));
    assert_eq!(cache.len(), 2);
    let before = work;
    read_all(&mut cache, &keys[4..], &nav, &hpa, &mut work);
    assert_eq!(
        work, before,
        "keys 4 and 5 were kept: key order breaks the LRU tie"
    );
}

#[test]
fn flow_cache_hard_max_applies_without_a_cap() {
    // A cache that keeps every entry between ticks (max 0) still has its hard ceiling.
    let (nav, hpa, keys) = grass_keys(5);
    let mut cache = FlowCache::new(0, 3, false);
    let mut work = FlowWork::default();
    cache.begin_tick(Tick(1));
    read_all(&mut cache, &keys, &nav, &hpa, &mut work);
    assert_eq!(cache.len(), 5, "a read never evicts");
    cache.begin_tick(Tick(2));
    assert_eq!(cache.len(), 3, "the hard max holds without a cap");
    cache.begin_tick(Tick(3));
    assert_eq!(cache.len(), 3, "no cap: nothing more goes");
}

#[test]
fn flow_cache_hard_max_bounds_memory() {
    // 200 one-off keys, 25 read per tick: the keys of the previous tick outnumber the cap
    // (4), and the hard max (16) bounds what the cache holds at every tick start.
    let (nav, hpa, keys) = grass_keys(200);
    let (max, hard_max) = (4, 16);
    let mut cache = FlowCache::new(max, hard_max, false);
    let mut work = FlowWork::default();
    let mut starts = Vec::new();
    for (t, batch) in (1..).zip(keys.chunks(25)) {
        cache.begin_tick(Tick(t));
        let start = cache.len();
        starts.push(start);
        read_all(&mut cache, batch, &nav, &hpa, &mut work);
        assert_eq!(
            cache.len(),
            start + batch.len(),
            "tick {t}: reads only insert"
        );
    }
    let hard = usize::try_from(hard_max).expect("a small max");
    assert_eq!(starts.len(), 8);
    assert!(
        starts.iter().all(|&n| n <= hard),
        "entries at the tick starts: {starts:?}"
    );
    // Not vacuous: the keys of the previous tick held the cache above the cap of 4, and the
    // hard max cut them.
    assert_eq!(
        starts[1..],
        [hard; 7],
        "entries at the tick starts: {starts:?}"
    );
    // A tick without reads: at the next start no entry was read in the previous tick, so the
    // cache drops to the cap.
    cache.begin_tick(Tick(9));
    assert_eq!(cache.len(), hard);
    cache.begin_tick(Tick(10));
    assert_eq!(cache.len(), usize::try_from(max).expect("a small max"));
}

#[test]
fn flow_cache_overflow_does_not_change_outcomes() {
    // R84, R124: 60 group fields followed at once, more than flow_cache_max (32).
    let mut s = flow_groups(60, 32, 128).build();
    s.run_ticks(5);
    assert_eq!(live_keys(&s).len(), 60, "sixty fields in use at once");
    assert_eq!(
        s.world().resource::<FlowCache>().len(),
        60,
        "the live keys stay above the cap"
    );
    drop(s);

    // Room for every key: each integration is built exactly once, so the abstract nodes
    // built are the sixty group charges (the settled counts).
    let roomy = run_full(flow_groups(60, 64, 256), 600);
    let charges: u64 = roomy.flow_charges.iter().map(|&(_, c)| c).sum();
    assert_eq!(roomy.flow_charges.len(), 60, "{:?}", roomy.flow_charges);
    assert_eq!(roomy.abstract_nodes, charges, "built once each");
    for (what, run) in [
        // Over the cap: the live keys are kept, still built once each (R124).
        ("capped", run_full(flow_groups(60, 32, 128), 600)),
        // A hard max of 2 evicts live keys at every tick start: reads rebuild them.
        ("overflowing", run_full(flow_groups(60, 2, 2), 600)),
        (
            "cold",
            run_full(flow_groups(60, 64, 256).flow_cache_cold(true), 600),
        ),
    ] {
        assert!(
            run.hashes == roomy.hashes,
            "a {what} flow cache changed the hashes"
        );
        // Charges are cold costs: the same whatever the cache held (never a re-charge).
        assert_eq!(
            run.flow_charges, roomy.flow_charges,
            "{what}: group charges"
        );
        assert_eq!(run.budget_spent, roomy.budget_spent, "{what}: budget spent");
        if what == "capped" {
            assert_eq!(run.abstract_nodes, charges, "capped: built once each");
        } else {
            // Reads rebuilt what eviction dropped: more integrations than one per key.
            assert!(
                run.abstract_nodes > charges,
                "{what}: {} abstract nodes built, the keys settle {charges}",
                run.abstract_nodes
            );
        }
    }
}

/// What [`run_full`] measured.
struct Measured {
    hashes: Vec<(Tick, u64)>,
    /// `"flowfield.abstract_nodes"` summed over the run.
    abstract_nodes: u64,
    /// `"path.budget_spent"` summed over the run.
    budget_spent: u64,
    /// `(request, charge)` of every `"flow"` path result, in journal order.
    flow_charges: Vec<(u64, u64)>,
}

/// Run `make` for `ticks` ticks and measure it.
fn run_full(make: ScenarioBuilder, ticks: u32) -> Measured {
    let mut s = make.build();
    let (mut abstract_nodes, mut budget_spent) = (0, 0);
    for _ in 0..ticks {
        s.run_ticks(1);
        let counters = s.world().resource::<SimCounters>();
        abstract_nodes += counters.get("flowfield.abstract_nodes");
        budget_spent += counters.get("path.budget_spent");
    }
    let flow_charges = entries(&s, "core.path_result")
        .into_iter()
        .filter(|e| e.data["status"] == "flow")
        .map(|e| {
            (
                e.data["request"].as_u64().expect("an id"),
                e.data["charge"].as_u64().expect("a charge"),
            )
        })
        .collect();
    Measured {
        hashes: s.hashes(),
        abstract_nodes,
        budget_spent,
        flow_charges,
    }
}

/// [`river_group`]'s tanks plus a truck (#9) with `Path.budget_per_tick` 1: the truck, ordered
/// alone at tick 0 before the group, spends the budget with its single request, so the group
/// request waits for tick 1.
fn group_behind_budget() -> ScenarioBuilder {
    let mut units: Vec<(&str, CellXY)> = block((8, 4), 8)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    units.push(("t.truck", (30, 4)));
    let ids: Vec<u32> = (1..=8).collect();
    on_map(&river(), &units)
        .ascii_rules(&rules_with("budget_per_tick: 8000,", "budget_per_tick: 1,"))
        .command(0, 0, &[9], move_to((30, 10)))
        .command(0, 0, &ids, move_to(RIVER_GOAL))
}

#[test]
fn group_request_waits_for_budget() {
    let mut s = group_behind_budget().build();
    s.run_ticks(1);
    let pending = &s.world().resource::<PathQueue>().pending;
    assert_eq!(pending.len(), 1, "the group request is held back");
    assert!(matches!(pending[0].kind, RequestKind::Group { .. }));
    for id in 1..=8 {
        assert!(
            matches!(follow(&s, id).state, PathState::Waiting { .. }),
            "#{id} waits"
        );
    }
    s.run_ticks(1);
    let result = entries(&s, "core.path_result")
        .into_iter()
        .find(|e| e.data["status"] == "flow")
        .expect("the group was processed");
    assert_eq!(result.tick, Tick(1));
    assert_eq!(result.data["waited"], 1);
    assert_eq!(on_flow(&s, 1..=8).len(), 8);
}

#[test]
fn branch_with_queued_group_request() {
    // Saved after tick 0 with the group request still queued: a Hashed `Group` in the save.
    let mut s = group_behind_budget().build();
    s.run_ticks(1);
    assert_eq!(s.world().resource::<PathQueue>().pending.len(), 1);
    drop(s);
    assert_branch(group_behind_budget, 1, 201);
}

#[test]
fn group_without_a_graph_is_journaled_single() {
    // A group whose profile has no HPA* graph gets no flow field: its members ask alone, and
    // the journal says so ("single", charge 0), never "flow".
    let mut s = river_group().build();
    let content = Arc::clone(&s.world().resource::<fl_sim::Content>().0);
    let foot = MoveProfile {
        class: MoveClass::Foot,
        size: 1,
    };
    let foot_only = NavGrid::build(
        s.world().resource::<TerrainGrid>(),
        &content,
        content.rules::<MovementRules>(),
        &[foot],
        content.rules::<PathRules>().max_clearance,
        Fx::from_int(CELL_M),
    );
    s.world_mut()
        .insert_resource(HpaGraphs::build(&foot_only, CLUSTER));
    s.run_ticks(1);
    let mut results: Vec<&JournalEntry> = entries(&s, "core.path_result");
    results.sort_by_key(|e| e.data["request"].as_u64());
    assert_eq!(results.len(), 9, "the group, then 8 single requests");
    assert_eq!(results[0].data["status"], "single");
    assert_eq!(results[0].data["charge"], 0);
    assert!(results.iter().all(|e| e.data["status"] != "flow"));
    assert!(on_flow(&s, 1..=8).is_empty());
}

#[test]
fn branch_mid_flow_follow() {
    let n = 40;
    let mut s = river_group().build();
    s.run_ticks(n);
    assert!(
        on_flow(&s, 1..=8).len() >= 4,
        "at least 4 units on the field at tick {n}"
    );
    drop(s);
    assert_branch(river_group, n, n + 200);
    assert_branch(river_group_bridge_lost, n, n + 200);
}

#[test]
fn flow_counters_recorded() {
    let mut s = river_group().build();
    s.run_ticks(1);
    let counters = s.world().resource::<SimCounters>();
    assert!(
        counters.get("flowfield.abstract_nodes") > 0,
        "on the request tick"
    );
    let mut cells_while_moving = false;
    for _ in 0..400 {
        s.run_ticks(1);
        if !on_flow(&s, 1..=8).is_empty()
            && s.world().resource::<SimCounters>().get("flowfield.cells") > 0
        {
            cells_while_moving = true;
            break;
        }
    }
    assert!(cells_while_moving, "a tile is built on the way");
}

#[test]
fn stale_group_request_is_discarded() {
    let tanks: Vec<(&str, CellXY)> = block((8, 4), 6)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    let ids: Vec<u32> = (1..=6).collect();
    let mut s = on_map(&river(), &tanks).build();
    s.issue(0, &ids, move_to(RIVER_GOAL), false);
    s.issue(0, &ids, StopCmd, false);
    s.run_ticks(1);
    let results = entries(&s, "core.path_result");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].data["status"], "stale");
    assert_eq!(results[0].data["charge"], 0);
    for id in ids {
        assert_eq!(follow(&s, id).state, PathState::Idle);
    }
}

#[test]
fn members_that_left_are_skipped() {
    let tanks: Vec<(&str, CellXY)> = block((8, 4), 6)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    let ids: Vec<u32> = (1..=6).collect();
    let mut s = on_map(&river(), &tanks).build();
    s.issue(0, &ids, move_to(RIVER_GOAL), false);
    s.issue(0, &[2, 5], StopCmd, false);
    s.run_ticks(1);
    let results = entries(&s, "core.path_result");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].data["status"], "flow");
    assert_eq!(on_flow(&s, ids.iter().copied()), [1, 3, 4, 6]);
    assert_eq!(follow(&s, 2).state, PathState::Idle);
}

#[test]
fn unrouted_member_gets_a_single_request() {
    // #6 stands on a grass cell ringed by water: no route, so a single request at the back.
    let rows = draw(48, 48, |x, y| match (x, y) {
        (29, 5) => '.',
        (28..=30, 4..=6) => '~',
        _ => '.',
    });
    let mut tanks: Vec<(&str, CellXY)> = block((8, 4), 5)
        .into_iter()
        .map(|c| ("t.tank", c))
        .collect();
    tanks.push(("t.tank", (29, 5)));
    let mut s = on_map(&rows, &tanks)
        .command(0, 0, &[1, 2, 3, 4, 5, 6], move_to((11, 40)))
        .build();
    s.run_ticks(1);
    let mut requests: Vec<&JournalEntry> = entries(&s, "core.path_request");
    requests.sort_by_key(|e| e.data["request"].as_u64());
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].data["kind"], "group");
    assert_eq!(requests[1].data["kind"], "single");
    assert_eq!(requests[1].sim_id, Some(SimId(6)));
    assert_eq!(on_flow(&s, 1..=6), [1, 2, 3, 4, 5]);
    let single = entries(&s, "core.path_result")
        .into_iter()
        .find(|e| e.sim_id == Some(SimId(6)))
        .expect("#6's single request was processed");
    assert_eq!(single.data["status"], "partial");
}

#[test]
fn smoke_flow_twin_and_shuffle() {
    assert_deterministic(river_group_bridge_lost, 300);
    assert_shuffle_invariant(river_group_bridge_lost, 300);
}

// --- formation slot spacing (Task 9 review Minor 2) ---------------------------------------

#[test]
fn space_slots_pinned() {
    let map = MapInfo::flat(
        FxVec2::new(Fx::from_int(1000), Fx::from_int(1000)),
        Fx::from_int(CELL_M),
    );
    let v = |x: i32, y: i32| FxVec2::new(Fx::from_int(x), Fx::from_int(y));
    let two = Fx::from_int(2);
    // The second slot moves one pitch south (ring 1 tries (0, −1) first); the third, 1 m from
    // the first, finds (0, −1) and (−1, 0) taken and takes (+1, 0).
    assert_eq!(
        space_slots(&[v(100, 100), v(100, 100), v(101, 100)], two, &map),
        [v(100, 100), v(100, 98), v(103, 100)]
    );
    // Slots already far enough apart stay; a zero pitch changes nothing.
    let apart = [v(100, 100), v(103, 100)];
    assert_eq!(space_slots(&apart, two, &map), apart);
    let same = [v(5, 5), v(5, 5)];
    assert_eq!(space_slots(&same, Fx::ZERO, &map), same);
}

/// Panics unless every two of `slots` are at least `pitch` apart.
fn assert_spaced(slots: &[FxVec2], pitch: Fx) {
    for (i, &a) in slots.iter().enumerate() {
        for &b in &slots[i + 1..] {
            assert!(
                (a - b).length_sq() >= pitch * pitch,
                "{a:?} and {b:?} are {} m apart, under {pitch}",
                (a - b).length()
            );
        }
    }
}

#[test]
fn spread_group_slots_keep_minimum_spacing() {
    // Two clumps of 128 a kilometre apart: scaled into a 40 m spread, each clump collapses
    // onto nearly one point; spacing keeps every slot a pitch apart.
    let map = MapInfo::flat(
        FxVec2::new(Fx::from_int(4000), Fx::from_int(4000)),
        Fx::from_int(CELL_M),
    );
    let positions: Vec<FxVec2> = (0..256)
        .map(|i| {
            let clump = if i < 128 { 500 } else { 1500 };
            FxVec2::new(
                Fx::from_int(clump + (i % 16) * 3),
                Fx::from_int(500 + (i / 16) * 3),
            )
        })
        .collect();
    let target = FxVec2::new(Fx::from_int(2000), Fx::from_int(2000));
    let slots = formation_targets(&positions, target, Fx::from_int(40), &map);
    let pitch = Fx::from_ratio(13, 2);
    let spaced = space_slots(&slots, pitch, &map);
    assert_spaced(&spaced, pitch);
    // The spacing keeps the group together: every slot within √n × pitch of the target.
    let reach = pitch.mul_int(17) + Fx::from_int(40);
    for s in &spaced {
        assert!((*s - target).length() <= reach, "{s:?}");
    }
}

#[test]
fn move_command_spaces_formation_slots() {
    // 64 tanks packed 2 m apart (closer than a hull): the move's slots are a pitch apart,
    // the largest footprint side (6 m) plus Steering.separation_margin_m (0.5 m).
    let rows = draw(64, 64, |_, _| '.');
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows_of(&rows), CELL_M)
        .player("t.red", 0);
    for i in 0..64 {
        b = b.unit("t.tank", 0, 40 + (i % 8) * 2, 40 + (i / 8) * 2);
    }
    let ids: Vec<u32> = (1..=64).collect();
    let mut s = b.command(0, 0, &ids, move_to((40, 40))).build();
    s.run_ticks(1);
    let slots: Vec<FxVec2> = ids.iter().map(|&id| slot(&s, id)).collect();
    assert_spaced(&slots, Fx::from_ratio(13, 2));
}

// --- the flow field itself (proptest) ------------------------------------------------------

/// The content of the ASCII mod; the proptest draws its own terrain over its kinds.
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
const SIDE: u32 = 48;

/// A 48 × 48 map of mixed ground (grass, road, forest) with water rectangles.
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

fn cells(nav: &NavGrid) -> impl Iterator<Item = CellXY> {
    let (w, h) = nav.cells();
    (0..h).flat_map(move |y| (0..w).map(move |x| (x, y)))
}

/// Check the field of `(dest, p)` against a full-map Dijkstra from the destination cluster's
/// passable cells (see the test).
fn check_field(nav: &NavGrid, p: MoveProfile, dest: ClusterId) -> Result<(), TestCaseError> {
    let hpa = HpaGraphs::build(nav, CLUSTER);
    let graph = hpa.graph(p).expect("a graph per content profile");
    let rect = graph.cluster_rect(dest);
    let seeds: Vec<(CellXY, u32)> = cells(nav)
        .filter(|&(x, y)| {
            (rect.min.0..=rect.max.0).contains(&x)
                && (rect.min.1..=rect.max.1).contains(&y)
                && nav.passable(p, i64::from(x), i64::from(y))
        })
        .map(|c| (c, 0))
        .collect();
    let (w, h) = nav.cells();
    let whole = CellRect {
        min: (0, 0),
        max: (w - 1, h - 1),
    };
    let reach = dijkstra_seeded(nav, p, &seeds, whole, &mut SearchStats::default());
    let key = FlowKey {
        cluster: dest,
        profile: p,
    };
    let mut cache = FlowCache::new(0, 0, false);
    let mut work = FlowWork::default();
    let total = usize::try_from(w * h).expect("a small map");
    for start in cells(nav) {
        let dir = cache.direction(key, start, nav, &hpa, &mut work);
        prop_assert_eq!(
            dir.is_some(),
            reach.cost(start).is_some(),
            "cell {:?}: direction {:?}",
            start,
            dir
        );
        let mut at = start;
        let mut seen = BTreeSet::from([start]);
        let mut dir = dir;
        while let Some(FlowDir::Toward(next)) = dir {
            prop_assert!(
                seen.insert(next),
                "from {:?}: {:?} visited twice",
                start,
                next
            );
            prop_assert!(seen.len() <= total);
            prop_assert!(
                at.0.abs_diff(next.0) <= 1 && at.1.abs_diff(next.1) <= 1,
                "{:?} → {:?} is not a step",
                at,
                next
            );
            if at.0 != next.0 && at.1 != next.1 {
                // The corner rule: an off-centre unit crosses a diagonal's side cells.
                let side = |x: u32, y: u32| nav.passable(p, i64::from(x), i64::from(y));
                prop_assert!(
                    side(next.0, at.1) && side(at.0, next.1),
                    "{:?} → {:?} cuts a corner",
                    at,
                    next
                );
            }
            at = next;
            dir = cache.direction(key, at, nav, &hpa, &mut work);
            prop_assert!(dir.is_some(), "from {:?}: {:?} has no direction", start, at);
        }
        if dir.is_some() {
            prop_assert_eq!(dir, Some(FlowDir::Arrived));
            prop_assert_eq!(graph.cluster_of(at), dest);
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// From every cell with a route, following `direction()` reaches the destination cluster
    /// without revisiting a cell; a cell has a direction iff a full-map Dijkstra from the
    /// destination cluster reaches it.
    #[test]
    fn flow_directions_reach_destination(
        ground in proptest::collection::vec(
            prop_oneof![Just(3u8), Just(4u8), Just(2u8)],
            (SIDE * SIDE) as usize,
        ),
        lakes in proptest::collection::vec((0..SIDE, 0..SIDE, 1u32..13, 1u32..13), 0..10),
        wide in any::<bool>(),
        dest in 0u32..9,
    ) {
        let nav = build_nav(&CONTENT, &random_terrain(&ground, &lakes));
        let p = if wide { WIDE } else { TANK };
        check_field(&nav, p, ClusterId(dest))?;
    }
}
