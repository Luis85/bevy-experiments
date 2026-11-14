//! Traits and schedules are data, and they change what a resident does.
//!
//! Each behaviour test runs the same world twice from the same seed. The two
//! runs differ in exactly one trait of one resident, so whatever that resident
//! does differently is caused by the trait. The schedules are edited in JSON so
//! the relevant block falls in the first minutes of the day.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    behavior::{LifeDecision, LifeGoal},
    colony::SocialLife,
    frontier::labor::ResidentProfile,
    resident::{self, BlockKind, ResidentTraits},
};
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

const STEP: u64 = 20;

fn app_with(behavior: Value, seed: u64) -> App {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.world["construction"]["world"]["preset"]["living_world"]["behavior"] = behavior;
    island_content::scenario::load_scenario(&doc, seed)
        .expect("edited behaviour loads")
        .app
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

fn entity(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("resident")
}

fn set_trait(app: &mut App, id: SimId, edit: impl FnOnce(&mut ResidentTraits)) {
    let actor = entity(app, id);
    edit(
        &mut app
            .world_mut()
            .get_mut::<ResidentTraits>(actor)
            .expect("traits"),
    );
}

/// What one resident did, sampled every [`STEP`] ticks.
#[derive(Clone, Copy, Debug, Default)]
struct Samples {
    work: u32,
    /// Samples spent in the authored `Leisure` branch of the tree.
    leisure: u32,
    home: u32,
    first_home: Option<u64>,
}

fn sample(app: &App, id: SimId, into: &mut Samples) {
    let actor = entity(app, id);
    let now = app.world().resource::<SimClock>().tick;
    let goal = app.world().get::<LifeDecision>(actor).map(|d| d.goal);
    if goal == Some(LifeGoal::Work) {
        into.work += 1;
    }
    if matches!(goal, Some(LifeGoal::Home | LifeGoal::Recovery)) {
        into.home += 1;
        into.first_home.get_or_insert(now);
    }
    if resident::explain(app.world(), actor).is_some_and(|record| {
        record["branch"]
            .as_str()
            .is_some_and(|branch| branch.contains(" > Leisure > "))
    }) {
        into.leisure += 1;
    }
}

fn run_sampled(app: &mut App, ids: &[SimId], until: u64) -> Vec<Samples> {
    let mut samples = vec![Samples::default(); ids.len()];
    while app.world().resource::<SimClock>().tick < until {
        run_ticks(app.world_mut(), STEP);
        for (id, into) in ids.iter().zip(&mut samples) {
            sample(app, *id, into);
        }
    }
    samples
}

fn meetings(app: &App, id: SimId) -> i32 {
    app.world()
        .get::<SocialLife>(entity(app, id))
        .map_or(0, |social| social.affinity.values().sum::<i32>() / 250)
}

#[test]
fn traits_are_seeded_per_resident_within_the_authored_ranges() {
    let app = island_content::new_sim_app(7, "living_world");
    let other = island_content::new_sim_app(11, "living_world");
    let spec = resident::spec(app.world())
        .expect("behaviour")
        .traits
        .clone();
    let people = residents(app.world());
    let traits: Vec<ResidentTraits> = people
        .iter()
        .map(|(_, actor)| *app.world().get::<ResidentTraits>(*actor).expect("traits"))
        .collect();
    for (value, range) in traits.iter().flat_map(|t| {
        [
            (t.diligence, spec.diligence),
            (t.sociability, spec.sociability),
            (t.chronotype, spec.chronotype),
            (t.thrift, spec.thrift),
        ]
    }) {
        assert!((range.min..=range.max).contains(&value));
    }
    // Residents are individuals, a seed is a different village, and the same
    // seed is the same village.
    for pick in [
        |t: &ResidentTraits| t.diligence,
        |t: &ResidentTraits| t.sociability,
        |t: &ResidentTraits| t.chronotype,
        |t: &ResidentTraits| t.thrift,
    ] {
        let distinct: std::collections::BTreeSet<i32> = traits.iter().map(pick).collect();
        assert!(
            distinct.len() > 10,
            "only {} distinct values",
            distinct.len()
        );
    }
    let again = island_content::new_sim_app(7, "living_world");
    let read = |app: &App| -> Vec<ResidentTraits> {
        residents(app.world())
            .iter()
            .map(|(_, actor)| *app.world().get::<ResidentTraits>(*actor).expect("traits"))
            .collect()
    };
    assert_eq!(read(&again), traits);
    assert_ne!(read(&other), traits);
    // Authored ranges are honoured: a fixed range gives everybody that value.
    let fixed = app_with(json!({"traits": {"thrift": {"min": 7, "max": 7}}}), 7);
    assert!(read(&fixed).iter().all(|t| t.thrift == 7));
}

#[test]
fn schedule_blocks_are_data_and_move_with_the_chronotype() {
    // Sleep from 10% to 90% of the day; a chronotype of 100 moves every block
    // 5% of the day (1200 ticks) later, one of -100 as much earlier.
    let behavior = json!({
        "traits": {"chronotype": {"min": 0, "max": 0}},
        "schedule": {"chronotype_shift": 50, "blocks": [
            {"kind": "work", "from": 0, "to": 1000},
            {"kind": "sleep", "from": 100, "to": 900}
        ]}
    });
    let mut early = app_with(behavior.clone(), 7);
    let mut late = app_with(behavior, 7);
    let (subject, _) = residents(early.world())[5];
    set_trait(&mut early, subject, |t| t.chronotype = -100);
    set_trait(&mut late, subject, |t| t.chronotype = 100);
    // The lark's night starts at tick 1200, the owl's at tick 3600.
    let lark = run_sampled(&mut early, &[subject], 4_000)[0];
    let owl = run_sampled(&mut late, &[subject], 4_000)[0];
    println!("CHRONOTYPE lark={lark:?} owl={owl:?}");
    let lark_home = lark.first_home.expect("the lark went home");
    let owl_home = owl.first_home.expect("the owl went home");
    assert!(
        (1_200..=1_400).contains(&lark_home),
        "the lark went home at tick {lark_home}"
    );
    assert!(
        (3_600..=3_800).contains(&owl_home),
        "the owl went home at tick {owl_home}"
    );
    assert!(
        lark.home >= owl.home + 100,
        "the lark was home for {} samples, the owl for {}",
        lark.home,
        owl.home
    );
    let actor = entity(&late, subject);
    assert!(resident::in_block(late.world(), actor, BlockKind::Sleep));
}

/// Leisure from 5% to 40% of the day, so the first hour of the run covers it.
/// Meetings may follow each other closely (the shipped cooldown, 2400 ticks,
/// would allow one or two in the sampled hour, for sociable and unsociable
/// residents alike).
fn early_leisure(traits: Value) -> Value {
    json!({
        "traits": traits,
        "schedule": {"diligence_shift": 100, "meet_cooldown_ticks": 300, "blocks": [
            {"kind": "work", "from": 0, "to": 700},
            {"kind": "leisure", "from": 50, "to": 400},
            {"kind": "sleep", "from": 700, "to": 1000}
        ]}
    })
}

#[test]
fn a_sociable_resident_takes_leisure_and_meets_more_people_than_an_unsociable_one() {
    // Everybody else is sociable, so there is company to be had.
    let behavior = early_leisure(json!({
        "sociability": {"min": 100, "max": 100},
        "diligence": {"min": 50, "max": 50},
        "chronotype": {"min": 0, "max": 0}
    }));
    let mut loner = app_with(behavior.clone(), 7);
    let mut mixer = app_with(behavior, 7);
    let (subject, _) = residents(loner.world())[3];
    set_trait(&mut loner, subject, |t| t.sociability = 0);
    let alone = run_sampled(&mut loner, &[subject], 3_600)[0];
    let social = run_sampled(&mut mixer, &[subject], 3_600)[0];
    let (met_alone, met_social) = (meetings(&loner, subject), meetings(&mixer, subject));
    println!(
        "SOCIABILITY unsociable={alone:?} meetings={met_alone} sociable={social:?} \
         meetings={met_social}"
    );
    // Since the review fix pass (npc-ai#3) everybody has an evening; an
    // unsociable resident spends it without company (checked below by the
    // meetings), so its leisure samples are no longer zero.
    assert!(
        social.leisure >= 40,
        "the sociable resident spent only {} samples at leisure",
        social.leisure
    );
    assert!(
        met_social > met_alone,
        "sociable met {met_social} people, unsociable {met_alone}"
    );
}

#[test]
fn a_diligent_resident_works_longer_before_leisure() {
    let behavior = early_leisure(json!({
        "sociability": {"min": 100, "max": 100},
        "diligence": {"min": 50, "max": 50},
        "chronotype": {"min": 0, "max": 0}
    }));
    // Leisure starts at 5% for diligence 50; 10 points earlier or later per
    // 10 diligence: never before tick 3600 for diligence 100.
    let mut idler = app_with(behavior.clone(), 7);
    let mut worker = app_with(behavior, 7);
    let ids: Vec<SimId> = residents(idler.world()).iter().map(|(id, _)| *id).collect();
    // The subject is whoever has the most work to do in this world.
    let mut pilot = app_with(
        early_leisure(json!({
            "sociability": {"min": 0, "max": 0},
            "chronotype": {"min": 0, "max": 0}
        })),
        7,
    );
    let busiest = run_sampled(&mut pilot, &ids, 1_200);
    let subject = ids[busiest
        .iter()
        .enumerate()
        .max_by_key(|(index, samples)| (samples.work, usize::MAX - index))
        .expect("residents")
        .0];
    set_trait(&mut idler, subject, |t| t.diligence = 0);
    set_trait(&mut worker, subject, |t| t.diligence = 100);
    let lazy = run_sampled(&mut idler, &[subject], 3_400)[0];
    let diligent = run_sampled(&mut worker, &[subject], 3_400)[0];
    println!(
        "DILIGENCE subject={} lazy={lazy:?} diligent={diligent:?}",
        subject.0
    );
    assert_eq!(
        diligent.leisure, 0,
        "a resident with diligence 100 took leisure before its block"
    );
    assert!(
        lazy.leisure >= 40,
        "diligence 0 took only {} samples",
        lazy.leisure
    );
    assert!(
        diligent.work >= lazy.work + 30,
        "diligent worked {} samples, lazy {}",
        diligent.work,
        lazy.work
    );
}

/// Coins `id` paid for water and food, from the trade receipts in the journal.
fn spent_on_provisions(app: &App, id: SimId) -> u64 {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| entry.unit == Some(id))
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some("VillageTrade") => {
                let kind = effect.get("kind").and_then(Value::as_str)?;
                let provision = kind == village::catalog::WATER
                    || village::catalog::food_restore_in(app.world(), kind).is_some();
                provision
                    .then(|| effect.get("coins").and_then(Value::as_u64))
                    .flatten()
            }
            _ => None,
        })
        .sum()
}

fn stocking(app: &App, id: SimId) -> bool {
    village::behavior::decision(app.world(), entity(app, id)) == Some("stocking provisions")
}

#[test]
fn a_thrifty_resident_keeps_its_coins_where_a_spender_stocks_provisions() {
    // Nobody is thrifty, and one thrift point would hold back 100 coins.
    let behavior = json!({"traits": {
        "thrift": {"min": 0, "max": 0}, "thrift_reserve_coins": 10000
    }});
    let mut spenders = app_with(behavior.clone(), 7);
    set_journal(spenders.world_mut(), true, false);
    let ids: Vec<SimId> = residents(spenders.world())
        .iter()
        .map(|(id, _)| *id)
        .collect();
    // The subject is the first resident who pays for provisions ahead of need.
    let mut subject = None;
    while spenders.world().resource::<SimClock>().tick < 4_800 && subject.is_none() {
        run_ticks(spenders.world_mut(), STEP);
        subject = ids
            .iter()
            .copied()
            .find(|id| stocking(&spenders, *id) && spent_on_provisions(&spenders, *id) > 0);
    }
    let subject = subject.expect("nobody stocked provisions in 4800 ticks");
    let at = spenders.world().resource::<SimClock>().tick;
    let mut saver = app_with(behavior, 7);
    set_journal(saver.world_mut(), true, false);
    set_trait(&mut saver, subject, |t| t.thrift = 100);
    let mut stocked = false;
    while saver.world().resource::<SimClock>().tick < at {
        run_ticks(saver.world_mut(), STEP);
        stocked |= stocking(&saver, subject);
    }
    let (spent, saved) = (
        spent_on_provisions(&spenders, subject),
        spent_on_provisions(&saver, subject),
    );
    println!(
        "THRIFT subject={} ({}) tick={at} spender_paid={spent} thrifty_paid={saved}",
        subject.0,
        spenders
            .world()
            .get::<Villager>(entity(&spenders, subject))
            .expect("resident")
            .name
    );
    assert!(!stocked, "the thrifty resident stocked provisions");
    assert!(spent > 0);
    assert_eq!(
        saved, 0,
        "the thrifty resident paid {saved} coins for provisions ahead of need"
    );
}
