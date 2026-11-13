//! Recorded village actions and the explicit host-to-world diary publication boundary.
use super::{ai, catalog, economy as econ, market, model::*};
use bevy_ecs::prelude::*;
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, Item};
use npc_framework::kernel::commands::{CommandError, entity};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Shared by resident policy, client affordances and command application.
pub const HOME_REST_REACH_MM: i32 = 800;
pub const HOME_REST_TICKS: u64 = 300;
pub const HOME_REST_RATE: i32 = 400;
pub const GROUND_REST_TICKS: u64 = 300;
pub const GROUND_REST_RATE: i32 = 40;

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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Maximum buy price, minimum sell price, or minimum posted offer price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<u32>,
}
fn one() -> u32 {
    1
}
/// Shared by agent planning and application-time command validation.
pub(super) fn verb_gate(world: &World, e: Entity, verb: &str) -> Option<String> {
    let v = world.get::<Vitality>(e)?;
    if v.died_at.is_some() {
        return (verb != "new_life").then(|| {
            "this actor is permanently dead; begin a new life in the same village".into()
        });
    }
    let person = world.get::<Villager>(e)?;
    if let Some(reason) = super::frontier::activities::busy(world, e)
        && !matches!(verb, "stop_work" | "talk" | "read_diary" | "write_diary")
    {
        return Some(reason);
    }
    if let Some(reason) = super::homestead::busy(world, e)
        && !matches!(verb, "stop_work" | "talk" | "read_diary" | "write_diary")
    {
        return Some(reason.into());
    }
    if world.get::<super::hunting::Hunting>(e).is_some()
        && (verb.eq_ignore_ascii_case("craft") || verb.starts_with("craft:") || verb == "gather")
    {
        return Some("finish or stop the current hunt before starting other work".into());
    }
    if person.resting_until > world.resource::<SimClock>().tick
        && !matches!(verb, "rest" | "read_diary" | "write_diary" | "stop_work")
    {
        return Some(format!("resting until tick {}", person.resting_until));
    }
    None
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
            text: String::new(),
            limit_price: None,
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
        if let Some(reason) = verb_gate(world, actor, &self.verb) {
            return Err(reason);
        }
        if self.verb != "talk" && !self.text.is_empty() {
            return Err("speech text is supported only by talk".into());
        }
        if self
            .limit_price
            .is_some_and(|price| !(1..=market::MAX_UNIT_PRICE).contains(&price))
        {
            return Err(format!(
                "limit_price must be 1..={}",
                market::MAX_UNIT_PRICE
            ));
        }
        if self.limit_price.is_some()
            && !matches!(self.verb.as_str(), "buy" | "sell" | "offer_goods")
        {
            return Err("limit_price is supported only by buy, sell and offer_goods".into());
        }
        match self.verb.as_str() {
            "stop_work" => Ok(()),
            "hunt" => super::hunting::check(
                world,
                actor,
                self.target.ok_or("hunt requires a prey target")?,
            ),
            "buy" | "sell" => {
                let trader = self.target(world)?;
                let buy = self.verb == "buy";
                // Stores, stalls and building sites trade through their own
                // validated transfers; `sell` there consigns or delivers.
                if let Some(checked) = super::civic::trade_check(
                    world,
                    actor,
                    trader,
                    buy,
                    &self.kind,
                    self.quantity,
                    self.limit_price,
                ) {
                    return checked;
                }
                econ::merchant_check(world, actor, trader, &self.kind, self.quantity, buy)?;
                let (buyer, seller) = if buy {
                    (actor, trader)
                } else {
                    (trader, actor)
                };
                let quoted = market::quote(world, buyer, seller, &self.kind, self.quantity)?;
                if self.limit_price.is_some_and(|limit| {
                    if buy {
                        quoted.unit_price > limit
                    } else {
                        quoted.unit_price < limit
                    }
                }) {
                    return Err("the current quote is outside your limit_price".into());
                }
                Ok(())
            }
            "offer_goods" => {
                market::check_offer(world, actor, &self.kind, self.quantity, self.limit_price)
            }
            "withdraw_offer" => {
                if market::has_offer(world, actor, &self.kind) {
                    Ok(())
                } else {
                    Err("this actor has no active offer for that good".into())
                }
            }
            "gather" => {
                let target = self.target(world)?;
                let p = world
                    .get::<ResourcePatch>(target)
                    .ok_or("not a resource patch")?;
                if !econ::near(world, actor, target) {
                    return Err("move within 1.5 m of the resource patch".into());
                }
                if p.remaining == 0 {
                    return Err({
                        "resource patch is depleted; renewable sites recover over time".into()
                    });
                }
                if p.kind == catalog::FISH && econ::count(world, actor, catalog::FISHING_ROD) == 0 {
                    return Err("fishing requires a fishing rod in your pack".into());
                }
                if world.get::<Gathering>(actor).is_some()
                    || world.get::<super::hunting::Hunting>(actor).is_some()
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
                    if self.kind.is_empty() {
                        super::roles::roles(world).baked
                    } else {
                        self.kind.as_str()
                    }
                } else {
                    super::roles::roles(world).drink
                };
                if self.verb == "eat" && catalog::food_restore_in(world, kind).is_none() {
                    return Err(
                        "eat needs a finished edible good; raw materials must be cooked".into(),
                    );
                }
                if econ::count(world, actor, kind) == 0 {
                    return Err(format!("buy {kind} first"));
                }
                Ok(())
            }
            "rest_home" | "rest_ground" => {
                let person = world.get::<Villager>(actor).ok_or("actor has no home")?;
                if self.verb == "rest_home"
                    && world.get::<SimPos>(actor).is_none_or(|pos| {
                        pos.dist2(person.home) > i64::from(HOME_REST_REACH_MM).pow(2)
                    })
                {
                    return Err("move within 0.8 m of your own home before resting".into());
                }
                Ok(())
            }
            "rest" => {
                let inn = self.target(world)?;
                let keeper = world
                    .get::<Villager>(inn)
                    .ok_or("target is not the innkeeper")?;
                if !super::frontier::economy_spec::is_service(world, keeper.role)
                    || !living(world, inn)
                {
                    return Err("rent a bed from the living innkeeper".into());
                }
                // Living world: a bed is rented at the inn, the keeper paid
                // wherever it is; elsewhere from the innkeeper in person.
                let at_the_inn = super::indoors::at_inn(world, actor, keeper.workplace);
                if !at_the_inn && !econ::near(world, actor, inn) {
                    return Err("move within 1.5 m of the innkeeper".into());
                }
                // Regional: the inn has a fixed number of guest beds.
                if super::indoors::inside_entity(world, actor) != Some(keeper.workplace)
                    && super::indoors::free_guest_bed(world, keeper.workplace).is_none()
                {
                    return Err("the inn has no free bed".into());
                }
                if world.get::<Wallet>(actor).map_or(0, |w| w.coins) < catalog::BED_PRICE {
                    return Err(format!("a bed costs {} coins", catalog::BED_PRICE));
                }
                if world
                    .get::<Wallet>(inn)
                    .and_then(|wallet| wallet.coins.checked_add(catalog::BED_PRICE))
                    .is_none()
                {
                    return Err("the innkeeper's wallet is full".into());
                }
                if world.get::<Villager>(actor).is_some_and(|v| {
                    v.resting_until > world.resource::<SimClock>().tick && v.rest_rate >= 400
                }) {
                    return Err("already resting in a paid bed".into());
                }
                Ok(())
            }
            "talk" if !self.text.is_empty() || self.target.is_none() => {
                super::speech::check(world, actor, self.target, &self.text)
            }
            "talk" | "ask_diary" => {
                let target = self.target(world)?;
                if !living(world, target)
                    || world.get::<Villager>(target).is_none()
                    || !econ::near(world, actor, target)
                {
                    return Err("move within 1.5 m of a living villager".into());
                }
                if actor == target {
                    return Err("omit target and supply text to speak aloud".into());
                }
                if self.verb == "talk"
                    && world
                        .get::<Villager>(target)
                        .is_some_and(|v| v.role == Role::Visitor)
                {
                    return Err("include text when speaking to a visiting actor".into());
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
            "recover_diary" => {
                let target = self.target(world)?;
                if world.get::<DroppedDiary>(target).is_none() {
                    return Err("target is not a dropped diary".into());
                }
                if world.get::<CarriedBy>(target).is_some() {
                    return Err("this diary is already carried; ask its holder for it".into());
                }
                if !econ::near(world, actor, target) {
                    return Err("move within 1.5 m of the dropped diary".into());
                }
                items::can_store(world, actor, target)
            }
            _ => Err("unknown village action".into()),
        }
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = entity(world, self.unit).map_err(|e| e.to_string())?;
        match self.verb.as_str() {
            "hunt" => super::hunting::begin(world, actor, self.target.expect("checked prey")),
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
                if let Some(done) = super::civic::trade(
                    world,
                    actor,
                    trader,
                    self.verb == "buy",
                    &self.kind,
                    self.quantity,
                ) {
                    return done;
                }
                if self.verb == "buy" {
                    econ::trade(world, actor, trader, &self.kind, self.quantity)
                } else {
                    econ::trade(world, trader, actor, &self.kind, self.quantity)
                }
            }
            "offer_goods" => {
                market::post_offer(world, actor, &self.kind, self.quantity, self.limit_price)
            }
            "withdraw_offer" => market::withdraw_offer(world, actor, &self.kind),
            "gather" => {
                let target = self.target(world)?;
                ai::begin_gather(world, actor, target, self.target.expect("checked target"))
            }
            "eat" | "drink" => {
                let (kind, need, amount) = if self.verb == "eat" {
                    let kind = if self.kind.is_empty() {
                        super::roles::roles(world).baked
                    } else {
                        self.kind.as_str()
                    };
                    (
                        kind,
                        "Hunger",
                        catalog::food_restore_in(world, kind).expect("checked food"),
                    )
                } else {
                    (super::roles::roles(world).drink, "Thirst", 90_000)
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
            "rest_home" | "rest_ground" => {
                let (kind, ticks, rate) = if self.verb == "rest_home" {
                    ("home", HOME_REST_TICKS, HOME_REST_RATE)
                } else {
                    ("ground", GROUND_REST_TICKS, GROUND_REST_RATE)
                };
                ai::stop_work(world, actor, "resting");
                ai::start_rest(world, actor, ticks, rate);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageRested",
                    &json!({"kind":kind,"ticks":ticks,"energy_per_tick":rate}),
                );
                Ok(())
            }
            "rest" => {
                let keeper = self.target(world)?;
                ai::stop_work(world, actor, "renting a bed");
                world.get_mut::<Wallet>(actor).expect("wallet").coins -= catalog::BED_PRICE;
                world.get_mut::<Wallet>(keeper).expect("wallet").coins += catalog::BED_PRICE;
                super::frontier::labor::took(world, keeper, catalog::BED_PRICE);
                ai::start_rest(world, actor, catalog::BED_TICKS, 400);
                super::indoors::rented_bed(world, actor, keeper);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageBedRented",
                    &json!({"coins":catalog::BED_PRICE,"ticks":catalog::BED_TICKS}),
                );
                Ok(())
            }
            "talk" => {
                if !self.text.is_empty() {
                    super::speech::speak(world, SimId(self.unit), self.target, &self.text);
                    return Ok(());
                }
                let target = self.target(world)?;
                let v = world.get::<Villager>(target).expect("villager");
                let offered: Vec<_> = {
                    catalog::trade_kinds(world)
                        .iter()
                        .copied()
                        .filter(|kind| market::sells(world, target, kind))
                        .collect()
                };
                let offers: Vec<_> = offered
                    .iter()
                    .map(|kind| {
                        json!({"kind":kind,
                        "quantity":market::available_to_sell(world,target,kind),
                        "price":market::asking_price(world,target,kind)})
                    })
                    .collect();
                let keeper = super::frontier::economy_spec::is_service(world, v.role);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageConversation",
                    &json!({"name":v.name,"role":v.role,"offers":offers,"bed_price":if keeper{Some(catalog::BED_PRICE)}else{None}}),
                );
                super::speech::speak(world, SimId(self.unit), self.target, "What do you offer?");
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
                items::take_from_holder(world, target);
                world.despawn(target);
                Ok(())
            }
            "recover_diary" => {
                let target = self.target(world)?;
                items::put_in_inventory(world, actor, target);
                record_effect(
                    world,
                    Some(SimId(self.unit)),
                    "VillageDiaryRecovered",
                    &json!({"diary":self.target}),
                );
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
        // A recorded action of a played character feeds its bonds and goals.
        if self.perform(world).is_ok() {
            super::live::acted(
                world,
                SimId(self.unit),
                &self.verb,
                self.target,
                &self.kind,
                self.quantity,
            );
        }
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
fn new_life(world: &mut World) -> Result<(), String> {
    let old = world.resource::<VillageState>().clone();
    let life = old.life + 1;
    let preset = world
        .get_resource::<super::BirthPreset>()
        .map(|p| p.0.clone())
        .unwrap_or_else(|| "human".into());
    let spec = crate::layout::UnitSpec {
        template: "scout".into(),
        preset: Some(preset),
        ..Default::default()
    };
    let pos = super::frontier::spec(world).map_or(SimPos::new(500, -1500), |spec| spec.actor_start);
    let id = crate::layout::spawn_unit(world, pos, &spec)?;
    if let Some(e) = entity_of(world, old.played) {
        world
            .entity_mut(e)
            .remove::<npc_framework::agent_io::control::ExternalController>();
    }
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
    let mut ids = world.resource_mut::<crate::IslandIds>();
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
