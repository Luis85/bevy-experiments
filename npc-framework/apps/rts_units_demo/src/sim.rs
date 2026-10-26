//! The demo's own sim content, defined in the host app (no `content/*`
//! crate): two unit types registered by stable name, a 96 × 96 m map with
//! wall blocks, two players with two squads each, and the scripted opening
//! orders. Integer-only like any sim content (the lints below), so the same
//! seed and orders give the same state hash.
//!
//! - Unit types: [`UnitKind::Infantry`] (tree `rts_infantry`) and
//!   [`UnitKind::Engineer`] (tree `rts_engineer`), each a tiny tree: the
//!   `npc_command` order branch (`has_unit_order` → `ExecuteOrder`) over an
//!   idle leaf (`RtsIdle`). Engineers have a repair branch between the two:
//!   guard [`COND_DAMAGED_WALL_NEAR`] → action [`RTS_REPAIR`] (walk to a
//!   free cell next to the nearest damaged wall within [`REPAIR_SCAN_MM`],
//!   then restore [`REPAIR_HP_PER_TICK`] per tick until full).
//! - Repair order: `UnitOrder::Interact { verb: "repair", target: wall }`,
//!   run by the order executor [`REPAIR_EXECUTOR`] registered for kind
//!   `Interact` (this host does not enable the `interaction` capability,
//!   whose glue would claim that kind). Same [`Repairing`] state as the leaf.
//! - Map: [`MAP_CELLS`]² cells of [`CELL_MM`], walls [`WALLS`] as
//!   `GridBlocker`s with a [`WallHealth`] ([`DAMAGED_WALLS`] start damaged);
//!   every unit is a `NavAgent` (grid A*, group moves reuse the leader's path).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type
)]

use bevy::prelude::*;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::kernel::json::JsonObject;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// Sim ticks per second (the host's `FixedUpdate` runs at its own rate).
pub const TICK_HZ: u32 = 10;
/// Grid cell size, mm.
pub const CELL_MM: i32 = 1000;
/// The map is `MAP_CELLS × MAP_CELLS` cells centred on the origin (96 m).
pub const MAP_CELLS: u32 = 96;
/// Default number of units (split over the 4 squads).
pub const DEFAULT_UNITS: u32 = 200;
/// Players and squads per player.
pub const PLAYERS: u32 = 2;
pub const SQUADS_PER_PLAYER: u32 = 2;
/// Squads in the demo.
pub const SQUADS: u32 = PLAYERS * SQUADS_PER_PLAYER;

/// The idle action (param: ticks).
pub const RTS_IDLE: &str = "RtsIdle";
pub const TREE_INFANTRY: &str = "rts_infantry";
pub const TREE_ENGINEER: &str = "rts_engineer";

/// The engineer's repair action (no param: the nearest damaged wall).
pub const RTS_REPAIR: &str = "RtsRepair";
/// Condition: an engineer with a damaged wall within [`REPAIR_SCAN_MM`].
pub const COND_DAMAGED_WALL_NEAR: &str = "rts_damaged_wall_near";
/// The order verb: `UnitOrder::Interact { verb: "repair", target: wall }`.
pub const REPAIR_VERB: &str = "repair";
/// The `Interact` order executor that runs [`REPAIR_VERB`].
pub const REPAIR_EXECUTOR: &str = "rts_units_demo.repair";
/// Full wall hit points.
pub const WALL_MAX_HP: u32 = 1000;
/// Hit points one engineer restores per tick (40 hp/s at 10 Hz).
pub const REPAIR_HP_PER_TICK: u32 = 4;
/// How far (mm, to the wall's edge) an idle engineer looks for damage.
pub const REPAIR_SCAN_MM: i32 = 12_000;
/// How close (mm, to the wall's edge) an engineer must stand to repair.
pub const REPAIR_REACH_MM: i32 = 1_200;
/// Walk tolerance to the approach cell's centre, mm.
const REPAIR_TOLERANCE_MM: i32 = 300;
/// Damaged walls in [`spawn_world`]: `(index into WALLS, hp)`.
pub const DAMAGED_WALLS: &[(usize, u32)] = &[(1, 400), (3, 600), (5, 250)];

/// An inclusive cell rectangle `((x0, z0), (x1, z1))`.
pub type CellRect = ((i32, i32), (i32, i32));

/// Wall blocks as inclusive cell rectangles `((x0, z0), (x1, z1))`: a
/// central divider with a gap in the middle and open ends, plus cover.
pub const WALLS: &[CellRect] = &[
    // Central divider.
    ((-1, -34), (0, -5)),
    ((-1, 5), (0, 34)),
    // Cover, west.
    ((-22, -14), (-19, -11)),
    ((-24, 10), (-19, 11)),
    ((-10, -24), (-8, -20)),
    // Cover, east.
    ((19, 11), (22, 14)),
    ((19, -11), (24, -10)),
    ((8, 20), (10, 24)),
];

/// The two unit types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum UnitKind {
    Infantry,
    Engineer,
}

impl UnitKind {
    /// The unit type's stable name (journal `Spawn`, inspection `kind`).
    pub const fn name(self) -> &'static str {
        match self {
            UnitKind::Infantry => "Infantry",
            UnitKind::Engineer => "Engineer",
        }
    }

    /// The tree it runs.
    pub const fn tree(self) -> &'static str {
        match self {
            UnitKind::Infantry => TREE_INFANTRY,
            UnitKind::Engineer => TREE_ENGINEER,
        }
    }

    /// Order move speed, mm per tick (1.5 / 1.2 m/s at 10 Hz).
    pub const fn speed_mm_per_tick(self) -> i32 {
        match self {
            UnitKind::Infantry => 150,
            UnitKind::Engineer => 120,
        }
    }
}

/// A demo unit: its type, player and squad index (`0..SQUADS`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct RtsUnit {
    pub kind: UnitKind,
    pub player: u32,
    pub squad: u32,
}

impl SimHash for RtsUnit {
    const NAME: &'static str = "rts_unit";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(match self.kind {
            UnitKind::Infantry => 0,
            UnitKind::Engineer => 1,
        });
        h.u32(self.player);
        h.u32(self.squad);
    }
}

/// A wall block (its `GridBlocker` carries the footprint).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Wall;

impl SimHash for Wall {
    const NAME: &'static str = "rts_wall";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

/// Hit points of a wall block (every [`Wall`] carries one). Damaged =
/// `hp < max`; engineers repair damaged blocks back to `max`.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct WallHealth {
    pub hp: u32,
    pub max: u32,
}

impl WallHealth {
    /// `hp` of [`WALL_MAX_HP`] (clamped).
    pub const fn new(hp: u32) -> Self {
        Self {
            hp: if hp < WALL_MAX_HP { hp } else { WALL_MAX_HP },
            max: WALL_MAX_HP,
        }
    }

    pub const fn is_damaged(&self) -> bool {
        self.hp < self.max
    }
}

impl SimHash for WallHealth {
    const NAME: &'static str = "rts_wall_health";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.hp);
        h.u32(self.max);
    }
}

/// The `RtsIdle` action: ticks left.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct RtsIdling {
    pub remaining: u32,
}

impl SimHash for RtsIdling {
    const NAME: &'static str = "rts_idling";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.remaining);
    }
}

fn idle_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let ticks = param.as_int().unwrap_or(30).max(1);
    world.entity_mut(agent).insert(RtsIdling {
        remaining: u32::try_from(ticks).unwrap_or(1),
    });
    Status::Running
}

fn idle_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<RtsIdling>();
    }
}

/// `NpcSet::Actions`: counts idle ticks down.
fn idle_system(mut q: Query<(&mut RtsIdling, &mut ActiveLeaf)>) {
    for (mut idle, mut leaf) in &mut q {
        if leaf.status != Status::Running {
            continue;
        }
        idle.remaining = idle.remaining.saturating_sub(1);
        if idle.remaining == 0 {
            leaf.status = Status::Success;
        }
    }
}

/// The order branch every unit type shares: orders win over idling.
fn order_branch(b: &mut TreeBuilder) {
    b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
        b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
    });
}

fn infantry_tree(b: &mut TreeBuilder) {
    b.selector("Infantry", |b| {
        order_branch(b);
        b.leaf("Scan", RTS_IDLE, 30);
    });
}

fn engineer_tree(b: &mut TreeBuilder) {
    b.selector("Engineer", |b| {
        order_branch(b);
        // Damaged wall nearby: repair it (the condition implies the leaf starts).
        b.guard(
            "Damage",
            COND_DAMAGED_WALL_NEAR,
            Abort::LowerPriority,
            |b| {
                b.leaf("Repair", RTS_REPAIR, ());
            },
        );
        b.sequence("Tinker", |b| {
            b.leaf("Inspect", RTS_IDLE, 20);
            b.leaf("Rest", RTS_IDLE, 50);
        });
    });
}

// ---------------------------------------------------------------------------
// Repair: engineers restore damaged walls (autonomous leaf + RTS order).
// ---------------------------------------------------------------------------

/// Why a repair could not go on (the order's `OrderFailed { reason }`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum RepairFailure {
    /// The wall left the sim (or is not a wall).
    WallGone,
    /// No free cell next to the wall.
    NoAccess,
    /// The walk was blocked for `CommandConfig::block_fail_ticks`.
    Blocked,
    /// No damaged wall within [`REPAIR_SCAN_MM`] (the autonomous leaf).
    NothingToRepair,
}

impl RepairFailure {
    pub const fn reason(self) -> &'static str {
        match self {
            RepairFailure::WallGone => "wall is gone",
            RepairFailure::NoAccess => "no free cell next to the wall",
            RepairFailure::Blocked => "blocked",
            RepairFailure::NothingToRepair => "no damaged wall in range",
        }
    }

    const fn code(self) -> u8 {
        match self {
            RepairFailure::WallGone => 0,
            RepairFailure::NoAccess => 1,
            RepairFailure::Blocked => 2,
            RepairFailure::NothingToRepair => 3,
        }
    }
}

/// Where a repair stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum RepairPhase {
    /// Walking to `goal` (a free cell next to the wall).
    Approach,
    /// In reach: restoring [`REPAIR_HP_PER_TICK`] per tick.
    Working,
    /// The wall is at full hit points.
    Done,
    Failed(RepairFailure),
}

/// The engineer's running repair: the `RtsRepair` action's component, also
/// used by the repair order executor (`by_order`), whose `tick` reads the
/// phase. `area` is the wall's extent in mm (`min`, `max` corners).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Repairing {
    pub target: SimId,
    pub goal: SimPos,
    pub area_min: SimPos,
    pub area_max: SimPos,
    pub phase: RepairPhase,
    pub by_order: bool,
}

impl SimHash for Repairing {
    const NAME: &'static str = "rts_repairing";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.target.0);
        for p in [self.goal, self.area_min, self.area_max] {
            h.i32(p.x);
            h.i32(p.z);
        }
        match self.phase {
            RepairPhase::Approach => h.u8(0),
            RepairPhase::Working => h.u8(1),
            RepairPhase::Done => h.u8(2),
            RepairPhase::Failed(f) => {
                h.u8(3);
                h.u8(f.code());
            }
        }
        h.u8(u8::from(self.by_order));
    }
}

/// A wall as repair sees it: id, entity, extent (mm) and health.
#[derive(Clone, Copy, Debug)]
struct WallSite {
    id: SimId,
    min: SimPos,
    max: SimPos,
    cells: (CellCoord, CellCoord),
    health: WallHealth,
}

/// The cells and mm extent a wall covers (its `GridBlocker` footprint).
fn wall_site(
    grid: &SpatialGrid,
    id: SimId,
    pos: SimPos,
    fp: Footprint,
    health: WallHealth,
) -> Option<WallSite> {
    let cells = grid.footprint_cells(pos, fp);
    let (lo, hi) = (*cells.first()?, *cells.last()?);
    let c = grid.cell_mm();
    Some(WallSite {
        id,
        min: SimPos::new(lo.x * c, lo.z * c),
        max: SimPos::new((hi.x + 1) * c, (hi.z + 1) * c),
        cells: (lo, hi),
        health,
    })
}

/// Squared distance (mm²) from `p` to the rectangle `min..max`.
pub fn dist2_to_area(p: SimPos, min: SimPos, max: SimPos) -> i64 {
    let axis = |v: i32, lo: i32, hi: i32| -> i64 {
        let (v, lo, hi) = (i64::from(v), i64::from(lo), i64::from(hi));
        (lo - v).max(v - hi).max(0)
    };
    let dx = axis(p.x, min.x, max.x);
    let dz = axis(p.z, min.z, max.z);
    dx * dx + dz * dz
}

fn in_reach(p: SimPos, min: SimPos, max: SimPos) -> bool {
    dist2_to_area(p, min, max) <= i64::from(REPAIR_REACH_MM).pow(2)
}

/// Every wall, sorted by `SimId`.
fn wall_sites(world: &World) -> Vec<WallSite> {
    let Some(grid) = world.get_resource::<SpatialGrid>() else {
        return Vec::new();
    };
    let Some(mut q) =
        world.try_query_filtered::<(&SimId, &SimPos, &GridBlocker, &WallHealth), With<Wall>>()
    else {
        return Vec::new();
    };
    let mut out: Vec<WallSite> = q
        .iter(world)
        .filter_map(|(id, pos, b, h)| wall_site(grid, *id, *pos, b.0, *h))
        .collect();
    out.sort_by_key(|w| w.id);
    out
}

/// The damaged wall nearest to `from` (edge distance) within `range_mm`;
/// ties by `SimId`. Public for the view's repair click.
pub fn nearest_damaged_wall(world: &World, from: SimPos, range_mm: i32) -> Option<SimId> {
    nearest_damaged_site(world, from, range_mm).map(|w| w.id)
}

fn nearest_damaged_site(world: &World, from: SimPos, range_mm: i32) -> Option<WallSite> {
    let r2 = i64::from(range_mm).pow(2);
    wall_sites(world)
        .into_iter()
        .filter(|w| w.health.is_damaged())
        .map(|w| (dist2_to_area(from, w.min, w.max), w))
        .filter(|(d2, _)| *d2 <= r2)
        .min_by_key(|(d2, w)| (*d2, w.id))
        .map(|(_, w)| w)
}

/// The free cell next to `site` closest to `from` (ties: lowest `(x, z)`).
fn approach_cell(grid: &SpatialGrid, from: SimPos, site: &WallSite) -> Option<SimPos> {
    let (lo, hi) = site.cells;
    let bounds = grid.bounds();
    let mut best: Option<(i64, CellCoord)> = None;
    for x in lo.x - 1..=hi.x + 1 {
        for z in lo.z - 1..=hi.z + 1 {
            let inside = (lo.x..=hi.x).contains(&x) && (lo.z..=hi.z).contains(&z);
            let cell = CellCoord::new(x, z);
            if inside || !bounds.contains(cell) || grid.is_blocking(cell) {
                continue;
            }
            let d2 = grid.center(cell).dist2(from);
            if best.is_none_or(|(bd, bc)| (d2, cell) < (bd, bc)) {
                best = Some((d2, cell));
            }
        }
    }
    best.map(|(_, c)| grid.center(c))
}

fn is_engineer(world: &World, unit: Entity) -> bool {
    world
        .get::<RtsUnit>(unit)
        .is_some_and(|u| u.kind == UnitKind::Engineer)
}

/// What a repair of `target` (or, with `None`, the nearest damaged wall in
/// scan range) by `unit` would be. Used by the condition, the leaf start
/// and the order executor alike, so the guard implies the leaf can start.
fn plan_repair(
    world: &World,
    unit: Entity,
    target: Option<SimId>,
    by_order: bool,
) -> Result<Repairing, RepairFailure> {
    let here = world
        .get::<SimPos>(unit)
        .copied()
        .ok_or(RepairFailure::WallGone)?;
    let site = match target {
        Some(id) => wall_sites(world)
            .into_iter()
            .find(|w| w.id == id)
            .ok_or(RepairFailure::WallGone)?,
        None => nearest_damaged_site(world, here, REPAIR_SCAN_MM)
            .ok_or(RepairFailure::NothingToRepair)?,
    };
    let (phase, goal) = if !site.health.is_damaged() {
        (RepairPhase::Done, here)
    } else if in_reach(here, site.min, site.max) {
        (RepairPhase::Working, here)
    } else {
        let grid = world
            .get_resource::<SpatialGrid>()
            .ok_or(RepairFailure::NoAccess)?;
        let goal = approach_cell(grid, here, &site).ok_or(RepairFailure::NoAccess)?;
        (RepairPhase::Approach, goal)
    };
    Ok(Repairing {
        target: site.id,
        goal,
        area_min: site.min,
        area_max: site.max,
        phase,
        by_order,
    })
}

/// Inserts the plan and starts the walk if it needs one.
fn begin_repair(world: &mut World, unit: Entity, plan: Repairing) {
    if plan.phase == RepairPhase::Approach {
        let speed = world
            .get::<Commandable>(unit)
            .map_or(UnitKind::Engineer.speed_mm_per_tick(), |c| {
                c.speed_mm_per_tick
            });
        set_move_intent(
            world,
            unit,
            MoveIntent {
                goal: plan.goal,
                speed_mm_per_tick: speed,
                tolerance_mm: REPAIR_TOLERANCE_MM,
            },
        );
    }
    world.entity_mut(unit).insert(plan);
}

fn end_repair(world: &mut World, unit: Entity) {
    clear_move_intent(world, unit);
    if let Ok(mut e) = world.get_entity_mut(unit) {
        e.remove::<Repairing>();
    }
}

/// Condition `rts_damaged_wall_near`: an engineer that can start repairing
/// a damaged wall within [`REPAIR_SCAN_MM`] (implies `RtsRepair` starts).
fn damaged_wall_near(world: &World, agent: Entity) -> bool {
    is_engineer(world, agent)
        && plan_repair(world, agent, None, false).is_ok_and(|p| p.phase != RepairPhase::Done)
}

fn repair_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    if !is_engineer(world, agent) {
        return Status::Failure;
    }
    match plan_repair(world, agent, None, false) {
        Ok(plan) if plan.phase != RepairPhase::Done => {
            begin_repair(world, agent, plan);
            Status::Running
        }
        _ => Status::Failure,
    }
}

fn repair_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    if world.get::<Repairing>(agent).is_some_and(|r| !r.by_order) {
        end_repair(world, agent);
    }
}

/// `NpcSet::Locomotion` after the movers: an approaching engineer starts
/// working once in reach; a walk blocked too long fails.
fn repair_approach_system(
    config: Option<Res<CommandConfig>>,
    mut q: Query<(&mut Repairing, &SimPos, Option<&MoveStatus>)>,
) {
    let limit = config.map_or_else(
        || CommandConfig::default().block_fail_ticks,
        |c| c.block_fail_ticks,
    );
    for (mut rep, pos, status) in &mut q {
        if rep.phase != RepairPhase::Approach {
            continue;
        }
        if in_reach(*pos, rep.area_min, rep.area_max) || matches!(status, Some(MoveStatus::Arrived))
        {
            rep.phase = RepairPhase::Working;
        } else if matches!(status, Some(MoveStatus::Blocked { ticks }) if *ticks >= limit) {
            rep.phase = RepairPhase::Failed(RepairFailure::Blocked);
        }
    }
}

/// What a finished repair journals (`JournalKind::Effect`).
#[derive(Serialize)]
#[serde(tag = "type")]
enum RepairEffect {
    WallRepaired { wall: u32, by: u32 },
}

/// `NpcSet::Actions`: working engineers, in `SimId` order, restore
/// [`REPAIR_HP_PER_TICK`] each; the leaf (not an order) ends on the result.
fn repair_work_system(
    index: Res<SimIndex>,
    clock: Res<SimClock>,
    mut journal: Option<ResMut<Journal>>,
    mut units: Query<(Entity, &SimId, &mut Repairing, Option<&mut ActiveLeaf>)>,
    mut walls: Query<&mut WallHealth, With<Wall>>,
) {
    let mut working: Vec<(SimId, Entity)> = units
        .iter()
        .filter(|(_, _, r, _)| r.phase == RepairPhase::Working)
        .map(|(e, id, _, _)| (*id, e))
        .collect();
    working.sort_unstable();
    for (id, e) in working {
        let Ok((_, _, mut rep, _)) = units.get_mut(e) else {
            continue;
        };
        let wall = index.get(rep.target).and_then(|w| walls.get_mut(w).ok());
        let Some(mut health) = wall else {
            rep.phase = RepairPhase::Failed(RepairFailure::WallGone);
            continue;
        };
        if !health.is_damaged() {
            rep.phase = RepairPhase::Done;
            continue;
        }
        health.hp = health.hp.saturating_add(REPAIR_HP_PER_TICK).min(health.max);
        if !health.is_damaged() {
            rep.phase = RepairPhase::Done;
            if let Some(j) = journal.as_deref_mut() {
                let effect = RepairEffect::WallRepaired {
                    wall: rep.target.0,
                    by: id.0,
                };
                j.push(
                    clock.tick,
                    Some(id),
                    JournalKind::Effect {
                        effect: JsonObject::from_typed(&effect).expect("an object"),
                        applied: true,
                    },
                );
            }
        }
    }
    for (_, _, rep, leaf) in &mut units {
        let Some(mut leaf) = leaf else { continue };
        if rep.by_order || leaf.status != Status::Running {
            continue;
        }
        match rep.phase {
            RepairPhase::Done => leaf.status = Status::Success,
            RepairPhase::Failed(_) => leaf.status = Status::Failure,
            _ => {}
        }
    }
}

// --- the order: `Interact { verb: "repair", target }` ----------------------

fn repair_target(order: &UnitOrder) -> Option<SimId> {
    match order {
        UnitOrder::Interact { verb, target } if verb == REPAIR_VERB => Some(*target),
        _ => None,
    }
}

fn repair_order_validate(world: &World, unit: Entity, order: &UnitOrder) -> Result<(), String> {
    let UnitOrder::Interact { verb, target } = order else {
        return Err("not an Interact order".into());
    };
    if verb != REPAIR_VERB {
        return Err(format!("unknown verb `{verb}` (known: {REPAIR_VERB})"));
    }
    if !is_engineer(world, unit) {
        return Err("only engineers can repair".into());
    }
    let is_wall = world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(*target))
        .is_some_and(|w| world.get::<WallHealth>(w).is_some());
    if !is_wall {
        return Err(format!("#{} is not a wall", target.0));
    }
    Ok(())
}

fn repair_order_start(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let Some(target) = repair_target(order) else {
        return OrderStep::Failed("not a repair order".into());
    };
    end_repair(world, unit);
    match plan_repair(world, unit, Some(target), true) {
        Ok(plan) if plan.phase == RepairPhase::Done => OrderStep::Done,
        Ok(plan) => {
            begin_repair(world, unit, plan);
            OrderStep::Running
        }
        Err(f) => OrderStep::Failed(f.reason().into()),
    }
}

fn repair_order_tick(world: &mut World, unit: Entity, _order: &UnitOrder) -> OrderStep {
    match world.get::<Repairing>(unit).map(|r| r.phase) {
        Some(RepairPhase::Done) => OrderStep::Done,
        Some(RepairPhase::Failed(f)) => OrderStep::Failed(f.reason().into()),
        Some(_) => OrderStep::Running,
        None => OrderStep::Failed("repair state lost".into()),
    }
}

fn repair_order_cancel(world: &mut World, unit: Entity, _order: &UnitOrder) {
    if world.get::<Repairing>(unit).is_some_and(|r| r.by_order) {
        end_repair(world, unit);
    }
}

/// The repair order executor (kind `Interact`, verb [`REPAIR_VERB`]).
fn repair_executor() -> OrderExecutor {
    OrderExecutor::new(repair_order_start, repair_order_tick)
        .with_validate(repair_order_validate)
        .with_cancel(repair_order_cancel)
}

fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    if let Some(u) = world.get::<RtsUnit>(entity) {
        return Some(u.kind.name());
    }
    world.get::<Wall>(entity).map(|_| "wall")
}

/// Registers the demo content by stable name (needs `NpcPlugins` with
/// `with_command()` and `NavPlugin`; any order).
pub struct RtsContentPlugin;

impl Plugin for RtsContentPlugin {
    fn build(&self, app: &mut App) {
        app.register_action(RTS_IDLE, idle_start, idle_stop)
            .register_action(RTS_REPAIR, repair_start, repair_stop)
            .register_cond(COND_DAMAGED_WALL_NEAR, damaged_wall_near)
            .register_order_executor("Interact", REPAIR_EXECUTOR, repair_executor())
            .register_tree(TREE_INFANTRY, infantry_tree)
            .register_tree(TREE_ENGINEER, engineer_tree)
            .register_sim_component::<RtsUnit>()
            .register_sim_component::<Wall>()
            .register_sim_component::<WallHealth>()
            .register_sim_component::<RtsIdling>()
            .register_sim_component::<Repairing>()
            .register_kind_name("rts_units_demo", kind_name)
            .add_systems(
                NpcSimTick,
                (
                    (idle_system, repair_work_system)
                        .chain()
                        .in_set(NpcSet::Actions),
                    repair_approach_system
                        .in_set(NpcSet::Locomotion)
                        .after(NpcMoverSet),
                ),
            );
    }
}

/// The grid the host inserts before the plugins (96 m, 1 m cells).
pub fn spatial_grid() -> SpatialGrid {
    SpatialGrid::new(CELL_MM, GridBounds::centered(MAP_CELLS, MAP_CELLS))
}

/// The framework as this host configures it: its seed, 10 Hz, driven from
/// the host's `FixedUpdate` (whose own timestep the host chose), with the
/// RTS command capability. Add `NavPlugin` and [`RtsContentPlugin`] next to it.
pub fn npc_plugins(seed: u64) -> NpcPlugins {
    NpcPlugins::new()
        .seed(seed)
        .tick_hz(TICK_HZ)
        .run_in(FixedUpdate)
        .with_command()
}

/// The sim side of the demo on `app`: grid, framework, nav, content.
pub fn add_sim(app: &mut App, plugins: NpcPlugins) {
    app.insert_resource(spatial_grid())
        .add_plugins((plugins, NavPlugin, RtsContentPlugin));
}

/// What [`spawn_world`] created.
#[derive(Resource, Clone, Debug, Default)]
pub struct DemoWorld {
    /// Squad entity ids, squad index order.
    pub squads: Vec<SimId>,
    /// Members per squad, squad index order.
    pub members: Vec<Vec<SimId>>,
    pub walls: Vec<SimId>,
}

/// The owner of squad `squad`.
pub const fn squad_player(squad: u32) -> u32 {
    squad / SQUADS_PER_PLAYER
}

/// Where squad `squad` starts (its centre, mm): player 0 west, player 1 east.
pub const fn squad_home(squad: u32) -> SimPos {
    let x = if squad_player(squad) == 0 {
        -38_000
    } else {
        38_000
    };
    let z = if squad.is_multiple_of(2) {
        -18_000
    } else {
        18_000
    };
    SimPos::new(x, z)
}

/// The formation each squad uses unless an order names one.
pub const fn squad_formation(squad: u32) -> Formation {
    Formation {
        kind: FormationKind::Box,
        spacing_mm: if squad.is_multiple_of(2) { 1500 } else { 1800 },
    }
}

const SPAWN_SPACING_MM: i32 = 1500;
/// Spawn block width (units per row, along z), at least.
const SPAWN_COLS: i32 = 10;
/// Spawn block depth (rows, along x), at most: wider blocks beyond it keep
/// large squads inside the map.
const SPAWN_MAX_ROWS: i32 = 12;

/// The centre of the inclusive cell rectangle and its footprint.
fn wall_block(a: (i32, i32), b: (i32, i32)) -> (SimPos, Footprint) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
    let half = CELL_MM / 2;
    (
        SimPos::new((x0 + x1 + 1) * half, (z0 + z1 + 1) * half),
        Footprint {
            half_x_mm: (x1 - x0) * half,
            half_z_mm: (z1 - z0) * half,
        },
    )
}

/// Spawns one wall block over the inclusive cell rectangle `rect` with `hp`
/// hit points (of [`WALL_MAX_HP`]). Content must be frozen.
pub fn spawn_wall(world: &mut World, rect: CellRect, hp: u32) -> SimId {
    let (pos, fp) = wall_block(rect.0, rect.1);
    spawn_sim(world, (pos, Wall, WallHealth::new(hp), GridBlocker(fp))).0
}

/// Spawns one unit of `kind` for `player` (squad index `squad`, only a
/// label: [`create_squad`] makes the squad) at `pos`. Content must be frozen.
pub fn spawn_unit(
    world: &mut World,
    kind: UnitKind,
    player: u32,
    squad: u32,
    pos: SimPos,
) -> SimId {
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(kind.tree())
        .unwrap_or_else(|| panic!("tree `{}` is not registered", kind.tree()));
    spawn_sim(
        world,
        (
            pos,
            RtsUnit {
                kind,
                player,
                squad,
            },
            Brain { tree },
            Commandable {
                speed_mm_per_tick: kind.speed_mm_per_tick(),
                arrive_mm: 200,
            },
            Owner(PlayerId(player)),
            NavAgent,
        ),
    )
    .0
}

/// Spawns the walls ([`DAMAGED_WALLS`] damaged), `units` units in [`SQUADS`] squads (split evenly, the
/// first squads take the remainder; every fifth unit is an Engineer) and
/// the squads. Content must be frozen (after `App::finish`).
pub fn spawn_world(world: &mut World, units: u32) -> DemoWorld {
    let mut out = DemoWorld::default();
    for (i, &rect) in WALLS.iter().enumerate() {
        let hp = DAMAGED_WALLS
            .iter()
            .find(|(w, _)| *w == i)
            .map_or(WALL_MAX_HP, |&(_, hp)| hp);
        out.walls.push(spawn_wall(world, rect, hp));
    }
    for squad in 0..SQUADS {
        let n = units / SQUADS + u32::from(squad < units % SQUADS);
        let player = squad_player(squad);
        let home = squad_home(squad);
        // Rows along x (behind the front, facing the enemy), columns along z.
        let n = i32::try_from(n).expect("unit count fits i32");
        let width = SPAWN_COLS.max((n + SPAWN_MAX_ROWS - 1) / SPAWN_MAX_ROWS);
        let rows = (n + width - 1) / width;
        let facing = if player == 0 { 1 } else { -1 };
        let mut ids = Vec::new();
        for i in 0..n {
            let (row, col) = (i / width, i % width);
            let cols = width.min(n - row * width);
            let lateral = (2 * col - (cols - 1)) * SPAWN_SPACING_MM / 2;
            let depth = (2 * row - (rows - 1)) * SPAWN_SPACING_MM / 2;
            let pos = SimPos::new(home.x - facing * depth, home.z + lateral);
            let kind = if i % 5 == 4 {
                UnitKind::Engineer
            } else {
                UnitKind::Infantry
            };
            ids.push(spawn_unit(world, kind, player, squad, pos));
        }
        let squad_id = create_squad(world, PlayerId(player), &ids, Some(squad_formation(squad)));
        out.squads.push(squad_id);
        out.members.push(ids);
    }
    world.insert_resource(out.clone());
    out
}

/// The scripted opening (recorded `IssueOrder`s): every squad crosses the
/// map around the walls; squad 1 attack-moves north first, then a
/// shift-queued move takes it south-east.
pub fn script_orders(demo: &DemoWorld) -> Vec<IssueOrder> {
    let order = |squad: usize, o: UnitOrder| {
        IssueOrder::new(
            PlayerId(squad_player(squad as u32)),
            [demo.squads[squad]],
            o,
        )
    };
    vec![
        order(
            0,
            UnitOrder::Move {
                pos: (30_000, 22_000),
            },
        ),
        order(1, UnitOrder::AttackMove { pos: (0, 40_000) }),
        order(
            1,
            UnitOrder::Move {
                pos: (30_000, -24_000),
            },
        )
        .queued(true),
        order(
            2,
            UnitOrder::Move {
                pos: (-30_000, 22_000),
            },
        ),
        order(
            3,
            UnitOrder::AttackMove {
                pos: (-30_000, -24_000),
            },
        ),
    ]
}

/// Submits [`script_orders`].
pub fn issue_script(world: &mut World) -> Result<(), CommandError> {
    let demo = world.resource::<DemoWorld>().clone();
    for cmd in script_orders(&demo) {
        submit_order(world, &cmd)?;
    }
    Ok(())
}

/// Unit progress: `(arrived, failed, busy)`. Arrived = no order left and
/// the last one succeeded.
pub fn progress(world: &mut World) -> (u32, u32, u32) {
    let mut q = world.query_filtered::<&OrderQueue, With<RtsUnit>>();
    let (mut arrived, mut failed, mut busy) = (0, 0, 0);
    for queue in q.iter(world) {
        if queue.has_order() {
            busy += 1;
        } else if queue.last_ok {
            arrived += 1;
        } else {
            failed += 1;
        }
    }
    (arrived, failed, busy)
}

/// Wall blocks with `hp < max`.
pub fn damaged_walls(world: &mut World) -> u32 {
    let mut q = world.query::<&WallHealth>();
    let n = q.iter(world).filter(|h| h.is_damaged()).count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The JSON summary of a headless run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub seed: u64,
    pub units: u32,
    pub squads: u32,
    pub ticks: u64,
    pub arrived: u32,
    pub failed: u32,
    /// The first tick at which every unit had arrived.
    pub all_arrived_at: Option<u64>,
    /// Wall blocks still damaged at the end.
    pub damaged_walls: u32,
    pub state_hash: String,
    pub content_hash: String,
}

/// A frozen headless sim app with nothing spawned (no window, no driver:
/// the caller spawns with [`spawn_wall`] / [`spawn_unit`] and runs ticks).
pub fn sim_app(seed: u64) -> App {
    let mut app = App::new();
    add_sim(&mut app, npc_plugins(seed).without_driver());
    app.finish();
    app.cleanup();
    app
}

/// A headless sim app (no window, no driver: the caller runs ticks), with
/// the world spawned and the script issued.
pub fn headless_app(seed: u64, units: u32) -> App {
    let mut app = sim_app(seed);
    let world = app.world_mut();
    spawn_world(world, units);
    issue_script(world).expect("script orders are valid");
    app
}

/// Runs `ticks` ticks headless and summarises.
pub fn run_headless(seed: u64, units: u32, ticks: u64) -> Summary {
    let mut app = headless_app(seed, units);
    let world = app.world_mut();
    let mut all_arrived_at = None;
    for _ in 0..ticks {
        run_ticks(world, 1);
        if all_arrived_at.is_none() {
            let (arrived, _, _) = progress(world);
            if arrived == units {
                all_arrived_at = Some(world.resource::<SimClock>().tick);
            }
        }
    }
    let (arrived, failed, _) = progress(world);
    let damaged_walls = damaged_walls(world);
    Summary {
        seed,
        units,
        squads: SQUADS,
        ticks: world.resource::<SimClock>().tick,
        arrived,
        failed,
        all_arrived_at,
        damaged_walls,
        state_hash: format!("{:016x}", state_hash(world)),
        content_hash: format!("{:016x}", content_hash(world)),
    }
}
