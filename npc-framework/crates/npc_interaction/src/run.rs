//! Running an interaction: the one path every entry point uses.
//!
//! [`begin`] checks ([`can_interact`]), claims the target, inserts
//! [`Interacting`] and journals `Interacted { outcome: "started" }`. The
//! `Interact` leaf (tree and `Direct` agents), the `Interact` command
//! (orders, `Advisor` agents, scripts) and the door-on-path behaviour all
//! call it. [`complete_interactions_system`] finishes every interaction whose
//! duration is over, in `SimId` order: it checks again, rolls the skill check
//! (through the hook), applies the effects, makes the noise, releases the
//! claim and journals the outcome (`success`, `failed`: the check failed, the
//! time is spent and nothing happens; `invalid`: the world changed meanwhile,
//! with the reason). [`cancel`] ends one early (`cancelled`).
//!
//! The journal says a little more than the verbs' own reasons (which
//! orders keep in hashed state, so they stay the bare rule): a refusal for
//! the actor's full inventory names what to drop
//! ([`with_room_hint`](crate::items::with_room_hint)), and a success that
//! put new items on the ground (a tool yield at the actor's feet) lists them
//! in `dropped`, with why the pack could not take them in `reason`.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{ActiveLeaf, Status};
use npc_jobs::{Reservations, free_for};
use npc_kernel::journal::{self, DroppedItem, JournalKind, ReleaseReason};
use npc_kernel::{SimClock, SimId, SimIdAllocator, SimPos};
use serde::{Deserialize, Serialize};

use crate::InteractionConfig;
use crate::defs::{
    Ctx, InteractionDef, InteractionHooks, InteractionListeners, InteractionRegistry, ItemArg,
};
use crate::items::{CarriedBy, Interactor, Item, entity_of, pack_fit, position_of, with_room_hint};
use crate::leaf::InteractLeaf;
use crate::targets::Interactable;

/// Who started an interaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum Source {
    /// The `Interact` leaf (a behaviour tree, or a `Direct` agent's queue).
    #[default]
    Tree,
    /// The `Interact` command.
    Command,
    /// The door-on-path behaviour.
    Auto,
}

/// A running interaction on its actor.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Interacting {
    pub verb: String,
    pub target: SimId,
    pub item: Option<SimId>,
    /// Tick it started.
    pub started: u64,
    /// Tick its effect applies (`started + duration`).
    pub ends: u64,
    pub source: Source,
}

/// `mm` as `"5.2 m"`.
pub fn metres(mm: i64) -> String {
    let mm = mm.max(0);
    format!("{}.{} m", mm / 1000, (mm % 1000) / 100)
}

fn def_of(world: &World, verb: &str) -> Option<InteractionDef> {
    world
        .get_resource::<InteractionRegistry>()
        .and_then(|r| r.verb(verb))
        .copied()
}

/// The reach for `def` (its own, else the configured default).
pub fn reach_of(world: &World, def: &InteractionDef) -> i32 {
    def.reach_mm.unwrap_or_else(|| {
        world
            .get_resource::<InteractionConfig>()
            .copied()
            .unwrap_or_default()
            .reach_mm
    })
}

/// The context for `actor` doing something to `target` (with `item`).
pub fn ctx_for(
    world: &World,
    actor: Entity,
    target: SimId,
    item: Option<SimId>,
) -> Result<Ctx, String> {
    let actor_id = *world
        .get::<SimId>(actor)
        .ok_or_else(|| "the actor is not a sim entity".to_owned())?;
    let target_e = entity_of(world, target).ok_or_else(|| format!("#{} is gone", target.0))?;
    let item = match item {
        Some(id) => Some((
            id,
            entity_of(world, id).ok_or_else(|| format!("item #{} is gone", id.0))?,
        )),
        None => None,
    };
    Ok(Ctx {
        actor,
        actor_id,
        target: target_e,
        target_id: target,
        item,
    })
}

/// Every check for `def` on `ctx` except "busy". With `listing`, a
/// required item may be missing (affordances list the verb with an `item`
/// argument).
pub fn check_def(
    world: &World,
    def: &InteractionDef,
    ctx: &Ctx,
    listing: bool,
) -> Result<(), String> {
    if world.get::<Interactor>(ctx.actor).is_none() {
        return Err(format!("#{} cannot interact", ctx.actor_id.0));
    }
    if let Some(i) = world.get::<Interactable>(ctx.target)
        && !i.offers(def.name)
    {
        return Err(format!("#{} does not offer {}", ctx.target_id.0, def.name));
    }
    if !(def.offer)(world, ctx) {
        return Err(format!("cannot {} #{}", def.name, ctx.target_id.0));
    }
    if def.item == ItemArg::Required && ctx.item.is_none() && !listing {
        return Err(format!("{} needs an item", def.name));
    }
    (def.precondition)(world, ctx)?;
    if def.needs_reach {
        let reach = reach_of(world, def);
        let (Some(a), Some(b)) = (
            world.get::<SimPos>(ctx.actor).copied(),
            position_of(world, ctx.target),
        ) else {
            return Err("out of reach".into());
        };
        let d2 = a.dist2(b);
        if d2 > i64::from(reach) * i64::from(reach) {
            return Err(format!(
                "too far: {} (reach {})",
                metres(d2.isqrt()),
                metres(i64::from(reach))
            ));
        }
    }
    if def.reserve_target
        && let Some(r) = world.get_resource::<Reservations>()
        && !free_for(r, ctx.target_id, ctx.actor_id)
    {
        return Err(format!("#{} is in use", ctx.target_id.0));
    }
    Ok(())
}

/// Whether `actor` can do `verb` to `target` (with `item`) now, ignoring
/// whether it is busy. `Err` is the reason.
pub fn can_interact(
    world: &World,
    actor: Entity,
    verb: &str,
    target: SimId,
    item: Option<SimId>,
) -> Result<Ctx, String> {
    let def = def_of(world, verb).ok_or_else(|| format!("unknown verb `{verb}`"))?;
    let ctx = ctx_for(world, actor, target, item)?;
    check_def(world, &def, &ctx, false)?;
    Ok(ctx)
}

/// What `actor` is busy with: its running interaction as `"verb #target"`,
/// else the first registered busy predicate that answers (name order, e.g.
/// `"craft bread"`).
pub fn busy_with(world: &World, actor: Entity) -> Option<String> {
    if let Some(i) = world.get::<Interacting>(actor) {
        return Some(format!("{} #{}", i.verb, i.target.0));
    }
    world
        .get_resource::<InteractionListeners>()?
        .busy
        .iter()
        .find_map(|(_, f)| f(world, actor))
}

/// Calls the finished hooks (name order) for `i`, which just ended.
fn notify_finished(
    world: &mut World,
    actor: Entity,
    i: &Interacting,
    outcome: &str,
    reason: Option<&str>,
) {
    let hooks = match world.get_resource::<InteractionListeners>() {
        Some(l) if !l.finished.is_empty() => l.finished.clone(),
        _ => return,
    };
    for (_, f) in hooks {
        f(world, actor, i, outcome, reason);
    }
}

#[allow(clippy::too_many_arguments)]
fn record_interacted(
    world: &mut World,
    actor: SimId,
    verb: &str,
    target: SimId,
    item: Option<SimId>,
    outcome: &str,
    reason: Option<String>,
    dropped: Vec<DroppedItem>,
) {
    journal::record(world, Some(actor), || JournalKind::Interacted {
        verb: verb.to_owned(),
        target,
        item,
        outcome: outcome.to_owned(),
        reason,
        dropped,
    });
}

/// A refusal as the journal shows it: with what to drop when it is about
/// `actor`'s full inventory (only computed while the journal records).
fn shown_reason(world: &World, actor: Entity, reason: &str) -> String {
    if journal::is_recording(world) {
        with_room_hint(world, actor, reason.to_owned())
    } else {
        reason.to_owned()
    }
}

/// The next `SimId` the allocator hands out (what an effect spawns gets
/// this id or a higher one).
fn next_sim_id(world: &World) -> u32 {
    world
        .get_resource::<SimIdAllocator>()
        .map_or(1, |a| a.next.max(1))
}

/// The items with ids in `from..` that lie on the ground (spawned by an
/// effect at someone's feet), in `SimId` order.
fn new_ground_items(world: &World, from: u32) -> Vec<DroppedItem> {
    (from..next_sim_id(world))
        .filter_map(|id| {
            let e = entity_of(world, SimId(id))?;
            if world.get::<CarriedBy>(e).is_some() {
                return None;
            }
            let (item, p) = (world.get::<Item>(e)?, world.get::<SimPos>(e)?);
            Some(DroppedItem {
                item: SimId(id),
                kind: item.kind.clone(),
                stack: item.stack,
                pos: [p.x, p.z],
            })
        })
        .collect()
}

/// Starts `verb` on `target` for `actor` (see the module docs). A refusal
/// is journaled (`outcome: "rejected"`) and returned.
pub fn begin(
    world: &mut World,
    actor: Entity,
    verb: &str,
    target: SimId,
    item: Option<SimId>,
    source: Source,
) -> Result<(), String> {
    let Some(&actor_id) = world.get::<SimId>(actor) else {
        return Err("the actor is not a sim entity".into());
    };
    let checked = match busy_with(world, actor) {
        Some(b) => Err(format!("busy with {b}")),
        None => can_interact(world, actor, verb, target, item),
    };
    let ctx = match checked {
        Ok(ctx) => ctx,
        Err(reason) => {
            let shown = shown_reason(world, actor, &reason);
            record_interacted(
                world,
                actor_id,
                verb,
                target,
                item,
                "rejected",
                Some(shown),
                Vec::new(),
            );
            return Err(reason);
        }
    };
    let def = def_of(world, verb).expect("checked above");
    if def.reserve_target
        && world.contains_resource::<Reservations>()
        && !npc_jobs::reserve(world, actor_id, target, 1)
    {
        let reason = format!("#{} is in use", target.0);
        record_interacted(
            world,
            actor_id,
            verb,
            target,
            item,
            "rejected",
            Some(reason.clone()),
            Vec::new(),
        );
        return Err(reason);
    }
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let duration = def.duration_for(world, &ctx);
    world.entity_mut(actor).insert(Interacting {
        verb: verb.to_owned(),
        target,
        item,
        started: tick,
        ends: tick + u64::from(duration),
        source,
    });
    record_interacted(
        world,
        actor_id,
        verb,
        target,
        item,
        "started",
        None,
        Vec::new(),
    );
    Ok(())
}

fn release_claim(world: &mut World, actor: SimId, target: SimId, reason: ReleaseReason) {
    let held = world
        .get_resource::<Reservations>()
        .is_some_and(|r| r.holds(target, actor));
    if !held {
        return;
    }
    world.resource_mut::<Reservations>().release(target, actor);
    journal::record(world, Some(actor), || JournalKind::Release {
        target,
        reason,
    });
}

/// Ends `actor`'s interaction early (`outcome: "cancelled"`, with `reason`),
/// releasing its claim. No-op when it is not interacting.
pub fn cancel(world: &mut World, actor: Entity, reason: &str) {
    let Some(i) = world.get::<Interacting>(actor).cloned() else {
        return;
    };
    let Some(&actor_id) = world.get::<SimId>(actor) else {
        return;
    };
    world.entity_mut(actor).remove::<Interacting>();
    if def_of(world, &i.verb).is_some_and(|d| d.reserve_target) {
        release_claim(world, actor_id, i.target, ReleaseReason::Aborted);
    }
    record_interacted(
        world,
        actor_id,
        &i.verb,
        i.target,
        i.item,
        "cancelled",
        Some(reason.to_owned()),
        Vec::new(),
    );
    notify_finished(world, actor, &i, "cancelled", Some(reason));
}

/// The skill check through the hook: `true` = passed (no check, no hook or
/// no rules for this actor all pass).
fn run_check(world: &mut World, def: &InteractionDef, ctx: &Ctx) -> Result<(), String> {
    let Some(check) = def.check_for(world, ctx) else {
        return Ok(());
    };
    let hook = world
        .get_resource::<InteractionHooks>()
        .and_then(|h| h.skill_check);
    let Some((_, f)) = hook else {
        return Ok(());
    };
    let purpose = format!("interaction.{}", def.name);
    match f(world, ctx.actor, &check, &purpose) {
        Some(false) => Err(format!("{} check failed", check.skill)),
        _ => Ok(()),
    }
}

fn make_noise(world: &mut World, def: &InteractionDef, ctx: &Ctx) {
    let Some(spec) = def.noise else {
        return;
    };
    let hook = world
        .get_resource::<InteractionHooks>()
        .and_then(|h| h.noise);
    let (Some((_, f)), Some(pos)) = (hook, world.get::<SimPos>(ctx.actor).copied()) else {
        return;
    };
    f(world, ctx.actor_id, pos, spec);
}

/// Finishes one interaction now (its duration is over): see the module docs.
/// Returns whether it succeeded.
pub fn complete(world: &mut World, actor: Entity) -> bool {
    let Some(i) = world.get::<Interacting>(actor).cloned() else {
        return false;
    };
    let Some(&actor_id) = world.get::<SimId>(actor) else {
        return false;
    };
    world.entity_mut(actor).remove::<Interacting>();
    let def = def_of(world, &i.verb);
    let first_new = next_sim_id(world);
    let (outcome, reason) = match (def, can_interact(world, actor, &i.verb, i.target, i.item)) {
        (Some(def), Ok(ctx)) => match run_check(world, &def, &ctx) {
            Ok(()) => {
                (def.effect)(world, &ctx);
                make_noise(world, &def, &ctx);
                ("success", None)
            }
            Err(why) => ("failed", Some(why)),
        },
        (_, Err(why)) => ("invalid", Some(why)),
        (None, Ok(_)) => ("invalid", Some(format!("unknown verb `{}`", i.verb))),
    };
    // What the journal shows (the hooks below get the bare reason).
    let (shown, dropped) = if !journal::is_recording(world) {
        (reason.clone(), Vec::new())
    } else if outcome == "success" {
        let dropped = new_ground_items(world, first_new);
        let items: Vec<Item> = dropped
            .iter()
            .filter_map(|d| world.get::<Item>(entity_of(world, d.item)?).cloned())
            .collect();
        let why = if dropped.is_empty() {
            None
        } else {
            pack_fit(world, actor, &items).and_then(|f| f.refusal())
        };
        (why, dropped)
    } else {
        (
            reason.as_deref().map(|r| shown_reason(world, actor, r)),
            Vec::new(),
        )
    };
    record_interacted(
        world, actor_id, &i.verb, i.target, i.item, outcome, shown, dropped,
    );
    if def.is_some_and(|d| d.reserve_target) {
        release_claim(world, actor_id, i.target, ReleaseReason::Finished);
    }
    notify_finished(world, actor, &i, outcome, reason.as_deref());
    let ok = outcome == "success";
    if i.source == Source::Tree
        && world.get::<InteractLeaf>(actor).is_some()
        && let Some(mut leaf) = world.get_mut::<ActiveLeaf>(actor)
    {
        leaf.status = if ok { Status::Success } else { Status::Failure };
    }
    ok
}

/// The actors in an interaction ([`complete_interactions_system`]'s cached
/// query).
pub type InteractingQuery = QueryState<(&'static SimId, Entity, &'static Interacting)>;

/// `NpcInteractionSet::Complete`: [`complete`] for every actor whose
/// interaction is due (`ends <= tick`), in `SimId` order.
pub fn complete_interactions_system(world: &mut World, actors: &mut InteractingQuery) {
    let _span = tracing::info_span!("mechanic", name = "interaction").entered();
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let mut due: Vec<(SimId, Entity)> = actors
        .iter(world)
        .filter(|(_, _, i)| i.ends <= tick)
        .map(|(&id, e, _)| (id, e))
        .collect();
    due.sort_unstable_by_key(|d| d.0);
    for (_, e) in due {
        complete(world, e);
    }
}
