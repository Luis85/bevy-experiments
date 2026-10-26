//! The built-in verbs (spec §4.3): `pick_up`, `drop`, `give`, `open`,
//! `close`, `lock`, `unlock`, `equip`, `unequip`, `use_tool_on`.
//!
//! `mount` and `dismount` are **reserved** for `npc_vehicles`, which
//! registers them ([`MOUNT`], [`DISMOUNT`]); this crate does not.
//!
//! Durations are defaults; content can replace a definition before the
//! freeze (`InteractionAppExt::override_interaction`).

use bevy_ecs::prelude::*;
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::spatial::{CellCoord, SpatialGrid};
use npc_kernel::{SimId, SimPos, spawn_sim};

use crate::defs::{Ctx, InteractionDef, ItemArg, SkillCheck};
use crate::door::{Door, sync_door};
use crate::items::{
    CarriedBy, Equipment, Inventory, Item, UNEQUIP_FIRST, can_store, carried_items,
    carried_matching, equipped_matching, inventory_full, kind_def, merge_target, place_in_world,
    put_in_inventory, put_in_slot,
};
use crate::targets::ToolTarget;

pub const PICK_UP: &str = "pick_up";
pub const DROP: &str = "drop";
pub const GIVE: &str = "give";
pub const OPEN: &str = "open";
pub const CLOSE: &str = "close";
pub const LOCK: &str = "lock";
pub const UNLOCK: &str = "unlock";
pub const EQUIP: &str = "equip";
pub const UNEQUIP: &str = "unequip";
pub const USE_TOOL_ON: &str = "use_tool_on";
/// Reserved for `npc_vehicles` (not registered here).
pub const MOUNT: &str = "mount";
/// Reserved for `npc_vehicles` (not registered here).
pub const DISMOUNT: &str = "dismount";
/// Verb names reserved for other capability crates.
pub const RESERVED_VERBS: [&str; 2] = [MOUNT, DISMOUNT];

/// Noise kind bit of door noises (high bits are reserved for interaction
/// defaults; content noise kinds use the low bits).
pub const NOISE_DOOR: u32 = 1 << 30;
/// Noise kind bit of tool work.
pub const NOISE_TOOL: u32 = 1 << 31;

// ---------------------------------------------------------------- helpers

fn carried_by(world: &World, item: Entity, holder: SimId) -> Option<&CarriedBy> {
    world.get::<CarriedBy>(item).filter(|c| c.holder == holder)
}

fn record_moved(
    world: &mut World,
    actor: SimId,
    item_e: Entity,
    from: Option<SimId>,
    to: Option<SimId>,
    slot: Option<String>,
) {
    let (Some(&id), Some(item)) = (
        world.get::<SimId>(item_e),
        world.get::<Item>(item_e).cloned(),
    ) else {
        return;
    };
    journal::record(world, Some(actor), || JournalKind::ItemMoved {
        item: id,
        kind: item.kind,
        stack: item.stack,
        from,
        to,
        slot,
    });
}

fn record_door(world: &mut World, actor: SimId, door_e: Entity) {
    let (Some(&id), Some(d)) = (
        world.get::<SimId>(door_e),
        world.get::<Door>(door_e).cloned(),
    ) else {
        return;
    };
    journal::record(world, Some(actor), || JournalKind::DoorChanged {
        door: id,
        open: d.open,
        locked: d.is_locked(),
    });
}

fn is_item_in_world(world: &World, ctx: &Ctx) -> bool {
    world.get::<Item>(ctx.target).is_some()
        && world.get::<SimPos>(ctx.target).is_some()
        && world.get::<CarriedBy>(ctx.target).is_none()
}

fn is_carried(world: &World, ctx: &Ctx) -> bool {
    world.get::<Item>(ctx.target).is_some() && carried_by(world, ctx.target, ctx.actor_id).is_some()
}

fn has_door(world: &World, ctx: &Ctx) -> bool {
    world.get::<Door>(ctx.target).is_some()
}

fn door(world: &World, ctx: &Ctx) -> Result<Door, String> {
    world
        .get::<Door>(ctx.target)
        .cloned()
        .ok_or_else(|| "is not a door".into())
}

fn mutate_door(world: &mut World, ctx: &Ctx, f: impl FnOnce(&mut Door)) {
    if let Some(mut d) = world.get_mut::<Door>(ctx.target) {
        f(&mut d);
    }
    sync_door(world, ctx.target);
    record_door(world, ctx.actor_id, ctx.target);
}

/// The key `ctx.actor` uses for `key`: the explicit item if it matches,
/// else the first carried match.
fn key_for(world: &World, ctx: &Ctx, key: &str) -> Option<SimId> {
    if let Some((id, e)) = ctx.item {
        let fits = world
            .get::<Item>(e)
            .and_then(|i| kind_def(world, &i.kind))
            .is_some_and(|d| d.matches(key))
            && carried_by(world, e, ctx.actor_id).is_some();
        return fits.then_some(id);
    }
    carried_matching(world, ctx.actor, key)
}

// ---------------------------------------------------------------- items

fn pick_up_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    can_store(world, ctx.actor, ctx.target)
}

fn pick_up_effect(world: &mut World, ctx: &Ctx) {
    let Some(item) = world.get::<Item>(ctx.target).cloned() else {
        return;
    };
    if let Some(into) = merge_target(world, ctx.actor, &item)
        && let Some(into_e) = crate::items::entity_of(world, into)
    {
        record_moved(
            world,
            ctx.actor_id,
            ctx.target,
            None,
            Some(ctx.actor_id),
            None,
        );
        if let Some(mut stack) = world.get_mut::<Item>(into_e) {
            stack.stack = stack.stack.saturating_add(item.stack);
        }
        world.despawn(ctx.target);
        return;
    }
    put_in_inventory(world, ctx.actor, ctx.target);
    record_moved(
        world,
        ctx.actor_id,
        ctx.target,
        None,
        Some(ctx.actor_id),
        None,
    );
}

fn drop_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    if world.get::<SimPos>(ctx.actor).is_none() {
        return Err("has no position to drop at".into());
    }
    Ok(())
}

fn drop_effect(world: &mut World, ctx: &Ctx) {
    let Some(&pos) = world.get::<SimPos>(ctx.actor) else {
        return;
    };
    place_in_world(world, ctx.target, pos);
    record_moved(
        world,
        ctx.actor_id,
        ctx.target,
        Some(ctx.actor_id),
        None,
        None,
    );
}

fn give_offer(world: &World, ctx: &Ctx) -> bool {
    ctx.target != ctx.actor && world.get::<Inventory>(ctx.target).is_some()
}

fn give_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    match ctx.item {
        Some((id, e)) => {
            if carried_by(world, e, ctx.actor_id).is_none() {
                return Err(format!("does not carry item #{}", id.0));
            }
            can_store(world, ctx.target, e).map_err(|why| format!("#{} {why}", ctx.target_id.0))
        }
        None if carried_items(world, ctx.actor).is_empty() => Err("has nothing to give".into()),
        None => Ok(()),
    }
}

fn give_effect(world: &mut World, ctx: &Ctx) {
    let Some((_, item_e)) = ctx.item else {
        return;
    };
    put_in_inventory(world, ctx.target, item_e);
    record_moved(
        world,
        ctx.actor_id,
        item_e,
        Some(ctx.actor_id),
        Some(ctx.target_id),
        None,
    );
}

fn equip_offer(world: &World, ctx: &Ctx) -> bool {
    carried_by(world, ctx.target, ctx.actor_id).is_some_and(|c| c.slot.is_none())
        && world
            .get::<Item>(ctx.target)
            .and_then(|i| kind_def(world, &i.kind))
            .is_some_and(|d| d.equip_slot.is_some())
}

fn equip_slot_of(world: &World, ctx: &Ctx) -> Option<String> {
    world
        .get::<Item>(ctx.target)
        .and_then(|i| kind_def(world, &i.kind))
        .and_then(|d| d.equip_slot.clone())
}

fn equip_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let slot = equip_slot_of(world, ctx).ok_or("cannot be equipped")?;
    let eq = world
        .get::<Equipment>(ctx.actor)
        .ok_or("has no equipment slots")?;
    match eq.slots.get(&slot) {
        None => Err(format!("has no {slot} slot")),
        Some(Some(other)) => Err(format!("{slot} holds #{} {UNEQUIP_FIRST}", other.0)),
        Some(None) => Ok(()),
    }
}

fn equip_effect(world: &mut World, ctx: &Ctx) {
    let Some(slot) = equip_slot_of(world, ctx) else {
        return;
    };
    put_in_slot(world, ctx.actor, ctx.target, &slot);
    record_moved(
        world,
        ctx.actor_id,
        ctx.target,
        Some(ctx.actor_id),
        Some(ctx.actor_id),
        Some(slot),
    );
}

fn unequip_offer(world: &World, ctx: &Ctx) -> bool {
    carried_by(world, ctx.target, ctx.actor_id).is_some_and(|c| c.slot.is_some())
}

fn unequip_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let inv = world
        .get::<Inventory>(ctx.actor)
        .ok_or("has no inventory")?;
    if inv.free_slots() == 0 {
        return Err(inventory_full(inv.slots));
    }
    Ok(())
}

fn unequip_effect(world: &mut World, ctx: &Ctx) {
    put_in_inventory(world, ctx.actor, ctx.target);
    record_moved(
        world,
        ctx.actor_id,
        ctx.target,
        Some(ctx.actor_id),
        Some(ctx.actor_id),
        None,
    );
}

// ---------------------------------------------------------------- doors

fn open_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let d = door(world, ctx)?;
    if d.open {
        return Err("door is already open".into());
    }
    if d.is_locked() {
        return Err("door is locked".into());
    }
    Ok(())
}

fn open_effect(world: &mut World, ctx: &Ctx) {
    mutate_door(world, ctx, |d| d.open = true);
}

fn close_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let d = door(world, ctx)?;
    if !d.open {
        return Err("door is already closed".into());
    }
    let (Some(grid), Some(&pos)) = (
        world.get_resource::<SpatialGrid>(),
        world.get::<SimPos>(ctx.target),
    ) else {
        return Ok(());
    };
    if let Some(other) = in_doorway(world, grid, grid.cell_of(pos), ctx.target_id) {
        return Err(format!("the doorway is not clear (#{})", other.0));
    }
    Ok(())
}

/// The lowest `SimId` other than `door` whose **live** `SimPos` lies in
/// `cell`. Not the grid index: that holds the positions of the start of the
/// tick, and `close` completes after the movers, so a unit that walked into
/// the doorway this tick would be shut in (final review bug 4).
fn in_doorway(world: &World, grid: &SpatialGrid, cell: CellCoord, door: SimId) -> Option<SimId> {
    let mut q = world.try_query::<(&SimId, &SimPos)>()?;
    q.iter(world)
        .filter(|&(&id, &p)| id != door && grid.cell_of(p) == cell)
        .map(|(&id, _)| id)
        .min()
}

fn close_effect(world: &mut World, ctx: &Ctx) {
    mutate_door(world, ctx, |d| d.open = false);
}

fn lock_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let d = door(world, ctx)?;
    let key = d.key.clone().ok_or("door has no lock")?;
    if d.is_locked() {
        return Err("door is already locked".into());
    }
    if d.open {
        return Err("close the door first".into());
    }
    key_for(world, ctx, &key)
        .map(|_| ())
        .ok_or_else(|| format!("needs {key}"))
}

fn lock_effect(world: &mut World, ctx: &Ctx) {
    mutate_door(world, ctx, |d| d.locked_by = d.key.clone());
}

fn unlock_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let d = door(world, ctx)?;
    let Some(key) = d.locked_by else {
        return Err("door is not locked".into());
    };
    key_for(world, ctx, &key)
        .map(|_| ())
        .ok_or_else(|| format!("door is locked: needs {key}"))
}

fn unlock_effect(world: &mut World, ctx: &Ctx) {
    mutate_door(world, ctx, |d| d.locked_by = None);
}

fn door_key(world: &World, ctx: &Ctx) -> Vec<String> {
    world
        .get::<Door>(ctx.target)
        .and_then(|d| d.locked_by.clone().or_else(|| d.key.clone()))
        .into_iter()
        .collect()
}

// ---------------------------------------------------------------- tools

fn has_tool_target(world: &World, ctx: &Ctx) -> bool {
    world.get::<ToolTarget>(ctx.target).is_some()
}

fn tool_target(world: &World, ctx: &Ctx) -> Result<ToolTarget, String> {
    world
        .get::<ToolTarget>(ctx.target)
        .cloned()
        .ok_or_else(|| "cannot be worked with a tool".into())
}

/// The equipped tool `ctx.actor` uses on the target.
pub fn tool_for(world: &World, ctx: &Ctx) -> Result<SimId, String> {
    let t = tool_target(world, ctx)?;
    let tag = &t.required_tag;
    if let Some((id, e)) = ctx.item {
        let ok = world
            .get::<CarriedBy>(e)
            .is_some_and(|c| c.holder == ctx.actor_id && c.slot.is_some())
            && world
                .get::<Item>(e)
                .and_then(|i| kind_def(world, &i.kind))
                .is_some_and(|d| d.matches(tag));
        return if ok {
            Ok(id)
        } else {
            Err(format!("item #{} is not an equipped {tag}", id.0))
        };
    }
    if let Some(id) = equipped_matching(world, ctx.actor, tag) {
        return Ok(id);
    }
    if carried_matching(world, ctx.actor, tag).is_some() {
        Err(format!("equip {tag} first"))
    } else {
        Err(format!("needs {tag}"))
    }
}

fn use_tool_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let t = tool_target(world, ctx)?;
    if t.depleted() {
        return Err("is used up".into());
    }
    tool_for(world, ctx).map(|_| ())
}

fn use_tool_duration(world: &World, ctx: &Ctx) -> u32 {
    world
        .get::<ToolTarget>(ctx.target)
        .map_or(0, |t| t.duration_ticks)
}

fn use_tool_check(world: &World, ctx: &Ctx) -> Option<SkillCheck> {
    world.get::<ToolTarget>(ctx.target)?.check.clone()
}

fn use_tool_requires(world: &World, ctx: &Ctx) -> Vec<String> {
    world
        .get::<ToolTarget>(ctx.target)
        .map(|t| vec![t.required_tag.clone()])
        .unwrap_or_default()
}

fn use_tool_effect(world: &mut World, ctx: &Ctx) {
    let Some(t) = world.get::<ToolTarget>(ctx.target).cloned() else {
        return;
    };
    let pos = world
        .get::<SimPos>(ctx.actor)
        .or_else(|| world.get::<SimPos>(ctx.target))
        .copied()
        .unwrap_or_default();
    for y in &t.yields {
        spawn_sim(
            world,
            (
                Item {
                    kind: y.kind.clone(),
                    stack: y.stack,
                    quality: y.quality,
                },
                pos,
            ),
        );
    }
    if let Some(mut target) = world.get_mut::<ToolTarget>(ctx.target)
        && let Some(n) = &mut target.remaining
    {
        *n = n.saturating_sub(1);
    }
}

/// Every built-in verb (what `InteractionPlugin` registers).
pub fn builtin_verbs() -> Vec<InteractionDef> {
    vec![
        InteractionDef::new(PICK_UP, pick_up_pre, pick_up_effect)
            .offer(is_item_in_world)
            .duration(10),
        InteractionDef::new(DROP, drop_pre, drop_effect)
            .offer(is_carried)
            .duration(5)
            .no_reach()
            .no_reservation(),
        InteractionDef::new(GIVE, give_pre, give_effect)
            .offer(give_offer)
            .duration(10)
            .item(ItemArg::Required)
            .no_reservation(),
        InteractionDef::new(OPEN, open_pre, open_effect)
            .offer(has_door)
            .duration(10)
            .noise(6000, NOISE_DOOR),
        InteractionDef::new(CLOSE, close_pre, close_effect)
            .offer(has_door)
            .duration(10)
            .noise(6000, NOISE_DOOR),
        InteractionDef::new(LOCK, lock_pre, lock_effect)
            .offer(has_door)
            .duration(20)
            .item(ItemArg::Optional)
            .requires(door_key),
        InteractionDef::new(UNLOCK, unlock_pre, unlock_effect)
            .offer(has_door)
            .duration(20)
            .item(ItemArg::Optional)
            .requires(door_key),
        InteractionDef::new(EQUIP, equip_pre, equip_effect)
            .offer(equip_offer)
            .duration(10)
            .no_reach()
            .no_reservation(),
        InteractionDef::new(UNEQUIP, unequip_pre, unequip_effect)
            .offer(unequip_offer)
            .duration(10)
            .no_reach()
            .no_reservation(),
        InteractionDef::new(USE_TOOL_ON, use_tool_pre, use_tool_effect)
            .offer(has_tool_target)
            .duration_fn(use_tool_duration)
            .check_fn(use_tool_check)
            .item(ItemArg::Optional)
            .requires(use_tool_requires)
            .noise(10_000, NOISE_TOOL),
    ]
}
