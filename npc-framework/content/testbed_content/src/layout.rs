//! The compound: walls, doors, furnishing and actors (spawned by the
//! scenario loader, [`crate::scenario`]).
//!
//! 1 m grid cells; a cell `(x, z)` has its centre at `(x·1000 + 500,
//! z·1000 + 500)` mm (+z is north). The compound spans cells x −12..=12,
//! z −10..=10:
//!
//! - outer walls with the **gate** (closed, unlocked) at cell (0, −10);
//! - an inner wall on row z = 3 with the **workshop door** (closed) at
//!   (−6, 3) and the **store door** (locked, key `key:store`) at (6, 3);
//! - a divider on column x = 0 (z 4..=9) between the **workshop** (west:
//!   workbench, anvil, forge, a book) and the **store** (east: pickaxe,
//!   hammer, a scroll, a chest);
//! - the **yard** (south: three trees, an ore rock, the axe, the store key,
//!   bread, a log pile and the `ByCrew` cart); outside the gate the `Direct`
//!   armored wagon and two more trees.
//!
//! Each straight wall run is one sim entity: `Prop { Wall }` plus a
//! `GridBlocker` and a `GridOccluder` with the run's footprint. Doors are
//! `Door` entities in one-cell gaps (the interaction plugin makes a closed
//! door's cell blocking and occluding). [`ascii_map`] draws the result.

use bevy_ecs::prelude::*;
use npc_framework::interaction::items::{kind_def, put_in_inventory, put_in_slot};
use npc_framework::kernel::spatial::CellCoord;
use npc_framework::prelude::*;
use npc_framework::rules::template_sheet;
use serde::Deserialize;

use crate::content::{
    ANVIL, AXE, BOOK, BREAD, FORGE, HAMMER, KEY_STORE, LOG, MINING, ORE, PICKAXE, SCROLL,
    STORE_KEY, TOOL_AXE, TOOL_PICKAXE, WOODCUTTING, WORKBENCH,
};
use crate::world::{Behaviour, Prop, PropKind, TestbedIds, TestbedUnit, kinds};

/// Compound half extent in cells (x).
pub const HALF_X: i32 = 12;
/// Compound half extent in cells (z).
pub const HALF_Z: i32 = 10;
/// Row of the inner wall (workshop and store to the north, yard south).
pub const INNER_Z: i32 = 3;
pub const GATE_CELL: (i32, i32) = (0, -HALF_Z);
pub const WORKSHOP_DOOR_CELL: (i32, i32) = (-6, INNER_Z);
pub const STORE_DOOR_CELL: (i32, i32) = (6, INNER_Z);
/// Grid size in cells (centred on the origin).
pub const GRID_CELLS: u32 = 64;
/// Grid size of the `frontier` layout.
pub const FRONTIER_GRID_CELLS: u32 = 128;

/// Actor walking speed, mm per tick (1.4 m/s at 20 Hz).
pub const UNIT_SPEED: i32 = 70;
pub const DEFAULT_WANDER_RADIUS_MM: i32 = 3000;
/// Vision range / half angle, hearing radius of every actor.
pub const VISION_RANGE_MM: i32 = 15_000;
pub const VISION_HALF_FOV_DEG: u16 = 60;
pub const HEARING_MM: i32 = 10_000;
pub const INVENTORY_SLOTS: u32 = 8;
pub const INVENTORY_MAX_G: u32 = 30_000;
pub const CART_SPEED: i32 = 80;
pub const WAGON_SPEED: i32 = 70;

/// The centre of cell `(x, z)`.
pub const fn cell(x: i32, z: i32) -> SimPos {
    SimPos::new(x * 1000 + 500, z * 1000 + 500)
}

// ---------------------------------------------------------------- walls

/// One wall run from cell `a` to cell `b` (same row or column, inclusive).
pub fn spawn_wall(world: &mut World, a: (i32, i32), b: (i32, i32)) -> SimId {
    assert!(a.0 == b.0 || a.1 == b.1, "wall runs are straight");
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
    let centre = SimPos::new((x0 + x1 + 1) * 500, (z0 + z1 + 1) * 500);
    let fp = Footprint {
        half_x_mm: (x1 - x0) * 500,
        half_z_mm: (z1 - z0) * 500,
    };
    let yaw = if z0 == z1 && x0 != x1 { 0 } else { 90 };
    spawn_sim(
        world,
        (
            centre,
            Prop {
                kind: PropKind::Wall,
                yaw_deg: yaw,
            },
            GridBlocker(fp),
            GridOccluder(fp),
        ),
    )
    .0
}

fn spawn_door(world: &mut World, at: (i32, i32), door: Door) -> SimId {
    spawn_sim(
        world,
        (
            cell(at.0, at.1),
            door,
            Prop::new(PropKind::Doorway),
            Perceivable::new(kinds::DOOR),
        ),
    )
    .0
}

/// Walls and doors (see the module docs).
pub fn spawn_walls(world: &mut World, ids: &mut TestbedIds, store_locked: bool) {
    let (hx, hz) = (HALF_X, HALF_Z);
    // Outer ring (the gate gap in the south wall).
    spawn_wall(world, (-hx, -hz), (GATE_CELL.0 - 1, -hz));
    spawn_wall(world, (GATE_CELL.0 + 1, -hz), (hx, -hz));
    spawn_wall(world, (-hx, hz), (hx, hz));
    spawn_wall(world, (-hx, -hz + 1), (-hx, hz - 1));
    spawn_wall(world, (hx, -hz + 1), (hx, hz - 1));
    // Inner wall with two doors, and the workshop/store divider.
    spawn_wall(
        world,
        (-hx + 1, INNER_Z),
        (WORKSHOP_DOOR_CELL.0 - 1, INNER_Z),
    );
    spawn_wall(
        world,
        (WORKSHOP_DOOR_CELL.0 + 1, INNER_Z),
        (STORE_DOOR_CELL.0 - 1, INNER_Z),
    );
    spawn_wall(world, (STORE_DOOR_CELL.0 + 1, INNER_Z), (hx - 1, INNER_Z));
    spawn_wall(world, (0, INNER_Z + 1), (0, hz - 1));

    let gate = spawn_door(world, GATE_CELL, Door::closed());
    ids.name("gate", gate);
    let workshop = spawn_door(world, WORKSHOP_DOOR_CELL, Door::closed());
    ids.name("workshop_door", workshop);
    let store_door = if store_locked {
        Door::locked(KEY_STORE)
    } else {
        Door::lockable(KEY_STORE)
    };
    let store = spawn_door(world, STORE_DOOR_CELL, store_door);
    ids.name("store_door", store);
}

// ---------------------------------------------------------------- things

/// An item lying at `pos` (a book or scroll also [`Teaches`](crate::content::teaches)).
pub fn spawn_item(world: &mut World, kind: &str, stack: u32, quality: i32, pos: SimPos) -> SimId {
    let (id, e) = spawn_sim(
        world,
        (
            pos,
            Item::new(kind).stack(stack).quality(quality),
            Perceivable::new(kinds::ITEM),
        ),
    );
    if let Some(t) = crate::content::teaches(kind) {
        world.entity_mut(e).insert(t);
    }
    id
}

fn tree(world: &mut World, pos: SimPos) -> SimId {
    spawn_sim(
        world,
        (
            pos,
            Prop::new(PropKind::Tree),
            ToolTarget::new(TOOL_AXE)
                .yields(ItemYield::new(LOG, 2))
                .duration(60)
                .check(WOODCUTTING, 2)
                .uses(3),
            Perceivable::new(kinds::TREE),
        ),
    )
    .0
}

fn ore_rock(world: &mut World, pos: SimPos) -> SimId {
    spawn_sim(
        world,
        (
            pos,
            Prop::new(PropKind::OreRock),
            ToolTarget::new(TOOL_PICKAXE)
                .yields(ItemYield::new(ORE, 1))
                .duration(80)
                .check(MINING, 0)
                .uses(6),
            Perceivable::new(kinds::ORE),
        ),
    )
    .0
}

fn station(world: &mut World, kind: PropKind, station: &str, pos: SimPos) -> SimId {
    spawn_sim(
        world,
        (
            pos,
            Prop::new(kind),
            Workstation::new([station]),
            Perceivable::new(kinds::STATION),
        ),
    )
    .0
}

/// The `ByCrew` cart: a required driver seat and one passenger seat.
pub fn cart_vehicle() -> Vehicle {
    Vehicle::by_crew(CART_SPEED)
        .seat(Seat::driver())
        .seat(Seat::passenger().offset(0, -700))
}

/// The `Direct` armored wagon: an optional (cosmetic) driver seat and two
/// passenger seats; it is a unit itself (orders, nav).
pub fn wagon_vehicle() -> Vehicle {
    Vehicle::direct(WAGON_SPEED)
        .seat(Seat::driver().optional())
        .seats(2, Seat::passenger().offset(0, -900))
}

fn vehicle(world: &mut World, kind: PropKind, v: Vehicle, speed: i32, pos: SimPos) -> SimId {
    spawn_sim(
        world,
        (
            pos,
            Prop::new(kind),
            v,
            Commandable::new(speed),
            Owner(PlayerId(1)),
            NavAgent,
            Perceivable::new(kinds::VEHICLE),
        ),
    )
    .0
}

/// Workstations, items, trees, the ore rock and the vehicles.
pub fn spawn_furnishing(world: &mut World, ids: &mut TestbedIds) {
    // Workshop.
    let id = station(world, PropKind::Workbench, WORKBENCH, cell(-9, 7));
    ids.name("workbench", id);
    let id = station(world, PropKind::Anvil, ANVIL, cell(-4, 7));
    ids.name("anvil", id);
    let id = station(world, PropKind::Forge, FORGE, cell(-2, 8));
    ids.name("forge", id);
    let id = spawn_item(world, BOOK, 1, 0, cell(-7, 5));
    ids.name("book", id);
    // Store.
    let id = spawn_item(world, PICKAXE, 1, 0, cell(4, 7));
    ids.name("pickaxe", id);
    let id = spawn_item(world, HAMMER, 1, 0, cell(6, 8));
    ids.name("hammer", id);
    let id = spawn_item(world, SCROLL, 1, 0, cell(8, 5));
    ids.name("scroll", id);
    let (id, _) = spawn_sim(world, (cell(9, 8), Prop::new(PropKind::Chest)));
    ids.name("chest", id);
    // Yard.
    for (i, (x, z)) in [(-9, -7), (-9, -3), (-6, -8)].into_iter().enumerate() {
        let id = tree(world, cell(x, z));
        ids.name(format!("tree_{i}"), id);
    }
    let id = ore_rock(world, cell(9, -7));
    ids.name("ore_rock", id);
    let id = spawn_item(world, AXE, 1, 0, cell(-4, -5));
    ids.name("axe", id);
    let id = spawn_item(world, STORE_KEY, 1, 0, cell(8, -2));
    ids.name("store_key", id);
    let id = spawn_item(world, BREAD, 3, 0, cell(1, -4));
    ids.name("bread", id);
    let id = spawn_item(world, LOG, 2, 0, cell(-3, -8));
    ids.name("logs", id);
    let id = vehicle(
        world,
        PropKind::Cart,
        cart_vehicle(),
        CART_SPEED,
        cell(4, -6),
    );
    ids.name("cart", id);
    // Outside the gate.
    let id = vehicle(
        world,
        PropKind::Wagon,
        wagon_vehicle(),
        WAGON_SPEED,
        cell(0, -15),
    );
    ids.name("wagon", id);
    for (i, (x, z)) in [(-5, -15), (6, -16)].into_iter().enumerate() {
        let id = tree(world, cell(x, z));
        ids.name(format!("tree_{}", i + 3), id);
    }
}

/// The ruin's wall ring: cells x −3..=3, z 26..=30, the gap at `(0, 26)`.
pub const RUIN_GAP_CELL: (i32, i32) = (0, 26);

/// The outside features of the `frontier` layout, spawned after the compound
/// (so its ids are those of every compound scenario): a tree grove west and
/// east of the compound, three ore rocks north-east, a ruin to the north (a
/// 7×5 wall ring with a one-cell gap on its south side, a chest and a scroll
/// inside) and items scattered where the start does not see them.
pub fn spawn_frontier(world: &mut World, ids: &mut TestbedIds) {
    const WEST: [(i32, i32); 7] = [
        (-26, -6),
        (-29, -3),
        (-25, -1),
        (-28, 2),
        (-24, 4),
        (-30, 5),
        (-27, 8),
    ];
    const EAST: [(i32, i32); 6] = [(24, -5), (27, -2), (25, 1), (29, 3), (23, 6), (28, 8)];
    for (i, (x, z)) in WEST.into_iter().chain(EAST).enumerate() {
        let id = tree(world, cell(x, z));
        ids.name(format!("grove_tree_{i}"), id);
    }
    for (i, (x, z)) in [(20, 15), (23, 17), (26, 14)].into_iter().enumerate() {
        let id = ore_rock(world, cell(x, z));
        ids.name(format!("far_ore_{i}"), id);
    }
    // The ruin.
    spawn_wall(world, (-3, 26), (-1, 26));
    spawn_wall(world, (1, 26), (3, 26));
    spawn_wall(world, (-3, 30), (3, 30));
    spawn_wall(world, (-3, 27), (-3, 29));
    spawn_wall(world, (3, 27), (3, 29));
    let (id, _) = spawn_sim(world, (cell(-1, 28), Prop::new(PropKind::Chest)));
    ids.name("ruin_chest", id);
    let id = spawn_item(world, SCROLL, 1, 0, cell(1, 28));
    ids.name("ruin_scroll", id);
    // Scattered items.
    for (i, (x, z)) in [(-18, -16), (31, -7)].into_iter().enumerate() {
        let id = spawn_item(world, BREAD, 1, 0, cell(x, z));
        ids.name(format!("far_bread_{i}"), id);
    }
    for (i, (x, z)) in [(14, -22), (-16, 20), (34, 20)].into_iter().enumerate() {
        let id = spawn_item(world, LOG, 1, 0, cell(x, z));
        ids.name(format!("far_log_{i}"), id);
    }
    let id = spawn_item(world, BOOK, 1, 0, cell(-36, 14));
    ids.name("far_book", id);
}

// ---------------------------------------------------------------- actors

/// An item an actor starts with (`carry`) or one placed in the world
/// (`world.items`, with `pos`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub kind: String,
    #[serde(default = "one")]
    pub stack: u32,
    #[serde(default)]
    pub quality: i32,
    #[serde(default)]
    pub pos: Option<(i32, i32)>,
    #[serde(default)]
    pub name: Option<String>,
}

fn one() -> u32 {
    1
}

/// The testbed fields of a scenario actor (`{"kind":"unit", "pos":[x,z],
/// ..these}`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct UnitSpec {
    /// Registered sheet template (required).
    pub template: String,
    /// Name in [`TestbedIds`].
    pub name: Option<String>,
    /// `PlayerId` that owns it (default 1).
    pub owner: Option<u32>,
    pub behaviour: Behaviour,
    /// Initial `Facing` `[x, z]` (default north, `[0, 1]`).
    pub facing: Option<(i32, i32)>,
    /// Movement does not turn it (`FixedFacing`).
    pub fixed_facing: bool,
    /// Item kinds spawned into its equipment (each kind's slot).
    pub equip: Vec<String>,
    /// Items spawned into its inventory.
    pub carry: Vec<ItemSpec>,
    /// `"ai"` (default) or `"player"` spend policy.
    pub spend_policy: Option<String>,
    pub wander_radius_mm: Option<i32>,
    pub speed_mm_per_tick: Option<i32>,
    /// `Stealthy { penalty }` (Per-roll detection modifier against it).
    pub stealthy: Option<i32>,
    /// Actor preset id ([`crate::actors`]): applied after the rest
    /// (`apply_preset`: its sheet, speed, carrying capacity, senses, allowed
    /// actions and model replace the template's), from the
    /// [`ActorPresets`](crate::actors::ActorPresets) resource or the
    /// built-ins; an invalid preset is never spawned.
    pub preset: Option<String>,
}

fn carried_item(world: &mut World, holder: Entity, spec: &ItemSpec) -> Result<Entity, String> {
    if kind_def(world, &spec.kind).is_none() {
        return Err(format!("unknown item kind `{}`", spec.kind));
    }
    let pos = world.get::<SimPos>(holder).copied().unwrap_or_default();
    let (_, e) = spawn_sim(
        world,
        (
            pos,
            Item::new(spec.kind.clone())
                .stack(spec.stack)
                .quality(spec.quality),
            Perceivable::new(kinds::ITEM),
        ),
    );
    if let Some(t) = crate::content::teaches(&spec.kind) {
        world.entity_mut(e).insert(t);
    }
    put_in_inventory(world, holder, e);
    Ok(e)
}

/// Spawns one actor (see [`UnitSpec`]); returns its `SimId`.
pub fn spawn_unit(world: &mut World, pos: SimPos, spec: &UnitSpec) -> Result<SimId, String> {
    // Checked before anything spawns.
    let preset = match &spec.preset {
        None => None,
        Some(name) => {
            let preset = crate::actors::resolve_preset(world, name)
                .ok_or_else(|| format!("unknown actor preset `{name}`"))?;
            crate::actors::validate(&preset, world).map_err(|reasons| {
                format!("actor preset `{name}` is invalid: {}", reasons.join("; "))
            })?;
            Some(preset)
        }
    };
    let sheet = template_sheet(world, &spec.template).ok_or_else(|| {
        format!(
            "unknown sheet template `{}` (known: {:?})",
            spec.template,
            crate::content::TEMPLATES
        )
    })?;
    let tree_name = spec.behaviour.tree_name();
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(tree_name)
        .ok_or_else(|| format!("tree `{tree_name}` is not registered"))?;
    let owner = spec.owner.unwrap_or(1);
    let speed = spec.speed_mm_per_tick.unwrap_or(UNIT_SPEED);
    let policy = match spec.spend_policy.as_deref() {
        None | Some("ai") => SpendPolicy::Ai,
        Some("player") => SpendPolicy::Player,
        Some(other) => return Err(format!("spend_policy must be ai or player, not `{other}`")),
    };
    let facing = spec
        .facing
        .map_or(Facing::default(), |(x, z)| Facing::new(x, z));
    let (id, e) = spawn_sim(
        world,
        (
            pos,
            TestbedUnit {
                template: spec.template.clone(),
                behaviour: spec.behaviour,
                speed_mm_per_tick: speed,
                home: pos,
                wander_radius_mm: spec.wander_radius_mm.unwrap_or(DEFAULT_WANDER_RADIUS_MM),
            },
            Brain { tree },
            sheet,
            (Experience::default(), policy),
            (
                Sensors::new()
                    .vision(VISION_RANGE_MM, VISION_HALF_FOV_DEG)
                    .hearing(HEARING_MM),
                facing,
                Perceivable::new(kinds::UNIT),
            ),
            (
                Interactor,
                Inventory::new(INVENTORY_SLOTS, INVENTORY_MAX_G),
                Equipment::hands(),
            ),
            (Commandable::new(speed), Owner(PlayerId(owner)), NavAgent),
        ),
    );
    if spec.fixed_facing {
        world.entity_mut(e).insert(FixedFacing);
    }
    if let Some(t) = crate::content::teacher(&spec.template) {
        world.entity_mut(e).insert(t);
    }
    if let Some(penalty) = spec.stealthy {
        world.entity_mut(e).insert(Stealthy { penalty });
    }
    for kind in &spec.equip {
        let slot = kind_def(world, kind)
            .ok_or_else(|| format!("unknown item kind `{kind}`"))?
            .equip_slot
            .clone()
            .ok_or_else(|| format!("item kind `{kind}` cannot be equipped"))?;
        let item = carried_item(
            world,
            e,
            &ItemSpec {
                kind: kind.clone(),
                stack: 1,
                quality: 0,
                pos: None,
                name: None,
            },
        )?;
        put_in_slot(world, e, item, &slot);
    }
    for c in &spec.carry {
        carried_item(world, e, c)?;
    }
    if let Some(preset) = &preset {
        crate::actors::apply_preset(world, e, preset);
    }
    Ok(id)
}

// ---------------------------------------------------------------- map

/// The ASCII map of `x0..=x1` × `z1..=z0` cells (north up): `#` wall, `D`
/// closed door, `L` locked door, `/` open door, `T` tree, `O` ore rock, `W`
/// workbench, `A` anvil, `F` forge, `C` chest, `c` cart, `V` wagon, items by
/// the first letter of their kind (`k` key, `x` axe, `p` pickaxe, `h` hammer,
/// `b` book, `s` scroll, `f` food, `l` logs, `o` ore, `i` ingot, `n` plank),
/// actors by their owner's digit, `.` open ground.
pub fn ascii_map(world: &World, x0: i32, x1: i32, z0: i32, z1: i32) -> String {
    // From the components (not the grid's index, which is only current
    // after a tick's `NpcSet::Inputs`), so it also works at tick 0.
    let grid = world.resource::<SpatialGrid>();
    let mut marks: std::collections::BTreeMap<CellCoord, (u8, char)> =
        std::collections::BTreeMap::new();
    let mut put = |c: CellCoord, mark: (u8, char)| {
        let e = marks.entry(c).or_insert((0, '.'));
        if mark.0 > e.0 {
            *e = mark;
        }
    };
    for &e in world.resource::<SimIndex>().map.values() {
        let Some(&pos) = world.get::<SimPos>(e) else {
            continue;
        };
        if let Some(b) = world.get::<GridBlocker>(e) {
            for c in grid.footprint_cells(pos, b.0) {
                put(c, (1, '#'));
            }
        }
        put(grid.cell_of(pos), symbol(world, e));
    }
    let mut rows = Vec::new();
    for z in (z0.min(z1)..=z0.max(z1)).rev() {
        let mut row = String::new();
        for x in x0..=x1 {
            let c = CellCoord::new(x, z);
            let mark = marks.get(&c).copied().unwrap_or((0, '.'));
            row.push(if mark.0 == 0 && grid.is_blocking(c) {
                '#'
            } else {
                mark.1
            });
        }
        rows.push(format!("{z:>4} {row}"));
    }
    rows.join("\n")
}

fn symbol(world: &World, e: Entity) -> (u8, char) {
    if let Some(owner) = world.get::<Owner>(e)
        && world.get::<TestbedUnit>(e).is_some()
    {
        let digit = char::from_digit(owner.0.0 % 10, 10).unwrap_or('?');
        return (9, digit);
    }
    if let Some(d) = world.get::<Door>(e) {
        let c = if d.open {
            '/'
        } else if d.is_locked() {
            'L'
        } else {
            'D'
        };
        return (5, c);
    }
    if let Some(p) = world.get::<Prop>(e) {
        let c = match p.kind {
            PropKind::Wall => return (0, '.'),
            PropKind::Doorway => 'D',
            PropKind::Tree => 'T',
            PropKind::OreRock => 'O',
            PropKind::Workbench => 'W',
            PropKind::Anvil => 'A',
            PropKind::Forge => 'F',
            PropKind::Chest => 'C',
            PropKind::Cart => 'c',
            PropKind::Wagon => 'V',
        };
        return (7, c);
    }
    if let Some(item) = world.get::<Item>(e) {
        let c = match item.kind.as_str() {
            STORE_KEY => 'k',
            AXE => 'x',
            PICKAXE => 'p',
            HAMMER => 'h',
            BOOK => 'b',
            SCROLL => 's',
            BREAD => 'f',
            LOG => 'l',
            ORE => 'o',
            crate::content::INGOT => 'i',
            crate::content::PLANK => 'n',
            _ => '?',
        };
        return (3, c);
    }
    (0, '.')
}
