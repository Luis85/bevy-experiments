//! Running a craft: the one path every entry point (the `Craft` leaf, the
//! `Craft` command, order picking) uses.
//!
//! [`begin_craft`] checks ([`can_craft`]), claims the workstation (with
//! capacity 1) and every input and tool item, inserts [`Crafting`] (with the
//! planned tools and `(input, units)`) and journals `CraftStarted` (a
//! refusal: `CraftFailed { outcome: "rejected" }`).
//! [`crafting_complete_system`] finishes every craft whose duration is
//! over, in `SimId` order: it checks again ([`check_claimed`]: the same
//! recipe, workstation and skill checks, and the **claimed** tools and
//! inputs still carried and sufficient; `invalid` if the world changed),
//! rolls the skill (`npc_rules::roll_skill`, purpose `craft.<recipe>`,
//! journaled `Roll`), applies the result, wears the tools, releases the
//! claims, journals `CraftDone`/`CraftFailed` and calls the completion
//! hooks with a [`CraftCompleted`]. [`cancel_craft`] ends one early.

use std::fmt;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{ActiveLeaf, Status};
use npc_interaction::items::{
    CarriedBy, Equipment, Interactor, Inventory, Item, can_store, entity_of, kind_def,
    place_in_world, position_of, put_in_inventory, take_from_holder,
};
use npc_interaction::{Interacting, run::metres};
use npc_jobs::{Reservations, free_for};
use npc_kernel::journal::{self, JournalKind, ReleaseReason};
use npc_kernel::{SimClock, SimId, SimPos, spawn_sim};
use npc_rules::{CharacterSheet, Critical, effective_skill, roll_skill};
use serde::{Deserialize, Serialize};

use crate::defs::{
    CraftCompleted, CraftResult, CraftingConfig, CraftingHooks, RecipeDef, RecipeRegistry,
};
use crate::leaf::CraftLeaf;
use crate::station::{Durability, Workstation, count_down};

/// Who started a craft.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum CraftSource {
    /// The `Craft` leaf.
    #[default]
    Tree,
    /// The `Craft` command.
    Command,
}

/// A running craft on its crafter.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Crafting {
    pub recipe: String,
    pub workstation: Option<SimId>,
    /// Claimed tool and input items (ascending).
    pub claimed: Vec<SimId>,
    pub started: u64,
    pub ends: u64,
    pub source: CraftSource,
    /// The workstation order it works on.
    pub order: Option<u32>,
    /// The planned tools, one per recipe tool key (in the recipe's order):
    /// what completion wears.
    #[serde(default)]
    pub tools: Vec<SimId>,
    /// The planned `(input item, units taken)`, in plan order: exactly what
    /// completion consumes.
    #[serde(default)]
    pub inputs: Vec<(SimId, u32)>,
}

/// Why a craft cannot start (or became invalid). `Display` is a short
/// human-readable reason (an affordance's `why_not`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CraftError {
    UnknownRecipe {
        recipe: String,
        known: String,
    },
    /// The actor is not a sim entity with `Interactor` and `Inventory`.
    NotACrafter {
        unit: u32,
    },
    Busy {
        with: String,
    },
    NeedsWorkstation {
        needs: Vec<String>,
    },
    NoWorkstationNeeded,
    WorkstationGone {
        workstation: u32,
    },
    NotAWorkstation {
        target: u32,
    },
    WrongWorkstation {
        workstation: u32,
        needs: Vec<String>,
    },
    TooFar {
        distance_mm: i64,
        reach_mm: i32,
    },
    WorkstationInUse {
        workstation: u32,
    },
    MissingTool {
        tool: String,
    },
    MissingInput {
        input: String,
        need: u32,
        have: u32,
    },
    /// Some inputs or tools are missing ([`can_craft`]): every recipe input
    /// and tool with what the crafter has, the missing ones first (each
    /// group in recipe order, inputs before tools).
    Missing {
        needs: Vec<CraftNeed>,
    },
    /// The crafter has a character sheet but no level in the skill.
    NoSkill {
        skill: String,
    },
    InvalidCount,
    NoOrders {
        workstation: u32,
    },
    /// The workstation already has `max` open orders.
    TooManyOrders {
        workstation: u32,
        max: u32,
    },
}

/// One recipe input or tool and what the crafter has of it
/// ([`CraftError::Missing`]): `have` counts units (inputs) or items (tools)
/// it could use.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CraftNeed {
    /// The recipe key (kind name or tag, e.g. `"material:wood"`,
    /// `"tool:hammer"`).
    pub key: String,
    pub tool: bool,
    pub need: u32,
    pub have: u32,
}

impl CraftNeed {
    pub fn missing(&self) -> bool {
        self.have < self.need
    }
}

/// A tool key as text: `"tool:hammer"` is `"hammer"` (the `tool:` tag
/// prefix dropped), any other key as it is.
pub fn tool_name(key: &str) -> &str {
    key.strip_prefix("tool:").unwrap_or(key)
}

impl fmt::Display for CraftNeed {
    /// `"1 plank (has 0)"`, `"tool hammer (has 1)"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tool {
            write!(f, "tool {} (has {})", tool_name(&self.key), self.have)
        } else {
            write!(f, "{} {} (has {})", self.need, self.key, self.have)
        }
    }
}

impl CraftError {
    pub fn unknown_recipe(world: &World, recipe: &str) -> Self {
        CraftError::UnknownRecipe {
            recipe: recipe.to_owned(),
            known: world
                .get_resource::<RecipeRegistry>()
                .map(RecipeRegistry::names)
                .unwrap_or_default(),
        }
    }
}

impl fmt::Display for CraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CraftError::UnknownRecipe { recipe, known } => {
                write!(f, "unknown recipe `{recipe}` (known: {known})")
            }
            CraftError::NotACrafter { unit } => write!(f, "#{unit} cannot craft"),
            CraftError::Busy { with } => write!(f, "busy with {with}"),
            CraftError::NeedsWorkstation { needs } => {
                write!(f, "needs a workstation: {}", needs.join(" or "))
            }
            CraftError::NoWorkstationNeeded => write!(f, "needs no workstation"),
            CraftError::WorkstationGone { workstation } => write!(f, "#{workstation} is gone"),
            CraftError::NotAWorkstation { target } => write!(f, "#{target} is not a workstation"),
            CraftError::WrongWorkstation { workstation, needs } => {
                write!(f, "#{workstation} is not a {}", needs.join(" or "))
            }
            CraftError::TooFar {
                distance_mm,
                reach_mm,
            } => write!(
                f,
                "too far: {} (reach {})",
                metres(*distance_mm),
                metres(i64::from(*reach_mm))
            ),
            CraftError::WorkstationInUse { workstation } => {
                write!(f, "#{workstation} is in use")
            }
            CraftError::MissingTool { tool } => write!(f, "needs {tool}"),
            CraftError::MissingInput { input, need, have } => {
                write!(f, "needs {need} {input} (has {have})")
            }
            CraftError::Missing { needs } => {
                let parts: Vec<String> = needs.iter().map(ToString::to_string).collect();
                write!(f, "needs {}", parts.join(", "))
            }
            CraftError::NoSkill { skill } => write!(f, "does not know {skill}"),
            CraftError::InvalidCount => write!(f, "count must be > 0"),
            CraftError::NoOrders { workstation } => {
                write!(f, "#{workstation} has no orders")
            }
            CraftError::TooManyOrders { workstation, max } => {
                write!(f, "#{workstation} already has {max} open orders")
            }
        }
    }
}

impl std::error::Error for CraftError {}

/// What a craft would use: the checked recipe, workstation, tools and
/// `(input item, units taken)`, all in `SimId` order per key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftPlan {
    pub recipe: RecipeDef,
    pub workstation: Option<SimId>,
    pub tools: Vec<SimId>,
    pub inputs: Vec<(SimId, u32)>,
}

impl CraftPlan {
    /// Every claimed item (tools and inputs), ascending.
    pub fn claimed(&self) -> Vec<SimId> {
        let mut v: Vec<SimId> = self
            .tools
            .iter()
            .copied()
            .chain(self.inputs.iter().map(|(id, _)| *id))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

fn config(world: &World) -> CraftingConfig {
    world
        .get_resource::<CraftingConfig>()
        .copied()
        .unwrap_or_default()
}

fn item_matches(world: &World, id: SimId, key: &str) -> Option<u32> {
    let e = entity_of(world, id)?;
    let item = world.get::<Item>(e)?;
    kind_def(world, &item.kind)
        .is_some_and(|d| d.matches(key))
        .then_some(item.stack)
}

fn free_item(world: &World, id: SimId, me: SimId) -> bool {
    world
        .get_resource::<Reservations>()
        .is_none_or(|r| free_for(r, id, me))
}

/// The recipe, crafter id and inventory for `actor` doing `recipe`, with
/// the workstation checks (`Err` is why not).
fn check_recipe_and_station<'w>(
    world: &'w World,
    actor: Entity,
    recipe: &str,
    workstation: Option<SimId>,
) -> Result<(RecipeDef, SimId, &'w Inventory), CraftError> {
    let def = world
        .get_resource::<RecipeRegistry>()
        .and_then(|r| r.get(recipe))
        .cloned()
        .ok_or_else(|| CraftError::unknown_recipe(world, recipe))?;
    let me = world.get::<SimId>(actor).copied();
    let (Some(me), Some(inv), Some(_)) = (
        me,
        world.get::<Inventory>(actor),
        world.get::<Interactor>(actor),
    ) else {
        return Err(CraftError::NotACrafter {
            unit: me.map_or(u32::MAX, |m| m.0),
        });
    };
    // Workstation.
    match (def.stations.is_empty(), workstation) {
        (true, Some(_)) => return Err(CraftError::NoWorkstationNeeded),
        (false, None) => {
            return Err(CraftError::NeedsWorkstation {
                needs: def.stations.clone(),
            });
        }
        (false, Some(ws)) => {
            let e =
                entity_of(world, ws).ok_or(CraftError::WorkstationGone { workstation: ws.0 })?;
            let station = world
                .get::<Workstation>(e)
                .ok_or(CraftError::NotAWorkstation { target: ws.0 })?;
            if !def.hosted_by(&station.kinds) {
                return Err(CraftError::WrongWorkstation {
                    workstation: ws.0,
                    needs: def.stations.clone(),
                });
            }
            let reach = config(world).reach_mm;
            let d2 = match (world.get::<SimPos>(actor), position_of(world, e)) {
                (Some(a), Some(b)) => a.dist2(b),
                _ => i64::MAX,
            };
            if d2 > i64::from(reach) * i64::from(reach) {
                return Err(CraftError::TooFar {
                    distance_mm: if d2 == i64::MAX { -1 } else { d2.isqrt() },
                    reach_mm: reach,
                });
            }
            if !free_item(world, ws, me) {
                return Err(CraftError::WorkstationInUse { workstation: ws.0 });
            }
        }
        (true, None) => {}
    }
    Ok((def, me, inv))
}

/// A sheet without a level in the recipe's skill cannot attempt it.
fn check_skill(world: &World, actor: Entity, me: SimId, def: &RecipeDef) -> Result<(), CraftError> {
    if let Some(skill) = &def.skill
        && world.get::<CharacterSheet>(actor).is_some()
        && effective_skill(world, me, skill).is_none()
    {
        return Err(CraftError::NoSkill {
            skill: skill.clone(),
        });
    }
    Ok(())
}

/// What `actor` doing `recipe` (at `workstation`) would use now, ignoring
/// whether it is busy; `Err` is why it cannot (see [`CraftError`]).
pub fn can_craft(
    world: &World,
    actor: Entity,
    recipe: &str,
    workstation: Option<SimId>,
) -> Result<CraftPlan, CraftError> {
    let (def, me, inv) = check_recipe_and_station(world, actor, recipe, workstation)?;
    // Tools: carried (equipped first), free, one item per tool key.
    let equipped = world
        .get::<Equipment>(actor)
        .map(Equipment::items)
        .unwrap_or_default();
    let mut tools: Vec<SimId> = Vec::new();
    // Every tool and input with what the crafter has (for the error).
    let mut tool_needs: Vec<CraftNeed> = Vec::new();
    for key in &def.tools {
        let usable = |id: SimId| item_matches(world, id, key).is_some() && free_item(world, id, me);
        let pick = equipped
            .iter()
            .chain(inv.items.iter())
            .copied()
            .find(|&id| !tools.contains(&id) && usable(id));
        let have = equipped
            .iter()
            .chain(inv.items.iter())
            .filter(|&&id| !tools.contains(&id) && usable(id))
            .count();
        tool_needs.push(CraftNeed {
            key: key.clone(),
            tool: true,
            need: 1,
            have: u32::try_from(have).unwrap_or(u32::MAX),
        });
        if let Some(id) = pick {
            tools.push(id);
        }
    }
    // Inputs: inventory items (not equipped, not a tool), ascending.
    let mut inputs: Vec<(SimId, u32)> = Vec::new();
    let mut input_needs: Vec<CraftNeed> = Vec::new();
    for input in &def.inputs {
        let mut have = 0u32;
        let mut need = input.count;
        for &id in &inv.items {
            let Some(ie) = entity_of(world, id) else {
                continue;
            };
            if tools.contains(&id)
                || inputs.iter().any(|(i, _)| *i == id)
                || world.get::<CarriedBy>(ie).is_some_and(|c| c.slot.is_some())
                || !free_item(world, id, me)
            {
                continue;
            }
            if let Some(stack) = item_matches(world, id, &input.key) {
                have = have.saturating_add(stack);
                if need > 0 {
                    let take = stack.min(need);
                    need -= take;
                    inputs.push((id, take));
                }
            }
        }
        input_needs.push(CraftNeed {
            key: input.key.clone(),
            tool: false,
            need: input.count,
            have,
        });
    }
    let all: Vec<CraftNeed> = input_needs.into_iter().chain(tool_needs).collect();
    if all.iter().any(CraftNeed::missing) {
        let (mut needs, rest): (Vec<CraftNeed>, Vec<CraftNeed>) =
            all.into_iter().partition(CraftNeed::missing);
        needs.extend(rest);
        return Err(CraftError::Missing { needs });
    }
    check_skill(world, actor, me, &def)?;
    Ok(CraftPlan {
        recipe: def,
        workstation,
        tools,
        inputs,
    })
}

/// The completion check of a running craft `c` (final review bug 6): the
/// recipe, workstation and skill checks of [`can_craft`], then the
/// **claimed** items instead of a new pick: every planned tool still
/// carried by the crafter and matching its tool key, every planned input
/// still in its inventory (not equipped) with at least the planned units,
/// and the planned units still covering each recipe input. Returns the
/// plan of exactly those items.
pub fn check_claimed(world: &World, actor: Entity, c: &Crafting) -> Result<CraftPlan, CraftError> {
    let (def, me, _) = check_recipe_and_station(world, actor, &c.recipe, c.workstation)?;
    let carried_by_me = |id: SimId, in_slot: Option<bool>| {
        entity_of(world, id)
            .and_then(|e| world.get::<CarriedBy>(e))
            .is_some_and(|cb| {
                cb.holder == me && in_slot.is_none_or(|slot| cb.slot.is_some() == slot)
            })
    };
    for (i, key) in def.tools.iter().enumerate() {
        let ok = c
            .tools
            .get(i)
            .is_some_and(|&t| carried_by_me(t, None) && item_matches(world, t, key).is_some());
        if !ok {
            return Err(CraftError::MissingTool { tool: key.clone() });
        }
    }
    // Each planned input entry covers the first recipe input it matches
    // that still needs units (plan order is recipe-input order).
    let mut need: Vec<u32> = def.inputs.iter().map(|i| i.count).collect();
    let mut have: Vec<u32> = vec![0; def.inputs.len()];
    for &(id, units) in &c.inputs {
        let stack = item_matches_any(world, id, &def, &need);
        let (Some((slot, stack)), true) = (stack, carried_by_me(id, Some(false))) else {
            continue;
        };
        have[slot] = have[slot].saturating_add(stack);
        if stack >= units {
            need[slot] = need[slot].saturating_sub(units);
        }
    }
    if let Some(i) = need.iter().position(|&n| n > 0) {
        return Err(CraftError::MissingInput {
            input: def.inputs[i].key.clone(),
            need: def.inputs[i].count,
            have: have[i],
        });
    }
    check_skill(world, actor, me, &def)?;
    Ok(CraftPlan {
        recipe: def,
        workstation: c.workstation,
        tools: c.tools.clone(),
        inputs: c.inputs.clone(),
    })
}

/// The index of the first recipe input that `id` matches and that still
/// `need`s units, with the item's stack.
fn item_matches_any(
    world: &World,
    id: SimId,
    def: &RecipeDef,
    need: &[u32],
) -> Option<(usize, u32)> {
    def.inputs
        .iter()
        .enumerate()
        .filter(|(i, _)| need[*i] > 0)
        .find_map(|(i, input)| item_matches(world, id, &input.key).map(|stack| (i, stack)))
}

/// What `actor` is busy with (`"craft <recipe>"`, `"verb #target"`).
pub fn busy_with(world: &World, actor: Entity) -> Option<String> {
    if let Some(c) = world.get::<Crafting>(actor) {
        return Some(format!("craft {}", c.recipe));
    }
    world
        .get::<Interacting>(actor)
        .map(|i| format!("{} #{}", i.verb, i.target.0))
}

fn record_failed(
    world: &mut World,
    actor: SimId,
    recipe: &str,
    workstation: Option<SimId>,
    outcome: &str,
    reason: String,
    consumed: bool,
) {
    journal::record(world, Some(actor), || JournalKind::CraftFailed {
        recipe: recipe.to_owned(),
        workstation,
        outcome: outcome.to_owned(),
        reason,
        consumed,
    });
}

fn release_claims(world: &mut World, actor: SimId, targets: &[SimId], reason: ReleaseReason) {
    for &target in targets {
        let held = world
            .get_resource::<Reservations>()
            .is_some_and(|r| r.holds(target, actor));
        if held {
            world.resource_mut::<Reservations>().release(target, actor);
            journal::record(world, Some(actor), || JournalKind::Release {
                target,
                reason,
            });
        }
    }
}

fn all_claims(c: &Crafting) -> Vec<SimId> {
    let mut v = c.claimed.clone();
    v.extend(c.workstation);
    v
}

/// Starts `recipe` for `actor` (at `workstation`); see the module docs. A
/// refusal is journaled (`CraftFailed { outcome: "rejected" }`) and
/// returned.
pub fn begin_craft(
    world: &mut World,
    actor: Entity,
    recipe: &str,
    workstation: Option<SimId>,
    source: CraftSource,
    order: Option<u32>,
) -> Result<(), CraftError> {
    let Some(&me) = world.get::<SimId>(actor) else {
        return Err(CraftError::NotACrafter { unit: u32::MAX });
    };
    let checked = match busy_with(world, actor) {
        Some(with) => Err(CraftError::Busy { with }),
        None => can_craft(world, actor, recipe, workstation),
    };
    let plan = match checked {
        Ok(p) => p,
        Err(e) => {
            record_failed(
                world,
                me,
                recipe,
                workstation,
                "rejected",
                e.to_string(),
                false,
            );
            return Err(e);
        }
    };
    world.init_resource::<Reservations>();
    let mut claimed: Vec<SimId> = Vec::new();
    for target in plan.workstation.iter().copied().chain(plan.claimed()) {
        if npc_jobs::reserve(world, me, target, 1) {
            claimed.push(target);
        } else {
            release_claims(world, me, &claimed, ReleaseReason::Aborted);
            let e = CraftError::WorkstationInUse {
                workstation: target.0,
            };
            record_failed(
                world,
                me,
                recipe,
                workstation,
                "rejected",
                e.to_string(),
                false,
            );
            return Err(e);
        }
    }
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let ends = tick + u64::from(plan.recipe.duration_ticks);
    world.entity_mut(actor).insert(Crafting {
        recipe: recipe.to_owned(),
        workstation,
        claimed: plan.claimed(),
        started: tick,
        ends,
        source,
        order,
        tools: plan.tools.clone(),
        inputs: plan.inputs.clone(),
    });
    journal::record(world, Some(me), || JournalKind::CraftStarted {
        recipe: recipe.to_owned(),
        workstation,
        ends,
    });
    Ok(())
}

/// Ends `actor`'s craft early (`CraftFailed { outcome: "cancelled" }`),
/// releasing its claims. No-op when it is not crafting.
pub fn cancel_craft(world: &mut World, actor: Entity, reason: &str) {
    let Some(c) = world.get::<Crafting>(actor).cloned() else {
        return;
    };
    let Some(&me) = world.get::<SimId>(actor) else {
        return;
    };
    world.entity_mut(actor).remove::<Crafting>();
    release_claims(world, me, &all_claims(&c), ReleaseReason::Aborted);
    record_failed(
        world,
        me,
        &c.recipe,
        c.workstation,
        "cancelled",
        reason.to_owned(),
        false,
    );
}

/// Takes `units` from item `id` (despawned when used up).
fn consume(world: &mut World, id: SimId, units: u32) {
    let Some(e) = entity_of(world, id) else {
        return;
    };
    let left = match world.get_mut::<Item>(e) {
        Some(mut item) => {
            item.stack = item.stack.saturating_sub(units);
            item.stack
        }
        None => return,
    };
    if left == 0 {
        take_from_holder(world, e);
        world.despawn(e);
    }
}

/// Wears tool `id` by `wear` (critical: also the no-durability quality
/// loss). Returns whether it broke.
fn wear_tool(world: &mut World, id: SimId, wear: u32, critical: bool) -> bool {
    let Some(e) = entity_of(world, id) else {
        return false;
    };
    let broke = match world.get_mut::<Durability>(e) {
        Some(mut d) => {
            d.current = d.current.saturating_sub(wear);
            d.current == 0
        }
        None => {
            if critical && let Some(mut item) = world.get_mut::<Item>(e) {
                item.quality = item.quality.saturating_sub(1);
            }
            false
        }
    };
    if broke {
        take_from_holder(world, e);
        world.despawn(e);
    }
    broke
}

fn spawn_outputs(world: &mut World, actor: Entity, recipe: &RecipeDef, quality: i32) -> Vec<SimId> {
    let pos = world.get::<SimPos>(actor).copied().unwrap_or_default();
    let mut out = Vec::new();
    for o in &recipe.outputs {
        let (id, e) = spawn_sim(
            world,
            (
                Item {
                    kind: o.kind.clone(),
                    stack: o.count,
                    quality,
                },
                pos,
            ),
        );
        if can_store(world, actor, e).is_ok() {
            put_in_inventory(world, actor, e);
        } else {
            place_in_world(world, e, pos);
        }
        out.push(id);
    }
    out
}

/// Finishes one craft now (its duration is over): see the module docs.
/// Returns the completion (`None`: not crafting, or invalid).
pub fn complete_craft(world: &mut World, actor: Entity) -> Option<CraftCompleted> {
    let c = world.get::<Crafting>(actor).cloned()?;
    let &me = world.get::<SimId>(actor)?;
    world.entity_mut(actor).remove::<Crafting>();
    let plan = match check_claimed(world, actor, &c) {
        Ok(p) => p,
        Err(e) => {
            release_claims(world, me, &all_claims(&c), ReleaseReason::Aborted);
            record_failed(
                world,
                me,
                &c.recipe,
                c.workstation,
                "invalid",
                e.to_string(),
                false,
            );
            set_leaf(world, actor, c.source, false);
            return None;
        }
    };
    let def = plan.recipe.clone();
    let cfg = config(world);
    let roll = match &def.skill {
        Some(skill) if world.get::<CharacterSheet>(actor).is_some() => {
            let tool_bonus: i32 = plan
                .tools
                .iter()
                .filter_map(|&t| entity_of(world, t))
                .filter_map(|e| world.get::<Item>(e))
                .map(|i| i.quality)
                .sum();
            let purpose = format!("craft.{}", def.name);
            roll_skill(
                world,
                me,
                skill,
                def.modifier.saturating_add(tool_bonus),
                &purpose,
            )
        }
        _ => None,
    };
    let (result, margin) = match roll {
        None => (CraftResult::Success, 0),
        Some(r) => (
            match (r.success, r.critical) {
                (true, Critical::Success) => CraftResult::CriticalSuccess,
                (true, _) => CraftResult::Success,
                (false, Critical::Failure) => CraftResult::CriticalFailure,
                (false, _) => CraftResult::Failure,
            },
            r.margin,
        ),
    };
    let consumed = result != CraftResult::Failure;
    if consumed {
        for &(id, units) in &plan.inputs {
            consume(world, id, units);
        }
    }
    let critical_failure = result == CraftResult::CriticalFailure;
    let wear = if critical_failure {
        cfg.crit_fail_wear
    } else {
        cfg.wear_per_craft
    };
    let mut broken = Vec::new();
    for &t in &plan.tools {
        if wear_tool(world, t, wear, critical_failure) {
            broken.push(t);
        }
    }
    let (quality, outputs) = if result.succeeded() {
        let q = cfg
            .quality
            .quality(margin, result == CraftResult::CriticalSuccess);
        (Some(q), spawn_outputs(world, actor, &def, q))
    } else {
        (None, Vec::new())
    };
    release_claims(world, me, &all_claims(&c), ReleaseReason::Finished);
    let broke = broken
        .iter()
        .map(|t| format!(", tool #{} broke", t.0))
        .collect::<String>();
    match (result, quality) {
        (CraftResult::Success | CraftResult::CriticalSuccess, Some(q)) => {
            journal::record(world, Some(me), || JournalKind::CraftDone {
                recipe: def.name.clone(),
                workstation: c.workstation,
                margin,
                quality: q,
                outputs: outputs.clone(),
            });
            if let (Some(ws), Some(order)) = (c.workstation, c.order) {
                count_down(world, ws, order);
            }
        }
        (CraftResult::CriticalFailure, _) => record_failed(
            world,
            me,
            &def.name,
            c.workstation,
            "critical",
            format!("critical failure by {}{broke}", -margin),
            true,
        ),
        _ => record_failed(
            world,
            me,
            &def.name,
            c.workstation,
            "failed",
            format!(
                "{} roll failed by {}{broke}",
                def.skill.as_deref().unwrap_or("skill"),
                -margin
            ),
            false,
        ),
    }
    let done = CraftCompleted {
        unit: me,
        recipe: def.name.clone(),
        workstation: c.workstation,
        skill: def.skill.clone(),
        result,
        margin,
        quality,
        outputs,
        duration_ticks: def.duration_ticks,
        xp: def.xp,
    };
    let hooks = world
        .get_resource::<CraftingHooks>()
        .map(|h| h.on_complete.clone())
        .unwrap_or_default();
    for (_, f) in hooks {
        f(world, &done);
    }
    set_leaf(world, actor, c.source, result.succeeded());
    Some(done)
}

fn set_leaf(world: &mut World, actor: Entity, source: CraftSource, ok: bool) {
    if source == CraftSource::Tree
        && world.get_entity(actor).is_ok()
        && world.get::<CraftLeaf>(actor).is_some()
        && let Some(mut leaf) = world.get_mut::<ActiveLeaf>(actor)
    {
        leaf.status = if ok { Status::Success } else { Status::Failure };
    }
}

/// The crafters ([`crafting_complete_system`]'s cached query).
pub type CrafterQuery = QueryState<(&'static SimId, Entity, &'static Crafting)>;

/// `NpcCraftingSet::Complete`: [`complete_craft`] for every crafter whose
/// craft is due (`ends <= tick`), in `SimId` order (perf mechanic
/// `crafting`).
pub fn crafting_complete_system(world: &mut World, crafters: &mut CrafterQuery) {
    let _span = tracing::info_span!("mechanic", name = "crafting").entered();
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let mut due: Vec<(SimId, Entity)> = crafters
        .iter(world)
        .filter(|(_, _, c)| c.ends <= tick)
        .map(|(&id, e, _)| (id, e))
        .collect();
    due.sort_unstable_by_key(|d| d.0);
    for (_, e) in due {
        if world.get_entity(e).is_ok() {
            complete_craft(world, e);
        }
    }
}
