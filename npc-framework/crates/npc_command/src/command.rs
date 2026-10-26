//! The recorded commands: [`IssueOrder`], [`CreateSquad`], [`DisbandSquad`].
//!
//! Every order is a `SimCommand` naming the [`PlayerId`] that gives it, and
//! is validated against ownership when submitted and again when it applies
//! (the kernel re-validates every command at its tick), so replays and a
//! future lock-step multiplayer see the same result.
//!
//! JSON, e.g.
//! `{"type":"IssueOrder","player":1,"units":[5,6,7],"order":{"type":"Move","pos":[10000,0]},"queue":true}`
//! (optional `"formation":{"kind":"Line","spacing_mm":2000}`, and
//! `"skip_invalid":true` to order only the units that can take the order);
//! `{"type":"CreateSquad","player":1,"members":[5,6,7]}`;
//! `{"type":"DisbandSquad","player":1,"squad":12}`.

use bevy_ecs::prelude::*;
use npc_kernel::commands::{CommandError, check_pos, entity, invalid};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{
    ContentCommand, SimClock, SimCommand, SimId, SimIndex, SimPos, spawn_sim, submit,
};
use serde::{Deserialize, Serialize};

use crate::CommandConfig;
use crate::exec::{OrderExecutor, OrderExecutors, enqueue_order, resolve_unit};
use crate::formation::{Formation, plan_group};
use crate::types::{
    Commandable, GroupLink, MemberOf, OrderQueue, Owner, PlayerId, QueuedOrder, Squad, UnitOrder,
};

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

/// The owner of `entity` (`None`: not ownable).
pub fn owner_of(world: &World, entity: Entity) -> Option<PlayerId> {
    world.get::<Owner>(entity).map(|o| o.0)
}

/// Checks that `player` may order the unit or squad `id`.
pub fn check_owner(world: &World, player: PlayerId, id: SimId) -> Result<Entity, CommandError> {
    let e = entity(world, id.0)?;
    if world.get::<Commandable>(e).is_none() && world.get::<Squad>(e).is_none() {
        return Err(CommandError::WrongKind {
            unit: id.0,
            expected: "commandable unit or squad".into(),
        });
    }
    match owner_of(world, e) {
        Some(owner) if owner == player => Ok(e),
        Some(owner) => Err(unavailable(format!(
            "unit {} is owned by player {}, not player {}",
            id.0, owner.0, player.0
        ))),
        None => Err(unavailable(format!("unit {} has no owner", id.0))),
    }
}

/// `ids` with squads replaced by their members, sorted by `SimId`, without
/// duplicates (ids that are gone are kept: validation reports them).
pub fn expand_units(world: &World, ids: &[SimId]) -> Vec<SimId> {
    let index = world.get_resource::<SimIndex>();
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        match index
            .and_then(|i| i.get(id))
            .and_then(|e| world.get::<Squad>(e))
        {
            Some(squad) => out.extend(squad.members.iter().copied()),
            None => out.push(id),
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// One order to a unit, a list of units or squads (`units`), from
/// `player`. With `queue` it is appended behind each unit's orders
/// (shift-queue), else it replaces them. A `Move` / `AttackMove` to two or
/// more units is a **group move**: every unit gets its own formation slot
/// around `pos` (`formation`, else the squad's when exactly one squad is
/// named, else `CommandConfig::default_formation`), validated against the
/// grid, and followers with a `NavAgent` reuse the leader's path.
///
/// By default one unit that cannot take the order (another player's, its
/// queue full, an executor's `validate` refuses it) rejects the whole
/// command. With `skip_invalid` the order goes to the units that pass and
/// the others are skipped: each skipped unit is journaled `OrderFailed {
/// order, reason: "skipped: <why>" }` when the command applies, and
/// [`submit_order_report`] / [`IssueOrder::partition`] list them up front.
/// The command is still rejected when no unit passes, or for checks that
/// do not depend on the unit (no units, too many units, a bad formation,
/// an invalid order, no executor).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IssueOrder {
    pub player: PlayerId,
    pub units: Vec<SimId>,
    pub order: UnitOrder,
    #[serde(default)]
    pub queue: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formation: Option<Formation>,
    /// Order only the units that can take it; skip (and journal) the rest.
    /// Default `false`, and then not serialized, so recorded commands stay
    /// byte-identical.
    #[serde(default, skip_serializing_if = "is_false")]
    pub skip_invalid: bool,
}

// serde's `skip_serializing_if` passes a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

/// A unit an [`IssueOrder`] with `skip_invalid` leaves out, and why.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkippedUnit {
    pub unit: SimId,
    pub reason: String,
}

/// What [`submit_order_report`] queued: the command's `(tick, seq)`, the
/// units that will take the order (squads expanded, sorted) and the ones
/// skipped (`skip_invalid`), as of submission (the command checks again
/// when it applies; its journal is authoritative).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueReceipt {
    pub tick: u64,
    pub seq: u32,
    pub units: Vec<SimId>,
    pub skipped: Vec<SkippedUnit>,
}

impl IssueOrder {
    /// `order` to `units` from `player`, replacing their orders.
    pub fn new(player: PlayerId, units: impl IntoIterator<Item = SimId>, order: UnitOrder) -> Self {
        Self {
            player,
            units: units.into_iter().collect(),
            order,
            queue: false,
            formation: None,
            skip_invalid: false,
        }
    }

    /// Shift-queue (`true`) or replace (`false`).
    pub fn queued(mut self, queue: bool) -> Self {
        self.queue = queue;
        self
    }

    pub fn formation(mut self, formation: Formation) -> Self {
        self.formation = Some(formation);
        self
    }

    /// Skip the units that cannot take the order instead of rejecting the
    /// command (see the type docs).
    pub fn skip_invalid(mut self, skip: bool) -> Self {
        self.skip_invalid = skip;
        self
    }

    /// The units that take this order (squads expanded, sorted by `SimId`)
    /// and, with `skip_invalid`, the ones skipped with their reasons
    /// (sorted by unit). Without `skip_invalid` the first failing unit is
    /// the error, exactly as [`ContentCommand::validate`] reports it.
    ///
    /// # Errors
    /// What rejects the whole command (see the type docs).
    pub fn partition(&self, world: &World) -> Result<(Vec<SimId>, Vec<SkippedUnit>), CommandError> {
        if !self.skip_invalid {
            self.validate(world)?;
            return Ok((expand_units(world, &self.units), Vec::new()));
        }
        let cfg = world
            .get_resource::<CommandConfig>()
            .copied()
            .unwrap_or_default();
        if self.units.is_empty() {
            return Err(invalid("units", "no units".into()));
        }
        let mut skipped: Vec<SkippedUnit> = Vec::new();
        let mut first_err: Option<CommandError> = None;
        let mut named: Vec<SimId> = Vec::with_capacity(self.units.len());
        for &id in &self.units {
            match check_owner(world, self.player, id) {
                Ok(_) => named.push(id),
                Err(e) => {
                    skip_unit(world, id, &e, &mut skipped);
                    first_err.get_or_insert(e);
                }
            }
        }
        let units = expand_units(world, &named);
        if units.len() > cfg.max_group as usize {
            return Err(invalid(
                "units",
                format!("{} units, at most {}", units.len(), cfg.max_group),
            ));
        }
        if let Some(f) = self.formation {
            f.validate()?;
        }
        check_order(world, &self.order, &cfg)?;
        let exec = world
            .get_resource::<OrderExecutors>()
            .and_then(|x| x.for_order(&self.order));
        let mut ok = Vec::with_capacity(units.len());
        for id in units {
            match check_unit(world, self, id, exec, &cfg) {
                Ok(()) => ok.push(id),
                Err(e) => {
                    skip_unit(world, id, &e, &mut skipped);
                    first_err.get_or_insert(e);
                }
            }
        }
        if ok.is_empty() {
            return Err(first_err.unwrap_or_else(|| invalid("units", "no units".into())));
        }
        skipped.sort_by_key(|s| s.unit);
        skipped.dedup_by_key(|s| s.unit);
        Ok((ok, skipped))
    }

    /// As a `SimCommand` (what `submit`, replays and BRP carry).
    pub fn to_command(&self) -> SimCommand {
        SimCommand::content(self)
    }
}

fn check_order(world: &World, order: &UnitOrder, cfg: &CommandConfig) -> Result<(), CommandError> {
    match order {
        UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } => {
            check_pos("order.pos", *pos)?;
        }
        UnitOrder::Follow { target } => {
            entity(world, target.0)?;
        }
        UnitOrder::Patrol { points } => {
            if points.is_empty() || points.len() > cfg.max_patrol_points as usize {
                return Err(invalid(
                    "order.points",
                    format!("1..={} points, got {}", cfg.max_patrol_points, points.len()),
                ));
            }
            for &p in points {
                check_pos("order.points", p)?;
            }
        }
        UnitOrder::Interact { target, .. } => {
            entity(world, target.0)?;
        }
        UnitOrder::Mount { vehicle } => {
            entity(world, vehicle.0)?;
        }
        UnitOrder::Stop | UnitOrder::Hold | UnitOrder::Unload => {}
        UnitOrder::Custom { name, .. } => {
            if name.is_empty() {
                return Err(invalid("order.name", "empty custom order name".into()));
            }
        }
    }
    let needs_executor = matches!(
        order,
        UnitOrder::Interact { .. }
            | UnitOrder::Mount { .. }
            | UnitOrder::Unload
            | UnitOrder::Custom { .. }
    );
    let has_executor = world
        .get_resource::<OrderExecutors>()
        .is_some_and(|x| x.for_order(order).is_some());
    if needs_executor && !has_executor {
        if let UnitOrder::Custom { name, .. } = order {
            let known: Vec<&str> = world
                .get_resource::<OrderExecutors>()
                .map(|x| x.custom.keys().copied().collect())
                .unwrap_or_default();
            return Err(unavailable(format!(
                "no executor registered for custom order `{name}` (known: {})",
                known.join(", ")
            )));
        }
        return Err(unavailable(format!(
            "no executor registered for order `{}` (its capability glue is not installed)",
            order.kind()
        )));
    }
    Ok(())
}

/// Records `id` as skipped (a squad: each of its members).
fn skip_unit(world: &World, id: SimId, err: &CommandError, skipped: &mut Vec<SkippedUnit>) {
    let reason = err.to_string();
    for unit in expand_units(world, &[id]) {
        skipped.push(SkippedUnit {
            unit,
            reason: reason.clone(),
        });
    }
}

/// The per-unit checks of an [`IssueOrder`] for (expanded) unit `id`.
fn check_unit(
    world: &World,
    cmd: &IssueOrder,
    id: SimId,
    exec: Option<OrderExecutor>,
    cfg: &CommandConfig,
) -> Result<(), CommandError> {
    let e = check_owner(world, cmd.player, id)?;
    if world.get::<Commandable>(e).is_none() {
        return Err(CommandError::WrongKind {
            unit: id.0,
            expected: "commandable unit".into(),
        });
    }
    if cmd.order.target() == Some(id) {
        return Err(invalid(
            "order",
            format!("unit {} cannot target itself", id.0),
        ));
    }
    let e = resolve_unit(world, e);
    if cmd.queue
        && world
            .get::<OrderQueue>(e)
            .is_some_and(|q| q.len() >= cfg.max_queue as usize)
    {
        return Err(unavailable(format!(
            "unit {}'s order queue is full ({})",
            id.0, cfg.max_queue
        )));
    }
    if let Some(exec) = exec {
        (exec.validate)(world, e, &cmd.order).map_err(unavailable)?;
    }
    Ok(())
}

impl ContentCommand for IssueOrder {
    const NAME: &'static str = "IssueOrder";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        if self.skip_invalid {
            return self.partition(world).map(|_| ());
        }
        let cfg = world
            .get_resource::<CommandConfig>()
            .copied()
            .unwrap_or_default();
        if self.units.is_empty() {
            return Err(invalid("units", "no units".into()));
        }
        for &id in &self.units {
            check_owner(world, self.player, id)?;
        }
        let units = expand_units(world, &self.units);
        if units.len() > cfg.max_group as usize {
            return Err(invalid(
                "units",
                format!("{} units, at most {}", units.len(), cfg.max_group),
            ));
        }
        if let Some(f) = self.formation {
            f.validate()?;
        }
        check_order(world, &self.order, &cfg)?;
        let exec = world
            .get_resource::<OrderExecutors>()
            .and_then(|x| x.for_order(&self.order));
        for &id in &units {
            check_unit(world, self, id, exec, &cfg)?;
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        if !self.skip_invalid {
            issue(world, &self);
            return;
        }
        // Validated just before; partition again for the units to skip.
        let Ok((units, skipped)) = self.partition(world) else {
            return;
        };
        let order = self.order.journal_name();
        for s in skipped {
            let reason = format!("skipped: {}", s.reason);
            let order = order.clone();
            journal::record(world, Some(s.unit), || {
                JournalKind::order_failed(order, reason)
            });
        }
        issue_to(world, &self, &units);
    }

    fn unit(&self) -> Option<SimId> {
        self.units.first().copied()
    }
}

/// Where `unit` starts a new order from: the end of its queued orders when
/// shift-queueing, else its position.
fn start_point(world: &World, unit: Entity, queue: bool) -> Option<SimPos> {
    let here = world.get::<SimPos>(unit).copied();
    if !queue {
        return here;
    }
    world
        .get::<OrderQueue>(unit)
        .and_then(|q| q.orders().filter_map(UnitOrder::end_point).last())
        .or(here)
}

/// Applies a validated [`IssueOrder`] (also callable from replayed sim
/// code, e.g. a glue command's `apply`). Every unit named gets the order
/// (`skip_invalid` is the command's business: see [`issue_to`]).
pub fn issue(world: &mut World, cmd: &IssueOrder) {
    let units = expand_units(world, &cmd.units);
    issue_to(world, cmd, &units);
}

/// [`issue`] to just `units` (expanded unit ids, e.g. the first list of
/// [`IssueOrder::partition`]); the formation still follows `cmd` (its own,
/// or its one squad's). Unrecorded: from replayed sim code only.
pub fn issue_to(world: &mut World, cmd: &IssueOrder, units: &[SimId]) {
    let cfg = world
        .get_resource::<CommandConfig>()
        .copied()
        .unwrap_or_default();
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let index = |world: &World, id: SimId| world.get_resource::<SimIndex>().and_then(|i| i.get(id));
    let mut entities: Vec<(SimId, Entity)> = units
        .iter()
        .filter_map(|&id| index(world, id).map(|e| (id, e)))
        .collect();
    entities.sort_unstable_by_key(|(id, _)| *id);

    let squad_formation = match cmd.units.as_slice() {
        [one] => index(world, *one)
            .and_then(|e| world.get::<Squad>(e))
            .map(|s| s.formation),
        _ => None,
    };
    let formation = cmd
        .formation
        .or(squad_formation)
        .unwrap_or(cfg.default_formation);

    let mut slots: Vec<(SimId, SimPos, Option<GroupLink>)> = Vec::new();
    if let Some(goal) = cmd.order.move_goal()
        && entities.len() >= 2
    {
        let from: Vec<(SimId, SimPos)> = entities
            .iter()
            .filter_map(|&(id, e)| start_point(world, e, cmd.queue).map(|p| (id, p)))
            .collect();
        if !from.is_empty() {
            let plan = plan_group(
                formation,
                goal,
                &from,
                world.get_resource::<SpatialGrid>(),
                cfg.relocate_radius_cells,
            );
            let leader_slot = plan.slot_of(plan.leader).unwrap_or(goal);
            for &(id, slot) in &plan.slots {
                let link = GroupLink {
                    leader: plan.leader,
                    offset: (
                        slot.x.saturating_sub(leader_slot.x),
                        slot.z.saturating_sub(leader_slot.z),
                    ),
                };
                slots.push((id, slot, Some(link)));
            }
        }
    }

    for (id, e) in entities {
        let (order, group) = match slots.binary_search_by_key(&id, |(i, _, _)| *i) {
            Ok(i) => {
                let (_, slot, link) = slots[i];
                let pos = (slot.x, slot.z);
                let order = match cmd.order {
                    UnitOrder::AttackMove { .. } => UnitOrder::AttackMove { pos },
                    _ => UnitOrder::Move { pos },
                };
                (order, link)
            }
            Err(_) => (cmd.order.clone(), None),
        };
        enqueue_order(
            world,
            e,
            QueuedOrder {
                order,
                group,
                player: cmd.player,
                issued: tick,
            },
            cmd.queue,
        );
    }
}

/// Validates and queues `cmd` for the next tick (recorded, replayable):
/// the one call a host UI, an agent or a script needs.
pub fn submit_order(world: &mut World, cmd: &IssueOrder) -> Result<(u64, u32), CommandError> {
    submit(world, None, cmd.to_command())
}

/// [`submit_order`] that also says which units take the order and which
/// are skipped (`skip_invalid`), for a UI or an agent to report.
///
/// # Errors
/// As [`submit_order`].
pub fn submit_order_report(
    world: &mut World,
    cmd: &IssueOrder,
) -> Result<IssueReceipt, CommandError> {
    let (units, skipped) = cmd.partition(world)?;
    let (tick, seq) = submit_order(world, cmd)?;
    Ok(IssueReceipt {
        tick,
        seq,
        units,
        skipped,
    })
}

/// Forms a squad of `members` owned by `player` (each leaves its old squad).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreateSquad {
    pub player: PlayerId,
    pub members: Vec<SimId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formation: Option<Formation>,
}

impl ContentCommand for CreateSquad {
    const NAME: &'static str = "CreateSquad";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        if self.members.is_empty() {
            return Err(invalid("members", "no members".into()));
        }
        if let Some(f) = self.formation {
            f.validate()?;
        }
        for &id in &self.members {
            let e = check_owner(world, self.player, id)?;
            if world.get::<Commandable>(e).is_none() {
                return Err(CommandError::WrongKind {
                    unit: id.0,
                    expected: "commandable unit".into(),
                });
            }
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        create_squad(world, self.player, &self.members, self.formation);
    }

    fn unit(&self) -> Option<SimId> {
        self.members.first().copied()
    }
}

/// Spawns a squad of `members` (sorted, deduplicated; each leaves its old
/// squad) owned by `player`; returns its id. Unrecorded: from replayed sim
/// code only (external input: [`CreateSquad`]).
pub fn create_squad(
    world: &mut World,
    player: PlayerId,
    members: &[SimId],
    formation: Option<Formation>,
) -> SimId {
    let mut members = members.to_vec();
    members.sort_unstable();
    members.dedup();
    let formation = formation.unwrap_or_else(|| {
        world
            .get_resource::<CommandConfig>()
            .copied()
            .unwrap_or_default()
            .default_formation
    });
    let (squad, _) = spawn_sim(
        world,
        (
            Squad {
                members: members.clone(),
                formation,
            },
            Owner(player),
        ),
    );
    for id in members {
        if let Some(e) = world.get_resource::<SimIndex>().and_then(|i| i.get(id)) {
            world.entity_mut(e).insert(MemberOf(squad));
        }
    }
    // Old squads drop the moved members on the next engine pass.
    squad
}

/// Dissolves `squad` (its members stay, without a squad).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisbandSquad {
    pub player: PlayerId,
    pub squad: SimId,
}

impl ContentCommand for DisbandSquad {
    const NAME: &'static str = "DisbandSquad";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = check_owner(world, self.player, self.squad)?;
        if world.get::<Squad>(e).is_none() {
            return Err(CommandError::WrongKind {
                unit: self.squad.0,
                expected: "squad".into(),
            });
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.squad.0) else {
            return;
        };
        let members = world
            .get::<Squad>(e)
            .map(|s| s.members.clone())
            .unwrap_or_default();
        for id in members {
            if let Some(m) = world.get_resource::<SimIndex>().and_then(|i| i.get(id))
                && world.get::<MemberOf>(m) == Some(&MemberOf(self.squad))
            {
                world.entity_mut(m).remove::<MemberOf>();
            }
        }
        world.despawn(e);
    }

    fn unit(&self) -> Option<SimId> {
        Some(self.squad)
    }
}
