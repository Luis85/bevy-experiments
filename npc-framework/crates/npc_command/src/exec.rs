//! The command engine: queueing, the built-in executors and the executor /
//! redirect hooks by name.
//!
//! [`command_engine_system`] runs once per tick in
//! [`NpcCommandSet::Execute`](crate::NpcCommandSet) (after `NpcSet::Sense`,
//! before `NpcSet::Think`) over every unit with an [`OrderQueue`] in `SimId`
//! order: units without a `Brain`, and `Brain` units whose tree runs the
//! `ExecuteOrder` leaf ([`ExecutingOrder`]). For each it starts the next
//! order when none runs and advances the current one ([`step_unit`]), at
//! most [`MAX_TRANSITIONS`] orders per unit and tick (so a queue of instant
//! orders cannot loop).
//!
//! Movement goes through the locomotion seam: orders write `MoveIntent`
//! and read last tick's `MoveStatus`, so arrival is noticed one tick after
//! the mover reports it.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_ai::{ActiveLeaf, Brain, Status, ThinkRequest};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{SimClock, SimId, SimIndex, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus, clear_move_intent, set_move_intent};
use npc_nav::{BlockedReport, NavAgent, NavState, Path, explain_blocked};

use crate::CommandConfig;
use crate::formation::relocate_blocked;
use crate::types::{
    ActiveOrder, Commandable, ExecutingOrder, MemberOf, OrderProgress, OrderQueue, QueuedOrder,
    Squad, UnitOrder,
};

/// The failure reason of a walk the mover reported blocked for
/// `CommandConfig::block_fail_ticks`; journaled with the nearest reachable
/// point and the blocker (`npc_nav::explain_blocked`).
pub const BLOCKED: &str = "blocked";

/// Default [`BlockedReports::per_tick`].
pub const BLOCKED_REPORTS_PER_TICK: u32 = 8;

/// The per-tick cap on blocked-walk reports: at most `per_tick` failed
/// walks per tick get `OrderFailed { nearest_reachable, blocker }`
/// (`npc_nav::explain_blocked`, units in `SimId` order as the engine runs
/// them); past it a failure carries only its `reason`. Tooling only: not
/// hashed, never read by the sim's decisions.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockedReports {
    pub per_tick: u32,
    tick: u64,
    built: u32,
}

impl Default for BlockedReports {
    fn default() -> Self {
        Self {
            per_tick: BLOCKED_REPORTS_PER_TICK,
            tick: 0,
            built: 0,
        }
    }
}

impl BlockedReports {
    /// A cap of `per_tick` reports per tick.
    pub fn new(per_tick: u32) -> Self {
        Self {
            per_tick,
            ..Self::default()
        }
    }

    /// Takes one report slot of tick `tick`; `false` past the cap.
    fn take(&mut self, tick: u64) -> bool {
        if self.tick != tick {
            (self.tick, self.built) = (tick, 0);
        }
        if self.built >= self.per_tick {
            return false;
        }
        self.built += 1;
        true
    }
}

/// The blocked-walk report for a walk of `unit` that failed as `blocked`
/// (journal on, within [`BlockedReports`]' cap), read before the intent is
/// dropped.
fn blocked_report(world: &mut World, unit: Entity, tick: u64) -> Option<BlockedReport> {
    if !journal::is_recording(world) {
        return None;
    }
    let goal = intent_goal(world, unit)?;
    if !world.get_resource_or_init::<BlockedReports>().take(tick) {
        return None;
    }
    explain_blocked(world, unit, goal)
}

/// At most this many orders start or finish per unit and tick.
pub const MAX_TRANSITIONS: usize = 8;

/// What one step of an order did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderStep {
    /// Still going.
    Running,
    /// Completed (journal `OrderDone`).
    Done,
    /// Ended without completing (journal `OrderFailed { reason }`).
    Failed(String),
    /// From an executor's `start`: not mine, run the built-in executor.
    Builtin,
}

/// An order executor registered by name for one order kind
/// ([`CommandAppExt::register_order_executor`](crate::CommandAppExt::register_order_executor)).
/// It is consulted before the built-in one, so glue can run `Interact`,
/// `Mount` and `Unload` (which have no built-in executor) or take over a
/// built-in kind for some units (its `start` returns [`OrderStep::Builtin`]
/// for the others).
///
/// Executors are found per order ([`OrderExecutors::for_order`]): a
/// [`UnitOrder::Custom`] by its `name`
/// ([`CommandAppExt::register_custom_order`](crate::CommandAppExt::register_custom_order)),
/// an `Interact` first by its `verb`
/// ([`CommandAppExt::register_interact_verb_executor`](crate::CommandAppExt::register_interact_verb_executor),
/// so a game verb coexists with the interaction glue's generic `Interact`
/// executor), every other order by its kind.
///
/// Lifecycle (the engine runs in `NpcCommandSet::Execute`, after
/// `NpcSet::Sense` and before `NpcSet::Think`, one unit at a time in
/// `SimId` order):
///
/// - `validate`: when the order is issued, once **per unit** (on the unit
///   after redirects). A refusal rejects the whole `IssueOrder` with the
///   reason, unless it was issued with `skip_invalid`, which skips just
///   that unit (journaled `OrderFailed { reason: "skipped: .." }`);
/// - `start`: when the order starts, and again when a paused order resumes;
/// - `tick`: every engine pass while it runs;
/// - `cancel`: whenever the order stops running: replaced, stopped,
///   paused by the tree, **and also** after `start`/`tick` returned
///   [`OrderStep::Done`] or [`OrderStep::Failed`] (so cleanup lives in one
///   place).
///
/// All of it is sim code (integers, `SimId` order). Executors must not
/// issue orders to the unit they run on (queue them through a command).
/// [`approach_step`](crate::approach_step) is the shared "walk into reach"
/// step for executors that act on a point or a footprint.
#[derive(Clone, Copy)]
pub struct OrderExecutor {
    pub validate: fn(&World, Entity, &UnitOrder) -> Result<(), String>,
    pub start: fn(&mut World, Entity, &UnitOrder) -> OrderStep,
    pub tick: fn(&mut World, Entity, &UnitOrder) -> OrderStep,
    pub cancel: fn(&mut World, Entity, &UnitOrder),
}

impl OrderExecutor {
    /// An executor that accepts every order and does nothing on cancel.
    pub fn new(
        start: fn(&mut World, Entity, &UnitOrder) -> OrderStep,
        tick: fn(&mut World, Entity, &UnitOrder) -> OrderStep,
    ) -> Self {
        Self {
            validate: |_, _, _| Ok(()),
            start,
            tick,
            cancel: |_, _, _| {},
        }
    }

    pub fn with_validate(
        mut self,
        f: fn(&World, Entity, &UnitOrder) -> Result<(), String>,
    ) -> Self {
        self.validate = f;
        self
    }

    pub fn with_cancel(mut self, f: fn(&mut World, Entity, &UnitOrder)) -> Self {
        self.cancel = f;
        self
    }
}

/// Registered executors: order kind → `(executor name, executor)`, custom
/// order name → executor, `Interact` verb → `(executor name, executor)`.
/// The names join the content hash (`order_executor` `kind=name`,
/// `order_custom` `name`, `order_verb` `verb=name`).
#[derive(Resource, Clone, Default)]
pub struct OrderExecutors {
    pub by_kind: BTreeMap<&'static str, (&'static str, OrderExecutor)>,
    /// [`UnitOrder::Custom`] executors by order name.
    pub custom: BTreeMap<&'static str, OrderExecutor>,
    /// `Interact` executors by verb, consulted before the `"Interact"` kind
    /// executor.
    pub verbs: BTreeMap<&'static str, (&'static str, OrderExecutor)>,
}

impl OrderExecutors {
    /// The executor registered for order kind `kind` (not custom orders or
    /// verbs: see [`OrderExecutors::for_order`]).
    pub fn get(&self, kind: &str) -> Option<OrderExecutor> {
        self.by_kind.get(kind).map(|(_, e)| *e)
    }

    /// The executor that runs `order`: a custom order's by name, an
    /// `Interact`'s by verb (else the `"Interact"` kind's), any other by
    /// kind.
    pub fn for_order(&self, order: &UnitOrder) -> Option<OrderExecutor> {
        match order {
            UnitOrder::Custom { name, .. } => self.custom.get(name.as_str()).copied(),
            UnitOrder::Interact { verb, .. } => self
                .verbs
                .get(verb.as_str())
                .map(|(_, e)| *e)
                .or_else(|| self.get(order.kind())),
            other => self.get(other.kind()),
        }
    }
}

/// Redirects an order given to `unit` to another unit (e.g. a `ByCrew`
/// vehicle forwards to its driver); `None` = keep it. Ownership is checked
/// on the unit the order names.
pub type RedirectFn = fn(&World, Entity) -> Option<Entity>;

/// Registered redirects, sorted by name; the first that answers wins (one
/// hop, no chains).
#[derive(Resource, Clone, Default)]
pub struct OrderRedirects {
    pub redirects: Vec<(&'static str, RedirectFn)>,
}

impl OrderRedirects {
    pub fn add(&mut self, name: &'static str, f: RedirectFn) {
        match self.redirects.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.redirects[i].1 = f,
            Err(i) => self.redirects.insert(i, (name, f)),
        }
    }

    /// Where an order to `unit` goes.
    pub fn resolve(&self, world: &World, unit: Entity) -> Entity {
        self.redirects
            .iter()
            .find_map(|(_, f)| f(world, unit))
            .unwrap_or(unit)
    }
}

fn sim_id(world: &World, e: Entity) -> Option<SimId> {
    world.get::<SimId>(e).copied()
}

fn tick_now(world: &World) -> u64 {
    world.get_resource::<SimClock>().map_or(0, |c| c.tick)
}

fn config(world: &World) -> CommandConfig {
    world
        .get_resource::<CommandConfig>()
        .copied()
        .unwrap_or_default()
}

fn executor_for(world: &World, order: &UnitOrder) -> Option<OrderExecutor> {
    world.get_resource::<OrderExecutors>()?.for_order(order)
}

/// Where an order to `unit` ends up after the registered redirects.
pub fn resolve_unit(world: &World, unit: Entity) -> Entity {
    world
        .get_resource::<OrderRedirects>()
        .map_or(unit, |r| r.resolve(world, unit))
}

/// Gives `unit` (after redirects) `order`: appended behind its other orders
/// when `queue`, else replacing them all (the running order is cancelled
/// and journaled `OrderFailed { reason: "replaced" }`, or `"stopped"` for a
/// `Stop`). Journals `OrderIssued` and asks a `Brain` unit to re-think.
///
/// Unrecorded: call it only from sim code that is itself replayed (a
/// command's `apply`, a system). External input goes through
/// [`IssueOrder`](crate::IssueOrder).
pub fn enqueue_order(world: &mut World, unit: Entity, order: QueuedOrder, queue: bool) {
    let unit = resolve_unit(world, unit);
    if world.get::<OrderQueue>(unit).is_none() {
        return;
    }
    let Some(id) = sim_id(world, unit) else {
        return;
    };
    if !queue {
        let reason = if order.order == UnitOrder::Stop {
            "stopped"
        } else {
            "replaced"
        };
        cancel_orders(world, unit, reason);
    }
    let kind = order.order.journal_name();
    let player = order.player.0;
    if let Some(mut q) = world.get_mut::<OrderQueue>(unit) {
        q.queued.push(order);
    }
    journal::record(world, Some(id), || JournalKind::OrderIssued {
        order: kind,
        queued: queue,
        player,
    });
    if world.get::<Brain>(unit).is_some() {
        world.entity_mut(unit).insert(ThinkRequest);
    }
}

/// Drops all of `unit`'s orders: the running one is cancelled (its
/// executor's `cancel`, movement stopped) and journaled `OrderFailed {
/// reason }`; queued ones are dropped silently.
pub fn cancel_orders(world: &mut World, unit: Entity, reason: &str) {
    let Some(mut q) = world.get_mut::<OrderQueue>(unit) else {
        return;
    };
    let current = q.current.take();
    q.queued.clear();
    if let Some(active) = current {
        end_active(world, unit, &active);
        if active.started.is_some()
            && let Some(id) = sim_id(world, unit)
        {
            let order = active.order.order.journal_name();
            journal::record(world, Some(id), || JournalKind::order_failed(order, reason));
        }
    }
}

/// Cleanup when an order stops running: executors cancel, built-in moves
/// stop the unit.
fn end_active(world: &mut World, unit: Entity, active: &ActiveOrder) {
    if active.progress == OrderProgress::Hooked {
        if let Some(exec) = executor_for(world, &active.order.order) {
            (exec.cancel)(world, unit, &active.order.order);
        }
    } else if !matches!(active.progress, OrderProgress::NotStarted) || active.started.is_some() {
        clear_move_intent(world, unit);
    }
}

/// Pauses the running order (the tree left `ExecuteOrder` for something
/// more urgent): movement stops, an executor's order is cancelled and
/// started again on resume; built-in orders resume where they were.
pub fn pause_orders(world: &mut World, unit: Entity) {
    let Some(mut q) = world.get_mut::<OrderQueue>(unit) else {
        return;
    };
    let Some(mut active) = q.current.take() else {
        return;
    };
    end_active(world, unit, &active);
    if matches!(
        active.progress,
        OrderProgress::Hooked | OrderProgress::WaitingForLeader { .. }
    ) {
        active.progress = OrderProgress::NotStarted;
    }
    if let Some(mut q) = world.get_mut::<OrderQueue>(unit) {
        q.current = Some(active);
    }
}

/// Starts and advances `unit`'s orders for this tick (see the module docs).
pub fn step_unit(world: &mut World, unit: Entity) {
    let tick = tick_now(world);
    let cfg = config(world);
    let Some(id) = sim_id(world, unit) else {
        return;
    };
    for _ in 0..MAX_TRANSITIONS {
        let active = {
            let Some(mut q) = world.get_mut::<OrderQueue>(unit) else {
                return;
            };
            match q.current.take() {
                Some(a) => a,
                None if q.queued.is_empty() => return,
                None => {
                    let next = q.queued.remove(0);
                    ActiveOrder {
                        order: next,
                        started: None,
                        progress: OrderProgress::NotStarted,
                    }
                }
            }
        };
        let mut active = active;
        if active.started.is_none() {
            active.started = Some(tick);
            let order = active.order.order.journal_name();
            journal::record(world, Some(id), || JournalKind::OrderStarted { order });
        }
        let step = run_order(world, unit, id, &mut active, tick, &cfg);
        match step {
            OrderStep::Running | OrderStep::Builtin => {
                if let Some(mut q) = world.get_mut::<OrderQueue>(unit) {
                    q.current = Some(active);
                }
                return;
            }
            OrderStep::Done => {
                end_active(world, unit, &active);
                let order = active.order.order.journal_name();
                journal::record(world, Some(id), || JournalKind::OrderDone { order });
                if let Some(mut q) = world.get_mut::<OrderQueue>(unit) {
                    q.last_ok = true;
                }
            }
            OrderStep::Failed(reason) => {
                // Read before `end_active` drops the walk's intent; tooling
                // only (nothing is computed while the journal is off).
                let report = if reason == BLOCKED {
                    blocked_report(world, unit, tick)
                } else {
                    None
                };
                end_active(world, unit, &active);
                let order = active.order.order.journal_name();
                journal::record(world, Some(id), || JournalKind::OrderFailed {
                    order,
                    reason,
                    nearest_reachable: report.and_then(|r| r.nearest_reachable).map(|p| [p.x, p.z]),
                    blocker: report.and_then(|r| r.blocker),
                });
                if let Some(mut q) = world.get_mut::<OrderQueue>(unit) {
                    q.last_ok = false;
                }
            }
        }
    }
}

/// One step of `active`: a registered executor first, else the built-in.
fn run_order(
    world: &mut World,
    unit: Entity,
    id: SimId,
    active: &mut ActiveOrder,
    tick: u64,
    cfg: &CommandConfig,
) -> OrderStep {
    let order = active.order.order.clone();
    if let Some(exec) = executor_for(world, &order) {
        match active.progress {
            OrderProgress::Hooked => return (exec.tick)(world, unit, &order),
            OrderProgress::NotStarted => match (exec.start)(world, unit, &order) {
                OrderStep::Builtin => {}
                step => {
                    active.progress = OrderProgress::Hooked;
                    return step;
                }
            },
            _ => {}
        }
    }
    builtin(world, unit, id, active, tick, cfg)
}

fn mover(world: &World, unit: Entity) -> Commandable {
    world.get::<Commandable>(unit).copied().unwrap_or_default()
}

fn pos_of(world: &World, unit: Entity) -> Option<SimPos> {
    world.get::<SimPos>(unit).copied()
}

fn intent_goal(world: &World, unit: Entity) -> Option<SimPos> {
    world.get::<MoveIntent>(unit).map(|i| i.goal)
}

fn blocked_too_long(world: &World, unit: Entity, cfg: &CommandConfig) -> bool {
    matches!(
        world.get::<MoveStatus>(unit),
        Some(MoveStatus::Blocked { ticks }) if *ticks >= cfg.block_fail_ticks
    )
}

fn walk(world: &mut World, unit: Entity, goal: SimPos, tolerance: i32) {
    let c = mover(world, unit);
    set_move_intent(
        world,
        unit,
        MoveIntent {
            goal,
            speed_mm_per_tick: c.speed_mm_per_tick,
            tolerance_mm: tolerance,
        },
    );
}

fn reached(pos: SimPos, goal: SimPos, tolerance: i32) -> bool {
    MoveIntent {
        goal,
        speed_mm_per_tick: 0,
        tolerance_mm: tolerance,
    }
    .reached(pos)
}

fn builtin(
    world: &mut World,
    unit: Entity,
    id: SimId,
    active: &mut ActiveOrder,
    tick: u64,
    cfg: &CommandConfig,
) -> OrderStep {
    let Some(pos) = pos_of(world, unit) else {
        return OrderStep::Failed("no position".into());
    };
    let order = active.order.order.clone();
    match order {
        UnitOrder::Stop => {
            clear_move_intent(world, unit);
            OrderStep::Done
        }
        UnitOrder::Hold => {
            if !matches!(active.progress, OrderProgress::Holding { .. }) {
                clear_move_intent(world, unit);
                active.progress = OrderProgress::Holding { at: pos };
            }
            OrderStep::Running
        }
        UnitOrder::Move { .. } | UnitOrder::AttackMove { .. } => {
            let goal = order.move_goal().expect("a move");
            move_step(world, unit, id, active, pos, goal, tick, cfg)
        }
        UnitOrder::Follow { target } => {
            let Some(t) = world.get_resource::<SimIndex>().and_then(|i| i.get(target)) else {
                return OrderStep::Failed("target gone".into());
            };
            let Some(tpos) = pos_of(world, t) else {
                return OrderStep::Failed("target gone".into());
            };
            match intent_goal(world, unit) {
                Some(g) if active.progress == OrderProgress::Moving => {
                    let r = i64::from(cfg.follow_retarget_mm.max(0));
                    if g.dist2(tpos) > r * r
                        && let Some(mut intent) = world.get_mut::<MoveIntent>(unit)
                    {
                        intent.goal = tpos;
                    }
                }
                _ => {
                    walk(world, unit, tpos, cfg.follow_distance_mm);
                    active.progress = OrderProgress::Moving;
                }
            }
            OrderStep::Running
        }
        UnitOrder::Patrol { points } => {
            if !matches!(active.progress, OrderProgress::Patrolling { .. }) {
                let mut route: Vec<SimPos> =
                    points.iter().map(|&(x, z)| SimPos::new(x, z)).collect();
                if route.len() == 1 {
                    route.push(pos);
                }
                active.progress = OrderProgress::Patrolling { route, next: 0 };
            }
            let arrive = mover(world, unit).arrive_mm;
            let OrderProgress::Patrolling { route, next } = &mut active.progress else {
                unreachable!("set above")
            };
            if route.is_empty() {
                return OrderStep::Failed("no patrol points".into());
            }
            let mut target = route[*next as usize % route.len()];
            if reached(pos, target, arrive) {
                *next = (*next + 1) % route.len() as u32;
                target = route[*next as usize];
                walk(world, unit, target, arrive);
            } else if intent_goal(world, unit) != Some(target) {
                walk(world, unit, target, arrive);
            } else if blocked_too_long(world, unit, cfg) {
                return OrderStep::Failed(BLOCKED.into());
            }
            OrderStep::Running
        }
        UnitOrder::Interact { .. } | UnitOrder::Mount { .. } | UnitOrder::Unload => {
            OrderStep::Failed(format!("no executor for `{}`", order.kind()))
        }
        UnitOrder::Custom { name, .. } => {
            OrderStep::Failed(format!("no executor for custom order `{name}`"))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn move_step(
    world: &mut World,
    unit: Entity,
    id: SimId,
    active: &mut ActiveOrder,
    pos: SimPos,
    goal: SimPos,
    tick: u64,
    cfg: &CommandConfig,
) -> OrderStep {
    let arrive = mover(world, unit).arrive_mm;
    if reached(pos, goal, arrive) {
        return OrderStep::Done;
    }
    match active.progress {
        OrderProgress::NotStarted => {
            let follower = active
                .order
                .group
                .filter(|g| g.leader != id && world.get::<NavAgent>(unit).is_some());
            if follower.is_some() {
                clear_move_intent(world, unit);
                active.progress = OrderProgress::WaitingForLeader { since: tick };
                return follow_leader(world, unit, active, goal, tick, cfg);
            }
            walk(world, unit, goal, arrive);
            active.progress = OrderProgress::Moving;
            OrderStep::Running
        }
        OrderProgress::WaitingForLeader { .. } => {
            follow_leader(world, unit, active, goal, tick, cfg)
        }
        _ => {
            if intent_goal(world, unit) != Some(goal) {
                walk(world, unit, goal, arrive);
                active.progress = OrderProgress::Moving;
            } else if blocked_too_long(world, unit, cfg) {
                return OrderStep::Failed(BLOCKED.into());
            }
            OrderStep::Running
        }
    }
}

/// A group follower with a `NavAgent`: once the leader has a path to its
/// own slot, reuse it shifted by the follower's offset (every waypoint
/// re-validated against the grid, the last one replaced by the follower's
/// slot) instead of searching its own. Falls back to its own search when
/// the leader is gone, has no `NavAgent`, failed, follows another goal, or
/// after `leader_wait_ticks`.
fn follow_leader(
    world: &mut World,
    unit: Entity,
    active: &mut ActiveOrder,
    goal: SimPos,
    tick: u64,
    cfg: &CommandConfig,
) -> OrderStep {
    let arrive = mover(world, unit).arrive_mm;
    let OrderProgress::WaitingForLeader { since } = active.progress else {
        return OrderStep::Running;
    };
    let Some(link) = active.order.group else {
        walk(world, unit, goal, arrive);
        active.progress = OrderProgress::Moving;
        return OrderStep::Running;
    };
    let leader_slot = SimPos::new(
        goal.x.saturating_sub(link.offset.0),
        goal.z.saturating_sub(link.offset.1),
    );
    let leader = world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(link.leader))
        .filter(|&l| world.get::<NavAgent>(l).is_some());
    let state = leader.and_then(|l| world.get::<NavState>(l).copied());
    let path = leader.and_then(|l| world.get::<Path>(l).cloned());
    let waited = tick.saturating_sub(since);
    match (state, path) {
        (Some(NavState::Following { goal: g }), Some(path)) if g == leader_slot => {
            let derived = derive_path(world, &path, link.offset, goal, cfg);
            let c = mover(world, unit);
            world.entity_mut(unit).insert((
                MoveIntent {
                    goal,
                    speed_mm_per_tick: c.speed_mm_per_tick,
                    tolerance_mm: arrive,
                },
                MoveStatus::Moving,
                NavState::Following { goal },
                derived,
            ));
            active.progress = OrderProgress::Moving;
        }
        // The leader's request is queued: wait for it (bounded).
        (Some(NavState::Pending { goal: g, .. }), _)
            if g == leader_slot && waited < u64::from(cfg.leader_wait_ticks) => {}
        // The leader has not started yet (it runs later in this pass).
        (Some(NavState::Idle), _) if waited < 2 => {}
        _ => {
            walk(world, unit, goal, arrive);
            active.progress = OrderProgress::Moving;
        }
    }
    OrderStep::Running
}

/// The leader's waypoints shifted by `offset`, re-validated, ending at `goal`.
fn derive_path(
    world: &World,
    leader: &Path,
    offset: (i32, i32),
    goal: SimPos,
    cfg: &CommandConfig,
) -> Path {
    let grid = world.get_resource::<SpatialGrid>();
    let n = leader.waypoints.len();
    let mut waypoints: Vec<SimPos> = Vec::with_capacity(n.max(1));
    for (i, wp) in leader.waypoints.iter().enumerate() {
        if i + 1 == n {
            break;
        }
        let mut p = [SimPos::new(
            wp.x.saturating_add(offset.0),
            wp.z.saturating_add(offset.1),
        )];
        if let Some(grid) = grid {
            relocate_blocked(grid, *wp, &mut p, cfg.relocate_radius_cells);
        }
        if waypoints.last() != Some(&p[0]) {
            waypoints.push(p[0]);
        }
    }
    if waypoints.last() != Some(&goal) {
        waypoints.push(goal);
    }
    Path {
        waypoints,
        markers: Vec::new(),
        next: 0,
    }
}

/// The commandable units ([`CommandQueries`]).
type UnitQuery = QueryState<
    (
        Entity,
        &'static SimId,
        &'static OrderQueue,
        Has<Brain>,
        Has<ExecutingOrder>,
    ),
    With<Commandable>,
>;

/// [`command_engine_system`]'s cached queries (a `QueryState` built every
/// tick costs more than a quiet tick's work).
pub struct CommandQueries {
    members: QueryState<(&'static SimId, Entity, &'static MemberOf)>,
    squads: QueryState<(&'static SimId, &'static Squad)>,
    units: UnitQuery,
}

impl FromWorld for CommandQueries {
    fn from_world(world: &mut World) -> Self {
        Self {
            members: world.query(),
            squads: world.query(),
            units: world.query_filtered(),
        }
    }
}

/// Drops squad members that left the sim or another squad took (sorted),
/// and `MemberOf` of a squad that left the sim (final review cleanup 3: a
/// `Despawn` of the squad itself).
fn prune_squads(world: &mut World, q: &mut CommandQueries) {
    let mut orphans: Vec<(SimId, Entity)> = q
        .members
        .iter(world)
        .filter(|(_, _, of)| {
            world
                .get_resource::<SimIndex>()
                .and_then(|i| i.get(of.0))
                .is_none_or(|e| world.get::<Squad>(e).is_none())
        })
        .map(|(&id, e, _)| (id, e))
        .collect();
    orphans.sort_unstable_by_key(|(id, _)| *id);
    for (_, e) in orphans {
        world.entity_mut(e).remove::<MemberOf>();
    }
    let mut squads: Vec<(SimId, Vec<SimId>)> = q
        .squads
        .iter(world)
        .map(|(&id, s)| (id, s.members.clone()))
        .collect();
    if squads.is_empty() {
        return;
    }
    squads.sort_unstable_by_key(|(id, _)| *id);
    for (squad, members) in squads {
        let keep: Vec<SimId> = members
            .iter()
            .copied()
            .filter(|m| {
                world
                    .get_resource::<SimIndex>()
                    .and_then(|i| i.get(*m))
                    .and_then(|e| world.get::<MemberOf>(e))
                    .is_some_and(|of| of.0 == squad)
            })
            .collect();
        if keep.len() != members.len()
            && let Some(e) = world.get_resource::<SimIndex>().and_then(|i| i.get(squad))
            && let Some(mut s) = world.get_mut::<Squad>(e)
        {
            s.members = keep;
        }
    }
}

/// `NpcCommandSet::Execute` (exclusive): squad pruning, then [`step_unit`]
/// for every unit that executes orders now, in `SimId` order; a `Brain`
/// unit whose queue ran dry gets its `ExecuteOrder` leaf finished
/// (`Success`, or `Failure` when its last order failed).
pub fn command_engine_system(world: &mut World, mut q: Local<CommandQueries>) {
    prune_squads(world, &mut q);
    let mut units: Vec<(SimId, Entity, bool)> = q
        .units
        .iter(world)
        .filter(|(_, _, queue, brain, executing)| {
            (!brain || *executing) && (queue.has_order() || *brain)
        })
        .map(|(e, &id, _, brain, _)| (id, e, brain))
        .collect();
    units.sort_unstable_by_key(|(id, _, _)| *id);
    for (_, unit, brain) in units {
        step_unit(world, unit);
        if brain {
            finish_leaf_if_dry(world, unit);
        }
    }
}

/// Ends a running `ExecuteOrder` leaf when nothing is left to do.
pub(crate) fn finish_leaf_if_dry(world: &mut World, unit: Entity) {
    let Some(q) = world.get::<OrderQueue>(unit) else {
        return;
    };
    if q.has_order() {
        return;
    }
    let status = if q.last_ok {
        Status::Success
    } else {
        Status::Failure
    };
    if let Some(mut leaf) = world.get_mut::<ActiveLeaf>(unit)
        && leaf.status == Status::Running
    {
        leaf.status = status;
    }
}
