//! Residents notice the played character (S3.6, npc-ai#6): the condition
//! `life.player_within` and the leaf `life.greet player` of the shipped tree.
//! A free resident greets once a day, never while asleep or at work; the day's
//! greeting is saved state (`Bond.greeted_day`), so a continued save does not
//! greet twice.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Vitality, colony, economy as econ,
    frontier::LivingWorldSpec,
    indoors,
    live::{self, LiveState},
    resident::{self, param},
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};

const DAY: u64 = 2_400;

/// Short clear days; no boat and no births.
fn quiet(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "clear");
    economy(world)["merchant"]["enabled"] = json!(false);
    economy(world)["demography"]["birth_chance_permille"] = json!(0);
}

fn player(app: &mut App) -> (SimId, Entity) {
    let id = app.world().resource::<village::VillageState>().played;
    let actor = entity(app.world(), id);
    colony::JoinIsland { unit: id.0 }
        .perform(app.world_mut())
        .expect("arrived");
    (id, actor)
}

/// The first living adult of the harbour.
fn neighbour(app: &App) -> (SimId, Entity) {
    let harbour = settlement(app.world(), "harbour");
    residents(app.world(), harbour)
        .into_iter()
        .find(|(_, actor)| {
            village::living(app.world(), *actor)
                && village::demography::stage(app.world(), *actor)
                    == village::demography::LifeStage::Adult
        })
        .expect("a harbour adult")
}

/// Puts the played character one metre beside `resident`.
fn beside(app: &mut App, actor: Entity, resident: Entity) {
    let at = *app.world().get::<SimPos>(resident).expect("position");
    teleport(app.world_mut(), actor, SimPos::new(at.x + 1_000, at.z));
}

fn greeted_day(app: &App, player: SimId, resident: SimId) -> Option<u64> {
    app.world()
        .resource::<LiveState>()
        .bonds
        .get(&player)?
        .get(&resident)?
        .greeted_day
}

/// The `VillageGreeting` effects `resident` journaled, oldest first.
fn greetings_by(app: &App, resident: SimId) -> Vec<Value> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| entry.unit == Some(resident))
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some("VillageGreeting") => {
                Some(serde_json::to_value(effect).expect("effect JSON"))
            }
            _ => None,
        })
        .collect()
}

fn today(app: &App) -> u64 {
    village::calendar(app.world()).0
}

#[test]
fn player_within_is_the_played_character_alive_and_in_range() {
    let mut app = world_with(quiet);
    let (_, resident) = neighbour(&app);
    // Not arrived yet: nobody to notice.
    assert!(!live::player_within(app.world(), resident, 100_000));
    let (_, actor) = player(&mut app);
    let at = *app.world().get::<SimPos>(resident).expect("position");
    teleport(app.world_mut(), actor, SimPos::new(at.x + 3_000, at.z));
    assert!(live::player_within(app.world(), resident, 6_000));
    assert!(live::player_within(app.world(), resident, 3_000));
    assert!(!live::player_within(app.world(), resident, 2_999));
    teleport(app.world_mut(), actor, SimPos::new(at.x + 6_001, at.z));
    assert!(!live::player_within(app.world(), resident, 6_000));
    // The played character never notices itself.
    assert!(!live::player_within(app.world(), actor, 100_000));
    // Dead: nobody to notice.
    teleport(app.world_mut(), actor, SimPos::new(at.x + 1_000, at.z));
    assert!(live::player_within(app.world(), resident, 6_000));
    app.world_mut()
        .get_mut::<Vitality>(actor)
        .expect("vitality")
        .died_at = Some(1);
    assert!(!live::player_within(app.world(), resident, 6_000));

    // The guard is data: `value` is metres, required, 1..=100.
    let mut guard = json!({"Guard": {"if": "life.player_within", "value": 6, "abort": "None"}});
    param::rewrite_guards(&mut guard).expect("rewritten");
    assert_eq!(guard["Guard"]["cond"], "life.player_within:6");
    let mut missing = json!({"Guard": {"if": "life.player_within", "abort": "None"}});
    let error = param::rewrite_guards(&mut missing).expect_err("needs a value");
    assert!(error.contains("value"), "{error}");
    let economy = app.world().resource::<LivingWorldSpec>().economy.clone();
    assert_eq!(
        param::parse("life.player_within:6", &economy),
        Ok(param::ParamCond::PlayerWithin { mm: 6_000 })
    );
    assert!(param::parse("life.player_within:0", &economy).is_err());
    assert!(param::parse("life.player_within:101", &economy).is_err());
    // The shipped tree registers it.
    let registry = app.world().resource::<npc_framework::ai::AiRegistry>();
    assert!(registry.cond_id("life.player_within:6").is_some());
    assert!(registry.action_id("life.greet").is_some());
}

/// Follows `resident` with the played character for `ticks`, one tick at a
/// time; returns the ticks at which it greeted, with the day.
fn follow(app: &mut App, actor: Entity, resident: Entity, ticks: u64) -> Vec<(u64, u64)> {
    let unit = id_of(app.world(), actor);
    let other = id_of(app.world(), resident);
    let mut greetings = Vec::new();
    let mut last = greeted_day(app, unit, other);
    let mut asleep_beside = 0;
    for _ in 0..ticks {
        beside(app, actor, resident);
        let asleep = indoors::asleep(app.world(), resident);
        run_ticks(app.world_mut(), 1);
        let now = greeted_day(app, unit, other);
        if asleep {
            asleep_beside += 1;
        }
        if now != last {
            let tick = app.world().resource::<SimClock>().tick;
            assert!(!asleep, "greeted in its sleep at tick {tick}");
            let decision = resident::decision(app.world(), resident).expect("decision");
            assert_eq!(decision.action, "life.greet", "{decision:?}");
            assert!(
                decision.branch.iter().any(|node| node == "Player nearby")
                    && !decision.branch.iter().any(|node| node == "Work hours"),
                "greeted outside free time: {:?}",
                decision.branch
            );
            // It turned to the played character.
            let here = *app.world().get::<SimPos>(resident).expect("position");
            let there = *app.world().get::<SimPos>(actor).expect("position");
            assert_eq!(
                app.world().get::<Facing>(resident).copied(),
                Some(Facing::towards(here, there))
            );
            greetings.push((tick, now.expect("a day")));
            last = now;
        }
    }
    if ticks >= DAY {
        assert!(asleep_beside > 0, "the resident never slept");
    }
    greetings
}

#[test]
fn a_free_resident_greets_the_player_once_a_day_and_never_asleep_or_at_work() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (unit, actor) = player(&mut app);
    let (other, resident) = neighbour(&app);
    let first_day = today(&app);
    let greetings = follow(&mut app, actor, resident, 2 * DAY);
    let days: Vec<u64> = greetings.iter().map(|(_, day)| *day).collect();
    assert_eq!(
        days,
        vec![first_day, first_day + 1],
        "one greeting a day: {greetings:?}"
    );
    // The bond gained half a talk each time.
    assert!(live::score(app.world(), unit, other) > 0);
    // Journal: the greeting and the spoken line, addressed to the player.
    let lines = &app
        .world()
        .resource::<LivingWorldSpec>()
        .economy
        .live
        .greetings
        .clone();
    let greeted = greetings_by(&app, other);
    // The journal is a ring: the latest greeting is still in it.
    assert!(!greeted.is_empty() && greeted.len() <= 2, "{greeted:?}");
    assert!(greeted.iter().all(|effect| effect["to"] == json!(unit.0)));
    assert!(
        greeted
            .iter()
            .all(|effect| lines.iter().any(|line| effect["text"] == json!(line))),
        "{greeted:?}"
    );
    let spoken = effects(app.world(), "VillageSpeech");
    assert!(
        spoken.iter().any(|effect| effect["target"] == json!(unit.0)
            && lines.iter().any(|line| effect["text"] == json!(line))),
        "{spoken:?}"
    );
}

#[test]
fn a_continued_save_remembers_the_day_greeting() {
    let mut app = world_with(quiet);
    let (unit, actor) = player(&mut app);
    let (other, resident) = neighbour(&app);
    let first_day = today(&app);
    // Follow until the first greeting, then save mid-day.
    let first = loop {
        let greetings = follow(&mut app, actor, resident, 20);
        if let Some(first) = greetings.first() {
            break *first;
        }
        assert_eq!(today(&app), first_day, "no greeting on the first day");
    };
    assert_eq!(greeted_day(&app, unit, other), Some(first.1));
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut resumed = world_with(quiet);
    let report =
        snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    assert_eq!(greeted_day(&resumed, unit, other), Some(first.1));
    assert_eq!(state_hash(resumed.world_mut()), state_hash(app.world_mut()));
    // The rest of the day: no second greeting in either world.
    let left = DAY - app.world().resource::<SimClock>().tick % DAY - 1;
    let again = follow(&mut app, actor, resident, left);
    let resumed_actor = entity(resumed.world(), unit);
    let resumed_resident = entity(resumed.world(), other);
    let resumed_again = follow(&mut resumed, resumed_actor, resumed_resident, left);
    assert!(again.is_empty(), "{again:?}");
    assert!(resumed_again.is_empty(), "{resumed_again:?}");
    assert_eq!(today(&app), first.1);
    assert_eq!(state_hash(resumed.world_mut()), state_hash(app.world_mut()));
}

#[test]
fn greetings_are_deterministic() {
    let run = || {
        let mut app = world_with(quiet);
        let (_, actor) = player(&mut app);
        let (_, resident) = neighbour(&app);
        let greetings = follow(&mut app, actor, resident, DAY);
        (greetings, state_hash(app.world_mut()))
    };
    let (a, hash_a) = run();
    let (b, hash_b) = run();
    assert!(!a.is_empty());
    assert_eq!(a, b);
    assert_eq!(hash_a, hash_b);
}

#[test]
fn a_resident_whose_trade_the_player_did_says_so() {
    let mut app = world_with(quiet);
    set_journal(app.world_mut(), true, false);
    let (unit, actor) = player(&mut app);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let people: Vec<(SimId, Entity)> = residents(app.world(), harbour)
        .into_iter()
        .filter(|(_, body)| {
            village::living(app.world(), *body)
                && village::demography::stage(app.world(), *body)
                    == village::demography::LifeStage::Adult
        })
        .collect();
    // Sell, as the played character, one good a harbour resident makes to
    // another harbour resident who wants it.
    let mut maker = None;
    'search: for (maker_id, maker_body) in &people {
        let role = app
            .world()
            .get::<village::Villager>(*maker_body)
            .expect("villager")
            .role;
        for good in village::catalog::outputs_for(app.world(), role).to_vec() {
            econ::grant(app.world_mut(), actor, Item::new(good).stack(1));
            for (buyer_id, buyer) in &people {
                if buyer_id == maker_id {
                    continue;
                }
                let at = *app.world().get::<SimPos>(*buyer).expect("position");
                teleport(app.world_mut(), actor, at);
                let mut sell = village::VillageAct::new(unit, "sell");
                sell.target = Some(*buyer_id);
                sell.kind = good.into();
                if sell.check(app.world()).is_ok() {
                    submit(app.world_mut(), None, SimCommand::content(&sell)).expect("accepted");
                    run_ticks(app.world_mut(), 1);
                    if live::day_so_far(app.world(), unit)
                        .is_some_and(|day| day.sold.get(good).copied().unwrap_or(0) > 0)
                    {
                        maker = Some((*maker_id, *maker_body));
                        break 'search;
                    }
                }
            }
        }
    }
    let (maker_id, maker_body) = maker.expect("the played character sold a harbour good");
    let name = app
        .world()
        .get::<village::Villager>(actor)
        .expect("villager")
        .name
        .clone();
    let left = DAY - app.world().resource::<SimClock>().tick % DAY - 1;
    let greetings = follow(&mut app, actor, maker_body, left);
    assert_eq!(greetings.len(), 1, "{greetings:?}");
    let line = format!("You've been busy at my trade, {name}.");
    let said = greetings_by(&app, maker_id);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0]["trade"], json!(true));
    assert_eq!(said[0]["text"], json!(line));
    assert_eq!(greeted_day(&app, unit, maker_id), Some(today(&app)));
}
