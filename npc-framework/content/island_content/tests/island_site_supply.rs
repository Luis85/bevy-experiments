//! Building materials across settlements (S4 housing fix): a site's shortfall
//! that its own settlement can neither take from its stores nor make is
//! assigned to the nearest settlement that makes or stores the good, whose
//! plan and haulers supply it; the boat leaves it alone; and the last worker
//! of a trade a site still waits for stays.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, Villager, catalog, civic, economy as econ,
    frontier::labor::{self, Employment},
    merchant, production,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// No boat, births or arrivals, and a small storage building that costs four
/// logs: what is tested is who supplies the site.
fn quiet(world: &mut Value) {
    economy(world)["merchant"]["enabled"] = json!(false);
    economy(world)["demography"]["enabled"] = json!(false);
    economy(world)["tuning"]["planner_crop_shortage_units"] = json!(100_000);
    woodpile(world);
}

fn woodpile(world: &mut Value) {
    economy(world)["buildings"]
        .as_array_mut()
        .expect("buildings")
        .push(json!({"id": "woodpile", "display": "Woodpile",
            "cost": [{"good": "log", "count": 4}], "work_ticks": 20,
            "footprint": {"width": 2, "depth": 2}, "kind": {"type": "storage", "capacity": 40},
            "solid": false, "civic": false}));
}

fn place(world: &mut World, building: &str, settlement: SimId) -> SimId {
    let def = civic::definition(world, building)
        .expect("definition")
        .clone();
    let (x, z, rotation) = civic::find_site(world, &def, settlement, 36).expect("a buildable site");
    civic::PlaceBuilding {
        building: building.into(),
        x,
        z,
        rotation,
    }
    .perform(world)
    .expect("placed")
}

/// Everybody of `settlement` (by production plan) whose trade gathers logs
/// takes another trade: the settlement can no longer make logs.
fn no_woodcutters(world: &mut World, settlement: SimId, instead: Role) {
    let cutters: Vec<Entity> = all_residents(world)
        .into_iter()
        .map(|(_, actor)| actor)
        .filter(|actor| {
            production::economic_settlement(world, *actor) == Some(settlement)
                && world
                    .get::<Villager>(*actor)
                    .is_some_and(|person| catalog::outputs_for(world, person.role).contains(&"log"))
        })
        .collect();
    for actor in cutters {
        world.get_mut::<Villager>(actor).expect("resident").role = instead;
    }
}

#[test]
fn a_site_in_a_village_without_a_woodcutter_is_supplied_by_a_neighbour() {
    let mut app = world_with(quiet);
    let world = app.world_mut();
    let ridge = settlement(world, "stone_ridge");
    no_woodcutters(world, ridge, Role::Fisher);
    fund(world, 400);
    let site = place(world, "woodpile", ridge);
    // Stone Ridge has no store with logs and nobody who cuts them: the four
    // logs are assigned to the nearest settlement that does.
    let shares = civic::unmet_elsewhere(world);
    assert_eq!(shares.len(), 1, "{shares:?}");
    let share = &shares[0];
    assert_eq!(
        (
            share.site,
            share.settlement,
            share.kind.as_str(),
            share.units
        ),
        (site, ridge, "log", 4)
    );
    let supplier = share.supplier;
    assert_ne!(supplier, ridge);
    // The supplier's plan takes it as civic demand; its own sites hold it.
    assert_eq!(civic::assigned_orders(world, supplier).get("log"), Some(&4));
    assert_eq!(civic::site_hold(world, supplier, "log"), 4);
    assert_eq!(civic::site_hold(world, ridge, "log"), 4);
    production::refresh_all(world);
    let planned: u32 = residents(world, supplier)
        .into_iter()
        .map(|(_, actor)| production::production_demand(world, actor, "log"))
        .sum();
    assert!(
        planned > 0,
        "no woodcutter of the supplier was given the logs"
    );
    // A resident of the supplier with spare logs carries them over, paid by
    // the civic fund, through the ordinary delivery.
    let carrier = residents(world, supplier)[0].1;
    econ::grant(world, carrier, Item::new("log").stack(6));
    let trip = civic::material_trip(world, carrier).expect("a trip to the foreign site");
    let civic::MaterialTrip::Deliver {
        site: to,
        kind,
        count,
    } = trip
    else {
        panic!("expected a delivery from the pack, got {trip:?}");
    };
    assert_eq!((to, kind.as_str()), (site, "log"));
    assert!(count > 0 && count <= 4);
    let coins = money(world);
    let logs = units(world, "log");
    let door = *world
        .get::<SimPos>(entity(world, site))
        .expect("site position");
    teleport(world, carrier, door);
    civic::deliver(world, carrier, entity(world, site), "log", count).expect("delivered");
    let delivered = civic::sites(world)
        .into_iter()
        .find(|view| view.id == site)
        .and_then(|view| view.delivered.get("log").copied());
    assert_eq!(delivered, Some(count));
    assert_eq!(money(world), coins, "coins are conserved");
    assert_eq!(units(world, "log"), logs, "logs are conserved");
    // What is left stays assigned to the supplier.
    assert_eq!(civic::site_hold(world, supplier, "log"), 4 - count);
}

#[test]
fn a_worker_living_elsewhere_supplies_the_sites_of_its_workplace() {
    // Seed 11 before the fix: Stone Ridge's carpenter lived in Alderwood,
    // its plan (Stone Ridge's) gave it the planks of a Stone Ridge house, and
    // it carried them to the boat because only Alderwood's sites were its own.
    let mut app = world_with(quiet);
    let world = app.world_mut();
    let ridge = settlement(world, "stone_ridge");
    let harbour = settlement(world, "harbour");
    let cutter = residents(world, ridge)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| {
            world
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == Role::Woodcutter)
        })
        .expect("a Stone Ridge woodcutter");
    world
        .get_mut::<village::frontier::SettlementMember>(cutter)
        .expect("member")
        .settlement = harbour;
    assert_eq!(production::economic_settlement(world, cutter), Some(ridge));
    fund(world, 400);
    let site = place(world, "woodpile", ridge);
    econ::grant(world, cutter, Item::new("log").stack(6));
    let trip = civic::material_trip(world, cutter).expect("a trip to the workplace's site");
    assert!(
        matches!(&trip, civic::MaterialTrip::Deliver { site: to, kind, .. }
            if *to == site && kind == "log"),
        "{trip:?}"
    );
}

#[test]
fn a_village_that_makes_the_good_supplies_its_own_site() {
    let mut app = world_with(quiet);
    let world = app.world_mut();
    let ridge = settlement(world, "stone_ridge");
    let has_cutter = all_residents(world).into_iter().any(|(_, actor)| {
        production::economic_settlement(world, actor) == Some(ridge)
            && world
                .get::<Villager>(actor)
                .is_some_and(|person| catalog::outputs_for(world, person.role).contains(&"log"))
    });
    assert!(has_cutter, "the shipped Stone Ridge has a woodcutter");
    fund(world, 400);
    place(world, "woodpile", ridge);
    assert!(civic::unmet_elsewhere(world).is_empty());
    assert_eq!(civic::site_hold(world, ridge, "log"), 4);
    for (id, _) in village::frontier::settlements::all(world) {
        if id != ridge {
            assert_eq!(civic::site_hold(world, id, "log"), 0);
        }
    }
}

#[test]
fn the_boat_does_not_take_logs_a_site_still_lacks() {
    let mut app = world_with(|world| {
        short_days(world, 2400);
        only_weather(world, "clear");
        let boat = &mut economy(world)["merchant"];
        boat["first_visit_day"] = json!(0);
        boat["arrive_permille"] = json!(20);
        boat["stay_ticks"] = json!(600);
        boat["interval_days"] = json!(1);
        economy(world)["demography"]["arrivals_per_visit"] = json!(0);
        economy(world)["demography"]["enabled"] = json!(false);
        economy(world)["tuning"]["stock_target_days"] = json!(0);
        economy(world)["tuning"]["founding_day"] = json!(8);
        woodpile(world);
    });
    run_ticks(app.world_mut(), 2);
    let world = app.world_mut();
    let harbour = settlement(world, "harbour");
    let cutter = residents(world, harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| {
            world
                .get::<Villager>(*actor)
                .is_some_and(|person| catalog::outputs_for(world, person.role).contains(&"log"))
        })
        .expect("a harbour woodcutter");
    econ::grant(world, cutter, Item::new("log").stack(16));
    let wanted = merchant::wanted_surplus(world, "log");
    assert!(wanted > 4, "the boat wants the logs: {wanted}");
    let (_, _, before) = civic::boat_trip(world, cutter).expect("a boat trip");
    fund(world, 400);
    place(world, "woodpile", harbour);
    // The four logs the site lacks are not surplus: neither the island's
    // offer to the boat nor the woodcutter's trip to the post counts them.
    assert_eq!(civic::island_shortfall(world, "log"), 4);
    assert_eq!(merchant::wanted_surplus(world, "log"), wanted - 4);
    let after = civic::boat_trip(world, cutter).map_or(0, |(_, _, units)| units);
    assert!(
        after + 4 <= before,
        "the boat trip took {after} logs, {before} before the site"
    );
}

/// The only worker of `role` in a settlement, after the others of it took
/// another trade.
fn last_of(world: &mut World, settlement: SimId, role: Role) -> Entity {
    let workers: Vec<Entity> = residents(world, settlement)
        .into_iter()
        .map(|(_, actor)| actor)
        .filter(|actor| {
            world
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == role)
        })
        .collect();
    let (last, rest) = workers.split_first().expect("a worker of the trade");
    for actor in rest {
        world.get_mut::<Villager>(*actor).expect("resident").role = Role::Fisher;
    }
    *last
}

/// The worker has been in the trade long enough to count as idle when
/// nothing of it is wanted, and is due a review.
fn ready_for_review(world: &mut World, actor: Entity) {
    if let Some(mut employment) = world.get_mut::<Employment>(actor) {
        employment.changed_at = 0;
        employment.cooldown_until = 0;
        employment.next_review = 0;
    }
}

fn reviewed(app: &mut bevy_app::App, actor: Entity) -> bool {
    let day = app.world().resource::<SimConfig>().day_length;
    let tenure = village::frontier::economy_spec::tuning(app.world())
        .map_or(0, |tuning| u64::from(tuning.idle_trade_days));
    let start = (tenure + 1) * day;
    let start = start - start % 300;
    // Two labour reviews apart, past the blocked time.
    app.world_mut().resource_mut::<SimClock>().tick = start;
    labor::tick(app.world_mut());
    app.world_mut().resource_mut::<SimClock>().tick = start + 1500;
    labor::tick(app.world_mut());
    labor::consider_assignment(app.world_mut(), actor)
}

fn reason(app: &bevy_app::App, actor: Entity) -> Option<String> {
    app.world()
        .get::<Employment>(actor)
        .map(|employment| employment.last_reason.clone())
}

#[test]
fn the_last_carpenter_stays_while_a_home_site_lacks_planks() {
    // A small store that costs four planks.
    let scene = |site: bool| {
        let mut app = world_with(|world| {
            quiet(world);
            // The shipped world keeps idle workers in their trade
            // (`idle_trade_days` 0); one day here, so a carpenter with nothing
            // to make counts as idle.
            economy(world)["tuning"]["idle_trade_days"] = json!(1);
            economy(world)["buildings"]
                .as_array_mut()
                .expect("buildings")
                .push(json!({"id": "plankpile", "display": "Plank pile",
                    "cost": [{"good": "plank", "count": 4}], "work_ticks": 20,
                    "footprint": {"width": 2, "depth": 2},
                    "kind": {"type": "storage", "capacity": 40},
                    "solid": false, "civic": false}));
        });
        let world = app.world_mut();
        let ridge = settlement(world, "stone_ridge");
        let carpenter = last_of(world, ridge, Role::Carpenter);
        ready_for_review(world, carpenter);
        if site {
            fund(world, 400);
            place(world, "plankpile", ridge);
        }
        (app, carpenter, ridge)
    };
    // Without a site, a carpenter with nothing to make is idle and takes
    // other work.
    let (mut control, idle, _) = scene(false);
    assert!(
        reviewed(&mut control, idle),
        "the control carpenter stayed: {:?}",
        reason(&control, idle)
    );
    let (mut app, carpenter, ridge) = scene(true);
    assert_eq!(civic::site_hold(app.world(), ridge, "plank"), 4);
    assert!(
        !reviewed(&mut app, carpenter),
        "the last carpenter left a village whose site lacks planks: {:?}",
        reason(&app, carpenter)
    );
    assert_eq!(
        app.world()
            .get::<Villager>(carpenter)
            .map(|person| person.role),
        Some(Role::Carpenter)
    );
    // A site that waits for planks is work: the carpenter is not idle.
    assert!(
        app.world()
            .get::<Employment>(carpenter)
            .is_some_and(|employment| employment.blocked_since.is_none())
    );
}
