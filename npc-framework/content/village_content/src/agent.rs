//! The village side of external agent control (`npc_framework::agent_io`):
//! observation sections, affordances and action handlers for villagers,
//! plus the village's scenario loader.
//!
//! [`VillageAgentPlugin`] registers (only when a host adds it, next to
//! `AgentIoPlugin`, so the plain village is unchanged):
//! - sections `needs` (`{"Hunger":{"value":..,"level":..},..}`) and `village`
//!   (home, activity, the nearest places with their approach and food);
//! - affordances `walk_to` (to a position, or `walk_to@<place>`), `eat`,
//!   `drink`, `sleep`, `work` (with `why_not` when no food / well slot / bed /
//!   work site is free);
//!   the generic `wait` comes from `npc_agent_io`;
//! - handlers: in `Direct` mode each verb queues the same actions the
//!   villager tree uses (`ReserveFood → MoveToDest → EatFood`,
//!   `ReserveWell → MoveToDest → DrinkWater`, ...), so an
//!   agent-driven villager eats, sleeps and works exactly like an autonomous
//!   one; in `Advisor` mode `walk_to` becomes an RTS order (`GoTo`).
//!
//! Scenario files (`content: "village"`): `world` is
//! `{"layout": "village" | "village_n" | "village_nav", "plots": N,
//! "stockpile_food": F}` (all optional); `actors` are extra villagers
//! `{"kind":"villager","pos":[x,z],"home":H?,"needs":{"Hunger":30000,..}?}`,
//! spawned after the layout (so the layout keeps its SimIds: house 1,
//! bed 2, stockpile 3, field 4, villager 5, well 6 in `village`; the first
//! actor is 7).

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::scenario::{LoadedScenario, ScenarioDoc, ScenarioLoaders};
use npc_framework::agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, AgentIoPlugin, ControlMode,
    Rejection, Section, Step,
};
use npc_framework::jobs::approach_of;
use npc_framework::kernel::commands::check_pos;
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::needs::update_level;
use npc_framework::prelude::*;
use serde_json::{Value, json};

use crate::actions::{DRINK_TICKS, EAT_TICKS, SLEEP_FULL, SLEEP_MORNING, WORK_ANY_TIME};
use crate::activity::Activity;
use crate::scenario::{try_new_sim_app_with, villager_bundle};
use crate::villager::{BB_DEST, VillagerTree, find_bed, find_food, find_well_for, find_work_site};
use crate::world::{Home, Place, PlaceKind, Stockpile, Villager, WorkSite};

/// Places listed in an observation / offered as walk targets: the nearest.
pub const NEAREST_PLACES: usize = 8;

/// The village's agent-control registrations (see the module docs).
pub struct VillageAgentPlugin;

impl Plugin for VillageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation("needs", needs_section)
            .register_observation("village", village_section)
            .register_affordances("village", affordances)
            .register_action_handler("walk_to", walk_to)
            .register_action_handler("eat", eat)
            .register_action_handler("drink", drink)
            .register_action_handler("sleep", sleep)
            .register_action_handler("work", work);
    }
}

/// Adds the village loader (`content: "village"`) to `loaders`.
pub fn register_scenario_loader(loaders: &mut ScenarioLoaders) {
    loaders.add("village", load_scenario);
}

// ---------------------------------------------------------------- observe

fn villager(world: &World, ctx: &AgentCtx) -> Option<Villager> {
    world.get::<Villager>(ctx.entity).copied()
}

fn percent(v: i32) -> i32 {
    v / 1000
}

fn needs_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let needs = world.get::<Needs>(ctx.entity)?;
    let registry = world.get_resource::<NeedRegistry>()?;
    let mut map = serde_json::Map::new();
    let mut parts = Vec::new();
    for id in registry.display_order() {
        let def = registry.def(id);
        let need = needs.get(id);
        map.insert(
            def.name.to_owned(),
            json!({"value": need.value, "level": need.level}),
        );
        parts.push(format!(
            "{} {}% ({:?})",
            def.name,
            percent(need.value),
            need.level
        ));
    }
    Some(
        Section::new(Value::Object(map))
            .line(format!("Needs (100% = satisfied): {}.", parts.join(", "))),
    )
}

fn kind_label(kind: PlaceKind) -> &'static str {
    match kind {
        PlaceKind::House => "house",
        PlaceKind::Bed => "bed",
        PlaceKind::Stockpile => "stockpile",
        PlaceKind::Field => "field",
        PlaceKind::Well => "well",
    }
}

/// `(id, entity, kind, approach, dist2)` of the nearest places, nearest
/// first (ties by SimId).
fn nearest_places(world: &World, from: SimPos) -> Vec<(SimId, Entity, PlaceKind, SimPos, i64)> {
    let mut places: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(&id, &e)| {
            let place = world.get::<Place>(e)?;
            let approach = approach_of(world, e)?;
            Some((id, e, place.kind, approach, from.dist2(approach)))
        })
        .collect();
    places.sort_by_key(|p| (p.4, p.0));
    places.truncate(NEAREST_PLACES);
    places
}

fn mm_to_m(d2: i64) -> String {
    npc_framework::agent_io::observe::metres(d2.isqrt())
}

fn village_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let v = villager(world, ctx)?;
    let pos = world.get::<SimPos>(ctx.entity).copied()?;
    let activity = world.get::<Activity>(ctx.entity).copied();
    let places = nearest_places(world, pos);
    let list: Vec<Value> = places
        .iter()
        .map(|&(id, e, kind, approach, d2)| {
            let pile = world.get::<Stockpile>(e);
            json!({
                "id": id, "kind": kind_label(kind), "approach": [approach.x, approach.z],
                "dist_mm": d2.isqrt(),
                "food": pile.map(|p| p.food), "capacity": pile.map(|p| p.capacity),
            })
        })
        .collect();
    let nearby: Vec<String> = places
        .iter()
        .map(|&(id, e, kind, _, d2)| {
            let food = world
                .get::<Stockpile>(e)
                .map(|p| format!(", food {}/{}", p.food, p.capacity))
                .unwrap_or_default();
            format!("{} #{} {} m{food}", kind_label(kind), id.0, mm_to_m(d2))
        })
        .collect();
    let value = json!({
        "home": v.home, "speed_mm_per_tick": v.speed, "activity": activity, "places": list,
    });
    Some(
        Section::new(value)
            .line(format!(
                "You live in house #{}; you walk {} mm per tick; activity: {:?}.",
                v.home.0,
                v.speed,
                activity.unwrap_or_default()
            ))
            .line(format!("Nearby: {}.", nearby.join("; "))),
    )
}

// ---------------------------------------------------------------- affordances

fn walk_ticks(world: &World, ctx: &AgentCtx, to: SimPos) -> u32 {
    let speed = villager(world, ctx).map_or(1, |v| v.speed.max(1));
    let from = world.get::<SimPos>(ctx.entity).copied().unwrap_or(to);
    u32::try_from(from.dist2(to).isqrt() / i64::from(speed)).unwrap_or(u32::MAX)
}

fn mode_gate(a: Affordance, ctx: &AgentCtx, direct_only: bool) -> Affordance {
    match ctx.mode() {
        ControlMode::Observer => a.why_not("observers cannot act"),
        ControlMode::Advisor if direct_only => {
            a.why_not("needs Direct control (as Advisor you can only give walk_to orders)")
        }
        _ => a,
    }
}

fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let Some(v) = villager(world, ctx) else {
        return Vec::new();
    };
    let Some(pos) = world.get::<SimPos>(ctx.entity).copied() else {
        return Vec::new();
    };
    let mut out = vec![mode_gate(
        Affordance::new("walk_to", "Walk to a position").arg("pos", "[x, z] in mm"),
        ctx,
        false,
    )];
    for (id, _, kind, approach, d2) in nearest_places(world, pos) {
        let a = Affordance::new(
            "walk_to",
            format!(
                "Walk to the {} #{} ({} m away)",
                kind_label(kind),
                id.0,
                mm_to_m(d2)
            ),
        )
        .target(id)
        .pos(approach.x, approach.z)
        .est_ticks(walk_ticks(world, ctx, approach));
        out.push(mode_gate(a, ctx, false));
    }
    let me = ctx.unit;
    let eat = match find_food(world, me) {
        Some((pile, e)) => {
            let to = approach_of(world, e).unwrap_or(pos);
            Affordance::new("eat", format!("Eat at the stockpile #{}", pile.0))
                .target(pile)
                .est_ticks(walk_ticks(world, ctx, to) + EAT_TICKS)
                .requires("food available")
        }
        None => Affordance::new("eat", "Eat").why_not("no stockpile has food available"),
    };
    out.push(mode_gate(eat, ctx, true));
    let drink = match find_well_for(world, ctx.entity) {
        Some((well, e)) => {
            let to = approach_of(world, e).unwrap_or(pos);
            Affordance::new("drink", format!("Drink at the well #{}", well.0))
                .target(well)
                .est_ticks(walk_ticks(world, ctx, to) + DRINK_TICKS)
                .requires("free well slot")
        }
        None => Affordance::new("drink", "Drink").why_not("no well has a free slot"),
    };
    out.push(mode_gate(drink, ctx, true));
    let sleep = match find_bed(world, me, v.home) {
        Some((bed, e)) => {
            let to = approach_of(world, e).unwrap_or(pos);
            Affordance::new("sleep", format!("Sleep in bed #{}", bed.0))
                .target(bed)
                .arg("until", "Full (default) or Morning")
                .est_ticks(walk_ticks(world, ctx, to))
                .requires("free bed at home")
        }
        None => Affordance::new("sleep", "Sleep").why_not("no free bed in your home"),
    };
    out.push(mode_gate(sleep, ctx, true));
    let work = match find_work_site(world, me, false) {
        Some((site, e)) => {
            let to = approach_of(world, e).unwrap_or(pos);
            let ticks = world.get::<WorkSite>(e).map_or(0, |w| w.work_ticks);
            Affordance::new(
                "work",
                format!("Work a shift at the field #{} (adds food)", site.0),
            )
            .target(site)
            .est_ticks(walk_ticks(world, ctx, to) + ticks)
            .requires("free work site")
        }
        None => Affordance::new("work", "Work").why_not("no free work site"),
    };
    out.push(mode_gate(work, ctx, true));
    out
}

// ---------------------------------------------------------------- handlers

fn action(name: &str, param: ParamValue) -> Step {
    Step::Action {
        action: name.to_owned(),
        param,
    }
}

fn direct_only(ctx: &AgentCtx, verb: &str) -> Result<(), Rejection> {
    if ctx.mode() == ControlMode::Direct {
        Ok(())
    } else {
        Err(Rejection::new(
            "direct_only",
            format!("`{verb}` needs Direct control"),
        ))
    }
}

fn need_villager(world: &World, ctx: &AgentCtx) -> Result<Villager, Rejection> {
    villager(world, ctx).ok_or_else(|| Rejection::new("unsupported", "only villagers can do this"))
}

fn walk_to(world: &World, ctx: &AgentCtx, req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    need_villager(world, ctx)?;
    let target_pos = req.target.and_then(|t| {
        let e = world.resource::<SimIndex>().get(t)?;
        approach_of(world, e)
    });
    let (x, z) = match (req.pos, target_pos) {
        (Some(p), _) => p,
        (None, Some(p)) => (p.x, p.z),
        (None, None) => {
            return Err(Rejection::new(
                "invalid_args",
                "walk_to needs pos [x, z] (mm) or a target place",
            ));
        }
    };
    let goal =
        check_pos("pos", (x, z)).map_err(|e| Rejection::new("invalid_args", e.to_string()))?;
    let label = format!(
        "Walk to ({} m, {} m)",
        npc_framework::agent_io::observe::metres(i64::from(x)),
        npc_framework::agent_io::observe::metres(i64::from(z))
    );
    if ctx.mode() == ControlMode::Advisor {
        return Ok(ActionPlan::Commands {
            label,
            commands: vec![SimCommand::Order {
                unit: ctx.unit.0,
                order: Order::GoTo { pos: (x, z) },
            }],
        });
    }
    Ok(ActionPlan::Queue {
        label,
        steps: vec![
            Step::SetBlackboard {
                key: BB_DEST.0,
                value: BbValue::Pos(goal.x, goal.z),
            },
            action("MoveToDest", ParamValue::None),
        ],
        est_ticks: Some(walk_ticks(world, ctx, goal)),
    })
}

fn eat(world: &World, ctx: &AgentCtx, _req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    need_villager(world, ctx)?;
    direct_only(ctx, "eat")?;
    let (pile, _) = find_food(world, ctx.unit)
        .ok_or_else(|| Rejection::new("unavailable", "no stockpile has food available"))?;
    Ok(ActionPlan::Queue {
        label: format!("Eat at the stockpile #{}", pile.0),
        steps: vec![
            action("ReserveFood", ParamValue::None),
            action("MoveToDest", ParamValue::None),
            action("EatFood", ParamValue::None),
        ],
        est_ticks: None,
    })
}

fn drink(world: &World, ctx: &AgentCtx, _req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    need_villager(world, ctx)?;
    direct_only(ctx, "drink")?;
    let (well, _) = find_well_for(world, ctx.entity)
        .ok_or_else(|| Rejection::new("unavailable", "no well has a free slot"))?;
    Ok(ActionPlan::Queue {
        label: format!("Drink at the well #{}", well.0),
        steps: vec![
            action("ReserveWell", ParamValue::None),
            action("MoveToDest", ParamValue::None),
            action("DrinkWater", ParamValue::None),
        ],
        est_ticks: None,
    })
}

fn sleep(world: &World, ctx: &AgentCtx, req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    let v = need_villager(world, ctx)?;
    direct_only(ctx, "sleep")?;
    let until = match req.str_arg("until") {
        None => SLEEP_FULL,
        Some(u) if u.eq_ignore_ascii_case(SLEEP_FULL) => SLEEP_FULL,
        Some(u) if u.eq_ignore_ascii_case(SLEEP_MORNING) => SLEEP_MORNING,
        Some(other) => {
            return Err(Rejection::new(
                "invalid_args",
                format!("until must be Full or Morning, not `{other}`"),
            ));
        }
    };
    let (bed, _) = find_bed(world, ctx.unit, v.home)
        .ok_or_else(|| Rejection::new("unavailable", "no free bed in your home"))?;
    Ok(ActionPlan::Queue {
        label: format!("Sleep in bed #{} until {until}", bed.0),
        steps: vec![
            action("ReserveBed", ParamValue::None),
            action("MoveToDest", ParamValue::None),
            action("SleepInBed", ParamValue::from(until)),
        ],
        est_ticks: None,
    })
}

fn work(world: &World, ctx: &AgentCtx, _req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    need_villager(world, ctx)?;
    direct_only(ctx, "work")?;
    let (site, _) = find_work_site(world, ctx.unit, false)
        .ok_or_else(|| Rejection::new("unavailable", "no free work site"))?;
    Ok(ActionPlan::Queue {
        label: format!("Work a shift at the field #{}", site.0),
        steps: vec![
            action("ReserveWork", ParamValue::None),
            action("MoveToDest", ParamValue::None),
            action("DoWork", ParamValue::from(WORK_ANY_TIME)),
        ],
        est_ticks: None,
    })
}

// ---------------------------------------------------------------- scenarios

fn first_home(world: &World) -> Option<SimId> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, e)| world.get::<Home>(**e).is_some())
        .map(|(&id, _)| id)
}

/// The village's scenario loader (see the module docs).
pub fn load_scenario(doc: &ScenarioDoc, seed: u64) -> Result<LoadedScenario, String> {
    let w = &doc.world;
    let layout = w.get("layout").and_then(Value::as_str).unwrap_or("village");
    let plots = w
        .get("plots")
        .and_then(Value::as_u64)
        .map_or(Ok(1), u32::try_from)
        .map_err(|_| "world.plots is too large".to_owned())?;
    let mut app = try_new_sim_app_with(seed, layout, plots, |app| {
        app.add_plugins((AgentIoPlugin::default(), VillageAgentPlugin));
    })?;
    let world = app.world_mut();
    if let Some(food) = w.get("stockpile_food").and_then(Value::as_u64) {
        let piles: Vec<Entity> = world.resource::<SimIndex>().map.values().copied().collect();
        let food = u32::try_from(food).map_err(|_| "world.stockpile_food is too large")?;
        for e in piles {
            if let Some(mut p) = world.get_mut::<Stockpile>(e) {
                if food > p.capacity {
                    return Err(format!(
                        "stockpile_food {food} exceeds capacity {}",
                        p.capacity
                    ));
                }
                p.food = food;
            }
        }
    }
    let tree = world.resource::<VillagerTree>().0;
    let mut actors = Vec::new();
    for (i, a) in doc.actors.iter().enumerate() {
        if a.kind != "villager" {
            return Err(format!(
                "actors[{i}]: unknown kind `{}` (village has: villager)",
                a.kind
            ));
        }
        let (x, z) = a.pos.ok_or_else(|| format!("actors[{i}]: needs pos"))?;
        let pos = check_pos("pos", (x, z)).map_err(|e| format!("actors[{i}]: {e}"))?;
        let home = match a.extra.get("home").and_then(Value::as_u64) {
            Some(h) => {
                let h = SimId(u32::try_from(h).map_err(|_| format!("actors[{i}]: bad home"))?);
                let ok = world
                    .resource::<SimIndex>()
                    .get(h)
                    .is_some_and(|e| world.get::<Home>(e).is_some());
                if !ok {
                    return Err(format!("actors[{i}]: {} is not a home", h.0));
                }
                h
            }
            None => first_home(world).ok_or_else(|| format!("actors[{i}]: no home to live in"))?,
        };
        let bundle = villager_bundle(world, home, pos, tree);
        let (id, e) = spawn_sim(world, bundle);
        if let Some(needs) = a.extra.get("needs").and_then(Value::as_object) {
            for (name, v) in needs {
                let value = v
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .filter(|v| (0..=NEED_MAX).contains(v))
                    .ok_or_else(|| format!("actors[{i}].needs.{name}: 0..={NEED_MAX}"))?;
                let registry = world.resource::<NeedRegistry>();
                let need_id = registry
                    .id(name)
                    .ok_or_else(|| format!("actors[{i}].needs: unknown need `{name}`"))?;
                let thresholds = registry.def(need_id).thresholds;
                if let Some(mut n) = world.get_mut::<Needs>(e) {
                    let need = n.get_mut(need_id);
                    need.value = value;
                    update_level(need, &thresholds);
                }
            }
        }
        actors.push(id);
    }
    app.insert_resource(ScenarioInfo {
        name: doc.info_name(),
        villagers: plots,
    });
    Ok(LoadedScenario { app, actors })
}
