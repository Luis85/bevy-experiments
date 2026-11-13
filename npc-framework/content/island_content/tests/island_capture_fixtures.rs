//! Prepared island saves for native interface captures. Each test builds the
//! shipped living world exactly as the host's New game does, plays it
//! headlessly to a moment the interface must show, and writes a save that
//! `island_game --front-end --island-save PATH` continues from.
//!
//! These are `#[ignore]`d generators, not checks of behaviour. Regenerate:
//!
//! ```sh
//! cargo test --locked -p island_content --test island_capture_fixtures -- --ignored --test-threads 1
//! ```
//!
//! Output: `target/npc-out/fixtures/{cottage-evening,upgrade-underway,hard-times,village-life,larger-cottage-done}.json`.
//! The host saves into the file it opened (every minute and on exit), so a
//! capture should copy a fixture somewhere else before launching on it.
//!
//! World changes are recorded commands (`HomesteadAct`, `LiveAct`,
//! `VillageAct`, `JoinIsland`) wherever one exists. Like the live-mode tests,
//! the set-up moves the played character (`teleport`), grants materials, and
//! moves coins between holders (none are made or lost). One bond score and
//! one need are written directly; each place says why.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, VillageAct, Wallet,
    colony::{self, JoinIsland},
    economy as econ,
    homestead::{self, HomesteadAct, HomesteadPlot},
    hunting::Wildlife,
    live::{LiveAct, LiveState},
    merchant::Merchant,
    view,
};
use life_support::{all_residents, entity, teleport, until};
use npc_framework::agent_io::scenario::{ScenarioDoc, attach_controllers};
use npc_framework::interaction::Item;
use npc_framework::jobs::Reservations;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use npc_framework::snapshot;
use std::path::PathBuf;

const SEED: u64 = 7;
const NAME: &str = "Tamsin";

fn fixtures() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("npc-out")
        .join("fixtures");
    std::fs::create_dir_all(&dir).expect("fixtures folder");
    dir
}

/// The host's New game construction for the shipped living world, with the
/// character named (`island::construction_from_preset` and
/// `session::named_construction` in `apps/island_game`).
fn construction() -> ScenarioDoc {
    let preset = island_content::worlds::builtin_preset("living_world").expect("shipped");
    let mut doc = island_content::scenario::scenario_doc("island_colony").expect("scenario");
    let region = preset.living_world.as_ref().expect("living world");
    let actor = doc.actors.first_mut().expect("arrival actor");
    actor.pos = Some((region.actor_start.x, region.actor_start.z));
    actor.extra.insert("name".into(), NAME.into());
    island_content::construction::ExplorerContext::for_world(
        "human",
        "human",
        &island_content::actors::human_preset(),
        "living_world",
        &preset,
    )
    .write_to(&mut doc)
    .expect("context");
    doc.world["explorer_context"]["actor"] = NAME.into();
    doc
}

/// The founding animals are construction identities: a save that lost one
/// to a hunt does not continue (`integrity::validate_snapshot_structure`,
/// reported to the simulation's owners). While a fixture is played, each is
/// claimed for the played character, so hunters leave them be; `unguard`
/// gives the claims back before the save is written.
fn guard(app: &mut App, by: SimId) -> Vec<SimId> {
    let mut animals: Vec<SimId> = app
        .world_mut()
        .query_filtered::<&SimId, With<Wildlife>>()
        .iter(app.world())
        .copied()
        .collect();
    animals.sort_unstable();
    for animal in &animals {
        assert!(npc_framework::jobs::reserve(
            app.world_mut(),
            by,
            *animal,
            1
        ));
    }
    animals
}

fn unguard(app: &mut App, by: SimId, animals: &[SimId]) {
    let mut claims = app.world_mut().resource_mut::<Reservations>();
    for animal in animals {
        claims.release(*animal, by);
    }
}

fn open(doc: &ScenarioDoc) -> App {
    let mut loaded = island_content::scenario::load_with(doc, SEED, |_| {}).expect("loads");
    attach_controllers(doc, &mut loaded).expect("controllers");
    loaded.app
}

/// Writes the save with the host's header, then reopens it the way the
/// host's title screen does, so a fixture that would not continue fails here.
fn write(app: &mut App, doc: &ScenarioDoc, name: &str) -> PathBuf {
    village::integrity::validate_colony(app.world()).expect("valid colony");
    // Stamped like every island save of this build.
    let mut snap = village::island_save::save_island(app.world_mut()).expect("snapshot");
    snap.header.insert("local_island".into(), 1.into());
    snap.header
        .insert("scenario".into(), serde_json::to_value(doc).expect("doc"));
    snap.header.insert("seed".into(), SEED.into());
    let path = fixtures().join(format!("{name}.json"));
    snapshot::write_atomic(&path, &snap).expect("written");

    let saved = snapshot::read(&path).expect("readable");
    let mut fresh = open(doc);
    let expected = snapshot::save(fresh.world_mut()).expect("baseline");
    village::integrity::validate_snapshot_structure(&expected, &saved).expect("structure");
    let report =
        snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    village::integrity::validate_colony(fresh.world()).expect("valid after restore");
    path
}

/// A recorded command, applied on the next tick.
fn act<C: ContentCommand>(app: &mut App, command: &C) {
    submit(app.world_mut(), None, SimCommand::content(command)).expect("accepted");
    run_ticks(app.world_mut(), 1);
}

fn joined(app: &mut App) -> (SimId, Entity) {
    let id = app.world().resource::<village::VillageState>().played;
    act(app, &JoinIsland { unit: id.0 });
    let actor = entity(app.world(), id);
    assert!(village::living(app.world(), actor), "the visitor joined");
    (id, actor)
}

/// Claims the free housing plot nearest the arrival and builds the cottage
/// with granted materials; the work itself runs tick by tick.
fn settle(app: &mut App, id: SimId, actor: Entity) -> SimPos {
    let start = *app.world().get::<SimPos>(actor).expect("position");
    let (plot_id, plot) = homestead::plots(app.world())
        .into_iter()
        .filter(|(_, plot)| {
            homestead::definition(app.world(), *plot).is_some_and(|def| def.use_kind == "housing")
                && app
                    .world()
                    .get::<HomesteadPlot>(*plot)
                    .is_some_and(|estate| estate.owner.is_none())
        })
        .min_by_key(|(plot_id, plot)| {
            (
                app.world().get::<SimPos>(*plot).expect("plot").dist2(start),
                *plot_id,
            )
        })
        .expect("a free housing plot");
    let centre = *app.world().get::<SimPos>(plot).expect("plot");
    let door = homestead::front_door(centre);
    teleport(app.world_mut(), actor, door);
    act(app, &HomesteadAct::new(id, "claim_plot", plot_id));
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(8));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(4));
    teleport(app.world_mut(), actor, door);
    act(app, &HomesteadAct::new(id, "build_cottage", plot_id));
    let built = until(app, 6_000, 20, |app| {
        app.world()
            .get::<HomesteadPlot>(plot)
            .is_some_and(|estate| estate.cottage_built)
    });
    assert!(built, "the cottage was never finished");
    colony::home_of(app.world(), id).expect("the cottage is home");
    door
}

/// Living residents nearest `at`, by distance then identity.
fn neighbours(app: &App, at: SimPos, count: usize, played: SimId) -> Vec<(SimId, Entity)> {
    let mut people: Vec<_> = all_residents(app.world())
        .into_iter()
        .filter(|(id, actor)| *id != played && village::living(app.world(), *actor))
        .map(|(id, actor)| {
            let pos = *app.world().get::<SimPos>(actor).expect("position");
            (pos.dist2(at), id, actor)
        })
        .collect();
    people.sort();
    people
        .into_iter()
        .take(count)
        .map(|(_, id, actor)| (id, actor))
        .collect()
}

/// Talks to three neighbours and gives each a liked good (recorded
/// commands), so the People tab knows them and their tastes. The first
/// becomes a friend who helps: friendship takes several days of gifts and
/// talk, which a fixture does not wait for, so that one score is raised to
/// the friend tier directly.
fn befriend(app: &mut App, id: SimId, actor: Entity, door: SimPos) -> Vec<SimId> {
    let mut known = Vec::new();
    for (index, (other_id, other)) in neighbours(app, door, 3, id).into_iter().enumerate() {
        let (liked, _) = view::gift_tastes(app.world(), other_id).expect("tastes");
        econ::grant(app.world_mut(), actor, Item::new(liked.as_str()).stack(1));
        let pos = *app.world().get::<SimPos>(other).expect("position");
        teleport(app.world_mut(), actor, pos);
        let mut talk = VillageAct::new(id, "talk");
        talk.target = Some(other_id);
        act(app, &talk);
        let pos = *app.world().get::<SimPos>(other).expect("position");
        teleport(app.world_mut(), actor, pos);
        let mut gift = LiveAct::new(id, "gift");
        gift.target = Some(other_id);
        gift.kind = liked;
        gift.check(app.world()).expect("gift possible");
        act(app, &gift);
        if index == 0 {
            let mut state = app.world_mut().resource_mut::<LiveState>();
            let bond = state
                .bonds
                .get_mut(&id)
                .and_then(|bonds| bonds.get_mut(&other_id))
                .expect("bond");
            bond.score = bond.score.max(22_000);
        }
        known.push(other_id);
    }
    teleport(app.world_mut(), actor, door);
    assert!(view::relationships(app.world(), id).len() >= 3);
    known
}

/// Runs to `permille` of the current day (or the next day if it is past),
/// keeping the played character at `door` and awake.
fn run_to(app: &mut App, actor: Entity, door: SimPos, permille: u64) {
    let day = app.world().resource::<SimConfig>().day_length.max(1);
    let now = app.world().resource::<SimClock>().tick;
    let mut target = now / day * day + day * permille / 1000;
    if target <= now {
        target += day;
    }
    while app.world().resource::<SimClock>().tick < target {
        teleport(app.world_mut(), actor, door);
        let left = target - app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), left.min(500));
        fed(app, actor);
    }
    teleport(app.world_mut(), actor, door);
    run_ticks(app.world_mut(), 1);
    teleport(app.world_mut(), actor, door);
}

/// Nobody plays the character while a fixture runs, so nobody feeds them:
/// over a long run their needs are kept above half, as if they had eaten,
/// drunk and slept (a fixture shortcut; the saves show a life, not a fast).
fn fed(app: &mut App, actor: Entity) {
    let ids: Vec<_> = ["Hunger", "Thirst", "Energy", "Warmth"]
        .into_iter()
        .filter_map(|name| app.world().resource::<NeedRegistry>().id(name))
        .collect();
    if let Some(mut needs) = app.world_mut().get_mut::<Needs>(actor) {
        for id in ids {
            let need = &mut needs.values[id.0 as usize];
            need.value = need.value.max(60_000);
        }
    }
}

/// Bread, a second giftable good and some coins in the pouch. The coins are
/// lent by the richest neighbour: they move, none are made.
fn stock_pouch(app: &mut App, actor: Entity) {
    econ::grant(app.world_mut(), actor, Item::new("bread").stack(3));
    econ::grant(
        app.world_mut(),
        actor,
        Item::new("village_berries").stack(4),
    );
    let held = app
        .world()
        .get::<Wallet>(actor)
        .map_or(0, |wallet| wallet.coins);
    let lender = all_residents(app.world())
        .into_iter()
        .filter(|(_, body)| *body != actor && village::living(app.world(), *body))
        .max_by_key(|(id, body)| {
            (
                app.world()
                    .get::<Wallet>(*body)
                    .map_or(0, |wallet| wallet.coins),
                std::cmp::Reverse(*id),
            )
        })
        .map(|(_, body)| body)
        .expect("a neighbour with coins");
    let lent = {
        let mut wallet = app.world_mut().get_mut::<Wallet>(lender).expect("wallet");
        let lent = 45_u32.saturating_sub(held).min(wallet.coins / 2);
        wallet.coins -= lent;
        lent
    };
    app.world_mut()
        .get_mut::<Wallet>(actor)
        .expect("wallet")
        .coins += lent;
    let coins = app
        .world()
        .get::<Wallet>(actor)
        .map_or(0, |wallet| wallet.coins);
    assert!(coins >= 10, "the played character has some coins: {coins}");
}

/// Upgrades one homestead tier at the door with granted materials; the work
/// runs tick by tick.
fn upgrade(app: &mut App, id: SimId, actor: Entity, door: SimPos, tier: &str) {
    let spec = view::homestead_tiers(app.world(), id);
    let index = spec.iter().position(|t| t.key == tier).expect("tier");
    for (good, count) in tier_cost(tier) {
        econ::grant(app.world_mut(), actor, Item::new(good).stack(count));
    }
    teleport(app.world_mut(), actor, door);
    let mut start = LiveAct::new(id, "upgrade");
    start.kind = tier.into();
    start.check(app.world()).expect("upgrade possible");
    act(app, &start);
    let done = until(app, 4_000, 20, |app| {
        if app.world().get::<MoveIntent>(actor).is_none() {
            teleport(app.world_mut(), actor, door);
        }
        view::homestead_tiers(app.world(), id)[index].done
    });
    assert!(done, "{tier} was never finished");
}

/// The shipped tier costs (living_world.json `live.homestead`).
fn tier_cost(tier: &str) -> Vec<(&'static str, u32)> {
    match tier {
        "storage" => vec![("plank", 4), ("log", 2)],
        "second_plot" => vec![("plank", 4), ("log", 4)],
        "better_tools" => vec![("plank", 4), ("ingot", 1)],
        "larger_cottage" => vec![("plank", 6), ("log", 4)],
        _ => unreachable!(),
    }
}

struct Life {
    doc: ScenarioDoc,
    app: App,
    id: SimId,
    actor: Entity,
    door: SimPos,
    guarded: Vec<SimId>,
}

fn cottage_life() -> Life {
    let doc = construction();
    let mut app = open(&doc);
    let (id, actor) = joined(&mut app);
    let guarded = guard(&mut app, id);
    let door = settle(&mut app, id, actor);
    Life {
        doc,
        app,
        id,
        actor,
        door,
        guarded,
    }
}

#[test]
#[ignore = "writes target/npc-out/fixtures/cottage-evening.json"]
fn cottage_evening() {
    let Life {
        doc,
        mut app,
        id,
        actor,
        door,
        guarded,
    } = cottage_life();
    // Late afternoon: the neighbours are about.
    run_to(&mut app, actor, door, 450);
    befriend(&mut app, id, actor, door);
    // Surplus offered for sale (a recorded VillageAct offer_goods, as the
    // Trade tab does): a resident who wants it walks over and buys it during
    // the afternoon, so the morning's account has a Sold row. The goods are
    // granted.
    for (good, count) in [("bread", 6), ("village_berries", 6), ("log", 4)] {
        econ::grant(app.world_mut(), actor, Item::new(good).stack(count));
        let mut offer = VillageAct::new(id, "offer_goods");
        offer.kind = good.into();
        offer.quantity = count / 2;
        if offer.check(app.world()).is_ok() {
            act(&mut app, &offer);
        }
    }
    run_to(&mut app, actor, door, 602);
    stock_pouch(&mut app, actor);
    // A loaf bought from a resident who sells bread (a recorded VillageAct
    // buy), so the morning's account has a Bought row. The seller's stock
    // is granted, as the live-mode tests do.
    let (seller_id, seller) = all_residents(app.world())
        .into_iter()
        .filter(|(other, body)| *other != id && village::living(app.world(), *body))
        .find(|(_, body)| village::market::sells(app.world(), *body, "bread"))
        .expect("somebody sells bread");
    econ::grant(app.world_mut(), seller, Item::new("bread").stack(4));
    let at = *app.world().get::<SimPos>(seller).expect("position");
    teleport(app.world_mut(), actor, at);
    let mut buy = VillageAct::new(id, "buy");
    buy.target = Some(seller_id);
    buy.kind = "bread".into();
    buy.check(app.world()).expect("a loaf can be bought");
    act(&mut app, &buy);
    let mut buyers: Vec<(SimId, Entity)> = all_residents(app.world())
        .into_iter()
        .filter(|(other, body)| *other != id && village::living(app.world(), *body))
        .collect();
    buyers.sort_by_key(|(other, _)| *other);
    // Sold already from the listing, or else to a resident with an open
    // demand (a recorded VillageAct sell).
    let mut sold =
        village::live::day_so_far(app.world(), id).is_some_and(|day| !day.sold.is_empty());
    let mut refused: Vec<String> = Vec::new();
    if !sold {
        'buyers: for (buyer_id, buyer) in &buyers {
            for good in ["log", "village_fish", "village_berries", "bread"] {
                let at = *app.world().get::<SimPos>(*buyer).expect("position");
                teleport(app.world_mut(), actor, at);
                let mut sell = VillageAct::new(id, "sell");
                sell.target = Some(*buyer_id);
                sell.kind = good.into();
                match sell.check(app.world()) {
                    Ok(()) => {
                        act(&mut app, &sell);
                        sold = true;
                        break 'buyers;
                    }
                    Err(reason) if refused.len() < 12 => {
                        refused.push(format!("{buyer_id:?} {good}: {reason}"));
                    }
                    Err(_) => {}
                }
            }
        }
    }
    // No resident had an open demand in the shipped world that afternoon:
    // the save is written without a sale and says why (reported to the
    // simulation's owners).
    if !sold {
        eprintln!("cottage-evening: nothing sold: {refused:?}");
    }
    teleport(app.world_mut(), actor, door);
    assert!(
        village::live::day_so_far(app.world(), id)
            .is_some_and(|day| !day.bought.is_empty() && (!sold || !day.sold.is_empty())),
        "the purchase (and any sale) are in the day's account"
    );
    teleport(app.world_mut(), actor, door);
    LiveAct::new(id, "sleep")
        .check(app.world())
        .expect("sleep is offered at the door");
    unguard(&mut app, id, &guarded);
    let path = write(&mut app, &doc, "cottage-evening");
    eprintln!("wrote {}", path.display());
}

#[test]
#[ignore = "writes target/npc-out/fixtures/upgrade-underway.json"]
fn upgrade_underway() {
    upgrade_save(false, "upgrade-underway");
}

/// The same moment with the played character still at the door: the
/// builder joins the owner's work there (both paces add).
#[test]
#[ignore = "writes target/npc-out/fixtures/upgrade-underway-at-door.json"]
fn upgrade_underway_at_door() {
    upgrade_save(true, "upgrade-underway-at-door");
}

/// The larger cottage half carried in, the rest ordered and a builder
/// hired by the physical flow; saved at once. `at_door` keeps the played
/// character at the door; otherwise they step 7 m away.
fn upgrade_save(at_door: bool, name: &str) {
    let Life {
        doc,
        mut app,
        id,
        actor,
        door,
        guarded,
    } = cottage_life();
    for tier in ["storage", "second_plot", "better_tools"] {
        upgrade(&mut app, id, actor, door, tier);
    }
    // Something in the chest.
    econ::grant(app.world_mut(), actor, Item::new("village_fish").stack(3));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    for (good, quantity) in [("village_fish", 3), ("log", 2)] {
        teleport(app.world_mut(), actor, door);
        let mut store = LiveAct::new(id, "store");
        store.kind = good.into();
        store.quantity = quantity;
        act(&mut app, &store);
    }
    assert!(
        view::chest(app.world(), id).is_some_and(|chest| !chest.goods.is_empty()),
        "the chest holds something"
    );
    run_to(&mut app, actor, door, 250);
    befriend(&mut app, id, actor, door);
    // The larger cottage, started with half its planks and logs carried to
    // the door; the rest ordered from residents and a builder hired, by the
    // physical flow (the recorded LiveAct verbs the upgrade card submits).
    // The save is written at once: nobody has taken the order yet.
    econ::grant(app.world_mut(), actor, Item::new("plank").stack(3));
    econ::grant(app.world_mut(), actor, Item::new("log").stack(2));
    lend(&mut app, actor, 400);
    teleport(app.world_mut(), actor, door);
    let mut start = LiveAct::new(id, "upgrade");
    start.kind = "larger_cottage".into();
    start.check(app.world()).expect("larger cottage possible");
    act(&mut app, &start);
    let site = view::upgrade_site(app.world(), id).expect("site");
    assert!(!site.supplied, "{site:?}");
    let delivered = |good: &str| {
        site.delivered
            .iter()
            .find(|(kind, _)| kind == good)
            .map_or(0, |(_, count)| *count)
    };
    for (good, count) in &site.cost {
        let missing = count.saturating_sub(delivered(good));
        if missing == 0 {
            continue;
        }
        let mut order = LiveAct::new(id, "order");
        order.kind = good.clone();
        order.quantity = missing;
        order.check(app.world()).expect("order possible");
        act(&mut app, &order);
    }
    let hire = LiveAct::new(id, "hire");
    hire.check(app.world()).expect("hire possible");
    act(&mut app, &hire);
    stock_pouch(&mut app, actor);
    // Then the played character steps away from the door (unless this save
    // is the one where they work there beside the builder).
    if !at_door {
        teleport(app.world_mut(), actor, SimPos::new(door.x + 7_000, door.z));
    }
    run_ticks(app.world_mut(), 1);
    let site = view::upgrade_site(app.world(), id).expect("site");
    assert!(!site.supplied, "materials still to come: {site:?}");
    assert!(
        site.ordered.iter().any(|(_, count)| *count > 0),
        "the rest is ordered: {site:?}"
    );
    assert!(site.builder.is_some(), "a builder is hired: {site:?}");
    assert!(
        !site
            .workers
            .iter()
            .any(|worker| matches!(worker.state, "fetching" | "bringing")),
        "nobody has taken the order yet: {:?}",
        site.workers
    );
    unguard(&mut app, id, &guarded);
    let path = write(&mut app, &doc, name);
    eprintln!("wrote {}", path.display());
}

#[test]
#[ignore = "writes target/npc-out/fixtures/hard-times.json"]
fn hard_times() {
    let doc = construction();
    let mut app = open(&doc);
    let (id, actor) = joined(&mut app);
    let guarded = guard(&mut app, id);
    // Mid-morning of the first day, so the capture is in daylight.
    run_ticks(app.world_mut(), 8_400);
    unguard(&mut app, id, &guarded);
    // Hard times, as moved coins: the played character keeps three, the
    // civic fund and the richest residents' surplus go to the merchant's
    // purse until the cheapest building is beyond the fund and the levy.
    let mut taken = 0_u32;
    {
        let mut wallet = app.world_mut().get_mut::<Wallet>(actor).expect("wallet");
        taken += wallet.coins.saturating_sub(3);
        wallet.coins = wallet.coins.min(3);
    }
    {
        let mut state = app.world_mut().resource_mut::<village::VillageState>();
        taken += state.fund;
        state.fund = 0;
    }
    let cheapest = view::catalogue(app.world())
        .iter()
        .map(|entry| entry.estimated_cost)
        .min()
        .expect("a catalogue");
    let mut rich: Vec<_> = all_residents(app.world())
        .into_iter()
        .filter(|(other, _)| *other != id)
        .map(|(other, body)| {
            let coins = app.world().get::<Wallet>(body).map_or(0, |w| w.coins);
            (std::cmp::Reverse(coins), other, body)
        })
        .collect();
    rich.sort();
    for (_, _, body) in rich {
        if village::civic::levy_capacity(app.world()) < cheapest {
            break;
        }
        let mut wallet = app.world_mut().get_mut::<Wallet>(body).expect("wallet");
        // 40 is the shipped destitution floor; a share of 19 above it is 0.
        let keep = wallet.coins.min(59);
        taken += wallet.coins - keep;
        wallet.coins = keep;
    }
    app.world_mut().resource_mut::<Merchant>().purse += taken;
    // A need left critical. Going hungry takes days of play; the fixture
    // writes the level a long fast would reach.
    let hunger = app
        .world()
        .resource::<NeedRegistry>()
        .id("Hunger")
        .expect("hunger");
    app.world_mut()
        .get_mut::<Needs>(actor)
        .expect("needs")
        .values[hunger.0 as usize]
        .value = 12_000;
    run_ticks(app.world_mut(), 1);
    assert!(
        view::catalogue(app.world())
            .iter()
            .all(|entry| !entry.available),
        "no building is affordable"
    );
    let left = app.world().get::<Wallet>(actor).map_or(0, |w| w.coins);
    assert!(left <= 3, "{left}");
    let path = write(&mut app, &doc, "hard-times");
    eprintln!("wrote {}", path.display());
}

/// The lender of coins for a guest's bed: the richest living resident.
fn lend(app: &mut App, to: Entity, coins: u32) {
    let held = app
        .world()
        .get::<Wallet>(to)
        .map_or(0, |wallet| wallet.coins);
    if held >= coins {
        return;
    }
    let lender = all_residents(app.world())
        .into_iter()
        .filter(|(_, body)| *body != to && village::living(app.world(), *body))
        .max_by_key(|(id, body)| {
            (
                app.world()
                    .get::<Wallet>(*body)
                    .map_or(0, |wallet| wallet.coins),
                std::cmp::Reverse(*id),
            )
        })
        .map(|(_, body)| body)
        .expect("a lender");
    let lent = (coins - held).min(app.world().get::<Wallet>(lender).expect("wallet").coins / 2);
    app.world_mut()
        .get_mut::<Wallet>(lender)
        .expect("wallet")
        .coins -= lent;
    app.world_mut().get_mut::<Wallet>(to).expect("wallet").coins += lent;
}

/// Village life on the afternoon of the second day: a resident of the inn's
/// village died of old age this morning (the body still lies there, so the
/// Departed list and mourning show), a guest has rented a bed at the inn,
/// and an awake neighbour stands beside the played character, who carries
/// giftable goods.
#[test]
#[ignore = "writes target/npc-out/fixtures/village-life.json"]
fn village_life() {
    use island_content::village::{Villager, demography::Life, frontier::labor::AssignProfession};
    let doc = construction();
    let mut app = open(&doc);
    let (id, actor) = joined(&mut app);
    let guarded = guard(&mut app, id);
    let inn = view::inns(app.world()).into_iter().next().expect("an inn");
    let settlement = inn.settlement.expect("the inn's village");
    // The played character stands in the street by the inn.
    let spot = SimPos::new(inn.entrance.x + 4_000, inn.entrance.z + 2_000);
    let village_people = |app: &App, skip: &[SimId]| -> Vec<(SimId, Entity)> {
        life_support::residents(app.world(), settlement)
            .into_iter()
            .filter(|(other, body)| {
                *other != id && !skip.contains(other) && village::living(app.world(), *body)
            })
            .collect()
    };
    // Old age comes for one resident tomorrow (the simulation's own death).
    let keeper_id = inn.keeper;
    let (dead_id, dead) = village_people(&app, &keeper_id.into_iter().collect::<Vec<_>>())
        .into_iter()
        .find(|(_, body)| app.world().get::<Life>(*body).is_some())
        .expect("a resident with a life");
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .get_mut::<Life>(dead)
        .expect("life")
        .death_day = today + 1;
    // Into the next morning: the day's first tick ages the village.
    run_to(&mut app, actor, spot, 990);
    run_to(&mut app, actor, spot, 30);
    let died = until(&mut app, 2_400, 20, |app| {
        !village::living(app.world(), dead)
    });
    assert!(died, "the old resident died");
    // The founding inns stand without an innkeeper: one resident of the
    // inn's village takes the post by the recorded AssignProfession, as the
    // resident inspector's Home and job tab would ask.
    if view::inn(app.world(), inn.inn).is_some_and(|inn| inn.keeper.is_none()) {
        let candidates = village_people(&app, &[dead_id]);
        let assigned = candidates.iter().find_map(|(resident, body)| {
            let order = AssignProfession {
                resident: *resident,
                workplace: inn.inn,
            };
            order.check(app.world()).ok().map(|()| (order, *body))
        });
        let (order, body) = assigned.expect("somebody can keep the inn");
        act(&mut app, &order);
        let keeps = until(&mut app, 6_000, 20, |app| {
            app.world()
                .get::<Villager>(body)
                .is_some_and(|person| person.role == village::Role::Innkeeper)
        });
        assert!(keeps, "the assignment took effect");
    }
    run_to(&mut app, actor, spot, 420);
    assert!(
        view::departed(app.world())
            .iter()
            .any(|gone| gone.resident == Some(dead_id)),
        "the Departed list names them"
    );
    // A guest at the inn, by a recorded VillageAct rest.
    // The inn with a living innkeeper (its workplace).
    let keeper = all_residents(app.world()).into_iter().find(|(_, body)| {
        village::living(app.world(), *body)
            && app.world().get::<Villager>(*body).is_some_and(|person| {
                person.role == village::Role::Innkeeper
                    && view::inn(app.world(), person.workplace).is_some()
            })
    });
    let (keeper_id, keeper) = keeper.unwrap_or_else(|| {
        panic!(
            "no living innkeeper: inns {:?}",
            view::inns(app.world())
                .iter()
                .map(|inn| (inn.inn, inn.keeper))
                .collect::<Vec<_>>()
        )
    });
    let workplace = app
        .world()
        .get::<Villager>(keeper)
        .expect("keeper")
        .workplace;
    let inn = view::inn(app.world(), workplace).expect("the keeper's inn");
    let guests: Vec<(SimId, Entity)> =
        life_support::residents(app.world(), inn.settlement.expect("the inn's village"))
            .into_iter()
            .filter(|(other, body)| {
                ![id, dead_id, keeper_id].contains(other) && village::living(app.world(), *body)
            })
            .collect();
    let (guest_id, guest) = guests[0];
    teleport(app.world_mut(), keeper, inn.entrance);
    teleport(app.world_mut(), guest, inn.entrance);
    lend(&mut app, guest, 60);
    let mut rest = VillageAct::new(guest_id, "rest");
    rest.target = Some(keeper_id);
    rest.check(app.world()).expect("a bed can be rented");
    act(&mut app, &rest);
    let taken = view::inn(app.world(), inn.inn)
        .expect("inn")
        .beds
        .iter()
        .any(|bed| bed.guest == Some(guest_id));
    assert!(taken, "the guest has a bed");
    // A neighbour stands awake beside the played character, not resting, so
    // the prompt offers Talk to and the People tab a gift.
    let now = app.world().resource::<SimClock>().tick;
    let (_, neighbour) = village_people(&app, &[dead_id, keeper_id, guest_id])
        .into_iter()
        .find(|(_, body)| {
            app.world()
                .get::<Villager>(*body)
                .is_some_and(|person| person.resting_until <= now)
        })
        .expect("an awake neighbour");
    teleport(app.world_mut(), actor, spot);
    teleport(
        app.world_mut(),
        neighbour,
        SimPos::new(spot.x + 900, spot.z),
    );
    stock_pouch(&mut app, actor);
    run_ticks(app.world_mut(), 1);
    unguard(&mut app, id, &guarded);
    let path = write(&mut app, &doc, "village-life");
    eprintln!("wrote {}", path.display());
}

/// The finished larger cottage by day: every tier built (the last one the
/// larger cottage, worked at the door tick by tick), a chest with goods,
/// the played character outside near the door, needs comfortable.
#[test]
#[ignore = "writes target/npc-out/fixtures/larger-cottage-done.json"]
fn larger_cottage_done() {
    let Life {
        doc,
        mut app,
        id,
        actor,
        door,
        guarded,
    } = cottage_life();
    for tier in ["storage", "second_plot", "better_tools"] {
        upgrade(&mut app, id, actor, door, tier);
        fed(&mut app, actor);
    }
    // The larger cottage by the physical flow: started empty-handed at the
    // door, every material ordered from residents (haulers walk it over and
    // are paid on hand-over), a builder hired for the work in work hours.
    // The played character waits outside and does nothing by hand.
    lend(&mut app, actor, 400);
    teleport(app.world_mut(), actor, door);
    let mut start = LiveAct::new(id, "upgrade");
    start.kind = "larger_cottage".into();
    start.check(app.world()).expect("larger cottage possible");
    act(&mut app, &start);
    let began = app.world().resource::<SimClock>().tick;
    let site = view::upgrade_site(app.world(), id).expect("site");
    for (good, count) in &site.cost {
        let mut order = LiveAct::new(id, "order");
        order.kind = good.clone();
        order.quantity = *count;
        order.check(app.world()).expect("order possible");
        act(&mut app, &order);
    }
    let hire = LiveAct::new(id, "hire");
    hire.check(app.world()).expect("hire possible");
    act(&mut app, &hire);
    let outside = SimPos::new(door.x + 3_000, door.z + 3_000);
    let day = app.world().resource::<SimConfig>().day_length.max(1);
    let index = view::homestead_tiers(app.world(), id)
        .iter()
        .position(|tier| tier.key == "larger_cottage")
        .expect("tier");
    let mut done = false;
    while app.world().resource::<SimClock>().tick < began + 4 * day {
        teleport(app.world_mut(), actor, outside);
        run_ticks(app.world_mut(), 400);
        fed(&mut app, actor);
        if view::homestead_tiers(app.world(), id)[index].done {
            done = true;
            break;
        }
    }
    let took = app.world().resource::<SimClock>().tick - began;
    eprintln!(
        "larger cottage: {} ticks, {} in-game hours (by orders, haulers and a hired builder)",
        took,
        took * 24 / day
    );
    assert!(
        done,
        "the larger cottage was not finished in four days: {:?}",
        view::upgrade_site(app.world(), id)
    );
    let home = colony::home_of(app.world(), id).expect("home");
    let capacity = view::home(app.world(), home).map(|home| home.capacity);
    assert_eq!(capacity, Some(4), "the larger cottage has four beds");
    for (good, quantity) in [("village_fish", 3), ("log", 2), ("bread", 2)] {
        econ::grant(app.world_mut(), actor, Item::new(good).stack(quantity));
        teleport(app.world_mut(), actor, door);
        let mut store = LiveAct::new(id, "store");
        store.kind = good.into();
        store.quantity = quantity;
        act(&mut app, &store);
    }
    assert!(
        view::chest(app.world(), id).is_some_and(|chest| chest.goods.len() >= 2),
        "the chest holds goods"
    );
    // Mid-morning, outside a few steps from the door.
    run_to(&mut app, actor, door, 380);
    let outside = SimPos::new(door.x + 3_000, door.z + 3_000);
    teleport(app.world_mut(), actor, outside);
    fed(&mut app, actor);
    run_ticks(app.world_mut(), 1);
    teleport(app.world_mut(), actor, outside);
    unguard(&mut app, id, &guarded);
    let path = write(&mut app, &doc, "larger-cottage-done");
    eprintln!("wrote {}", path.display());
}

/// A village with alerts showing: three residents of the first settlement
/// go without food and water. Going hungry takes days of play; the fixture
/// writes the level a long fast would reach (the need is written directly,
/// as in `hard_times`), runs two ticks so the simulation itself counts the
/// starvation and thirst, and saves. The alerts list then has rows to hover.
#[test]
#[ignore = "writes target/npc-out/fixtures/village-alerts.json"]
fn village_alerts() {
    use island_content::village::Vitality;
    let doc = construction();
    let mut app = open(&doc);
    let (id, actor) = joined(&mut app);
    let guarded = guard(&mut app, id);
    // Mid-morning of the first day, in daylight.
    run_ticks(app.world_mut(), 8_400);
    fed(&mut app, actor);
    let registry = app.world().resource::<NeedRegistry>();
    let (hunger, thirst) = (
        registry.id("Hunger").expect("hunger"),
        registry.id("Thirst").expect("thirst"),
    );
    let mut people: Vec<(SimId, Entity)> = all_residents(app.world())
        .into_iter()
        .filter(|(other, body)| *other != id && village::living(app.world(), *body))
        .collect();
    people.sort_by_key(|(other, _)| *other);
    let chosen: Vec<Entity> = people.iter().take(3).map(|(_, body)| *body).collect();
    for body in &chosen {
        let mut needs = app.world_mut().get_mut::<Needs>(*body).expect("needs");
        needs.values[hunger.0 as usize].value = 0;
        needs.values[thirst.0 as usize].value = 0;
    }
    run_ticks(app.world_mut(), 2);
    let suffering = chosen
        .iter()
        .filter(|body| {
            app.world()
                .get::<Vitality>(**body)
                .is_some_and(|vital| vital.starved_ticks > 0 || vital.dehydrated_ticks > 0)
        })
        .count();
    assert!(suffering >= 2, "two or more residents suffer: {suffering}");
    unguard(&mut app, id, &guarded);
    let path = write(&mut app, &doc, "village-alerts");
    eprintln!("wrote {}", path.display());
}
