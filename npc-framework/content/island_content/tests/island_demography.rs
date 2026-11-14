//! Demography of the coastal world: ages and life stages, couples, births,
//! children, coming of age, old age and inheritance, and the conservation of
//! coins and goods through all of it and through the boat's trade.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager, Wallet, civic,
    colony::{self, ColonyAct, SocialLife},
    demography::{self, Demography, Life, LifeStage},
    economy as econ,
    frontier::labor::ResidentProfile,
    merchant, view,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// One-day seasons of 1200 ticks, no boat and clear skies: a year is 4800 ticks.
fn quick_lives(world: &mut Value) {
    short_days(world, 1200);
    only_weather(world, "clear");
    economy(world)["tuning"]["days_per_season"] = json!(1);
    economy(world)["tuning"]["founding_day"] = json!(0);
    economy(world)["merchant"]["enabled"] = json!(false);
    let life = &mut economy(world)["demography"];
    life["courtship_days"] = json!(1);
    life["birth_chance_permille"] = json!(0);
}

fn life(world: &World, actor: Entity) -> Life {
    world.get::<Life>(actor).expect("life").clone()
}

fn befriend(world: &mut World, a: Entity, b: Entity, affinity: i32) {
    let (a_id, b_id) = (id_of(world, a), id_of(world, b));
    for (actor, other) in [(a, b_id), (b, a_id)] {
        world
            .get_mut::<SocialLife>(actor)
            .expect("social life")
            .affinity
            .insert(other, affinity);
    }
}

/// Makes two residents a couple under one roof, as courtship would.
fn marry(world: &mut World, a: Entity, b: Entity) -> SimId {
    let (a_id, b_id) = (id_of(world, a), id_of(world, b));
    world.get_mut::<Life>(a).expect("life").partner = Some(b_id);
    world.get_mut::<Life>(b).expect("life").partner = Some(a_id);
    let house = colony::home_of(world, a_id).expect("home");
    ColonyAct {
        verb: "assign_home".into(),
        resident: Some(b_id),
        house: Some(house),
        value: 0,
    }
    .perform(world)
    .expect("moved in");
    house
}

fn young(world: &mut World, actor: Entity) {
    let today = village::calendar(world).0 as i64;
    let mut life = world.get_mut::<Life>(actor).expect("life");
    life.born_day = today - 10;
    life.death_day = today as u64 + 1_000;
    life.stage = LifeStage::Adult;
}

#[test]
fn founders_have_seeded_ages_and_stages_that_are_hashed_data() {
    let mut app = world_with(|_| {});
    let ages = |app: &App| -> Vec<u64> {
        all_residents(app.world())
            .into_iter()
            .map(|(_, actor)| demography::age_days(app.world(), actor).expect("age"))
            .collect()
    };
    let seven = ages(&app);
    assert_eq!(seven.len(), 42);
    // Between coming of age (8 seasons) and the founders' oldest (38), in days.
    assert!(
        seven.iter().all(|age| (8 * 7..39 * 7).contains(age)),
        "{seven:?}"
    );
    assert!(
        seven
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 20
    );
    assert_ne!(seven, ages(&world_for(11, |_| {})), "ages follow the seed");
    assert_eq!(seven, ages(&world_with(|_| {})));
    for (_, actor) in all_residents(app.world()) {
        let life = life(app.world(), actor);
        let today = village::calendar(app.world()).0;
        assert!(life.death_day > today, "nobody is founded dead");
        let elder = seven_days(app.world(), actor) >= 36 * 7;
        assert_eq!(life.stage == LifeStage::Elder, elder);
        assert_eq!(
            demography::work_percent(app.world(), actor),
            if elder { 125 } else { 100 }
        );
        assert!(life.partner.is_none() && life.parents.is_empty());
    }
    let census = view::census(app.world());
    assert_eq!(
        (census.residents, census.children, census.couples),
        (42, 0, 0)
    );
    // The age is part of the state hash.
    let before = state_hash(app.world_mut());
    let somebody = all_residents(app.world())[0].1;
    app.world_mut()
        .get_mut::<Life>(somebody)
        .expect("life")
        .born_day -= 1;
    assert_ne!(state_hash(app.world_mut()), before);
    // The household view reports it.
    let (id, actor) = all_residents(app.world())[5];
    let home = view::household(app.world(), id).expect("household view");
    assert_eq!(home.resident, id);
    assert_eq!(home.members.len(), 1);
    assert_eq!(home.capacity, 2);
    assert_eq!(home.free_beds, 1);
    assert_eq!(
        home.person.name,
        app.world().get::<Villager>(actor).expect("villager").name
    );
    assert_eq!(home.person.age_years, home.person.age_days / 28);
    assert!(home.partner.is_none() && home.children.is_empty());
    serde_json::to_value(&home).expect("the view is plain data");
}

fn seven_days(world: &World, actor: Entity) -> u64 {
    demography::age_days(world, actor).expect("age")
}

#[test]
fn sustained_affinity_between_housed_adults_makes_a_couple_under_one_roof() {
    let mut app = world_with(quick_lives);
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, a), (b_id, b), (c_id, c), (_, d)) = (people[0], people[1], people[2], people[3]);
    for actor in [a, b, c, d] {
        young(app.world_mut(), actor);
    }
    befriend(app.world_mut(), a, b, 20_000);
    // One-sided fondness is not enough.
    let d_id = id_of(app.world(), d);
    app.world_mut()
        .get_mut::<SocialLife>(c)
        .expect("social")
        .affinity
        .insert(d_id, 90_000);
    let coins = money(app.world());
    // The first daily pass starts the courtship, the next ones test it.
    run_ticks(app.world_mut(), 1_201);
    assert_eq!(life(app.world(), a).courting, Some(b_id));
    assert!(effects(app.world(), "VillagePairing").is_empty(), "not yet");
    run_ticks(app.world_mut(), 1_200);
    let pairing = effects(app.world(), "VillagePairing");
    assert_eq!(pairing.len(), 1, "one couple formed");
    assert_eq!(pairing[0]["a"], json!(a_id));
    assert_eq!(pairing[0]["b"], json!(b_id));
    assert_eq!(life(app.world(), a).partner, Some(b_id));
    assert_eq!(life(app.world(), b).partner, Some(a_id));
    assert_eq!(life(app.world(), c).partner, None);
    let home = colony::home_of(app.world(), a_id);
    assert!(home.is_some() && home == colony::home_of(app.world(), b_id));
    assert_eq!(pairing[0]["home"], json!(home));
    let view = view::household(app.world(), a_id).expect("household");
    assert_eq!(view.partner.as_ref().and_then(|p| p.id), Some(b_id));
    assert_eq!(view.members.len(), 2);
    assert_eq!(view.free_beds, 0);
    assert_eq!(view::census(app.world()).couples, 1);
    assert_eq!(money(app.world()), coins);
    let _ = c_id;
    village::integrity::validate_colony(app.world()).expect("valid with a couple");
}

#[test]
fn a_couple_with_a_bed_and_food_has_a_child_who_eats_and_comes_of_age() {
    let mut app = world_with(|world| {
        quick_lives(world);
        let life = &mut economy(world)["demography"];
        life["birth_chance_permille"] = json!(1000);
        life["birth_cooldown_days"] = json!(1_000);
        life["adult_age_seasons"] = json!(2);
        life["birth_food_units"] = json!(2);
        // No building levy here: only the fund's own coins can pay for a home.
        economy(world)["tuning"]["building_levy_percent"] = json!(0);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, a), (b_id, b)) = (people[0], people[1]);
    for actor in [a, b] {
        young(app.world_mut(), actor);
    }
    let house = marry(app.world_mut(), a, b);
    // The home is full: a child needs a bed, or a home the planner can build.
    assert_eq!(demography::free_beds(app.world(), house), 0);
    assert_eq!(civic::budget(app.world()), 0);
    assert!(!civic::can_build_home(app.world(), harbour));
    run_ticks(app.world_mut(), 1_201);
    assert!(
        effects(app.world(), "VillageBirth").is_empty(),
        "no bed and no means to build one: no child"
    );
    fund(app.world_mut(), 700);
    assert!(civic::can_build_home(app.world(), harbour));
    for actor in [a, b] {
        econ::grant(app.world_mut(), actor, Item::new("bread").stack(4));
    }
    let coins = money(app.world());
    run_ticks(app.world_mut(), 1_200);
    let births = effects(app.world(), "VillageBirth");
    assert_eq!(births.len(), 1, "one child was born");
    assert_eq!(births[0]["parents"], json!([a_id, b_id]));
    assert_eq!(births[0]["home"], json!(house));
    let child = app.world().resource::<Demography>().children[0].clone();
    assert_eq!((child.home, child.fed), (house, 100_000));
    assert_eq!(demography::occupancy(app.world(), house), 3);
    assert_eq!(demography::crowding(app.world(), harbour), (1, 3));
    let home = view::household(app.world(), a_id).expect("household");
    assert_eq!(home.children_at_home.len(), 1);
    assert_eq!(home.children_at_home[0].stage, "child");
    assert_eq!(home.children.len(), 1);
    assert_eq!(view::census(app.world()).children, 1);
    // The child does not work and has no wallet; it eats the parents' food.
    assert_eq!(residents(app.world(), harbour).len(), 14);
    let food = |app: &App| {
        [
            "bread",
            village::catalog::COOKED_MEAL,
            village::catalog::BERRIES,
        ]
        .into_iter()
        .map(|kind| econ::count(app.world(), a, kind) + econ::count(app.world(), b, kind))
        .sum::<u32>()
    };
    app.world_mut().resource_mut::<Demography>().children[0].fed = 30_000;
    let before = food(&app);
    run_ticks(app.world_mut(), 600);
    let fed = app.world().resource::<Demography>().children[0].fed;
    assert!(fed > 30_000, "the child was fed: {fed}");
    assert!(food(&app) < before, "the meal came from a parent's pack");
    // The parents plan food for the child as well as for themselves.
    assert!(demography::dependants_hunger(app.world(), a, 2_400) > 0);

    // Two seasons later the child comes of age.
    let grown = until(&mut app, 2_400, 100, |app| {
        !effects(app.world(), "VillageComingOfAge").is_empty()
    });
    assert!(grown, "the child never came of age");
    let record = effects(app.world(), "VillageComingOfAge").remove(0);
    let id = SimId(record["resident"].as_u64().expect("id") as u32);
    let actor = entity(app.world(), id);
    assert!(app.world().resource::<Demography>().children.is_empty());
    assert_eq!(residents(app.world(), harbour).len(), 15);
    let profile = app.world().get::<ResidentProfile>(actor).expect("profile");
    let person = app.world().get::<Villager>(actor).expect("villager");
    assert_eq!(person.name, child.name);
    assert!(profile.allowed_roles.contains(&person.role));
    // One aptitude is a parent's.
    let inherited = record["inherited_aptitude"].as_str().expect("aptitude");
    let parents: Vec<_> = [a, b]
        .into_iter()
        .flat_map(|parent| {
            app.world()
                .get::<ResidentProfile>(parent)
                .expect("profile")
                .allowed_roles
                .clone()
        })
        .collect();
    assert!(parents.iter().any(|role| role.name() == inherited));
    assert!(
        profile
            .allowed_roles
            .iter()
            .any(|role| role.name() == inherited)
    );
    let adult = life(app.world(), actor);
    assert_eq!(adult.parents, vec![a_id, b_id]);
    assert_eq!(adult.stage, LifeStage::Adult);
    assert_eq!(adult.born_day, child.born_day as i64);
    // The parents' gift is theirs: no coin was made.
    let gift = record["gift_coins"].as_u64().expect("gift");
    assert!(gift > 0);
    assert_eq!(
        u64::from(app.world().get::<Wallet>(actor).expect("wallet").coins),
        gift
    );
    assert_eq!(money(app.world()), coins);
    let family = view::household(app.world(), a_id).expect("household");
    assert!(family.children.iter().any(|kid| kid.id == Some(id)));
    assert_eq!(
        view::household(app.world(), id)
            .expect("child's view")
            .parents
            .len(),
        2
    );
    assert_eq!(view::census(app.world()).came_of_age, 1);
    village::integrity::validate_colony(app.world()).expect("valid with a new adult");
}

#[test]
fn births_wait_for_food_and_children_are_rehoused_when_a_home_stands() {
    let mut app = world_with(|world| {
        quick_lives(world);
        let life = &mut economy(world)["demography"];
        life["birth_chance_permille"] = json!(1000);
        life["birth_food_units"] = json!(60);
        // One home of four beds stands empty in Harbour.
        world["living_world"]["settlements"][0]["homes"][13]["capacity"] = json!(4);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, a), (b_id, b), (last_id, last)) = (people[0], people[1], people[13]);
    let _ = (last, b_id);
    for actor in [a, b] {
        young(app.world_mut(), actor);
    }
    let house = marry(app.world_mut(), a, b);
    fund(app.world_mut(), 700);
    run_ticks(app.world_mut(), 1_201);
    assert!(
        effects(app.world(), "VillageBirth").is_empty(),
        "too little food in the house for a child"
    );
    // A child in an overfull home: the family moves when a home that fits
    // stands empty.
    let big = colony::home_of(app.world(), last_id).expect("the big home");
    ColonyAct {
        verb: "assign_home".into(),
        resident: Some(last_id),
        house: Some(colony::home_of(app.world(), people[12].0).expect("neighbour")),
        value: 0,
    }
    .perform(app.world_mut())
    .expect("the last founder moves in with a neighbour");
    {
        let mut state = app.world_mut().resource_mut::<Demography>();
        state.next_child = 1;
        state.children.push(demography::Child {
            id: 1,
            name: "Pip".into(),
            born_day: 0,
            home: house,
            settlement: harbour,
            parents: vec![a_id, b_id],
            fed: 100_000,
            unfed_ticks: 0,
            waiting_for_seat: false,
        });
    }
    assert_eq!(demography::crowding(app.world(), harbour), (1, 3));
    demography::rehouse_families(app.world_mut(), harbour);
    assert_eq!(colony::home_of(app.world(), a_id), Some(big));
    assert_eq!(colony::home_of(app.world(), b_id), Some(big));
    assert_eq!(app.world().resource::<Demography>().children[0].home, big);
    assert_eq!(demography::crowding(app.world(), harbour), (0, 0));
    assert_eq!(effects(app.world(), "VillageFamilyMoved").len(), 1);
    assert_eq!(demography::free_beds(app.world(), big), 1);
}

#[test]
fn old_age_takes_a_resident_on_the_seeded_day_and_the_household_inherits() {
    let mut app = world_with(|world| {
        quick_lives(world);
        add_salt(world);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((old_id, old), (heir_id, heir), (alone_id, alone), (ageing_id, ageing)) =
        (people[0], people[1], people[2], people[3]);
    for actor in [old, heir, alone, ageing] {
        young(app.world_mut(), actor);
    }
    marry(app.world_mut(), old, heir);
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .get_mut::<Life>(old)
        .expect("life")
        .death_day = today + 1;
    app.world_mut()
        .get_mut::<Life>(alone)
        .expect("life")
        .death_day = today + 1;
    // Somebody turns elder on the same morning (36 seasons of one day).
    app.world_mut()
        .get_mut::<Life>(ageing)
        .expect("life")
        .born_day = today as i64 - 35;
    let speed = app
        .world()
        .get::<Villager>(ageing)
        .expect("villager")
        .base_speed;
    econ::grant(app.world_mut(), old, Item::new(SALT).stack(3));
    let coins = money(app.world());
    let salt = units(app.world_mut(), SALT);
    run_ticks(app.world_mut(), 1_201);

    let deaths = effects(app.world(), "VillageDeath");
    assert_eq!(deaths.len(), 2);
    assert!(deaths.iter().all(|death| death["cause"] == "old age"));
    assert!(deaths.iter().all(|death| death["coins_dropped"] == 0));
    assert!(!village::living(app.world(), old) && !village::living(app.world(), alone));
    assert_eq!(life(app.world(), old).cause_of_death, "old age");
    // The partner in the household inherits coins and plain goods.
    let wills = effects(app.world(), "VillageInheritance");
    let will = wills
        .iter()
        .find(|will| will["from"] == json!(old_id))
        .expect("the estate is journaled");
    let estate = will["coins"].as_u64().expect("coins");
    assert!(estate > 0, "the estate held coins");
    assert_eq!(will["heirs"], json!([{"heir": heir_id, "coins": estate}]));
    assert_eq!(will["to_civic_fund"], 0);
    assert_eq!(app.world().get::<Wallet>(old).expect("wallet").coins, 0);
    let salted = will["goods"]
        .as_array()
        .expect("goods")
        .iter()
        .find(|entry| entry["kind"] == SALT)
        .expect("the salt is in the will");
    assert_eq!(salted["quantity"], 3);
    assert_eq!(salted["heir"], json!(heir_id));
    assert_eq!(econ::count(app.world(), heir, SALT), 3);
    assert_eq!(econ::count(app.world(), old, SALT), 0);
    assert_eq!(life(app.world(), heir).partner, None, "widowed");
    // Nobody shared the other home: the coins go to the civic fund.
    let lonely_will = wills
        .iter()
        .find(|will| will["from"] == json!(alone_id))
        .expect("second estate");
    assert_eq!(lonely_will["to_civic_fund"], lonely_will["coins"]);
    assert!(lonely_will["coins"].as_u64().expect("coins") > 0);
    assert_eq!(lonely_will["heirs"], json!([]));
    // Nothing was made or lost.
    assert_eq!(money(app.world()), coins);
    assert_eq!(units(app.world_mut(), SALT), salt);
    let census = view::census(app.world());
    assert_eq!(census.residents, 40);
    assert_eq!(census.deaths.get("old age"), Some(&2));
    // The home stays with the household.
    assert!(colony::home_of(app.world(), heir_id).is_some());

    // The elder slowed down.
    assert_eq!(life(app.world(), ageing).stage, LifeStage::Elder);
    let elder = effects(app.world(), "VillageElder");
    assert_eq!(elder.len(), 1);
    assert_eq!(elder[0]["speed_percent"], 75);
    assert_eq!(
        app.world()
            .get::<Villager>(ageing)
            .expect("villager")
            .base_speed,
        speed * 75 / 100
    );
    assert_eq!(demography::work_percent(app.world(), ageing), 125);
    assert_eq!(
        view::household(app.world(), ageing_id)
            .expect("view")
            .person
            .stage,
        "elder"
    );
    village::integrity::validate_colony(app.world()).expect("valid after deaths");
}

/// A good nobody makes or uses, so every unit can be accounted for.
const SALT: &str = "village_salt";

fn add_salt(world: &mut Value) {
    economy(world)["goods"]
        .as_array_mut()
        .expect("goods")
        .push(json!({
            "key": SALT, "display": "salt", "base_price": 4,
            "item": {"weight_g": 200, "max_stack": 40}}));
}

#[test]
fn coins_and_goods_are_conserved_across_births_deaths_inheritance_and_boat_trade() {
    let mut app = world_with(|world| {
        quick_lives(world);
        add_salt(world);
        let life = &mut economy(world)["demography"];
        life["birth_chance_permille"] = json!(1000);
        life["birth_cooldown_days"] = json!(2);
        life["birth_food_units"] = json!(1);
        life["adult_age_seasons"] = json!(2);
        life["elder_age_seasons"] = json!(4);
        life["lifespan_min_seasons"] = json!(6);
        life["lifespan_max_seasons"] = json!(9);
        life["founder_max_age_seasons"] = json!(5);
        life["arrivals_per_visit"] = json!(1);
        economy(world)["merchant"] = json!({
            "enabled": true, "first_visit_day": 0, "interval_days": 1,
            "arrive_permille": 20, "stay_ticks": 600, "purse": 300,
            "buys": [{"good": SALT, "max_units": 3, "price_percent": 100}],
            "sells": [{"good": SALT, "units": 2, "price_percent": 150, "want_units": 10_000}]});
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    // Couples in every settlement, salt in packs and in the trading post.
    for key in ["harbour", "alderwood", "stone_ridge"] {
        let place = settlement(app.world(), key);
        let people = residents(app.world(), place);
        for pair in people.chunks(2).take(5) {
            marry(app.world_mut(), pair[0].1, pair[1].1);
            // The couples are made young adults: founders' ages are seeded
            // from their identities (2 to 5 seasons here, elder at 4), and
            // only a couple both aged 2 is still adult on the first day a
            // birth can happen. With the shipped construction's identities
            // (island split, step 5) no chosen couple happened to be; the
            // test needs couples, not that seed luck.
            let today = village::calendar(app.world()).0 as i64;
            for (_, actor) in pair {
                let mut life = app.world_mut().get_mut::<Life>(*actor).expect("life");
                life.born_day = today - 2;
                life.stage = LifeStage::Adult;
                life.death_day = life.death_day.max(today as u64 + 6);
            }
        }
        for (_, actor) in people {
            econ::grant(app.world_mut(), actor, Item::new(SALT).stack(2));
            econ::grant(app.world_mut(), actor, Item::new("bread").stack(2));
        }
    }
    let harbour = settlement(app.world(), "harbour");
    let post = entity(app.world(), view::boat(app.world()).post.expect("post"));
    let door = *app.world().get::<SimPos>(post).expect("post");
    let consignor = residents(app.world(), harbour)[13].1;
    econ::grant(app.world_mut(), consignor, Item::new(SALT).stack(20));
    teleport(app.world_mut(), consignor, door);
    civic::depot_deposit(app.world_mut(), consignor, post, SALT, 20).expect("consigned");
    fund(app.world_mut(), 900);

    let coins = money(app.world());
    let salt = u64::from(units(app.world_mut(), SALT));
    let ledger = |app: &mut App| -> (u64, u64) {
        let state = app.world().resource::<merchant::Merchant>();
        let (imported, exported) = (
            state.imported.get(SALT).copied().unwrap_or(0),
            state.exported.get(SALT).copied().unwrap_or(0),
        );
        (
            money(app.world()),
            u64::from(units(app.world_mut(), SALT)) + exported - imported,
        )
    };
    // The journal is a ring: look for the inheritance while it is recent.
    let mut inherited = false;
    for _ in 0..120 {
        run_ticks(app.world_mut(), 100);
        let now = app.world().resource::<SimClock>().tick;
        assert_eq!(ledger(&mut app), (coins, salt), "drift at tick {now}");
        inherited = inherited || !effects(app.world(), "VillageInheritance").is_empty();
    }
    let census = view::census(app.world());
    let state = app.world().resource::<merchant::Merchant>().clone();
    println!(
        "LIFE_CONSERVATION ticks=12000 census={census:?} visits={} exported={:?} imported={:?} \
         purse={} coins={coins} salt={salt}",
        state.visits, state.exported, state.imported, state.purse
    );
    // Everything the test claims to cover did happen.
    assert!(census.births > 0, "no child was born");
    assert!(census.came_of_age > 0, "no child came of age");
    assert!(
        census.deaths.get("old age").copied().unwrap_or(0) > 0,
        "nobody died of old age"
    );
    assert!(inherited, "nothing was inherited");
    assert!(state.visits >= 5);
    assert!(state.exported.get(SALT).copied().unwrap_or(0) >= 9);
    assert!(state.imported.get(SALT).copied().unwrap_or(0) >= 2);
    assert!(census.arrivals > 0, "no settler came");
    assert!(state.coins_paid > 0 && state.coins_earned > 0);
    assert_eq!(
        u64::from(state.purse) + state.coins_paid + state.duty_paid,
        300 + state.coins_earned,
        "the purse moves only with the trade and its duty to the fund"
    );
    village::integrity::validate_colony(app.world()).expect("valid after ten days of lives");
}

/// The fund cannot pay the grants of the day: the rest is raised from coins
/// above the floor, and nobody is left below it.
#[test]
fn a_relief_levy_pays_the_grants_an_empty_fund_cannot() {
    let mut app = world_with(quick_lives);
    set_journal(app.world_mut(), true, false);
    let floor = village::frontier::economy_spec::tuning(app.world())
        .expect("tuning")
        .destitution_floor;
    let people = all_residents(app.world());
    // Five residents lose everything to a sixth; the fund is empty.
    let mut moved =
        std::mem::take(&mut app.world_mut().resource_mut::<village::VillageState>().fund);
    for (_, actor) in people.iter().take(5) {
        moved += std::mem::take(
            &mut app
                .world_mut()
                .get_mut::<Wallet>(*actor)
                .expect("wallet")
                .coins,
        );
    }
    let rich = people[10].1;
    app.world_mut()
        .get_mut::<Wallet>(rich)
        .expect("wallet")
        .coins += moved;
    let coins = money(app.world());
    let levied = until(&mut app, 1_300, 1, |app| {
        !effects(app.world(), "VillageCivicLevy").is_empty()
    });
    assert!(levied, "the first day ended");
    let relief = effects(app.world(), "VillageReliefLevy");
    assert_eq!(relief.len(), 1, "one relief levy");
    let raised = relief[0]["raised"].as_u64().expect("raised");
    assert!(raised > 0);
    assert_eq!(
        relief[0]["raised"], relief[0]["needed"],
        "the whole shortfall"
    );
    assert!(raised <= relief[0]["capacity"].as_u64().expect("capacity"));
    assert_eq!(relief[0]["percent"], json!(5));
    let levy = effects(app.world(), "VillageCivicLevy");
    assert_eq!(levy[0]["relief_levy"], json!(raised));
    assert!(levy[0]["floor_grants"].as_u64().expect("grants") >= raised);
    for (id, actor) in all_residents(app.world()) {
        let held = app.world().get::<Wallet>(actor).expect("wallet").coins;
        assert!(held >= floor, "resident {} holds {held} coins", id.0);
    }
    assert_eq!(
        money(app.world()),
        coins,
        "a levy moves coins, it makes none"
    );
    village::integrity::validate_colony(app.world()).expect("valid after a relief levy");
}

/// A day after death the body is gone: the identity no longer resolves, the
/// bed is free, and the record of the life stays in the demography.
#[test]
fn the_dead_are_laid_to_rest_and_remembered() {
    let mut app = world_with(quick_lives);
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((old_id, old), (heir_id, heir)) = (people[0], people[1]);
    for actor in [old, heir] {
        young(app.world_mut(), actor);
    }
    let house = marry(app.world_mut(), old, heir);
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .get_mut::<Life>(old)
        .expect("life")
        .death_day = today + 1;
    let name = app
        .world()
        .get::<Villager>(old)
        .expect("villager")
        .name
        .clone();
    let coins = money(app.world());
    assert!(view::departed(app.world()).is_empty());
    run_ticks(app.world_mut(), 1_201);

    // Dead, inherited from, and still lying there for a day.
    assert!(!village::living(app.world(), old));
    let gone = view::departed(app.world());
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0].resident, Some(old_id));
    assert_eq!(gone[0].name, name);
    assert_eq!(gone[0].cause, "old age");
    assert_eq!(gone[0].day, today + 1);
    assert_eq!(gone[0].household, Some(house));
    assert_eq!(gone[0].settlement, Some(harbour));
    assert!(!gone[0].role.is_empty());
    assert!(!gone[0].laid_to_rest);
    assert!(app.world().resource::<SimIndex>().get(old_id).is_some());
    assert_eq!(effects(app.world(), "VillageInheritance").len(), 1);

    let before = state_hash(app.world_mut());
    run_ticks(app.world_mut(), 1_200);
    assert_ne!(before, state_hash(app.world_mut()));
    // The next morning the body is removed.
    assert!(
        app.world().resource::<SimIndex>().get(old_id).is_none(),
        "the body is gone"
    );
    let gone = view::departed(app.world());
    assert_eq!(gone.len(), 1);
    assert!(gone[0].laid_to_rest);
    assert_eq!(effects(app.world(), "VillageLaidToRest").len(), 1);
    assert!(
        all_residents(app.world())
            .iter()
            .all(|(id, _)| *id != old_id),
        "no roster names the dead"
    );
    // The bed is free and the widow keeps the home.
    let home = view::home(app.world(), house).expect("home");
    assert!(home.beds.iter().all(|bed| bed.resident != Some(old_id)));
    assert_eq!(
        view::bed_of(app.world(), heir_id).map(|bed| bed.0),
        Some(house)
    );
    assert_eq!(life(app.world(), heir).partner, None);
    assert_eq!(view::census(app.world()).deaths.get("old age"), Some(&1));
    assert_eq!(money(app.world()), coins, "no coin left with the body");
    village::integrity::validate_colony(app.world()).expect("valid without the body");
    run_ticks(app.world_mut(), 2_400);
    village::integrity::validate_colony(app.world()).expect("valid two days on");
}

/// The trailing case: the nearest food seller is walking away. A buyer does
/// not follow somebody on the move beyond a few steps; it turns to a seller
/// who stands still or to a store.
#[test]
fn a_hungry_resident_buys_at_a_fixed_place_and_does_not_trail_a_walking_seller() {
    let mut app = world_with(quick_lives);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let (_, buyer) = people[0];
    for kind in [
        "bread",
        village::catalog::COOKED_MEAL,
        village::catalog::BERRIES,
    ] {
        let held = econ::count(app.world(), buyer, kind);
        if held > 0 {
            econ::consume(app.world_mut(), buyer, kind, held).expect("emptied");
        }
    }
    let here = *app.world().get::<SimPos>(buyer).expect("buyer");
    // Whoever the buyer would go to now stands still.
    let first = view::supplier(app.world(), buyer, "bread").expect("somebody sells bread");
    let seller = entity(app.world(), first);
    assert!(app.world().get::<Villager>(seller).is_some(), "a person");
    app.world_mut().entity_mut(seller).remove::<MoveIntent>();
    teleport(app.world_mut(), seller, SimPos::new(here.x + 3_000, here.z));
    assert_eq!(view::supplier(app.world(), buyer, "bread"), Some(first));
    // The same seller walking off, twenty metres away: not a target any more.
    let far = SimPos::new(here.x + 20_000, here.z);
    app.world_mut().entity_mut(seller).insert((
        far,
        MoveIntent::to(SimPos::new(here.x + 90_000, here.z), 70),
    ));
    let other = view::supplier(app.world(), buyer, "bread");
    assert_ne!(other, Some(first), "the buyer would trail a walking seller");
    if let Some(other) = other {
        let place = entity(app.world(), other);
        assert!(
            app.world().get::<MoveIntent>(place).is_none()
                || app
                    .world()
                    .get::<SimPos>(place)
                    .is_some_and(|pos| pos.dist2(here) <= 6_000_i64.pow(2)),
            "the other source stands still or is at hand"
        );
    }
    // Walking, but within a few steps: the buyer can simply reach it.
    app.world_mut()
        .entity_mut(seller)
        .insert(SimPos::new(here.x + 3_000, here.z));
    assert_eq!(view::supplier(app.world(), buyer, "bread"), Some(first));
    // Standing still again, far away: a fixed place, whatever the distance.
    app.world_mut().entity_mut(seller).remove::<MoveIntent>();
    app.world_mut().entity_mut(seller).insert(far);
    assert!(view::supplier(app.world(), buyer, "bread").is_some());
}

/// A child with an empty larder: the parent fetches food for it, without
/// spare coins and without being hungry; a starving child wakes the parent.
#[test]
fn a_parent_fetches_food_for_a_hungry_child_with_an_empty_larder() {
    let mut app = world_with(quick_lives);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, a), (b_id, b)) = (people[0], people[1]);
    for actor in [a, b] {
        young(app.world_mut(), actor);
    }
    let house = marry(app.world_mut(), a, b);
    let foods = [
        "bread",
        village::catalog::COOKED_MEAL,
        village::catalog::BERRIES,
    ];
    for actor in [a, b] {
        for kind in foods {
            let held = econ::count(app.world(), actor, kind);
            if held > 0 {
                econ::consume(app.world_mut(), actor, kind, held).expect("emptied");
            }
        }
        // Fed themselves, and with no coin to spare for provisions.
        app.world_mut()
            .get_mut::<Wallet>(actor)
            .expect("wallet")
            .coins = 30;
    }
    assert!(!demography::child_needs_food(app.world(), a, 60_000));
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .resource_mut::<Demography>()
        .children
        .push(demography::Child {
            id: 1,
            name: "Wren".into(),
            born_day: today,
            home: house,
            settlement: harbour,
            parents: vec![a_id, b_id],
            fed: 40_000,
            unfed_ticks: 0,
            waiting_for_seat: false,
        });
    app.world_mut().resource_mut::<Demography>().next_child = 2;
    assert!(demography::child_needs_food(app.world(), a, 60_000));
    assert!(!demography::child_needs_food(app.world(), a, 1));
    let fed = until(&mut app, 1_100, 10, |app| {
        let world = app.world();
        let carried: u32 = [a, b]
            .iter()
            .map(|actor| {
                foods
                    .iter()
                    .map(|kind| econ::count(world, *actor, kind))
                    .sum::<u32>()
            })
            .sum();
        carried > 0 || world.resource::<Demography>().children[0].fed > 40_000
    });
    assert!(fed, "no parent brought food home for the child");
    village::integrity::validate_colony(app.world()).expect("valid");
}
