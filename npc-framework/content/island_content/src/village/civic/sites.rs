//! Construction sites: paid material deliveries, staged building work,
//! completion into a building with the same identity, and cancellation.
use super::model::{
    Building, CIVIC_OWNER, CivicState, ConstructionSite, Depot, Goods, Hold, SiteWork,
};
use crate::village::frontier::building_spec::BuildingKind;
use crate::village::frontier::{economy_spec, settlements};
use crate::village::{
    Gathering, VillagePlace, VillageState, Wallet, colony::Household, economy as econ, living,
    market,
};
use bevy_ecs::prelude::*;
use npc_framework::crafting::{Crafting, Workstation};
use npc_framework::interaction::Item;
use npc_framework::interaction::items;
use npc_framework::prelude::*;
use serde_json::json;

/// Coins the civic fund may spend on materials now: everything above the
/// reserve kept for destitution grants.
pub fn budget(world: &World) -> u32 {
    let reserve = reserve(world);
    let fund = world
        .get_resource::<VillageState>()
        .map_or(0, |state| state.fund);
    // Coins raised by the building levy are spendable whatever the reserve.
    let earmarked = world
        .get_resource::<CivicState>()
        .map_or(0, |state| state.earmarked)
        .min(fund);
    earmarked + (fund - earmarked).saturating_sub(reserve)
}

/// Coins the building levy could raise now: every living resident's share of
/// the coins held above the destitution floor.
pub fn levy_capacity(world: &World) -> u32 {
    let Some(tuning) = economy_spec::tuning(world) else {
        return 0;
    };
    crate::village::demography::residents(world)
        .into_iter()
        .map(|(_, actor)| {
            levy_share(
                world,
                actor,
                tuning.destitution_floor,
                tuning.building_levy_percent,
            )
        })
        .fold(0, u32::saturating_add)
}

fn levy_share(world: &World, actor: Entity, floor: u32, percent: u32) -> u32 {
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    (u64::from(coins.saturating_sub(floor)) * u64::from(percent) / 100) as u32
}

/// The building levy: when open sites and the planners' deferred projects
/// need more than the fund may spend, residents pay the difference, each in
/// proportion to the coins held above the destitution floor and never more
/// than `building_levy_percent` of them. The coins are earmarked: they buy
/// building materials whatever the reserve says, and what is left when no
/// project remains returns with the dividend. Returns the coins raised.
pub(crate) fn raise_building_levy(world: &mut World) -> u32 {
    let Some(tuning) = economy_spec::tuning(world).cloned() else {
        return 0;
    };
    let need = fund_hold(world).saturating_sub(budget(world));
    let Some((raised, payers, capacity)) = collect(world, need, tuning.building_levy_percent)
    else {
        return 0;
    };
    let mut civic = world.resource_mut::<CivicState>();
    civic.earmarked = civic.earmarked.saturating_add(raised);
    civic.building_levied = civic.building_levied.saturating_add(u64::from(raised));
    let earmarked = civic.earmarked;
    record_effect(
        world,
        None,
        "VillageBuildingLevy",
        &json!({"raised":raised,"needed":need,"payers":payers,"capacity":capacity,
            "percent":tuning.building_levy_percent,"earmarked":earmarked}),
    );
    raised
}

/// Moves up to `need` coins into the fund from every living resident, each in
/// proportion to the coins held above the destitution floor and never more
/// than `percent` of them. Returns the coins raised, the payers and what the
/// levy could have raised; `None` when nothing is needed or nobody can pay.
fn collect(world: &mut World, need: u32, percent: u32) -> Option<(u32, usize, u64)> {
    let floor = economy_spec::tuning(world)?.destitution_floor;
    if need == 0 || percent == 0 {
        return None;
    }
    let shares: Vec<(Entity, u32)> = crate::village::demography::residents(world)
        .into_iter()
        .map(|(_, actor)| (actor, levy_share(world, actor, floor, percent)))
        .filter(|(_, share)| *share > 0)
        .collect();
    let capacity: u64 = shares.iter().map(|(_, share)| u64::from(*share)).sum();
    if capacity == 0 {
        return None;
    }
    let wanted = u64::from(need).min(capacity);
    let mut raised = 0_u32;
    for (actor, share) in &shares {
        // Rounded up, so a small need is met; never more than the share.
        let pay = ((u64::from(*share) * wanted).div_ceil(capacity) as u32)
            .min(*share)
            .min(need - raised);
        if pay == 0 {
            continue;
        }
        world.get_mut::<Wallet>(*actor).expect("payer").coins -= pay;
        raised += pay;
        if raised >= need {
            break;
        }
    }
    let mut state = world.resource_mut::<VillageState>();
    state.fund = state.fund.saturating_add(raised);
    Some((raised, shares.len(), capacity))
}

/// The relief levy: when the fund cannot pay the destitution grants of the
/// day, the `shortfall` is raised from coins above the floor, at most
/// `relief_levy_percent` of them each. Returns the coins raised.
pub(crate) fn raise_relief_levy(world: &mut World, shortfall: u32) -> u32 {
    let Some(percent) = economy_spec::tuning(world).map(|tuning| tuning.relief_levy_percent) else {
        return 0;
    };
    let Some((raised, payers, capacity)) = collect(world, shortfall, percent) else {
        return 0;
    };
    record_effect(
        world,
        None,
        "VillageReliefLevy",
        &json!({"raised":raised,"needed":shortfall,"payers":payers,"capacity":capacity,
            "percent":percent}),
    );
    raised
}

/// Daily upkeep of the earmark: it never exceeds the fund (grants come first)
/// nor what open sites and deferred projects still need.
pub(crate) fn settle_earmark(world: &mut World) {
    let fund = world.resource::<VillageState>().fund;
    let hold = fund_hold(world);
    if let Some(mut civic) = world.get_resource_mut::<CivicState>() {
        civic.earmarked = civic.earmarked.min(fund).min(hold);
    }
}

/// Why a site is, or is not, being supplied: one sentence for players.
pub fn funding(world: &World, site: &ConstructionSite) -> String {
    if site.supplied() {
        return "All materials are delivered.".into();
    }
    let missing: u32 = site
        .cost
        .keys()
        .map(|kind| {
            site.missing(kind)
                .saturating_mul(market::unit_price(world, kind).unwrap_or(0))
        })
        .fold(0, u32::saturating_add);
    let budget = site_budget(world);
    if budget >= missing {
        format!(
            "Funded: the materials still to come cost about {missing} coins and the civic fund \
             may spend {budget}."
        )
    } else {
        format!(
            "Short of coins: the materials still to come cost about {missing} coins, the civic \
             fund may spend {budget} (it keeps {} for destitution grants) and the building levy \
             can raise up to {} more.",
            reserve(world),
            levy_capacity(world)
        )
    }
}

/// Coins the fund keeps back for destitution grants: the fixed floor
/// `civic_reserve`, or `civic_reserve_grant_days` days of its recent
/// shortfall (grants paid above the levy collected, averaged over the last
/// levy days) when that is more. A run of days on which the grants exceed the
/// levy therefore raises the reserve before building and importing can drain
/// the fund; a fund that merely passes the levy on to the poor keeps the floor.
pub fn reserve(world: &World) -> u32 {
    let Some(tuning) = economy_spec::tuning(world) else {
        return 0;
    };
    let recent = world.get_resource::<CivicState>().map_or(0, |state| {
        let days = state.grant_history.len().max(1) as u64;
        let total: u64 = state
            .grant_history
            .iter()
            .map(|grant| u64::from(*grant))
            .sum();
        (total * u64::from(tuning.civic_reserve_grant_days) / days).min(u64::from(u32::MAX)) as u32
    });
    tuning.civic_reserve.max(recent)
}

/// Notes one levy day for [`reserve`]: the grants paid above the levy.
pub(crate) fn note_grants(world: &mut World, granted: u32, contributed: u32) {
    let granted = granted.saturating_sub(contributed);
    let window = economy_spec::tuning(world)
        .map_or(7, |tuning| tuning.civic_reserve_window_days.max(1)) as usize;
    if let Some(mut state) = world.get_resource_mut::<CivicState>() {
        state.grant_history.push(granted);
        let extra = state.grant_history.len().saturating_sub(window);
        state.grant_history.drain(..extra);
    }
}

/// Open sites of a settlement, ascending by identity.
pub fn open_sites(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    state
        .sites
        .get(&settlement)
        .into_iter()
        .flatten()
        .filter_map(|id| Some((*id, world.resource::<SimIndex>().get(*id)?)))
        // Builders and haulers take building sites before prop sites; each
        // group keeps its placing order (a stable partition).
        .partition::<Vec<_>, _>(|(_, entity)| {
            !world
                .get::<ConstructionSite>(*entity)
                .is_some_and(ConstructionSite::is_prop)
        })
        .into_concat()
}

trait IntoConcat {
    fn into_concat(self) -> Vec<(SimId, Entity)>;
}
impl IntoConcat for (Vec<(SimId, Entity)>, Vec<(SimId, Entity)>) {
    fn into_concat(self) -> Vec<(SimId, Entity)> {
        let (mut buildings, props) = self;
        buildings.extend(props);
        buildings
    }
}

/// Materials the open sites of a settlement still wait for and the fund can
/// pay at today's quotes, as the funded production plan's civic demand.
pub fn material_orders(world: &World, settlement: SimId) -> Goods {
    let mut orders = Goods::new();
    let mut coins = site_budget(world);
    for (_, entity) in open_sites(world, settlement) {
        let Some(site) = world.get::<ConstructionSite>(entity) else {
            continue;
        };
        for kind in site.cost.keys() {
            let price = market::unit_price(world, kind).unwrap_or(u32::MAX).max(1);
            let units = site.missing(kind).min(coins / price);
            if units > 0 {
                coins -= units * price;
                *orders.entry(kind.clone()).or_default() += units;
            }
        }
    }
    orders
}

/// Coins needed to finish paying for every open site at today's quotes.
pub fn outstanding_cost(world: &World) -> u32 {
    let Some(state) = world.get_resource::<CivicState>() else {
        return 0;
    };
    state
        .sites
        .values()
        .flatten()
        .filter_map(|id| world.resource::<SimIndex>().get(*id))
        .filter_map(|entity| world.get::<ConstructionSite>(entity))
        .flat_map(|site| {
            site.cost.keys().map(|kind| {
                site.missing(kind)
                    .saturating_mul(market::unit_price(world, kind).unwrap_or(0))
            })
        })
        .fold(0, u32::saturating_add)
}

/// Coins the fund may pay for building materials: its budget, or what it
/// holds for open sites (`fund_hold`, which floor grants do not take),
/// whichever is more.
pub fn site_budget(world: &World) -> u32 {
    let fund = world
        .get_resource::<VillageState>()
        .map_or(0, |state| state.fund);
    budget(world).max(fund_hold(world).min(fund))
}

/// What the fund withholds from the daily dividend for open sites.
pub fn fund_hold(world: &World) -> u32 {
    let slack = economy_spec::tuning(world).map_or(0, |tuning| tuning.civic_budget_slack_percent);
    // Open sites, plus the projects the planners have decided on and could
    // not yet pay for: those coins are not handed back as a dividend.
    let pending: u64 = world.get_resource::<CivicState>().map_or(0, |state| {
        state
            .planners
            .values()
            .map(|planner| u64::from(planner.pending_cost))
            .sum()
    });
    let cost = u64::from(outstanding_cost(world)) + pending;
    (cost * (100 + u64::from(slack)) / 100).min(u64::from(u32::MAX)) as u32
}

/// The same validation a delivery performs, without changing anything.
/// Returns the coins the fund would pay.
pub fn check_delivery(
    world: &World,
    actor: Entity,
    site: Entity,
    kind: &str,
    count: u32,
) -> Result<u32, String> {
    if !living(world, actor) {
        return Err("only a living actor can deliver materials".into());
    }
    let state = world
        .get::<ConstructionSite>(site)
        .ok_or("target is not a building site")?;
    if !(1..=20).contains(&count) {
        return Err("quantity must be 1..=20".into());
    }
    if !econ::near(world, actor, site) {
        return Err("move within 1.5 m of the building site".into());
    }
    let hauler = world.get::<SimId>(actor).copied();
    let now = world.resource::<SimClock>().tick;
    if state.open(kind, hauler, now) < count {
        return Err("the site does not need that many of this material".into());
    }
    if econ::count(world, actor, kind) < count {
        return Err(format!("carry {count} unreserved {kind} to the site first"));
    }
    let price = market::unit_price(world, kind).ok_or("this good is not traded here")?;
    let total = price.checked_mul(count).ok_or("price overflow")?;
    if total > site_budget(world) {
        return Err("the civic fund cannot pay for these materials yet".into());
    }
    if world
        .get::<Wallet>(actor)
        .and_then(|wallet| wallet.coins.checked_add(total))
        .is_none()
    {
        return Err("the carrier cannot be paid".into());
    }
    Ok(total)
}

/// Hands materials over at the site. The civic fund pays the carrier the
/// quoted price; no coin or good is created or lost.
pub fn deliver(
    world: &mut World,
    actor: Entity,
    site: Entity,
    kind: &str,
    count: u32,
) -> Result<(), String> {
    let total = check_delivery(world, actor, site, kind, count)?;
    econ::consume(world, actor, kind, count)?;
    let hauler = *world.get::<SimId>(actor).expect("checked actor");
    let site_id = *world.get::<SimId>(site).expect("site identity");
    let delivered_at = world.resource::<SimClock>().tick;
    let supplied = {
        let mut state = world
            .get_mut::<ConstructionSite>(site)
            .expect("checked site");
        *state.delivered.entry(kind.into()).or_default() += count;
        state.incoming.remove(&hauler);
        state.paid = state.paid.saturating_add(total);
        state.last_delivery_at = Some(delivered_at);
        state.supplied()
    };
    world.resource_mut::<VillageState>().fund -= total;
    {
        let mut civic = world.resource_mut::<CivicState>();
        civic.earmarked = civic.earmarked.saturating_sub(total);
    }
    world
        .get_mut::<Wallet>(actor)
        .expect("checked wallet")
        .coins += total;
    let mut civic = world.resource_mut::<CivicState>();
    civic.spent = civic.spent.saturating_add(u64::from(total));
    crate::village::colony_demand_changed(world);
    record_effect(
        world,
        Some(hauler),
        "VillageSiteDelivery",
        &json!({"site":site_id,"kind":kind,"quantity":count,"coins":total,
            "unit_price":total / count,"supplied":supplied}),
    );
    Ok(())
}

/// The site in the actor's settlement that is supplied, unfinished and free
/// for a builder, if any.
pub fn work_target(world: &World, actor: Entity) -> Option<(SimId, Entity)> {
    let settlement = settlements::member(world, actor)?;
    let me = *world.get::<SimId>(actor)?;
    let now = world.resource::<SimClock>().tick;
    open_sites(world, settlement)
        .into_iter()
        .find(|(id, entity)| {
            world.get::<ConstructionSite>(*entity).is_some_and(|site| {
                site.supplied() && site.work_done < site.work_ticks && site.free_for(me, now)
            }) && world.get::<SimPos>(*entity).is_some_and(|pos| {
                !crate::village::routing::avoiding(world, actor, *pos, Some(*id))
            })
        })
}

/// Starts one stage of building work at a supplied site.
pub fn begin_work(world: &mut World, actor: Entity, site: Entity) -> Result<(), String> {
    if !living(world, actor) {
        return Err("only a living actor can build".into());
    }
    let state = world
        .get::<ConstructionSite>(site)
        .ok_or("target is not a building site")?;
    if !state.supplied() {
        return Err("the site still waits for materials".into());
    }
    let remaining = state.work_ticks.saturating_sub(state.work_done);
    if remaining == 0 {
        return Err("the building is finished".into());
    }
    if !econ::near(world, actor, site) {
        return Err("move within 1.5 m of the building site".into());
    }
    if world.get::<SiteWork>(actor).is_some()
        || world.get::<Gathering>(actor).is_some()
        || world.get::<Crafting>(actor).is_some()
        || world
            .get::<crate::village::hunting::Hunting>(actor)
            .is_some()
        || world
            .get::<crate::village::homestead::CottageBuild>(actor)
            .is_some()
    {
        return Err("already working".into());
    }
    let me = *world.get::<SimId>(actor).ok_or("actor has no identity")?;
    let site_id = *world.get::<SimId>(site).expect("site identity");
    if !state.free_for(me, world.resource::<SimClock>().tick) {
        return Err("another builder is at work on this site".into());
    }
    let stage = economy_spec::tuning(world).map_or(200, |tuning| tuning.build_stage_ticks);
    let ticks = remaining.min(stage);
    let pace = u64::from(crate::village::colony::work_pace_percent(world, actor));
    let tool = u64::from(build_tool_percent(world, actor));
    let now = world.resource::<SimClock>().tick;
    let mut ticks = ticks;
    let mut ends = now.saturating_add((u64::from(ticks) * pace / 100 * tool / 100).max(1));
    // Resume a stage a critical need interrupted at this site (S3.5).
    if let Some(suspended) = world.get::<super::model::SuspendedWork>(actor).copied() {
        world
            .entity_mut(actor)
            .remove::<super::model::SuspendedWork>();
        if suspended.site == site_id && suspended.expires_at > now {
            ticks = suspended.ticks.min(remaining);
            ends = now.saturating_add(suspended.remaining);
            record_effect(
                world,
                Some(me),
                "VillageWorkResumed",
                &json!({"site":site_id,"ticks_left":suspended.remaining}),
            );
        }
    }
    world
        .entity_mut(actor)
        .remove::<MoveIntent>()
        .insert(SiteWork {
            site: site_id,
            ticks,
            ends,
        });
    claim(world, site, me, ends);
    Ok(())
}

/// The builder's tool: the tool of the trade that builds (a hammer).
pub(crate) fn builder_tool(world: &World) -> Option<&'static str> {
    crate::village::catalog::role_tool_in(world, crate::village::roles::roles(world).builder)
}

/// Stage time in percent for `actor`: `tuning.build_tool_percent` with a
/// usable builder's tool, `build_bare_percent` without (S2.2).
pub fn build_tool_percent(world: &World, actor: Entity) -> u32 {
    let Some(tuning) = economy_spec::tuning(world) else {
        return 100;
    };
    match builder_tool(world) {
        Some(tool) if crate::village::living::usable_tool(world, actor, tool).is_some() => {
            tuning.build_tool_percent
        }
        Some(_) => tuning.build_bare_percent,
        None => 100,
    }
}

/// Marks `builder` as the resident on the way to, or at work on, the site.
pub(super) fn claim(world: &mut World, site: Entity, builder: SimId, until: u64) {
    if let Some(mut state) = world.get_mut::<ConstructionSite>(site)
        && (state.builder != Some(builder) || state.builder_until != until)
    {
        state.builder = Some(builder);
        state.builder_until = until;
    }
}

/// Stops building work without crediting the unfinished stage.
pub fn cancel_work(world: &mut World, actor: Entity) {
    // A read first: profiles without building sites never register the type.
    if world.get::<SiteWork>(actor).is_none() {
        return;
    }
    let Some(work) = world.entity_mut(actor).take::<SiteWork>() else {
        return;
    };
    let me = world.get::<SimId>(actor).copied();
    if let Some(mut state) = world
        .resource::<SimIndex>()
        .get(work.site)
        .and_then(|site| world.get_mut::<ConstructionSite>(site))
        && state.builder == me
    {
        state.builder = None;
        state.builder_until = 0;
    }
}

/// Ends the building work of everybody at a site that is no longer one.
fn release_workers(world: &mut World, site: SimId) {
    let workers: Vec<Entity> = world
        .query::<(Entity, &SiteWork)>()
        .iter(world)
        .filter(|(_, work)| work.site == site)
        .map(|(entity, _)| entity)
        .collect();
    for worker in workers {
        world.entity_mut(worker).remove::<SiteWork>();
    }
}

/// Credits finished stages and completes buildings; called once per tick.
/// Without an open site nobody can be building, so nothing is looked at.
pub(super) fn tick(world: &mut World, now: u64) {
    if world
        .resource::<CivicState>()
        .sites
        .values()
        .all(Vec::is_empty)
    {
        return;
    }
    let mut workers: Vec<_> = world
        .query::<(Entity, &SimId, &SiteWork)>()
        .iter(world)
        .map(|(entity, id, work)| (*id, entity, *work))
        .collect();
    workers.sort_by_key(|(id, _, _)| *id);
    for (id, actor, work) in workers {
        if !living(world, actor) {
            cancel_work(world, actor);
            continue;
        }
        if work.ends > now {
            continue;
        }
        cancel_work(world, actor);
        let Some(site) = world.resource::<SimIndex>().get(work.site) else {
            continue;
        };
        let Some(mut state) = world.get_mut::<ConstructionSite>(site) else {
            continue;
        };
        state.work_done = state
            .work_done
            .saturating_add(work.ticks)
            .min(state.work_ticks);
        let (done, total) = (state.work_done, state.work_ticks);
        // A finished stage wears the builder's tool (S2.2).
        if let Some(tool) = builder_tool(world) {
            crate::village::living::wear_tool(world, actor, id, tool);
        }
        record_effect(
            world,
            Some(id),
            "VillageSiteProgress",
            &json!({"site":work.site,"work_done":done,"work_ticks":total}),
        );
        if done >= total {
            complete(world, work.site, Some(id));
        }
    }
}

/// Turns a finished site into its building. The entity and `SimId` stay.
fn complete(world: &mut World, site_id: SimId, builder: Option<SimId>) {
    finish(world, site_id, builder, false);
}

/// Lays out a building that stands from the founding: no materials, no work
/// and no entry in the construction counters. Returns the building.
pub(crate) fn found(
    world: &mut World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
    reason: &str,
) -> Result<SimId, String> {
    let site = super::placement::place_site(world, building, x, z, rotation, true, reason, false)?;
    finish(world, site, None, true);
    Ok(site)
}

/// Lays out a building of an exported world: a finished building, or an
/// open site waiting for its materials. Returns the building.
pub(crate) fn lay_out(
    world: &mut World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
    finished: bool,
) -> Result<SimId, String> {
    let site = super::placement::place_site(
        world,
        building,
        x,
        z,
        rotation,
        false,
        "laid out from an exported world",
        false,
    )?;
    if finished {
        finish(world, site, None, false);
    }
    Ok(site)
}

fn finish(world: &mut World, site_id: SimId, builder: Option<SimId>, founding: bool) {
    let Some(entity) = world.resource::<SimIndex>().get(site_id) else {
        return;
    };
    let Some(site) = world.entity_mut(entity).take::<ConstructionSite>() else {
        return;
    };
    // A prop site becomes the prop: the site and its body go, the prop
    // stands where it was placed.
    if let super::model::SiteKind::Prop { x, z, rotation } = site.kind {
        release_workers(world, site_id);
        {
            let mut civic = world.resource_mut::<CivicState>();
            if let Some(sites) = civic.sites.get_mut(&site.settlement) {
                sites.retain(|id| *id != site_id);
            }
        }
        if let Some(blocker) = site
            .blocker
            .and_then(|id| world.resource::<SimIndex>().get(id))
        {
            world.despawn(blocker);
        }
        world.despawn(entity);
        crate::village::colony_demand_changed(world);
        let prop = crate::village::frontier::props::spawn_built(
            world,
            &site.building,
            site.plot,
            site.settlement,
            x,
            z,
            rotation,
        );
        record_effect(
            world,
            builder,
            "LivingPropBuilt",
            &json!({"site":site_id,"prop":prop,"kind":site.building,"x":x,"z":z,
                "rotation":rotation,"settlement":site.settlement}),
        );
        return;
    }
    let Some(def) = economy_spec::registry(world)
        .and_then(|registry| registry.spec.building(&site.building))
        .cloned()
    else {
        return;
    };
    release_workers(world, site_id);
    let now = world.resource::<SimClock>().tick;
    let door = site.plot.door();
    let seats = match &def.kind {
        BuildingKind::Workplace { capacity, .. } => *capacity,
        _ => 0,
    };
    world.entity_mut(entity).insert(Building {
        building: site.building.clone(),
        label: site.label.clone(),
        settlement: site.settlement,
        plot: site.plot,
        blocker: site.blocker,
        completed_at: now,
        seats,
    });
    let mut depot = false;
    match &def.kind {
        BuildingKind::Home { beds } => {
            world.entity_mut(entity).insert(Household {
                label: site.label.clone(),
                capacity: *beds,
                residents: Vec::new(),
                door,
                heated_until: 0,
                logs_burned: 0,
            });
        }
        BuildingKind::Workplace {
            profession,
            station,
            ..
        } => {
            world.entity_mut(entity).insert((
                VillagePlace {
                    role: *profession,
                    label: site.label.clone(),
                },
                Workstation::new([station.as_str()]),
                Approach(door),
            ));
        }
        BuildingKind::Storage { capacity } => {
            depot = true;
            world
                .entity_mut(entity)
                .insert(Depot::new(*capacity, false));
        }
        BuildingKind::Stall { capacity } => {
            depot = true;
            world.entity_mut(entity).insert(Depot::new(*capacity, true));
        }
    }
    {
        let mut civic = world.resource_mut::<CivicState>();
        if let Some(sites) = civic.sites.get_mut(&site.settlement) {
            sites.retain(|id| *id != site_id);
        }
        let buildings = civic.buildings.entry(site.settlement).or_default();
        buildings.push(site_id);
        buildings.sort_unstable();
        if depot {
            let depots = civic.depots.entry(site.settlement).or_default();
            depots.push(site_id);
            depots.sort_unstable();
        }
        if founding {
            civic.founded.push(site_id);
            civic.founded.sort_unstable();
        } else {
            civic.completed += 1;
        }
        if let Some(planner) = civic.planners.get_mut(&site.settlement)
            && planner.project == Some(site_id)
        {
            planner.project = None;
        }
    }
    if let Some(field) = &def.field {
        crate::village::fields::established(world, entity, field.plots, &field.crop, 0);
        // The store of a field is its granary: seed and harvest, nothing else.
        let mut crop: Vec<String> = economy_spec::registry(world)
            .and_then(|registry| registry.spec.crop(&field.crop))
            .map(|crop| vec![crop.seed.good.clone(), crop.harvest.good.clone()])
            .unwrap_or_default();
        crop.sort();
        crop.dedup();
        if let Some(mut depot) = world.get_mut::<Depot>(entity) {
            depot.only = crop;
        }
    }
    crate::village::colony_demand_changed(world);
    if founding {
        record_effect(
            world,
            None,
            "VillageBuildingFounded",
            &json!({"building":site_id,"definition":site.building,"kind":def.kind.name(),
                "label":site.label,"settlement":site.settlement,"reason":site.reason}),
        );
        return;
    }
    record_effect(
        world,
        builder,
        "VillageBuildingCompleted",
        &json!({"building":site_id,"definition":site.building,"kind":def.kind.name(),
            "label":site.label,"settlement":site.settlement,"civic":site.civic,
            "paid":site.paid,"started":site.placed_at}),
    );
    if site.civic {
        record_effect(
            world,
            None,
            "VillageCivicProject",
            &json!({"settlement":site.settlement,"decision":"completed","building":site.building,
                "site":site_id,"reason":site.reason,"paid":site.paid}),
        );
    }
    if matches!(def.kind, BuildingKind::Home { .. }) {
        crate::village::demography::rehouse_families(world, site.settlement);
        super::planner::house_homeless(world, site.settlement);
    }
}

/// Abandons an unfinished site. Delivered materials become civic stock of
/// the nearest store of the settlement with room; the rest is laid on the
/// ground at the entrance. Coins already paid stay paid: the goods are the
/// settlement's.
pub fn cancel(world: &mut World, site_id: SimId, reason: &str) {
    let Some(entity) = world.resource::<SimIndex>().get(site_id) else {
        return;
    };
    let Some(site) = world.entity_mut(entity).take::<ConstructionSite>() else {
        return;
    };
    release_workers(world, site_id);
    let door = site.plot.door();
    let mut depots: Vec<(i64, SimId, Entity)> = super::depot::depots(world, site.settlement)
        .into_iter()
        .filter_map(|(id, depot)| Some((world.get::<SimPos>(depot)?.dist2(door), id, depot)))
        .collect();
    depots.sort_unstable_by_key(|(distance, id, _)| (*distance, *id));
    let mut stored = Goods::new();
    let mut dropped = Goods::new();
    let mut store = None;
    for (kind, count) in &site.delivered {
        let mut left = *count;
        for (_, id, depot) in &depots {
            let Some(mut state) = world.get_mut::<Depot>(*depot) else {
                continue;
            };
            let part = if state.accepts(kind) {
                left.min(state.free())
            } else {
                0
            };
            if part > 0 {
                state.add(kind, CIVIC_OWNER, part);
                left -= part;
                *stored.entry(kind.clone()).or_default() += part;
                store.get_or_insert(*id);
            }
            if left == 0 {
                break;
            }
        }
        if left > 0 {
            *dropped.entry(kind.clone()).or_default() += left;
            let stack = items::kind_def(world, kind)
                .filter(|def| def.stackable)
                .map_or(1, |def| def.max_stack.max(1));
            while left > 0 {
                let part = left.min(stack);
                spawn_sim(
                    world,
                    (
                        door,
                        Item::new(kind).stack(part),
                        Perceivable::new(crate::world::kinds::ITEM),
                    ),
                );
                left -= part;
            }
        }
    }
    {
        let mut civic = world.resource_mut::<CivicState>();
        if let Some(sites) = civic.sites.get_mut(&site.settlement) {
            sites.retain(|id| *id != site_id);
        }
        civic.cancelled += 1;
        if let Some(planner) = civic.planners.get_mut(&site.settlement)
            && planner.project == Some(site_id)
        {
            planner.project = None;
        }
    }
    if let Some(blocker) = site
        .blocker
        .and_then(|id| world.resource::<SimIndex>().get(id))
    {
        world.despawn(blocker);
    }
    world.despawn(entity);
    crate::village::colony_demand_changed(world);
    record_effect(
        world,
        None,
        "VillageSiteCancelled",
        &json!({"site":site_id,"building":site.building,"settlement":site.settlement,
            "reason":reason,"stored":stored,"store":store,"dropped":dropped,"paid":site.paid,
            "label":site.label,"pos":[door.x, door.z],"cell":[site.plot.x, site.plot.z]}),
    );
    if site.civic {
        record_effect(
            world,
            None,
            "VillageCivicProject",
            &json!({"settlement":site.settlement,"decision":"cancelled","building":site.building,
                "site":site_id,"reason":reason}),
        );
    }
}

/// Registers a hauler's promise to bring `count` of `kind`.
pub(super) fn promise(
    world: &mut World,
    site: Entity,
    hauler: SimId,
    kind: &str,
    count: u32,
    loaded: bool,
) {
    let now = world.resource::<SimClock>().tick;
    let ticks = economy_spec::tuning(world).map_or(1200, |tuning| tuning.haul_reservation_ticks);
    if let Some(mut state) = world.get_mut::<ConstructionSite>(site) {
        let loaded = loaded
            || state
                .incoming
                .get(&hauler)
                .is_some_and(|hold| hold.loaded && hold.kind == kind);
        let hold = Hold {
            kind: kind.into(),
            count,
            until: now.saturating_add(ticks),
            loaded,
        };
        if state.incoming.get(&hauler) != Some(&hold) {
            state.incoming.insert(hauler, hold);
        }
    }
}

/// Drops promises that ran out, so a site never waits on a lost hauler.
pub(super) fn expire(world: &mut World, settlement: SimId, now: u64) {
    for (_, entity) in open_sites(world, settlement) {
        if world
            .get::<ConstructionSite>(entity)
            .is_some_and(|site| site.incoming.values().any(|hold| hold.until <= now))
            && let Some(mut site) = world.get_mut::<ConstructionSite>(entity)
        {
            site.incoming.retain(|_, hold| hold.until > now);
        }
    }
}

/// Takes a finished building off the map (the Shape tools' undo of a placed
/// plot): refused while it holds goods or seed, a crop grows on it, or
/// somebody works there. Nothing it held is lost, because it holds nothing.
pub(crate) fn remove_finished(world: &mut World, building_id: SimId) -> Result<(), String> {
    let (entity, building) = removable(world, building_id)?;
    {
        let mut civic = world.resource_mut::<CivicState>();
        if let Some(list) = civic.buildings.get_mut(&building.settlement) {
            list.retain(|id| *id != building_id);
        }
        if let Some(list) = civic.depots.get_mut(&building.settlement) {
            list.retain(|id| *id != building_id);
        }
        civic.completed = civic.completed.saturating_sub(1);
    }
    if let Some(mut state) = world.get_resource_mut::<crate::village::fields::FieldState>()
        && let Some(list) = state.fields.get_mut(&building.settlement)
    {
        list.retain(|id| *id != building_id);
    }
    if let Some(blocker) = building
        .blocker
        .and_then(|id| world.resource::<SimIndex>().get(id))
    {
        world.despawn(blocker);
    }
    world.despawn(entity);
    crate::village::colony_demand_changed(world);
    Ok(())
}

/// Whether [`remove_finished`] may take the building away now, and if not
/// why, in one sentence.
pub(crate) fn removable(world: &World, building_id: SimId) -> Result<(Entity, Building), String> {
    let entity = world
        .resource::<SimIndex>()
        .get(building_id)
        .ok_or("that building no longer exists")?;
    let building = world
        .get::<Building>(entity)
        .cloned()
        .ok_or("that is not a finished building")?;
    if world
        .get::<Depot>(entity)
        .is_some_and(|depot| depot.lots.values().any(|lot| lot.values().any(|n| *n > 0)))
    {
        return Err("goods are stored there; they must be taken out first".into());
    }
    if let Some(field) = world.get::<crate::village::fields::Field>(entity) {
        if field.seed_stock > 0 {
            return Err("seed is stored there; it must be sown or taken out first".into());
        }
        if field.plots.iter().any(|plot| {
            plot.stage != crate::village::fields::PlotStage::Fallow || plot.claimed_by.is_some()
        }) {
            return Err("a crop grows there; wait for the harvest".into());
        }
    }
    let busy = world
        .try_query::<(
            &crate::village::Villager,
            Option<&crate::village::fields::FieldWork>,
        )>()
        .is_some_and(|mut query| {
            query.iter(world).any(|(person, work)| {
                person.workplace == building_id
                    || work.is_some_and(|work| work.field == building_id)
            })
        });
    if busy {
        return Err("somebody works there; try again when the task ends".into());
    }
    Ok((entity, building))
}
