//! The input layer (spec §3.3): [`SimCommand`] is the only way external input
//! (player, CLI, remote interface, replays) changes the sim.
//!
//! [`submit`] validates a command and queues it for a tick;
//! [`apply_commands_system`] applies due commands at the start of that tick
//! (`NpcSet::Inputs`) in `(tick, seq)` order, journals each one and appends
//! it to the [`CommandLog`], which is what a [`Replay`](crate::replay::Replay)
//! records. A command is validated again when it applies; if its target is
//! gone by then (e.g. despawned by an earlier command) it is skipped with a
//! warning, recorded in [`SkippedCommands`] and not logged, so a replay of
//! the log reproduces the run. [`submit_deferred`] skips the submission-time
//! state checks (scripted input, e.g. a command that targets a unit spawned
//! earlier in the same tick).
//!
//! **Inside a tick.** Once `NpcSet::Inputs` of tick `t` applied its commands
//! (`CommandQueue::applied_through`), tick `t` takes no more: [`submit`]
//! refuses to queue a command inside `NpcSimTick` for "the next tick"
//! (`Unavailable`; sim code uses [`submit_deferred`], which queues it for
//! `t + 1`), and an explicit `at_tick <= t` is `TickInPast`. So every
//! command applies at its own tick and the [`CommandLog`] keeps its own
//! `(tick, seq)` key: no two log entries share a key, and a replay that
//! re-queues the log applies everything in the same order.
//!
//! Core commands are `Despawn`, `SetNeed` (handled by the needs crate through
//! [`SetNeedHandler`]) and `Order`. Everything else is a **content command**:
//! a type implementing [`ContentCommand`], registered by its stable `NAME`
//! (`KernelAppExt::register_command`). On the wire every command is one JSON
//! object tagged by `"type"`, so `{"type":"SetFood","stockpile":3,"food":0}`
//! reads the same whether `SetFood` is core or content.

use std::collections::BTreeMap;
use std::fmt;

use bevy_ecs::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::clock::SimClock;
use crate::ids::{SimId, SimIndex};
use crate::journal::{self, JournalKind};
use crate::json::JsonObject;
use crate::math::SimPos;
use crate::orders::{CurrentOrder, Order, apply_order};

/// Positions in commands must lie within ±this many mm on each axis (1 km).
pub const POS_LIMIT: i32 = 1_000_000;

/// External input. JSON: tagged by `"type"`, unit ids are plain `SimId`
/// numbers, positions `[x, z]` in mm, e.g.
/// `{"type":"Order","unit":5,"order":{"type":"GoTo","pos":[1000,2000]}}`.
/// Any other `"type"` is a content command (`Content`), dispatched by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SimCommand {
    /// Removes any sim entity (its reservations are released).
    Despawn { unit: u32 },
    /// Sets a need's value; `need` is the need's registered name (`"Hunger"`).
    SetNeed { unit: u32, need: String, value: i32 },
    /// Gives a unit that takes orders (has [`CurrentOrder`]) an order.
    Order { unit: u32, order: Order },
    /// A registered [`ContentCommand`]: `kind` is its `NAME`, `payload` its
    /// fields (without `"type"`, field order kept).
    Content { kind: String, payload: JsonObject },
}

/// The wire form of the core variants.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum CoreRepr {
    Despawn { unit: u32 },
    SetNeed { unit: u32, need: String, value: i32 },
    Order { unit: u32, order: Order },
}

const CORE_KINDS: [&str; 3] = ["Despawn", "SetNeed", "Order"];

impl Serialize for SimCommand {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let core = match self {
            SimCommand::Despawn { unit } => CoreRepr::Despawn { unit: *unit },
            SimCommand::SetNeed { unit, need, value } => CoreRepr::SetNeed {
                unit: *unit,
                need: need.clone(),
                value: *value,
            },
            SimCommand::Order { unit, order } => CoreRepr::Order {
                unit: *unit,
                order: *order,
            },
            SimCommand::Content { kind, payload } => {
                let mut map = s.serialize_map(Some(payload.0.len() + 1))?;
                map.serialize_entry("type", kind)?;
                for (k, v) in &payload.0 {
                    map.serialize_entry(k, v)?;
                }
                return map.end();
            }
        };
        core.serialize(s)
    }
}

impl<'de> Deserialize<'de> for SimCommand {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut obj = JsonObject::deserialize(d)?;
        let kind = match obj.get("type") {
            Some(serde_json::Value::String(k)) => k.clone(),
            Some(_) => return Err(D::Error::custom("command `type` must be a string")),
            None => return Err(D::Error::missing_field("type")),
        };
        if CORE_KINDS.contains(&kind.as_str()) {
            let core: CoreRepr = obj.to_typed().map_err(D::Error::custom)?;
            return Ok(match core {
                CoreRepr::Despawn { unit } => SimCommand::Despawn { unit },
                CoreRepr::SetNeed { unit, need, value } => {
                    SimCommand::SetNeed { unit, need, value }
                }
                CoreRepr::Order { unit, order } => SimCommand::Order { unit, order },
            });
        }
        obj.remove("type");
        Ok(SimCommand::Content { kind, payload: obj })
    }
}

impl SimCommand {
    /// A content command from its typed value.
    ///
    /// # Panics
    /// When `C` does not serialize to a JSON object (a `ContentCommand`
    /// must be a struct with named fields).
    pub fn content<C: ContentCommand>(cmd: &C) -> Self {
        let payload = JsonObject::from_typed(cmd).unwrap_or_else(|e| {
            panic!(
                "content command {} must serialize to a JSON object: {e}",
                C::NAME
            )
        });
        SimCommand::Content {
            kind: C::NAME.to_owned(),
            payload,
        }
    }

    /// The command's `"type"` name.
    pub fn kind(&self) -> &str {
        match self {
            SimCommand::Despawn { .. } => "Despawn",
            SimCommand::SetNeed { .. } => "SetNeed",
            SimCommand::Order { .. } => "Order",
            SimCommand::Content { kind, .. } => kind,
        }
    }

    /// The existing unit a core command targets (content commands: see
    /// [`SimCommand::unit_in`]).
    pub fn unit(&self) -> Option<SimId> {
        match self {
            SimCommand::Despawn { unit }
            | SimCommand::SetNeed { unit, .. }
            | SimCommand::Order { unit, .. } => Some(SimId(*unit)),
            SimCommand::Content { .. } => None,
        }
    }

    /// The existing unit the command targets, asking the registered content
    /// command for `Content` (`None` for e.g. a spawn).
    pub fn unit_in(&self, world: &World) -> Option<SimId> {
        match self {
            SimCommand::Content { kind, payload } => world
                .get_resource::<CommandRegistry>()
                .and_then(|r| r.get(kind))
                .and_then(|fns| (fns.unit)(payload)),
            core => core.unit(),
        }
    }
}

/// A game-specific command. Implement it for a struct with named fields,
/// register it with `register_command::<C>()`; it is then accepted as
/// `{"type": NAME, ..fields}` by `submit`, `npcctl --commands` and BRP
/// `npc/command`, journaled and recorded in replays like a core command.
pub trait ContentCommand: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable name: the JSON `"type"` tag and the registry key.
    const NAME: &'static str;
    /// Whether the command can apply to `world` now. Must never panic.
    fn validate(&self, world: &World) -> Result<(), CommandError>;
    /// Applies a validated command.
    fn apply(self, world: &mut World);
    /// The existing unit the command targets (the journal entry's unit).
    fn unit(&self) -> Option<SimId> {
        None
    }
}

/// Type-erased functions of one registered content command.
#[derive(Clone, Copy)]
pub struct ContentCommandFns {
    pub name: &'static str,
    /// Parses the payload and re-serializes it in the type's own field order
    /// (fills defaults), so journals and replays always see one form.
    pub canonical: fn(&JsonObject) -> Result<JsonObject, CommandError>,
    pub validate: fn(&World, &JsonObject) -> Result<(), CommandError>,
    pub apply: fn(&mut World, &JsonObject),
    pub unit: fn(&JsonObject) -> Option<SimId>,
}

fn parse<C: ContentCommand>(payload: &JsonObject) -> Result<C, CommandError> {
    payload
        .to_typed::<C>()
        .map_err(|reason| CommandError::InvalidValue {
            field: C::NAME.to_owned(),
            reason,
        })
}

impl ContentCommandFns {
    pub fn of<C: ContentCommand>() -> Self {
        Self {
            name: C::NAME,
            canonical: |p| {
                let cmd = parse::<C>(p)?;
                JsonObject::from_typed(&cmd).map_err(|reason| CommandError::InvalidValue {
                    field: C::NAME.to_owned(),
                    reason,
                })
            },
            validate: |w, p| parse::<C>(p)?.validate(w),
            apply: |w, p| {
                if let Ok(cmd) = parse::<C>(p) {
                    cmd.apply(w);
                }
            },
            unit: |p| parse::<C>(p).ok().and_then(|c| c.unit()),
        }
    }
}

/// Registered content commands by name.
#[derive(Resource, Default, Clone)]
pub struct CommandRegistry {
    pub commands: BTreeMap<&'static str, ContentCommandFns>,
}

impl CommandRegistry {
    pub fn get(&self, kind: &str) -> Option<&ContentCommandFns> {
        self.commands.get(kind)
    }

    /// Registers `C`. Panics on a duplicate name or a name that is a core
    /// command.
    pub fn add<C: ContentCommand>(&mut self) {
        assert!(
            !CORE_KINDS.contains(&C::NAME),
            "content command `{}` collides with a core command",
            C::NAME
        );
        let prev = self.commands.insert(C::NAME, ContentCommandFns::of::<C>());
        assert!(
            prev.is_none(),
            "content command `{}` is registered twice",
            C::NAME
        );
    }
}

/// How `SetNeed` is checked and applied; installed by the needs crate.
#[derive(Resource, Clone, Copy)]
pub struct SetNeedHandler {
    pub validate: fn(&World, Entity, &str, i32) -> Result<(), CommandError>,
    pub apply: fn(&mut World, Entity, &str, i32),
}

/// Why a command was rejected. JSON: tagged by `"error"`, e.g.
/// `{"error":"UnknownUnit","unit":999}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error")]
pub enum CommandError {
    /// No sim entity has this id (never existed or despawned).
    UnknownUnit { unit: u32 },
    /// The entity exists but is not a `expected` (`"villager"`, `"stockpile"`, ..).
    WrongKind { unit: u32, expected: String },
    /// `tick` already ran (`now` is the next tick to run).
    TickInPast { tick: u64, now: u64 },
    /// A value is out of range.
    InvalidValue { field: String, reason: String },
    /// No content command is registered under `kind`.
    UnknownCommand { kind: String, known: Vec<String> },
    /// The command cannot apply in this world (e.g. no home to spawn into).
    Unavailable { reason: String },
    /// The world lacks sim resources (plugins not installed).
    NotReady { missing: String },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownUnit { unit } => write!(f, "unknown unit {unit}"),
            CommandError::WrongKind { unit, expected } => {
                write!(f, "unit {unit} is not a {expected}")
            }
            CommandError::TickInPast { tick, now } => {
                write!(f, "tick {tick} already ran (next tick is {now})")
            }
            CommandError::InvalidValue { field, reason } => write!(f, "invalid {field}: {reason}"),
            CommandError::UnknownCommand { kind, known } => {
                write!(f, "unknown command `{kind}` (known: {})", known.join(", "))
            }
            CommandError::Unavailable { reason } => write!(f, "{reason}"),
            CommandError::NotReady { missing } => write!(f, "sim not ready: missing {missing}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Commands waiting for their tick, keyed and applied by `(tick, seq)`.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandQueue {
    pub pending: BTreeMap<(u64, u32), SimCommand>,
    /// The last tick whose commands [`apply_commands_system`] applied (set
    /// at the start of every `NpcSet::Inputs`): that tick takes no more
    /// commands. Not hashed (it follows `SimClock`).
    pub applied_through: Option<u64>,
}

impl CommandQueue {
    /// The first tick a new command can still apply at, given `now` (the
    /// current `SimClock.tick`): `now`, or `now + 1` inside `NpcSimTick`
    /// once the commands of `now` were applied.
    pub fn next_open_tick(&self, now: u64) -> u64 {
        match self.applied_through {
            Some(t) if t >= now => t + 1,
            _ => now,
        }
    }

    /// Queues `cmd` at `tick` without validation; returns its seq (one past
    /// the highest seq already pending at that tick).
    pub fn push(&mut self, tick: u64, cmd: SimCommand) -> u32 {
        let seq = self
            .pending
            .range((tick, 0)..=(tick, u32::MAX))
            .next_back()
            .map_or(0, |(&(_, s), _)| s + 1);
        self.pending.insert((tick, seq), cmd);
        seq
    }
}

/// Every applied command as `(tick, seq, cmd)` (its own queue key, which is
/// the tick it applied at), in application order.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandLog {
    pub entries: Vec<(u64, u32, SimCommand)>,
}

/// The first tick a command submitted now can apply at, and whether that is
/// because the call happens inside `NpcSimTick` (after `NpcSet::Inputs`).
fn open_tick(world: &World) -> Result<(u64, bool), CommandError> {
    let now = world
        .get_resource::<SimClock>()
        .ok_or_else(|| not_ready("SimClock"))?
        .tick;
    let open = world
        .get_resource::<CommandQueue>()
        .map_or(now, |q| q.next_open_tick(now));
    Ok((open, open > now))
}

/// Validates `cmd` and queues it for `at_tick` (default: the next tick to
/// run, i.e. the current `SimClock.tick`). Returns its `(tick, seq)`.
/// Several commands on one tick apply in submission order. Content
/// commands are queued in their canonical form.
///
/// Meant for input from outside the sim (between ticks). Inside
/// `NpcSimTick` (after the tick's commands applied) it refuses a command
/// without `at_tick` (`Unavailable`: use [`submit_deferred`], which queues
/// it for the next tick) and one for the running tick (`TickInPast`).
pub fn submit(
    world: &mut World,
    at_tick: Option<u64>,
    cmd: SimCommand,
) -> Result<(u64, u32), CommandError> {
    let (open, inside_tick) = open_tick(world)?;
    if inside_tick && at_tick.is_none() {
        return Err(CommandError::Unavailable {
            reason: format!(
                "submit called inside NpcSimTick after tick {}'s commands applied: use \
                 submit_deferred (queues for tick {open}) or an explicit at_tick",
                open - 1
            ),
        });
    }
    let tick = at_tick.unwrap_or(open);
    if tick < open {
        return Err(CommandError::TickInPast { tick, now: open });
    }
    let cmd = canonical(world, cmd)?;
    validate(world, &cmd)?;
    let seq = world.get_resource_or_init::<CommandQueue>().push(tick, cmd);
    Ok((tick, seq))
}

/// Queues `cmd` for `at_tick` (default: the next tick) with **deferred
/// validation**: only what cannot change before the tick is checked now
/// (the tick is not in the past; a content command's kind is registered and
/// its payload parses, queued in canonical form). Whether its units exist,
/// have the right kind, etc. is checked only when it applies, against the
/// world at that moment (as [`apply_commands_system`] does for every
/// command). So a command may target a unit that an earlier command of the
/// same tick spawns; one that does not validate then is skipped, warned
/// about and recorded in [`SkippedCommands`] instead of being applied.
///
/// Meant for scripted input such as command files (`npcctl --commands`);
/// interactive input (BRP `npc/command`) keeps [`submit`], which rejects a
/// bad command at once. Both give the same run for commands that apply.
/// Inside `NpcSimTick` (after the tick's commands applied) the default
/// tick is the next one, and the running tick is `TickInPast`.
pub fn submit_deferred(
    world: &mut World,
    at_tick: Option<u64>,
    cmd: SimCommand,
) -> Result<(u64, u32), CommandError> {
    let (open, _) = open_tick(world)?;
    let tick = at_tick.unwrap_or(open);
    if tick < open {
        return Err(CommandError::TickInPast { tick, now: open });
    }
    let cmd = canonical(world, cmd)?;
    let seq = world.get_resource_or_init::<CommandQueue>().push(tick, cmd);
    Ok((tick, seq))
}

/// A queued command that did not validate when its tick came.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedCommand {
    pub tick: u64,
    pub seq: u32,
    pub cmd: SimCommand,
    pub error: CommandError,
}

/// Every command [`apply_commands_system`] skipped, in order. Tooling only
/// (not hashed, never read by the sim): lets a caller of [`submit_deferred`]
/// report a command that failed validation at apply time.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SkippedCommands {
    pub entries: Vec<SkippedCommand>,
}

fn not_ready(missing: &str) -> CommandError {
    CommandError::NotReady {
        missing: missing.to_owned(),
    }
}

/// `InvalidValue` for `field`.
pub fn invalid(field: &str, reason: String) -> CommandError {
    CommandError::InvalidValue {
        field: field.to_owned(),
        reason,
    }
}

/// The entity of `unit`, when it is still in the sim.
pub fn entity(world: &World, unit: u32) -> Result<Entity, CommandError> {
    world
        .get_resource::<SimIndex>()
        .ok_or_else(|| not_ready("SimIndex"))?
        .get(SimId(unit))
        .ok_or(CommandError::UnknownUnit { unit })
}

/// `unit`'s entity, which must have component `T` (a `expected`).
pub fn entity_with<T: Component>(
    world: &World,
    unit: u32,
    expected: &str,
) -> Result<Entity, CommandError> {
    let e = entity(world, unit)?;
    if world.get::<T>(e).is_none() {
        return Err(CommandError::WrongKind {
            unit,
            expected: expected.to_owned(),
        });
    }
    Ok(e)
}

/// A position within ±[`POS_LIMIT`] mm, or `InvalidValue` for `field`.
pub fn check_pos(field: &str, (x, z): (i32, i32)) -> Result<SimPos, CommandError> {
    if x.abs() > POS_LIMIT || z.abs() > POS_LIMIT {
        return Err(invalid(
            field,
            format!("({x}, {z}) is outside ±{POS_LIMIT} mm"),
        ));
    }
    Ok(SimPos::new(x, z))
}

fn content_fns<'w>(world: &'w World, kind: &str) -> Result<&'w ContentCommandFns, CommandError> {
    let registry = world
        .get_resource::<CommandRegistry>()
        .ok_or_else(|| not_ready("CommandRegistry (NpcKernelPlugin)"))?;
    registry
        .get(kind)
        .ok_or_else(|| CommandError::UnknownCommand {
            kind: kind.to_owned(),
            known: registry.commands.keys().map(|k| (*k).to_owned()).collect(),
        })
}

/// `cmd` with a content payload in its registered type's canonical form.
pub fn canonical(world: &World, cmd: SimCommand) -> Result<SimCommand, CommandError> {
    match cmd {
        SimCommand::Content { kind, payload } => {
            let payload = (content_fns(world, &kind)?.canonical)(&payload)?;
            Ok(SimCommand::Content { kind, payload })
        }
        core => Ok(core),
    }
}

/// Checks that `cmd` can apply to `world` now (ids exist and have the right
/// kind, values in range). Never panics.
pub fn validate(world: &World, cmd: &SimCommand) -> Result<(), CommandError> {
    match cmd {
        SimCommand::Despawn { unit } => {
            entity(world, *unit)?;
        }
        SimCommand::SetNeed { unit, need, value } => {
            let e = entity(world, *unit)?;
            let handler = world
                .get_resource::<SetNeedHandler>()
                .ok_or_else(|| not_ready("SetNeedHandler (the needs plugin)"))?;
            (handler.validate)(world, e, need, *value)?;
        }
        SimCommand::Order { unit, order } => {
            entity_with::<CurrentOrder>(world, *unit, "unit that takes orders")?;
            if let Order::GoTo { pos } = order {
                check_pos("order.pos", *pos)?;
            }
        }
        SimCommand::Content { kind, payload } => {
            (content_fns(world, kind)?.validate)(world, payload)?;
        }
    }
    Ok(())
}

/// `NpcSet::Inputs` (exclusive, first): applies every pending command due at
/// or before the current tick in `(tick, seq)` order. Each applied command
/// is journaled (`JournalKind::Command`) and logged in [`CommandLog`] under
/// its own `(tick, seq)` key (the current tick: [`submit`] and
/// [`submit_deferred`] never queue for a tick that already ran; only a
/// queue edited by hand can hold an overdue one). The current tick is then
/// closed for new commands (`CommandQueue::applied_through`).
pub fn apply_commands_system(world: &mut World) {
    let Some(tick) = world.get_resource::<SimClock>().map(|c| c.tick) else {
        return;
    };
    world.get_resource_or_init::<CommandQueue>().applied_through = Some(tick);
    let due_now = world
        .get_resource::<CommandQueue>()
        .and_then(|q| q.pending.keys().next().copied())
        .is_some_and(|(t, _)| t <= tick);
    if !due_now {
        return;
    }
    let due = {
        let mut queue = world.resource_mut::<CommandQueue>();
        let later = queue.pending.split_off(&(tick + 1, 0));
        std::mem::replace(&mut queue.pending, later)
    };
    for ((queued_at, seq), cmd) in due {
        let checked = canonical(world, cmd.clone()).and_then(|c| validate(world, &c).map(|()| c));
        let cmd = match checked {
            Ok(cmd) => cmd,
            Err(err) => {
                tracing::warn!("tick {tick}: skipping command {cmd:?}: {err}");
                world
                    .get_resource_or_init::<SkippedCommands>()
                    .entries
                    .push(SkippedCommand {
                        tick,
                        seq,
                        cmd,
                        error: err,
                    });
                continue;
            }
        };
        let unit = cmd.unit_in(world);
        journal::record(world, unit, || JournalKind::Command { cmd: cmd.clone() });
        apply(world, &cmd);
        world
            .get_resource_or_init::<CommandLog>()
            .entries
            .push((queued_at, seq, cmd));
    }
}

/// Applies a validated command.
fn apply(world: &mut World, cmd: &SimCommand) {
    match cmd {
        SimCommand::Despawn { unit } => {
            if let Ok(e) = entity(world, *unit) {
                world.despawn(e);
            }
        }
        SimCommand::SetNeed { unit, need, value } => {
            if let Ok(e) = entity(world, *unit)
                && let Some(handler) = world.get_resource::<SetNeedHandler>().copied()
            {
                (handler.apply)(world, e, need, *value);
            }
        }
        SimCommand::Order { unit, order } => {
            if let Ok(e) = entity(world, *unit) {
                apply_order(world, e, *order);
            }
        }
        SimCommand::Content { kind, payload } => {
            if let Some(fns) = world
                .get_resource::<CommandRegistry>()
                .and_then(|r| r.get(kind))
                .copied()
            {
                (fns.apply)(world, payload);
            }
        }
    }
}

#[cfg(test)]
mod deferred_tests {
    use super::*;
    use bevy_app::App;

    fn world() -> App {
        let mut app = App::new();
        app.add_plugins(crate::NpcKernelPlugin::default());
        crate::ensure_frozen(app.world_mut());
        app
    }

    #[test]
    fn submit_deferred_validates_at_apply_time() {
        let mut app = world();
        let w = app.world_mut();
        let bad = SimCommand::Despawn { unit: 999 };
        assert_eq!(
            submit(w, None, bad.clone()),
            Err(CommandError::UnknownUnit { unit: 999 }),
            "submit checks the world now"
        );
        assert_eq!(submit_deferred(w, None, bad.clone()), Ok((0, 0)));
        // Same-tick: a unit that exists when the command applies is fine.
        let (id, _) = crate::ids::spawn_sim(w, ());
        assert_eq!(
            submit_deferred(w, None, SimCommand::Despawn { unit: id.0 }),
            Ok((0, 1))
        );
        crate::run_ticks(w, 1);
        let skipped = &w.resource::<SkippedCommands>().entries;
        assert_eq!(skipped.len(), 1);
        assert_eq!((skipped[0].tick, skipped[0].seq), (0, 0));
        assert_eq!(skipped[0].error, CommandError::UnknownUnit { unit: 999 });
        let log = &w.resource::<CommandLog>().entries;
        assert_eq!(log.len(), 1, "only the valid command applied: {log:?}");
        assert!(w.resource::<SimIndex>().get(id).is_none(), "despawned");
    }

    #[test]
    fn submit_deferred_still_rejects_what_cannot_change() {
        let mut app = world();
        let w = app.world_mut();
        crate::run_ticks(w, 2);
        assert_eq!(
            submit_deferred(w, Some(1), SimCommand::Despawn { unit: 1 }),
            Err(CommandError::TickInPast { tick: 1, now: 2 })
        );
        let unknown = SimCommand::Content {
            kind: "NoSuchCommand".into(),
            payload: JsonObject::default(),
        };
        assert!(matches!(
            submit_deferred(w, None, unknown),
            Err(CommandError::UnknownCommand { .. })
        ));
    }

    /// What a system inside `NpcSimTick` got back from the submit calls.
    #[derive(Resource, Default)]
    struct MidTick(Vec<Result<(u64, u32), CommandError>>);

    fn submit_mid_tick(world: &mut World) {
        if world.resource::<SimClock>().tick != 0 {
            return;
        }
        let (_, b) = crate::ids::spawn_sim(world, ());
        let b = world.get::<SimId>(b).unwrap().0;
        let got = vec![
            submit(world, None, SimCommand::Despawn { unit: b }),
            submit(world, Some(0), SimCommand::Despawn { unit: b }),
            submit_deferred(world, Some(0), SimCommand::Despawn { unit: b }),
            submit_deferred(world, None, SimCommand::Despawn { unit: b }),
        ];
        world.resource_mut::<MidTick>().0 = got;
    }

    /// Final review cleanup 1: a command queued inside `NpcSimTick` for the
    /// tick being run applied a tick late and was logged under
    /// `(tick applied, original seq)`, which could collide with a command
    /// submitted for that tick (a replay then loses one of them). Now
    /// `submit` refuses inside the tick, `submit_deferred` queues for the
    /// next tick, and the log keeps every command's own `(tick, seq)`.
    #[test]
    fn mid_tick_submits_never_collide_in_the_command_log() {
        let mut app = App::new();
        app.add_plugins(crate::NpcKernelPlugin::default())
            .init_resource::<MidTick>()
            .add_systems(
                crate::NpcSimTick,
                submit_mid_tick.in_set(crate::NpcSet::Actions),
            );
        crate::ensure_frozen(app.world_mut());
        let w = app.world_mut();
        let (a, _) = crate::ids::spawn_sim(w, ());
        assert_eq!(
            submit(w, None, SimCommand::Despawn { unit: a.0 }),
            Ok((0, 0))
        );
        crate::run_ticks(w, 1);
        let got = std::mem::take(&mut w.resource_mut::<MidTick>().0);
        assert!(
            matches!(got[0], Err(CommandError::Unavailable { .. })),
            "submit inside the tick: {:?}",
            got[0]
        );
        assert_eq!(got[1], Err(CommandError::TickInPast { tick: 0, now: 1 }));
        assert_eq!(got[2], Err(CommandError::TickInPast { tick: 0, now: 1 }));
        assert_eq!(got[3], Ok((1, 0)), "deferred to the next tick");
        // Between ticks, another command for tick 1.
        let (c, _) = crate::ids::spawn_sim(w, ());
        assert_eq!(
            submit(w, None, SimCommand::Despawn { unit: c.0 }),
            Ok((1, 1))
        );
        crate::run_ticks(w, 1);
        let keys: Vec<(u64, u32)> = w
            .resource::<CommandLog>()
            .entries
            .iter()
            .map(|(t, s, _)| (*t, *s))
            .collect();
        assert_eq!(keys, vec![(0, 0), (1, 0), (1, 1)], "unique, in order");
    }
}
