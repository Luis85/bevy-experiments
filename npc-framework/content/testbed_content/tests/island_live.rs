//! Live mode in the coastal world: sleeping to the morning with a day
//! summary, repeat orders, bonds and gifts, homestead tiers and goals,
//! gatherings and mourning, and a scripted played life among the residents.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{
    self, ResourcePatch, VillageAct, Villager, Wallet,
    colony::{self, SocialLife},
    demography::Life,
    economy as econ,
    homestead::{self, HomesteadAct, HomesteadPlot},
    indoors::Repose,
    live::{self, LiveAct},
    market, view,
};

const DAY: u64 = 2_400;

/// Short days and clear skies; no boat and no births, so that only the
/// played life changes what the test counts.
fn quiet(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "clear");
    economy(world)["merchant"]["enabled"] = json!(false);
    economy(world)["demography"]["birth_chance_permille"] = json!(0);
}

fn player(app: &mut App) -> (SimId, Entity) {
    let id = app.world().resource::<village::VillageState>().played;
    let actor = entity(app.world(), id);
    let _ = colony::JoinIsland { unit: id.0 }.perform(app.world_mut());
    assert_eq!(
        app.world().get::<Villager>(actor).expect("villager").role,
        village::Role::Visitor
    );
    (id, actor)
}

/// A recorded command, applied on the next tick.
fn act<C: ContentCommand>(app: &mut App, command: &C) {
    submit(app.world_mut(), None, SimCommand::content(command)).expect("accepted");
    run_ticks(app.world_mut(), 1);
}

fn beside(app: &mut App, actor: Entity, other: Entity) {
    let pos = *app.world().get::<SimPos>(other).expect("position");
    teleport(app.world_mut(), actor, pos);
}

/// Claims a housing plot and builds the cottage with granted materials.
fn settle(app: &mut App, id: SimId, actor: Entity) -> (SimId, SimPos) {
    let (plot_id, plot) = homestead::plots(app.world())
        .into_iter()
        .find(|(_, plot)| {
            homestead::definition(app.world(), *plot).is_some_and(|def| def.use_kind == "housing")
                && app
                    .world()
                    .get::<HomesteadPlot>(*plot)
                    .is_some_and(|estate| estate.owner.is_none())
        })
        .expect("a free housing plot");
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
    let built = until(app, 4_000, 10, |app| {
        app.world()
            .get::<HomesteadPlot>(plot)
            .is_some_and(|estate| estate.cottage_built)
    });
    assert!(built, "the cottage was never finished");
    let house = colony::home_of(app.world(), id).expect("the cottage is home");
    (house, homestead::front_door(centre))
}

/// A living resident who offers `kind` for sale, with stock to spare.
fn seller(app: &mut App, kind: &str) -> (SimId, Entity) {
    let harbour = settlement(app.world(), "harbour");
    let (id, actor) = residents(app.world(), harbour)
        .into_iter()
        .find(|(_, actor)| market::sells(app.world(), *actor, kind))
        .expect("somebody sells it");
    econ::grant(app.world_mut(), actor, Item::new(kind).stack(8));
    (id, actor)
}

fn coins(app: &App) -> u64 {
    money(app.world())
}

#[test]
fn the_played_character_sleeps_to_the_morning_and_gets_a_day_summary() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (id, actor) = player(&mut app);
    let (house, door) = settle(&mut app, id, actor);
    // A purchase and a talk, as recorded commands, go into the day's account.
    let (baker_id, baker) = seller(&mut app, "bread");
    // The baker lends the price of a loaf: coins move, none are made.
    app.world_mut()
        .get_mut::<Wallet>(baker)
        .expect("wallet")
        .coins -= 20;
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += 20;
    run_ticks(app.world_mut(), 1);
    beside(&mut app, actor, baker);
    let before = econ::count(app.world(), actor, "bread");
    let mut buy = VillageAct::new(id, "buy");
    buy.target = Some(baker_id);
    buy.kind = "bread".into();
    act(&mut app, &buy);
    assert_eq!(econ::count(app.world(), actor, "bread"), before + 1);
    let mut talk = VillageAct::new(id, "talk");
    talk.target = Some(baker_id);
    act(&mut app, &talk);

    // Too early; then, in the evening at the door, to bed until morning.
    teleport(app.world_mut(), actor, door);
    let now = app.world().resource::<SimClock>().tick;
    let today = now / DAY;
    if now % DAY < DAY * 6 / 10 {
        let early = LiveAct::new(id, "sleep").check(app.world());
        assert!(
            early
                .as_ref()
                .is_err_and(|reason| reason.contains("too early")),
            "{early:?}"
        );
        run_ticks(app.world_mut(), today * DAY + DAY * 62 / 100 - now);
    }
    teleport(app.world_mut(), actor, door);
    assert_eq!(view::day_summary(app.world(), id), None, "no morning yet");
    act(&mut app, &LiveAct::new(id, "sleep"));
    let morning = (today + 1) * DAY;
    assert_eq!(view::asleep_until(app.world(), id), Some(morning));
    assert_eq!(view::repose(app.world(), actor), Repose::Asleep);
    assert_eq!(view::inside(app.world(), id), Some(house));
    assert!(
        LiveAct::new(id, "sleep")
            .check(app.world())
            .is_err_and(|reason| reason.contains("asleep"))
    );
    // The simulation skips nothing: every tick to the morning is played.
    let left = morning - app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), left - 1);
    assert_eq!(view::repose(app.world(), actor), Repose::Asleep);
    run_ticks(app.world_mut(), 3);
    assert_eq!(view::asleep_until(app.world(), id), None);
    assert_ne!(view::repose(app.world(), actor), Repose::Asleep);

    let summary = view::day_summary(app.world(), id).expect("the day's summary");
    assert_eq!(
        summary.day + 1,
        village::calendar(app.world()).0,
        "yesterday"
    );
    assert!(summary.spent > 0, "{summary:?}");
    assert!(
        summary
            .received
            .get("bread")
            .is_some_and(|units| *units >= 1)
    );
    assert!(summary.met.contains(&baker_id));
    assert_eq!(summary.low.len(), 4);
    assert!(summary.slept);
    assert_eq!(effects(app.world(), "VillageDaySummary").len(), 1);
    assert_eq!(effects(app.world(), "VillageSleep").len(), 1);
    // A second night can be cut short.
    run_ticks(app.world_mut(), DAY * 65 / 100);
    teleport(app.world_mut(), actor, door);
    act(&mut app, &LiveAct::new(id, "sleep"));
    assert!(view::asleep_until(app.world(), id).is_some());
    act(&mut app, &LiveAct::new(id, "wake"));
    assert_eq!(view::asleep_until(app.world(), id), None);
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn talk_trade_and_gifts_build_a_bond_with_real_effects_and_neglect_wears_it() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (id, actor) = player(&mut app);
    let (friend_id, friend) = seller(&mut app, "bread");
    // The friend lends some coins: coins move, none are made.
    app.world_mut()
        .get_mut::<Wallet>(friend)
        .expect("wallet")
        .coins -= 60;
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += 60;
    let total = coins(&app);
    let (liked, disliked) = view::gift_tastes(app.world(), friend_id).expect("tastes");
    assert_ne!(liked, disliked);
    econ::grant(app.world_mut(), actor, Item::new(liked.as_str()).stack(6));
    econ::grant(
        app.world_mut(),
        actor,
        Item::new(disliked.as_str()).stack(2),
    );
    assert!(view::relationships(app.world(), id).is_empty());

    let gift = |app: &mut App, kind: &str| {
        beside(app, actor, friend);
        let mut gift = LiveAct::new(id, "gift");
        gift.target = Some(friend_id);
        gift.kind = kind.into();
        act(app, &gift);
    };
    // A liked gift: 6,000 points, an acquaintance at once.
    let held = econ::count(app.world(), friend, &liked);
    gift(&mut app, &liked);
    assert_eq!(
        econ::count(app.world(), friend, &liked),
        held + 1,
        "handed over"
    );
    assert_eq!(live::score(app.world(), id, friend_id), 6_000);
    let bond = view::relationships(app.world(), id);
    assert_eq!(
        (bond[0].other, bond[0].tier.as_str()),
        (friend_id, "acquaintance")
    );
    assert_eq!(bond[0].discount_percent, 0);
    // A second gift the same day is taken and does not count.
    gift(&mut app, &liked);
    assert_eq!(live::score(app.world(), id, friend_id), 6_000);
    // A disliked gift the next day costs.
    run_ticks(app.world_mut(), DAY);
    gift(&mut app, &disliked);
    assert_eq!(live::score(app.world(), id, friend_id), 4_000);
    // Three more days of liked gifts and a talk a day: a friend.
    for _ in 0..3 {
        run_ticks(app.world_mut(), DAY);
        gift(&mut app, &liked);
        let mut talk = VillageAct::new(id, "talk");
        talk.target = Some(friend_id);
        beside(&mut app, actor, friend);
        act(&mut app, &talk);
    }
    let score = live::score(app.world(), id, friend_id);
    assert!(score >= 20_000, "{score}");
    let bond = view::relationships(app.world(), id);
    assert_eq!(bond[0].tier, "friend");
    assert_eq!(bond[0].discount_percent, 10);
    assert!(
        view::relationships(app.world(), friend_id)
            .iter()
            .any(|relation| relation.other == id && relation.with_player)
    );
    assert!(
        effects(app.world(), "VillageBondTier")
            .iter()
            .any(|record| record["tier"] == "friend" && record["up"] == true)
    );
    // Real effects. A friend takes a tenth off.
    beside(&mut app, actor, friend);
    let asked = market::asking_price(app.world(), friend, "bread").expect("price") * 5;
    let quote = market::quote(app.world(), actor, friend, "bread", 5).expect("quote");
    assert_eq!(quote.total, (asked - asked / 10).max(1));
    assert!(
        asked < 10 || quote.total < asked,
        "{asked} against {}",
        quote.total
    );
    // And gave something back when talked to.
    assert!(
        effects(app.world(), "VillageGiftReturned")
            .iter()
            .any(|record| record["to"] == json!(id)),
        "a friend returns a gift"
    );
    assert_eq!(coins(&app), total, "bonds move goods, never coins");
    // Neglect: far from everybody, after three days without a word the bond
    // fades day by day.
    teleport(app.world_mut(), actor, SimPos::new(120_000, 120_000));
    let score = live::score(app.world(), id, friend_id);
    run_ticks(app.world_mut(), DAY * 6);
    let faded = live::score(app.world(), id, friend_id);
    assert!(
        faded < score && faded >= score - 6 * 500,
        "{score} then {faded}"
    );
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn a_repeat_order_gathers_until_its_count_is_used_up() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (id, actor) = player(&mut app);
    // A patch the played character can gather from bare-handed.
    let patches: Vec<(SimId, Entity, String)> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let patch = app.world().get::<ResourcePatch>(*entity)?;
            (patch.remaining >= 5).then(|| (*id, *entity, patch.kind.clone()))
        })
        .collect();
    let mut chosen = None;
    for (patch_id, patch, kind) in patches {
        beside(&mut app, actor, patch);
        let mut gather = VillageAct::new(id, "gather");
        gather.target = Some(patch_id);
        if gather.check(app.world()).is_ok() {
            chosen = Some((patch_id, patch, kind));
            break;
        }
    }
    let (patch_id, patch, kind) = chosen.expect("something to gather");
    let before = econ::count(app.world(), actor, &kind);
    let mut order = LiveAct::new(id, "repeat");
    order.target = Some(patch_id);
    order.quantity = 3;
    act(&mut app, &order);
    assert_eq!(
        view::repeat_order(app.world(), id).map(|order| order.1 + order.2),
        Some(3)
    );
    let done = until(&mut app, 3_000, 10, |app| {
        // The gatherer stays at the patch.
        if app.world().get::<MoveIntent>(actor).is_none() {
            let pos = *app.world().get::<SimPos>(patch).expect("patch");
            app.world_mut().entity_mut(actor).insert(pos);
        }
        view::repeat_order(app.world(), id).is_none()
    });
    assert!(
        done,
        "the order never ended: {:?}",
        view::repeat_order(app.world(), id)
    );
    run_ticks(app.world_mut(), 400);
    assert_eq!(econ::count(app.world(), actor, &kind), before + 3);
    let stopped = effects(app.world(), "VillageRepeatStopped");
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0]["reason"], "count_done");
    let so_far = view::day_so_far(app.world(), id).expect("ledger");
    let summary = view::day_summary(app.world(), id);
    let gathered = so_far.gathered.get(&kind).copied().unwrap_or(0)
        + summary.map_or(0, |day| day.gathered.get(&kind).copied().unwrap_or(0));
    assert_eq!(gathered, 3, "gathered goods are booked as gathered");
    // Out of reach, the order walks back to its patch, as a single gathering
    // would, and gathers there; nothing is gathered at a distance.
    let at = *app.world().get::<SimPos>(patch).expect("patch");
    let mut order = LiveAct::new(id, "repeat");
    order.target = Some(patch_id);
    order.quantity = 2;
    act(&mut app, &order);
    teleport(app.world_mut(), actor, SimPos::new(at.x + 8_000, at.z));
    let held = econ::count(app.world(), actor, &kind);
    let mut gathered_away = false;
    let walked = until(&mut app, 3_000, 5, |app| {
        if app.world().get::<village::Gathering>(actor).is_some() {
            let here = *app.world().get::<SimPos>(actor).expect("actor");
            gathered_away |= here.dist2(at) > 2_250_000;
            assert!(
                view::activity_progress(app.world(), actor)
                    .is_some_and(|(done, total)| { total > 0 && done <= total })
            );
        }
        view::repeat_order(app.world(), id).is_none()
    });
    assert!(walked, "{:?}", view::repeat_order(app.world(), id));
    assert!(!gathered_away, "a repeat gathered out of reach");
    run_ticks(app.world_mut(), 400);
    assert_eq!(econ::count(app.world(), actor, &kind), held + 2);
    let stopped = effects(app.world(), "VillageRepeatStopped");
    assert_eq!(stopped.len(), 2);
    assert_eq!(stopped[1]["reason"], "count_done", "{}", stopped[1]);
    // An order that cannot go on stops and says why.
    let mut order = LiveAct::new(id, "repeat");
    order.target = Some(patch_id);
    order.quantity = 2;
    act(&mut app, &order);
    app.world_mut()
        .get_mut::<ResourcePatch>(patch)
        .expect("patch")
        .remaining = 0;
    run_ticks(app.world_mut(), 40);
    assert_eq!(view::repeat_order(app.world(), id), None);
    let stopped = effects(app.world(), "VillageRepeatStopped");
    assert_eq!(stopped.len(), 3);
    assert_eq!(stopped[2]["reason"], "target_gone", "{}", stopped[2]);
}

#[test]
fn goals_unlock_homestead_tiers_paid_in_goods_and_work() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (id, actor) = player(&mut app);
    let goals = view::goals(app.world(), id);
    assert!(goals.done.is_empty());
    assert_eq!(
        goals.current.as_ref().map(|goal| goal.key.as_str()),
        Some("cottage")
    );
    assert_eq!(goals.total, 7);
    let tiers = view::homestead_tiers(app.world(), id);
    let keys: Vec<&str> = tiers.iter().map(|tier| tier.key.as_str()).collect();
    assert_eq!(
        keys,
        ["storage", "second_plot", "better_tools", "larger_cottage"]
    );
    assert!(tiers.iter().all(|tier| !tier.available && !tier.done));
    assert_eq!(tiers[0].reason, "build your cottage first");

    let (house, door) = settle(&mut app, id, actor);
    // The living-world cottage is the catalogue cottage: 4 x 5 m, two beds.
    let home = view::home(app.world(), house).expect("home");
    assert_eq!(home.capacity, 2);
    assert_eq!(
        (
            home.body_max.x - home.body_min.x,
            home.body_max.z - home.body_min.z
        ),
        (4_000, 5_000)
    );
    let goals = view::goals(app.world(), id);
    assert_eq!(goals.done.len(), 1);
    assert_eq!(goals.done[0].reward, vec!["a storage chest".to_owned()]);
    let current = goals.current.expect("next goal");
    assert_eq!((current.key.as_str(), current.count), ("storage", 1));
    // The storage tier is open; the second plot waits for it.
    let tiers = view::homestead_tiers(app.world(), id);
    assert!(tiers[0].available, "{}", tiers[0].reason);
    assert!(tiers[1].reason.contains("build a storage chest first"));
    // The upgrade is a site: started empty-handed, it waits for materials
    // and no work counts until they are in.
    teleport(app.world_mut(), actor, door);
    let mut upgrade = LiveAct::new(id, "upgrade");
    upgrade.kind = "storage".into();
    act(&mut app, &upgrade);
    let site = view::upgrade_site(app.world(), id).expect("site");
    assert!(!site.supplied);
    run_ticks(app.world_mut(), 100);
    assert_eq!(
        view::upgrade_site(app.world(), id)
            .expect("site")
            .remaining_ticks,
        400
    );
    // Carried to the door, the goods go in by themselves; work then counts
    // only at the cottage.
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    run_ticks(app.world_mut(), 2);
    assert!(view::upgrade_site(app.world(), id).expect("site").supplied);
    assert_eq!(econ::count(app.world(), actor, "plank"), 0);
    teleport(app.world_mut(), actor, SimPos::new(door.x + 30_000, door.z));
    run_ticks(app.world_mut(), 100);
    let paused = view::upgrade_site(app.world(), id).map(|site| site.remaining_ticks);
    teleport(app.world_mut(), actor, door);
    run_ticks(app.world_mut(), 100);
    let worked = view::upgrade_site(app.world(), id).map(|site| site.remaining_ticks);
    assert!(worked < paused, "{paused:?} then {worked:?}");
    let finished = until(&mut app, 1_200, 10, |app| {
        if app.world().get::<MoveIntent>(actor).is_none() {
            app.world_mut().entity_mut(actor).insert(door);
        }
        view::homestead_tiers(app.world(), id)[0].done
    });
    assert!(finished, "the upgrade was never finished");
    assert_eq!(
        view::chest(app.world(), id).map(|chest| chest.capacity),
        Some(40)
    );
    assert_eq!(effects(app.world(), "VillageHomesteadUpgraded").len(), 1);
    let goals = view::goals(app.world(), id);
    assert_eq!(goals.done.len(), 2);
    assert_eq!(
        goals.current.map(|goal| goal.key),
        Some("second_plot".to_owned())
    );
    // The tool shed shortens the owner's gathering (a tier, not a tool flag).
    assert_eq!(live::gather_percent(app.world(), actor), 100);
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn the_village_gathers_on_the_feast_day_and_mourns_its_dead() {
    let mut app = world_with(|world| {
        quiet(world);
        // One-day seasons from the first spring day: day 2 is autumn.
        economy(world)["tuning"]["days_per_season"] = json!(1);
        economy(world)["tuning"]["founding_day"] = json!(0);
        // With one-day seasons the whole autumn lies within the coat lead,
        // so buying a coat (a priority before winter) would compete with the
        // feast; the shipped feast day (autumn day 2 of 7) is before it.
        economy(world)["tuning"]["coat_lead_days"] = json!(0);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((old_id, old), (fond_id, fond)) = (people[0], people[1]);
    let calendar = view::gatherings(app.world());
    assert_eq!(calendar.len(), 2);
    assert_eq!(
        (calendar[0].key.as_str(), calendar[0].next_day),
        ("harvest_feast", 2)
    );
    assert!(!calendar[0].active && calendar[0].last_attendance.is_none());

    // Somebody dies of old age on the first morning; a neighbour was fond.
    let today = village::calendar(app.world()).0;
    {
        let mut life = app.world_mut().get_mut::<Life>(old).expect("life");
        life.death_day = today + 1;
    }
    for (_, actor) in &people[1..] {
        let mut life = app.world_mut().get_mut::<Life>(*actor).expect("life");
        life.born_day = today as i64 - 10;
        life.death_day = today + 1_000;
    }
    app.world_mut()
        .get_mut::<SocialLife>(fond)
        .expect("social")
        .affinity
        .insert(old_id, 30_000);
    // The dead is laid to rest on the second morning, and is mourned.
    run_ticks(app.world_mut(), 2 * DAY + 2);
    let grief = effects(app.world(), "VillageMourning");
    assert_eq!(grief.len(), 1);
    assert!(
        grief[0]["mourners"]
            .as_array()
            .is_some_and(|mourners| mourners.contains(&json!(fond_id)))
    );
    assert!(
        view::mourning(app.world())
            .iter()
            .any(|(mourner, _, _)| *mourner == fond_id)
    );
    // Day 2, the feast: from 0.56 to 0.70 of the day.
    run_ticks(app.world_mut(), DAY * 60 / 100);
    assert!(
        view::gatherings(app.world())
            .iter()
            .any(|feast| feast.active)
    );
    assert_eq!(effects(app.world(), "VillageGatheringBegan").len(), 1);
    run_ticks(app.world_mut(), DAY * 15 / 100);
    let held = effects(app.world(), "VillageGathering");
    assert_eq!(held.len(), 1);
    let attended = held[0]["attended"].as_u64().expect("attended");
    println!(
        "GATHERING attended={attended} of {}",
        all_residents(app.world()).len()
    );
    if attended < 20 {
        for (id, actor) in all_residents(app.world()) {
            println!(
                "FEAST_ABSENT {} label={:?} needs={:?} coat={}",
                id.0,
                view::activity_label(app.world(), actor),
                ["Hunger", "Thirst", "Energy", "Warmth"].map(|need| econ::need(
                    app.world(),
                    actor,
                    need
                ) / 1000),
                village::living::has_coat(app.world(), actor),
            );
        }
    }
    assert!(attended >= 20, "only {attended} came to the feast");
    let calendar = view::gatherings(app.world());
    let feast = calendar
        .iter()
        .find(|feast| feast.key == "harvest_feast")
        .expect("feast");
    assert_eq!(feast.last_attendance, Some(attended as u32));
    assert_eq!(feast.next_day, 6, "next year's feast");
    run_ticks(app.world_mut(), DAY * 4);
    assert!(view::mourning(app.world()).is_empty(), "grief passes");
    village::integrity::validate_colony(app.world()).expect("valid");
}

/// A scripted played life over four days: a cottage, a repeat order, a
/// neighbour befriended by talk, trade and gifts, an upgrade, and a night's
/// sleep each day. The village around it stays alive, housed and solvent.
#[test]
fn a_played_life_leaves_the_village_healthy() {
    let mut app = world_with(|world| {
        short_days(world, 6_000);
        only_weather(world, "clear");
    });
    set_journal(app.world_mut(), true, false);
    let day = 6_000_u64;
    let (id, actor) = player(&mut app);
    let founders = all_residents(app.world()).len();
    let (house, door) = settle(&mut app, id, actor);
    let total = coins(&app);
    let (friend_id, friend) = seller(&mut app, "bread");
    let total = total.max(coins(&app));
    let (liked, _) = view::gift_tastes(app.world(), friend_id).expect("tastes");
    econ::grant(app.world_mut(), actor, Item::new(liked.as_str()).stack(5));
    let mut nights = 0;
    for round in 0..4_u64 {
        // Morning: talk, buy bread, give a gift.
        beside(&mut app, actor, friend);
        let mut talk = VillageAct::new(id, "talk");
        talk.target = Some(friend_id);
        act(&mut app, &talk);
        beside(&mut app, actor, friend);
        let mut buy = VillageAct::new(id, "buy");
        buy.target = Some(friend_id);
        buy.kind = "bread".into();
        if buy.check(app.world()).is_ok() {
            act(&mut app, &buy);
        }
        beside(&mut app, actor, friend);
        let mut gift = LiveAct::new(id, "gift");
        gift.target = Some(friend_id);
        gift.kind = liked.clone();
        if gift.check(app.world()).is_ok() {
            act(&mut app, &gift);
        }
        // The storage chest is real work at the cottage.
        if round == 1 {
            econ::grant(app.world_mut(), actor, Item::new("plank").stack(4));
            econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
            teleport(app.world_mut(), actor, door);
            let mut upgrade = LiveAct::new(id, "upgrade");
            upgrade.kind = "storage".into();
            act(&mut app, &upgrade);
        }
        // The rest of the day at home; in the evening to bed.
        let now = app.world().resource::<SimClock>().tick;
        let evening = (now / day) * day + day * 65 / 100;
        while app.world().resource::<SimClock>().tick < evening {
            teleport(app.world_mut(), actor, door);
            run_ticks(app.world_mut(), 100);
        }
        teleport(app.world_mut(), actor, door);
        if LiveAct::new(id, "sleep").check(app.world()).is_ok() {
            act(&mut app, &LiveAct::new(id, "sleep"));
            nights += 1;
        }
        let morning = (app.world().resource::<SimClock>().tick / day + 1) * day;
        let left = morning - app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), left + 5);
        assert!(
            village::living(app.world(), actor),
            "the played character died"
        );
    }
    assert!(nights >= 3, "slept {nights} nights");
    let summary = view::day_summary(app.world(), id).expect("summary");
    assert!(summary.slept);
    assert!(live::score(app.world(), id, friend_id) >= 20_000);
    assert_eq!(view::home(app.world(), house).expect("home").capacity, 2);
    assert!(
        view::chest(app.world(), id).is_some(),
        "the chest was built"
    );
    assert!(view::goals(app.world(), id).done.len() >= 2);
    // The village around the played life.
    let people = all_residents(app.world());
    // Settlers may land by boat; nobody may die.
    assert!(people.len() >= founders, "nobody died or vanished");
    for (resident, body) in &people {
        assert!(
            village::living(app.world(), *body),
            "resident {} died",
            resident.0
        );
        assert!(colony::home_of(app.world(), *resident).is_some());
    }
    assert_eq!(
        coins(&app),
        total,
        "coins are conserved around the played life"
    );
    assert!(app.world().resource::<village::VillageState>().transactions > 20);
    village::integrity::validate_colony(app.world()).expect("valid after a played life");
}

/// The price floor in percent of the base price, with the control: without
/// it the same forced price stays at one coin.
#[test]
fn a_price_cannot_fall_below_its_floor() {
    let forced = |percent: u32| -> (u32, u32) {
        let mut app = world_with(|world| {
            quiet(world);
            economy(world)["tuning"]["price_floor_percent"] = json!(percent);
            // The floor alone: reservation prices and stock cover off.
            economy(world)["tuning"]["reservation_prices"] = json!(false);
            economy(world)["tuning"]["stock_cover_pricing"] = json!(false);
        });
        run_ticks(app.world_mut(), 2);
        let base = village::catalog::price_in(app.world(), "bread").expect("base price");
        app.world_mut()
            .resource_mut::<market::MarketState>()
            .commodities
            .get_mut("bread")
            .expect("bread is quoted")
            .reference_milli = 1_000;
        assert_eq!(market::unit_price(app.world(), "bread"), Some(1));
        run_ticks(app.world_mut(), 320);
        (
            market::unit_price(app.world(), "bread").expect("price"),
            base,
        )
    };
    let (with_floor, base) = forced(50);
    let (without, _) = forced(0);
    assert!(base >= 4, "bread has a base price worth halving: {base}");
    assert!(
        with_floor >= base / 2,
        "{with_floor} against a base of {base}"
    );
    assert!(without < base / 2, "the control stayed low: {without}");
}

/// A guest who rents a bed at night sleeps until morning; the beds are data.
#[test]
fn an_inn_guest_sleeps_the_night_and_the_beds_are_data() {
    let mut app = world_with(|world| {
        quiet(world);
        economy(world)["tuning"]["inn_guest_beds"] = json!(2);
    });
    run_ticks(app.world_mut(), DAY * 80 / 100);
    let inn = view::inns(app.world()).remove(0);
    assert_eq!((inn.capacity, inn.beds.len()), (2, 2));
    let people = residents(app.world(), inn.settlement.expect("settlement"));
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
    let guests: Vec<_> = people
        .iter()
        .copied()
        .filter(|(id, _)| *id != keeper_id)
        .take(3)
        .collect();
    let now = app.world().resource::<SimClock>().tick;
    let mut outcomes = Vec::new();
    for (id, actor) in &guests {
        teleport(app.world_mut(), *actor, inn.entrance);
        app.world_mut()
            .get_mut::<Wallet>(*actor)
            .expect("wallet")
            .coins = 50;
        let mut rest = VillageAct::new(*id, "rest");
        rest.target = Some(keeper_id);
        outcomes.push(rest.perform(app.world_mut()));
    }
    assert!(outcomes[0].is_ok() && outcomes[1].is_ok(), "{outcomes:?}");
    assert_eq!(outcomes[2], Err("the inn has no free bed".to_owned()));
    let full = view::inn(app.world(), inn.inn).expect("inn");
    assert_eq!(view::repose(app.world(), guests[0].1), Repose::Asleep);
    assert!(
        full.beds[0].until >= now + DAY / 10,
        "a night, not a nap: until {} at {now}",
        full.beds[0].until
    );
    assert!(full.beds[0].until <= (now / DAY + 1) * DAY + DAY / 20);
}

/// What the interface asks for: a day's use, who makes a good, and the
/// catalogue with a reason for every entry.
#[test]
fn daily_use_producers_and_the_catalogue_say_what_the_interface_needs() {
    let mut app = world_with(quiet);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour).len() as u32;
    let used = view::daily_use(app.world(), harbour);
    let water = used
        .iter()
        .find(|entry| entry.good == village::catalog::WATER)
        .expect("water");
    // Thirst falls by 3 a tick; a unit of water restores 90,000.
    assert_eq!(water.per_day, (people * 3 * DAY as u32).div_ceil(90_000));
    assert!(water.stock > 0 && water.days_left.is_some());
    let food: u32 = used
        .iter()
        .filter(|entry| entry.good != village::catalog::WATER)
        .map(|entry| entry.per_day)
        .sum();
    assert!(food >= 1, "{used:?}");
    assert!(used.iter().any(|entry| entry.good == "bread"));

    let bakers = view::producers(app.world(), "bread");
    assert!(!bakers.is_empty());
    assert!(
        bakers
            .iter()
            .all(|maker| maker.role == "baker" && !maker.name.is_empty())
    );
    assert!(bakers.iter().any(|maker| maker.settlement == Some(harbour)));
    assert!(view::producers(app.world(), "no such good").is_empty());

    let catalogue = view::catalogue(app.world());
    assert_eq!(catalogue.len(), village::civic::catalog(app.world()).len());
    assert!(
        catalogue
            .iter()
            .all(|entry| !entry.reason.is_empty() && !entry.display.is_empty())
    );
    assert!(catalogue.iter().any(|entry| entry.available));
    // With no levy and an empty fund nothing that costs can be paid for.
    let poor = world_with(|world| {
        quiet(world);
        economy(world)["tuning"]["building_levy_percent"] = json!(0);
    });
    assert!(
        view::catalogue(poor.world())
            .iter()
            .filter(|entry| entry.estimated_cost > 0)
            .all(|entry| !entry.available && entry.reason.contains("can raise only 0 more"))
    );
    serde_json::to_value(&catalogue).expect("plain data");
}

/// Facts the inspector and the Live HUD read: an age to print, the year's
/// census, the goal step, the pack, the tool, and why a need is low.
#[test]
fn the_hud_accessors_report_age_pack_tool_goal_step_and_low_needs() {
    let mut app = world_with(quiet);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    // Founders are adults of a plausible age: 18 to 65, never "6 years old".
    for (id, _) in &people {
        let age = view::age(app.world(), *id).expect("age");
        assert_eq!(age.stage, "adult");
        assert!((18..65).contains(&age.years), "{age:?}");
        assert_eq!(age.label, format!("Adult · {}", age.years));
        assert_eq!(age.calendar_years, age.seasons / 4);
    }
    let (id, actor) = player(&mut app);
    assert_eq!(
        view::age(app.world(), id),
        None,
        "a played character has no birth on record"
    );
    let goals = view::goals(app.world(), id);
    assert_eq!((goals.step, goals.total), (1, 7));
    let pack = view::pack(app.world(), actor).expect("pack");
    let before = pack.weight_g;
    assert!(pack.max_weight_g > 0 && before <= pack.max_weight_g);
    econ::grant(app.world_mut(), actor, Item::new("bread").stack(2));
    assert!(view::pack(app.world(), actor).expect("pack").weight_g > before);
    // A woodcutter carries its axe.
    let cutter = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role.name() == "woodcutter")
        })
        .expect("a woodcutter");
    let tool = view::held_tool(app.world(), cutter.1).expect("an axe");
    assert_eq!(tool.0, "axe");
    assert!(tool.1 > 0);
    assert_eq!(view::held_tool(app.world(), actor), None);
    // Why a need is low.
    assert_eq!(
        view::need_reason(app.world(), actor, "Hunger"),
        None,
        "not low"
    );
    let (_, hungry) = people[3];
    for kind in [
        "bread",
        village::catalog::COOKED_MEAL,
        village::catalog::BERRIES,
    ] {
        let held = econ::count(app.world(), hungry, kind);
        if held > 0 {
            econ::consume(app.world_mut(), hungry, kind, held).expect("emptied");
        }
    }
    let needs = app
        .world()
        .resource::<npc_framework::needs::NeedRegistry>()
        .id("Hunger")
        .expect("need");
    app.world_mut()
        .get_mut::<npc_framework::needs::Needs>(hungry)
        .expect("needs")
        .get_mut(needs)
        .value = 30_000;
    let reason = view::need_reason(app.world(), hungry, "Hunger").expect("a reason");
    assert!(reason.starts_with("No food in the pack"), "{reason}");
    econ::grant(app.world_mut(), hungry, Item::new("bread"));
    assert_eq!(
        view::need_reason(app.world(), hungry, "Hunger").as_deref(),
        Some("Food is in the pack; it has not eaten yet.")
    );
    // The census of the year: a death is booked in its year.
    assert!(view::yearly_census(app.world()).is_empty());
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .get_mut::<Life>(people[0].1)
        .expect("life")
        .death_day = today + 1;
    run_ticks(app.world_mut(), DAY + 2);
    let years = view::yearly_census(app.world());
    assert_eq!(years.len(), 1);
    assert_eq!((years[0].births, years[0].deaths), (0, 1));
    // Asleep: progress through the night is known.
    // Whoever is asleep now, in the evening (somebody asleep earlier may
    // have woken since).
    run_ticks(app.world_mut(), DAY * 80 / 100 - 2);
    let sleeper = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| view::repose(app.world(), *actor) == Repose::Asleep);
    let (_, sleeper) = sleeper.expect("somebody asleep in the evening");
    let (done, total) = view::activity_progress(app.world(), sleeper).expect("progress");
    assert!(total > 0 && done <= total);
}

/// What the interface found missing: tastes learned per neighbour, the tool
/// the current activity puts in the hand, every catalogue reason, the
/// played character's craft progress and the real cottage body in the
/// homestead observation.
#[test]
fn the_interface_learns_tastes_tools_catalogue_reasons_and_the_cottage_body() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (id, actor) = player(&mut app);
    // Tastes: unknown to a stranger, learned with the first gift.
    let (resident, body) = seller(&mut app, "bread");
    let relation = |app: &App| {
        view::relationships(app.world(), id)
            .into_iter()
            .find(|relation| relation.other == resident)
    };
    beside(&mut app, actor, body);
    econ::grant(app.world_mut(), actor, Item::new("bread").stack(1));
    let mut gift = LiveAct::new(id, "gift");
    gift.target = Some(resident);
    gift.kind = "bread".into();
    act(&mut app, &gift);
    let known = relation(&app).expect("a bond after the gift");
    assert!(known.tastes_known);
    assert_eq!(known.tastes, view::gift_tastes(app.world(), resident));
    let other = all_residents(app.world())
        .into_iter()
        .find(|(other, _)| *other != resident)
        .expect("another resident")
        .0;
    assert!(
        view::relationships(app.world(), id)
            .iter()
            .filter(|relation| relation.other == other)
            .all(|relation| !relation.tastes_known && relation.tastes.is_none())
    );

    // The hand: gathering logs with an axe puts the axe there, with the
    // homestead tool tier once built.
    let log = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find_map(|(patch_id, entity)| {
            let patch = app.world().get::<ResourcePatch>(*entity)?;
            (patch.kind == "log" && patch.remaining > 2).then_some((*patch_id, *entity))
        })
        .expect("a wood");
    village::living::grant_tool(app.world_mut(), actor, "axe");
    beside(&mut app, actor, log.1);
    let mut gather = VillageAct::new(id, "gather");
    gather.target = Some(log.0);
    act(&mut app, &gather);
    let held = view::held(app.world(), actor).expect("a tool for the gathering");
    assert_eq!(
        (held.tool.as_str(), held.activity, held.carried),
        ("axe", "gather", true)
    );
    assert_eq!(held.tier, None);
    assert_eq!(
        view::held_tool(app.world(), actor).map(|(tool, _)| tool),
        Some("axe".to_owned())
    );
    let (done, total) = view::activity_progress(app.world(), actor).expect("progress");
    assert!(total > 0 && done <= total, "{done}/{total}");
    app.world_mut()
        .resource_mut::<live::LiveState>()
        .tiers
        .entry(id)
        .or_default()
        .extend(["storage", "second_plot", "better_tools"].map(String::from));
    assert_eq!(
        view::held(app.world(), actor).and_then(|held| held.tier),
        Some("better_tools".to_owned())
    );

    // The catalogue: the money first, then every other sentence that holds.
    for entry in view::catalogue(app.world()) {
        assert!(!entry.reasons.is_empty(), "{entry:?}");
        assert!(entry.reasons.iter().all(|reason| reason.ends_with('.')));
        assert!(entry.reasons.contains(&entry.reason), "{entry:?}");
    }
    let house = view::catalogue(app.world())
        .into_iter()
        .find(|entry| entry.building == "house")
        .expect("the family house");
    assert!(
        house
            .reasons
            .iter()
            .any(|reason| reason.contains("homestead tier")),
        "{house:?}"
    );

    // The homestead observation reports the catalogue cottage body.
    let (plot_id, plot) = homestead::plots(app.world())
        .into_iter()
        .find(|(_, plot)| {
            homestead::definition(app.world(), *plot).is_some_and(|def| def.use_kind == "housing")
        })
        .expect("a housing plot");
    let observed = homestead::plot_json(app.world(), plot, true).expect("observation");
    let centre = *app.world().get::<SimPos>(plot).expect("plot");
    let (body, _, _) = homestead::cottage_body(app.world(), homestead::front_door(centre), None);
    assert_eq!(
        observed["cottage_center"],
        json!(body),
        "plot {}",
        plot_id.0
    );
    assert!(observed["cottage_half_extents_mm"].is_array());
}

/// Fishing grounds lie at the water's edge; a fisher fishes there in its
/// work hours, and the view reads "fishing".
#[test]
fn fishers_fish_at_the_water_and_say_so() {
    let mut app = world_with(quiet);
    let grounds: Vec<(SimId, SimPos)> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let patch = app.world().get::<ResourcePatch>(*entity)?;
            (patch.kind == village::catalog::FISH)
                .then(|| (*id, *app.world().get::<SimPos>(*entity).expect("position")))
        })
        .collect();
    assert_eq!(grounds.len(), 3, "one fishing ground a settlement");
    for (_, at) in &grounds {
        let (x, z) = (at.x.div_euclid(1000), at.z.div_euclid(1000));
        let wet = (-1..=1).any(|dz| {
            (-1..=1).any(|dx| {
                village::frontier::terrain::cell(app.world(), x + dx, z + dz)
                    .is_some_and(|cell| !cell.walkable())
            })
        });
        assert!(wet, "the fishing ground at {at:?} is not at the water");
    }
    // A fisher of the founding, at its ground with its rod, fishes; while it
    // does the view reads "fishing" and the hand holds the rod. (In the
    // shipped calendar fishers start once the founding fish is used up,
    // after about two and a half days: `regional_economy_soak`.)
    let (fisher, body) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == village::Role::Fisher)
        })
        .expect("a fisher");
    let (ground, at) = grounds
        .iter()
        .min_by_key(|(_, at)| {
            app.world()
                .get::<SimPos>(body)
                .map_or(i64::MAX, |here| here.dist2(*at))
        })
        .copied()
        .expect("a ground");
    teleport(app.world_mut(), body, at);
    let mut gather = VillageAct::new(fisher, "gather");
    gather.target = Some(ground);
    gather.perform(app.world_mut()).expect("fishing");
    run_ticks(app.world_mut(), 20);
    assert!(app.world().get::<village::Gathering>(body).is_some());
    assert_eq!(view::activity_label(app.world(), body), Some("fishing"));
    assert_eq!(
        view::held(app.world(), body).map(|held| (held.tool, held.carried)),
        Some((village::catalog::FISHING_ROD.to_owned(), true))
    );
    let before = econ::count(app.world(), body, village::catalog::FISH);
    assert!(until(&mut app, 600, 10, |app| {
        econ::count(app.world(), body, village::catalog::FISH) > before
    }));
}
