//! Shared recorded restoration work. Ecology owns the effect; this adapter owns
//! work duration, reach and exact physical material claims.
use super::ecology::{self, EcologyWorkKind};
use crate::village::{Villager, economy, homestead::BuildMaterial, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::agent_io::observe::known_targets;
use npc_framework::agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, Rejection, Section,
};
use npc_framework::interaction::{
    CarriedBy, InteractionAppExt, Item,
    items::{self, entity_of},
};
use npc_framework::kernel::{commands::CommandError, journal::ReleaseReason};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const VERBS: &[&str] = &["plant_tree", "restore_habitat"];
pub const WORK_TICKS: u64 = 240;

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct EcologyWork {
    pub target: SimId,
    pub verb: String,
    pub ends: u64,
    pub materials: Vec<BuildMaterial>,
}
impl SimHash for EcologyWork {
    const NAME: &'static str = "living_ecology_work_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.target.0);
        h.str(&self.verb);
        h.u64(self.ends);
        h.u32(self.materials.len() as u32);
        for m in &self.materials {
            h.u32(m.item.0);
            h.str(&m.kind);
            h.u32(m.count);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierAct {
    pub unit: SimId,
    pub target: SimId,
    pub verb: String,
}

fn kind(verb: &str) -> Result<EcologyWorkKind, String> {
    match verb {
        "plant_tree" => Ok(EcologyWorkKind::PlantTree),
        "restore_habitat" => Ok(EcologyWorkKind::RestoreHabitat),
        _ => Err("unknown restoration action".into()),
    }
}
pub fn busy(world: &World, actor: Entity) -> Option<String> {
    world
        .get::<EcologyWork>(actor)
        .map(|_| "restoring the landscape; finish or stop_work first".into())
}
pub fn register(app: &mut App) {
    app.register_sim_component::<EcologyWork>()
        .register_command::<FrontierAct>()
        .register_busy("living.ecology_work", busy)
        .add_busy_check("living.ecology_work", busy)
        .register_observation("living_world", observe)
        .register_affordances("living.ecology_work", affordances);
    for verb in VERBS {
        app.register_action_handler(verb, handle);
    }
}
fn check_site(world: &World, actor: Entity, target: SimId, verb: &str) -> Result<(), String> {
    if !living(world, actor) || world.get::<Villager>(actor).is_none() {
        return Err("restoration requires a living resident in the regional world".into());
    }
    let site = entity_of(world, target).ok_or("resource site is missing")?;
    if !economy::near(world, actor, site) {
        return Err("move within 1.5 m of the resource site".into());
    }
    ecology::check_work(world, actor, target, kind(verb)?)
}
impl FrontierAct {
    pub fn check(&self, world: &World) -> Result<(), String> {
        let actor = entity_of(world, self.unit).ok_or("resident is missing")?;
        check_site(world, actor, self.target, &self.verb)?;
        // The action may be applying from an external queue; that queue's own
        // busy flag must not reject its work. Only existing physical work conflicts.
        if world.get::<crate::village::Gathering>(actor).is_some()
            || world
                .get::<crate::village::homestead::CottageBuild>(actor)
                .is_some()
            || world
                .get::<crate::village::hunting::Hunting>(actor)
                .is_some()
            || world
                .get::<npc_framework::crafting::Crafting>(actor)
                .is_some()
            || world
                .get::<npc_framework::interaction::Interacting>(actor)
                .is_some()
            || world
                .get::<Villager>(actor)
                .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick)
            || busy(world, actor).is_some()
        {
            return Err("finish or stop the current work before restoring land".into());
        }
        if world.resource::<Reservations>().count(self.target) > 0 {
            return Err("this site is already claimed by work".into());
        }
        for (name, count) in ecology::work_inputs(world, kind(&self.verb)?) {
            economy::selected(world, actor, name, count)?;
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = entity_of(world, self.unit).expect("checked actor");
        let mut materials = Vec::new();
        for (name, count) in ecology::work_inputs(world, kind(&self.verb)?) {
            for (item, selected) in economy::selected(world, actor, name, count)? {
                materials.push(BuildMaterial {
                    item,
                    kind: name.into(),
                    count: selected.stack,
                });
            }
        }
        materials.sort_by_key(|m| m.item);
        let work = EcologyWork {
            target: self.target,
            verb: self.verb.clone(),
            ends: world.resource::<SimClock>().tick.saturating_add(WORK_TICKS),
            materials,
        };
        let claims = claims(&work);
        for (i, claim) in claims.iter().copied().enumerate() {
            if !npc_framework::jobs::reserve(world, self.unit, claim, 1) {
                for previous in claims.iter().copied().take(i) {
                    release(world, self.unit, previous, ReleaseReason::Failed);
                }
                return Err("restoration materials or site were claimed by other work".into());
            }
        }
        npc_framework::command::cancel_orders(world, actor, "restoring the landscape");
        world.entity_mut(actor).remove::<MoveIntent>().insert(work);
        record_effect(
            world,
            Some(self.unit),
            "LandscapeWorkStarted",
            &json!({"target":self.target,"verb":self.verb}),
        );
        Ok(())
    }
}
impl ContentCommand for FrontierAct {
    const NAME: &'static str = "FrontierAct";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
    fn unit(&self) -> Option<SimId> {
        Some(self.unit)
    }
}
fn claims(work: &EcologyWork) -> Vec<SimId> {
    let mut result: Vec<_> = work
        .materials
        .iter()
        .map(|m| m.item)
        .chain([work.target])
        .collect();
    result.sort_unstable();
    result.dedup();
    result
}
fn release(world: &mut World, actor: SimId, target: SimId, reason: ReleaseReason) {
    let held = world.resource::<Reservations>().holds(target, actor);
    world.resource_mut::<Reservations>().release(target, actor);
    if held {
        npc_framework::kernel::journal::record(world, Some(actor), || JournalKind::Release {
            target,
            reason,
        });
    }
}
pub fn cancel(world: &mut World, actor: Entity) {
    let Some(work) = world.entity_mut(actor).take::<EcologyWork>() else {
        return;
    };
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return;
    };
    for claim in claims(&work) {
        release(world, id, claim, ReleaseReason::Aborted);
    }
    record_effect(
        world,
        Some(id),
        "LandscapeWorkCancelled",
        &json!({"target":work.target,"verb":work.verb}),
    );
}
/// Structural and ownership validation for persisted work. A changed habitat can
/// still invalidate completion without making an otherwise sound save unloadable.
pub fn validate_work(world: &World, actor: Entity, work: &EcologyWork) -> Result<(), String> {
    let me = *world.get::<SimId>(actor).ok_or("resident missing")?;
    if !living(world, actor) || world.get::<Villager>(actor).is_none() {
        return Err("restoration work has no living resident".into());
    }
    let expected: std::collections::BTreeMap<_, _> = ecology::work_inputs(world, kind(&work.verb)?)
        .into_iter()
        .map(|(name, count)| (name.to_owned(), count))
        .collect();
    let mut actual = std::collections::BTreeMap::<String, u32>::new();
    if work
        .materials
        .windows(2)
        .any(|pair| pair[0].item >= pair[1].item)
        || work
            .materials
            .iter()
            .any(|m| m.count == 0 || m.item == work.target)
    {
        return Err("restoration needs unique, ordered, positive material claims".into());
    }
    for material in &work.materials {
        let count = actual.entry(material.kind.clone()).or_default();
        *count = count
            .checked_add(material.count)
            .ok_or("restoration material count overflows")?;
    }
    if actual != expected {
        return Err("restoration materials do not match the action's exact recipe".into());
    }
    if entity_of(world, work.target)
        .is_none_or(|e| world.get::<ecology::EcologicalPatch>(e).is_none())
    {
        return Err("restoration target is not an ecological site".into());
    }
    if !world.resource::<Reservations>().holds(work.target, me) {
        return Err("site reservation was lost".into());
    }
    for material in &work.materials {
        let item = entity_of(world, material.item).ok_or("material was lost")?;
        if !world.resource::<Reservations>().holds(material.item, me)
            || world
                .get::<CarriedBy>(item)
                .is_none_or(|held| held.holder != me)
            || world
                .get::<Item>(item)
                .is_none_or(|item| item.kind != material.kind || item.stack < material.count)
        {
            return Err("restoration materials or ownership changed".into());
        }
    }
    Ok(())
}
fn finish_check(world: &World, actor: Entity, work: &EcologyWork) -> Result<(), String> {
    validate_work(world, actor, work)?;
    check_site(world, actor, work.target, &work.verb)
}
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let work: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| {
            world
                .get::<EcologyWork>(*e)
                .cloned()
                .map(|work| (*id, *e, work))
        })
        .collect();
    for (id, actor, work) in work {
        if !living(world, actor) {
            cancel(world, actor);
            continue;
        }
        if now < work.ends {
            continue;
        }
        if finish_check(world, actor, &work).is_err() {
            cancel(world, actor);
            continue;
        }
        world.entity_mut(actor).remove::<EcologyWork>();
        for claim in claims(&work) {
            release(world, id, claim, ReleaseReason::Finished);
        }
        for material in &work.materials {
            let e = entity_of(world, material.item).expect("checked material");
            let left = world.get::<Item>(e).expect("item").stack - material.count;
            if left == 0 {
                items::take_from_holder(world, e);
                world.despawn(e);
            } else {
                world.get_mut::<Item>(e).expect("item").stack = left;
            }
        }
        ecology::complete_work(
            world,
            id,
            work.target,
            kind(&work.verb).expect("checked verb"),
        )
        .expect("same-tick validated ecology completion");
    }
}
fn handle(world: &World, ctx: &AgentCtx, request: &ActionRequest) -> Result<ActionPlan, Rejection> {
    if request
        .args
        .as_object()
        .is_some_and(|args| !args.is_empty())
    {
        return Err(Rejection::new(
            "invalid_args",
            "restoration takes a target and no additional arguments",
        ));
    }
    let target = request
        .target
        .ok_or_else(|| Rejection::new("invalid_args", "choose a resource site"))?;
    if known_targets(world, ctx).is_some_and(|known| !known.contains(&target)) {
        return Err(Rejection::new(
            "not_perceived",
            "discover this site before restoring it",
        ));
    }
    let command = FrontierAct {
        unit: ctx.unit,
        target,
        verb: request.verb.clone(),
    };
    command
        .check(world)
        .map_err(|reason| Rejection::new("unavailable", reason))?;
    Ok(ActionPlan::Commands {
        label: request.verb.clone(),
        commands: vec![SimCommand::content(&command)],
    })
}
fn local_sites(world: &World, ctx: &AgentCtx) -> Vec<(SimId, Entity)> {
    let known = known_targets(world, ctx);
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(id, e)| {
            known.as_ref().is_none_or(|known| known.contains(id))
                && world.get::<ecology::EcologicalPatch>(**e).is_some()
                && economy::near(world, ctx.entity, **e)
        })
        .map(|(id, e)| (*id, *e))
        .collect()
}
fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let mut result = Vec::new();
    for (id, _) in local_sites(world, ctx) {
        for verb in VERBS {
            let command = FrontierAct {
                unit: ctx.unit,
                target: id,
                verb: (*verb).into(),
            };
            let mut affordance = Affordance::new(
                verb,
                if *verb == "plant_tree" {
                    "Plant a tree cutting"
                } else {
                    "Restore habitat"
                },
            )
            .target(id)
            .est_ticks(WORK_TICKS as u32);
            if let Err(reason) = command.check(world) {
                affordance.ok = false;
                affordance.why_not = Some(reason);
            }
            result.push(affordance);
        }
    }
    result
}
fn observe(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let spec = super::spec(world)?;
    let map_enabled = npc_framework::agent_io::map::enabled(world);
    // A published chart is immutable construction information, never live foreign stock.
    let destinations: Vec<_> = spec
        .settlements
        .iter()
        .filter(|_| map_enabled)
        .map(|s| {
            json!({"key":s.key,"name":s.label,
        "position":{"x":s.center.x,"z":s.center.z},"biome":s.biome})
        })
        .collect();
    // The good that needs its own tool to gather (fish and a rod), from data.
    let fishing = super::economy_spec::registry(world)
        .and_then(|registry| {
            registry.trade_kinds().iter().find_map(|kind| {
                let good = registry.good(kind)?;
                good.gather_tool_required
                    .then(|| json!({"catch":kind,"tool":good.gather_tool,"verb":"gather"}))
            })
        })
        .unwrap_or(serde_json::Value::Null);
    let sites: Vec<_> = local_sites(world, ctx).into_iter().filter_map(|(id, e)| {
        let p = world.get::<crate::village::ResourcePatch>(e)?;
        let ecology = world.get::<ecology::EcologicalPatch>(e)?;
        let pos = world.get::<SimPos>(e)?;
        let actions: Vec<_> = VERBS.iter().map(|verb| {
            let check = FrontierAct { unit:ctx.unit,target:id,verb:(*verb).into() }.check(world);
            json!({"verb":verb,"available":check.is_ok(),"reason":check.err(),
                "inputs":ecology::work_inputs(world, kind(verb).expect("declared verb")),"ticks":WORK_TICKS})
        }).collect();
        let now=world.resource::<SimClock>().tick;
        let day=world.resource::<SimConfig>().day_length.max(1);
        let growth_hours=ecology.pending_growth.iter().map(|g|g.matures_at).min()
            .map(|tick|tick.saturating_sub(now).saturating_mul(24).div_ceil(day));
        Some(json!({"id":id.0,"kind":p.kind,"remaining":p.remaining,"capacity":p.capacity,
            "vitality_percent":ecology.biomass_milli/1000,"soil_percent":ecology.soil_milli/1000,
            "disturbance_percent":ecology.disturbance_milli/1000,"plantings":ecology.pending_growth.len(),"growth_hours":growth_hours,
            "position":{"x":pos.x,"z":pos.z},"actions":actions}))
    }).collect();
    Some(Section::new(
        json!({"chart":destinations,"width_mm":map_enabled.then(||spec.width_cells.saturating_mul(1000)),
        "height_mm":map_enabled.then(||spec.height_cells.saturating_mul(1000)),"nearby_sites":sites,
        "fishing":fishing,
        "work":world.get::<EcologyWork>(ctx.entity)}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_disabled_removes_chart_but_keeps_local_ecology_and_actions() {
        let mut app = crate::new_sim_app(7, "living_world");
        let unit = app
            .world()
            .resource::<crate::village::VillageState>()
            .played;
        let entity = app.world().resource::<SimIndex>().get(unit).unwrap();
        crate::village::colony::JoinIsland { unit: unit.0 }
            .perform(app.world_mut())
            .unwrap();
        let source = app
            .world()
            .resource::<SimIndex>()
            .map
            .values()
            .copied()
            .find(|e| app.world().get::<ecology::EcologicalPatch>(*e).is_some())
            .unwrap();
        let at = *app.world().get::<SimPos>(source).unwrap();
        app.world_mut().entity_mut(entity).insert(at);
        // A fixture relocation is not perception: let the ordinary sensors
        // discover the nearby site before testing the map presentation switch.
        run_ticks(app.world_mut(), 4);
        let context = AgentCtx {
            unit,
            entity,
            controller: None,
            planned: &[],
        };
        assert!(
            !observe(app.world(), &context).unwrap().value["nearby_sites"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            observe(app.world(), &context).unwrap().value["chart"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        app.world_mut()
            .insert_resource(npc_framework::agent_io::map::MapEnabled(false));
        let hidden = observe(app.world(), &context).unwrap().value;
        assert!(hidden["chart"].as_array().unwrap().is_empty());
        assert!(hidden["width_mm"].is_null() && hidden["height_mm"].is_null());
        assert!(!hidden["nearby_sites"].as_array().unwrap().is_empty());
        assert!(hidden["nearby_sites"][0]["actions"].is_array());
    }
}
