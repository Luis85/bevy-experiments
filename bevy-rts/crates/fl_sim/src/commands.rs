//! Player commands, the only input to the sim (spec §5.5, ADR-0007): handlers, the registry,
//! the inbox, validation and apply, and the core handlers (`debug.*` handlers live in
//! [`debug_cmds`](crate::debug_cmds)).
//!
//! A [`PlayerCommand`] carries a [`CommandKind`]: a [`CommandTypeId`] plus the postcard bytes of
//! a [`CommandPayload`] (the data, defined in `fl_protocol` or the feature crate). Features
//! register each payload with a [`CommandHandler`] through
//! [`SimBuilder::register_command`](crate::SimBuilder::register_command); type ids are positions
//! in payload-name order, fixed when the sim is built.
//!
//! [`step_tick`](crate::step_tick) puts each tick's commands into the [`CommandInbox`], and
//! [`apply_commands`] runs them in `(player, seq)` order. Commands that fail validation change
//! no gameplay state and land in [`CommandLog::rejected`].
//!
//! Journal records (phase `Input`): `"core.command_applied"` or `"core.command_rejected"`
//! for every command, and `"core.order_changed"` for every unit whose [`MoveOrder`] a core
//! command changed (with `"core.path_request"` for every path a move asks for).
//!
//! The core order commands, `"core.move"`, `"core.rotate"` and `"core.follow"`, share the
//! Shift-queue rules: without `queue` the units' orders become the new order; with `queue`
//! it is appended, and if any of the units already holds [`MAX_WAYPOINTS`] orders the whole
//! command is [`RejectReason::QueueFull`], changing nothing. `"core.stop"` is never queued: it
//! drops every order at once whether or not `queue` is set.
//!
//! Every payload coordinate comes from the wire and is clamped to the map (or validated)
//! before any arithmetic (R104), so an extreme value can neither overflow (a panic in debug
//! builds, which would stop a threaded runner) nor wrap differently in release builds.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityRef, World};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandKind, CommandPayload, CommandTypeId, FollowCmd, MoveCmd, PlayerCommand, PlayerId,
    RotateCmd, SimId, StopCmd, Tick, tables,
};
use fl_view::SimEvent;
use serde::{Deserialize, Serialize};

use crate::build::{Content, MatchHeaderRes};
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::events::SimEvents;
use crate::hash::{SimHash, StateHasher};
use crate::ids::{SimEntity, SimIndex};
use crate::map::MapInfo;
use crate::movement::{MovementRules, SteeringRules};
use crate::nav::{MoveProfile, PathRules};
use crate::observe::{Journal, JournalRecord};
use crate::orders::{Reason, formation_targets, idle, journal_order_changed, space_slots};
use crate::path::{PathFollow, PathState, enqueue_group_in, enqueue_path_in, unit_profile};
use crate::players::Players;
use crate::registry::StableRegistry;
use crate::save::LoadCtx;
use crate::schedule::{InputSet, Phase};
use crate::units::{Footprint, Mobility, MoveOrder, Order, Owner, Pose};

/// What the sim does with one payload type `P`.
pub trait CommandHandler<P: CommandPayload>: Send + Sync + 'static {
    /// Whether `unit` can carry out `payload` (usually: it has the enabling component).
    fn unit_can(&self, _payload: &P, _unit: EntityRef<'_>) -> bool {
        true
    }

    /// Carry out `payload` for `ctx.units`, which are already validated.
    ///
    /// # Errors
    /// Why the command could not be carried out; the sim logs it as rejected. A handler that
    /// rejects must not have changed anything.
    fn apply(
        &self,
        payload: &P,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason>;
}

/// `"core.invalid_target"` (R79): the command's target is a cell its units cannot stand on,
/// or a rect outside the map. A [`RejectReason::Feature`] code, not a new variant (R71).
pub(crate) const INVALID_TARGET: RejectReason = RejectReason::Feature("core.invalid_target");

/// What [`CommandHandler::apply`] gets besides the payload.
pub struct CommandCtx<'a> {
    pub tick: Tick,
    pub player: PlayerId,
    pub seq: u32,
    /// The command's units that exist, belong to `player` (any owner for a `DEBUG_ONLY`
    /// type) and pass [`CommandHandler::unit_can`]: deduplicated, in [`SimId`] order.
    pub units: &'a [(SimId, Entity)],
    pub queue: bool,
}

/// The most units one [`PlayerCommand`] may name (before deduplication); more is
/// [`RejectReason::TooManyUnits`]. A core rule: commands are hashed input, which a peer could
/// otherwise grow at will.
pub const MAX_COMMAND_UNITS: usize = 256;

/// The most orders a [`MoveOrder`] holds, every kind counted (R77); a queued order onto a full
/// queue is [`RejectReason::QueueFull`]. A core rule, like [`MAX_COMMAND_UNITS`].
pub const MAX_WAYPOINTS: usize = 32;

/// Why a command was not applied, in the order [`apply_commands`] checks. Each reason has a
/// stable, namespaced [`code`](Self::code), which the sim reports
/// ([`SimEvent::CommandRejected`], the journal) instead of the variant name.
///
/// Non-exhaustive: a feature that needs its own reason returns [`RejectReason::Feature`] with
/// a `"<feature>.<reason>"` code; it never adds a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, thiserror::Error)]
#[non_exhaustive]
pub enum RejectReason {
    #[error("the command's tick has already run")]
    Late,
    #[error("the command's tick has not come yet")]
    Early,
    #[error("the player already sent a command with this seq or a later one")]
    DuplicateSeq,
    #[error("no command type has this id")]
    UnknownType,
    #[error("the payload does not decode")]
    BadPayload,
    #[error("no such player")]
    UnknownPlayer,
    #[error("debug commands are disabled")]
    DebugDisabled,
    #[error("none of the units can carry out the command")]
    NoValidUnits,
    #[error("the payload names content that does not exist")]
    UnknownContent,
    #[error("the command names more than {MAX_COMMAND_UNITS} units")]
    TooManyUnits,
    #[error("the unit already holds {MAX_WAYPOINTS} orders")]
    QueueFull,
    /// A feature's own reason: its stable code, `"<feature>.<reason>"` (such as
    /// `"infantry.building_full"`), reported verbatim.
    #[error("rejected by a feature: {0}")]
    Feature(&'static str),
}

impl RejectReason {
    /// The stable code of the reason: `"core.<reason>"` for the core reasons, a
    /// [`Feature`](Self::Feature) reason's own code verbatim.
    pub fn code(&self) -> &'static str {
        match self {
            RejectReason::Late => "core.late",
            RejectReason::Early => "core.early",
            RejectReason::DuplicateSeq => "core.duplicate_seq",
            RejectReason::UnknownType => "core.unknown_type",
            RejectReason::BadPayload => "core.bad_payload",
            RejectReason::UnknownPlayer => "core.unknown_player",
            RejectReason::DebugDisabled => "core.debug_disabled",
            RejectReason::NoValidUnits => "core.no_valid_units",
            RejectReason::UnknownContent => "core.unknown_content",
            RejectReason::TooManyUnits => "core.too_many_units",
            RejectReason::QueueFull => "core.queue_full",
            RejectReason::Feature(code) => code,
        }
    }
}

/// Decodes, validates and applies one command of a registered payload type; returns the
/// units it was applied to.
type RunFn =
    Arc<dyn Fn(&mut World, &PlayerCommand) -> Result<Vec<SimId>, RejectReason> + Send + Sync>;

/// A registered payload type and its handler.
pub(crate) struct CommandEntry {
    type_id: TypeId,
    run: RunFn,
}

impl CommandEntry {
    pub(crate) fn new<P: CommandPayload>(handler: impl CommandHandler<P>) -> Self {
        CommandEntry {
            type_id: TypeId::of::<P>(),
            run: Arc::new(move |world, cmd| run_command::<P>(&handler, world, cmd)),
        }
    }
}

/// Every registered payload type, sorted by name; a type's [`CommandTypeId`] is its dense id.
/// External `"core.command_registry"`.
#[derive(Resource)]
pub struct CommandRegistry {
    registry: StableRegistry<CommandEntry>,
}

impl CommandRegistry {
    /// # Panics
    /// If `registry` is not frozen, or holds more names than a `CommandTypeId` can number.
    pub(crate) fn new(registry: StableRegistry<CommandEntry>) -> Self {
        assert!(registry.is_frozen(), "the command registry must be frozen");
        assert!(
            registry.len() <= usize::from(u16::MAX),
            "too many command types"
        );
        CommandRegistry { registry }
    }

    /// The id of payload type `P`.
    ///
    /// # Panics
    /// If `P` is not registered.
    pub fn type_id<P: CommandPayload>(&self) -> CommandTypeId {
        let id = self
            .registry
            .id(P::NAME)
            .filter(|&id| {
                self.registry
                    .get_by_id(id)
                    .is_some_and(|e| e.type_id == TypeId::of::<P>())
            })
            .unwrap_or_else(|| {
                panic!(
                    "command {} ('{}') is not registered",
                    std::any::type_name::<P>(),
                    P::NAME
                )
            });
        CommandTypeId(u16::try_from(id).expect("new() caps the count at u16::MAX"))
    }

    /// `payload` as a [`CommandKind`].
    ///
    /// # Panics
    /// As [`type_id`](Self::type_id) does.
    pub fn encode<P: CommandPayload>(&self, payload: &P) -> CommandKind {
        CommandKind {
            type_id: self.type_id::<P>(),
            payload: postcard::to_allocvec(payload).expect("command payloads serialize"),
        }
    }

    /// The name of command type `id`.
    pub fn name(&self, id: CommandTypeId) -> Option<&'static str> {
        self.registry.name(u32::from(id.0))
    }

    fn runner(&self, id: CommandTypeId) -> Option<RunFn> {
        self.registry
            .get_by_id(u32::from(id.0))
            .map(|e| Arc::clone(&e.run))
    }
}

/// The id in this sim of the command type that had dense id `id` in the `"commands"` string
/// table of `ctx`'s save (or replay): command type ids are positions in name order, so they
/// move when a command name sorting before them is added. An id the saved table never had
/// stays unknown (`u16::MAX`, never a valid id), so the command is still rejected as
/// [`RejectReason::UnknownType`].
///
/// # Errors
/// The save has no `"commands"` table, or the name no longer exists in this sim.
pub(crate) fn remap_command_id(
    ctx: &LoadCtx<'_>,
    id: CommandTypeId,
) -> Result<CommandTypeId, String> {
    let saved = ctx
        .saved_tables()
        .get(tables::COMMANDS)
        .ok_or_else(|| format!("the save has no string table '{}'", tables::COMMANDS))?
        .len();
    if usize::from(id.0) >= saved {
        return Ok(CommandTypeId(u16::MAX));
    }
    let current = ctx.remap(tables::COMMANDS, u32::from(id.0))?;
    Ok(CommandTypeId(
        u16::try_from(current).expect("CommandRegistry caps the count at u16::MAX"),
    ))
}

/// [`remap_command_id`] for every command of `commands`, in place.
///
/// # Errors
/// As [`remap_command_id`].
pub(crate) fn remap_command_ids(
    ctx: &LoadCtx<'_>,
    commands: &mut [PlayerCommand],
) -> Result<(), String> {
    for cmd in commands {
        cmd.kind.type_id = remap_command_id(ctx, cmd.kind.type_id)?;
    }
    Ok(())
}

/// This tick's commands, in `(player, seq)` order: filled by
/// [`step_tick`](crate::step_tick), emptied by [`apply_commands`]. Transient (kept: filled
/// before the tick) `"core.command_inbox"`.
#[derive(Resource, Default, Debug)]
pub struct CommandInbox(pub Vec<PlayerCommand>);

/// The highest command `seq` the sim has taken from each player: every command that is
/// neither late, early nor a duplicate takes its seq, whether or not it then applies. Hashed
/// `"core.last_seq"`; a client restores its command stamper from it after a load
/// ([`last_seqs`]).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastSeq(pub BTreeMap<PlayerId, u32>);

impl SimHash for LastSeq {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u64(self.0.len() as u64);
        for (player, seq) in &self.0 {
            player.sim_hash(h);
            seq.sim_hash(h);
        }
    }
}

/// The highest `seq` taken from each player so far.
pub fn last_seqs(world: &World) -> BTreeMap<PlayerId, u32> {
    world
        .get_resource::<LastSeq>()
        .map(|last| last.0.clone())
        .unwrap_or_default()
}

/// The commands the sim applied or rejected this tick, in apply order. Transient
/// `"core.command_log"`: reset to empty at the start of every tick, in
/// [`InputSet::Housekeeping`](crate::InputSet::Housekeeping).
#[derive(Resource, Default, Debug)]
pub struct CommandLog {
    pub applied: Vec<PlayerCommand>,
    pub rejected: Vec<(PlayerCommand, RejectReason)>,
}

impl CommandLog {
    /// Empty both lists, keeping their buffers: the Transient reset of `"core.command_log"`.
    fn clear(&mut self) {
        self.applied.clear();
        self.rejected.clear();
    }
}

/// Journal `"core.command_applied"`: a command was carried out for `units` (the validated
/// ones, in `SimId` order).
#[derive(Debug, Serialize)]
struct CommandApplied {
    /// The command type's name.
    command: String,
    seq: u32,
    units: Vec<SimId>,
    queue: bool,
}

impl JournalRecord for CommandApplied {
    const KIND: &'static str = "core.command_applied";
}

/// Journal `"core.command_rejected"`: a command was not carried out, for `reason` (its
/// [`RejectReason::code`]).
#[derive(Debug, Serialize)]
struct CommandRejected {
    /// The command type's name, or `"#<type id>"` for an unknown type.
    command: String,
    seq: u32,
    reason: String,
}

impl JournalRecord for CommandRejected {
    const KIND: &'static str = "core.command_rejected";
}

/// The name of `cmd`'s command type, `"#<type id>"` if it has none.
fn command_name(world: &World, cmd: &PlayerCommand) -> String {
    world
        .resource::<CommandRegistry>()
        .name(cmd.kind.type_id)
        .map_or_else(|| format!("#{}", cmd.kind.type_id.0), str::to_owned)
}

/// Log `cmd` as rejected in `tick`: a `debug` record, the [`CommandLog`], counter
/// `"commands.rejected"`, [`SimEvent::CommandRejected`] and journal
/// `"core.command_rejected"`, both with the reason's [`code`](RejectReason::code).
fn reject(world: &mut World, cmd: PlayerCommand, reason: RejectReason, tick: Tick) {
    debug_assert!(
        reason.code().contains('.'),
        "reject code '{}' is not namespaced ('<feature>.<reason>')",
        reason.code()
    );
    tracing::debug!(
        tick = cmd.exec_tick.0,
        player = cmd.player.0,
        seq = cmd.seq,
        type_id = cmd.kind.type_id.0,
        reason = reason.code(),
        "command rejected"
    );
    world
        .resource_mut::<SimCounters>()
        .add("commands.rejected", 1);
    world
        .resource_mut::<SimEvents<SimEvent>>()
        .push(SimEvent::CommandRejected {
            player: cmd.player,
            seq: cmd.seq,
            reason: reason.code().to_owned(),
        });
    if world.resource::<Journal>().enabled() {
        let record = CommandRejected {
            command: command_name(world, &cmd),
            seq: cmd.seq,
            reason: reason.code().to_owned(),
        };
        world
            .resource_mut::<Journal>()
            .record(tick, Phase::Input, None, Some(cmd.player), &record);
    }
    world
        .resource_mut::<CommandLog>()
        .rejected
        .push((cmd, reason));
}

/// Apply this tick's commands in `(player, seq)` order. Exclusive, in
/// [`InputSet::Apply`](crate::InputSet::Apply); it is the hook's only core system, and
/// feature systems in the hook run after it.
///
/// Each command is checked, in this order:
/// 1. `exec_tick` before the tick: [`RejectReason::Late`]; after it: [`RejectReason::Early`];
/// 2. `seq` not above the player's [`LastSeq`]: [`RejectReason::DuplicateSeq`]; otherwise the
///    command takes its seq;
/// 3. the type ([`RejectReason::UnknownType`]), the payload ([`RejectReason::BadPayload`]),
///    the player ([`RejectReason::UnknownPlayer`]);
/// 4. a `DEBUG_ONLY` type without `cheats_allowed`: [`RejectReason::DebugDisabled`];
/// 5. the units: more than [`MAX_COMMAND_UNITS`] is [`RejectReason::TooManyUnits`]; then
///    deduplicated and sorted, kept if they exist, belong to the player (not
///    checked for `DEBUG_ONLY` types, which may target anyone's units) and pass
///    [`CommandHandler::unit_can`]; none left for a `NEEDS_UNITS` type:
///    [`RejectReason::NoValidUnits`];
/// 6. the handler, which may reject (for example [`RejectReason::UnknownContent`], or a
///    feature's [`RejectReason::Feature`]).
///
/// An applied command counts `"commands.applied"` and is journaled
/// `"core.command_applied"`.
pub(crate) fn apply_commands(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let mut inbox = std::mem::take(&mut world.resource_mut::<CommandInbox>().0);
    inbox.sort_by_key(|cmd| (cmd.player, cmd.seq));
    for cmd in inbox {
        match check_and_run(world, &cmd, tick) {
            Ok(units) => {
                world
                    .resource_mut::<SimCounters>()
                    .add("commands.applied", 1);
                if world.resource::<Journal>().enabled() {
                    let record = CommandApplied {
                        command: command_name(world, &cmd),
                        seq: cmd.seq,
                        units,
                        queue: cmd.queue,
                    };
                    world.resource_mut::<Journal>().record(
                        tick,
                        Phase::Input,
                        None,
                        Some(cmd.player),
                        &record,
                    );
                }
                world.resource_mut::<CommandLog>().applied.push(cmd);
            }
            Err(reason) => reject(world, cmd, reason, tick),
        }
    }
}

/// Checks 1 and 2 of [`apply_commands`], then the type's runner. Returns the units the
/// command was applied to.
fn check_and_run(
    world: &mut World,
    cmd: &PlayerCommand,
    tick: Tick,
) -> Result<Vec<SimId>, RejectReason> {
    if cmd.exec_tick < tick {
        return Err(RejectReason::Late);
    }
    if cmd.exec_tick > tick {
        return Err(RejectReason::Early);
    }
    {
        let mut last = world.resource_mut::<LastSeq>();
        if last.0.get(&cmd.player).is_some_and(|&seq| cmd.seq <= seq) {
            return Err(RejectReason::DuplicateSeq);
        }
        last.0.insert(cmd.player, cmd.seq);
    }
    let run = world
        .resource::<CommandRegistry>()
        .runner(cmd.kind.type_id)
        .ok_or(RejectReason::UnknownType)?;
    run(world, cmd)
}

/// Decode `bytes` as `P`; trailing bytes are an error.
fn decode<P: CommandPayload>(bytes: &[u8]) -> Result<P, RejectReason> {
    match postcard::take_from_bytes::<P>(bytes) {
        Ok((payload, [])) => Ok(payload),
        _ => Err(RejectReason::BadPayload),
    }
}

/// Checks 3 (payload, player) to 6 of [`apply_commands`] for payload type `P`. Returns the
/// validated units.
fn run_command<P: CommandPayload>(
    handler: &impl CommandHandler<P>,
    world: &mut World,
    cmd: &PlayerCommand,
) -> Result<Vec<SimId>, RejectReason> {
    let payload = decode::<P>(&cmd.kind.payload)?;
    if world.resource::<Players>().get(cmd.player).is_none() {
        return Err(RejectReason::UnknownPlayer);
    }
    let cheats_allowed = world
        .get_resource::<MatchHeaderRes>()
        .is_some_and(|header| header.0.cheats_allowed);
    if P::DEBUG_ONLY && !cheats_allowed {
        return Err(RejectReason::DebugDisabled);
    }

    if cmd.units.len() > MAX_COMMAND_UNITS {
        return Err(RejectReason::TooManyUnits);
    }
    let mut ids = cmd.units.clone();
    ids.sort_unstable();
    ids.dedup();
    let index = world.resource::<SimIndex>();
    let units: Vec<(SimId, Entity)> = ids
        .into_iter()
        .filter_map(|id| {
            let entity = index.get(id)?;
            let unit = world.entity(entity);
            let owned = P::DEBUG_ONLY || unit.get::<Owner>().is_some_and(|o| o.0 == cmd.player);
            (owned && handler.unit_can(&payload, unit)).then_some((id, entity))
        })
        .collect();
    if P::NEEDS_UNITS && units.is_empty() {
        return Err(RejectReason::NoValidUnits);
    }

    let ctx = CommandCtx {
        tick: cmd.exec_tick,
        player: cmd.player,
        seq: cmd.seq,
        units: &units,
        queue: cmd.queue,
    };
    handler.apply(&payload, world, &ctx)?;
    Ok(units.into_iter().map(|(id, _)| id).collect())
}

/// Journal `"core.order_changed"` for unit `id` of `ctx`'s command.
fn journal_order(world: &mut World, ctx: &CommandCtx<'_>, id: SimId, reason: Reason) {
    journal_order_changed(world, ctx.tick, Phase::Input, id, Some(ctx.player), reason);
}

/// Whether one of `units` already holds [`MAX_WAYPOINTS`] orders, so a queued order onto
/// them is [`RejectReason::QueueFull`].
fn any_full(world: &World, units: &[(SimId, Entity)]) -> bool {
    units.iter().any(|&(_, entity)| {
        world
            .get::<MoveOrder>(entity)
            .is_some_and(|order| order.orders.len() >= MAX_WAYPOINTS)
    })
}

/// Give `entity` `order`: appended to its orders with `queue`, else replacing them. Returns
/// whether `order` is now the current order (the only one).
fn give_order(world: &mut World, entity: Entity, order: Order, queue: bool) -> bool {
    let mut unit = world.entity_mut(entity);
    match unit.get_mut::<MoveOrder>() {
        Some(mut queued) if queue => {
            queued.orders.push(order);
            queued.orders.len() == 1
        }
        Some(mut queued) => {
            queued.orders.clear();
            queued.orders.push(order);
            true
        }
        None => {
            unit.insert(MoveOrder {
                orders: vec![order],
            });
            true
        }
    }
}

/// The position of `entity`, if it has a [`Pose`].
fn position(world: &World, entity: Entity) -> Option<FxVec2> {
    world.get::<Pose>(entity).map(|pose| pose.pos)
}

/// `"core.move"`: drive to `target` (clamped to the map). One unit drives to `target`; a
/// group keeps its formation: each unit's own target is its slot of [`formation_targets`]
/// (spread `Movement.formation_max_spread_m`), from the positions the units hold when the
/// command applies, queued or not, then spaced at least [`slot_pitch`] apart by
/// [`space_slots`]. Without `queue` the units' orders become this one move and every unit asks
/// for a path (an older request turns stale): per movement profile, a set of at least
/// `Path.flow_min_group` units shares one group request (a flow field to the cluster of
/// `target`, [`enqueue_group_in`]), and the rest ask for single requests. Group requests are
/// enqueued first, in [`MoveProfile`] order, then the single requests, in [`SimId`] order
/// (ADR-0015). With `queue` the move is appended to the [`MoveOrder`], and a unit that had no
/// order asks for a single request. Needs [`Mobility`] and a map
/// ([`RejectReason::NoValidUnits`] without one: no target is bounded).
struct MoveHandler;

impl CommandHandler<MoveCmd> for MoveHandler {
    fn unit_can(&self, _payload: &MoveCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Mobility>()
    }

    fn apply(
        &self,
        payload: &MoveCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let map = world
            .get_resource::<MapInfo>()
            .ok_or(RejectReason::NoValidUnits)?;
        let target = map.clamp(payload.target);
        if ctx.queue && any_full(world, ctx.units) {
            return Err(RejectReason::QueueFull);
        }
        let targets = if ctx.units.len() > 1 {
            let positions: Vec<FxVec2> = ctx
                .units
                .iter()
                .map(|&(_, entity)| position(world, entity).unwrap_or(target))
                .collect();
            let spread = world
                .resource::<Content>()
                .0
                .rules::<MovementRules>()
                .formation_max_spread_m;
            let slots = formation_targets(&positions, target, spread, map);
            space_slots(&slots, slot_pitch(world, ctx.units), map)
        } else {
            vec![target]
        };
        let mut current = Vec::new();
        for (&(id, entity), &slot) in ctx.units.iter().zip(&targets) {
            if give_order(world, entity, Order::Move { target: slot }, ctx.queue) {
                current.push((id, entity, slot));
            }
            journal_order(world, ctx, id, Reason::Command);
        }
        if ctx.queue {
            for (id, _, slot) in current {
                enqueue_path_in(world, id, slot, Phase::Input);
            }
            return Ok(());
        }
        let min_group = usize::try_from(
            world
                .resource::<Content>()
                .0
                .rules::<PathRules>()
                .flow_min_group,
        )
        .unwrap_or(usize::MAX);
        let mut by_profile: BTreeMap<MoveProfile, Vec<(SimId, FxVec2)>> = BTreeMap::new();
        let mut singles: Vec<(SimId, FxVec2)> = Vec::new();
        for (id, entity, slot) in current {
            match unit_profile(world, entity) {
                Some(profile) => by_profile.entry(profile).or_default().push((id, slot)),
                None => singles.push((id, slot)),
            }
        }
        let mut groups = Vec::new();
        for members in by_profile.into_values() {
            if members.len() >= min_group {
                groups.push(members);
            } else {
                singles.extend(members);
            }
        }
        singles.sort_unstable_by_key(|&(id, _)| id);
        for members in groups {
            enqueue_group_in(world, members, target, Phase::Input);
        }
        for (id, slot) in singles {
            enqueue_path_in(world, id, slot, Phase::Input);
        }
        Ok(())
    }
}

/// The least distance between two formation slots of `units`: the largest footprint side
/// (width or length) among them plus `Steering.separation_margin_m`, so neighbouring slots
/// never ask two hulls to overlap (Task 9 review Minor 2).
fn slot_pitch(world: &World, units: &[(SimId, Entity)]) -> Fx {
    let side = units
        .iter()
        .filter_map(|&(_, entity)| world.get::<Footprint>(entity))
        .map(|footprint| footprint.width.max(footprint.length))
        .max()
        .unwrap_or(Fx::ZERO);
    let margin = world
        .resource::<Content>()
        .0
        .rules::<SteeringRules>()
        .separation_margin_m;
    side + margin
}

/// `"core.rotate"`: turn in place to face `toward`, clamped to the map first (R104: a wire
/// value never reaches arithmetic unclamped). Each unit's facing is the direction from where
/// it will start the turn to `toward` (its heading if that is `toward` itself): its position
/// now, or with `queue` the target of the last move already in its orders, if any, so a
/// rotate queued after a move faces the point from where the move ends (R105; C7 resolves the
/// facing when the order becomes current instead). A unit without a [`Pose`] still gets the
/// order, like a move or a follow, and it completes at once. Without `queue` the orders become
/// this rotate and the path goes back to `Idle` (a queued request turns stale); with `queue`
/// it is appended. Needs [`Mobility`] and a map ([`RejectReason::NoValidUnits`] without one,
/// as for a move).
struct RotateHandler;

impl CommandHandler<RotateCmd> for RotateHandler {
    fn unit_can(&self, _payload: &RotateCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Mobility>()
    }

    fn apply(
        &self,
        payload: &RotateCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        // R104: the wire point is clamped before any arithmetic.
        let toward = world
            .get_resource::<MapInfo>()
            .ok_or(RejectReason::NoValidUnits)?
            .clamp(payload.toward);
        if ctx.queue && any_full(world, ctx.units) {
            return Err(RejectReason::QueueFull);
        }
        for &(id, entity) in ctx.units {
            let pose = world.get::<Pose>(entity).copied();
            let queued_end = world
                .get::<MoveOrder>(entity)
                .filter(|_| ctx.queue)
                .and_then(|order| {
                    order.orders.iter().rev().find_map(|order| match order {
                        Order::Move { target } => Some(*target),
                        _ => None,
                    })
                });
            let from = queued_end.or(pose.map(|pose| pose.pos)).unwrap_or(toward);
            let facing = if toward == from {
                pose.map_or(Angle::ZERO, |pose| pose.heading)
            } else {
                (toward - from).angle()
            };
            if give_order(world, entity, Order::Rotate { facing }, ctx.queue) {
                idle(world, entity);
            }
            journal_order(world, ctx, id, Reason::Command);
        }
        Ok(())
    }
}

/// `"core.follow"`: follow unit `leader`, keeping the unit's offset from it: `pos − leader`
/// shortened to at most `Movement.follow_distance_m`, its direction kept; a unit exactly on
/// its leader keeps `follow_distance_m` behind the leader's heading instead. The leader is never
/// one of its own followers ([`unit_can`](CommandHandler::unit_can) drops it from the units,
/// so a unit told to follow itself alone is [`RejectReason::NoValidUnits`]); a leader that
/// does not exist or has no [`Pose`] is `"core.invalid_target"`. Without `queue` the orders
/// become this follow and the path goes back to `Idle`, so
/// [`advance_orders`](crate::orders) plans one in this tick; with `queue` it is appended.
/// Needs [`Mobility`].
struct FollowHandler;

impl CommandHandler<FollowCmd> for FollowHandler {
    fn unit_can(&self, payload: &FollowCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Mobility>()
            && unit
                .get::<SimEntity>()
                .is_some_and(|sim| sim.id() != payload.leader)
    }

    fn apply(
        &self,
        payload: &FollowCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let leader = world
            .resource::<SimIndex>()
            .get(payload.leader)
            .and_then(|leader| world.get::<Pose>(leader).copied())
            .ok_or(INVALID_TARGET)?;
        if ctx.queue && any_full(world, ctx.units) {
            return Err(RejectReason::QueueFull);
        }
        let distance = world
            .resource::<Content>()
            .0
            .rules::<MovementRules>()
            .follow_distance_m;
        for &(id, entity) in ctx.units {
            let offset = position(world, entity).map_or(FxVec2::ZERO, |pos| pos - leader.pos);
            let length = offset.length();
            let offset = if length == Fx::ZERO {
                // On the leader's spot: keep `distance` behind it, never its centre.
                -(FxVec2::from_angle(leader.heading) * distance)
            } else if length > distance {
                offset * distance / length
            } else {
                offset
            };
            let order = Order::Follow {
                leader: payload.leader,
                offset,
            };
            if give_order(world, entity, order, ctx.queue) {
                idle(world, entity);
            }
            journal_order(world, ctx, id, Reason::Command);
        }
        Ok(())
    }
}

/// `"core.stop"`: never queued (`queue` is ignored). Drop every order; the path goes back to
/// `Idle` with no corners, so a queued request turns stale.
struct StopHandler;

impl CommandHandler<StopCmd> for StopHandler {
    fn apply(
        &self,
        _payload: &StopCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        for &(id, entity) in ctx.units {
            if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
                follow.state = PathState::Idle;
                follow.corners.clear();
                follow.next = 0;
            }
            if let Some(mut order) = world.get_mut::<MoveOrder>(entity) {
                order.orders.clear();
                journal_order(world, ctx, id, Reason::Stopped);
            }
        }
        Ok(())
    }
}

/// The command resources, the core handlers, the journal records and the apply system.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app
        .init_resource::<CommandInbox>()
        .init_resource::<CommandLog>()
        .init_resource::<LastSeq>();
    sim.register_resource::<CommandRegistry>("core.command_registry", Class::External)
        .register_resource::<CommandInbox>(
            "core.command_inbox",
            Class::TransientKept("filled by step_tick before the tick, emptied by apply_commands"),
        )
        .register_resource::<LastSeq>("core.last_seq", Class::hashed(1))
        .register_resource::<CommandLog>(
            "core.command_log",
            Class::TransientReset(CommandLog::clear),
        )
        .register_command::<MoveCmd>(MoveHandler)
        .register_command::<RotateCmd>(RotateHandler)
        .register_command::<FollowCmd>(FollowHandler)
        .register_command::<StopCmd>(StopHandler)
        .register_journal_record::<CommandApplied>()
        .register_journal_record::<CommandRejected>()
        .add_systems(InputSet::Apply, apply_commands);
}

#[cfg(test)]
mod tests {
    use fl_math::Fx;
    use fl_protocol::DebugWanderCmd;

    use super::*;

    #[test]
    fn decode_rejects_garbage_and_trailing_bytes() {
        let bytes = postcard::to_allocvec(&DebugWanderCmd {
            radius: Fx::from_int(3),
        })
        .expect("encodes");
        assert!(decode::<DebugWanderCmd>(&bytes).is_ok());
        let mut long = bytes.clone();
        long.push(0);
        assert_eq!(
            decode::<DebugWanderCmd>(&long),
            Err(RejectReason::BadPayload)
        );
        assert_eq!(decode::<MoveCmd>(&[0xFF; 3]), Err(RejectReason::BadPayload));
        assert_eq!(decode::<StopCmd>(&[]), Ok(StopCmd));
    }
}
