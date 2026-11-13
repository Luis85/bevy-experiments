//! Construction and cold exposure for the opt-in living island.
use super::{Role, VillagePlace, Villager, Vitality, Wallet, catalog, economy, model::Season};
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::{Durability, Workstation};
use npc_framework::interaction::{Equipment, Item, items};
use npc_framework::needs::{NeedDef, NeedRegistry, NeedThresholds, Needs};
use npc_framework::prelude::*;
use serde_json::json;

pub const EXTRA_RESIDENTS: [(Role, i32, i32, &str); 3] = [
    (Role::Hunter, -15, -10, "Rhea's hunting camp"),
    (Role::Tanner, -8, -10, "Sage's tannery"),
    (Role::Tailor, 0, -12, "Lina's tailor shop"),
];

pub fn register(app: &mut App) {
    app.register_need(NeedDef {
        name: "Warmth",
        key: "warmth",
        order: 3,
        decay_per_tick: 0,
        thresholds: NeedThresholds {
            low_enter: 50_000,
            low_exit: 60_000,
            urgent_enter: 20_000,
            urgent_exit: 30_000,
        },
    });
}

pub fn prepare_person(world: &mut World, actor: Entity) {
    if let Some(mut equipment) = world.get_mut::<Equipment>(actor) {
        equipment.slots.entry("body".into()).or_insert(None);
    } else {
        world
            .entity_mut(actor)
            .insert(Equipment::new(["main_hand", "off_hand", "body"]));
    }
}

/// Append all living-mode construction after the incumbent actors and seed stock.
pub fn spawn(world: &mut World) {
    let ids: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| {
            world
                .get::<Villager>(*e)
                .is_some_and(|v| v.role != Role::Visitor)
        })
        .collect();
    // A small finite bootstrap; demand planning, rather than seed-stock quotas,
    // must supply later needs. Removing stock does not recycle assigned SimIds.
    for actor in ids {
        let role = world.get::<Villager>(actor).expect("resident").role;
        for kind in catalog::outputs(role) {
            let extra = economy::count(world, actor, kind).saturating_sub(2);
            if extra > 0 {
                let _ = economy::consume(world, actor, kind, extra);
            }
        }
        for kind in ["bread", catalog::WATER] {
            let keep = if catalog::outputs(role).contains(&kind) {
                2
            } else {
                1
            };
            let extra = economy::count(world, actor, kind).saturating_sub(keep);
            if extra > 0 {
                let _ = economy::consume(world, actor, kind, extra);
            }
        }
    }
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(super::behavior::TREE)
        .expect("life tree");
    for (role, x, z, label) in EXTRA_RESIDENTS {
        let pos = crate::layout::cell(x, z);
        let station_kind = match role {
            Role::Tanner => catalog::TAN,
            Role::Tailor => catalog::TAILOR,
            _ => catalog::INN,
        };
        let (station, _) = spawn_sim(
            world,
            (
                pos,
                VillagePlace {
                    role,
                    label: label.into(),
                },
                Workstation::new([station_kind]),
                Approach(pos),
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        spawn_sim(
            world,
            (
                SimPos::new(pos.x, pos.z + 2000),
                GridBlocker(Footprint {
                    half_x_mm: 1000,
                    half_z_mm: 1000,
                }),
                GridOccluder(Footprint {
                    half_x_mm: 1000,
                    half_z_mm: 1000,
                }),
            ),
        );
        let home = SimPos::new(pos.x - 1000, pos.z - 1000);
        let spec = crate::layout::UnitSpec {
            template: "novice".into(),
            behaviour: crate::Behaviour::Idle,
            ..Default::default()
        };
        let id = crate::layout::spawn_unit(world, SimPos::new(pos.x, pos.z - 1000), &spec)
            .expect("resident template");
        let actor = world.resource::<SimIndex>().get(id).expect("resident");
        super::initialise_person(world, actor, role, role.person(), home, station);
        world
            .entity_mut(actor)
            .remove::<(BrainState, ActiveLeaf)>()
            .insert((Brain { tree }, Inventory::new(64, 500_000)));
        world.get_mut::<Wallet>(actor).expect("wallet").coins = super::NPC_SEED_COINS;
        for kind in ["bread", catalog::WATER] {
            economy::grant(world, actor, Item::new(kind));
        }
        if role == Role::Hunter {
            for _ in 0..2 {
                economy::grant(world, actor, Item::new(catalog::SPEAR));
            }
        }
        if role == Role::Tanner {
            economy::grant(world, actor, Item::new(catalog::BOWL));
        }
        let mut ids = world.resource_mut::<crate::IslandIds>();
        ids.name(format!("village_{}", role.tag()), station);
        ids.name(role.person(), id);
        ids.units.push(id);
    }
    let day = super::calendar(world).0;
    world
        .resource_mut::<super::VillageState>()
        .last_dividend_day = day;
}

pub fn cold(world: &World) -> bool {
    {
        let (_, season, night) = super::calendar(world);
        night || season == Season::Winter || super::weather::snowing(world)
    }
}

/// Living world: winter, or the last `tuning.coat_lead_days` of autumn.
pub fn coat_season(world: &World) -> bool {
    let Some(lead) = super::frontier::economy_spec::tuning(world).map(|t| t.coat_lead_days) else {
        return false;
    };
    let (day, season, _) = super::calendar(world);
    let per_season = super::days_per_season(world).max(1);
    match season {
        Season::Winter => true,
        Season::Autumn => per_season - day % per_season <= u64::from(lead),
        _ => false,
    }
}

/// Living world: coins the civic fund may give a resident towards a coat now.
/// In the coat season a coat grant is a survival grant and may draw on the
/// destitution reserve; in any other season only on the balance above it.
/// Never on coins the building levy raised for sites.
pub fn coat_grant_room(world: &World) -> u32 {
    let Some(state) = world.get_resource::<super::VillageState>() else {
        return 0;
    };
    let earmarked = world
        .get_resource::<super::civic::CivicState>()
        .map_or(0, |civic| civic.earmarked);
    let free = state.fund.saturating_sub(earmarked);
    if coat_season(world) {
        free
    } else {
        free.saturating_sub(super::civic::reserve(world))
    }
}

/// Living world: a resident without a coat who should get one now: in the
/// coat season always; in any other season while a coat is on offer in its
/// settlement and it, or the fund's balance above its reserve, can pay.
pub fn coat_wanted_now(world: &World, actor: Entity) -> bool {
    if has_coat(world, actor)
        || economy::count(world, actor, super::roles::roles(world).garment) > 0
    {
        return false;
    }
    if coat_season(world) {
        return true;
    }
    if !super::frontier::economy_spec::tuning(world).is_some_and(|t| t.coat_grants_any_season) {
        return false;
    }
    let Some(settlement) = super::frontier::settlements::member(world, actor) else {
        return false;
    };
    let Some(price) = super::market::unit_price(world, super::roles::roles(world).garment) else {
        return false;
    };
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    // The stock on offer is the dearest test: last. Coats are never stored
    // (clothing is not storable), so a coat maker of the settlement with one
    // to spare is what is on offer.
    (coins >= price || coat_grant_room(world) >= price - coins)
        && super::demography::members(world, settlement)
            .into_iter()
            .any(|(_, maker)| {
                world.get::<Villager>(maker).is_some_and(|person| {
                    catalog::outputs_for(world, person.role)
                        .contains(&super::roles::roles(world).garment)
                }) && super::market::available_to_sell(
                    world,
                    maker,
                    super::roles::roles(world).garment,
                ) > 0
            })
}

/// Living world: out in the cold without a coat and below
/// `tuning.exposure_warmth`, so no outdoor errand should start.
pub fn exposed(world: &World, actor: Entity) -> bool {
    super::frontier::economy_spec::tuning(world).is_some_and(|tuning| {
        cold(world)
            && !has_coat(world, actor)
            && economy::need(world, actor, "Warmth") < tuning.exposure_warmth
    })
}

/// Living world, every 20 ticks: a resident who carries a coat and wears none
/// (or only a worn-out one) puts it on at once, in any season; the worn-out
/// one goes back to the pack. Sorted by id.
pub fn wear_carried_coats(world: &mut World) {
    if !world.resource::<SimClock>().tick.is_multiple_of(20) {
        return;
    }
    let mut people: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    for (id, actor) in people {
        if !super::living(world, actor) || has_coat(world, actor) {
            continue;
        }
        let Some(coat) = super::economy::free_items(world, actor)
            .into_iter()
            .filter(|(_, item)| item.kind == super::roles::roles(world).garment)
            .filter_map(|(item, _)| world.resource::<SimIndex>().get(item).map(|e| (item, e)))
            .find(|(_, item)| {
                world
                    .get::<Durability>(*item)
                    .is_none_or(|durability| durability.current > 0)
            })
        else {
            continue;
        };
        if world.get::<Equipment>(actor).is_none() {
            continue;
        }
        // A worn-out coat in the slot goes back to the pack first.
        if let Some(old) =
            items::equipped_matching(world, actor, super::roles::roles(world).garment)
                .and_then(|old| world.resource::<SimIndex>().get(old))
        {
            items::put_in_inventory(world, actor, old);
        }
        // A coat from a pack (a purchase, a gift) may carry no wear yet.
        if world.get::<Durability>(coat.1).is_none() {
            let full = catalog::durability_in(world, super::roles::roles(world).garment)
                .unwrap_or(catalog::TOOL_DURABILITY);
            world.entity_mut(coat.1).insert(Durability::new(full));
        }
        items::put_in_slot(world, actor, coat.1, "body");
        record_effect(world, Some(id), "VillageCoatWorn", &json!({"coat":coat.0}));
    }
}

/// Living world: a founder of the first families who comes wearing a coat:
/// `tuning.founder_coats_per_three` of every three founders in founding
/// order (`ordinal`, which runs over settlements, households and trades), so
/// the first winter's rush is a third of the island, not all of it.
pub fn found_wearing_coat(world: &mut World, actor: Entity, ordinal: usize) {
    let per_three = super::frontier::economy_spec::tuning(world)
        .map_or(0, |tuning| tuning.founder_coats_per_three) as usize;
    if ordinal % 3 >= per_three || world.get::<Equipment>(actor).is_none() {
        return;
    }
    let (_, coat) = spawn_sim(
        world,
        (
            Item::new(super::roles::roles(world).garment),
            Durability::new(
                catalog::durability_in(world, super::roles::roles(world).garment)
                    .unwrap_or(catalog::TOOL_DURABILITY),
            ),
            Perceivable::new(crate::world::kinds::ITEM),
        ),
    );
    items::put_in_inventory(world, actor, coat);
    items::put_in_slot(world, actor, coat, "body");
}

pub fn has_coat(world: &World, actor: Entity) -> bool {
    items::equipped_matching(world, actor, super::roles::roles(world).garment)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|item| world.get::<Durability>(item))
        .is_some_and(|durability| durability.current > 0)
}

/// Cold ticks in the next two simulation days. Future activity is unknown, so
/// this conservatively assumes exposure outside already-scheduled sheltered
/// rest. Day/night and season boundaries use the actual simulation calendar.
pub fn clothing_lead_exposure(world: &World, actor: Entity) -> u64 {
    let config = world.resource::<SimConfig>();
    let now = world.resource::<SimClock>().tick;
    let end = now.saturating_add(config.day_length.saturating_mul(2));
    let calendar = (super::first_day(world), super::days_per_season(world));
    let mut exposure = cold_ticks(now, end, config, calendar);
    if let Some(person) = world.get::<Villager>(actor)
        && (person.rest_rate >= super::HOME_REST_RATE || super::indoors::asleep(world, actor))
        && person.resting_until > now
    {
        exposure = exposure.saturating_sub(cold_ticks(
            now,
            end.min(person.resting_until),
            config,
            calendar,
        ));
    }
    exposure
}

fn cold_ticks(start: u64, end: u64, config: &SimConfig, calendar: (u64, u64)) -> u64 {
    let day = config.day_length.max(1);
    let night_start = (u128::from(day) * u128::from(1000 - config.night_permille.min(1000)))
        .div_ceil(1000) as u64;
    let mut at = start;
    let mut total = 0_u64;
    while at < end {
        let offset = at % day;
        let span = (day - offset).min(end - at);
        if super::calendar_with(at, config, calendar.0, calendar.1).1 == Season::Winter {
            total = total.saturating_add(span);
        } else {
            total = total.saturating_add((offset + span).saturating_sub(offset.max(night_start)));
        }
        at += span;
    }
    total
}

/// Desired *carried* clothing, excluding the currently equipped coat. A single
/// real spare covers an approaching replacement; ledger reservation credits it
/// once instead of financing repeated coats. Existing worlds keep their policy.
pub fn clothing_reserve_target(world: &World, actor: Entity) -> u32 {
    if super::policy::active(world).is_none() {
        let (day, season, _) = super::calendar(world);
        let winter_soon = season == Season::Winter || ((day + 2) / super::DAYS_PER_SEASON) % 4 == 3;
        return u32::from(
            (winter_soon || economy::need(world, actor, "Warmth") < 80_000)
                && !has_coat(world, actor),
        );
    }
    let exposure = clothing_lead_exposure(world, actor);
    let remaining = items::equipped_matching(world, actor, super::roles::roles(world).garment)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|item| world.get::<Durability>(item))
        .map_or(0, |durability| durability.current);
    u32::from(exposure > 0 && u64::from(remaining) <= exposure)
}

/// Below this Warmth a regional resident loses health every tick. Built-in
/// default of `economy.tuning.cold_health_threshold`.
pub const COLD_HEALTH_THRESHOLD: i32 = 10_000;
/// Health lost per tick below [`COLD_HEALTH_THRESHOLD`]: full health lasts about
/// two thirds of a day of continuous deep cold.
pub const COLD_HEALTH_DRAIN: i32 = 6;

/// Construction-time endowment of one durable work tool. Later tools are crafted.
pub fn grant_tool(world: &mut World, actor: Entity, kind: &str) {
    let (_, item) = spawn_sim(
        world,
        (
            Item::new(kind),
            Durability::new(
                catalog::durability_in(world, kind).unwrap_or(catalog::TOOL_DURABILITY),
            ),
            Perceivable::new(crate::world::kinds::ITEM),
        ),
    );
    items::put_in_inventory(world, actor, item);
}

/// Whether the actor carries a usable (not worn-out) tool of this kind.
pub fn usable_tool(world: &World, actor: Entity, kind: &str) -> Option<Entity> {
    super::economy::free_items(world, actor)
        .into_iter()
        .filter(|(_, item)| item.kind == kind)
        .filter_map(|(id, _)| world.resource::<SimIndex>().get(id))
        .find(|item| {
            world
                .get::<Durability>(*item)
                .is_none_or(|durability| durability.current > 0)
        })
}

/// Ticks one gather takes for this actor. Outside the regional profile this is
/// the unchanged base duration of the patch. In it, the matching tool speeds the work,
/// bare hands slow it, and morale scales the result within a bounded band.
pub fn gather_ticks(world: &World, actor: Entity, kind: &str, base: u32) -> u32 {
    let (with_tool, bare) = super::frontier::economy_spec::tuning(world).map_or(
        (catalog::TOOL_GATHER_PERCENT, catalog::BARE_GATHER_PERCENT),
        |tuning| (tuning.tool_gather_percent, tuning.bare_gather_percent),
    );
    let tool = match catalog::gather_tool_in(world, kind) {
        Some(tool) if usable_tool(world, actor, tool).is_some() => with_tool,
        Some(_) => bare,
        None => 100,
    };
    let pace = super::colony::work_pace_percent(world, actor);
    (u64::from(base) * u64::from(tool) * u64::from(pace) / 10_000).clamp(1, u64::from(u32::MAX))
        as u32
}

/// One completed gather wears the matching tool; a spent tool breaks for good.
pub(super) fn wear_gather_tool(world: &mut World, actor: Entity, id: SimId, kind: &str) {
    let Some(tool) = catalog::gather_tool_in(world, kind) else {
        return;
    };
    let Some(item) = usable_tool(world, actor, tool) else {
        return;
    };
    let Some(mut durability) = world.get_mut::<Durability>(item) else {
        return;
    };
    durability.current = durability.current.saturating_sub(1);
    if durability.current == 0 {
        items::take_from_holder(world, item);
        world.despawn(item);
        record_effect(world, Some(id), "VillageToolWornOut", &json!({"kind":tool}));
    }
}

fn change_need(world: &mut World, actor: Entity, name: &str, delta: i32) {
    let Some(id) = world.resource::<NeedRegistry>().id(name) else {
        return;
    };
    if let Some(mut needs) = world.get_mut::<Needs>(actor) {
        let value = needs.get_mut(id);
        value.value = value.value.saturating_add(delta).clamp(0, 100_000);
    }
}

pub fn climate_tick(world: &mut World) {
    let cold = cold(world);
    // 100 outside the regional profile and in ordinary weather.
    let loss_percent = super::weather::warmth_loss_percent(world).max(1) as i32;
    let tick = world.resource::<SimClock>().tick;
    let actors: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, e)| {
            world.get::<Villager>(**e).is_some()
                && world
                    .get::<Vitality>(**e)
                    .is_some_and(|v| v.died_at.is_none())
        })
        .map(|(id, e)| (*id, *e))
        .collect();
    for (id, actor) in actors {
        if tick > 0
            && world
                .get::<Villager>(actor)
                .is_some_and(|v| v.resting_until == tick && v.rest_rate > 0)
        {
            record_effect(
                world,
                Some(id),
                "VillageRestCompleted",
                &json!({"completed":true}),
            );
        }
        // Resting at home, or simply inside it: the hearth, not the weather.
        let sheltered = world
            .get::<Villager>(actor)
            .is_some_and(|v| v.resting_until > tick && v.rest_rate >= super::HOME_REST_RATE)
            || super::indoors::inside_entity(world, actor).is_some();
        let coat = items::equipped_matching(world, actor, super::roles::roles(world).garment)
            .and_then(|item| world.resource::<SimIndex>().get(item))
            .filter(|item| {
                world
                    .get::<Durability>(*item)
                    .is_some_and(|d| d.current > 0)
            });
        let heated = sheltered && super::colony::home_heated(world, actor);
        let warmth_before = economy::need(world, actor, "Warmth");
        change_need(
            world,
            actor,
            "Warmth",
            if sheltered {
                if heated { 100 } else { 20 }
            } else if !cold {
                10
            } else if coat.is_some() {
                20 * 100 / loss_percent
            } else {
                -4 * loss_percent / 100
            },
        );
        if cold && !sheltered {
            if let Some(coat) = coat {
                let broken = {
                    let mut d = world.get_mut::<Durability>(coat).expect("coat");
                    d.current = d.current.saturating_sub(1);
                    d.current == 0
                };
                if broken {
                    let garment = super::roles::roles(world).garment;
                    items::take_from_holder(world, coat);
                    world.despawn(coat);
                    record_effect(
                        world,
                        Some(id),
                        "VillageClothingWornOut",
                        &json!({"kind":garment}),
                    );
                }
            }
            if economy::need(world, actor, "Warmth") < 20_000 {
                change_need(world, actor, "Energy", -2);
            }
        }
        if let Some((threshold, drain)) = super::frontier::economy_spec::tuning(world)
            .map(|tuning| (tuning.cold_health_threshold, tuning.cold_health_drain))
        {
            let warmth = economy::need(world, actor, "Warmth");
            if warmth < threshold {
                if let Some(mut vitality) = world.get_mut::<Vitality>(actor) {
                    vitality.health = vitality.health.saturating_sub(drain);
                }
                if warmth_before >= threshold {
                    record_effect(
                        world,
                        Some(id),
                        "VillageColdExposure",
                        &json!({"warmth":warmth,"threshold":threshold,
                            "health_per_tick":-drain}),
                    );
                }
            }
        }
    }
}
