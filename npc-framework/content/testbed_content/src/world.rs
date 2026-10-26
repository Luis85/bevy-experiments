//! The testbed's own sim components: [`TestbedUnit`] on actors and [`Prop`]
//! on the compound's fixed things (walls, trees, workstations, vehicles), plus
//! the perception kind bits and the non-hashed [`TestbedIds`] lookup.
//!
//! Everything else on testbed entities is capability data (`Item`, `Door`,
//! `ToolTarget`, `Workstation`, `Vehicle`, `CharacterSheet`, `Sensors`, ..),
//! registered and hashed by the capability plugins.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// `Perceivable` kind bits (content-defined; `Sensors::interest` and the
/// perception conditions test against them).
pub mod kinds {
    pub const UNIT: u32 = 1;
    pub const ITEM: u32 = 1 << 1;
    pub const DOOR: u32 = 1 << 2;
    pub const TREE: u32 = 1 << 3;
    pub const STATION: u32 = 1 << 4;
    pub const VEHICLE: u32 = 1 << 5;
    pub const ORE: u32 = 1 << 6;
    /// A chest. Only explorer worlds make chests perceivable (the host,
    /// plan Task 27): the shared scenarios keep their hashes.
    pub const CONTAINER: u32 = 1 << 7;
    /// A wall run, in the fog of war's last-seen statics only
    /// (`FogStatics::occluder_kind`): never on a `Perceivable`.
    pub const WALL: u32 = 1 << 8;
    /// What the fog of war remembers as last seen (`FogStatics::kinds`):
    /// doors, trees, ore rocks, workstations, items and chests. Units and
    /// vehicles move, so they are only ever shown where visible.
    pub const FOG_STATICS: u32 = DOOR | TREE | ORE | STATION | ITEM | CONTAINER;
    /// What stops a robot's LIDAR beam in its cell
    /// (`LidarObstacles::kinds`): units, vehicles, trees, ore rocks,
    /// workstations and chests. Items are small (a beam passes them), and a
    /// door is left to its cell (blocking while closed, open ground once
    /// open).
    pub const LIDAR_OBSTACLES: u32 = UNIT | VEHICLE | TREE | ORE | STATION | CONTAINER;
}

/// What an actor does when nobody orders it (its tree; see [`crate::tree`]).
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
#[serde(rename_all = "snake_case")]
pub enum Behaviour {
    /// Stands still (orders, driving and agents still work).
    #[default]
    Idle,
    /// Walks to random free spots within `wander_radius_mm` of home.
    Wander,
    /// Works `ToolTarget`s it has the equipped tool for (chops trees with an
    /// axe, mines ore with a pickaxe), else wanders.
    Worker,
}

impl Behaviour {
    pub fn tag(self) -> u8 {
        match self {
            Behaviour::Idle => 0,
            Behaviour::Wander => 1,
            Behaviour::Worker => 2,
        }
    }

    /// The registered tree of this behaviour.
    pub fn tree_name(self) -> &'static str {
        match self {
            Behaviour::Idle => crate::tree::TREE_IDLE,
            Behaviour::Wander => crate::tree::TREE_WANDER,
            Behaviour::Worker => crate::tree::TREE_WORKER,
        }
    }
}

/// A testbed actor: the sheet template it was made from, its behaviour,
/// walking speed and the home spot it wanders around.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct TestbedUnit {
    pub template: String,
    pub behaviour: Behaviour,
    pub speed_mm_per_tick: i32,
    pub home: SimPos,
    pub wander_radius_mm: i32,
}

impl SimHash for TestbedUnit {
    const NAME: &'static str = "testbed_unit";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.template);
        h.u8(self.behaviour.tag());
        h.i32(self.speed_mm_per_tick);
        self.home.sim_hash(h);
        h.i32(self.wander_radius_mm);
    }
}

/// What a fixed thing of the compound is (the view picks its model by it).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
#[serde(rename_all = "snake_case")]
pub enum PropKind {
    /// A straight wall run: its cells are the `GridBlocker` footprint.
    Wall,
    /// A doorway frame holding a `Door`.
    Doorway,
    Tree,
    OreRock,
    Workbench,
    Anvil,
    Forge,
    Chest,
    /// The `ByCrew` cart.
    Cart,
    /// The `Direct` armored wagon.
    Wagon,
}

impl PropKind {
    pub const ALL: [PropKind; 10] = [
        PropKind::Wall,
        PropKind::Doorway,
        PropKind::Tree,
        PropKind::OreRock,
        PropKind::Workbench,
        PropKind::Anvil,
        PropKind::Forge,
        PropKind::Chest,
        PropKind::Cart,
        PropKind::Wagon,
    ];

    pub fn tag(self) -> u8 {
        match self {
            PropKind::Wall => 0,
            PropKind::Doorway => 1,
            PropKind::Tree => 2,
            PropKind::OreRock => 3,
            PropKind::Workbench => 4,
            PropKind::Anvil => 5,
            PropKind::Forge => 6,
            PropKind::Chest => 7,
            PropKind::Cart => 8,
            PropKind::Wagon => 9,
        }
    }

    /// Kind name (inspect, journal `Spawn`, the view).
    pub fn name(self) -> &'static str {
        match self {
            PropKind::Wall => "wall",
            PropKind::Doorway => "doorway",
            PropKind::Tree => "tree",
            PropKind::OreRock => "ore_rock",
            PropKind::Workbench => "workbench",
            PropKind::Anvil => "anvil",
            PropKind::Forge => "forge",
            PropKind::Chest => "chest",
            PropKind::Cart => "cart",
            PropKind::Wagon => "wagon",
        }
    }
}

/// A fixed thing of the compound, with its facing for the view (whole
/// degrees about the vertical axis; the sim never reads it).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Prop {
    pub kind: PropKind,
    pub yaw_deg: i32,
}

impl Prop {
    pub const fn new(kind: PropKind) -> Self {
        Self { kind, yaw_deg: 0 }
    }
}

impl SimHash for Prop {
    const NAME: &'static str = "testbed_prop";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(self.kind.tag());
        h.i32(self.yaw_deg);
    }
}

/// Names of what a scenario spawned (`"gate"`, `"store_door"`, `"cart"`,
/// `"tree_0"`, named actors, ..) → `SimId`, plus the actors in scenario
/// order. A lookup for tests, tools and the view; **not** hashed (it only
/// mirrors spawn order).
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub struct TestbedIds {
    pub names: BTreeMap<String, SimId>,
    /// Every actor, in the order of the scenario's `actors`.
    pub units: Vec<SimId>,
}

impl TestbedIds {
    /// The `SimId` named `name`.
    ///
    /// # Panics
    /// When nothing of that name was spawned.
    pub fn id(&self, name: &str) -> SimId {
        *self.names.get(name).unwrap_or_else(|| {
            panic!(
                "testbed has nothing named `{name}` (known: {:?})",
                self.names.keys().collect::<Vec<_>>()
            )
        })
    }

    pub fn get(&self, name: &str) -> Option<SimId> {
        self.names.get(name).copied()
    }

    pub(crate) fn name(&mut self, name: impl Into<String>, id: SimId) {
        let name = name.into();
        assert!(
            self.names.insert(name.clone(), id).is_none(),
            "testbed name `{name}` used twice"
        );
    }
}

/// Kind namer (`register_kind_name("testbed", ..)`; sorts before the
/// capability crates' `~npc_*` namers, so a cart is a "cart", not a vehicle).
pub fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    if let Some(p) = world.get::<Prop>(entity) {
        return Some(p.kind.name());
    }
    world.get::<TestbedUnit>(entity).map(|_| "actor")
}

/// The testbed's fog-of-war statics (plan Task 9, spec G8): doors with
/// their open state, trees felled or standing, ore rocks, workstations,
/// items and chests ([`kinds::FOG_STATICS`]), and the wall runs (their
/// `GridOccluder`, as [`kinds::WALL`]).
pub fn fog_statics() -> npc_framework::perception::FogStatics {
    npc_framework::perception::FogStatics {
        kinds: kinds::FOG_STATICS,
        occluder_kind: kinds::WALL,
        open: Some(door_open),
        felled: Some(tree_felled),
    }
}

/// The testbed's LIDAR obstacles (plan Task 12 fix round 1):
/// [`kinds::LIDAR_OBSTACLES`].
pub fn lidar_obstacles() -> npc_framework::perception::LidarObstacles {
    npc_framework::perception::LidarObstacles {
        kinds: kinds::LIDAR_OBSTACLES,
    }
}

/// A door's open state (`None` for anything else).
fn door_open(e: EntityRef) -> Option<bool> {
    e.get::<npc_framework::interaction::Door>().map(|d| d.open)
}

/// Whether a tree is felled (its `ToolTarget` used up: the view shows a
/// stump); `None` for anything but a tree.
fn tree_felled(e: EntityRef) -> Option<bool> {
    if e.get::<Prop>()?.kind != PropKind::Tree {
        return None;
    }
    e.get::<npc_framework::interaction::ToolTarget>()
        .map(npc_framework::interaction::ToolTarget::depleted)
}
