//! Recorded village actions and the explicit host-to-world diary publication boundary.
use super::{ai, catalog, economy as econ, model::*};
use bevy_ecs::prelude::*;
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, Item};
use npc_framework::kernel::commands::{CommandError, entity};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VillageAct {
    pub unit: u32,
    pub verb: String,
    #[serde(default)]
    pub target: Option<SimId>,
    #[serde(default)]
    pub kind: String,
    #[serde(default = "one")]
    pub quantity: u32,
    #[serde(default = "one")]
    pub page: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
}
fn one() -> u32 {
    1
}
impl VillageAct {
    pub fn new(unit: SimId, verb: &str) -> Self {
        Self {
            unit: unit.0,
            verb: verb.into(),
            target: None,
            kind: String::new(),
            quantity: 1,
            page: 1,
            title: String::new(),
            body: String::new(),
        }
    }
    fn target(&self, world: &World) -> Result<Entity, String> {
        self.target
            .and_then(|id| entity_of(world, id))
            .ok_or_else(|| "target is missing".into())
    }
    pub fn check(&self, world: &World) -> Result<(), String> {
        let actor = entity(world, self.unit).map_err(|e| e.to_string())?;
        let state = world
            .get_resource::<VillageState>()
            .ok_or("not an island village")?;
        if self.verb == "new_life" {
            if state.played != SimId(self.unit) || living(world, actor) {
                return Err(
                    "a successor may join only after the current actor's permanent death".into(),
                );
            }
            if state.life >= 1000 {
                return Err("this village has reached its 1,000-life limit".into());
            }
            return Ok(());
        }
        if !living(world, actor) {
            return Err("this actor is dead; begin a new life in the same village".into());
        }
        match self.verb.as_str() {
            "stop_work" => Ok(()),
            "buy" | "sell" => econ::merchant_check(
                world,
                actor,
                self.target(world)?,
                &self.kind,
                self.quantity,
                self.verb == "buy",
            ),
            "gather" => {
                let target = self.target(world)?;
                let p = world
                    .get::<ResourcePatch>(target)
                    .ok_or("not a resource patch")?;
                if p.remaining == 0 {
                    return Err("resource patch is depleted until tomorrow".into());
                }
                if !econ::near(world, actor, target) {
                    return Err("move within 1.5 m of the resource patch".into());
                }
                if world.get::<Gathering>(actor).is_some()
                    || world
                        .get::<npc_framework::crafting::Crafting>(actor)
                        .is_some()
                {
                    return Err("already working".into());
                }
                econ::fit(world, actor, &[Item::new(&p.kind)])
            }
            "eat" | "drink" => {
                let kind = if self.verb == "eat" {
                    "bread"
                } else {
                    catalog::WATER
                };
                if econ::count(world, actor, kind) == 0 {
                    return Err(format!("buy {kind} first"));
                }
                Ok(())
            }
            "rest" => {
                let inn = self.target(world)?;
                let keeper = world
                    .get::<Villager>(inn)
                    .ok_or("target is not the innkeeper")?;
                if keeper.role != Role::Innkeeper || !living(world, inn) {
                    return Err("rent a bed from the living innkeeper".into());
                }
                if !econ::near(world, actor, inn) {
                    return Err("move within 1.5 m of the innkeeper".into());
                }
                if world.get::<Wallet>(actor).map_or(0, |w| w.coins) < 3 {
                    return Err("a bed costs 3 coins".into());
                }
                if world.get::<Villager>(actor).is_some_and(|v| {
                    v.resting_until > world.resource::<SimClock>().tick && v.rest_rate >= 400
                }) {
                    return Err("already resting in a paid bed".into());
                }
                Ok(())
            }
            "talk" | "ask_diary" => {
                let target = self.target(world)?;
                if !living(world, target)
                    || world.get::<Villager>(target).is_none()
                    || !econ::near(world, actor, target)
                {
                    return Err("move within 1.5 m of a living villager".into());
                }
                if self.verb == "ask_diary" {
                    if world
                        .get::<Villager>(target)
                        .is_some_and(|v| v.role == Role::Visitor)
                    {
                        return Err("a recovered diary can be requested from village residents; visitors keep their own possessions".into());
                    }
                    let item = items::carried_items(world, target)
                        .into_iter()
                        .filter_map(|id| entity_of(world, id))
                        .find(|e| world.get::<DroppedDiary>(*e).is_some())
                        .ok_or("this villager has no recovered diary")?;
                    items::can_store(world, actor, item)?;
                }
                Ok(())
            }
            "read_diary" => {
                let target = self.target(world)?;
                let book = world
                    .get::<DroppedDiary>(target)
                    .ok_or("target is not a dropped diary")?;
                if world
                    .get::<CarriedBy>(target)
                    .is_none_or(|c| c.holder != SimId(self.unit))
                {
                    return Err("pick up the diary before reading it".into());
                }
                if self.page == 0 || self.page as usize > book.pages.len().max(1) {
                    return Err(format!("page must be 1..={}", book.pages.len().max(1)));
                }
                Ok(())
            }
            "write_diary" => {
                validate_pages(&[DiaryPage {
                    tick: 0,
                    title: self.title.clone(),
                    body: self.body.clone(),
                }])?;
                let book = world
                    .get::<VillageDiary>(actor)
                    .ok_or("this actor has no diary")?;
                let mut pages = book.pages.clone();
                pages.push(DiaryPage {
                    tick: world.resource::<SimClock>().tick,
                    title: self.title.clone(),
                    body: self.body.clone(),
                });
                validate_pages(&pages)
            }
            "collect_coins" => {
                let target = self.target(world)?;
                let purse = world
                    .get::<CoinPurse>(target)
                    .ok_or("not a dropped coin purse")?;
                let held = world.get::<CarriedBy>(target);
                if held.is_some_and(|c| c.holder != SimId(self.unit))
                    || (held.is_none() && !econ::near(world, actor, target))
                {
                    return Err("move within 1.5 m of the dropped coins".into());
                }
                if world
                    .get::<Wallet>(actor)
                    .and_then(|w| w.coins.checked_add(purse.coins))
                    .is_none()
                {
                    return Err("wallet is full".into());
                }
                Ok(())
            }
            _ => Err("unknown village action".into()),
        }
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = entity(world, self.unit).map_err(|e| e.to_string())?;
        match self.verb.as_str() {
            "stop_work" => {
                ai::stop_work(world, actor, "stopped by actor");
                let now = world.resource::<SimClock>().tick;
                if let Some(mut v) = world.get_mut::<Villager>(actor) {
                    v.resting_until = now;
                    v.rest_rate = 0;
                }
                Ok(())
            }
            "new_life" => new_life(world),
            "buy" | "sell" => {
                let trader = self.target(world)?;
                if self.verb == "buy" {
                    econ::trade(world, actor, trader, &self.kind, self.quantity)
                } else {
                    econ::trade(world, trader, actor, &self.kind, self.quantity)
                }
            }
            "gather" => {
                let target = self.target(world)?;
                ai::begin_gather(world, actor, target, self.target.expect("checked target"))
            }
            "eat" | "drink" => {
                let (kind, need, amount) = if self.verb == "eat" {
                    ("bread", "Hunger", 80_000)
                } else {
                    (catalog::WATER, "Thirst", 90_000)
                };
                econ::consume(world, actor, kind, 1)?;
                econ::restore(world, actor, need, amount);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageConsumed",
                    &json!({"kind":kind,"need":need}),
                );
                Ok(())
            }
            "rest" => {
                let keeper = self.target(world)?;
                ai::stop_work(world, actor, "renting a bed");
                world.get_mut::<Wallet>(actor).expect("wallet").coins -= 3;
                world.get_mut::<Wallet>(keeper).expect("wallet").coins += 3;
                ai::start_rest(world, actor, 300, 400);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageBedRented",
                    &json!({"coins":3,"ticks":300}),
                );
                Ok(())
            }
            "talk" => {
                let target = self.target(world)?;
                let v = world.get::<Villager>(target).expect("villager");
                let offers:Vec<_>=catalog::outputs(v.role).iter().map(|kind|json!({"kind":kind,"quantity":econ::count(world,target,kind),"price":catalog::price(kind)})).collect();
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageConversation",
                    &json!({"name":v.name,"role":v.role,"offers":offers,"bed_price":if v.role==Role::Innkeeper{Some(3)}else{None}}),
                );
                Ok(())
            }
            "read_diary" => {
                let target = self.target(world)?;
                let book = world.get::<DroppedDiary>(target).expect("diary").clone();
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageDiaryRead",
                    &json!({"diary":self.target,"author":book.author,"page":self.page,"page_count":book.pages.len(),"entry":book.pages.get(self.page as usize - 1),"published":book.sealed}),
                );
                Ok(())
            }
            "write_diary" => {
                let tick = world.resource::<SimClock>().tick;
                world
                    .get_mut::<VillageDiary>(actor)
                    .expect("diary")
                    .pages
                    .push(DiaryPage {
                        tick,
                        title: self.title.clone(),
                        body: self.body.clone(),
                    });
                Ok(())
            }
            "ask_diary" => {
                let trader = self.target(world)?;
                let item = items::carried_items(world, trader)
                    .into_iter()
                    .filter_map(|id| entity_of(world, id))
                    .find(|e| world.get::<DroppedDiary>(*e).is_some())
                    .expect("checked diary");
                items::put_in_inventory(world, actor, item);
                Ok(())
            }
            "collect_coins" => {
                let target = self.target(world)?;
                let coins = world.get::<CoinPurse>(target).expect("purse").coins;
                world.get_mut::<Wallet>(actor).expect("wallet").coins += coins;
                world.despawn(target);
                Ok(())
            }
            _ => Err("unknown village action".into()),
        }
    }
}
impl ContentCommand for VillageAct {
    const NAME: &'static str = "VillageAct";
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
fn new_life(world: &mut World) -> Result<(), String> {
    let old = world.resource::<VillageState>().clone();
    if let Some(e) = entity_of(world, old.played) {
        world
            .entity_mut(e)
            .remove::<npc_framework::agent_io::control::ExternalController>();
    }
    let life = old.life + 1;
    let preset = world
        .get_resource::<crate::actors::ActorPresets>()
        .and_then(|p| p.0.keys().next().cloned())
        .unwrap_or_else(|| "human".into());
    let spec = crate::layout::UnitSpec {
        template: "scout".into(),
        preset: Some(preset),
        ..Default::default()
    };
    let pos = SimPos::new(500, -1500);
    let id = crate::layout::spawn_unit(world, pos, &spec)?;
    world.resource_mut::<VillageState>().life = life;
    let e = entity_of(world, id).expect("new life");
    super::initialise_person(
        world,
        e,
        Role::Visitor,
        &format!("{} {life}", old.agent_alias),
        pos,
        SimId(0),
    );
    world.entity_mut(e).insert((
        VillageDiary::default(),
        npc_framework::perception::RevealsFog,
    ));
    // The alias is a snapshot-only host lookup. The new SimId is authoritative.
    let mut ids = world.resource_mut::<crate::TestbedIds>();
    for assigned in ids.names.values_mut() {
        if *assigned == old.played {
            *assigned = id;
        }
    }
    ids.units.push(id);
    world.resource_mut::<VillageState>().played = id;
    record_effect(
        world,
        Some(id),
        "VillageNewLife",
        &json!({"previous":old.played,"new_actor":id,"life":life,"world_preserved":true,"reenter_world":true}),
    );
    Ok(())
}
pub fn validate_pages(pages: &[DiaryPage]) -> Result<(), String> {
    if pages.len() > MAX_DIARY_PAGES {
        return Err(format!(
            "a physical diary holds at most {MAX_DIARY_PAGES} pages"
        ));
    }
    let mut size = 0;
    for p in pages {
        if p.title.trim().is_empty()
            || p.title.chars().count() > 200
            || p.title.contains(['\n', '\r'])
            || p.body.len() > 8192
        {
            return Err("diary pages need a one-line title (1..=200 characters) and a body of at most 8192 bytes".into());
        }
        size += p.title.len() + p.body.len();
    }
    if size > MAX_DIARY_BYTES {
        return Err("physical diary exceeds 1 MiB; keep shorter entries".into());
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishDiary {
    pub unit: u32,
    pub diary: SimId,
    pub pages: Vec<DiaryPage>,
}
impl ContentCommand for PublishDiary {
    const NAME: &'static str = "VillagePublishDiary";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let book = entity_of(world, self.diary)
            .and_then(|e| world.get::<DroppedDiary>(e))
            .ok_or_else(|| CommandError::Unavailable {
                reason: "dropped diary is missing".into(),
            })?;
        if book.author != SimId(self.unit) || book.sealed {
            return Err(CommandError::Unavailable {
                reason: "diary is already sealed or belongs to another actor".into(),
            });
        }
        validate_pages(&self.pages).map_err(|reason| CommandError::Unavailable { reason })?;
        if self.pages.iter().any(|p| p.tick > book.died_at) {
            return Err(CommandError::Unavailable {
                reason: "a deceased actor cannot add later pages".into(),
            });
        }
        Ok(())
    }
    fn apply(self, world: &mut World) {
        if self.validate(world).is_err() {
            return;
        }
        if let Some(e) = entity_of(world, self.diary)
            && let Some(mut book) = world.get_mut::<DroppedDiary>(e)
        {
            book.pages = self.pages;
            book.sealed = true;
        }
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
