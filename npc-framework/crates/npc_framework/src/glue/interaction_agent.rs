//! Interaction × agent-io (features `interaction` + `agent-io`):
//! affordances for every registered verb (with `why_not`), one action
//! handler per verb, and the `inventory` observation section.
//!
//! [`InteractionAgentPlugin`] registers:
//!
//! - affordance provider `npc_interaction`: candidates are the unit's
//!   carried items plus the entities that are an item, a door, a tool
//!   target, another inventory holder or `Interactable` and that the unit
//!   **knows**: with a perception provider, everything it perceives or
//!   remembers, at any distance (no hidden radius: a far one is offered with
//!   `why_not: "too far: 12.3 m (reach 1.5 m)"`); without perception, those
//!   within `InteractionConfig::affordance_radius_mm`. Each verb (by name)
//!   is offered for each candidate it applies to, with the same checks the
//!   `Interact` leaf and command use (`why_not`: `"door is locked"`,
//!   `"needs tool:axe"`, `"too far: 5.2 m (reach 1.5 m)"`, ..). With the
//!   `rules` feature and the `npc_rules` skill-check hook, a verb that rolls
//!   a skill check carries its odds (`check`, and a label suffix
//!   `[Mining 11 vs 3d6: ~62%]`, `low odds` under 30%). A tool target's
//!   label names its yield and remaining uses, and `use_tool_on`'s says
//!   where the yield lands and the pack load after picking it up (`yields 2
//!   × log at your feet (pack 24950 -> 30950 of 30000 g, too heavy to pick
//!   up)`). A refusal for the unit's full inventory names what to drop
//!   (`"inventory is full (8 slots): drop 1 of [plank #66, ingot #89] to
//!   make room"`, `npc_interaction::items::with_room_hint`), in `why_not` and
//!   in the handler's rejection;
//! - the handler [`interaction_handler`] for every verb (registered in
//!   `Plugin::finish`, so verbs content registers later are covered; a
//!   handler content registered itself for a verb is kept). A `Direct`
//!   controller queues the `Interact` leaf (`SetBlackboard` target and item
//!   slots, then `Action "Interact"`); an `Advisor` controller submits the
//!   `Interact` command. Either way the interaction runs through
//!   `npc_interaction::begin`, journaled like any other. Args: `{"item": SimId}`
//!   for verbs that take an item (`give`, an explicit tool or key);
//! - observation section [`INVENTORY_SECTION`] (`"inventory"`): carried items,
//!   equipment, load and the running interaction. Equipped items take no
//!   slot (`"equipped_use_slots": false`), so "You carry" can list more
//!   things than there are slots; the load line says so when something is
//!   equipped (and that unequipping needs a free slot);
//! - possessions provider `npc_interaction` (carried and equipped items:
//!   always known, never "remembered"), describer `npc_interaction`
//!   (`store_key #26`, `2 × log #34`, `locked door #12`, `ore_rock #24
//!   (needs tool:pickaxe; yields 1 × ore; 4 uses left)`) and busy predicate
//!   `npc_interaction` (a running interaction).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::observe::known_targets;
use npc_agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, AgentRegistry, ControlMode,
    Rejection, Section, Step,
};
use npc_ai::{BbValue, ParamValue};
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{SimCommand, SimId, SimPos};
use serde_json::json;

use npc_interaction::builtin::USE_TOOL_ON;
use npc_interaction::inspect::item_json;
use npc_interaction::items::{
    carried_items, carried_weight, entity_of, pack_fit, position_of, with_room_hint,
};
use npc_interaction::run::{busy_with, check_def, ctx_for, metres};
use npc_interaction::{
    CarriedBy, Door, Equipment, INTERACT_ACTION, Interact, Interactable, Interacting,
    InteractionConfig, InteractionDef, InteractionRegistry, Interactor, Inventory, Item, ItemArg,
    ToolTarget,
};

/// `" (needs tool:pickaxe; yields 1 × ore; 4 uses left)"` of a tool target.
fn tool_target_note(t: &ToolTarget) -> String {
    tool_target_note_with(t, None)
}

/// [`tool_target_note`] with `load` after the yield (`"yields 2 × log at
/// your feet (pack ..)"`).
fn tool_target_note_with(t: &ToolTarget, load: Option<String>) -> String {
    let mut parts = vec![format!("needs {}", t.required_tag)];
    if !t.yields.is_empty() {
        let y: Vec<String> = t
            .yields
            .iter()
            .map(|y| format!("{} × {}", y.stack, y.kind))
            .collect();
        let mut part = format!("yields {}", y.join(" + "));
        if let Some(l) = load {
            part.push(' ');
            part.push_str(&l);
        }
        parts.push(part);
    }
    match t.remaining {
        Some(0) => parts.push("used up".to_owned()),
        Some(n) => parts.push(format!("{n} uses left")),
        None => {}
    }
    format!(" ({})", parts.join("; "))
}

/// Describer `npc_interaction`: items by kind (`store_key #26`, `2 × log
/// #34`), doors by state (`locked door #12`), tool targets with their kind,
/// tool, yield and uses (`ore_rock #24 (needs tool:pickaxe; ..)`).
pub fn describe_entity(world: &World, id: SimId) -> Option<String> {
    let e = entity_of(world, id)?;
    if let Some(item) = world.get::<Item>(e) {
        return Some(if item.stack > 1 {
            format!("{} × {} #{}", item.stack, item.kind, id.0)
        } else {
            format!("{} #{}", item.kind, id.0)
        });
    }
    if let Some(d) = world.get::<Door>(e) {
        let state = match (d.open, d.is_locked()) {
            (true, _) => "open",
            (false, true) => "locked",
            (false, false) => "closed",
        };
        return Some(format!("{state} door #{}", id.0));
    }
    if let Some(t) = world.get::<ToolTarget>(e) {
        return Some(format!(
            "{} #{}{}",
            npc_kernel::inspect::kind_name(world, e),
            id.0,
            tool_target_note(t)
        ));
    }
    None
}

/// Where a tool target's yield lands and what picking it up does to
/// `actor`'s pack: `"at your feet (pack 24950 -> 30950 of 30000 g, too
/// heavy to pick up)"` (`, no free slot to pick up`; nothing when it fits).
/// `None` without an inventory or a yield.
pub fn yield_load(world: &World, actor: Entity, t: &ToolTarget) -> Option<String> {
    if t.yields.is_empty() {
        return None;
    }
    let items: Vec<Item> = t
        .yields
        .iter()
        .map(|y| Item::new(y.kind.clone()).stack(y.stack).quality(y.quality))
        .collect();
    let f = pack_fit(world, actor, &items)?;
    let tail = if f.too_heavy() {
        ", too heavy to pick up"
    } else if f.no_slot() {
        ", no free slot to pick up"
    } else {
        ""
    };
    Some(format!(
        "at your feet (pack {} -> {} of {} g{tail})",
        f.load_g, f.after_g, f.max_g
    ))
}

/// Possessions provider `npc_interaction`: carried and equipped items.
pub fn carried_possessions(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    carried_items(world, ctx.entity)
}

/// Busy predicate `npc_interaction`: a running interaction.
pub fn interaction_busy(world: &World, e: Entity) -> Option<String> {
    world
        .get::<Interacting>(e)
        .map(|i| format!("{} #{}", i.verb, i.target.0))
}

fn describe(world: &World, id: SimId) -> String {
    let Some(e) = entity_of(world, id) else {
        return format!("#{}", id.0);
    };
    if let Some(item) = world.get::<Item>(e) {
        return if item.stack > 1 {
            format!("{} × {} #{}", item.stack, item.kind, id.0)
        } else {
            format!("the {} #{}", item.kind, id.0)
        };
    }
    if let Some(d) = world.get::<Door>(e) {
        let state = match (d.open, d.is_locked()) {
            (true, _) => "open",
            (false, true) => "locked",
            (false, false) => "closed",
        };
        return format!("the {state} door #{}", id.0);
    }
    if let Some(t) = world.get::<ToolTarget>(e) {
        return format!(
            "{} #{}{}",
            npc_kernel::inspect::kind_name(world, e),
            id.0,
            tool_target_note(t)
        );
    }
    format!("#{}", id.0)
}

/// Whether the `npc_interaction` affordance provider lists verbs for `e`
/// when it is near: an item, a door, a tool target, another inventory
/// holder or an `Interactable`. (Other glue offers verbs for the rest, e.g.
/// `mount` for vehicles, and skips these so ids never repeat.)
pub fn is_candidate(world: &World, e: Entity) -> bool {
    world.get::<Item>(e).is_some()
        || world.get::<Door>(e).is_some()
        || world.get::<ToolTarget>(e).is_some()
        || world.get::<Inventory>(e).is_some()
        || world.get::<Interactable>(e).is_some()
}

/// What the interaction affordances consider: carried items plus nearby or
/// known candidates (see the module docs).
fn candidates(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    let (actor, me) = (ctx.entity, ctx.unit);
    let mut out = carried_items(world, actor);
    if let Some(known) = known_targets(world, ctx) {
        out.extend(known.into_iter().filter(|&id| {
            id != me && entity_of(world, id).is_some_and(|e| is_candidate(world, e))
        }));
        out.sort_unstable();
        out.dedup();
        return out;
    }
    let radius = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .affordance_radius_mm;
    if let (Some(grid), Some(&pos)) = (
        world.get_resource::<SpatialGrid>(),
        world.get::<SimPos>(actor),
    ) {
        for id in grid.in_radius(pos, radius) {
            if id == me {
                continue;
            }
            let Some(e) = entity_of(world, id) else {
                continue;
            };
            if is_candidate(world, e) {
                out.push(id);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The affordance of verb `def` by `actor` on `target` (`None`: the verb
/// does not apply to it), with the checks of the `Interact` leaf and
/// command (`why_not` when one fails; not the "busy" or mode checks).
pub fn verb_affordance(
    world: &World,
    def: &InteractionDef,
    actor: Entity,
    target: SimId,
) -> Option<Affordance> {
    let ctx = ctx_for(world, actor, target, None).ok()?;
    if !(def.offer)(world, &ctx) {
        return None;
    }
    if let Some(i) = world.get::<Interactable>(ctx.target)
        && !i.offers(def.name)
    {
        return None;
    }
    let name = match world.get::<ToolTarget>(ctx.target) {
        Some(t) if def.name == USE_TOOL_ON => format!(
            "{} #{}{}",
            npc_kernel::inspect::kind_name(world, ctx.target),
            target.0,
            tool_target_note_with(t, yield_load(world, actor, t))
        ),
        _ => describe(world, target),
    };
    let mut label = format!("{} {name}", def.name.replace('_', " "));
    if let (Some(a), Some(b)) = (
        world.get::<SimPos>(actor).copied(),
        position_of(world, ctx.target),
    ) && def.needs_reach
    {
        label.push_str(&format!(" ({} away)", metres(a.dist2(b).isqrt())));
    }
    #[cfg(feature = "rules")]
    let check = odds::verb_check(world, def, &ctx);
    #[cfg(feature = "rules")]
    if let Some(c) = &check {
        label.push_str(&odds::label_suffix(c));
    }
    let mut a = Affordance::new(def.name, label)
        .target(target)
        .est_ticks(def.duration_for(world, &ctx));
    #[cfg(feature = "rules")]
    {
        a.check = check;
    }
    if let Some(req) = def.requires {
        for r in req(world, &ctx) {
            a = a.requires(&r);
        }
    }
    match def.item {
        ItemArg::None => {}
        ItemArg::Optional => a = a.arg("item", "SimId of a carried item to use (optional)"),
        ItemArg::Required => a = a.arg("item", "SimId of a carried item (required)"),
    }
    Some(match check_def(world, def, &ctx, true) {
        Ok(()) => a,
        Err(why) => a.why_not(with_room_hint(world, actor, why)),
    })
}

/// Name of the inventory section.
pub const INVENTORY_SECTION: &str = "inventory";
/// Name of the affordance provider.
pub const INTERACTION_AFFORDANCES: &str = "npc_interaction";

/// Affordance provider `npc_interaction`.
pub fn interaction_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let actor = ctx.entity;
    if world.get::<Interactor>(actor).is_none() {
        return Vec::new();
    }
    let Some(registry) = world.get_resource::<InteractionRegistry>() else {
        return Vec::new();
    };
    let targets = candidates(world, ctx);
    let busy = busy_with(world, actor);
    let mut out = Vec::new();
    for def in registry.verbs.values() {
        for &t in &targets {
            let Some(mut a) = verb_affordance(world, def, actor, t) else {
                continue;
            };
            if a.ok {
                if ctx.mode() == ControlMode::Observer {
                    a = a.why_not("observers cannot act");
                } else if ctx.mode() == ControlMode::Advisor
                    && let Some(b) = &busy
                {
                    a = a.why_not(format!("busy with {b}"));
                }
            }
            out.push(a);
        }
    }
    out
}

/// The action handler of every verb.
pub fn interaction_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let registry = world
        .get_resource::<InteractionRegistry>()
        .ok_or_else(|| Rejection::new("unknown_verb", "no interaction registry"))?;
    let def = *registry
        .verb(&req.verb)
        .ok_or_else(|| Rejection::new("unknown_verb", format!("unknown verb `{}`", req.verb)))?;
    let target = req
        .target
        .ok_or_else(|| Rejection::new("invalid_args", format!("{} needs a target", def.name)))?;
    let item = match req.opt_int("item")? {
        Some(v) => Some(SimId(u32::try_from(v).map_err(|_| {
            Rejection::new("invalid_args", format!("item {v} is not a SimId"))
        })?)),
        None => None,
    };
    let checked = ctx_for(world, ctx.entity, target, item)
        .and_then(|c| check_def(world, &def, &c, false).map(|()| c));
    let c = checked
        .map_err(|why| Rejection::new("unavailable", with_room_hint(world, ctx.entity, why)))?;
    let label = format!("{} {}", def.name.replace('_', " "), describe(world, target));
    let est = def.duration_for(world, &c);
    match ctx.mode() {
        ControlMode::Direct => {
            let config = world
                .get_resource::<InteractionConfig>()
                .copied()
                .unwrap_or_default();
            Ok(ActionPlan::Queue {
                label,
                steps: vec![
                    Step::SetBlackboard {
                        key: config.target_slot,
                        value: BbValue::U32(target.0),
                    },
                    Step::SetBlackboard {
                        key: config.item_slot,
                        value: item.map_or(BbValue::None, |i| BbValue::U32(i.0)),
                    },
                    Step::Action {
                        action: INTERACT_ACTION.to_owned(),
                        param: ParamValue::Name(def.name.to_owned()),
                    },
                ],
                est_ticks: Some(est),
            })
        }
        _ => {
            let mut cmd = Interact::new(ctx.unit, def.name, target);
            cmd.item = item.map(|i| i.0);
            Ok(ActionPlan::Commands {
                label,
                commands: vec![SimCommand::content(&cmd)],
            })
        }
    }
}

/// Observation section `inventory`: carried items, equipment, load and the
/// running interaction.
pub fn inventory_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let e = ctx.entity;
    let inv = world.get::<Inventory>(e);
    let eq = world.get::<Equipment>(e);
    if inv.is_none() && eq.is_none() && world.get::<Interactor>(e).is_none() {
        return None;
    }
    let items: Vec<serde_json::Value> = carried_items(world, e)
        .into_iter()
        .map(|id| item_json(world, id))
        .collect();
    let weight = carried_weight(world, e);
    let interacting = world.get::<Interacting>(e);
    let value = json!({
        "items": items,
        "equipment": eq.map(|q| &q.slots),
        "slots": inv.map(|i| i.slots),
        "used_slots": inv.map(|i| i.items.len()),
        "weight_g": weight,
        "max_weight_g": inv.map(|i| i.max_weight_g),
        "equipped_use_slots": inv.map(|_| false),
        "interacting": interacting,
    });
    let carried: Vec<String> = carried_items(world, e)
        .into_iter()
        .map(|id| {
            let slot = entity_of(world, id)
                .and_then(|ie| world.get::<CarriedBy>(ie))
                .and_then(|c| c.slot.clone());
            match slot {
                Some(s) => format!("{} (in {s})", describe(world, id)),
                None => describe(world, id),
            }
        })
        .collect();
    let mut section = Section::new(value);
    section = section.line(if carried.is_empty() {
        "You carry nothing.".to_owned()
    } else {
        format!("You carry: {}.", carried.join(", "))
    });
    if let Some(i) = inv {
        let equipped: Vec<String> = eq
            .map(|q| {
                q.slots
                    .iter()
                    .filter_map(|(slot, id)| {
                        let id = (*id)?;
                        let name =
                            describe_entity(world, id).unwrap_or_else(|| format!("#{}", id.0));
                        Some(format!("{name} in {slot}"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rule = if equipped.is_empty() {
            String::new()
        } else {
            format!(
                "; equipped items ({}) take no slot, but unequipping one needs a free slot",
                equipped.join(", ")
            )
        };
        section = section.line(format!(
            "Load {weight} of {} g, {} of {} slots used{rule}.",
            i.max_weight_g,
            i.items.len(),
            i.slots
        ));
    }
    if let Some(i) = interacting {
        section = section.line(format!(
            "You are busy: {} {} (done at tick {}).",
            i.verb,
            describe(world, i.target),
            i.ends
        ));
    }
    Some(section)
}

/// Interaction × agent-io glue (see the module docs). Add it with
/// `InteractionPlugin` and `AgentIoPlugin`.
pub struct InteractionAgentPlugin;

impl Plugin for InteractionAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation(INVENTORY_SECTION, inventory_section)
            .register_affordances(INTERACTION_AFFORDANCES, interaction_affordances)
            .register_possessions(INTERACTION_AFFORDANCES, carried_possessions)
            .register_describer(INTERACTION_AFFORDANCES, describe_entity)
            .register_busy(INTERACTION_AFFORDANCES, interaction_busy);
    }

    fn finish(&self, app: &mut App) {
        let verbs: Vec<&'static str> = app
            .world()
            .get_resource::<InteractionRegistry>()
            .map(|r| r.verbs.keys().copied().collect())
            .unwrap_or_default();
        let mut registry = app.world_mut().get_resource_or_init::<AgentRegistry>();
        for verb in verbs {
            registry.handlers.entry(verb).or_insert(interaction_handler);
        }
    }
}

/// Success odds of skill-checked verbs (feature `rules` too): the verb's
/// check on its target (`InteractionDef::check_for`), as the `npc_rules`
/// skill-check hook rolls it: the actor's effective skill plus the check's
/// modifier, with the rule set's dice and critical rules.
#[cfg(feature = "rules")]
pub mod odds {
    use bevy_ecs::prelude::*;

    use npc_interaction::{Ctx, InteractionDef, InteractionHooks};
    use npc_rules::{LevelSource, RuleSet};
    use serde_json::{Value, json};

    /// Below this many permille the label says `low odds`.
    pub const LOW_ODDS_PERMILLE: u64 = 300;

    /// Chance (permille, rounded down) that a roll of `rules`' dice against
    /// `target` succeeds (exact: every dice outcome is counted).
    pub fn success_permille(rules: &RuleSet, target: i32) -> u64 {
        let count = usize::from(rules.dice.count);
        let sides = usize::from(rules.dice.sides.max(1));
        // ways[t]: dice outcomes with total t.
        let mut ways: Vec<u64> = vec![1];
        for _ in 0..count {
            let mut next = vec![0u64; ways.len() + sides];
            for (t, &w) in ways.iter().enumerate() {
                for f in 1..=sides {
                    next[t + f] += w;
                }
            }
            ways = next;
        }
        let total: u64 = ways.iter().sum();
        let ok: u64 = ways
            .iter()
            .enumerate()
            .filter(|&(t, &w)| {
                w > 0 && i32::try_from(t).is_ok_and(|t| rules.resolve_roll(t, target).success)
            })
            .map(|(_, &w)| w)
            .sum();
        ok * 1000 / total.max(1)
    }

    /// The check `def` rolls for `ctx`, as the affordance's `check` JSON:
    /// `{"skill","target_number","dice","odds_permille","default"?}`, or
    /// `{"skill","can_attempt":false,"odds_permille":0}`. `None` when the
    /// verb has no check, the hook is not `npc_rules` or the actor has no
    /// character sheet (the check then always passes).
    pub fn verb_check(world: &World, def: &InteractionDef, ctx: &Ctx) -> Option<Value> {
        let hooks = world.get_resource::<InteractionHooks>()?;
        if hooks.skill_check.map(|(n, _)| n) != Some("npc_rules") {
            return None;
        }
        world.get::<npc_rules::CharacterSheet>(ctx.actor)?;
        let check = def.check_for(world, ctx)?;
        skill_check(world, ctx.actor, ctx.actor_id, &check.skill, check.modifier)
    }

    /// A `npc_rules` roll of `skill` with `modifier` by `actor` (what
    /// `npc_rules::roll_skill` targets: the effective level plus the
    /// modifier), as [`verb_check`]'s JSON. `None` without a character
    /// sheet. Crafts use it too (modifier: the recipe's plus its tools'
    /// quality).
    pub fn skill_check(
        world: &World,
        actor: Entity,
        actor_id: npc_kernel::SimId,
        skill: &str,
        modifier: i32,
    ) -> Option<Value> {
        world.get::<npc_rules::CharacterSheet>(actor)?;
        let default_rules;
        let rules = match world.get_resource::<RuleSet>() {
            Some(r) => r,
            None => {
                default_rules = RuleSet::default();
                &default_rules
            }
        };
        let dice = format!("{}d{}", rules.dice.count, rules.dice.sides);
        let Some(level) = npc_rules::skill_level(world, actor_id, skill) else {
            return Some(json!({
                "skill": skill, "can_attempt": false, "dice": dice, "odds_permille": 0,
            }));
        };
        let target = level.level.saturating_add(modifier);
        let mut v = json!({
            "skill": skill, "target_number": target, "dice": dice,
            "odds_permille": success_permille(rules, target),
        });
        if let LevelSource::Default { from, modifier } = &level.source {
            v["default"] = json!(format!("{from}{modifier:+}"));
        }
        Some(v)
    }

    /// `" [Mining 11 vs 3d6: ~62%]"`, `" [Mining 5 (default ST-5) vs 3d6:
    /// ~4%, low odds]"`, `" [Mining: cannot attempt (no skill, no default):
    /// fails]"`.
    pub fn label_suffix(check: &Value) -> String {
        let skill = check["skill"].as_str().unwrap_or("?");
        if check["can_attempt"] == json!(false) {
            return format!(" [{skill}: cannot attempt (no skill, no default): fails]");
        }
        let odds = check["odds_permille"].as_u64().unwrap_or(0);
        let default = check["default"]
            .as_str()
            .map(|d| format!(" (default {d})"))
            .unwrap_or_default();
        format!(
            " [{skill} {}{default} vs {}: ~{}%{}]",
            check["target_number"],
            check["dice"].as_str().unwrap_or("?"),
            odds / 10,
            if odds < LOW_ODDS_PERMILLE {
                ", low odds"
            } else {
                ""
            }
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn three_d6_odds_are_exact() {
            let r = RuleSet::default();
            // 3d6 ≤ 10 is 108/216 = 50 %, but 3-4 always succeed and 17-18
            // always fail under the default critical rules.
            let p10 = success_permille(&r, 10);
            assert_eq!(p10, 500, "{p10}");
            assert!(success_permille(&r, 3) >= 18, "3 and 4 always succeed");
            assert!(success_permille(&r, 20) < 1000, "17 and 18 always fail");
        }
    }
}
