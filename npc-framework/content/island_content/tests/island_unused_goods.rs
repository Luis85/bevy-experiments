//! Goods a resident has no use for go back to market (living world): a
//! former tailor's thread is carried to a store, where the tailors buy it.
mod life_support;

use island_content::village::{self, Role, Villager, catalog, civic, economy as econ, market};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::json;

fn holder(app: &bevy_app::App, role: Role) -> (SimId, bevy_ecs::prelude::Entity) {
    let harbour = settlement(app.world(), "harbour");
    residents(app.world(), harbour)
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == role)
        })
        .expect("a resident of the trade")
}

#[test]
fn thread_a_fisher_cannot_use_is_offered_and_carried_to_a_store() {
    let mut app = world_with(|world| {
        short_days(world, 6_000);
        only_weather(world, "clear");
        economy(world)["tuning"]["founding_day"] = json!(0);
    });
    let (_, fisher) = holder(&app, Role::Fisher);
    let (_, tailor) = holder(&app, Role::Tailor);
    econ::grant(app.world_mut(), fisher, Item::new(catalog::THREAD).stack(4));
    econ::grant(app.world_mut(), tailor, Item::new(catalog::THREAD).stack(4));
    // The fisher has no use for thread; the tailor spins and sews with it.
    assert!(market::no_use(app.world(), fisher, catalog::THREAD));
    assert!(!market::no_use(app.world(), tailor, catalog::THREAD));
    assert!(!market::no_use(app.world(), fisher, catalog::WATER));
    // Stores stand in the harbour (the trading post and the market stall,
    // founded with the boat): the fisher's whole
    // thread is a deposit.
    run_ticks(app.world_mut(), 2);
    let trip = civic::surplus_trip(app.world(), fisher).expect("a deposit for the thread");
    assert_eq!((trip.1.as_str(), trip.2), (catalog::THREAD, 4));
    // The tailor makes thread: it stores only what lies above its working
    // stock of 2 (`tuning.depot_working_stock`).
    let held = econ::count(app.world(), tailor, catalog::THREAD);
    if let Some((_, kind, units)) = civic::surplus_trip(app.world(), tailor)
        && kind == catalog::THREAD
    {
        assert!(units + 2 <= held, "{units} of {held}");
    }
    // Within a day the thread leaves the fisher's pack.
    let gone = until(&mut app, 6_000, 20, |app| {
        econ::count(app.world(), fisher, catalog::THREAD) == 0
    });
    assert!(
        gone,
        "the fisher still holds {} thread",
        econ::count(app.world(), fisher, catalog::THREAD)
    );
    village::integrity::validate_colony(app.world()).expect("valid");
}
