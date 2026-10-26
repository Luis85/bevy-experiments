//! The provider registries capability crates plug into (spec §3.1), by
//! stable name, so `npc_agent_io` never knows about them:
//!
//! - [`ObservationFn`]: adds a named section (JSON plus prompt-ready text
//!   lines) to a unit's observation. A *perception* provider
//!   ([`AgentIoAppExt::register_perception`]) also turns the observation's
//!   `omniscient` flag off: the world sections then come from what the unit
//!   perceives.
//! - [`AffordanceFn`]: lists what the unit can do right now ([`Affordance`],
//!   including unavailable ones with `why_not`).
//! - [`ActionHandlerFn`]: turns a request for a verb into an [`ActionPlan`]
//!   (queued [`Step`]s for `Direct` control, or plain `SimCommand`s such as
//!   orders), after validating it against the unit's capabilities.
//! - [`KnownFn`] ([`AgentIoAppExt::register_known`], the perception glue):
//!   the entities a unit perceives or remembers (`None` for a unit without a
//!   perception model, e.g. no senses). For a unit with one, **affordances
//!   may only target known entities**, the unit itself or its possessions: [`crate::observe::affordances`] drops every
//!   other one centrally, whichever provider listed it (the perception
//!   anti-goal, agent-control spec §4). `world/state` stays omniscient.
//! - [`PossessionsFn`] ([`AgentIoAppExt::register_possessions`]):
//!   what the unit holds or rides (interaction and vehicles glue). Always
//!   known; perception listings leave them out of "remembered".
//! - [`DescribeFn`] ([`AgentIoAppExt::register_describer`]): a short name of
//!   an entity (`store_key #26`) for every section's text
//!   ([`crate::observe::describe`]).
//! - [`UnitInfoFn`] ([`AgentIoAppExt::register_unit_info`]): extra fields of
//!   `agent/list_units` rows (e.g. the owning player).
//! - [`BusyFn`] ([`AgentIoAppExt::register_busy`]): what keeps a unit busy
//!   besides its action queue (orders, an interaction, a craft), for
//!   `agent/events {until_idle}`.
//!
//! Providers only read the world (`&World`), in registration-name order.
//! Registering works before or after `AgentIoPlugin`, in any order.

use std::collections::BTreeMap;

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::{SimCommand, SimId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::control::{ControlMode, ExternalController, Step};

/// Who is observing or acting.
#[derive(Clone, Copy, Debug)]
pub struct AgentCtx<'a> {
    pub unit: SimId,
    pub entity: Entity,
    /// The unit's controller, if any.
    pub controller: Option<&'a ExternalController>,
    /// Commands planned by earlier steps of the same atomic multi-step
    /// `agent/act` (not applied yet), so a handler can accept a step that
    /// depends on them (`execute_orders` after an `order`). Empty for
    /// observations and single acts.
    pub planned: &'a [SimCommand],
}

impl AgentCtx<'_> {
    /// The controller's mode (`Observer` when none is attached).
    pub fn mode(&self) -> ControlMode {
        self.controller.map_or(ControlMode::Observer, |c| c.mode)
    }
}

/// One observation section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// The section's JSON (`observation.sections.<name>`).
    pub value: Value,
    /// Lines for the text renderer, in order (may be empty).
    pub lines: Vec<String>,
}

impl Section {
    pub fn new(value: Value) -> Self {
        Self {
            value,
            lines: Vec::new(),
        }
    }

    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }
}

/// Adds a section to `ctx.unit`'s observation (`None`: nothing to add).
pub type ObservationFn = fn(&World, &AgentCtx) -> Option<Section>;
/// Lists affordances for `ctx.unit`.
pub type AffordanceFn = fn(&World, &AgentCtx) -> Vec<Affordance>;
/// Validates a request and plans it.
pub type ActionHandlerFn = fn(&World, &AgentCtx, &ActionRequest) -> Result<ActionPlan, Rejection>;
/// Entities `ctx.unit` perceives or remembers; `None` when the unit has no
/// perception model (e.g. no senses): it is then not perception-gated.
pub type KnownFn = fn(&World, &AgentCtx) -> Option<Vec<SimId>>;
/// What `ctx.unit` holds or rides.
pub type PossessionsFn = fn(&World, &AgentCtx) -> Vec<SimId>;
/// A short description of `id` (`"store_key #26"`); `None`: not this
/// describer's kind of thing.
pub type DescribeFn = fn(&World, SimId) -> Option<String>;
/// Extra `agent/list_units` fields of a unit (a JSON object), if any.
pub type UnitInfoFn = fn(&World, Entity) -> Option<Value>;
/// What keeps the unit busy besides its action queue (`Some("order Move")`).
pub type BusyFn = fn(&World, Entity) -> Option<String>;
/// Whether the unit may use `verb` at all: `Some(reason)` when not (`"this
/// actor cannot pick_up"`). Content decides what a unit may do (e.g. from its
/// actor preset); see [`verb_gate`].
pub type VerbGateFn = fn(&World, Entity, &str) -> Option<String>;

/// Something the unit can do (spec §3.2). JSON:
/// `{"id":"eat@3","verb":"eat","target":3,"label":"Eat at the stockpile #3 (8.2 m)",
///   "args":{},"est_ticks":200,"requires":["food"],"ok":true}`; unavailable ones
/// carry `"ok":false,"why_not":"..."`; skill-checked ones a `check`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affordance {
    /// Stable within one world state: `verb`, `verb@target` or `verb@x,z`.
    pub id: String,
    pub verb: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<SimId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<(i32, i32)>,
    pub label: String,
    /// Argument schema: `{name: "description"}` (empty: none).
    #[serde(default)]
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub est_ticks: Option<u32>,
    /// Preconditions (satisfied when `ok`).
    #[serde(default)]
    pub requires: Vec<String>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why_not: Option<String>,
    /// The skill check the action rolls, when a provider knows it:
    /// `{"skill":"Mining","target_number":11,"dice":"3d6","odds_permille":625}`
    /// (`odds_permille`: the chance of success, 0..=1000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<Value>,
    /// Unavailable for a reason no plan fixes, whatever its words
    /// ([`Affordance::structural`]; a verb gate's refusal): `brief` hides it.
    /// Not serialized.
    #[serde(skip)]
    pub structural: bool,
}

impl Affordance {
    /// An available affordance for `verb` (id `verb`).
    pub fn new(verb: &str, label: impl Into<String>) -> Self {
        Self {
            id: verb.to_owned(),
            verb: verb.to_owned(),
            target: None,
            pos: None,
            label: label.into(),
            args: Value::Object(serde_json::Map::new()),
            est_ticks: None,
            requires: Vec::new(),
            ok: true,
            why_not: None,
            check: None,
            structural: false,
        }
    }

    /// Targets `id` (and the id becomes `verb@id`).
    pub fn target(mut self, id: SimId) -> Self {
        self.target = Some(id);
        self.id = format!("{}@{}", self.verb, id.0);
        self
    }

    /// At position `(x, z)` mm (the id becomes `verb@x,z` unless targeted).
    pub fn pos(mut self, x: i32, z: i32) -> Self {
        self.pos = Some((x, z));
        if self.target.is_none() {
            self.id = format!("{}@{x},{z}", self.verb);
        }
        self
    }

    pub fn arg(mut self, name: &str, description: &str) -> Self {
        if let Value::Object(map) = &mut self.args {
            map.insert(name.to_owned(), Value::String(description.to_owned()));
        }
        self
    }

    pub fn est_ticks(mut self, ticks: u32) -> Self {
        self.est_ticks = Some(ticks);
        self
    }

    pub fn requires(mut self, what: &str) -> Self {
        self.requires.push(what.to_owned());
        self
    }

    /// Marks it unavailable, with the reason.
    pub fn why_not(mut self, reason: impl Into<String>) -> Self {
        self.ok = false;
        self.why_not = Some(reason.into());
        self
    }

    /// Marks it unavailable for a structural reason no plan fixes (whatever
    /// the reason's words): [`Affordance::structurally_impossible`].
    pub fn structural(mut self, reason: impl Into<String>) -> Self {
        self.structural = true;
        self.why_not(reason)
    }

    /// Whether it is unavailable for a *structural* reason no plan fixes
    /// ("door has no lock", "door is already closed", "cannot mount
    /// itself"), as opposed to one an agent can work on ("too far: ..",
    /// "door is locked: needs key:store", "needs tool:pickaxe").
    /// `agent/observe {brief}` hides these, and those marked
    /// [`Affordance::structural`] (a verb gate's refusal).
    pub fn structurally_impossible(&self) -> bool {
        const MARKS: &[&str] = &[
            "has no lock",
            "already",
            "is not locked",
            "has nothing to give",
            "cannot mount itself",
            "a vehicle cannot mount",
            "does not offer",
            "cannot interact",
            "is not an item",
            "has no inventory",
            "has no character sheet",
        ];
        !self.ok
            && (self.structural
                || self
                    .why_not
                    .as_deref()
                    .is_some_and(|w| MARKS.iter().any(|m| w.contains(m))))
    }
}

/// A request to act: one of the offered affordances, or an explicit verb.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub verb: String,
    #[serde(default)]
    pub target: Option<SimId>,
    #[serde(default)]
    pub pos: Option<(i32, i32)>,
    /// Verb-specific arguments (`{"ticks": 100}`).
    #[serde(default)]
    pub args: Value,
}

impl ActionRequest {
    /// Integer argument `name` (`None` when missing *or wrongly typed*;
    /// [`ActionRequest::opt_int`] rejects a wrongly typed one).
    pub fn int_arg(&self, name: &str) -> Option<i64> {
        self.args.get(name).and_then(Value::as_i64)
    }

    /// String argument `name` (`None` when missing or wrongly typed; see
    /// [`ActionRequest::opt_str`]).
    pub fn str_arg(&self, name: &str) -> Option<&str> {
        self.args.get(name).and_then(Value::as_str)
    }

    /// Integer argument `name`: `Ok(None)` when missing or `null`,
    /// `invalid_args` when present with another type (`"ticks":"50"`).
    pub fn opt_int(&self, name: &str) -> Result<Option<i64>, Rejection> {
        match self.args.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_i64().map(Some).ok_or_else(|| {
                Rejection::new(
                    "invalid_args",
                    format!("{name} must be an integer, got {v}"),
                )
            }),
        }
    }

    /// String argument `name`: `Ok(None)` when missing or `null`,
    /// `invalid_args` when present with another type.
    pub fn opt_str(&self, name: &str) -> Result<Option<&str>, Rejection> {
        match self.args.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_str().map(Some).ok_or_else(|| {
                Rejection::new("invalid_args", format!("{name} must be a string, got {v}"))
            }),
        }
    }
}

/// What an action becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionPlan {
    /// Queue these steps (`Direct` control only).
    Queue {
        label: String,
        steps: Vec<Step>,
        est_ticks: Option<u32>,
    },
    /// Submit these commands now (e.g. an `Order` for `Advisor` control).
    /// Every command is validated before any is submitted (all or nothing).
    Commands {
        label: String,
        commands: Vec<SimCommand>,
    },
    /// Several plans in order, as one atomic act (the `interact` verb: an
    /// `Interact` order, then `execute_orders`). The first `Queue` part
    /// replaces the unit's action queue unless the act says `queue`; later
    /// parts append.
    Sequence {
        label: String,
        plans: Vec<ActionPlan>,
    },
}

impl ActionPlan {
    /// The plan's label.
    pub fn label(&self) -> &str {
        match self {
            ActionPlan::Queue { label, .. }
            | ActionPlan::Commands { label, .. }
            | ActionPlan::Sequence { label, .. } => label,
        }
    }
}

/// Why an action was refused: a short `reason` code and a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    pub reason: String,
    pub message: String,
}

impl Rejection {
    pub fn new(reason: &str, message: impl Into<String>) -> Self {
        Self {
            reason: reason.to_owned(),
            message: message.into(),
        }
    }
}

/// Every registered provider, by name.
#[derive(Resource, Clone, Default)]
pub struct AgentRegistry {
    /// `name -> (provider, is_perception)`.
    pub observations: BTreeMap<&'static str, (ObservationFn, bool)>,
    pub affordances: BTreeMap<&'static str, AffordanceFn>,
    pub handlers: BTreeMap<&'static str, ActionHandlerFn>,
    /// What a unit perceives or remembers (the affordance gate).
    pub known: BTreeMap<&'static str, KnownFn>,
    /// What a unit holds or rides (always known; not "remembered").
    pub possessions: BTreeMap<&'static str, PossessionsFn>,
    pub describers: BTreeMap<&'static str, DescribeFn>,
    pub unit_info: BTreeMap<&'static str, UnitInfoFn>,
    pub busy: BTreeMap<&'static str, BusyFn>,
    /// Verb gates ([`verb_gate`]); none by default (every verb allowed).
    pub verb_gates: BTreeMap<&'static str, VerbGateFn>,
}

impl AgentRegistry {
    /// Whether any perception provider is registered (else observations are
    /// `omniscient`).
    pub fn has_perception(&self) -> bool {
        self.observations.values().any(|(_, p)| *p)
    }
}

/// Why `entity` may not use `verb`: the first registered verb gate (name
/// order) that refuses, or `None` when every gate allows it (or none is
/// registered). The affordances of a refused verb carry the reason as
/// `why_not` and are structural (`brief` hides them), and `agent/act`
/// rejects it (`not_allowed`); glue that runs a verb on a unit's behalf (an
/// order) asks too.
pub fn verb_gate(world: &World, entity: Entity, verb: &str) -> Option<String> {
    world
        .get_resource::<AgentRegistry>()?
        .verb_gates
        .values()
        .find_map(|f| f(world, entity, verb))
}

fn insert<V>(map: &mut BTreeMap<&'static str, V>, kind: &str, name: &'static str, v: V) {
    assert!(
        map.insert(name, v).is_none(),
        "{kind} `{name}` is registered twice"
    );
}

/// Registration on `App` (the facade re-exports it with feature `agent-io`).
pub trait AgentIoAppExt {
    /// Adds observation section `name`.
    fn register_observation(&mut self, name: &'static str, f: ObservationFn) -> &mut Self;
    /// Adds perception section `name`; with one registered, observations
    /// are no longer `omniscient`.
    fn register_perception(&mut self, name: &'static str, f: ObservationFn) -> &mut Self;
    /// Adds affordance provider `name`.
    fn register_affordances(&mut self, name: &'static str, f: AffordanceFn) -> &mut Self;
    /// Adds the handler of `verb`.
    fn register_action_handler(&mut self, verb: &'static str, f: ActionHandlerFn) -> &mut Self;
    /// Adds known-entities provider `name`: what the unit perceives or
    /// remembers. With a perception provider, affordances only target
    /// known entities (see the module docs).
    fn register_known(&mut self, name: &'static str, f: KnownFn) -> &mut Self;
    /// Adds possessions provider `name`: what the unit holds or rides.
    fn register_possessions(&mut self, name: &'static str, f: PossessionsFn) -> &mut Self;
    /// Adds describer `name` (short entity names for texts).
    fn register_describer(&mut self, name: &'static str, f: DescribeFn) -> &mut Self;
    /// Adds `agent/list_units` fields provider `name`.
    fn register_unit_info(&mut self, name: &'static str, f: UnitInfoFn) -> &mut Self;
    /// Adds busy predicate `name` (`agent/events {until_idle}`).
    fn register_busy(&mut self, name: &'static str, f: BusyFn) -> &mut Self;
    /// Adds verb gate `name` ([`verb_gate`]). Not part of the content hash:
    /// a gate only shapes what an external agent is offered and may submit.
    fn register_verb_gate(&mut self, name: &'static str, f: VerbGateFn) -> &mut Self;
}

impl AgentIoAppExt for App {
    fn register_observation(&mut self, name: &'static str, f: ObservationFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.observations, "observation", name, (f, false));
        self
    }

    fn register_perception(&mut self, name: &'static str, f: ObservationFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.observations, "observation", name, (f, true));
        self
    }

    fn register_affordances(&mut self, name: &'static str, f: AffordanceFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.affordances, "affordance provider", name, f);
        self
    }

    fn register_action_handler(&mut self, verb: &'static str, f: ActionHandlerFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.handlers, "action handler", verb, f);
        self
    }

    fn register_known(&mut self, name: &'static str, f: KnownFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.known, "known provider", name, f);
        self
    }

    fn register_possessions(&mut self, name: &'static str, f: PossessionsFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.possessions, "possessions provider", name, f);
        self
    }

    fn register_describer(&mut self, name: &'static str, f: DescribeFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.describers, "describer", name, f);
        self
    }

    fn register_unit_info(&mut self, name: &'static str, f: UnitInfoFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.unit_info, "unit info provider", name, f);
        self
    }

    fn register_busy(&mut self, name: &'static str, f: BusyFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.busy, "busy predicate", name, f);
        self
    }

    fn register_verb_gate(&mut self, name: &'static str, f: VerbGateFn) -> &mut Self {
        let mut r = self.world_mut().get_resource_or_init::<AgentRegistry>();
        insert(&mut r.verb_gates, "verb gate", name, f);
        self
    }
}
