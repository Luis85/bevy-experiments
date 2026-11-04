use super::*;
use npc_framework::kernel::commands::{CommandError, entity};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomesteadAct {
    pub unit: u32,
    pub verb: String,
    pub target: SimId,
    #[serde(default)]
    pub bed: u32,
    #[serde(default = "seed_kind")]
    pub kind: String,
}
fn seed_kind() -> String {
    catalog::GRAIN.into()
}

pub(super) fn idle(world: &World, actor: Entity) -> Result<(), String> {
    if world.get::<CottageBuild>(actor).is_some()
        || world.get::<super::super::Gathering>(actor).is_some()
        || world.get::<super::super::hunting::Hunting>(actor).is_some()
        || world
            .get::<npc_framework::crafting::Crafting>(actor)
            .is_some()
        || world
            .get::<Villager>(actor)
            .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick)
    {
        return Err("already working or resting; finish or stop_work first".into());
    }
    Ok(())
}

fn near_position(world: &World, actor: Entity, pos: SimPos) -> bool {
    world
        .get::<SimPos>(actor)
        .is_some_and(|at| at.dist2(pos) <= 2_250_000)
}

fn site_clear(world: &World, plot: Entity) -> Result<(), String> {
    let center = *world.get::<SimPos>(plot).ok_or("plot has no position")?;
    let house = cottage_center(center);
    let grid = world.resource::<SpatialGrid>();
    let cells = grid.footprint_cells(house, cottage_footprint());
    if cells
        .iter()
        .any(|cell| !geography::for_world(world).land_cell(cell.x, cell.z))
    {
        return Err("the cottage footprint must be on land".into());
    }
    for entity in world.resource::<SimIndex>().map.values().copied() {
        let Some(pos) = world.get::<SimPos>(entity).copied() else {
            continue;
        };
        if let Some(blocker) = world.get::<GridBlocker>(entity)
            && grid
                .footprint_cells(pos, blocker.0)
                .iter()
                .any(|cell| cells.contains(cell))
        {
            return Err("the cottage site is blocked".into());
        }
        if world.get::<Villager>(entity).is_some()
            && living(world, entity)
            && cells.contains(&grid.cell_of(pos))
        {
            return Err(
                "a living actor occupies the cottage site; stand near its front door".into(),
            );
        }
    }
    Ok(())
}

fn build_plan(world: &World, actor: Entity, plot: Entity) -> Result<Vec<BuildMaterial>, String> {
    let estate = world
        .get::<HomesteadPlot>(plot)
        .ok_or("not a homestead plot")?;
    if definition(world, plot).is_none_or(|definition| definition.use_kind != "housing") {
        return Err("cottages can be built only on housing plots".into());
    }
    if estate.cottage_built {
        return Err("this estate already has a cottage".into());
    }
    let plot_id = *world.get::<SimId>(plot).ok_or("plot identity is missing")?;
    if world.resource::<Reservations>().count(plot_id) > 0 {
        return Err("the cottage site is already claimed by work".into());
    }
    site_clear(world, plot)?;
    let mut materials = Vec::new();
    for (kind, count) in [("plank", PLANK_COST), ("log", LOG_COST)] {
        for (item, selected) in economy::selected(world, actor, kind, count)? {
            materials.push(BuildMaterial {
                item,
                kind: kind.into(),
                count: selected.stack,
            });
        }
    }
    materials.sort_by_key(|material| material.item);
    Ok(materials)
}

impl HomesteadAct {
    pub fn new(unit: SimId, verb: &str, target: SimId) -> Self {
        Self {
            unit: unit.0,
            verb: verb.into(),
            target,
            bed: 0,
            kind: seed_kind(),
        }
    }

    pub fn check(&self, world: &World) -> Result<(), String> {
        if !geography::is_expanded(world) {
            return Err("homesteads require an expanded Living world".into());
        }
        let actor = entity(world, self.unit).map_err(|e| e.to_string())?;
        if !living(world, actor) || world.get::<Villager>(actor).is_none() {
            return Err("only living villagers can work a homestead".into());
        }
        idle(world, actor)?;
        let plot = entity_of(world, self.target).ok_or("homestead plot is missing")?;
        let estate = world
            .get::<HomesteadPlot>(plot)
            .ok_or("target is not a homestead plot")?;
        let definition = definition(world, plot).ok_or("plot is not in this world's geography")?;
        let center = *world.get::<SimPos>(plot).ok_or("plot has no position")?;
        let point = if matches!(self.verb.as_str(), "plant" | "water_crop" | "harvest") {
            bed_position(world, plot, self.bed).ok_or("bed index is outside this plot")?
        } else {
            front_door(center)
        };
        if !near_position(world, actor, point) {
            return Err("move within 1.5 m of the advertised plot door or bed position".into());
        }
        let me = SimId(self.unit);
        if self.verb == "claim_plot" {
            if estate.owner == Some(me) {
                return Err("you already own this plot".into());
            }
            if estate
                .owner
                .and_then(|owner| entity_of(world, owner))
                .is_some_and(|owner| living(world, owner))
            {
                return Err("this estate has a living owner".into());
            }
            if world.resource::<Reservations>().count(self.target) > 0 {
                return Err("this estate has work in progress".into());
            }
            let owned = plots(world)
                .into_iter()
                .filter(|(_, other)| {
                    world
                        .get::<HomesteadPlot>(*other)
                        .is_some_and(|state| state.owner == Some(me))
                        && super::definition(world, *other)
                            .is_some_and(|other| other.use_kind == definition.use_kind)
                })
                .count();
            let limit = if definition.use_kind == "housing" {
                MAX_HOUSING_PLOTS
            } else {
                MAX_GARDEN_PLOTS
            };
            if owned >= limit {
                return Err("you may own one housing plot and one garden plot".into());
            }
            return Ok(());
        }
        if estate.owner != Some(me) {
            return Err("claim this plot before working it; foreign estates are protected".into());
        }
        match self.verb.as_str() {
            "build_cottage" => build_plan(world, actor, plot).map(|_| ()),
            "plant" => {
                let bed = estate
                    .beds
                    .get(self.bed as usize)
                    .ok_or("bed index is outside this plot")?;
                if bed.is_some() {
                    return Err("this bed already contains a crop".into());
                }
                if self.kind != catalog::GRAIN {
                    return Err("only village_grain can be planted in these hardy beds".into());
                }
                economy::selected(world, actor, catalog::GRAIN, SEED_COUNT).map(|_| ())
            }
            "water_crop" => {
                let crop = estate
                    .beds
                    .get(self.bed as usize)
                    .and_then(Option::as_ref)
                    .ok_or("plant a real seed in this bed first")?;
                if crop.watered_at.is_some() {
                    return Err("this crop is already watered".into());
                }
                economy::selected(world, actor, catalog::WATER, WATER_COUNT).map(|_| ())
            }
            "harvest" => {
                let crop = estate
                    .beds
                    .get(self.bed as usize)
                    .and_then(Option::as_ref)
                    .ok_or("this bed has no crop")?;
                if crop
                    .ready_at
                    .is_none_or(|ready| ready > world.resource::<SimClock>().tick)
                {
                    return Err("this crop is not mature; water it once and wait for growth".into());
                }
                economy::fit(
                    world,
                    actor,
                    &[Item::new(catalog::GRAIN).stack(HARVEST_GRAIN)],
                )
            }
            _ => Err("unknown homestead action".into()),
        }
    }

    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = entity(world, self.unit).map_err(|e| e.to_string())?;
        let plot = entity_of(world, self.target).expect("checked plot");
        let me = SimId(self.unit);
        let now = world.resource::<SimClock>().tick;
        match self.verb.as_str() {
            "claim_plot" => {
                let built = world
                    .get::<HomesteadPlot>(plot)
                    .expect("plot")
                    .cottage_built;
                world.get_mut::<HomesteadPlot>(plot).expect("plot").owner = Some(me);
                if built {
                    let center = *world.get::<SimPos>(plot).expect("plot position");
                    world.get_mut::<Villager>(actor).expect("owner").home = front_door(center);
                }
                record_effect(
                    world,
                    Some(me),
                    "VillagePlotClaimed",
                    &json!({"plot":self.target}),
                );
            }
            "build_cottage" => {
                let materials = build_plan(world, actor, plot)?;
                let mut claims: Vec<_> = materials
                    .iter()
                    .map(|material| material.item)
                    .chain([self.target])
                    .collect();
                claims.sort_unstable();
                claims.dedup();
                for (index, claim) in claims.iter().copied().enumerate() {
                    if !npc_framework::jobs::reserve(world, me, claim, 1) {
                        for previous in claims.iter().copied().take(index) {
                            release(world, actor, previous, ReleaseReason::Failed);
                        }
                        return Err(
                            "the plot or building materials were claimed by other work".into()
                        );
                    }
                }
                npc_framework::command::cancel_orders(world, actor, "building a cottage");
                world
                    .entity_mut(actor)
                    .remove::<MoveIntent>()
                    .insert(CottageBuild {
                        plot: self.target,
                        ends: now.saturating_add(u64::from(BUILD_TICKS)),
                        materials,
                    });
                record_effect(
                    world,
                    Some(me),
                    "VillageCottageStarted",
                    &json!({"plot":self.target,"ends":now.saturating_add(u64::from(BUILD_TICKS))}),
                );
            }
            "plant" => {
                economy::consume(world, actor, catalog::GRAIN, SEED_COUNT)?;
                world.get_mut::<HomesteadPlot>(plot).expect("plot").beds[self.bed as usize] =
                    Some(Crop {
                        kind: catalog::GRAIN.into(),
                        planted_at: now,
                        watered_at: None,
                        ready_at: None,
                        ready_announced: false,
                    });
                record_effect(
                    world,
                    Some(me),
                    "VillageCropPlanted",
                    &json!({"plot":self.target,"bed":self.bed,"kind":catalog::GRAIN}),
                );
            }
            "water_crop" => {
                economy::consume(world, actor, catalog::WATER, WATER_COUNT)?;
                let ready_at = now.saturating_add(u64::from(GROW_TICKS));
                let mut estate = world.get_mut::<HomesteadPlot>(plot).expect("plot");
                let crop = estate.beds[self.bed as usize].as_mut().expect("crop");
                crop.watered_at = Some(now);
                crop.ready_at = Some(ready_at);
                record_effect(
                    world,
                    Some(me),
                    "VillageCropWatered",
                    &json!({"plot":self.target,"bed":self.bed,"ready_at":ready_at}),
                );
            }
            "harvest" => {
                let announce = !world.get::<HomesteadPlot>(plot).expect("plot").beds
                    [self.bed as usize]
                    .as_ref()
                    .expect("crop")
                    .ready_announced;
                world.get_mut::<HomesteadPlot>(plot).expect("plot").beds[self.bed as usize] = None;
                if announce {
                    record_effect(
                        world,
                        Some(me),
                        "VillageCropReady",
                        &json!({"plot":self.target,"bed":self.bed}),
                    );
                }
                economy::grant(world, actor, Item::new(catalog::GRAIN).stack(HARVEST_GRAIN));
                economy::produce(world, catalog::GRAIN, HARVEST_GRAIN);
                super::super::production::completed(world, me, catalog::GRAIN, HARVEST_GRAIN);
                record_effect(
                    world,
                    Some(me),
                    "VillageCropHarvested",
                    &json!({"plot":self.target,"bed":self.bed,"kind":catalog::GRAIN,"quantity":HARVEST_GRAIN}),
                );
            }
            _ => unreachable!("checked homestead action"),
        }
        Ok(())
    }
}

pub(super) fn finish_check(
    world: &World,
    actor: Entity,
    build: &CottageBuild,
) -> Result<(), String> {
    if !living(world, actor) {
        return Err("builder is no longer living".into());
    }
    let me = *world.get::<SimId>(actor).ok_or("builder is missing")?;
    let plot = entity_of(world, build.plot).ok_or("plot is missing")?;
    let estate = world.get::<HomesteadPlot>(plot).ok_or("plot is missing")?;
    if estate.owner != Some(me) || estate.cottage_built {
        return Err("estate ownership or cottage state changed".into());
    }
    if world
        .get::<Villager>(actor)
        .is_none_or(|v| v.resting_until > world.resource::<SimClock>().tick)
    {
        return Err("builder is resting or no longer a villager".into());
    }
    if !near_position(
        world,
        actor,
        front_door(*world.get::<SimPos>(plot).ok_or("plot position")?),
    ) {
        return Err("builder left the cottage site".into());
    }
    site_clear(world, plot)?;
    if !world.resource::<Reservations>().holds(build.plot, me) {
        return Err("plot claim was lost".into());
    }
    for material in &build.materials {
        let item = entity_of(world, material.item).ok_or("building material was lost")?;
        if !world.resource::<Reservations>().holds(material.item, me)
            || world
                .get::<CarriedBy>(item)
                .is_none_or(|held| held.holder != me)
            || world
                .get::<Item>(item)
                .is_none_or(|item| item.kind != material.kind || item.stack < material.count)
        {
            return Err("building materials or their ownership changed".into());
        }
    }
    Ok(())
}

impl ContentCommand for HomesteadAct {
    const NAME: &'static str = "HomesteadAct";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
