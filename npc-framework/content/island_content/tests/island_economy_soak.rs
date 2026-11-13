//! Maintained economic fixtures, not evidence of unaided discovery or a live provider.
//! The operator harness owns the clock and reads fixture identities/positions. The
//! visiting actor receives no money, goods, need edits or teleportation.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, ResourcePatch, Role, Season, VillageState, Villager, Vitality, Wallet,
};
use npc_framework::agent_io::api;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const OUTPUTS: &[&str] = &[
    village::catalog::GRAIN,
    village::catalog::FLOUR,
    "bread",
    village::catalog::WATER,
    "log",
    "plank",
    village::catalog::BOWL,
    "ore",
    "ingot",
    "hammer",
];

fn id(app: &App, name: &str) -> SimId {
    app.world()
        .resource::<island_content::TestbedIds>()
        .id(name)
}

fn entity(app: &App, id: SimId) -> Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

fn money(app: &mut App) -> u64 {
    let wallets: u64 = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|w| u64::from(w.coins))
        .sum();
    let purses: u64 = app
        .world_mut()
        .query::<&CoinPurse>()
        .iter(app.world())
        .map(|w| u64::from(w.coins))
        .sum();
    wallets + purses + u64::from(app.world().resource::<VillageState>().fund)
}

fn act(app: &mut App, mut args: Value) {
    args["unit"] = json!(app.world().resource::<VillageState>().played.0);
    args["controller"] = json!("visitor");
    let reply = api::call(app.world_mut(), "agent/act", Some(args)).unwrap();
    assert_eq!(reply["accepted"], true, "{reply}");
}

fn walk(app: &mut App, pos: SimPos) {
    act(
        app,
        json!({"steps":[
            {"verb":"order","args":{"order":{"type":"Move","pos":[pos.x,pos.z]}}},
            {"verb":"execute_orders"}
        ]}),
    );
    run_ticks(app.world_mut(), 900);
    let actor = entity(app, app.world().resource::<VillageState>().played);
    let here = *app.world().get::<SimPos>(actor).unwrap();
    assert!(
        here.dist2(pos) <= 2_250_000,
        "walk failed: {here:?} -> {pos:?}"
    );
}

fn visit(app: &mut App, name: &str) -> SimId {
    let target = id(app, name);
    // Residents may move while the visitor approaches; reacquire their position.
    for _ in 0..6 {
        let actor = entity(app, app.world().resource::<VillageState>().played);
        let trader = entity(app, target);
        if village::economy::near(app.world(), actor, trader) {
            return target;
        }
        let pos = *app.world().get::<SimPos>(trader).unwrap();
        walk(app, pos);
    }
    panic!("moving trader {name} could not be reached within the bounded retry budget");
}

fn visitor_cycle(app: &mut App) -> Value {
    run_ticks(app.world_mut(), 1);
    let actor = entity(app, app.world().resource::<VillageState>().played);
    assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, 0);
    let patch = id(app, "patch_log");
    let pos = *app.world().get::<SimPos>(entity(app, patch)).unwrap();
    walk(app, pos);
    for _ in 0..4 {
        act(app, json!({"verb":"gather","target":patch.0}));
        run_ticks(app.world_mut(), 260);
    }
    let inn = visit(app, "Tess");
    act(
        app,
        json!({"verb":"sell","target":inn.0,"args":{"kind":"log","quantity":4}}),
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, 12);
    let first_sale_tick = app.world().resource::<SimClock>().tick;
    assert!(
        first_sale_tick <= 12_000,
        "first sale missed ten-minute 1x target"
    );
    let mut purchases = Vec::new();
    for (name, kind, verb, need) in [
        ("Mira", "bread", "eat", "Hunger"),
        ("Nell", village::catalog::WATER, "drink", "Thirst"),
    ] {
        let trader = visit(app, name);
        act(
            app,
            json!({"verb":"buy","target":trader.0,"args":{"kind":kind}}),
        );
        run_ticks(app.world_mut(), 1);
        let before = village::economy::need(app.world(), actor, need);
        act(app, json!({"verb":verb}));
        run_ticks(app.world_mut(), 1);
        assert!(village::economy::need(app.world(), actor, need) > before);
        purchases.push(json!({"kind":kind,"tick":app.world().resource::<SimClock>().tick}));
    }
    let inn = visit(app, "Tess");
    let before = village::economy::need(app.world(), actor, "Energy");
    act(app, json!({"verb":"rest","target":inn.0}));
    run_ticks(app.world_mut(), village::catalog::BED_TICKS + 2);
    assert!(village::economy::need(app.world(), actor, "Energy") > before);
    assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, 2);
    let cycle_tick = app.world().resource::<SimClock>().tick;
    assert!(
        cycle_tick < 24_000,
        "needs cycle missed the normal-day target"
    );
    json!({"first_sale_tick":first_sale_tick,"purchases":purchases,"cycle_tick":cycle_tick,
        "source_of_funds":"four gathered logs; no visitor subsidy"})
}

#[derive(Default)]
struct Availability {
    no_bread_samples: u64,
    no_water_samples: u64,
    no_log_buyer_samples: u64,
    longest_no_bread: u64,
    longest_no_water: u64,
    longest_no_buyer: u64,
    bread_streak: u64,
    water_streak: u64,
    buyer_streak: u64,
}

impl Availability {
    fn sample(&mut self, app: &App) {
        let bread = entity(app, id(app, "Mira"));
        let water = entity(app, id(app, "Nell"));
        let has_bread = village::economy::count(app.world(), bread, "bread") > 0;
        let has_water = village::economy::count(app.world(), water, village::catalog::WATER) > 0;
        let has_buyer = Role::RESIDENTS.into_iter().any(|role| {
            if !village::catalog::wants(role, "log") {
                return false;
            }
            let e = entity(app, id(app, role.person()));
            village::living(app.world(), e)
                && app.world().get::<Wallet>(e).is_some_and(|w| w.coins >= 3)
                && village::economy::count(app.world(), e, "log")
                    < village::catalog::stock_limit("log")
        });
        for (available, count, streak, longest) in [
            (
                has_bread,
                &mut self.no_bread_samples,
                &mut self.bread_streak,
                &mut self.longest_no_bread,
            ),
            (
                has_water,
                &mut self.no_water_samples,
                &mut self.water_streak,
                &mut self.longest_no_water,
            ),
            (
                has_buyer,
                &mut self.no_log_buyer_samples,
                &mut self.buyer_streak,
                &mut self.longest_no_buyer,
            ),
        ] {
            if available {
                *streak = 0;
            } else {
                *count += 1;
                *streak += 1;
                *longest = (*longest).max(*streak);
            }
        }
    }

    fn json(&self) -> Value {
        json!({"sample_interval_ticks":200,
            "unavailable_samples":{"bread":self.no_bread_samples,"water":self.no_water_samples,"log_buyer":self.no_log_buyer_samples},
            "longest_unavailable_sample_streak":{"bread":self.longest_no_bread,"water":self.longest_no_water,"log_buyer":self.longest_no_buyer}})
    }
}

fn report(app: &mut App, seed: u64, fixture: &str, availability: &Availability, cycle: Value) {
    let mut people: Vec<_> = app
        .world_mut()
        .query::<(Entity, &SimId, &Villager, &Vitality, &Wallet)>()
        .iter(app.world())
        .map(|(e, id, v, vital, wallet)| {
            let stock: BTreeMap<_, _> = village::catalog::TRADE_KINDS
                .iter()
                .map(|kind| (*kind, village::economy::count(app.world(), e, kind)))
                .collect();
            (
                *id,
                json!({"role":v.role,"health":vital.health,"dead":vital.died_at.is_some(),
                "coins":wallet.coins,"stock":stock}),
            )
        })
        .collect();
    people.sort_by_key(|(id, _)| *id);
    let state = app.world().resource::<VillageState>().clone();
    let result = json!({"fixture":fixture,"seed":seed,"tick":app.world().resource::<SimClock>().tick,
        "population":people.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "produced":state.produced,"transactions":state.transactions,"currency_total":money(app),
        "availability":availability.json(),"visitor_cycle":cycle});
    println!("ISLAND_ECONOMY_EVIDENCE {result}");
}

fn assert_residents_alive(app: &mut App, seed: u64) {
    let residents: Vec<_> = app
        .world_mut()
        .query::<(&Villager, &Vitality)>()
        .iter(app.world())
        .filter(|(v, _)| v.role != Role::Visitor)
        .map(|(v, h)| (v.role, h.health, h.died_at))
        .collect();
    assert_eq!(residents.len(), 9);
    assert!(
        residents
            .iter()
            .all(|(_, health, death)| *health > 0 && death.is_none()),
        "seed {seed}: {residents:?}"
    );
}

#[test]
fn three_seed_stream_length_soaks_conserve_currency_with_and_without_a_trading_visitor() {
    // A 45-minute stream crosses two normal day boundaries. Exercise a full
    // hour so nightly recovery and the second community payment cannot hide
    // behind a successful opening stock/endowment.
    const SOAK_TICKS: u64 = 24_000 * 3;
    for seed in [1, 7, 42] {
        for visitor in [false, true] {
            let mut app = island_content::new_sim_app(seed, "ai_village");
            assert_eq!(app.world().resource::<SimConfig>().day_length, 24_000);
            let total = money(&mut app);
            let cycle = if visitor {
                visitor_cycle(&mut app)
            } else {
                Value::Null
            };
            let mut availability = Availability::default();
            while app.world().resource::<SimClock>().tick < SOAK_TICKS {
                let remaining = SOAK_TICKS - app.world().resource::<SimClock>().tick;
                run_ticks(app.world_mut(), remaining.min(200));
                availability.sample(&app);
                assert_eq!(money(&mut app), total, "seed {seed}: currency changed");
            }
            // Cross the daily redistribution boundary as well as the night.
            run_ticks(app.world_mut(), 1);
            assert_eq!(money(&mut app), total);
            if !visitor {
                let actor = entity(&app, app.world().resource::<VillageState>().played);
                assert_eq!(
                    app.world().get::<Wallet>(actor).unwrap().coins,
                    0,
                    "a non-trading visitor must never receive a recurring stipend"
                );
            }
            report(
                &mut app,
                seed,
                if visitor {
                    "three_normal_days_opening_trading_visitor"
                } else {
                    "three_normal_days_npc_only"
                },
                &availability,
                cycle,
            );
            assert_residents_alive(&mut app, seed);
            let state = app.world().resource::<VillageState>();
            for kind in OUTPUTS {
                if *kind == village::catalog::BOWL && !state.produced.contains_key(*kind) {
                    // Jun starts with eight bowls, above the six-bowl target,
                    // while the inn needs only two. An early bowl stall is
                    // stocked saturation, not proof of broken production.
                    let carpenter = entity(&app, id(&app, "Jun"));
                    assert!(
                        village::economy::count(app.world(), carpenter, kind)
                            >= village::catalog::stock_limit(kind)
                    );
                    continue;
                }
                assert!(
                    state.produced.get(*kind).copied().unwrap_or(0) > 0,
                    "seed {seed} never produced {kind}: {:?}",
                    state.produced
                );
            }
            assert!(state.transactions > 0);
        }
    }
}

#[test]
fn depleted_bowl_demand_restarts_the_secondary_carpentry_stage_for_all_three_seeds() {
    for seed in [1, 7, 42] {
        let mut app = island_content::new_sim_app(seed, "ai_village");
        let total = money(&mut app);
        let carpenter = entity(&app, id(&app, "Jun"));
        let initial = village::economy::count(app.world(), carpenter, village::catalog::BOWL);
        assert!(initial >= village::catalog::stock_limit(village::catalog::BOWL));
        // Explicit demand fixture: operator depletes Jun's pre-endowed bowls;
        // no currency, input materials or actor needs are created or changed.
        village::economy::consume(app.world_mut(), carpenter, village::catalog::BOWL, initial)
            .unwrap();
        run_ticks(app.world_mut(), 2400);
        let produced = app
            .world()
            .resource::<VillageState>()
            .produced
            .get(village::catalog::BOWL)
            .copied()
            .unwrap_or(0);
        assert!(
            produced > 0,
            "seed {seed}: depleted bowl supply did not restart"
        );
        assert_eq!(money(&mut app), total);
        assert_residents_alive(&mut app, seed);
        report(
            &mut app,
            seed,
            "operator_depleted_bowl_demand",
            &Availability::default(),
            json!({"operator_removed_initial_bowls":initial,"new_bowls_produced":produced}),
        );
    }
}

#[test]
fn exhausted_field_replenishes_only_at_the_day_boundary_with_the_new_seasons_capacity() {
    for (day, season, expected_grain) in [
        (1, Season::Spring, 60),
        (7, Season::Summer, 80),
        (14, Season::Autumn, 60),
        (21, Season::Winter, 20),
        (28, Season::Spring, 60),
    ] {
        let mut app = island_content::new_sim_app(7, "ai_village");
        let day_length = app.world().resource::<SimConfig>().day_length;
        let boundary = day * day_length;
        let patch = entity(&app, id(&app, "patch_village_grain"));
        // Explicit operator boundary fixture. There is no visitor clock control,
        // free harvest or change to the shipped seasonal/calendar settings.
        app.world_mut().resource_mut::<SimClock>().tick = boundary - 1;
        {
            let mut field = app.world_mut().get_mut::<ResourcePatch>(patch).unwrap();
            assert_eq!(field.capacity, 80);
            field.remaining = 0;
            field.replenish_at = boundary;
        }
        let total = money(&mut app);
        run_ticks(app.world_mut(), 1);
        assert_eq!(
            app.world().get::<ResourcePatch>(patch).unwrap().remaining,
            0
        );
        run_ticks(app.world_mut(), 1);
        assert_eq!(village::calendar(app.world()).1, season);
        let field = app.world().get::<ResourcePatch>(patch).unwrap();
        assert_eq!(field.remaining, expected_grain, "day {day}: {season:?}");
        assert_eq!(field.replenish_at, boundary + day_length);
        // Exhausting that day's supply never grants a second harvest early.
        app.world_mut()
            .get_mut::<ResourcePatch>(patch)
            .unwrap()
            .remaining = 0;
        run_ticks(app.world_mut(), 10);
        assert_eq!(
            app.world().get::<ResourcePatch>(patch).unwrap().remaining,
            0
        );
        assert_eq!(money(&mut app), total);
    }
}
