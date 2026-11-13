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
    let mut value = json!({"id":id,"kind":"homestead_plot","name":definition.label,"plot_id":estate.plot_id,
        "position":center,"use_kind":definition.use_kind,"state_known":state_known,
        "owner":if state_known {estate.owner}else{None},"owner_name":if state_known {owner_name}else{None},
        "cottage_built":state_known.then_some(estate.cottage_built),"cottage_center":cottage_center(center),
        "front_door":front_door(center),"beds":beds});
    // The living world's cottage is a catalogue body behind the door (and
    // grows with the larger-cottage tier): report the body there is, or
    // would be, instead of the old fixed offset.
    {
        let built = estate
            .cottage_entity
            .and_then(|id| entity_of(world, id))
            .and_then(|cottage| super::built_body(world, cottage));
        let (body_center, body) = built.unwrap_or_else(|| {
            let (centre, body, _) = super::cottage_body(world, front_door(center), None);
            (centre, body)
        });
        value["cottage_center"] = json!(body_center);
        value["cottage_half_extents_mm"] = json!([body.half_x_mm, body.half_z_mm]);
    }
    value["owner_alive"] = json!(if state_known {
        estate
            .owner
            .map(|owner| entity_of(world, owner).is_some_and(|entity| living(world, entity)))
    } else {
        None
    });
    Some(value)
}

fn current(world: &World, ctx: &AgentCtx, id: SimId, plot: Entity) -> bool {
    economy::near(world, ctx.entity, plot)
        || world
            .get::<PerceptionMemory>(ctx.entity)
            .and_then(|memory| memory.get(id))
            .is_some_and(|percept| percept.sensed)
}

pub fn observe(world: &World, ctx: &AgentCtx) -> Option<Section> {
    if world.get::<Villager>(ctx.entity).is_none() {
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
    let mut value = json!({"plots":rows,"building":building,
        "cottage":{"inputs":[{"kind":super::super::roles::roles(world).lumber,"count":PLANK_COST},{"kind":super::super::roles::roles(world).timber,"count":LOG_COST}],"duration_ticks":BUILD_TICKS},
        "crop":{"seed_kind":super::super::roles::roles(world).grain,"seed_count":SEED_COUNT,"water_kind":super::super::roles::roles(world).drink,"water_count":WATER_COUNT,
            "growth_ticks":GROW_TICKS,"yield_kind":super::super::roles::roles(world).grain,"yield_count":HARVEST_GRAIN},
        "claim_limits":{"housing":MAX_HOUSING_PLOTS,"garden":MAX_GARDEN_PLOTS}});
    value["crops"] = json!(crop_catalog(world));
    let mut section = Section::new(value)
        .line("Homesteads: claim one housing and one garden plot. A cottage costs 8 planks + 4 logs and 600 ticks; it changes your home without moving you. Plant one real grain seed, water once with one clean water, then harvest three grain after 6000 ticks. Use each advertised bed position and its bed index; all work requires reach. stop_work cancels construction without spending its reserved materials.");
    section = section.line("Colony crop choices are declared in homestead.crops. Hardy berries consume one carried berry and one clean water, grow for 9000 ticks, and yield four edible/tradable berries. Choose plant.args.kind explicitly; omitted kind still means village_grain.");
    Some(section)
}

fn checked(mut affordance: Affordance, result: Result<(), String>) -> Affordance {
    if let Err(reason) = result {
        affordance.ok = false;
        affordance.why_not = Some(reason);
    }
    affordance
}

pub fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    if world.get::<Villager>(ctx.entity).is_none() || !living(world, ctx.entity) {
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
                if verb == "plant" {
                    for crop in crop_catalog(world) {
                        command.kind = crop.kind.into();
                        candidates.push((bed, crop.kind, command.check(world)));
                    }
                } else {
                    candidates.push((
                        bed,
                        super::super::roles::roles(world).grain,
                        command.check(world),
                    ));
                }
            }
            let selected = candidates
                .iter()
                .find(|(_, _, check)| check.is_ok())
                .or_else(|| candidates.first());
            let (bed, kind, check) = selected
                .map(|(bed, kind, check)| (*bed, *kind, check.clone()))
                .unwrap_or_else(|| {
                    (
                        0,
                        super::super::roles::roles(world).grain,
                        Err("this plot has no crop beds".into()),
                    )
                });
            let label = {
                match verb {
                    "plant" => "Plant a crop from your pack",
                    "harvest" => "Harvest a mature crop",
                    _ => label,
                }
            };
            let mut affordance=Affordance::new(verb,format!("{label} on {}; approach the chosen bed position",definition.label))
                .target(id).arg("bed",&format!("0..{}; default 0. See homestead.plots[].beds[].position for exact approach coordinates",estate.beds.len().saturating_sub(1)));
            if verb == "plant" {
                let options = {
                    format!(
                        "village_grain or village_berries; default village_grain. Checked available choice: {kind}; pass kind explicitly for berries."
                    )
                };
                affordance = affordance.arg("kind", &options);
            }
            if let Some(pos) = bed_position(world, plot, bed) {
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
