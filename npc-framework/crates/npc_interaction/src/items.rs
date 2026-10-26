//! Items, inventories and equipment.
//!
//! An [`Item`] is a sim entity. It is either **in the world** (it has a
//! `SimPos` and is indexed by the spatial grid) or **carried** (no `SimPos`;
//! [`CarriedBy`] names the holder, and the holder's [`Inventory`] or
//! [`Equipment`] lists it). The lists hold `SimId`s, kept sorted, so every
//! walk over a holder's items is in `SimId` order.
//!
//! Item kinds are registered by name ([`ItemKindDef`],
//! `InteractionAppExt::register_item_kind`): weight per unit in grams,
//! stacking, content tags (`"tool:axe"`, `"key:gate"`) and the equipment
//! slot the item goes into.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use npc_kernel::{SimId, SimIndex, SimPos};
use serde::{Deserialize, Serialize};

use crate::defs::InteractionRegistry;

/// A registered item kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemKindDef {
    /// Stable name (what [`Item::kind`] holds).
    pub name: String,
    /// Weight of one unit, in grams.
    pub weight_g: u32,
    /// Whether a picked-up item merges into a carried stack of the same kind
    /// and quality (up to `max_stack`).
    pub stackable: bool,
    pub max_stack: u32,
    /// Content tags, e.g. `"tool:axe"`, `"key:gate"` (sorted, unique).
    pub tags: Vec<String>,
    /// The equipment slot `equip` puts it in (`None`: not equippable).
    pub equip_slot: Option<String>,
}

impl ItemKindDef {
    /// A kind of weight 0, not stackable, without tags or slot.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            weight_g: 0,
            stackable: false,
            max_stack: 1,
            tags: Vec::new(),
            equip_slot: None,
        }
    }

    pub fn weight_g(mut self, grams: u32) -> Self {
        self.weight_g = grams;
        self
    }

    /// Stackable up to `max_stack` units per item.
    pub fn stackable(mut self, max_stack: u32) -> Self {
        self.stackable = true;
        self.max_stack = max_stack.max(1);
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        let tag = tag.into();
        if let Err(i) = self.tags.binary_search(&tag) {
            self.tags.insert(i, tag);
        }
        self
    }

    pub fn equip_slot(mut self, slot: impl Into<String>) -> Self {
        self.equip_slot = Some(slot.into());
        self
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.binary_search_by(|t| t.as_str().cmp(tag)).is_ok()
    }

    /// Whether this kind "is" `key`: its name or one of its tags.
    pub fn matches(&self, key: &str) -> bool {
        self.name == key || self.has_tag(key)
    }
}

/// An item: `stack` units of registered kind `kind`, of some `quality`
/// (content-defined, 0 = standard).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Item {
    pub kind: String,
    pub stack: u32,
    pub quality: i32,
}

impl Item {
    /// One unit of `kind`, quality 0.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            stack: 1,
            quality: 0,
        }
    }

    pub fn stack(mut self, stack: u32) -> Self {
        self.stack = stack.max(1);
        self
    }

    pub fn quality(mut self, quality: i32) -> Self {
        self.quality = quality;
        self
    }
}

/// What a unit can carry: at most `slots` items (stacks count once) and
/// `max_weight_g` grams in total (inventory plus equipment). `items` is kept
/// sorted by `SimId`; change it only through the interaction verbs (or
/// [`put_in_inventory`] / [`take_from_holder`]).
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Inventory {
    pub slots: u32,
    pub max_weight_g: u32,
    pub items: Vec<SimId>,
}

impl Inventory {
    pub fn new(slots: u32, max_weight_g: u32) -> Self {
        Self {
            slots,
            max_weight_g,
            items: Vec::new(),
        }
    }

    fn insert(&mut self, id: SimId) {
        if let Err(i) = self.items.binary_search(&id) {
            self.items.insert(i, id);
        }
    }

    fn remove(&mut self, id: SimId) -> bool {
        match self.items.binary_search(&id) {
            Ok(i) => {
                self.items.remove(i);
                true
            }
            Err(_) => false,
        }
    }

    /// Whether `id` is in the inventory (not counting equipment).
    pub fn contains(&self, id: SimId) -> bool {
        self.items.binary_search(&id).is_ok()
    }

    /// Free slots.
    pub fn free_slots(&self) -> u32 {
        self.slots
            .saturating_sub(u32::try_from(self.items.len()).unwrap_or(u32::MAX))
    }
}

/// Named equipment slots (`"main_hand"`, `"off_hand"`, ..), each empty or
/// holding one carried item. The slot set is the unit's: `equip` only uses
/// slots that exist here.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Equipment {
    pub slots: BTreeMap<String, Option<SimId>>,
}

impl Equipment {
    /// Empty slots with these names.
    pub fn new<S: Into<String>>(slots: impl IntoIterator<Item = S>) -> Self {
        Self {
            slots: slots.into_iter().map(|s| (s.into(), None)).collect(),
        }
    }

    /// The common two hands: `main_hand`, `off_hand`.
    pub fn hands() -> Self {
        Self::new([MAIN_HAND, OFF_HAND])
    }

    /// Equipped items, ascending.
    pub fn items(&self) -> Vec<SimId> {
        let mut v: Vec<SimId> = self.slots.values().filter_map(|s| *s).collect();
        v.sort_unstable();
        v
    }

    /// The slot holding `id`.
    pub fn slot_of(&self, id: SimId) -> Option<&str> {
        self.slots
            .iter()
            .find(|(_, v)| **v == Some(id))
            .map(|(k, _)| k.as_str())
    }
}

/// The usual main-hand slot name.
pub const MAIN_HAND: &str = "main_hand";
/// The usual off-hand slot name.
pub const OFF_HAND: &str = "off_hand";

/// On a carried item: who holds it, and the equipment slot it is in
/// (`None`: in the inventory).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct CarriedBy {
    pub holder: SimId,
    pub slot: Option<String>,
}

/// Marks a unit that can perform interactions (verbs, the `Interact` leaf
/// and command, doors on its path).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Interactor;

/// The entity of `id`, when it is in the sim.
pub fn entity_of(world: &World, id: SimId) -> Option<Entity> {
    world.get_resource::<SimIndex>()?.get(id)
}

/// The registered kind of `item`.
pub fn kind_def<'w>(world: &'w World, kind: &str) -> Option<&'w ItemKindDef> {
    world.get_resource::<InteractionRegistry>()?.kinds.get(kind)
}

/// Weight of an item (unit weight × stack, 0 for an unknown kind).
pub fn item_weight(world: &World, item: &Item) -> u32 {
    kind_def(world, &item.kind).map_or(0, |d| d.weight_g.saturating_mul(item.stack))
}

/// Every item `holder` carries (inventory and equipment), ascending.
pub fn carried_items(world: &World, holder: Entity) -> Vec<SimId> {
    let mut v: Vec<SimId> = world
        .get::<Inventory>(holder)
        .map(|i| i.items.clone())
        .unwrap_or_default();
    if let Some(eq) = world.get::<Equipment>(holder) {
        v.extend(eq.items());
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// Total weight `holder` carries, in grams.
pub fn carried_weight(world: &World, holder: Entity) -> u32 {
    carried_items(world, holder)
        .into_iter()
        .filter_map(|id| entity_of(world, id))
        .filter_map(|e| world.get::<Item>(e))
        .fold(0u32, |acc, item| {
            acc.saturating_add(item_weight(world, item))
        })
}

/// The first carried item (lowest `SimId`) whose kind matches `key` (its
/// name or a tag).
pub fn carried_matching(world: &World, holder: Entity, key: &str) -> Option<SimId> {
    carried_items(world, holder).into_iter().find(|&id| {
        entity_of(world, id)
            .and_then(|e| world.get::<Item>(e))
            .and_then(|item| kind_def(world, &item.kind))
            .is_some_and(|d| d.matches(key))
    })
}

/// The first equipped item (lowest `SimId`) whose kind matches `key`.
pub fn equipped_matching(world: &World, holder: Entity, key: &str) -> Option<SimId> {
    let eq = world.get::<Equipment>(holder)?;
    eq.items().into_iter().find(|&id| {
        entity_of(world, id)
            .and_then(|e| world.get::<Item>(e))
            .and_then(|item| kind_def(world, &item.kind))
            .is_some_and(|d| d.matches(key))
    })
}

/// The carried stack `item` would merge into when `holder` picks it up.
pub fn merge_target(world: &World, holder: Entity, item: &Item) -> Option<SimId> {
    let def = kind_def(world, &item.kind)?;
    if !def.stackable {
        return None;
    }
    let inv = world.get::<Inventory>(holder)?;
    inv.items.iter().copied().find(|&id| {
        entity_of(world, id)
            .and_then(|e| world.get::<Item>(e))
            .is_some_and(|other| {
                other.kind == item.kind
                    && other.quality == item.quality
                    && other.stack.saturating_add(item.stack) <= def.max_stack
            })
    })
}

/// Whether `holder`'s inventory can take `item` (entity `item_e`): a free
/// slot (or a stack to merge into) and the weight limit. `Err` says why.
pub fn can_store(world: &World, holder: Entity, item_e: Entity) -> Result<(), String> {
    let Some(inv) = world.get::<Inventory>(holder) else {
        return Err("has no inventory".into());
    };
    let Some(item) = world.get::<Item>(item_e) else {
        return Err("is not an item".into());
    };
    if kind_def(world, &item.kind).is_none() {
        return Err(format!("unknown item kind `{}`", item.kind));
    }
    let already = world
        .get::<CarriedBy>(item_e)
        .zip(world.get::<SimId>(holder))
        .is_some_and(|(c, &h)| c.holder == h);
    let weight = item_weight(world, item);
    let total = if already {
        carried_weight(world, holder)
    } else {
        carried_weight(world, holder).saturating_add(weight)
    };
    if total > inv.max_weight_g {
        return Err(format!(
            "too heavy: {} would carry {} of {} g",
            item.kind, total, inv.max_weight_g
        ));
    }
    if merge_target(world, holder, item).is_none() && inv.free_slots() == 0 {
        return Err(inventory_full(inv.slots));
    }
    Ok(())
}

/// The refusal of a full inventory: `"inventory is full (8 slots)"`.
pub fn inventory_full(slots: u32) -> String {
    format!("inventory is full ({slots} slots)")
}

/// The end of `equip`'s refusal when the slot is taken (`"main_hand holds
/// #17 (unequip it first)"`).
pub const UNEQUIP_FIRST: &str = "(unequip it first)";

/// How many items [`make_room_hint`] names at most.
pub const MAKE_ROOM_LIST: usize = 3;

/// `holder`'s inventory items (not equipped) lightest first (weight, then
/// `SimId`), at most `n`: dropping any one of them frees a slot.
pub fn lightest_items(world: &World, holder: Entity, n: usize) -> Vec<SimId> {
    let Some(inv) = world.get::<Inventory>(holder) else {
        return Vec::new();
    };
    let mut v: Vec<(u32, SimId)> = inv
        .items
        .iter()
        .filter_map(|&id| {
            let item = world.get::<Item>(entity_of(world, id)?)?;
            Some((item_weight(world, item), id))
        })
        .collect();
    v.sort_unstable();
    v.into_iter().take(n).map(|(_, id)| id).collect()
}

/// `"plank #66"`, `"2 × log #34"`.
fn item_name(world: &World, id: SimId) -> String {
    match entity_of(world, id).and_then(|e| world.get::<Item>(e)) {
        Some(i) if i.stack > 1 => format!("{} × {} #{}", i.stack, i.kind, id.0),
        Some(i) => format!("{} #{}", i.kind, id.0),
        None => format!("#{}", id.0),
    }
}

/// What to drop to free a slot in `holder`'s full inventory: `"drop 1 of
/// [plank #66, ingot #89] to make room"` (the [`lightest_items`], up to
/// [`MAKE_ROOM_LIST`]). `None` when a slot is free or nothing can be dropped.
pub fn make_room_hint(world: &World, holder: Entity) -> Option<String> {
    let inv = world.get::<Inventory>(holder)?;
    if inv.free_slots() > 0 {
        return None;
    }
    let names: Vec<String> = lightest_items(world, holder, MAKE_ROOM_LIST)
        .into_iter()
        .map(|id| item_name(world, id))
        .collect();
    (!names.is_empty()).then(|| format!("drop 1 of [{}] to make room", names.join(", ")))
}

/// A refusal `why` for `holder` with what to drop when it is about
/// `holder`'s full inventory: `"inventory is full (8 slots): drop 1 of
/// [plank #66, ingot #89] to make room"`, or `equip`'s `"main_hand holds
/// #17 (unequip it first; inventory is full (8 slots): drop 1 of [..] to
/// make room)"`. Other reasons come back unchanged. Text for people and
/// agents only (journal, command refusals, `why_not`): the verbs' own
/// reasons stay the bare rule.
pub fn with_room_hint(world: &World, holder: Entity, why: String) -> String {
    let Some(inv) = world.get::<Inventory>(holder) else {
        return why;
    };
    let full = inventory_full(inv.slots);
    let Some(hint) = make_room_hint(world, holder) else {
        return why;
    };
    if why == full {
        return format!("{why}: {hint}");
    }
    match why.strip_suffix(UNEQUIP_FIRST) {
        Some(head) => format!("{head}(unequip it first; {full}: {hint})"),
        None => why,
    }
}

/// What picking up `items` (not carried yet) would do to `holder`'s pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackFit {
    /// Carried now (inventory and equipment), in grams.
    pub load_g: u32,
    /// Carried after picking every item up.
    pub after_g: u32,
    pub max_g: u32,
    pub slots: u32,
    pub free_slots: u32,
    /// Slots the items need (an item that merges into a carried stack
    /// needs none).
    pub slots_needed: u32,
}

impl PackFit {
    pub fn too_heavy(&self) -> bool {
        self.after_g > self.max_g
    }

    pub fn no_slot(&self) -> bool {
        self.slots_needed > self.free_slots
    }

    /// Why the pack cannot take the items: `"pack would carry 30950 of
    /// 30000 g"`, `"inventory is full (8 slots)"`, `"needs 2 free slots
    /// (has 1)"`; `None` when they fit.
    pub fn refusal(&self) -> Option<String> {
        if self.too_heavy() {
            Some(format!(
                "pack would carry {} of {} g",
                self.after_g, self.max_g
            ))
        } else if self.no_slot() && self.free_slots == 0 {
            Some(inventory_full(self.slots))
        } else if self.no_slot() {
            Some(format!(
                "needs {} free slots (has {})",
                self.slots_needed, self.free_slots
            ))
        } else {
            None
        }
    }
}

/// What picking up `items` would do to `holder`'s pack (`None` without an
/// inventory). Read-only.
pub fn pack_fit(world: &World, holder: Entity, items: &[Item]) -> Option<PackFit> {
    let inv = world.get::<Inventory>(holder)?;
    let load_g = carried_weight(world, holder);
    let after_g = items
        .iter()
        .fold(load_g, |acc, i| acc.saturating_add(item_weight(world, i)));
    let slots_needed = items
        .iter()
        .filter(|i| merge_target(world, holder, i).is_none())
        .count();
    Some(PackFit {
        load_g,
        after_g,
        max_g: inv.max_weight_g,
        slots: inv.slots,
        free_slots: inv.free_slots(),
        slots_needed: u32::try_from(slots_needed).unwrap_or(u32::MAX),
    })
}

/// The holder of a carried item.
pub fn holder_of(world: &World, item_e: Entity) -> Option<SimId> {
    world.get::<CarriedBy>(item_e).map(|c| c.holder)
}

/// Where an entity is: its `SimPos`, or (for a carried item) its holder's.
pub fn position_of(world: &World, e: Entity) -> Option<SimPos> {
    if let Some(&p) = world.get::<SimPos>(e) {
        return Some(p);
    }
    let holder = holder_of(world, e)?;
    world.get::<SimPos>(entity_of(world, holder)?).copied()
}

/// Takes a carried item out of its holder's inventory or equipment (no-op
/// for an item in the world). Returns the holder and the slot it was in.
pub fn take_from_holder(world: &mut World, item_e: Entity) -> Option<(SimId, Option<String>)> {
    let carried = world.get::<CarriedBy>(item_e)?.clone();
    let id = *world.get::<SimId>(item_e)?;
    if let Some(holder_e) = entity_of(world, carried.holder) {
        if let Some(mut inv) = world.get_mut::<Inventory>(holder_e) {
            inv.remove(id);
        }
        if let Some(mut eq) = world.get_mut::<Equipment>(holder_e) {
            for v in eq.slots.values_mut() {
                if *v == Some(id) {
                    *v = None;
                }
            }
        }
    }
    world.entity_mut(item_e).remove::<CarriedBy>();
    Some((carried.holder, carried.slot))
}

/// Puts `item_e` into `holder_e`'s inventory (taking it from the world or
/// another holder first). Capacity is the caller's check ([`can_store`]).
pub fn put_in_inventory(world: &mut World, holder_e: Entity, item_e: Entity) {
    let (Some(&holder), Some(&id)) = (world.get::<SimId>(holder_e), world.get::<SimId>(item_e))
    else {
        return;
    };
    take_from_holder(world, item_e);
    world
        .entity_mut(item_e)
        .remove::<(SimPos, npc_kernel::PrevSimPos)>()
        .insert(CarriedBy { holder, slot: None });
    if let Some(mut inv) = world.get_mut::<Inventory>(holder_e) {
        inv.insert(id);
    }
}

/// Moves a carried item from the inventory into equipment slot `slot`.
pub fn put_in_slot(world: &mut World, holder_e: Entity, item_e: Entity, slot: &str) {
    let (Some(&holder), Some(&id)) = (world.get::<SimId>(holder_e), world.get::<SimId>(item_e))
    else {
        return;
    };
    take_from_holder(world, item_e);
    world.entity_mut(item_e).insert(CarriedBy {
        holder,
        slot: Some(slot.to_owned()),
    });
    if let Some(mut eq) = world.get_mut::<Equipment>(holder_e) {
        eq.slots.insert(slot.to_owned(), Some(id));
    }
}

/// Puts `item_e` on the ground at `pos` (taking it from its holder first).
pub fn place_in_world(world: &mut World, item_e: Entity, pos: SimPos) {
    take_from_holder(world, item_e);
    world
        .entity_mut(item_e)
        .insert((pos, npc_kernel::PrevSimPos(pos)));
}

/// `SimId` leave hook (`npc_interaction.items`, final review bugs 2 and 3),
/// called with the entity that leaves while its components are still
/// there:
///
/// - **A holder leaves:** everything it carries (inventory and equipment)
///   drops where it stood: its `SimPos` (else its own holder's position,
///   else its last indexed grid position), in `SimId` order, each journaled
///   as `ItemMoved { from: holder, to: None }`. The items get a `SimPos`,
///   so the spatial update puts them back in the grid. (Applied through
///   `commands`, at the despawn's flush.)
/// - **A carried item leaves:** its id is taken out of its holder's
///   inventory and equipment at once, so no ghost blocks a slot.
pub fn on_sim_id_leave(world: &mut DeferredWorld, id: SimId, entity: Entity) {
    let carried = carried_items(world, entity);
    if !carried.is_empty() {
        let pos = world
            .get::<SimPos>(entity)
            .copied()
            .or_else(|| position_of(world, entity))
            .or_else(|| {
                world
                    .get_resource::<npc_kernel::spatial::SpatialGrid>()
                    .and_then(|g| g.position_of(id))
            });
        world
            .commands()
            .queue(move |w: &mut World| drop_orphans(w, id, &carried, pos));
    }
    if let Some(holder) = world.get::<CarriedBy>(entity).map(|c| c.holder)
        && let Some(holder_e) = world.get_resource::<SimIndex>().and_then(|i| i.get(holder))
    {
        if let Some(mut inv) = world.get_mut::<Inventory>(holder_e) {
            inv.remove(id);
        }
        if let Some(mut eq) = world.get_mut::<Equipment>(holder_e) {
            for v in eq.slots.values_mut() {
                if *v == Some(id) {
                    *v = None;
                }
            }
        }
    }
}

/// Puts the items `holder` (gone) carried on the ground at `pos`, in
/// `SimId` order (see [`on_sim_id_leave`]). Items that left too, or that
/// another holder took meanwhile, are skipped.
fn drop_orphans(world: &mut World, holder: SimId, items: &[SimId], pos: Option<SimPos>) {
    for &item in items {
        let Some(e) = entity_of(world, item) else {
            continue;
        };
        if world.get::<CarriedBy>(e).is_none_or(|c| c.holder != holder) {
            continue;
        }
        let mut ent = world.entity_mut(e);
        ent.remove::<CarriedBy>();
        if let Some(pos) = pos {
            ent.insert((pos, npc_kernel::PrevSimPos(pos)));
        }
        let Some(it) = world.get::<Item>(e).cloned() else {
            continue;
        };
        npc_kernel::journal::record(world, Some(holder), || {
            npc_kernel::journal::JournalKind::ItemMoved {
                item,
                kind: it.kind,
                stack: it.stack,
                from: Some(holder),
                to: None,
                slot: None,
            }
        });
    }
}
