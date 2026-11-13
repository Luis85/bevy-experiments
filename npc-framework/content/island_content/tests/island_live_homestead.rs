//! A scripted played life on the homestead that uses every live verb of the
//! homestead pipeline: upgrades as private sites paid from escrow
//! (deliveries from the pack, orders carried to the door by a hauling
//! resident, a hired builder who works at the door, a cancellation in the
//! middle of a haul), the storage chest, the garden plot allowance, repeat
//! orders of crafting and garden work, and a friend who walks over to help.
//! A village site and a homestead site open at once both finish. The village
//! around it stays healthy and every coin and unit is accounted for.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, VillageAct, Villager, Wallet, catalog, colony, economy as econ,
    frontier::settlements,
    homestead::{self, HomesteadAct, HomesteadPlot},
    live::{self, LiveAct, LiveState},
    view,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

const DAY: u64 = 6_000;

fn quiet(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "clear");
    economy(world)["merchant"]["enabled"] = json!(false);
    economy(world)["demography"]["birth_chance_permille"] = json!(0);
    // From the first day of spring: the played life runs before winter, so
    // coatless friends are not kept indoors by the cold (`island_cold.rs`).
    economy(world)["tuning"]["founding_day"] = json!(0);
}

fn act<C: ContentCommand>(app: &mut App, command: &C) {
    submit(app.world_mut(), None, SimCommand::content(command)).expect("accepted");
    run_ticks(app.world_mut(), 1);
}

fn refused(app: &App, command: &LiveAct) -> String {
    command.check(app.world()).expect_err("refused")
}

/// Gives the settlement's woodcutters logs to sell, so an order has a seller
/// whatever the season left in their packs (the coins are not touched).
fn stock_woodcutters(app: &mut App, settlement: SimId) {
    for (_, actor) in residents(app.world(), settlement) {
        if app
            .world()
            .get::<Villager>(actor)
            .is_some_and(|person| person.role == Role::Woodcutter)
        {
            econ::grant(app.world_mut(), actor, Item::new("log").stack(6));
        }
    }
}

/// Keeps the played character fed wherever it is.
fn feed(app: &mut App, actor: Entity) {
    for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
        econ::restore(app.world_mut(), actor, need, 100_000);
    }
}

/// The door is within the 4 m a delivery or a stage of work needs.
fn at_door(app: &App, who: SimId, door: SimPos) -> bool {
    app.world()
        .get::<SimPos>(entity(app.world(), who))
        .is_some_and(|at| at.dist2(door) <= 4_000_i64.pow(2))
}

/// Keeps the played character fed and at `place` while time passes.
fn stay(app: &mut App, actor: Entity, place: SimPos, ticks: u64) {
    let end = app.world().resource::<SimClock>().tick + ticks;
    while app.world().resource::<SimClock>().tick < end {
        for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
            econ::restore(app.world_mut(), actor, need, 100_000);
        }
        if app.world().get::<MoveIntent>(actor).is_none() {
            teleport(app.world_mut(), actor, place);
        }
        run_ticks(app.world_mut(), 20);
    }
}

fn upgrade(app: &mut App, id: SimId, tier: &str) {
    let mut command = LiveAct::new(id, "upgrade");
    command.kind = tier.into();
    act(app, &command);
    assert!(
        view::upgrade_site(app.world(), id).is_some_and(|site| site.tier == tier),
        "{tier} did not start"
    );
}

fn plots_of(app: &App, kind: &str) -> Vec<(SimId, Entity)> {
    homestead::plots(app.world())
        .into_iter()
        .filter(|(_, plot)| {
            homestead::definition(app.world(), *plot).is_some_and(|def| def.use_kind == kind)
        })
        .collect()
}

fn lowest_resident_need(world: &World) -> i32 {
    all_residents(world)
        .into_iter()
        .filter(|(_, actor)| village::living(world, *actor))
        .flat_map(|(_, actor)| {
            ["Hunger", "Thirst", "Energy", "Warmth"].map(|need| econ::need(world, actor, need))
        })
        .min()
        .unwrap_or(100_000)
}

#[test]
fn a_homestead_life_uses_every_live_verb_and_the_village_stays_healthy() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let id = app.world().resource::<village::VillageState>().played;
    let actor = entity(app.world(), id);
    colony::JoinIsland { unit: id.0 }
        .perform(app.world_mut())
        .expect("arrived");
    let founders = all_residents(app.world()).len();
    let mut lowest = i32::MAX;

    // A cottage on a housing plot: the catalogue cottage body.
    let (plot_id, plot) = plots_of(&app, "housing")[0];
    let centre = *app.world().get::<SimPos>(plot).expect("plot");
    teleport(app.world_mut(), actor, centre);
    HomesteadAct::new(id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .expect("claimed");
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(8));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    HomesteadAct::new(id, "build_cottage", plot_id)
        .perform(app.world_mut())
        .expect("building");
    assert!(until(&mut app, 4_000, 10, |app| {
        app.world()
            .get::<HomesteadPlot>(plot)
            .is_some_and(|estate| estate.cottage_built)
    }));
    let house = colony::home_of(app.world(), id).expect("home");
    let door = homestead::front_door(centre);
    // A declared outside purse, as in the shock tests: the played character
    // arrives with savings for orders and a builder.
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += 300;
    let total = money(app.world());
    let settlement = live::homestead_settlement(app.world(), id).expect("home settlement");

    // Storage: started at the door, its logs ordered from the residents for
    // a fee held in escrow, carried to the door by a hauling resident and
    // built in stages by a hired builder who works there.
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    let coins_before = app.world().get::<Wallet>(actor).expect("wallet").coins;
    upgrade(&mut app, id, "storage");
    let site = view::upgrade_site(app.world(), id).expect("site");
    assert_eq!(site.delivered, vec![("plank".to_owned(), 4)]);
    let mut order = LiveAct::new(id, "order");
    order.kind = "log".into();
    order.quantity = 3;
    assert!(refused(&app, &order).contains("not wanted or already ordered"));
    order.quantity = 2;
    stock_woodcutters(&mut app, settlement);
    act(&mut app, &order);
    let held = view::upgrade_site(app.world(), id).expect("site").escrow;
    assert!(held > 0);
    assert_eq!(
        app.world().get::<Wallet>(actor).expect("wallet").coins,
        coins_before - held
    );
    assert_eq!(money(app.world()), total, "escrow is counted");
    // The owner walks away: nothing reaches the site but what a resident
    // carries to the door.
    let away = SimPos::new(door.x + 30_000, door.z);
    teleport(app.world_mut(), actor, away);
    let mut logs = 0;
    let mut seen_hauler = false;
    let brought = until(&mut app, 2 * DAY, 5, |app| {
        feed(app, actor);
        let site = view::upgrade_site(app.world(), id).expect("site");
        seen_hauler |= site
            .workers
            .iter()
            .any(|worker| worker.job == "hauler" && worker.text.contains("log"));
        let now = site
            .delivered
            .iter()
            .find(|(kind, _)| kind == "log")
            .map_or(0, |(_, count)| *count);
        if now > logs {
            let delivery = effects(app.world(), "VillageHomesteadDelivered")
                .into_iter()
                .rev()
                .find(|effect| effect["for"] == json!(id))
                .expect("a delivery effect");
            let hauler = SimId(delivery["by"].as_u64().expect("hauler") as u32);
            assert_ne!(hauler, id);
            assert!(
                at_door(app, hauler, door),
                "{} handed over logs away from the door",
                hauler.0
            );
            assert!(delivery["coins"].as_u64().unwrap_or(0) > 0, "{delivery}");
            logs = now;
        }
        site.supplied
    });
    assert!(brought, "{:?}", view::upgrade_site(app.world(), id));
    assert!(seen_hauler, "the site reported who brought the logs");
    let site = view::upgrade_site(app.world(), id).expect("site");
    assert!(site.ordered.is_empty());
    assert_eq!(
        money(app.world()),
        total,
        "coins after the order was filled"
    );
    act(&mut app, &LiveAct::new(id, "hire"));
    let builder = view::upgrade_site(app.world(), id)
        .and_then(|site| site.builder)
        .expect("a builder");
    assert_ne!(builder, id);
    let builder_actor = entity(app.world(), builder);
    // The owner stays away: the builder walks to the door and builds there.
    let mut remaining = site.remaining_ticks;
    let mut worked_at_door = 0;
    let finished = until(&mut app, 3 * DAY, 5, |app| {
        feed(app, actor);
        if let Some(work) = app.world().get::<live::UpgradeWork>(builder_actor) {
            assert_eq!(work.owner, id);
            assert!(
                at_door(app, builder, door),
                "the builder works away from the door"
            );
            assert!(view::activity_progress(app.world(), builder_actor).is_some());
            worked_at_door += 1;
        }
        match view::upgrade_site(app.world(), id) {
            Some(site) => {
                if site.remaining_ticks < remaining {
                    assert!(
                        worked_at_door > 0,
                        "work advanced before the builder stood at the door"
                    );
                    remaining = site.remaining_ticks;
                }
                false
            }
            None => true,
        }
    });
    assert!(finished, "{:?}", view::upgrade_site(app.world(), id));
    assert!(worked_at_door > 0);
    assert!(!effects(app.world(), "VillageHomesteadBuilderWorked").is_empty());
    let chest = view::chest(app.world(), id).expect("chest");
    assert_eq!((chest.capacity, chest.used), (40, 0));
    assert_eq!(money(app.world()), total, "coins after the builder's wages");

    // The chest: store and take, with refusals.
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("bread").stack(3));
    let mut store = LiveAct::new(id, "store");
    store.kind = "bread".into();
    store.quantity = 3;
    act(&mut app, &store);
    let mut take = LiveAct::new(id, "take");
    take.kind = "bread".into();
    take.quantity = 4;
    assert!(refused(&app, &take).contains("does not hold"));
    take.quantity = 1;
    act(&mut app, &take);
    assert_eq!(
        view::chest(app.world(), id).expect("chest").goods,
        vec![("bread".to_owned(), 2)]
    );

    // The garden allowance: one garden plot until the second plot is built.
    let gardens = plots_of(&app, "garden");
    let (first_garden, first_plot) = gardens[0];
    let (second_garden, second_plot) = gardens[1];
    let garden_centre = *app.world().get::<SimPos>(first_plot).expect("garden");
    teleport(app.world_mut(), actor, garden_centre);
    HomesteadAct::new(id, "claim_plot", first_garden)
        .perform(app.world_mut())
        .expect("first garden");
    let second_centre = *app.world().get::<SimPos>(second_plot).expect("garden");
    teleport(app.world_mut(), actor, second_centre);
    let reason = HomesteadAct::new(id, "claim_plot", second_garden)
        .check(app.world())
        .expect_err("beyond the allowance");
    assert!(reason.contains("1 garden plot"), "{reason}");

    // The second plot: started, its log ordered, cancelled while a hauler
    // carries the log (goods and escrow come back, the hauler keeps the log
    // it brought, nothing is left over), then started again and built by the
    // owner's own work.
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    upgrade(&mut app, id, "second_plot");
    let mut order = LiveAct::new(id, "order");
    order.kind = "log".into();
    order.quantity = 1;
    stock_woodcutters(&mut app, settlement);
    act(&mut app, &order);
    teleport(app.world_mut(), actor, away);
    let carrying = until(&mut app, 2 * DAY, 5, |app| {
        feed(app, actor);
        view::upgrade_site(app.world(), id).is_some_and(|site| {
            site.workers.iter().any(|worker| worker.state == "bringing")
                && site.delivered.iter().all(|(kind, _)| kind != "log")
        })
    });
    assert!(carrying, "{:?}", view::upgrade_site(app.world(), id));
    let hauler = view::upgrade_site(app.world(), id)
        .expect("site")
        .workers
        .into_iter()
        .find(|worker| worker.state == "bringing")
        .expect("a hauler")
        .id;
    let hauler_actor = entity(app.world(), hauler);
    let hauler_logs = econ::count(app.world(), hauler_actor, "log");
    assert!(hauler_logs > 0, "the hauler carries the log");
    teleport(app.world_mut(), actor, door);
    let wallet = app.world().get::<Wallet>(actor).expect("wallet").coins;
    let escrow = view::upgrade_site(app.world(), id).expect("site").escrow;
    // Measured at the cancellation itself: elsewhere logs are cut and burned.
    let logs_before = units(app.world_mut(), "log");
    act(&mut app, &LiveAct::new(id, "cancel_upgrade"));
    assert!(view::upgrade_site(app.world(), id).is_none());
    assert_eq!(
        econ::count(app.world(), hauler_actor, "log"),
        hauler_logs,
        "the hauler keeps the log it carried"
    );
    assert!(app.world().get::<live::UpgradeWork>(hauler_actor).is_none());
    assert_eq!(econ::count(app.world(), actor, "plank"), 4);
    assert_eq!(
        app.world().get::<Wallet>(actor).expect("wallet").coins,
        wallet + escrow
    );
    assert_eq!(money(app.world()), total, "coins after the cancellation");
    assert_eq!(
        units(app.world_mut(), "log"),
        logs_before,
        "no log was made or lost by the cancellation"
    );
    let cancelled = effects(app.world(), "VillageHomesteadUpgradeCancelled");
    assert!(
        cancelled.last().expect("cancelled")["kept_by_haulers"]
            .as_array()
            .is_some_and(|kept| !kept.is_empty()),
        "{:?}",
        cancelled.last()
    );
    let cancelled_at = app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), 600);
    let late = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| entry.tick >= cancelled_at)
        .filter(|entry| match &entry.kind {
            JournalKind::Effect { effect, .. } => {
                effect.get("type").and_then(Value::as_str) == Some("VillageHomesteadDelivered")
                    && effect.get("for").and_then(Value::as_u64) == Some(u64::from(id.0))
            }
            _ => false,
        })
        .count();
    assert_eq!(late, 0, "nothing is delivered to a cancelled upgrade");
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    upgrade(&mut app, id, "second_plot");
    stay(&mut app, actor, door, 700);
    assert!(view::homestead_tiers(app.world(), id)[1].done);
    teleport(app.world_mut(), actor, second_centre);
    HomesteadAct::new(id, "claim_plot", second_garden)
        .perform(app.world_mut())
        .expect("the second garden is allowed now");

    // Repeat orders of garden work on the first garden: plant, water, harvest.
    econ::grant(app.world_mut(), actor, Item::new(catalog::GRAIN).stack(2));
    econ::grant(app.world_mut(), actor, Item::new(catalog::WATER).stack(2));
    for task in ["plant", "water_crop"] {
        let mut repeat = LiveAct::new(id, "repeat");
        repeat.target = Some(first_garden);
        repeat.task = task.into();
        repeat.quantity = 2;
        act(&mut app, &repeat);
        assert!(until(&mut app, 2_000, 20, |app| {
            for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
                econ::restore(app.world_mut(), actor, need, 100_000);
            }
            view::repeat_order(app.world(), id).is_none()
        }));
        let stopped = effects(app.world(), "VillageRepeatStopped");
        assert_eq!(
            stopped.last().expect("stopped")["reason"],
            "count_done",
            "{task}: {}",
            stopped.last().expect("stopped")
        );
    }
    // A third planting has no seed left: the order stops and says why.
    let mut repeat = LiveAct::new(id, "repeat");
    repeat.target = Some(first_garden);
    repeat.task = "plant".into();
    repeat.quantity = 1;
    act(&mut app, &repeat);
    assert!(until(&mut app, 400, 20, |app| view::repeat_order(
        app.world(),
        id
    )
    .is_none()));
    let last = effects(app.world(), "VillageRepeatStopped");
    assert_eq!(last.last().expect("stopped")["reason"], "inputs_gone");

    // A repeat order of crafting at the carpenter's workshop.
    let workshop =
        settlements::workplace(app.world(), settlement, Role::Carpenter).expect("workshop");
    let workshop_pos = *app
        .world()
        .get::<SimPos>(entity(app.world(), workshop))
        .expect("workshop");
    teleport(app.world_mut(), actor, workshop_pos);
    econ::grant(app.world_mut(), actor, Item::new("log").stack(1));
    let planks = econ::count(app.world(), actor, "plank");
    let mut craft = LiveAct::new(id, "repeat");
    craft.target = Some(workshop);
    craft.recipe = catalog::SAW.into();
    craft.quantity = 2;
    match craft.check(app.world()) {
        Ok(()) => {
            act(&mut app, &craft);
            stay(&mut app, actor, workshop_pos, 600);
            assert!(econ::count(app.world(), actor, "plank") >= planks + 4);
            let stopped = effects(app.world(), "VillageRepeatStopped");
            assert_eq!(
                stopped.last().expect("stopped")["reason"],
                "inputs_gone",
                "the second saw has no log"
            );
            let made = view::day_so_far(app.world(), id).expect("ledger").made;
            let earlier = view::day_summary(app.world(), id).map(|day| day.made);
            assert!(
                made.get("plank").copied().unwrap_or(0)
                    + earlier
                        .and_then(|made| made.get("plank").copied())
                        .unwrap_or(0)
                    >= 4,
                "made goods are booked as made"
            );
        }
        Err(reason) => panic!("a played character cannot saw at the workshop: {reason}"),
    }

    // A friend at a helping tier works on the tool shed in its free time.
    // The neighbourhood has warmed to the player: every resident of the
    // settlement is a friend at the helping tier (one of them is free first).
    let friend = settlement_friend(&app, settlement, id);
    for (neighbour, _) in residents(app.world(), settlement) {
        if neighbour == id {
            continue;
        }
        app.world_mut()
            .resource_mut::<LiveState>()
            .bonds
            .entry(id)
            .or_default()
            .entry(neighbour)
            .or_default()
            .score = 25_000;
    }
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    econ::grant(app.world_mut(), actor, Item::new("ingot").stack(1));
    upgrade(&mut app, id, "better_tools");
    // The owner stays away, so only help moves the work.
    teleport(app.world_mut(), actor, SimPos::new(door.x + 30_000, door.z));
    let helped = until(&mut app, 4 * DAY, 5, |app| {
        lowest = lowest.min(lowest_resident_need(app.world()));
        for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
            econ::restore(app.world_mut(), actor, need, 100_000);
        }
        !effects(app.world(), "VillageFriendHelped").is_empty()
    });
    let friend_actor = entity(app.world(), friend);
    assert!(
        helped,
        "no friend helped in four days: needs {:?}, decision {:?}",
        ["Hunger", "Thirst", "Energy", "Warmth"].map(|need| econ::need(
            app.world(),
            friend_actor,
            need
        )),
        village::behavior::decision(app.world(), friend_actor)
    );
    let help = &effects(app.world(), "VillageFriendHelped")[0];
    assert_eq!(help["to"], json!(id));
    let helper = SimId(
        effects(app.world(), "VillageFriendHelped")
            .into_iter()
            .next()
            .and_then(|_| {
                app.world()
                    .resource::<Journal>()
                    .entries
                    .iter()
                    .find(|entry| {
                        matches!(&entry.kind, JournalKind::Effect { effect, .. }
                            if effect.get("type").and_then(Value::as_str) == Some("VillageFriendHelped"))
                    })
                    .and_then(|entry| entry.unit)
            })
            .expect("a helping friend")
            .0,
    );
    if help["how"] != "harvest" {
        assert!(
            at_door(&app, helper, door),
            "the friend helped away from the door: {help}"
        );
    }
    stay(&mut app, actor, door, 700);
    assert!(view::homestead_tiers(app.world(), id)[2].done);
    assert_eq!(live::gather_percent(app.world(), actor), 80);

    // The larger cottage grows the body into the family house.
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(6));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    teleport(app.world_mut(), actor, door);
    upgrade(&mut app, id, "larger_cottage");
    stay(&mut app, actor, door, 1_000);
    let home = view::home(app.world(), house).expect("home");
    assert_eq!(home.capacity, 4);
    assert_eq!(
        (
            home.body_max.x - home.body_min.x,
            home.body_max.z - home.body_min.z
        ),
        (5_000, 6_000)
    );
    let goals = view::goals(app.world(), id);
    assert!(goals.done.len() >= 5, "{:?}", goals.current);

    // The day summary books goods by the action that brought them.
    run_ticks(app.world_mut(), DAY);
    assert!(view::day_summary(app.world(), id).is_some());

    // The village around the played life.
    lowest = lowest.min(lowest_resident_need(app.world()));
    let people = all_residents(app.world());
    assert_eq!(people.len(), founders, "nobody died or vanished");
    for (resident, body) in &people {
        assert!(village::living(app.world(), *body), "{} died", resident.0);
        assert!(colony::home_of(app.world(), *resident).is_some());
    }
    println!("HOMESTEAD_LIFE lowest_resident_need={lowest}");
    assert!(lowest >= 15_000, "a resident fell to {lowest}");
    assert_eq!(money(app.world()), total, "coins are conserved");
    let _ = VillageAct::new(id, "stop_work");
    village::integrity::validate_colony(app.world()).expect("valid after a played life");
}

/// A resident of the settlement, not the played character, for a friend.
fn settlement_friend(app: &App, settlement: SimId, player: SimId) -> SimId {
    residents(app.world(), settlement)
        .into_iter()
        .find(|(id, actor)| {
            *id != player
                && app
                    .world()
                    .get::<Villager>(*actor)
                    .is_some_and(|person| person.role != Role::Visitor)
        })
        .map(|(id, _)| id)
        .expect("a resident")
}

/// A village site paid by the civic fund and a homestead upgrade paid from
/// escrow, open at the same time in the same settlement: both are supplied by
/// hauling residents and built at their doors, the fund pays nothing for the
/// homestead and the escrow nothing for the village.
#[test]
fn a_village_site_and_a_homestead_site_open_at_once_both_finish() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let id = app.world().resource::<village::VillageState>().played;
    let actor = entity(app.world(), id);
    colony::JoinIsland { unit: id.0 }
        .perform(app.world_mut())
        .expect("arrived");
    let (plot_id, plot) = plots_of(&app, "housing")[0];
    let centre = *app.world().get::<SimPos>(plot).expect("plot");
    teleport(app.world_mut(), actor, centre);
    HomesteadAct::new(id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .expect("claimed");
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(8));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    HomesteadAct::new(id, "build_cottage", plot_id)
        .perform(app.world_mut())
        .expect("building");
    assert!(until(&mut app, 4_000, 10, |app| {
        app.world()
            .get::<HomesteadPlot>(plot)
            .is_some_and(|estate| estate.cottage_built)
    }));
    let door = homestead::front_door(centre);
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += 300;
    let settlement = live::homestead_settlement(app.world(), id).expect("home settlement");
    // The fund gets the coins for the stall from the residents, as a levy.
    fund(app.world_mut(), 200);
    let total = money(app.world());
    let def = village::civic::definition(app.world(), "market_stall")
        .expect("stall")
        .clone();
    let (x, z, rotation) =
        village::civic::find_site(app.world(), &def, settlement, 36).expect("a stall site");
    let stall = village::civic::PlaceBuilding {
        building: "market_stall".into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed");
    let fund_spent = app.world().resource::<village::civic::CivicState>().spent;
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    upgrade(&mut app, id, "storage");
    let mut order = LiveAct::new(id, "order");
    order.kind = "log".into();
    order.quantity = 2;
    stock_woodcutters(&mut app, settlement);
    act(&mut app, &order);
    act(&mut app, &LiveAct::new(id, "hire"));
    let escrow = view::upgrade_site(app.world(), id).expect("site").escrow;
    teleport(app.world_mut(), actor, SimPos::new(door.x + 30_000, door.z));
    let both = until(&mut app, 4 * DAY, 20, |app| {
        feed(app, actor);
        let village_done = app
            .world()
            .get::<village::civic::Building>(entity(app.world(), stall))
            .is_some();
        village_done && view::upgrade_site(app.world(), id).is_none()
    });
    assert!(
        both,
        "village site {:?}, homestead {:?}",
        village::civic::sites(app.world()),
        view::upgrade_site(app.world(), id)
    );
    assert!(view::homestead_tiers(app.world(), id)[0].done);
    let civic = app.world().resource::<village::civic::CivicState>();
    assert!(civic.spent > fund_spent, "the fund paid for the stall");
    assert_eq!(civic.cancelled, 0, "the planner cancelled nothing");
    assert!(escrow > 0);
    assert_eq!(money(app.world()), total, "coins are conserved");
}

/// A played character with a built cottage, standing at its door with coins
/// for orders, at nine o'clock (work hours) of a day.
fn homestead_owner() -> (App, SimId, Entity, SimPos, SimId) {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let id = app.world().resource::<village::VillageState>().played;
    let actor = entity(app.world(), id);
    colony::JoinIsland { unit: id.0 }
        .perform(app.world_mut())
        .expect("arrived");
    let (plot_id, plot) = plots_of(&app, "housing")[0];
    let centre = *app.world().get::<SimPos>(plot).expect("plot");
    teleport(app.world_mut(), actor, centre);
    HomesteadAct::new(id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .expect("claimed");
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(8));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    HomesteadAct::new(id, "build_cottage", plot_id)
        .perform(app.world_mut())
        .expect("building");
    assert!(until(&mut app, 4_000, 10, |app| {
        app.world()
            .get::<HomesteadPlot>(plot)
            .is_some_and(|estate| estate.cottage_built)
    }));
    let door = homestead::front_door(centre);
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += 300;
    let settlement = live::homestead_settlement(app.world(), id).expect("home settlement");
    let now = app.world().resource::<SimClock>().tick;
    let mut nine = now - now % DAY + DAY * 9 / 24;
    if nine < now {
        nine += DAY;
    }
    stay(&mut app, actor, door, nine - now);
    (app, id, actor, door, settlement)
}

/// Starts the storage tier with its logs from the pack and orders its four
/// planks; the owner walks away. Returns the tick of the order.
fn order_planks(app: &mut App, id: SimId, actor: Entity, door: SimPos) -> u64 {
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    upgrade(app, id, "storage");
    let mut order = LiveAct::new(id, "order");
    order.kind = "plank".into();
    order.quantity = 4;
    act(app, &order);
    teleport(app.world_mut(), actor, SimPos::new(door.x + 30_000, door.z));
    app.world().resource::<SimClock>().tick
}

fn planks_delivered(app: &App, id: SimId) -> u32 {
    view::upgrade_site(app.world(), id).map_or(4, |site| {
        site.delivered
            .iter()
            .find(|(kind, _)| kind == "plank")
            .map_or(0, |(_, count)| *count)
    })
}

/// A plank order placed in work hours is taken from a resident's spare
/// planks within about two island hours.
#[test]
fn a_plank_order_is_carried_from_spare_stock_within_two_hours() {
    let (mut app, id, actor, door, settlement) = homestead_owner();
    // A resident who is no carpenter holds planks it does not need.
    let (_, holder) = residents(app.world(), settlement)
        .into_iter()
        .find(|(other, person)| {
            *other != id
                && app
                    .world()
                    .get::<Villager>(*person)
                    .is_some_and(|person| person.role != Role::Carpenter)
        })
        .expect("a resident");
    econ::grant(app.world_mut(), holder, Item::new("plank").stack(4));
    let ordered = order_planks(&mut app, id, actor, door);
    let hours = DAY / 24;
    let taken = until(&mut app, 2 * hours, 5, |app| {
        feed(app, actor);
        view::upgrade_site(app.world(), id).is_some_and(|site| {
            site.workers
                .iter()
                .any(|worker| worker.job == "hauler" && worker.text.contains("plank"))
        }) || planks_delivered(app, id) > 0
    });
    assert!(
        taken,
        "nobody took the plank order within two hours of tick {ordered}: {:?}",
        view::upgrade_site(app.world(), id)
    );
    let delivered = until(&mut app, DAY, 10, |app| {
        feed(app, actor);
        planks_delivered(app, id) >= 4
    });
    assert!(delivered, "{:?}", view::upgrade_site(app.world(), id));
}

/// When nobody on the island has a plank to spare, the open order is demand
/// for the carpenters: they saw the planks and a resident carries them.
#[test]
fn a_plank_order_nobody_can_fill_is_sawn_by_a_carpenter() {
    let (mut app, id, actor, door, _) = homestead_owner();
    for (_, person) in all_residents(app.world()) {
        let planks = econ::count(app.world(), person, "plank");
        if planks > 0 {
            econ::consume(app.world_mut(), person, "plank", planks).expect("planks");
        }
    }
    let sawn_before = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .get("plank")
        .copied()
        .unwrap_or(0);
    let ordered = order_planks(&mut app, id, actor, door);
    let delivered = until(&mut app, DAY, 10, |app| {
        feed(app, actor);
        planks_delivered(app, id) >= 4
    });
    let sawn = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .get("plank")
        .copied()
        .unwrap_or(0)
        - sawn_before;
    assert!(
        delivered,
        "the plank order of tick {ordered} was not filled in a day ({sawn} sawn): {:?}",
        view::upgrade_site(app.world(), id)
    );
    assert!(sawn > 0, "the planks were sawn for the order");
}

/// Builder states seen once a tick until the upgrade is finished:
/// `(state, ticks)` spans, and whether "walking" was ever seen standing still.
fn builder_spans(
    app: &mut App,
    id: SimId,
    owner: Entity,
    keep_owner_at: Option<SimPos>,
) -> (bool, Vec<(&'static str, u64)>, bool) {
    let mut spans: Vec<(&'static str, u64)> = Vec::new();
    let mut still_walking = false;
    let finished = until(app, 3 * DAY, 1, |app| {
        feed(app, owner);
        if let Some(door) = keep_owner_at {
            teleport(app.world_mut(), owner, door);
        }
        let worker = view::upgrade_site(app.world(), id).and_then(|site| {
            site.workers
                .into_iter()
                .find(|worker| worker.job == "builder")
        });
        if let Some(worker) = worker {
            if worker.state == "walking"
                && app
                    .world()
                    .get::<MoveIntent>(entity(app.world(), worker.id))
                    .is_none()
            {
                still_walking = true;
            }
            match spans.last_mut() {
                Some((state, ticks)) if *state == worker.state => *ticks += 1,
                _ => spans.push((worker.state, 1)),
            }
        }
        view::upgrade_site(app.world(), id).is_none()
    });
    (finished, spans, still_walking)
}

/// The hired builder, starting from its own home, reaches the door and
/// works there on its first trip: `hired`, then one `walking` span, then
/// `working` for each stage. Prints how long the states last, in ticks.
#[test]
fn the_hired_builder_is_seen_working_at_the_door() {
    let (mut app, id, actor, door, _) = homestead_owner();
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    upgrade(&mut app, id, "storage");
    act(&mut app, &LiveAct::new(id, "hire"));
    let builder = view::upgrade_site(app.world(), id)
        .and_then(|site| {
            site.workers
                .into_iter()
                .find(|worker| worker.job == "builder")
        })
        .expect("a builder")
        .id;
    let builder_actor = entity(app.world(), builder);
    let home = app
        .world()
        .get::<Villager>(builder_actor)
        .expect("builder")
        .home;
    teleport(app.world_mut(), builder_actor, home);
    // The owner stands 7 m from the door, as in play.
    teleport(app.world_mut(), actor, SimPos::new(door.x + 7_000, door.z));
    let (finished, spans, still_walking) = builder_spans(&mut app, id, actor, None);
    println!("BUILDER_STATES spans={spans:?}");
    assert!(finished, "{:?}", view::upgrade_site(app.world(), id));
    let first_work = spans
        .iter()
        .position(|(state, _)| *state == "working")
        .expect("the builder was never seen working at the door");
    let trips = spans[..first_work]
        .iter()
        .filter(|(state, _)| *state == "walking")
        .count();
    assert_eq!(trips, 1, "the first stage took {trips} trips: {spans:?}");
    assert!(
        spans[..first_work]
            .iter()
            .all(|(state, _)| *state != "hired")
            || spans[0].0 == "hired" && spans[1..first_work].iter().all(|(s, _)| *s != "hired"),
        "the trip lapsed back to hired: {spans:?}"
    );
    assert!(!still_walking, "walking while standing still: {spans:?}");
}

/// While the owner works the stage by hand at the door, the hired builder
/// joins it (both paces add): it is seen working, never walking in place.
#[test]
fn the_hired_builder_joins_the_owner_working_at_the_door() {
    let (mut app, id, actor, door, _) = homestead_owner();
    teleport(app.world_mut(), actor, door);
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    upgrade(&mut app, id, "storage");
    act(&mut app, &LiveAct::new(id, "hire"));
    // The builder starts 10 m away, so it arrives while the owner works.
    let builder = view::upgrade_site(app.world(), id)
        .and_then(|site| {
            site.workers
                .into_iter()
                .find(|worker| worker.job == "builder")
        })
        .expect("a builder")
        .id;
    let builder_actor = entity(app.world(), builder);
    teleport(
        app.world_mut(),
        builder_actor,
        SimPos::new(door.x - 10_000, door.z),
    );
    let (finished, spans, still_walking) = builder_spans(&mut app, id, actor, Some(door));
    println!("BUILDER_WITH_OWNER spans={spans:?}");
    assert!(finished, "{:?}", view::upgrade_site(app.world(), id));
    assert!(!still_walking, "walking while standing still: {spans:?}");
    assert!(
        spans.iter().any(|(state, _)| *state == "working"),
        "the builder never joined the work: {spans:?}"
    );
}
