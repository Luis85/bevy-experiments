use super::*;
use npc_framework::agent_io::observe::known_targets;
use npc_framework::agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, Rejection, Section,
};
use serde_json::Value;

pub(super) fn owned_known(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    plots(world)
        .into_iter()
        .filter(|(_, plot)| {
            world
                .get::<HomesteadPlot>(*plot)
                .is_some_and(|estate| estate.owner == Some(ctx.unit))
        })
        .map(|(id, _)| id)
        .collect()
}

pub fn describe(world: &World, id: SimId) -> Option<String> {
    let entity = entity_of(world, id)?;
    if let Some(estate) = world.get::<HomesteadPlot>(entity) {
        let label =
            definition(world, entity).map_or("Homestead plot", |definition| definition.label);
        return Some(format!("{label} ({}) #{}", estate.plot_id, id.0));
    }
    world
        .get::<Cottage>(entity)
        .map(|_| format!("Cottage #{}", id.0))
}

/// Full operator projection when state_known=true; scoped observation can retain
/// learned static layout without exposing an unseen foreign owner's live estate.
pub fn plot_json(world: &World, plot: Entity, state_known: bool) -> Option<Value> {
    let id = *world.get::<SimId>(plot)?;
    let center = *world.get::<SimPos>(plot)?;
    let estate = world.get::<HomesteadPlot>(plot)?;
    let definition = definition(world, plot)?;
    let now = world.resource::<SimClock>().tick;
    let owner_name = estate
        .owner
        .and_then(|id| entity_of(world, id))
        .and_then(|entity| world.get::<Villager>(entity))
        .map(|v| v.name.as_str());
    let beds:Vec<_>=estate.beds.iter().enumerate().map(|(index,bed)| {
        let crop=state_known.then_some(bed.as_ref()).flatten();
        let stage=if !state_known { "unknown" } else { crop_stage(crop,now) };
        json!({"index":index,"position":bed_position(world,plot,index as u32),
            "kind":crop.map(|crop|crop.kind.as_str()),"stage":stage,
            "planted_at":crop.map(|crop|crop.planted_at),"watered_at":crop.and_then(|crop|crop.watered_at),
            "ready_at":crop.and_then(|crop|crop.ready_at)})
    }).collect();
    Some(
        json!({"id":id,"kind":"homestead_plot","name":definition.label,"plot_id":estate.plot_id,
        "position":center,"use_kind":definition.use_kind,"state_known":state_known,
        "owner":if state_known {estate.owner}else{None},"owner_name":if state_known {owner_name}else{None},
        "cottage_built":state_known.then_some(estate.cottage_built),"cottage_center":cottage_center(center),
        "front_door":front_door(center),"beds":beds}),
    )
}

fn current(world: &World, ctx: &AgentCtx, id: SimId, plot: Entity) -> bool {
    economy::near(world, ctx.entity, plot)
        || world
            .get::<PerceptionMemory>(ctx.entity)
            .and_then(|memory| memory.get(id))
            .is_some_and(|percept| percept.sensed)
}

pub fn observe(world: &World, ctx: &AgentCtx) -> Option<Section> {
    if !geography::is_expanded(world) || world.get::<Villager>(ctx.entity).is_none() {
        return None;
    }
    let known = known_targets(world, ctx);
    let mut rows = Vec::new();
    for (id, plot) in plots(world) {
        let estate = world.get::<HomesteadPlot>(plot).expect("plot");
        let own = estate.owner == Some(ctx.unit);
        if !own && known.as_ref().is_some_and(|known| !known.contains(&id)) {
            continue;
        }
        let sensed = current(world, ctx, id, plot);
        if let Some(mut row) = plot_json(world, plot, own || sensed) {
            row["own"] = json!(own);
            row["current"] = json!(sensed);
            rows.push(row);
        }
    }
    let building = world
        .get::<CottageBuild>(ctx.entity)
        .map(|build| json!({"plot":build.plot,"ends":build.ends}));
    Some(Section::new(json!({"plots":rows,"building":building,
        "cottage":{"inputs":[{"kind":"plank","count":PLANK_COST},{"kind":"log","count":LOG_COST}],"duration_ticks":BUILD_TICKS},
        "crop":{"seed_kind":catalog::GRAIN,"seed_count":SEED_COUNT,"water_kind":catalog::WATER,"water_count":WATER_COUNT,
            "growth_ticks":GROW_TICKS,"yield_kind":catalog::GRAIN,"yield_count":HARVEST_GRAIN},
        "claim_limits":{"housing":MAX_HOUSING_PLOTS,"garden":MAX_GARDEN_PLOTS}}))
        .line("Homesteads: claim one housing and one garden plot. A cottage costs 8 planks + 4 logs and 600 ticks; it changes your home without moving you. Plant one real grain seed, water once with one clean water, then harvest three grain after 6000 ticks. Use each advertised bed position and its bed index; all work requires reach. stop_work cancels construction without spending its reserved materials."))
}

fn checked(mut affordance: Affordance, result: Result<(), String>) -> Affordance {
    if let Err(reason) = result {
        affordance.ok = false;
        affordance.why_not = Some(reason);
    }
    affordance
}

pub fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    if !geography::is_expanded(world)
        || world.get::<Villager>(ctx.entity).is_none()
        || !living(world, ctx.entity)
    {
        return Vec::new();
    }
    let known = known_targets(world, ctx);
    let mut result = Vec::new();
    for (id, plot) in plots(world) {
        let estate = world.get::<HomesteadPlot>(plot).expect("plot");
        let own = estate.owner == Some(ctx.unit);
        if !own && known.as_ref().is_some_and(|known| !known.contains(&id)) {
            continue;
        }
        let Some(definition) = definition(world, plot) else {
            continue;
        };
        let center = *world.get::<SimPos>(plot).expect("plot position");
        if !own {
            let mut affordance = Affordance::new(
                "claim_plot",
                format!(
                    "Claim {} if unowned or its owner has died",
                    definition.label
                ),
            )
            .target(id);
            affordance.pos = Some((center.x, center.z));
            result.push(checked(
                affordance,
                HomesteadAct::new(ctx.unit, "claim_plot", id).check(world),
            ));
            continue;
        }
        if definition.use_kind == "housing" && !estate.cottage_built {
            let mut affordance = Affordance::new(
                "build_cottage",
                format!("Build a cottage on {}: 8 planks + 4 logs", definition.label),
            )
            .target(id)
            .est_ticks(BUILD_TICKS)
            .requires("plank x8")
            .requires("log x4");
            let door = front_door(center);
            affordance.pos = Some((door.x, door.z));
            result.push(checked(
                affordance,
                HomesteadAct::new(ctx.unit, "build_cottage", id).check(world),
            ));
        }
        for (verb, label) in [
            ("plant", "Plant one grain seed"),
            ("water_crop", "Water one planted bed"),
            ("harvest", "Harvest three grain from a mature bed"),
        ] {
            let mut candidates = Vec::new();
            for bed in 0..estate.beds.len() as u32 {
                let mut command = HomesteadAct::new(ctx.unit, verb, id);
                command.bed = bed;
                candidates.push((bed, command.check(world)));
            }
            let selected = candidates
                .iter()
                .find(|(_, check)| check.is_ok())
                .map_or(0, |(bed, _)| *bed);
            let check = candidates
                .iter()
                .find(|(bed, _)| *bed == selected)
                .map(|(_, check)| check.clone())
                .unwrap_or_else(|| Err("this plot has no crop beds".into()));
            let mut affordance=Affordance::new(verb,format!("{label} on {}; approach the chosen bed position",definition.label))
                .target(id).arg("bed",&format!("0..{}; default 0. See homestead.plots[].beds[].position for exact approach coordinates",estate.beds.len().saturating_sub(1)));
            if verb == "plant" {
                affordance = affordance.arg("kind", "village_grain; default village_grain");
            }
            if let Some(pos) = bed_position(world, plot, selected) {
                affordance.pos = Some((pos.x, pos.z));
            }
            result.push(checked(affordance, check));
        }
    }
    result
}

pub fn handle(
    world: &World,
    ctx: &AgentCtx,
    request: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    for key in request
        .args
        .as_object()
        .into_iter()
        .flat_map(|object| object.keys())
    {
        let valid = key == "bed"
            && matches!(request.verb.as_str(), "plant" | "water_crop" | "harvest")
            || key == "kind" && request.verb == "plant";
        if !valid {
            return Err(Rejection::new(
                "invalid_args",
                format!("unknown homestead argument `{key}`"),
            ));
        }
    }
    let target = request
        .target
        .ok_or_else(|| Rejection::new("invalid_args", "homestead work requires a plot target"))?;
    if known_targets(world, ctx)
        .as_ref()
        .is_some_and(|known| !known.contains(&target))
    {
        return Err(Rejection::new(
            "not_perceived",
            "the plot is not perceived, remembered or owned",
        ));
    }
    let mut command = HomesteadAct::new(ctx.unit, &request.verb, target);
    command.bed = match request.opt_int("bed")? {
        None => 0,
        Some(bed) => u32::try_from(bed)
            .map_err(|_| Rejection::new("invalid_args", "bed must be a nonnegative index"))?,
    };
    if let Some(kind) = request.opt_str("kind")? {
        command.kind = kind.into();
    }
    command
        .check(world)
        .map_err(|reason| Rejection::new("unavailable", reason))?;
    Ok(ActionPlan::Commands {
        label: format!("Homestead {}", request.verb),
        commands: vec![SimCommand::content(&command)],
    })
}
