//! The Trade workspace reads the boat's outlook and the played character's
//! goods on stalls straight from the world, and changes nothing.
use super::model::trade;
use super::*;
use island_content::village::{self, civic, economy as econ};
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;

fn trade_rows(world: &World) -> Vec<model::Row> {
    model::rows(
        world,
        &json!({"sections":{"village":{"nearby_market":[]}}}),
        &PlayerState {
            tab: Some(Tab::Trade),
            ..default()
        },
        true,
    )
}

fn titles(rows: &[model::Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            if row.detail.is_empty() {
                format!("{} [{}]", row.title, row.key)
            } else {
                format!("{} | {} [{}]", row.title, row.detail, row.key)
            }
        })
        .collect()
}

/// The first resident, made the played character.
fn play_first_resident(world: &mut World) -> (SimId, Entity) {
    let mut people: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<village::Villager>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    let (id, entity) = people[0];
    world.resource_mut::<village::VillageState>().played = id;
    (id, entity)
}

#[test]
fn trade_words_are_plain_and_counted() {
    use crate::island_shell::model::BoatOutlook;
    assert_eq!(
        trade::boat_when_line(&BoatOutlook::Due { days: 2 }),
        "The boat calls in 2 days."
    );
    assert_eq!(
        trade::boat_when_line(&BoatOutlook::Due { days: 1 }),
        "The boat calls tomorrow."
    );
    assert_eq!(
        trade::boat_when_line(&BoatOutlook::Due { days: 0 }),
        "The boat calls later today."
    );
    assert_eq!(
        trade::boat_when_line(&BoatOutlook::In {
            until: "14:20".into()
        }),
        "The boat is in until 14:20."
    );
    assert_eq!(
        trade::boat_goods_line(
            &["plank".into(), "cloth".into()],
            &[("salt".into(), 6), ("iron_tool".into(), 1)]
        ),
        "It buys planks, cloth. It brings 6 salt, 1 iron tool."
    );
    assert_eq!(trade::boat_goods_line(&[], &[]), "");
    assert_eq!(
        trade::boat_prices_line(&[
            ("plank".into(), 1200),
            ("salt".into(), 1000),
            ("cloth".into(), 900)
        ])
        .as_deref(),
        Some("Prices outside: planks up 20%, cloth down 10%.")
    );
    assert_eq!(trade::boat_prices_line(&[("salt".into(), 1000)]), None);
    assert_eq!(
        trade::consigned_line(true, "Harbour market", "bread", 6, Some(4)),
        "On the stall at Harbour market: 6 bread at 4 coins each."
    );
    assert_eq!(
        trade::consigned_line(false, "Harbour barn 1", "plank", 1, Some(1)),
        "In the store at Harbour barn 1: 1 plank at 1 coin each."
    );
    assert_eq!(
        trade::consigned_line(true, "Harbour market", "hammer", 2, None),
        "On the stall at Harbour market: 2 hammers, no market price yet."
    );
}

#[test]
fn trade_shows_the_boat_and_an_empty_shelf_without_touching_the_world() {
    let mut app = island_content::new_sim_app(7, "living_world");
    run_ticks(app.world_mut(), 2);
    play_first_resident(app.world_mut());
    let before = state_hash(app.world_mut());
    let rows = trade_rows(app.world());
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "reading changes nothing"
    );
    let lines = titles(&rows);
    println!("{}", lines.join("\n"));
    assert!(
        village::view::boat(app.world()).enabled,
        "the island has a boat"
    );
    let when = rows
        .iter()
        .find(|row| row.key == "boat_when")
        .expect("boat");
    assert!(when.title.starts_with("The boat "), "{}", when.title);
    assert!(when.detail.starts_with("It buys "), "{}", when.detail);
    let empty = rows
        .iter()
        .find(|row| row.key == "consigned_empty")
        .expect("empty state");
    assert_eq!(empty.title, trade::NOTHING_CONSIGNED);
    // The boat leads the stalls, and the stalls lead the offers.
    let at = |key: &str| rows.iter().position(|row| row.key == key).unwrap();
    assert!(at("boat_when") < at("consigned"));
    assert!(at("consigned") < at("offers"));
}

#[test]
fn a_lot_on_a_stall_shows_with_its_units_and_quote() {
    let mut app = island_content::new_sim_app(7, "living_world");
    // The founders and their buildings are in place after the first ticks.
    run_ticks(app.world_mut(), 2);
    let (owner_id, owner) = play_first_resident(app.world_mut());
    let (stall_id, stall) = civic::all_depots(app.world())
        .into_iter()
        .find(|(_, entity)| {
            app.world()
                .get::<civic::Depot>(*entity)
                .is_some_and(|depot| depot.public)
        })
        .expect("a market stall");
    let door = *app.world().get::<SimPos>(stall).expect("stall");
    app.world_mut()
        .entity_mut(owner)
        .remove::<MoveIntent>()
        .insert(door);
    econ::grant(app.world_mut(), owner, Item::new("hammer"));
    econ::grant(app.world_mut(), owner, Item::new("hammer"));
    civic::depot_deposit(app.world_mut(), owner, stall, "hammer", 2).expect("laid down");
    let lots = village::view::consigned(app.world(), owner_id);
    assert_eq!(lots.len(), 1);
    let before = state_hash(app.world_mut());
    let rows = trade_rows(app.world());
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "reading changes nothing"
    );
    println!("{}", titles(&rows).join("\n"));
    assert!(rows.iter().all(|row| row.key != "consigned_empty"));
    let lot = rows
        .iter()
        .find(|row| row.key == format!("consigned_{}_hammer", stall_id.0))
        .expect("the lot");
    let label = app
        .world()
        .get::<civic::Building>(stall)
        .map(|building| building.label.clone())
        .expect("a stall has a name");
    let expected = trade::consigned_line(true, &label, "hammer", 2, lots[0].quote);
    assert_eq!(lot.title, expected);
    assert!(
        lot.title
            .starts_with(&format!("On the stall at {label}: 2 hammers"))
    );
}
