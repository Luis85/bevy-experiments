//! Spawning units and items for the island's scenario loader
//! ([`crate::scenario`]).
//!
//! 1 m grid cells; a cell `(x, z)` has its centre at `(x·1000 + 500,
//! z·1000 + 500)` mm (+z is north). [`ascii_map`] draws a region.

use bevy_ecs::prelude::*;
use npc_framework::interaction::items::{kind_def, put_in_inventory, put_in_slot};
use npc_framework::kernel::spatial::CellCoord;
use npc_framework::prelude::*;
use npc_framework::rules::template_sheet;
use serde::Deserialize;

use crate::content::{AXE, BOOK, BREAD, HAMMER, LOG, ORE, PICKAXE, SCROLL, STORE_KEY};
use crate::world::{Behaviour, Prop, PropKind, TestbedUnit, kinds};

/// Grid size in cells (centred on the origin).
pub const GRID_CELLS: u32 = 64;

/// Actor walking speed, mm per tick (1.4 m/s at 20 Hz).
pub const UNIT_SPEED: i32 = 70;
pub const DEFAULT_WANDER_RADIUS_MM: i32 = 3000;
/// Vision range / half angle, hearing radius of every actor.
pub const VISION_RANGE_MM: i32 = 15_000;
pub const VISION_HALF_FOV_DEG: u16 = 60;
pub const HEARING_MM: i32 = 10_000;
pub const INVENTORY_SLOTS: u32 = 8;
pub const INVENTORY_MAX_G: u32 = 30_000;

/// The centre of cell `(x, z)`.
pub const fn cell(x: i32, z: i32) -> SimPos {
    SimPos::new(x * 1000 + 500, z * 1000 + 500)
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
    if !crate::world_runtime::enabled(world, |s| s.progression) {
        world
            .entity_mut(e)
            .remove::<Experience>()
            .remove::<SpendPolicy>();
    }
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
