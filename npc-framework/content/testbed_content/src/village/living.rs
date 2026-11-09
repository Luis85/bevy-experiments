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
    if !super::needs_driven(world) {
        return;
    }
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
        let mut ids = world.resource_mut::<crate::TestbedIds>();
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
    super::needs_driven(world) && {
        let (_, season, night) = super::calendar(world);
        night || season == Season::Winter
    }
}

pub fn has_coat(world: &World, actor: Entity) -> bool {
    items::equipped_matching(world, actor, catalog::WARM_CLOTHES)
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
    let mut exposure = cold_ticks(now, end, config);
    if let Some(person) = world.get::<Villager>(actor)
        && person.rest_rate >= super::HOME_REST_RATE
        && person.resting_until > now
    {
        exposure = exposure.saturating_sub(cold_ticks(now, end.min(person.resting_until), config));
    }
    exposure
}

fn cold_ticks(start: u64, end: u64, config: &SimConfig) -> u64 {
    let day = config.day_length.max(1);
    let night_start = (u128::from(day) * u128::from(1000 - config.night_permille.min(1000)))
        .div_ceil(1000) as u64;
    let mut at = start;
    let mut total = 0_u64;
    while at < end {
        let offset = at % day;
        let span = (day - offset).min(end - at);
        if super::calendar_at(at, config, true).1 == Season::Winter {
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
    let remaining = items::equipped_matching(world, actor, catalog::WARM_CLOTHES)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|item| world.get::<Durability>(item))
        .map_or(0, |durability| durability.current);
    u32::from(exposure > 0 && u64::from(remaining) <= exposure)
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
    if !super::needs_driven(world) {
        return;
    }
    let cold = cold(world);
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
        let sheltered = world
            .get::<Villager>(actor)
            .is_some_and(|v| v.resting_until > tick && v.rest_rate >= super::HOME_REST_RATE);
        let coat = items::equipped_matching(world, actor, catalog::WARM_CLOTHES)
            .and_then(|item| world.resource::<SimIndex>().get(item))
            .filter(|item| {
                world
                    .get::<Durability>(*item)
                    .is_some_and(|d| d.current > 0)
            });
        let heated = sheltered && super::colony::home_heated(world, actor);
        change_need(
            world,
            actor,
            "Warmth",
            if sheltered {
                if heated { 100 } else { 20 }
            } else if !cold {
                10
            } else if coat.is_some() {
                20
            } else {
                -4
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
                    items::take_from_holder(world, coat);
                    world.despawn(coat);
                    record_effect(
                        world,
                        Some(id),
                        "VillageClothingWornOut",
                        &json!({"kind":catalog::WARM_CLOTHES}),
                    );
                }
            }
            if economy::need(world, actor, "Warmth") < 20_000 {
                change_need(world, actor, "Energy", -2);
            }
        }
    }
}
