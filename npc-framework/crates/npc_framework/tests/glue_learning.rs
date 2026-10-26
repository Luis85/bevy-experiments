//! Wave F4: the learning and crafting glue (ADR 0011/0012).
//!
//! - progression × crafting: a completed craft awards its XP (source
//!   `craft`, levels, banked points) and practice;
//! - progression × interaction: studying a book raises a skill (capped by
//!   the book), teaching is gated by the teacher's level, interaction skill
//!   checks teach through the rules roll feed;
//! - crafting × interaction: no interaction starts while a unit crafts (and
//!   no craft while it interacts);
//! - opt-in: none of it without both capabilities; determinism.

#![cfg(all(feature = "progression", feature = "crafting", feature = "interaction"))]

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::{CraftError, CraftSource, begin_craft};
use npc_framework::glue::{Teacher, Teaches};
use npc_framework::interaction::items::put_in_inventory;
use npc_framework::interaction::{Ctx, begin, can_interact};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::kernel::registry::ContentHash;
use npc_framework::prelude::*;
use npc_framework::progression::Learning;

fn ok(_: &World, _: &Ctx) -> Result<(), String> {
    Ok(())
}
fn nothing(_: &mut World, _: &Ctx) {}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(
        NpcPlugins::new()
            .seed(11)
            .without_driver()
            .with_progression()
            .with_crafting(),
    )
    .add_plugins(JournalPlugin::default())
    .register_item_kind(ItemKindDef::new("reed").stackable(20))
    .register_item_kind(ItemKindDef::new("basket"))
    .register_item_kind(ItemKindDef::new("primer"))
    .register_skill(SkillDef::new("Weaving", Stat::Dx, Difficulty::Easy).default_stat(Stat::Dx, -4))
    .register_skill(SkillDef::new("Lore", Stat::Iq, Difficulty::Average).default_stat(Stat::Iq, -5))
    .register_recipe(
        RecipeDef::new("basket")
            .input("reed", 2)
            .skill("Weaving", 10)
            .duration(30)
            .output("basket", 1)
            .xp(150),
    )
    .register_interaction(
        InteractionDef::new("tinker", ok, nothing)
            .check("Weaving", 10)
            .duration(1)
            .no_reservation(),
    );
    app.finish();
    app.cleanup();
    app
}

fn journal(app: &App) -> Vec<(Option<SimId>, JournalKind)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .map(|e| (e.unit, e.kind.clone()))
        .collect()
}

fn learner(app: &mut App, pos: SimPos, skills: &[(&str, i32)]) -> (SimId, Entity) {
    let mut sheet = CharacterSheet::default();
    for (s, p) in skills {
        sheet.skills.insert((*s).to_owned(), *p);
    }
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Interactor,
            Inventory::new(4, 100_000),
            sheet,
            Experience::default(),
            SpendPolicy::Player,
        ),
    )
}

fn give(app: &mut App, holder: Entity, item: impl Bundle) -> SimId {
    let (id, e) = spawn_sim(app.world_mut(), (SimPos::new(0, 0), item));
    put_in_inventory(app.world_mut(), holder, e);
    id
}

fn level(app: &App, unit: SimId, skill: &str) -> Option<i32> {
    npc_framework::rules::effective_skill(app.world(), unit, skill)
}

fn interact(app: &mut App, unit: SimId, verb: &str, target: SimId) {
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Interact::new(unit, verb, target)),
    )
    .unwrap_or_else(|e| panic!("{verb}: {e}"));
}

#[test]
fn a_craft_awards_xp_levels_up_and_practises_the_skill() {
    let mut app = app();
    let (u, ue) = learner(&mut app, SimPos::new(0, 0), &[("Weaving", 4)]);
    give(&mut app, ue, Item::new("reed").stack(2));
    begin_craft(
        app.world_mut(),
        ue,
        "basket",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap();
    run_ticks(app.world_mut(), 31);
    let j = journal(&app);
    assert!(
        j.iter().any(|(w, k)| *w == Some(u)
            && matches!(k, JournalKind::XpGained { source, amount: 150, .. } if source == "craft")),
        "craft XP: {j:?}"
    );
    assert!(j.iter().any(|(w, k)| *w == Some(u)
        && matches!(
            k,
            JournalKind::LevelUp {
                level: 1,
                points: 5
            }
        )));
    let exp = *app.world().get::<Experience>(ue).unwrap();
    assert!(exp.level >= 1 && exp.xp >= 150, "{exp:?}");
    assert_eq!(app.world().get::<CharacterSheet>(ue).unwrap().unspent, 5);
    let learning = app.world().get::<Learning>(ue).unwrap().get("Weaving");
    assert_eq!(learning.uses, 1, "the craft roll came through the feed");
    assert_eq!(
        learning.practice,
        30 + 30,
        "roll practice + the craft's duration"
    );
}

#[test]
fn studying_a_book_raises_the_skill_up_to_the_books_limit() {
    let mut app = app();
    let (u, ue) = learner(&mut app, SimPos::new(0, 0), &[]);
    let before = level(&app, u, "Lore").unwrap();
    let book = give(
        &mut app,
        ue,
        (Item::new("primer"), Teaches::new("Lore", 12, 200)),
    );
    interact(&mut app, u, "study", book);
    run_ticks(app.world_mut(), 202);
    let after = level(&app, u, "Lore").unwrap();
    assert!(after > before, "{before} -> {after}");
    assert!(journal(&app).iter().any(|(w, k)| *w == Some(u)
        && matches!(k, JournalKind::SkillImproved { skill, via, .. } if skill == "Lore" && via == "study")));
    // A book that teaches no further than the reader already knows.
    let thin = give(
        &mut app,
        ue,
        (Item::new("primer"), Teaches::new("Lore", after, 200)),
    );
    let why = can_interact(app.world(), ue, "study", thin, None).unwrap_err();
    assert_eq!(
        why,
        format!("already Lore {after} (it teaches up to {after})")
    );
    // Units that do not progress cannot study.
    let (_, other) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), Interactor, CharacterSheet::default()),
    );
    let why = can_interact(app.world(), other, "study", book, None).unwrap_err();
    assert!(why.contains("no experience"), "{why}");
}

#[test]
fn teaching_is_gated_by_the_teachers_level() {
    let mut app = app();
    let mut sheet = CharacterSheet::default();
    sheet.skills.insert("Lore".into(), 8);
    let (t, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 0),
            Interactor,
            sheet,
            Teacher::new(["Lore"], 100),
        ),
    );
    let (s, _) = learner(&mut app, SimPos::new(1000, 0), &[]);
    let before = level(&app, s, "Lore").unwrap();
    interact(&mut app, t, "teach", s);
    run_ticks(app.world_mut(), 102);
    let after = level(&app, s, "Lore").unwrap();
    assert!(after > before, "{before} -> {after}");
    assert!(journal(&app).iter().any(|(w, k)| *w == Some(s)
        && matches!(k, JournalKind::SkillImproved { via, .. } if via == "teaching")));

    // A weak teacher (Lore 1 point) and a strong student (4 points).
    let mut weak = CharacterSheet::default();
    weak.skills.insert("Lore".into(), 1);
    let (_, we) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 1000),
            Interactor,
            weak,
            Teacher::new(["Lore"], 100),
        ),
    );
    let (strong, _) = learner(&mut app, SimPos::new(1000, 1000), &[("Lore", 4)]);
    let why = can_interact(app.world(), we, "teach", strong, None).unwrap_err();
    let (tl, sl) = (
        level(&app, SimId(app.world().get::<SimId>(we).unwrap().0), "Lore").unwrap(),
        level(&app, strong, "Lore").unwrap(),
    );
    assert_eq!(
        why,
        format!("teacher level {tl} must be at least the student's {sl} + 1")
    );
}

#[test]
fn interaction_checks_teach_through_the_roll_feed() {
    let mut app = app();
    let (u, ue) = learner(&mut app, SimPos::new(0, 0), &[("Weaving", 1)]);
    let (thing, _) = spawn_sim(app.world_mut(), (SimPos::new(500, 0), Item::new("basket")));
    interact(&mut app, u, "tinker", thing);
    run_ticks(app.world_mut(), 3);
    assert!(journal(&app).iter().any(|(w, k)| *w == Some(u)
        && matches!(k, JournalKind::Roll { purpose, .. } if purpose == "interaction.tinker")));
    let learning = app.world().get::<Learning>(ue).unwrap().get("Weaving");
    assert_eq!(learning.uses, 1);
    assert!(journal(&app).iter().any(|(w, k)| *w == Some(u)
        && matches!(k, JournalKind::XpGained { source, .. } if source == "roll")));
}

#[test]
fn no_interaction_starts_while_crafting_and_no_craft_while_interacting() {
    let mut app = app();
    let (u, ue) = learner(&mut app, SimPos::new(0, 0), &[("Weaving", 4)]);
    give(&mut app, ue, Item::new("reed").stack(4));
    let (thing, _) = spawn_sim(app.world_mut(), (SimPos::new(500, 0), Item::new("basket")));
    begin_craft(
        app.world_mut(),
        ue,
        "basket",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap();
    let err = submit(
        app.world_mut(),
        None,
        SimCommand::content(&Interact::new(u, "pick_up", thing)),
    )
    .unwrap_err();
    assert!(err.to_string().contains("busy with craft basket"), "{err}");
    let err = begin(
        app.world_mut(),
        ue,
        "pick_up",
        thing,
        None,
        npc_framework::interaction::Source::Command,
    )
    .unwrap_err();
    assert_eq!(err, "busy with craft basket");
    run_ticks(app.world_mut(), 31);
    // Free again: it interacts, and meanwhile cannot craft.
    begin(
        app.world_mut(),
        ue,
        "pick_up",
        thing,
        None,
        npc_framework::interaction::Source::Command,
    )
    .unwrap();
    let err = begin_craft(
        app.world_mut(),
        ue,
        "basket",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CraftError::Busy { .. }), "{err:?}");
}

fn names(plugins: NpcPlugins) -> Vec<String> {
    let mut app = App::new();
    app.add_plugins(plugins.without_driver());
    app.finish();
    app.cleanup();
    app.world()
        .resource::<ContentHash>()
        .manifest
        .iter()
        .map(|(c, n)| format!("{c}:{n}"))
        .collect()
}

#[test]
fn learning_glue_needs_both_capabilities() {
    // `name` alone matches any category; `cat:name` exactly.
    let has = |n: &[String], name: &str| {
        n.iter()
            .any(|x| x == name || x.split_once(':').is_some_and(|(_, v)| v == name))
    };
    let progression = names(NpcPlugins::new().with_progression());
    assert!(!has(&progression, "study") && !has(&progression, "teaches"));
    assert!(
        !has(&progression, "craft_hook:npc_progression"),
        "no craft hook"
    );
    let interaction = names(NpcPlugins::new().with_interaction());
    assert!(!has(&interaction, "study") && !has(&interaction, "busy=npc_crafting"));
    let crafting = names(NpcPlugins::new().with_crafting());
    assert!(has(&crafting, "busy=npc_crafting"));
    assert!(!has(&crafting, "craft_hook:npc_progression") && !has(&crafting, "study"));
    let both = names(NpcPlugins::new().with_progression().with_crafting());
    for name in [
        "craft_hook:npc_progression",
        "study",
        "teach",
        "teaches",
        "teacher",
        "busy=npc_crafting",
    ] {
        assert!(has(&both, name), "{name}");
    }
}

fn scripted_run() -> u64 {
    let mut app = app();
    let (u, ue) = learner(&mut app, SimPos::new(0, 0), &[("Weaving", 2)]);
    give(&mut app, ue, Item::new("reed").stack(2));
    let book = give(
        &mut app,
        ue,
        (Item::new("primer"), Teaches::new("Lore", 12, 50)),
    );
    begin_craft(
        app.world_mut(),
        ue,
        "basket",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap();
    run_ticks(app.world_mut(), 31);
    interact(&mut app, u, "study", book);
    run_ticks(app.world_mut(), 60);
    state_hash(app.world_mut())
}

#[test]
fn learning_glue_is_deterministic() {
    assert_eq!(scripted_run(), scripted_run());
}
