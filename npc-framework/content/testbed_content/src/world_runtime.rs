//! Opt-in explorer mechanics. Presets are immutable construction data;
//! per-actor bases and collapse latches survive snapshots without new hashes.
use crate::actors::Body;
use crate::worlds::{WorldPreset, need_specs};
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{AgentCtx, AgentIoAppExt, Section};
use npc_framework::interaction::{Ctx, InteractionDef};
use npc_framework::needs::{NeedDef, NeedRegistry, NeedThresholds, Needs, PauseDecay};
use npc_framework::perception::{Sensor, Sensors};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Resource, Clone)]
pub struct ActiveWorld(pub WorldPreset);
#[derive(Resource)]
struct Applied;
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct WorldActor {
    pub body: Body,
    pub base_speed: i32,
    pub base_sensors: Sensors,
    pub collapsed: bool,
    pub depleted_mask: u8,
}
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RecoverySource {
    pub need: usize,
}
type RecoveryFns = (
    fn(&World, &Ctx) -> bool,
    fn(&World, &Ctx) -> Result<(), String>,
    fn(&mut World, &Ctx),
);

pub fn prepare(app: &mut App, p: &WorldPreset) {
    if app.world().contains_resource::<ActiveWorld>() {
        return;
    }
    app.insert_resource(ActiveWorld(p.clone()));
    app.world_mut().resource_mut::<SimConfig>().day_length = p.day_night.cycle_ticks;
    app.world_mut().resource_mut::<SimConfig>().night_permille =
        if p.day_night.on { 300 } else { 0 };
    app.register_snapshot_component_named::<WorldActor>("explorer_world_actor")
        .register_snapshot_component_named::<RecoverySource>("explorer_recovery_source")
        .register_verb_gate("testbed.world_rules", verb_gate)
        .register_observation("needs", needs_section)
        .register_describer("testbed.recovery_source", describe_source);
    for (i, (name, spec)) in need_specs(p).into_iter().enumerate() {
        if !spec.on {
            continue;
        }
        app.register_need(NeedDef {
            name,
            key: keys()[i],
            order: i as i32,
            decay_per_tick: spec.rise_milli_per_tick,
            thresholds: NeedThresholds {
                low_enter: (100_001 - spec.low_milli).min(100_000),
                low_exit: (102_000 - spec.low_milli).min(100_000),
                urgent_enter: 100_001 - spec.out_milli,
                urgent_exit: (102_000 - spec.out_milli).min(100_000),
            },
        });
        let (offer, check, effect): RecoveryFns = match i {
            0 => (offer_food, check_food, recover_food),
            1 => (offer_water, check_water, recover_water),
            2 => (offer_bed, check_bed, recover_bed),
            _ => (offer_charger, check_charger, recover_charger),
        };
        app.register_interaction(
            InteractionDef::new(verbs()[i], check, effect)
                .offer(offer)
                .duration(40)
                .reach(1500),
        );
    }
    if need_specs(p).iter().any(|(_, spec)| spec.on) || night_penalty(p) {
        app.add_systems(
            NpcSimTick,
            (
                update_state
                    .in_set(NpcSet::Sense)
                    .before(npc_framework::perception::NpcPerceptionSet),
                scale_movement
                    .in_set(NpcSet::Locomotion)
                    .before(NpcMoverSet),
            ),
        );
    }
}
fn night_penalty(p: &WorldPreset) -> bool {
    p.day_night.on && p.day_night.night_vision_pct != 100
}
fn keys() -> [&'static str; 4] {
    ["hunger", "thirst", "energy", "battery"]
}
fn verbs() -> [&'static str; 4] {
    ["eat", "drink", "rest", "charge"]
}
fn relevant(body: Body, i: usize) -> bool {
    match body {
        Body::Human => i < 3,
        Body::Robot => i == 3,
    }
}
pub fn enabled(world: &World, field: fn(&crate::worlds::SystemsSpec) -> bool) -> bool {
    world
        .get_resource::<ActiveWorld>()
        .is_none_or(|p| field(&p.0.systems))
}
pub fn apply(world: &mut World, played: SimId, p: &WorldPreset) {
    if world.contains_resource::<Applied>() {
        return;
    }
    world.insert_resource(Applied);
    let Some(entity) = world.resource::<SimIndex>().get(played) else {
        return;
    };
    let body = world
        .get_resource::<crate::actors::ActorPresets>()
        .and_then(|presets| presets.0.values().next())
        .map_or(Body::Human, |a| a.body);
    let has_needs = need_specs(p)
        .iter()
        .enumerate()
        .any(|(i, (_, s))| s.on && relevant(body, i));
    if has_needs || night_penalty(p) {
        let base_speed = world
            .get::<Commandable>(entity)
            .map_or(70, |c| c.speed_mm_per_tick);
        let base_sensors = world.get::<Sensors>(entity).cloned().unwrap_or_default();
        world.entity_mut(entity).insert(WorldActor {
            body,
            base_speed,
            base_sensors,
            collapsed: false,
            depleted_mask: 0,
        });
    }
    if has_needs {
        let registry = world.resource::<NeedRegistry>();
        let full = registry.full();
        let paused: Vec<_> = need_specs(p)
            .iter()
            .enumerate()
            .filter(|(i, (_, s))| s.on && !relevant(body, *i))
            .filter_map(|(_, (name, _))| registry.id(name))
            .collect();
        world
            .entity_mut(entity)
            .insert((full, PauseDecay::of(&paused)));
    }
    for (i, (name, spec)) in need_specs(p).into_iter().enumerate() {
        if !spec.on {
            continue;
        }
        let need = world
            .resource::<NeedRegistry>()
            .id(name)
            .expect("prepared need");
        let pos = crate::layout::cell(-3 + i as i32 * 2, -2);
        let kind = match i {
            0 | 1 => crate::world::PropKind::Chest,
            2 => crate::world::PropKind::Workbench,
            _ => crate::world::PropKind::Anvil,
        };
        let (id, _) = spawn_sim(
            world,
            (
                pos,
                crate::world::Prop::new(kind),
                RecoverySource { need: i },
                Provides {
                    need,
                    amount_per_tick: 2500,
                },
                Approach(pos),
                Interactable::new([verbs()[i]]),
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        world
            .resource_mut::<crate::TestbedIds>()
            .name(["food_source", "water_source", "bed", "charger"][i], id);
    }
    for (prefix, count, behaviour, template) in [
        (
            "worker",
            p.inhabitants.workers,
            crate::Behaviour::Worker,
            "woodcutter",
        ),
        (
            "skeleton",
            p.inhabitants.skeletons,
            crate::Behaviour::Wander,
            "skeleton_warrior",
        ),
    ] {
        for i in 0..count {
            let spec = crate::layout::UnitSpec {
                name: Some(format!("{prefix}_{i}")),
                template: template.into(),
                behaviour,
                equip: if prefix == "worker" {
                    vec![crate::content::AXE.into()]
                } else {
                    vec![]
                },
                ..Default::default()
            };
            let id =
                crate::layout::spawn_unit(world, crate::layout::cell(-5 + i32::from(i), -4), &spec)
                    .expect("validated built-in inhabitant");
            if night_penalty(p) {
                let entity = world
                    .resource::<SimIndex>()
                    .get(id)
                    .expect("spawned inhabitant");
                let base_speed = world
                    .get::<Commandable>(entity)
                    .expect("inhabitant commandable")
                    .speed_mm_per_tick;
                let base_sensors = world.get::<Sensors>(entity).cloned().unwrap_or_default();
                world.entity_mut(entity).insert(WorldActor {
                    body: Body::Human,
                    base_speed,
                    base_sensors,
                    collapsed: false,
                    depleted_mask: 0,
                });
            }
            world
                .resource_mut::<crate::TestbedIds>()
                .name(format!("{prefix}_{i}"), id);
            if prefix == "skeleton" {
                let patrol = IssueOrder {
                    player: PlayerId(1),
                    units: vec![id],
                    order: UnitOrder::Patrol {
                        points: vec![(-4500, -3500), (4500, -3500)],
                    },
                    queue: false,
                    formation: None,
                    skip_invalid: false,
                };
                // Constructor state belongs to the descriptor, so rebuilding a
                // replay must not enqueue a second external input for it.
                patrol.validate(world).expect("built-in patrol is valid");
                patrol.apply(world);
            }
        }
    }
}
fn badness(world: &World, e: Entity, name: &str) -> Option<i32> {
    let id = world.get_resource::<NeedRegistry>()?.id(name)?;
    Some(100_000 - world.get::<Needs>(e)?.get(id).value)
}
fn actor_specs(world: &World, e: Entity) -> Option<(&WorldActor, &WorldPreset)> {
    Some((
        world.get::<WorldActor>(e)?,
        &world.get_resource::<ActiveWorld>()?.0,
    ))
}
fn speed_pct(world: &World, e: Entity) -> i32 {
    let Some((actor, p)) = actor_specs(world, e) else {
        return 100;
    };
    if actor.collapsed {
        return 0;
    }
    need_specs(p)
        .iter()
        .enumerate()
        .filter(|(i, (_, s))| s.on && relevant(actor.body, *i))
        .filter(|(_, (name, s))| badness(world, e, name).is_some_and(|v| v >= s.low_milli))
        .map(|(_, (_, s))| i32::from(s.low_speed_pct))
        .min()
        .unwrap_or(100)
}
pub(crate) fn update_state(world: &mut World) {
    let p = world.resource::<ActiveWorld>().0.clone();
    let night = world
        .resource::<SimConfig>()
        .is_night(world.resource::<SimClock>().tick);
    let mut actors: Vec<_> = world
        .query::<(Entity, &SimId, &WorldActor)>()
        .iter(world)
        .map(|(e, id, a)| (*id, e, a.clone()))
        .collect();
    actors.sort_by_key(|(id, _, _)| *id);
    for (id, e, mut actor) in actors {
        let before_mask = actor.depleted_mask;
        for (i, (name, s)) in need_specs(&p)
            .iter()
            .enumerate()
            .filter(|(i, (_, s))| s.on && relevant(actor.body, *i))
        {
            if let Some(value) = badness(world, e, name) {
                if value >= s.out_milli {
                    actor.depleted_mask |= 1 << i;
                } else if value < s.low_milli {
                    actor.depleted_mask &= !(1 << i);
                }
            }
        }
        let collapsed = actor.depleted_mask != 0;
        if before_mask != actor.depleted_mask {
            let depleted: Vec<_> = need_specs(&p)
                .iter()
                .enumerate()
                .filter(|(i, _)| actor.depleted_mask & (1 << i) != 0)
                .map(|(_, (name, _))| *name)
                .collect();
            record_effect(
                world,
                Some(id),
                "ExplorerNeedState",
                &json!({"collapsed":collapsed,"depleted":depleted,"reason":if collapsed {if actor.body==Body::Robot {"battery empty"} else {"too exhausted"}} else {"restored below low threshold"}}),
            );
        }
        if collapsed && !actor.collapsed {
            npc_framework::agent_io::control::cancel_all(world, e);
            npc_framework::command::cancel_orders(world, e, "collapsed");
        }
        actor.collapsed = collapsed;
        let mut sensors = actor.base_sensors.clone();
        let pct = if night {
            i32::from(p.day_night.night_vision_pct)
        } else {
            100
        };
        for sensor in &mut sensors.sensors {
            match sensor {
                Sensor::Vision(v) => {
                    v.range_mm = (i64::from(v.range_mm) * i64::from(pct) / 100) as i32
                }
                Sensor::Camera(c) => {
                    c.range_mm = (i64::from(c.range_mm) * i64::from(pct) / 100) as i32
                }
                _ => {}
            }
        }
        world.entity_mut(e).insert((actor.clone(), sensors));
        let speed = actor.base_speed * speed_pct(world, e) / 100;
        if let Some(mut unit) = world.get_mut::<crate::TestbedUnit>(e) {
            unit.speed_mm_per_tick = speed;
        }
        if let Some(mut command) = world.get_mut::<Commandable>(e) {
            command.speed_mm_per_tick = speed;
        }
    }
}
pub(crate) fn scale_movement(world: &mut World) {
    let mut actors: Vec<_> = world
        .query::<(Entity, &SimId, &WorldActor)>()
        .iter(world)
        .map(|(e, id, a)| (*id, e, a.base_speed))
        .collect();
    actors.sort_by_key(|(id, _, _)| *id);
    for (_, e, base) in actors {
        let speed = base * speed_pct(world, e) / 100;
        if let Some(mut intent) = world.get_mut::<MoveIntent>(e) {
            intent.speed_mm_per_tick = speed;
        }
    }
}
/// Recovery is intrinsic only for an enabled need of this body.
pub fn intrinsic_recovery(world: &World, e: Entity, verb: &str) -> bool {
    let Some((a, p)) = actor_specs(world, e) else {
        return false;
    };
    verbs()
        .iter()
        .position(|v| *v == verb)
        .is_some_and(|i| need_specs(p)[i].1.on && relevant(a.body, i))
}
pub fn verb_gate(world: &World, e: Entity, verb: &str) -> Option<String> {
    let p = &world.get_resource::<ActiveWorld>()?.0;
    if (!p.systems.crafting && verb.eq_ignore_ascii_case("craft"))
        || (!p.systems.locked_doors && matches!(verb, "lock" | "unlock"))
        || (!p.systems.vehicles && matches!(verb, "mount" | "dismount" | "drive_to" | "unload"))
        || (!p.systems.progression && matches!(verb, "study" | "teach" | "spend_points"))
    {
        return Some("this system is disabled in this world".into());
    }
    let actor = world.get::<WorldActor>(e)?;
    if actor.collapsed
        && !matches!(
            verb,
            "rest" | "charge" | "order" | "interact" | "execute_orders"
        )
    {
        Some(
            if actor.body == Body::Robot {
                "battery empty"
            } else {
                "too exhausted"
            }
            .into(),
        )
    } else {
        None
    }
}
fn source_ok(world: &World, ctx: &Ctx, i: usize) -> bool {
    world
        .get::<RecoverySource>(ctx.target)
        .is_some_and(|s| s.need == i)
        && intrinsic_recovery(world, ctx.actor, verbs()[i])
}
fn check(world: &World, ctx: &Ctx, i: usize) -> Result<(), String> {
    if source_ok(world, ctx, i) {
        if world
            .get::<WorldActor>(ctx.actor)
            .is_some_and(|a| a.collapsed)
        {
            let here = world.get::<SimPos>(ctx.actor).copied();
            let there = npc_framework::jobs::approach_of(world, ctx.target);
            if !matches!((here,there),(Some(a),Some(b)) if a.dist2(b)<=1500*1500) {
                return Err("recovery while collapsed requires being at the bed or charger".into());
            }
        }
        Ok(())
    } else {
        Err("this source cannot restore this body's need".into())
    }
}
fn recover(world: &mut World, ctx: &Ctx, i: usize) {
    if !source_ok(world, ctx, i) {
        return;
    }
    let Some(provides) = world.get::<Provides>(ctx.target).copied() else {
        return;
    };
    if let Some(mut needs) = world.get_mut::<Needs>(ctx.actor) {
        let value = needs.get(provides.need).value;
        needs.get_mut(provides.need).value = (value + provides.amount_per_tick * 40).min(100_000);
    }
}
macro_rules! recovery {
    ($offer:ident,$check:ident,$effect:ident,$i:expr) => {
        fn $offer(w: &World, c: &Ctx) -> bool {
            source_ok(w, c, $i)
        }
        fn $check(w: &World, c: &Ctx) -> Result<(), String> {
            check(w, c, $i)
        }
        fn $effect(w: &mut World, c: &Ctx) {
            recover(w, c, $i)
        }
    };
}
recovery!(offer_food, check_food, recover_food, 0);
recovery!(offer_water, check_water, recover_water, 1);
recovery!(offer_bed, check_bed, recover_bed, 2);
recovery!(offer_charger, check_charger, recover_charger, 3);
fn needs_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let (actor, p) = actor_specs(world, ctx.entity)?;
    let mut values = serde_json::Map::new();
    let mut lines = Vec::new();
    for (i, (name, s)) in need_specs(p).into_iter().enumerate() {
        if !s.on || !relevant(actor.body, i) {
            continue;
        }
        let value = badness(world, ctx.entity, name)?;
        let level = if value >= s.out_milli {
            "out"
        } else if value >= s.low_milli {
            "low"
        } else {
            "ok"
        };
        values.insert(
            keys()[i].into(),
            json!({"value":value,"level":level,"rise_milli_per_tick":s.rise_milli_per_tick}),
        );
        lines.push(format!("{}: {} ({level}).", keys()[i], value));
    }
    if values.is_empty() {
        return None;
    }
    values.insert("collapsed".into(), actor.collapsed.into());
    values.insert("speed_pct".into(), speed_pct(world, ctx.entity).into());
    let mut section = Section::new(values.into());
    section.lines = lines;
    Some(section)
}
fn describe_source(world: &World, id: SimId) -> Option<String> {
    let e = world.resource::<SimIndex>().get(id)?;
    let source = world.get::<RecoverySource>(e)?;
    Some(format!(
        "{} #{}",
        ["food supply", "water supply", "bed", "charger"][source.need],
        id.0
    ))
}
