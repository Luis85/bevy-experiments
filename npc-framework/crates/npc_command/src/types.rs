//! Components and order data: ownership, the per-unit [`OrderQueue`], the
//! [`UnitOrder`] vocabulary, squads.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::json::JsonObject;
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

use crate::formation::Formation;

/// A player (a commanding side). Serializes as a plain number. Player 0 is
/// the default owner of a [`Commandable`] unit without an explicit
/// [`Owner`] (single-player hosts never need to name it).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
#[serde(transparent)]
pub struct PlayerId(pub u32);

/// Who may order this unit (or squad). Every order names a [`PlayerId`] and
/// is rejected unless it matches.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Owner(pub PlayerId);

/// Marks a unit that takes RTS orders, with how it moves while executing
/// them. Requires an [`OrderQueue`] and an [`Owner`] (player 0 unless set).
///
/// Units **without** a `Brain` execute their orders directly (the command
/// engine writes their `MoveIntent`). Units **with** a `Brain` execute them
/// only while their tree runs the `ExecuteOrder` leaf (guard it with the
/// `has_unit_order` condition), so a tree decides when orders win.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[require(OrderQueue, Owner)]
#[reflect(Component)]
pub struct Commandable {
    /// Speed of order moves, mm per tick.
    pub speed_mm_per_tick: i32,
    /// How close (mm) a `Move` / `Patrol` point counts as reached.
    pub arrive_mm: i32,
}

impl Default for Commandable {
    fn default() -> Self {
        Self {
            speed_mm_per_tick: 100,
            arrive_mm: 0,
        }
    }
}

impl Commandable {
    pub const fn new(speed_mm_per_tick: i32) -> Self {
        Self {
            speed_mm_per_tick,
            arrive_mm: 0,
        }
    }
}

/// One RTS order. JSON: tagged by `"type"`, positions `[x, z]` in mm, ids
/// plain `SimId` numbers, e.g. `{"type":"Move","pos":[1000,2000]}`,
/// `{"type":"Follow","target":7}`, `{"type":"Patrol","points":[[0,0],[5000,0]]}`,
/// `{"type":"Custom","name":"repair","args":{"wall":12}}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[serde(tag = "type")]
pub enum UnitOrder {
    /// Walk to `pos`; done on arrival.
    Move { pos: (i32, i32) },
    /// A `Move` with its own journal tag (combat does not exist yet).
    AttackMove { pos: (i32, i32) },
    /// Drop everything: the current order and the queue (done at once).
    Stop,
    /// Stand where you are until another order replaces this one (never
    /// completes on its own, so orders queued behind it wait).
    Hold,
    /// Stay within `CommandConfig::follow_distance_mm` of `target`; fails
    /// when the target leaves the sim, never completes on its own.
    Follow { target: SimId },
    /// Walk the points in a loop (one point: between it and where the unit
    /// stood when the patrol started). Never completes on its own.
    Patrol { points: Vec<(i32, i32)> },
    /// Do interaction `verb` to `target`. Data only: needs an executor
    /// registered for `"Interact"` (the command × interaction glue).
    Interact { verb: String, target: SimId },
    /// Board `vehicle`. Data only: needs an executor for `"Mount"`.
    Mount { vehicle: SimId },
    /// Unload a vehicle's occupants. Data only: needs an executor for `"Unload"`.
    Unload,
    /// A game-defined order kind: run by the executor registered under
    /// `name` ([`CommandAppExt::register_custom_order`](crate::CommandAppExt::register_custom_order));
    /// rejected when issued without one. `args` is the order's own data
    /// (field order kept; hashed as its JSON text), e.g. read with
    /// `args.to_typed::<MyArgs>()`. The journal's `order` field is `name`.
    Custom {
        name: String,
        #[serde(default)]
        #[reflect(ignore)]
        args: JsonObject,
    },
}

impl UnitOrder {
    /// The kind name of [`UnitOrder::Custom`] ([`UnitOrder::kind`]); custom
    /// orders are dispatched by their `name`, not by this kind.
    pub const CUSTOM: &'static str = "Custom";

    /// Every built-in order kind name ([`UnitOrder::kind`]), in declaration
    /// order: the kinds [`CommandAppExt::register_order_executor`](crate::CommandAppExt::register_order_executor)
    /// accepts. [`UnitOrder::Custom`] (kind [`UnitOrder::CUSTOM`]) is not
    /// among them: its executors are registered by name.
    pub const KINDS: [&'static str; 9] = [
        "Move",
        "AttackMove",
        "Stop",
        "Hold",
        "Follow",
        "Patrol",
        "Interact",
        "Mount",
        "Unload",
    ];

    /// The kind name (the JSON `"type"` tag; the journal's `order` field;
    /// the key executors are registered under).
    pub fn kind(&self) -> &'static str {
        match self {
            UnitOrder::Move { .. } => "Move",
            UnitOrder::AttackMove { .. } => "AttackMove",
            UnitOrder::Stop => "Stop",
            UnitOrder::Hold => "Hold",
            UnitOrder::Follow { .. } => "Follow",
            UnitOrder::Patrol { .. } => "Patrol",
            UnitOrder::Interact { .. } => "Interact",
            UnitOrder::Mount { .. } => "Mount",
            UnitOrder::Unload => "Unload",
            UnitOrder::Custom { .. } => Self::CUSTOM,
        }
    }

    /// The name the journal records in `order` (`OrderIssued`,
    /// `OrderStarted`, `OrderDone`, `OrderFailed`): the [`UnitOrder::kind`],
    /// or a custom order's `name`.
    pub fn journal_name(&self) -> String {
        match self {
            UnitOrder::Custom { name, .. } => name.clone(),
            other => other.kind().to_owned(),
        }
    }

    /// A custom order's `(name, args)`.
    pub fn custom(&self) -> Option<(&str, &JsonObject)> {
        match self {
            UnitOrder::Custom { name, args } => Some((name.as_str(), args)),
            _ => None,
        }
    }

    /// `UnitOrder::Custom { name, args }` from typed args (anything that
    /// serializes to a JSON object; field order kept).
    ///
    /// # Errors
    /// When `args` does not serialize to a JSON object.
    pub fn custom_with(name: impl Into<String>, args: &impl Serialize) -> Result<Self, String> {
        Ok(UnitOrder::Custom {
            name: name.into(),
            args: JsonObject::from_typed(args)?,
        })
    }

    /// The `u8` discriminant (declaration order) used by the state hash.
    pub fn discriminant(&self) -> u8 {
        match self {
            UnitOrder::Move { .. } => 0,
            UnitOrder::AttackMove { .. } => 1,
            UnitOrder::Stop => 2,
            UnitOrder::Hold => 3,
            UnitOrder::Follow { .. } => 4,
            UnitOrder::Patrol { .. } => 5,
            UnitOrder::Interact { .. } => 6,
            UnitOrder::Mount { .. } => 7,
            UnitOrder::Unload => 8,
            UnitOrder::Custom { .. } => 9,
        }
    }

    /// The destination of a `Move` / `AttackMove`.
    pub fn move_goal(&self) -> Option<SimPos> {
        match *self {
            UnitOrder::Move { pos: (x, z) } | UnitOrder::AttackMove { pos: (x, z) } => {
                Some(SimPos::new(x, z))
            }
            _ => None,
        }
    }

    /// Where the unit will stand once this order is done, as far as the
    /// order says (`Move`/`AttackMove` goal, last patrol point); used as the
    /// "from" point of formations for shift-queued group moves.
    pub fn end_point(&self) -> Option<SimPos> {
        match self {
            UnitOrder::Patrol { points } => points.last().map(|&(x, z)| SimPos::new(x, z)),
            other => other.move_goal(),
        }
    }

    /// The other unit the order targets, if any.
    pub fn target(&self) -> Option<SimId> {
        match *self {
            UnitOrder::Follow { target } | UnitOrder::Interact { target, .. } => Some(target),
            UnitOrder::Mount { vehicle } => Some(vehicle),
            _ => None,
        }
    }
}

/// The kernel's minimal standing order as an RTS order (`GoTo` → `Move`,
/// `Clear` → `Stop`). The two systems stay independent: `npc_command` never
/// reads or writes the kernel's `CurrentOrder`.
impl From<npc_kernel::Order> for UnitOrder {
    fn from(order: npc_kernel::Order) -> Self {
        match order {
            npc_kernel::Order::GoTo { pos } => UnitOrder::Move { pos },
            npc_kernel::Order::Clear => UnitOrder::Stop,
        }
    }
}

/// A unit's place in a group move: its leader (whose path followers with a
/// `NavAgent` reuse, shifted by `offset`) and its offset from the leader's
/// slot in mm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct GroupLink {
    pub leader: SimId,
    /// This unit's slot minus the leader's slot.
    pub offset: (i32, i32),
}

/// An order waiting in a unit's queue.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct QueuedOrder {
    pub order: UnitOrder,
    /// Set for the members of a group move.
    pub group: Option<GroupLink>,
    /// The player who issued it.
    pub player: PlayerId,
    /// Tick it was issued.
    pub issued: u64,
}

/// Where the built-in executor of the current order stands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum OrderProgress {
    /// Not started (or paused by the tree: started again on resume).
    #[default]
    NotStarted,
    /// Walking to the goal (`Move`, `AttackMove`, `Follow`).
    Moving,
    /// A group follower with a `NavAgent` waiting (since `since`) for the
    /// leader's path, to reuse it shifted by its offset.
    WaitingForLeader { since: u64 },
    /// Holding at `at`.
    Holding { at: SimPos },
    /// Patrolling `route`, walking to `route[next]`.
    Patrolling { route: Vec<SimPos>, next: u32 },
    /// Run by a registered executor (its own state lives elsewhere).
    Hooked,
}

/// The order a unit is executing.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct ActiveOrder {
    pub order: QueuedOrder,
    /// Tick the order started (`None` until it starts).
    pub started: Option<u64>,
    pub progress: OrderProgress,
}

/// Per-unit orders: the current one and the shift-queue behind it (front =
/// next). Written only by order commands and the command engine.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct OrderQueue {
    pub current: Option<ActiveOrder>,
    pub queued: Vec<QueuedOrder>,
    /// How the last finished order ended (`true`: done, `false`: failed);
    /// the `ExecuteOrder` leaf reports it when the queue runs dry.
    pub last_ok: bool,
}

impl OrderQueue {
    /// Whether the unit has anything to do (the `has_unit_order` condition).
    pub fn has_order(&self) -> bool {
        self.current.is_some() || !self.queued.is_empty()
    }

    /// Number of orders held (current + queued).
    pub fn len(&self) -> usize {
        usize::from(self.current.is_some()) + self.queued.len()
    }

    pub fn is_empty(&self) -> bool {
        !self.has_order()
    }

    /// The orders in execution order (current first).
    pub fn orders(&self) -> impl Iterator<Item = &UnitOrder> {
        self.current
            .as_ref()
            .map(|a| &a.order.order)
            .into_iter()
            .chain(self.queued.iter().map(|q| &q.order))
    }
}

/// Marker: this `Brain` unit's tree is running the `ExecuteOrder` leaf, so
/// the engine executes its orders.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct ExecutingOrder;

/// A squad: an entity (with `SimId` and [`Owner`]) whose members are
/// ordered together. `members` is sorted by `SimId`; members that leave the
/// sim are pruned. A squad can later carry its own `Brain`.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[require(Owner)]
#[reflect(Component)]
pub struct Squad {
    pub members: Vec<SimId>,
    /// The formation a squad order uses (unless the order names one).
    pub formation: Formation,
}

/// The squad a unit belongs to (at most one).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct MemberOf(pub SimId);
