//! Residents go inside their homes: they rest, sleep, warm up and shelter
//! indoors, and nobody with a bed sleeps in the open.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager, Wallet,
    colony::{self, ColonyAct, Household, SocialLife},
    demography::{self, Demography},
    economy as econ,
    frontier::labor,
    indoors::{self, Indoors, Repose},
    living, view,
};
use life_support::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use serde_json::json;

fn set_need(world: &mut World, actor: Entity, need: &str, value: i32) {
    let id = world.resource::<NeedRegistry>().id(need).expect("need");
    world
        .get_mut::<Needs>(actor)
        .expect("needs")
        .get_mut(id)
        .value = value;
}

fn at_home(world: &World, actor: Entity) -> bool {
    let person = world.get::<Villager>(actor).expect("villager");
    world
        .get::<SimPos>(actor)
        .is_some_and(|here| here.dist2(person.home) <= 1_200_i64.pow(2))
}

/// Whether `id` is inside an inn in a guest bed it rented: a tired resident
/// far from home may sleep there (ADR 0041).
fn inn_guest(world: &World, id: SimId) -> bool {
    view::inside(world, id).is_some_and(|inn| {
        indoors::is_inn(world, inn)
            && view::inn(world, inn)
                .is_some_and(|inn| inn.beds.iter().any(|bed| bed.guest == Some(id)))
    })
}

/// Housed residents who rest in the open: asleep (a bed's rest) and collapsed
/// (the slow rest of exhaustion). Both must stay empty.
fn resting_outside(world: &World) -> Vec<String> {
    let now = world.resource::<SimClock>().tick;
    let mut out = Vec::new();
    for (id, actor) in all_residents(world) {
        let person = world.get::<Villager>(actor).expect("villager");
        if !village::living(world, actor)
            || person.resting_until <= now
            || colony::home_of(world, id).is_none()
        {
            continue;
        }
        let home = colony::home_of(world, id);
        if inn_guest(world, id) {
            continue;
        }
        if view::inside(world, id) != home || !at_home(world, actor) {
            out.push(format!(
                "{} ({}) rests at rate {} outside its home at tick {now}",
                person.name, id.0, person.rest_rate
            ));
        }
    }
    out
}

#[test]
fn a_tired_resident_walks_to_the_door_then_is_inside_and_asleep() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let (id, actor) = residents(app.world(), harbour)[2];
    let house = colony::home_of(app.world(), id).expect("home");
    let workplace = app
        .world()
        .get::<Villager>(actor)
        .expect("villager")
        .workplace;
    let away = *app
        .world()
        .get::<SimPos>(entity(app.world(), workplace))
        .expect("workplace");
    // At work, far from home, the resident is outdoors. It works a while,
    // so that coming home later is news.
    teleport(app.world_mut(), actor, away);
    run_ticks(app.world_mut(), 2);
    assert_eq!(view::inside(app.world(), id), None);
    assert_eq!(view::repose(app.world(), actor), Repose::Awake);
    run_ticks(app.world_mut(), 700);
    teleport(app.world_mut(), actor, away);
    run_ticks(app.world_mut(), 1);
    assert_eq!(view::inside(app.world(), id), None);
    // Tired: walk home (outdoors all the way), then inside, then at rest.
    set_need(app.world_mut(), actor, "Energy", 20_000);
    let mut walked = false;
    let rested = until(&mut app, 3_000, 1, |app| {
        let world = app.world();
        let inside = view::inside(world, id);
        if world.get::<MoveIntent>(actor).is_some() {
            walked = true;
            assert_eq!(inside, None, "a walking resident is not indoors");
        }
        if view::repose(world, actor) != Repose::Awake {
            assert!(at_home(world, actor), "resting away from the door");
        }
        inside == Some(house) && view::repose(world, actor) != Repose::Awake
    });
    assert!(walked, "the resident never walked home");
    assert!(rested, "the resident never rested inside its home");
    let person = app.world().get::<Villager>(actor).expect("villager");
    // By day it is a short rest at the nap rate; in the sleep block it is
    // the sleep of the night.
    let nap = village::frontier::economy_spec::tuning(app.world())
        .expect("tuning")
        .nap_energy_per_tick;
    assert_eq!(person.rest_rate, nap as i32);
    assert_eq!(view::repose(app.world(), actor), Repose::Resting);
    assert_eq!(view::bed_of(app.world(), id), Some((house, 0)));
    let home = view::home(app.world(), house).expect("home view");
    assert!(home.beds[0].in_bed && !home.beds[0].asleep);
    assert_eq!(home.occupants, vec![id]);
    let entered = effects(app.world(), "VillageWentIndoors");
    assert!(
        entered
            .iter()
            .any(|record| record["building"] == json!(house)),
        "going indoors after the walk is journaled"
    );
    // Rested, it leaves for work and is outdoors again.
    set_need(app.world_mut(), actor, "Energy", 100_000);
    let left = until(&mut app, 2_000, 5, |app| {
        view::inside(app.world(), id).is_none() && !at_home(app.world(), actor)
    });
    assert!(left, "the resident never left home again");
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn indoors_the_hearth_decides_warmth_and_passers_by_cannot_meet_the_resident() {
    let mut app = world_with(|world| {
        short_days(world, 2_000);
        only_weather(world, "snow");
        economy(world)["merchant"]["enabled"] = json!(false);
    });
    // Into the first night: residents are home, most of them asleep.
    run_ticks(app.world_mut(), 1_500);
    assert!(
        living::cold(app.world()),
        "a snowy night is cold in the open"
    );
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    // Somebody without a coat who is inside, and somebody held in the square
    // in the snow.
    let square = village::frontier::settlements::definition(app.world(), harbour)
        .expect("harbour")
        .center;
    let coatless = |app: &App, actor: Entity| !living::has_coat(app.world(), actor);
    let (out_id, outside) = *people
        .iter()
        .find(|(_, actor)| coatless(&app, *actor))
        .expect("somebody has no coat");
    let mut sheltered = None;
    for _ in 0..40 {
        for _ in 0..10 {
            teleport(app.world_mut(), outside, square);
            run_ticks(app.world_mut(), 1);
        }
        sheltered = people.iter().copied().find(|(id, actor)| {
            *id != out_id
                && coatless(&app, *actor)
                && view::inside(app.world(), *id).is_some()
                && econ::need(app.world(), *actor, "Warmth") < 99_000
        });
        if sheltered.is_some() {
            break;
        }
    }
    let (in_id, inside) = sheltered.expect("nobody was indoors at night");
    let house = view::inside(app.world(), in_id).expect("inside");
    let before = (
        econ::need(app.world(), inside, "Warmth"),
        econ::need(app.world(), outside, "Warmth"),
    );
    let mut stayed = true;
    for _ in 0..20 {
        teleport(app.world_mut(), outside, square);
        run_ticks(app.world_mut(), 1);
        stayed &= view::inside(app.world(), in_id) == Some(house);
    }
    let after = (
        econ::need(app.world(), inside, "Warmth"),
        econ::need(app.world(), outside, "Warmth"),
    );
    assert_eq!(view::inside(app.world(), out_id), None);
    assert!(after.1 < before.1, "the snow chills whoever is outside");
    if stayed {
        assert!(
            after.0 > before.0,
            "indoors the home shelters, awake or not: {} then {}",
            before.0,
            after.0
        );
    }

    // A neighbour standing at the door is outdoors and does not meet the
    // resident inside.
    let door = app.world().get::<Villager>(inside).expect("villager").home;
    let affinity = |app: &App, from: Entity, to: SimId| {
        app.world()
            .get::<SocialLife>(from)
            .and_then(|social| social.affinity.get(&to))
            .copied()
            .unwrap_or(0)
    };
    let known = affinity(&app, inside, out_id);
    let mut tested = 0;
    for _ in 0..60 {
        app.world_mut()
            .get_mut::<SocialLife>(outside)
            .expect("social")
            .last_social_tick = 0;
        teleport(app.world_mut(), outside, door);
        run_ticks(app.world_mut(), 1);
        if view::inside(app.world(), in_id) == Some(house) {
            tested += 1;
            assert_eq!(view::inside(app.world(), out_id), None);
        } else {
            break;
        }
    }
    if tested == 60 {
        assert_eq!(
            affinity(&app, inside, out_id),
            known,
            "no meeting through the wall"
        );
    }
    assert!(tested > 0);
}

#[test]
fn beds_are_shared_by_capacity_and_only_the_homeless_sleep_outside() {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        economy(world)["merchant"]["enabled"] = json!(false);
    });
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, _), (b_id, _)) = (people[0], people[1]);
    let house = colony::home_of(app.world(), a_id).expect("home");
    ColonyAct {
        verb: "assign_home".into(),
        resident: Some(b_id),
        house: Some(house),
        value: 0,
    }
    .perform(app.world_mut())
    .expect("moved in");
    {
        let mut state = app.world_mut().resource_mut::<Demography>();
        state.next_child = 1;
        state.children.push(demography::Child {
            id: 1,
            name: "Pip".into(),
            born_day: 19,
            home: house,
            settlement: harbour,
            parents: vec![a_id, b_id],
            fed: 100_000,
            unfed_ticks: 0,
            waiting_for_seat: false,
        });
    }
    let home = view::home(app.world(), house).expect("home");
    assert_eq!(home.capacity, 2);
    assert_eq!(home.beds.len(), 2);
    assert_eq!(
        home.beds.iter().map(|bed| bed.resident).collect::<Vec<_>>(),
        vec![Some(a_id), Some(b_id)],
        "the adults take the beds in identity order"
    );
    assert_eq!(home.without_bed, vec!["Pip".to_owned()], "the child shares");
    assert_eq!(view::bed_of(app.world(), b_id), Some((house, 1)));
    // A founding home: the body lies behind the door, towards -z.
    assert!(!home.built);
    assert_eq!(home.facing, 2);
    assert!(home.body_hi.1 < home.entrance_cell.1);
    assert!(home.body_lo.0 <= home.entrance_cell.0 && home.entrance_cell.0 <= home.body_hi.0);
    let (width, depth) = (
        home.body_hi.0 - home.body_lo.0 + 1,
        home.body_hi.1 - home.body_lo.1 + 1,
    );
    assert!(
        (4..=5).contains(&width) && (5..=6).contains(&depth),
        "{width} by {depth}"
    );
    // The emptied home has two free beds.
    let old = view::homes(app.world())
        .into_iter()
        .find(|home| home.beds.iter().all(|bed| bed.resident.is_none()) && home.capacity == 2)
        .expect("the home the partner left");
    assert!(old.beds.iter().all(|bed| bed.name.is_empty()));
    assert_eq!(view::homes(app.world()).len(), 42);
    serde_json::to_value(&home).expect("plain data");

    // A newcomer without a bed has no indoors: the one case of rest in the
    // open, until the planner gives it a bed.
    let newcomer = labor::settle_newcomer(app.world_mut(), harbour, "Wanderer").expect("newcomer");
    assert_eq!(view::bed_of(app.world(), newcomer), None);
    assert_eq!(view::inside(app.world(), newcomer), None);
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        view::inside(app.world(), newcomer),
        None,
        "without a home there is no indoors"
    );
    assert!(resting_outside(app.world()).is_empty());
}

/// Three short days (6000 ticks each): at every sampled tick no housed
/// resident rests or sleeps outside the building of its home.
#[test]
fn over_several_days_nobody_with_a_bed_rests_outside_their_home() {
    let mut app = world_with(|world| {
        short_days(world, 6_000);
        only_weather(world, "clear");
    });
    let mut asleep_indoors = 0_u32;
    let mut indoors_samples = 0_u32;
    for _ in 0..360 {
        run_ticks(app.world_mut(), 50);
        let offenders = resting_outside(app.world());
        assert!(offenders.is_empty(), "{offenders:?}");
        for (id, actor) in all_residents(app.world()) {
            let inside = view::inside(app.world(), id);
            if inside.is_some() && !inn_guest(app.world(), id) {
                indoors_samples += 1;
                assert_eq!(inside, colony::home_of(app.world(), id));
                assert!(at_home(app.world(), actor));
                assert!(app.world().get::<MoveIntent>(actor).is_none());
            }
            if view::repose(app.world(), actor) == Repose::Asleep {
                assert!(inside.is_some(), "asleep outdoors");
                asleep_indoors += 1;
            }
        }
    }
    println!("INDOORS samples={indoors_samples} asleep={asleep_indoors}");
    assert!(
        asleep_indoors > 100,
        "residents slept indoors: {asleep_indoors}"
    );
    assert!(indoors_samples > asleep_indoors);
    // Every resident carries the state, and it is part of the hash.
    let before = state_hash(app.world_mut());
    let (_, somebody) = all_residents(app.world())[0];
    let state = *app.world().get::<Indoors>(somebody).expect("indoors state");
    app.world_mut()
        .get_mut::<Indoors>(somebody)
        .expect("state")
        .since = state.since + 1;
    assert_ne!(state_hash(app.world_mut()), before);
    let _ = indoors::is_inn;
    village::integrity::validate_colony(app.world()).expect("valid");
}

/// The shipped world: in the middle of the night at least nine in ten housed
/// residents are asleep in their own bed, they stay asleep through the night,
/// and nobody with a bed rests outside its home.
#[test]
fn housed_residents_sleep_through_the_night_in_their_own_beds() {
    nights(1);
}

/// The same over three days. Run explicitly:
/// cargo test --locked --release -p island_content --test island_indoors -- --ignored --nocapture
#[test]
#[ignore = "explicit: three days (72000 ticks) of the shipped world"]
fn housed_residents_sleep_through_three_nights_in_their_own_beds() {
    nights(3);
}

fn nights(count: u64) {
    const DAY: u64 = 24_000;
    let mut app = world_with(|_| {});
    let asleep_at_home = |world: &World| -> (usize, usize) {
        let housed: Vec<_> = all_residents(world)
            .into_iter()
            .filter(|(id, actor)| {
                village::living(world, *actor) && colony::home_of(world, *id).is_some()
            })
            .collect();
        let asleep = housed
            .iter()
            .filter(|(id, actor)| {
                view::repose(world, *actor) == Repose::Asleep
                    && view::inside(world, *id) == colony::home_of(world, *id)
            })
            .count();
        (asleep, housed.len())
    };
    for night in 0..count {
        // The sleep block is the last three tenths of the day, shifted by at
        // most a fortieth for early and late risers.
        for (permille, least_percent) in [(780_u64, 0), (850, 90), (960, 90)] {
            let at = night * DAY + DAY * permille / 1000;
            let now = app.world().resource::<SimClock>().tick;
            run_ticks(app.world_mut(), at - now);
            let (asleep, housed) = asleep_at_home(app.world());
            println!("SLEEP night={night} permille={permille} asleep={asleep}/{housed}");
            assert!(
                asleep * 100 >= housed * least_percent,
                "night {night} at {permille}: {asleep} of {housed} asleep in their own bed"
            );
            let offenders = resting_outside(app.world());
            assert!(offenders.is_empty(), "{offenders:?}");
        }
        // Noon after: everybody is up.
        let noon = (night + 1) * DAY + DAY * 300 / 1000;
        if night + 1 < count {
            let now = app.world().resource::<SimClock>().tick;
            run_ticks(app.world_mut(), noon - now);
            let (asleep, _) = asleep_at_home(app.world());
            assert_eq!(asleep, 0, "nobody sleeps at noon");
        }
    }
    village::integrity::validate_colony(app.world()).expect("valid after the nights");
}

/// An inn is a place like a home: entrance, doorway, body, facing and a fixed
/// number of guest beds. A fifth guest is turned away.
#[test]
fn an_inn_is_a_place_with_guest_beds_and_a_full_inn_turns_guests_away() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let inns = view::inns(app.world());
    assert!(!inns.is_empty(), "the coastal world has an inn");
    let inn = inns[0].clone();
    assert_eq!(view::inn(app.world(), inn.inn), Some(inn.clone()));
    assert!(indoors::is_inn(app.world(), inn.inn));
    assert_eq!(
        view::inn(app.world(), view::homes(app.world())[0].house),
        None
    );
    // The body is what the simulation blocks: five by five metres in front of
    // the entrance, the doorway on the wall facing it.
    assert!(!inn.built);
    assert_eq!(inn.facing, 0);
    assert_eq!(inn.body_max.x - inn.body_min.x, 5_000);
    assert_eq!(inn.body_max.z - inn.body_min.z, 5_000);
    assert_eq!(inn.door, SimPos::new(inn.entrance.x, inn.body_min.z));
    assert!(inn.door.z > inn.entrance.z && inn.door.z - inn.entrance.z <= 1_000);
    assert_eq!(inn.capacity, view::INN_BEDS);
    assert_eq!(inn.beds.len(), 4);
    assert!(inn.beds.iter().all(|bed| bed.guest.is_none()));
    assert!(inn.occupants.is_empty());
    serde_json::to_value(&inn).expect("plain data");

    // An innkeeper at the entrance, and five guests beside him.
    let settlement = inn.settlement.expect("settlement");
    let people = residents(app.world(), settlement);
    let (keeper_id, keeper) = match inn.keeper {
        Some(id) => (id, entity(app.world(), id)),
        None => {
            let (id, actor) = people[0];
            let mut person = app
                .world_mut()
                .get_mut::<Villager>(actor)
                .expect("villager");
            person.role = village::Role::Innkeeper;
            person.workplace = inn.inn;
            (id, actor)
        }
    };
    teleport(app.world_mut(), keeper, inn.entrance);
    assert_eq!(
        view::inn(app.world(), inn.inn).expect("inn").keeper,
        Some(keeper_id)
    );
    let guests: Vec<_> = people
        .iter()
        .copied()
        .filter(|(id, _)| *id != keeper_id)
        .take(5)
        .collect();
    let mut outcomes = Vec::new();
    for (id, actor) in &guests {
        teleport(app.world_mut(), *actor, inn.entrance);
        app.world_mut()
            .get_mut::<Wallet>(*actor)
            .expect("wallet")
            .coins = 50;
        let mut rest = village::VillageAct::new(*id, "rest");
        rest.target = Some(keeper_id);
        outcomes.push(rest.perform(app.world_mut()));
    }
    assert!(outcomes[..4].iter().all(Result::is_ok), "{outcomes:?}");
    assert_eq!(outcomes[4], Err("the inn has no free bed".to_owned()));
    let full = view::inn(app.world(), inn.inn).expect("inn");
    let in_beds: Vec<_> = full.beds.iter().map(|bed| bed.guest).collect();
    assert_eq!(
        in_beds,
        guests[..4]
            .iter()
            .map(|(id, _)| Some(*id))
            .collect::<Vec<_>>(),
        "guests take the beds in order"
    );
    assert!(
        full.beds
            .iter()
            .all(|bed| bed.until > 2 && !bed.name.is_empty())
    );
    assert_eq!(full.occupants.len(), 4);
    assert_ne!(view::inside(app.world(), guests[4].0), Some(inn.inn));
    village::integrity::validate_colony(app.world()).expect("valid with a full inn");
}

/// A homestead cottage reports the body the simulation blocks: the two by
/// two metres of its footprint behind the plot, with its three beds.
#[test]
fn a_homestead_cottage_reports_the_body_the_simulation_blocks() {
    use island_content::village::homestead;
    use npc_framework::kernel::spawn_sim;
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let plot = SimPos::new(40_500, -60_500);
    let centre = homestead::cottage_center(plot);
    let body = homestead::cottage_footprint();
    let (house, _) = spawn_sim(
        app.world_mut(),
        (
            centre,
            homestead::Cottage {
                plot: SimId(0),
                plot_id: "test".into(),
            },
            Household {
                label: "Homestead cottage".into(),
                capacity: 3,
                residents: Vec::new(),
                door: homestead::front_door(plot),
                heated_until: 0,
                logs_burned: 0,
            },
        ),
    );
    let home = view::home(app.world(), house).expect("home view");
    assert_eq!(home.entrance, plot);
    assert_eq!(home.capacity, 3);
    assert_eq!(home.beds.len(), 3);
    assert_eq!(
        home.body_min,
        SimPos::new(centre.x - body.half_x_mm, centre.z - body.half_z_mm)
    );
    assert_eq!(
        home.body_max,
        SimPos::new(centre.x + body.half_x_mm, centre.z + body.half_z_mm)
    );
    assert_eq!(home.body_max.x - home.body_min.x, 2_000);
    assert_eq!(home.body_max.z - home.body_min.z, 2_000);
    assert_eq!(home.facing, 2, "the body lies towards -z of the plot");
    assert_eq!(home.door, SimPos::new(plot.x, home.body_max.z));
    // Cells: every cell of the reported body lies inside the blocked body.
    for x in home.body_lo.0..=home.body_hi.0 {
        for z in home.body_lo.1..=home.body_hi.1 {
            assert!(x * 1000 < home.body_max.x && (x + 1) * 1000 > home.body_min.x);
            assert!(z * 1000 < home.body_max.z && (z + 1) * 1000 > home.body_min.z);
        }
    }
    // A founding home and a built one put the doorway on the wall in front
    // of the entrance too.
    for other in view::homes(app.world()) {
        let on_wall = match other.facing {
            0 => other.door == SimPos::new(other.entrance.x, other.body_min.z),
            1 => other.door == SimPos::new(other.body_min.x, other.entrance.z),
            2 => other.door == SimPos::new(other.entrance.x, other.body_max.z),
            _ => other.door == SimPos::new(other.body_max.x, other.entrance.z),
        };
        assert!(on_wall, "{}: door {:?}", other.label, other.door);
        assert!(other.body_min.x <= other.door.x && other.door.x <= other.body_max.x);
    }
}
