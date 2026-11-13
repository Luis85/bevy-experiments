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
    if let Some(reason) = super::super::frontier::activities::busy(world, actor) {
        return Err(reason);
    }
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
    let (house, body, _) = cottage_body(world, front_door(center), None);
    body_fits(world, house, body, None)?;
    // Living world: the plot must also hold the body the cottage grows into.
    if super::super::frontier::enabled(world)
        && let Some(larger) = super::super::live::larger_building(world)
    {
        let (house, body, _) = cottage_body(world, front_door(center), Some(&larger));
        body_fits(world, house, body, None)
            .map_err(|reason| format!("this plot has no room for the larger cottage: {reason}"))?;
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
    let roles = super::super::roles::roles(world);
    for (kind, count) in [(roles.lumber, PLANK_COST), (roles.timber, LOG_COST)] {
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
            super::super::colony::can_claim_cottage(world, plot, me)?;
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
            let allowance = super::super::live::garden_allowance(world, me);
            let limit = if definition.use_kind == "housing" {
                MAX_HOUSING_PLOTS
            } else {
                allowance.map_or(MAX_GARDEN_PLOTS, |plots| plots as usize)
            };
            if owned >= limit {
                return Err(match allowance {
                    Some(plots) if definition.use_kind != "housing" => format!(
                        "you may own {plots} garden plot{}; a homestead upgrade that adds plots allows another",
                        if plots == 1 { "" } else { "s" }
                    ),
                    _ => "you may own one housing plot and one garden plot".into(),
                });
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
                let crop = crop_definition(world, &self.kind)
                    .ok_or("this crop is not available in this world's farm rules")?;
                economy::selected(world, actor, crop.seed_kind, crop.seed_count).map(|_| ())
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
                let definition = crop_definition(world, &crop.kind)
                    .ok_or("the planted crop is not supported by this world's farm rules")?;
                economy::selected(world, actor, definition.water_kind, definition.water_count)
                    .map(|_| ())
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
                let definition = crop_definition(world, &crop.kind)
                    .ok_or("the planted crop is not supported by this world's farm rules")?;
                economy::fit(
                    world,
                    actor,
                    &[Item::new(definition.yield_kind).stack(definition.yield_count)],
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
                    super::super::colony::cottage_claimed(world, plot, me);
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
                let definition = crop_definition(world, &self.kind).expect("checked crop kind");
                economy::consume(world, actor, definition.seed_kind, definition.seed_count)?;
                world.get_mut::<HomesteadPlot>(plot).expect("plot").beds[self.bed as usize] =
                    Some(Crop {
                        kind: definition.kind.into(),
                        planted_at: now,
                        watered_at: None,
                        ready_at: None,
                        ready_announced: false,
                    });
                record_effect(
                    world,
                    Some(me),
                    "VillageCropPlanted",
                    &json!({"plot":self.target,"bed":self.bed,"kind":definition.kind}),
                );
            }
            "water_crop" => {
                let kind = &world.get::<HomesteadPlot>(plot).expect("plot").beds[self.bed as usize]
                    .as_ref()
                    .expect("crop")
                    .kind;
                let definition = crop_definition(world, kind).expect("checked crop kind");
                economy::consume(world, actor, definition.water_kind, definition.water_count)?;
                let ready_at = now.saturating_add(u64::from(definition.growth_ticks));
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
                let kind = &world.get::<HomesteadPlot>(plot).expect("plot").beds[self.bed as usize]
                    .as_ref()
                    .expect("crop")
                    .kind;
                let definition = crop_definition(world, kind).expect("checked crop kind");
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
                economy::grant(
                    world,
                    actor,
                    Item::new(definition.yield_kind).stack(definition.yield_count),
                );
                economy::produce(world, definition.yield_kind, definition.yield_count);
                super::super::production::completed(
                    world,
                    me,
                    definition.yield_kind,
                    definition.yield_count,
                );
                super::super::live::booked(
                    world,
                    me,
                    super::super::live::Book::Gathered,
                    definition.yield_kind,
                    definition.yield_count,
                );
                record_effect(
                    world,
                    Some(me),
                    "VillageCropHarvested",
                    &json!({"plot":self.target,"bed":self.bed,"kind":definition.yield_kind,"quantity":definition.yield_count}),
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
        if self.perform(world).is_ok() {
            super::super::live::homestead_acted(world, SimId(self.unit), &self.verb);
        }
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
