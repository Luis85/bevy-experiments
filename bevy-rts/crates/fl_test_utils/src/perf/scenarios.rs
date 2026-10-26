//! The scenarios of the work-counter perf gate: [`CounterSim`], a sim set up headless with
//! the commands it receives by tick. The scale mix on the 2 km terrain map or the flat, open map,
//! and three pathing workloads the mix does not exercise (group moves, more live flow fields
//! than the cache keeps, a crowd through a choke point). Every scenario builds its own sim, so
//! it starts with empty navigation caches (R86).

use std::alloc::System;
use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::World;
use fl_content::{ContentDb, Faction};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    Controller, FactionId, MoveCmd, PlayerCommand, PlayerId, SimId, TeamId, Tick, UnitTypeId,
};
use fl_sim::nav::MoveProfile;
use fl_sim::{
    CommandRegistry, MatchSetup, PlayerInfo, Players, RunConfig, ScaleScenario, SimClock,
    SpawnRequest, Wander, build_sim_app, setup_map, setup_spawn, step_tick,
};
use fl_view::{TickInput, TickOutput};
use stats_alloc::{Region, Stats, StatsAlloc};

use crate::builder::base_content;
use crate::terrain::{ascii_terrain_content, unit_cells};
/// Units of [`CounterSim::group_move`].
const GROUP_MOVE_UNITS: u32 = 300;
/// [`CounterSim::group_move`] orders every player's units every this many ticks...
const GROUP_MOVE_EVERY: u32 = 50;
/// ...this many times, from tick 0.
const GROUP_MOVE_ROUNDS: u32 = 5;
/// The quadrant centres of the 2 km scale map, in metres: south-west, south-east, north-east,
/// north-west.
const QUADRANTS: [(i32, i32); 4] = [(500, 500), (1500, 500), (1500, 1500), (500, 1500)];

/// The flow fields [`CounterSim::flow_thrash`] keeps live at once: more than
/// `Path.flow_cache_max` (32).
const THRASH_GROUPS: u32 = 36;
/// Units per group of [`CounterSim::flow_groups`] (`Path.flow_min_group`).
const FLOW_GROUP_UNITS: u32 = 6;
/// The tick [`CounterSim::flow_groups`] orders its groups on: the first tick the perf gate
/// measures (after its 40 warmup ticks), so the group requests are measured too.
const FLOW_GROUPS_TICK: u32 = 40;
/// [`CounterSim::flow_groups`]' map: this many cells a side (4 m cells), 8 × 8 clusters.
const FLOW_MAP_CELLS: u32 = 128;

/// Cell side of the ASCII maps here, in metres.
const ASCII_CELL_M: i32 = 4;
/// [`CounterSim::choke_crowd`]'s map, in cells.
const CHOKE_W: u32 = 64;
const CHOKE_H: u32 = 32;

/// Whether cell `(x, y)` of [`CounterSim::choke_crowd`]'s map is water: columns 18 to 32 are,
/// but for a funnel narrowing by a cell per column on each side, from rows 2 to 29 at column
/// 18 to the two-cell gap (rows 15 and 16) at columns 31 and 32. Its diagonal sides are
/// stairs of concave corners for the crowd to press into.
fn choke_water(x: u32, y: u32) -> bool {
    let narrowing = 31u32.saturating_sub(x);
    (18..=32).contains(&x) && (2 * y).abs_diff(31) > 1 + 2 * narrowing
}

/// A sim set up for a work-counter run, and the commands it receives, by tick.
pub struct CounterSim {
    app: App,
    commands: BTreeMap<Tick, Vec<PlayerCommand>>,
}

impl CounterSim {
    /// [`ScaleScenario::on_terrain`]`(units)`: the entity mix (followers and projectiles in
    /// flight) wandering on the 2 km terrain map through single path requests; no commands.
    ///
    /// # Panics
    /// If the sim does not build or the scenario does not set up.
    pub fn scale(units: u32) -> Self {
        CounterSim::from_scale(&ScaleScenario::on_terrain(units), false)
    }

    /// [`ScaleScenario::with_units`]`(units)`: the same entity mix on M0's flat, open map (no
    /// redraws, wandering on open ground), the workload `docs/perf/M0.json` measured; it keeps
    /// non-path work and allocations gated at M0 levels (R102, R125).
    ///
    /// # Panics
    /// As [`CounterSim::scale`].
    pub fn flat(units: u32) -> Self {
        CounterSim::from_scale(&ScaleScenario::with_units(units), false)
    }

    /// Group moves on the 2 km terrain map: [`ScaleScenario::on_terrain`]`(300)` without
    /// followers, projectiles, sensing or wandering. At ticks `50 r` for rounds `r` in 0..5,
    /// every player `p` orders all its units (one `core.move`, so one flow-field group request
    /// per movement profile) to quadrant `(p + r + 1) % 4`'s centre (south-west, south-east,
    /// north-east, north-west) moved by `80 r − 160` m on both axes, so every round's
    /// destination clusters, and its fields, are new.
    ///
    /// # Panics
    /// As [`CounterSim::scale`].
    pub fn group_move() -> Self {
        let mut sim = CounterSim::quiet_scale(GROUP_MOVE_UNITS);
        let mut orders = Orders::default();
        let groups = units_by(sim.app.world_mut(), |owner, _| owner);
        for round in 0..GROUP_MOVE_ROUNDS {
            let shift = i32::try_from(round).expect("few rounds") * 80 - 160;
            for (&player, units) in &groups {
                let quadrant = (usize::from(player.0) + round as usize + 1) % 4;
                let (x, y) = QUADRANTS[quadrant];
                let target = metres(x + shift, y + shift);
                orders.order(
                    &mut sim,
                    Tick(round * GROUP_MOVE_EVERY),
                    player,
                    units,
                    target,
                );
            }
        }
        sim
    }

    /// More live flow fields than the cache keeps: [`CounterSim::flow_groups`]`(36)`, against
    /// a `Path.flow_cache_max` of 32. The cache evicts only at a tick start and keeps the
    /// fields read in the previous tick (R124), so each field is built once, in the request
    /// tick, and later ticks build only the tiles of clusters entered for the first time.
    /// (Before R124 every read past the 32nd key missed: about 91 integrations and tiles were
    /// rebuilt every tick.)
    ///
    /// # Panics
    /// As [`CounterSim::flow_groups`].
    pub fn flow_thrash() -> Self {
        CounterSim::flow_groups(THRASH_GROUPS)
    }

    /// `groups` flow fields followed at once: on a 128 × 128-cell ASCII map of grass (4 m
    /// cells, 8 × 8 clusters of 16 cells), `groups` groups of 6 `t.rifleman`s of player 0
    /// (`t.red`). Group `g` stands in row `2 + 3 g`, columns 2 to 7; the units spawn member by
    /// member (the first of every group, then the second, ...), so consecutive `SimId`s are of
    /// different groups. At tick 40 group `g` is ordered (one `core.move`) to the centre of
    /// cluster `(1 + g % 6, 1 + g / 6)`: one destination cluster, so one flow field, per group.
    ///
    /// # Panics
    /// If `groups` is over 36 (the rows and clusters run out), or the sim does not build or
    /// the map does not set up.
    pub fn flow_groups(groups: u32) -> Self {
        assert!(groups <= 36, "at most 36 flow groups fit the map");
        let rows: Vec<String> = (0..FLOW_MAP_CELLS)
            .map(|_| ".".repeat(FLOW_MAP_CELLS as usize))
            .collect();
        let (mut sim, content) = CounterSim::ascii(&rows);
        let rifleman = content
            .unit_id("t.rifleman")
            .expect("the ASCII mod has t.rifleman");
        let mut members: Vec<Vec<SimId>> = vec![Vec::new(); groups as usize];
        for m in 0..FLOW_GROUP_UNITS {
            for (g, group) in (0..groups).zip(&mut members) {
                group.push(sim.spawn(rifleman, 2 + m, 2 + 3 * g));
            }
        }
        let mut orders = Orders::default();
        for (g, units) in (0..groups).zip(&members) {
            let cluster = |i: u32| 16 * i + 8;
            let target = cell_centre_m(cluster(1 + g % 6), cluster(1 + g / 6));
            orders.order(&mut sim, Tick(FLOW_GROUPS_TICK), PlayerId(0), units, target);
        }
        sim
    }

    /// A crowd through a choke point: a 64 × 32-cell ASCII map of grass (4 m cells) cut north
    /// to south by water narrowing to a funnel ([`choke_water`]: from 28 cells wide at column
    /// 18 to a two-cell gap at columns 31 and 32); 192 `t.tank`s of player 0 (`t.red`) in a
    /// 16 × 12 block of cells (columns 2 to 17, rows 9 to 20), ordered at tick 0 (one
    /// `core.move`, one flow field) to the centre of cell (36, 16), just past the gap, so the
    /// first to arrive stand in the way of the rest. The gate budgets its `steer.pairs` and
    /// path and flow counters; no unit is ever fully blocked here (`move.blocked` is gated in
    /// the scale scenarios, R127).
    ///
    /// # Panics
    /// If the sim does not build or the map does not set up.
    pub fn choke_crowd() -> Self {
        let rows: Vec<String> = (0..CHOKE_H)
            .rev()
            .map(|y| {
                (0..CHOKE_W)
                    .map(|x| if choke_water(x, y) { '~' } else { '.' })
                    .collect()
            })
            .collect();
        let (mut sim, content) = CounterSim::ascii(&rows);
        let tank = content.unit_id("t.tank").expect("the ASCII mod has t.tank");
        let mut units = Vec::new();
        for y in 9..21 {
            for x in 2..18 {
                units.push(sim.spawn(tank, x, y));
            }
        }
        Orders::default().order(
            &mut sim,
            Tick(0),
            PlayerId(0),
            &units,
            cell_centre_m(36, 16),
        );
        sim
    }

    /// The sim world.
    pub fn world(&self) -> &World {
        self.app.world()
    }

    /// `scale` on the base content, with the journal on or off.
    pub(super) fn from_scale(scale: &ScaleScenario, journal: bool) -> Self {
        let mut app = counter_app(&base_content(), scale.seed, journal);
        scale
            .populate(app.world_mut())
            .unwrap_or_else(|err| panic!("the scale scenario does not set up: {err}"));
        CounterSim {
            app,
            commands: BTreeMap::new(),
        }
    }

    /// [`ScaleScenario::on_terrain`]`(units)` with units only: no followers, projectiles,
    /// sensing or wandering.
    fn quiet_scale(units: u32) -> Self {
        let scale = ScaleScenario {
            followers_per_unit: 0,
            projectiles: 0,
            sensor_every: 0,
            ..ScaleScenario::on_terrain(units)
        };
        let mut sim = CounterSim::from_scale(&scale, false);
        let world = sim.app.world_mut();
        let mut wanderers = world.query_filtered::<Entity, With<Wander>>();
        let wanderers: Vec<_> = wanderers.iter(world).collect();
        for entity in wanderers {
            world.entity_mut(entity).remove::<Wander>();
        }
        sim
    }

    /// The sim of the game's features on the ASCII map `rows` (4 m cells; row 0 the north
    /// edge, see [`ascii_terrain_content`]) with player 0 (`t.red`), no units, no commands; and
    /// its content.
    fn ascii(rows: &[String]) -> (Self, Arc<ContentDb>) {
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let content = ascii_terrain_content(&rows, ASCII_CELL_M);
        let mut app = counter_app(&content, 0, false);
        let world = app.world_mut();
        setup_map(world, "t.ascii").unwrap_or_else(|err| panic!("the ASCII map: {err}"));
        add_player(world, &content, "t.red");
        let sim = CounterSim {
            app,
            commands: BTreeMap::new(),
        };
        (sim, content)
    }

    /// Spawn a `unit` of player 0 at the centre of ASCII cell `(x, y)`; its id.
    fn spawn(&mut self, unit: UnitTypeId, x: u32, y: u32) -> SimId {
        let request = SpawnRequest {
            unit: Some(unit),
            owner: PlayerId(0),
            pos: cell_centre_m(x, y),
            heading: Angle::ZERO,
            init: None,
        };
        setup_spawn(self.app.world_mut(), &request)
            .unwrap_or_else(|err| panic!("a unit at cell ({x}, {y}): {err}"))
    }

    /// Step one tick with the commands due; with `alloc`, what the allocator did meanwhile
    /// (the tick's output included).
    pub(super) fn step(
        &mut self,
        alloc: Option<&'static StatsAlloc<System>>,
    ) -> (TickOutput, Option<Stats>) {
        let world = self.app.world_mut();
        let tick = world.resource::<SimClock>().tick;
        let commands = self.commands.remove(&tick).unwrap_or_default();
        let region = alloc.map(Region::new);
        let output = step_tick(world, TickInput { tick, commands });
        let change = region.map(|region| region.change());
        (output, change)
    }
}

/// The commands of a [`CounterSim`], with each player's next seq.
#[derive(Default)]
struct Orders {
    seqs: BTreeMap<PlayerId, u32>,
}

impl Orders {
    /// `player` orders `units` to move to `target` (not queued) at `tick`.
    fn order(
        &mut self,
        sim: &mut CounterSim,
        tick: Tick,
        player: PlayerId,
        units: &[SimId],
        target: FxVec2,
    ) {
        let seq = self.seqs.entry(player).or_insert(0);
        *seq += 1;
        let kind = sim
            .app
            .world()
            .resource::<CommandRegistry>()
            .encode(&MoveCmd { target });
        sim.commands.entry(tick).or_default().push(PlayerCommand {
            exec_tick: tick,
            player,
            seq: *seq,
            units: units.to_vec(),
            kind,
            queue: false,
        });
    }
}

/// A sim of the game's features on `content` with match seed `seed`: the state hash at the
/// rules' interval and, unless `journal`, the journal off, as a release match runs.
fn counter_app(content: &Arc<ContentDb>, seed: u64, journal: bool) -> App {
    let setup = MatchSetup {
        seed,
        cheats_allowed: false,
    };
    let config = RunConfig {
        journal: Some(journal),
        ..RunConfig::default()
    };
    build_sim_app(&fl_catalog::features(), Arc::clone(content), setup, config)
        .unwrap_or_else(|err| panic!("the sim does not build: {err}"))
}

/// Append a human player of `faction` on team 0 to [`Players`].
fn add_player(world: &mut World, content: &ContentDb, faction: &str) {
    let faction = content
        .kind::<Faction>()
        .index_of(faction)
        .unwrap_or_else(|| panic!("faction '{faction}' does not exist"));
    let mut players = world.resource_mut::<Players>();
    let id = u8::try_from(players.list.len()).expect("at most 256 players");
    players.list.push(PlayerInfo {
        id: PlayerId(id),
        faction: FactionId(u16::try_from(faction).expect("fewer than 65536 factions")),
        team: TeamId(0),
        controller: Controller::Human,
    });
}

/// The units (entities with `Mobility`) grouped by `key(owner, movement profile)`, each
/// group in `SimId` order ([`unit_cells`]).
fn units_by<K: Ord>(
    world: &mut World,
    key: impl Fn(PlayerId, MoveProfile) -> K,
) -> BTreeMap<K, Vec<SimId>> {
    let mut groups: BTreeMap<K, Vec<SimId>> = BTreeMap::new();
    for unit in unit_cells(world) {
        if let Some(owner) = unit.owner {
            groups
                .entry(key(owner, unit.profile))
                .or_default()
                .push(unit.id);
        }
    }
    groups
}

/// `(x, y)` metres.
fn metres(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

/// The centre of cell `(x, y)` of the ASCII maps here (4 m cells).
fn cell_centre_m(x: u32, y: u32) -> FxVec2 {
    let m = |i: u32| Fx::from_ratio(i64::from(2 * i + 1) * i64::from(ASCII_CELL_M), 2);
    FxVec2::new(m(x), m(y))
}
