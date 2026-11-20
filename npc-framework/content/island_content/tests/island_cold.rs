//! Nobody freezes (living world): a resident without a coat who is getting
//! too cold goes home to get warm before anything but a lower critical need;
//! out in the cold without a coat no outdoor errand starts; and ahead of
//! winter a resident without a coat buys one, the civic fund paying for it
//! when the resident cannot.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Season, Villager, Wallet, catalog, economy as econ, living};
use life_support::*;
use npc_framework::interaction::items;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

const DAY: u64 = 2_400;

/// Short days, snow every day, a winter day; no boat.
fn winter(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "snow");
    economy(world)["merchant"]["enabled"] = json!(false);
    economy(world)["tuning"]["founding_day"] = json!(23);
}

fn set_need(app: &mut App, actor: Entity, need: &str, value: i32) {
    let id = app
        .world()
        .resource::<npc_framework::needs::NeedRegistry>()
        .id(need)
        .expect("need");
    app.world_mut()
        .get_mut::<npc_framework::needs::Needs>(actor)
        .expect("needs")
        .get_mut(id)
        .value = value;
}

/// Takes every coat a resident wears or carries.
fn strip_coat(app: &mut App, actor: Entity) {
    if let Some(coat) = items::equipped_matching(app.world(), actor, catalog::WARM_CLOTHES)
        .and_then(|id| app.world().resource::<SimIndex>().get(id))
    {
        items::take_from_holder(app.world_mut(), coat);
        app.world_mut().despawn(coat);
    }
    let held = econ::count(app.world(), actor, catalog::WARM_CLOTHES);
    if held > 0 {
        econ::consume(app.world_mut(), actor, catalog::WARM_CLOTHES, held).expect("taken");
    }
    assert!(!living::has_coat(app.world(), actor));
}

/// Sage's case (seed 7, tick 193800 of the 14-day audit): a farmer without a
/// coat, out in the snow 44 m from home, hungry but not critically, buying
/// food while its Warmth ran down to zero. Now it goes home to get warm first.
#[test]
fn a_freezing_resident_without_a_coat_gets_warm_before_buying_food() {
    let mut app = world_with(winter);
    run_ticks(app.world_mut(), 2);
    assert_eq!(village::calendar(app.world()).1, Season::Winter);
    let (id, actor) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == village::Role::Farmer)
        })
        .expect("a farmer");
    strip_coat(&mut app, actor);
    let home = app.world().get::<Villager>(actor).expect("villager").home;
    teleport(app.world_mut(), actor, SimPos::new(home.x + 44_000, home.z));
    set_need(&mut app, actor, "Warmth", 22_000);
    set_need(&mut app, actor, "Hunger", 40_000);
    let mut lowest = i32::MAX;
    let mut reached = false;
    for _ in 0..240 {
        run_ticks(app.world_mut(), 10);
        let warmth = econ::need(app.world(), actor, "Warmth");
        lowest = lowest.min(warmth);
        let here = *app.world().get::<SimPos>(actor).expect("position");
        reached |= here.dist2(home) <= 1_500_i64.pow(2);
        if reached && warmth >= 40_000 {
            break;
        }
    }
    assert!(reached, "{} never went home", id.0);
    assert!(lowest >= 15_000, "Warmth fell to {lowest}");
    assert!(econ::need(app.world(), actor, "Warmth") > lowest);
}

/// The first leaf the resident starts for warmth or water from now on.
fn first_label(app: &mut App, actor: Entity) -> String {
    let id = *app.world().get::<SimId>(actor).expect("id");
    let from = app.world().resource::<SimClock>().tick;
    for _ in 0..300 {
        run_ticks(app.world_mut(), 1);
        let started = app
            .world()
            .resource::<Journal>()
            .entries
            .iter()
            .filter(|entry| entry.unit == Some(id) && entry.tick >= from)
            .find_map(|entry| match &entry.kind {
                JournalKind::LeafStart { action, status, .. }
                    if format!("{status:?}") == "Running"
                        && ["life.warm_up", "life.drink", "life.buy", "life.gather"]
                            .contains(&action.as_str()) =>
                {
                    Some(action.clone())
                }
                _ => None,
            });
        if let Some(action) = started {
            return action;
        }
    }
    panic!("neither warmth nor water in 300 ticks");
}

/// The lowest critical need comes first: critically thirsty and colder
/// still, the resident warms up; critically thirsty and warmer, it drinks.
#[test]
fn the_lowest_critical_need_is_served_first() {
    let mut app = world_with(winter);
    set_journal(app.world_mut(), true, true);
    run_ticks(app.world_mut(), 2);
    let (_, actor) = all_residents(app.world())[0];
    strip_coat(&mut app, actor);
    // Out of doors and awake.
    let home = app.world().get::<Villager>(actor).expect("villager").home;
    let away = SimPos::new(home.x + 20_000, home.z);
    teleport(app.world_mut(), actor, away);
    set_need(&mut app, actor, "Thirst", 12_000);
    set_need(&mut app, actor, "Warmth", 8_000);
    let label = first_label(&mut app, actor);
    assert_eq!(label, "life.warm_up");
    set_need(&mut app, actor, "Warmth", 14_000);
    set_need(&mut app, actor, "Thirst", 5_000);
    let id = *app.world().get::<SimId>(actor).expect("id");
    village::VillageAct::new(id, "stop_work")
        .perform(app.world_mut())
        .expect("stopped");
    teleport(app.world_mut(), actor, away);
    let label = first_label(&mut app, actor);
    assert!(
        matches!(label.as_str(), "life.drink" | "life.buy" | "life.gather"),
        "{label}"
    );
}

/// Out in the cold without a coat and below `exposure_warmth`, no outdoor
/// errand starts: the resident waits at home instead.
#[test]
fn no_outdoor_errand_starts_in_the_cold_without_a_coat() {
    let mut app = world_with(winter);
    set_journal(app.world_mut(), true, true);
    run_ticks(app.world_mut(), 2);
    let (id, actor) = all_residents(app.world())[1];
    strip_coat(&mut app, actor);
    set_need(&mut app, actor, "Warmth", 45_000);
    assert!(living::exposed(app.world(), actor));
    // Only starts from now on count: at tick 0, warm and in a coat, the
    // resident may start an errand (S4: the new help leaf gives an idle
    // resident one; the test read the whole journal, the founding included).
    let exposed_from = app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), 400);
    let errand = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| entry.unit == Some(id) && entry.tick >= exposed_from)
        .any(|entry| {
            matches!(&entry.kind, JournalKind::LeafStart { action, status, .. }
                if format!("{status:?}") == "Running"
                    && ["life.gather", "life.haul", "life.socialise", "life.craft"]
                        .contains(&action.as_str()))
        });
    assert!(!errand, "an outdoor errand started in the cold");
}

/// Ahead of winter a resident without a coat buys one; one who cannot pay
/// is given the price by the civic fund, and that is journaled.
#[test]
fn ahead_of_winter_everybody_gets_a_coat() {
    let mut app = world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
        economy(world)["merchant"]["enabled"] = json!(false);
        // Late autumn: within the coat lead before winter.
        economy(world)["tuning"]["founding_day"] = json!(19);
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    assert!(living::coat_season(app.world()));
    let coins = money(app.world());
    // Above the reserve the fund keeps for destitution grants.
    fund(app.world_mut(), 600);
    let (id, actor) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role != village::Role::Tailor)
        })
        .expect("a resident");
    strip_coat(&mut app, actor);
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins = 0;
    // A tailor has coats to sell, and keeps having them: the whole village
    // without coats buys at once, and those who can pay come first.
    let tailor = all_residents(app.world())
        .into_iter()
        .find(|(_, other)| {
            app.world()
                .get::<Villager>(*other)
                .is_some_and(|person| person.role == village::Role::Tailor)
                && village::frontier::settlements::member(app.world(), *other)
                    == village::frontier::settlements::member(app.world(), actor)
        })
        .expect("a tailor at home")
        .1;
    for _ in 0..20 {
        econ::grant(
            app.world_mut(),
            tailor,
            npc_framework::interaction::Item::new(catalog::WARM_CLOTHES),
        );
    }
    // The fund pays open building sites and hands its surplus back as the
    // daily dividend; the residents keep it able to pay a coat at its
    // ceiling price (170 coins), so what is tested is the grant itself.
    let dressed = until(&mut app, 6 * DAY, 1, |app| {
        top_up_fund(app.world_mut(), 400, actor);
        // It cannot pay until the fund gives it the price (a relief payment
        // would otherwise buy the coat first): its coins go to the fund.
        if !effects(app.world(), "VillageCoatGrant")
            .iter()
            .any(|grant| grant["resident"] == json!(id))
        {
            let mut wallet = app.world_mut().get_mut::<Wallet>(actor).expect("wallet");
            let coins = std::mem::take(&mut wallet.coins);
            app.world_mut().resource_mut::<village::VillageState>().fund += coins;
        }
        let spare = econ::count(app.world(), tailor, catalog::WARM_CLOTHES).saturating_sub(
            village::production::desired_stock(app.world(), tailor, catalog::WARM_CLOTHES),
        );
        for _ in spare..3 {
            econ::grant(
                app.world_mut(),
                tailor,
                npc_framework::interaction::Item::new(catalog::WARM_CLOTHES),
            );
        }
        living::has_coat(app.world(), actor)
            || econ::count(app.world(), actor, catalog::WARM_CLOTHES) > 0
    });
    let grants = effects(app.world(), "VillageCoatGrant");
    let region = village::frontier::settlements::member(app.world(), actor).expect("home");
    assert!(
        dressed,
        "{} has no coat after six days (grants {grants:?}, coat season {}, on offer {},          price {:?}, budget {}, decision {:?}; the tailor holds {} and keeps {})",
        id.0,
        living::coat_season(app.world()),
        village::stock::on_offer(app.world(), region, catalog::WARM_CLOTHES),
        village::market::unit_price(app.world(), catalog::WARM_CLOTHES),
        village::civic::budget(app.world()),
        village::behavior::decision(app.world(), actor),
        econ::count(app.world(), tailor, catalog::WARM_CLOTHES),
        village::production::desired_stock(app.world(), tailor, catalog::WARM_CLOTHES),
    );
    assert!(
        grants.iter().any(|grant| grant["resident"] == json!(id)),
        "{grants:?}"
    );
    // Paid by the fund: the grant names the coins it gave.
    assert!(
        grants
            .iter()
            .any(|grant| grant["resident"] == json!(id) && grant["coins"].as_u64() > Some(0)),
        "{grants:?}"
    );
    let _ = coins;
}

/// Moves coins above 100 of residents other than `tested` (whose grant is on
/// its way to a coat) into the fund until it holds `level`.
fn top_up_fund(world: &mut World, level: u32, tested: Entity) {
    for (_, actor) in all_residents(world) {
        if actor == tested {
            continue;
        }
        let fund = world.resource::<village::VillageState>().fund;
        if fund >= level {
            return;
        }
        let part = world
            .get::<Wallet>(actor)
            .map_or(0, |wallet| wallet.coins.saturating_sub(100))
            .min(level - fund);
        world.get_mut::<Wallet>(actor).expect("wallet").coins -= part;
        world.resource_mut::<village::VillageState>().fund += part;
    }
}

/// A resident who holds a coat and wears none puts it on at once, in any
/// season: here in spring, far from the coat season.
#[test]
fn a_coat_carried_is_worn_at_once_in_any_season() {
    let mut app = world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
        economy(world)["merchant"]["enabled"] = json!(false);
        economy(world)["tuning"]["founding_day"] = json!(0);
    });
    run_ticks(app.world_mut(), 2);
    assert!(!living::coat_season(app.world()));
    let (_, actor) = all_residents(app.world())[0];
    strip_coat(&mut app, actor);
    econ::grant(
        app.world_mut(),
        actor,
        npc_framework::interaction::Item::new(catalog::WARM_CLOTHES),
    );
    run_ticks(app.world_mut(), 40);
    assert!(
        living::has_coat(app.world(), actor),
        "the carried coat was not put on"
    );
    assert_eq!(econ::count(app.world(), actor, catalog::WARM_CLOTHES), 0);
}

/// Two founders in three come wearing a coat, spread over trades and
/// households; the rest buy theirs from the tailors.
#[test]
fn two_founders_in_three_come_wearing_a_coat() {
    let mut app = world_with(|_| {});
    run_ticks(app.world_mut(), 1);
    let people = all_residents(app.world());
    let coated: Vec<_> = people
        .iter()
        .filter(|(_, actor)| living::has_coat(app.world(), *actor))
        .collect();
    let (total, worn) = (people.len(), coated.len());
    assert!(
        worn * 3 >= total * 2 - 3 && worn * 3 <= total * 2 + 3,
        "{worn} of {total} wear a coat"
    );
    let trades: std::collections::BTreeSet<_> = coated
        .iter()
        .filter_map(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .map(|person| person.role)
        })
        .collect();
    assert!(
        trades.len() >= 8,
        "coats are spread over trades: {trades:?}"
    );
    let homes: std::collections::BTreeSet<_> = coated
        .iter()
        .filter_map(|(id, _)| village::colony::home_of(app.world(), *id))
        .collect();
    assert!(
        homes.len() >= 10,
        "coats are spread over households: {}",
        homes.len()
    );
}

/// Every settlement is founded with its tailors' workshop built and each
/// tailor holding one batch of coat materials (`tuning.found_coat_workshops`).
#[test]
fn every_settlement_is_founded_with_a_coat_workshop_and_materials() {
    let mut app = world_with(|_| {});
    run_ticks(app.world_mut(), 3);
    let workshops: Vec<_> = village::civic::buildings(app.world())
        .into_iter()
        .filter(|building| building.building == "tailor_shop")
        .collect();
    let settlements: std::collections::BTreeSet<_> = workshops
        .iter()
        .map(|building| building.settlement)
        .collect();
    assert_eq!(settlements.len(), 3, "{workshops:?}");
    let tailors: Vec<_> = all_residents(app.world())
        .into_iter()
        .filter(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == village::Role::Tailor)
        })
        .collect();
    assert!(!tailors.is_empty());
    for (_, tailor) in tailors {
        // The first coat recipe of the data: 2 leather and 2 thread.
        let thread = econ::count(app.world(), tailor, catalog::THREAD);
        let leather = econ::count(app.world(), tailor, catalog::LEATHER);
        assert!(
            thread >= 2 && leather >= 2,
            "a tailor holds {thread} thread, {leather} leather"
        );
    }
}
