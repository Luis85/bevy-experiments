use super::model::*;
use super::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::run_ticks;
use serde_json::json;
use testbed_content::village::resident::{BlockKind, ResidentTraits, ScheduleSpec};

fn names(key: &str) -> String {
    key.strip_prefix("village_")
        .unwrap_or(key)
        .replace('_', " ")
}

/// No internal identifier may reach the player.
fn assert_plain(text: &str) {
    for jargon in [
        "life.",
        "village_",
        " holds, but",
        "is false",
        "funded",
        "_",
    ] {
        assert!(!text.contains(jargon), "`{jargon}` leaked into: {text}");
    }
}

/// The live world may gain reasons this view has no wording for yet; they
/// still must not show an identifier or the record's own grammar.
fn assert_no_identifiers(text: &str) {
    for jargon in [
        "life.",
        "village_",
        " holds, but",
        "is false",
        "has_unit_order",
    ] {
        assert!(!text.contains(jargon), "`{jargon}` leaked into: {text}");
    }
}

/// A record as the simulation journals it for a miller who went to gather.
fn work_record() -> Value {
    json!({
        "tree": "island_life",
        "branch": "Resident life > Work hours > Profession > Gather",
        "node": 61,
        "action": "life.gather",
        "param": "outputs",
        "activity": "gathering",
        "considered": [
            {"node": "Critical thirst", "rejected": "life.critical_thirst is false"},
            {"node": "Critical hunger", "rejected": "life.critical_hunger is false"},
            {"node": "Player orders", "rejected": "has_unit_order is false"},
            {"node": "Current activity", "rejected": "life.busy is false"},
            {"node": "Thirst", "rejected": "life.thirsty is false"},
            {"node": "Hunger", "rejected": "life.hungry is false"},
            {"node": "Tired", "rejected": "life.tired is false"},
            {"node": "Cold", "rejected": "life.cold is false"},
            {"node": "Bedtime", "rejected": "life.bedtime is false"},
            {"node": "Meal time", "rejected": "life.wants_meal is false"},
            {"node": "Chores", "rejected": "life.collect_found: nothing lost within reach; \
                life.equip: a coat is already worn or none is carried; life.needs_fuel is false; \
                life.has_spare_coins is false; life.buy clothes: nothing wanted is on offer at an \
                affordable price"},
            {"node": "Leisure", "rejected": "life.wants_leisure is false"},
            {"node": "Consider a new job",
             "rejected": "life.change_job: no requested or better-paid vacancy"},
            {"node": "Build", "rejected": "life.build: no building commission"},
            {"node": "Pursue production goal",
             "rejected": "life.pursue_goal: no funded production goal has an executable plan"},
            {"node": "Craft",
             "rejected": "life.craft: no funded recipe has its inputs in the pack"},
            {"node": "Hunt",
             "rejected": "life.hunt: not a hunter with a spear, funded demand and prey in reach"},
            {"node": "Buy materials",
             "rejected": "life.buy inputs: nothing wanted is on offer at an affordable price"},
        ],
    })
}

#[test]
fn a_work_record_names_the_choice_and_the_nearest_options_that_fell_through() {
    let why = why(&work_record(), &WhyContext::plain(&names));
    assert_eq!(why.chosen, "Gathering, because it is their working time.");
    assert_eq!(
        why.alternatives,
        [
            "Working to a plan: no paying order they can fill from where they stand.",
            "Crafting at the workplace: they lack the ingredients for anything that would sell.",
            "Buying work materials: nothing they want is for sale at a price they can pay.",
        ],
        "the three options just above the chosen one, without those that are nobody's business"
    );
    assert_eq!(
        why.ruled_out.as_deref(),
        Some(
            "Checked first: not thirsty, not hungry, not tired, not cold, not bedtime yet, no \
             meal due, no free time now."
        )
    );
    assert_eq!(why.plan, None);
    let text = why.text();
    assert_plain(&text);
    assert!(text.contains("Would have come first:\n• Working to a plan"));
    // Hunting is somebody's business for a hunter.
    let hunter = WhyContext {
        hunter: true,
        ..WhyContext::plain(&names)
    };
    assert!(
        super::model::why(&work_record(), &hunter)
            .alternatives
            .iter()
            .any(|line| line.starts_with("Hunting: it takes a spear"))
    );
}

#[test]
fn a_record_with_nothing_passed_over_says_only_what_was_chosen() {
    let record = json!({
        "branch": "Resident life > Critical thirst > Quench thirst now > Drink",
        "action": "life.drink",
        "param": "",
        "activity": "drinking",
        "considered": [],
    });
    let why = why(&record, &WhyContext::plain(&names));
    assert_eq!(
        why.chosen,
        "Drinking, because they are dangerously thirsty."
    );
    assert!(why.alternatives.is_empty());
    assert_eq!(why.ruled_out, None);
    assert_eq!(
        why.text(),
        "Drinking, because they are dangerously thirsty."
    );
    // A leaf directly under the root, and a record without an activity label.
    let idle = json!({
        "branch": "Resident life > Idle",
        "action": "life.idle",
        "param": "",
        "activity": null,
        "considered": [{"node": "Night", "rejected": "life.in_sleep_window is false"}],
    });
    let why = super::model::why(&idle, &WhyContext::plain(&names));
    assert_eq!(
        why.chosen,
        "Standing idle, because nothing needs doing right now."
    );
    assert_eq!(why.ruled_out.as_deref(), Some("Checked first: not night."));
}

#[test]
fn a_plan_record_names_the_goal_and_the_steps_left() {
    let record = json!({
        "branch": "Resident life > Work hours > Profession > Pursue production goal",
        "action": "life.pursue_goal",
        "param": "",
        "activity": "working to a plan",
        "considered": [
            {"node": "Consider a new job",
             "rejected": "life.change_job: no requested or better-paid vacancy"},
            {"node": "Build", "rejected": "life.build: no building commission"},
        ],
        "plan": {"goal": "village_bread", "steps": 3, "first": "buy village_flour",
                 "cost": 412, "expanded": 17},
    });
    let why = why(&record, &WhyContext::plain(&names));
    assert_eq!(
        why.chosen,
        "Working to a plan, because it is their working time."
    );
    assert!(
        why.alternatives.is_empty(),
        "a job change and a commission are nobody's business here: {:?}",
        why.alternatives
    );
    assert_eq!(
        why.plan.as_deref(),
        Some("Plan: supply bread, 3 steps to go. Next: buy flour.")
    );
    assert_plain(&why.text());
    // The plan the simulation holds now is more current than the record's.
    let live = WhyContext {
        plan: Some(PlanView {
            goal: "bread".into(),
            steps: vec!["bake bread".into()],
            remaining: 1,
        }),
        ..WhyContext::plain(&names)
    };
    assert_eq!(
        super::model::why(&record, &live).plan.as_deref(),
        Some("Plan: supply bread, 1 step to go. Next: bake bread.")
    );
    assert_eq!(
        plan_line(&PlanView {
            goal: "planks".into(),
            steps: vec!["gather logs".into(), "saw planks".into()],
            remaining: 2,
        }),
        "Plan: supply planks, 2 steps to go. Next: gather logs, then saw planks."
    );
    assert_eq!(
        plan_line(&PlanView {
            goal: "leather".into(),
            steps: vec!["buy hides".into(), "buy hides".into()],
            remaining: 3,
        }),
        "Plan: supply leather, 3 steps to go. Next: buy hides, twice."
    );
    // A remembered plan is not why somebody is asleep.
    let asleep = json!({
        "branch": "Resident life > Tired > Sleep at home",
        "action": "life.sleep", "param": "", "activity": "resting at home",
        "considered": [],
    });
    assert_eq!(super::model::why(&asleep, &live).plan, None);
}

#[test]
fn carrying_on_is_explained_by_what_began_the_activity() {
    let carry_on = json!({
        "branch": "Resident life > Current activity > Carry on",
        "action": "life.continue",
        "param": "",
        "activity": "finishing the current activity",
        "considered": [
            {"node": "Critical thirst", "rejected": "life.critical_thirst is false"},
            {"node": "Critical hunger", "rejected": "life.critical_hunger is false"},
            {"node": "Player orders", "rejected": "has_unit_order is false"},
        ],
    });
    let began = json!({
        "branch": "Resident life > Tired > Sleep at home",
        "action": "life.sleep",
        "param": "",
        "activity": "resting at home",
        "considered": [
            {"node": "Thirst", "rejected": "life.thirsty is false"},
            {"node": "Hunger", "rejected": "life.hungry holds, but life.eat: no food carried; \
                life.buy food: no affordable seller offers it; life.gather food: no reachable \
                patch has any left, or the pack is full"},
        ],
    });
    let context = WhyContext {
        origin: Some(&began),
        ..WhyContext::plain(&names)
    };
    let why = why(&carry_on, &context);
    assert_eq!(
        why.chosen,
        "Still resting at home. It began because they were tired, and nothing more urgent has \
         come up."
    );
    assert_eq!(
        why.alternatives,
        [
            "Eating: they carry no food.",
            "Buying food: nobody sells it at a price they can pay.",
            "Foraging: no patch in reach has any left, or their pack is full.",
        ]
    );
    assert_eq!(
        why.ruled_out.as_deref(),
        Some("Checked first: not thirsty.")
    );
    assert_plain(&why.text());
    // The card's own words for the present activity are used when given.
    let asleep = WhyContext {
        origin: Some(&began),
        activity: Some("sleeping".into()),
        ..WhyContext::plain(&names)
    };
    assert!(
        super::model::why(&carry_on, &asleep)
            .chosen
            .starts_with("Still sleeping. It began because they were tired")
    );
    // Without the record that began it (a session opened mid-activity).
    let alone = super::model::why(&carry_on, &WhyContext::plain(&names));
    assert_eq!(
        alone.chosen,
        "Finishing what they had started; nothing more urgent has come up."
    );
    assert_eq!(
        alone.ruled_out.as_deref(),
        Some("Checked first: not dangerously thirsty, not starving.")
    );
}

#[test]
fn a_low_need_whose_branch_was_closed_is_explained_not_denied() {
    // `life.thirsty` also needs a way to drink: false does not mean "not thirsty".
    let record = json!({
        "branch": "Resident life > Work hours > Profession > Gather",
        "action": "life.gather", "param": "outputs", "activity": "gathering",
        "considered": [
            {"node": "Critical thirst", "rejected": "life.critical_thirst is false"},
            {"node": "Thirst", "rejected": "life.thirsty is false"},
            {"node": "Hunger", "rejected": "life.hungry is false"},
            {"node": "Cold", "rejected": "life.cold is false"},
        ],
    });
    let context = WhyContext {
        thirst: NeedLevel::Critical,
        warmth: NeedLevel::Low,
        ..WhyContext::plain(&names)
    };
    let why = why(&record, &context);
    assert_eq!(
        why.alternatives[0],
        "Drinking: thirsty, but no water is carried, for sale at a price they can pay, or in \
         reach."
    );
    assert!(why.alternatives[1].starts_with("Warming up at home: cold, but"));
    assert_eq!(why.ruled_out.as_deref(), Some("Checked first: not hungry."));
}

#[test]
fn weighed_options_and_unknown_names_stay_readable() {
    let record = json!({
        "branch": "Resident life > Market day > Pick a stall > Browse",
        "action": "life.browse_stalls",
        "param": "fish_market",
        "considered": [
            {"node": "Eat", "score": 72_000, "chosen": true},
            {"node": "Drink", "score": 40_000, "chosen": false},
            {"node": "Haggle", "rejected": "life.haggle: the buyer refused the trade"},
        ],
    });
    let why = why(&record, &WhyContext::plain(&names));
    assert_eq!(
        why.chosen,
        "Browse stalls (fish market), because of the rule \u{201c}Market day\u{201d}."
    );
    assert_eq!(
        why.alternatives,
        [
            "Haggle: the buyer refused the trade.",
            "Drink: it mattered less just then (40000 against 72000).",
        ]
    );
    assert_eq!(action_phrase("life.buy", "water"), "buying water");
    assert_eq!(
        action_phrase("life.go_to", "centre"),
        "strolling to the square"
    );
}

#[test]
fn traits_read_as_four_short_words() {
    let early = ResidentTraits {
        diligence: 90,
        sociability: 10,
        chronotype: -80,
        thrift: 95,
    };
    assert_eq!(
        trait_words(early, 35),
        [
            "early riser",
            "hard-working",
            "keeps to themself",
            "thrifty"
        ]
    );
    let late = ResidentTraits {
        diligence: 20,
        sociability: 80,
        chronotype: 70,
        thrift: 5,
    };
    assert_eq!(
        trait_words(late, 35),
        ["night owl", "easy-going", "sociable", "free-spending"]
    );
    assert_eq!(
        trait_words(ResidentTraits::NEUTRAL, 35),
        [
            "keeps regular hours",
            "steady worker",
            "friendly",
            "careful with money"
        ]
    );
}

#[test]
fn need_levels_follow_the_thresholds() {
    assert_eq!(need_level(80_000, 50_000, 15_000), NeedLevel::Fine);
    assert_eq!(need_level(50_000, 50_000, 15_000), NeedLevel::Fine);
    assert_eq!(need_level(49_999, 50_000, 15_000), NeedLevel::Low);
    assert_eq!(need_level(14_999, 50_000, 15_000), NeedLevel::Critical);
    assert!(NeedLevel::Critical > NeedLevel::Low && NeedLevel::Low > NeedLevel::Fine);
    assert_eq!(percent_of(100_500), 100);
    assert_eq!(percent_of(-4), 0);
    assert_eq!(percent_of(14_999), 14);
}

#[test]
fn the_day_strip_follows_the_schedule_and_the_residents_own_hours() {
    let spec = ScheduleSpec::default();
    let plan = day_plan(&spec, ResidentTraits::NEUTRAL);
    let slots: Vec<(Slot, i32, i32)> = plan
        .iter()
        .map(|segment| (segment.slot, segment.from, segment.to))
        .collect();
    assert_eq!(
        slots,
        [
            (Slot::Work, 0, 250),
            (Slot::Meal, 250, 300),
            (Slot::Work, 300, 600),
            (Slot::Meal, 600, 650),
            (Slot::Leisure, 650, 700),
            (Slot::Sleep, 700, 1000),
        ],
        "a meal shows inside working hours; the segments cover the whole day"
    );
    assert_eq!(clock_text(0), "06:00");
    assert_eq!(clock_text(250), "12:00");
    assert_eq!(clock_text(700), "22:48");
    assert_eq!(
        schedule_caption(&plan, 100),
        "Work now · meal 12:00 · work 13:12 · meal 20:24"
    );
    assert_eq!(
        schedule_caption(&plan, 800),
        "Sleep now · work 06:00 · meal 12:00 · work 13:12"
    );
    assert_eq!(current_segment(&plan, 650), Some(4));
    // A night owl's whole day is later; the sleep run wraps past sunrise.
    let owl = ResidentTraits {
        chronotype: 100,
        ..ResidentTraits::NEUTRAL
    };
    let late = day_plan(&spec, owl);
    assert_eq!(
        (late[0].slot, late[0].from, late[0].to),
        (Slot::Sleep, 0, 25)
    );
    assert_eq!(late[1].slot, Slot::Work);
    assert!(
        schedule_caption(&late, 990).starts_with("Sleep now · work 06:36"),
        "{}",
        schedule_caption(&late, 990)
    );
    // The diligent start their free time later; at the limit they take none.
    let keen = ResidentTraits {
        diligence: 100,
        ..ResidentTraits::NEUTRAL
    };
    assert!(!in_slot(&spec, keen, BlockKind::Leisure, 660));
    assert!(in_slot(&spec, keen, BlockKind::Leisure, 690));
    assert!(in_slot(
        &spec,
        ResidentTraits::NEUTRAL,
        BlockKind::Leisure,
        660
    ));
}

fn living_world() -> App {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    app.init_resource::<Inspector>()
        .insert_resource(IslandMode::Inspector)
        .init_resource::<Selected>();
    app
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != Role::Visitor)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

#[test]
fn the_strip_agrees_with_the_simulations_own_schedule_test() {
    let mut app = living_world();
    for _ in 0..6 {
        run_ticks(app.world_mut(), 400);
        let world = app.world();
        let spec = resident::spec(world).expect("behaviour data");
        let now = resident::day_permille(world);
        for (id, entity) in residents(world) {
            let traits = resident::traits(world, entity);
            for kind in [
                BlockKind::Work,
                BlockKind::Meal,
                BlockKind::Leisure,
                BlockKind::Sleep,
            ] {
                assert_eq!(
                    in_slot(&spec.schedule, traits, kind, now),
                    resident::in_block(world, entity, kind),
                    "resident {} {kind:?} at {now}",
                    id.0
                );
            }
            // The strip is drawn from the simulation's own blocks; it is the
            // same day the schedule and the traits give.
            let blocks: Vec<_> = village::view::schedule_blocks(world, entity)
                .into_iter()
                .map(|block| (block.kind, block.from_permille, block.to_permille))
                .collect();
            assert_eq!(
                plan_of_blocks(&blocks),
                day_plan(&spec.schedule, traits),
                "resident {}",
                id.0
            );
        }
    }
}

#[test]
fn every_resident_gets_a_plain_card_and_reading_it_changes_nothing() {
    let mut app = living_world();
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 700);
    track(app.world_mut());
    let recorded = app.world().resource::<Inspector>().latest.len();
    assert!(
        recorded > 30,
        "only {recorded} residents had a journaled decision"
    );
    let before = state_hash(app.world_mut());
    let world = app.world();
    let mut reasons = 0;
    for (id, entity) in residents(world) {
        let why = why_text(world, id, entity);
        assert!(!why.is_empty());
        assert_no_identifiers(&why);
        reasons += usize::from(why.contains("because") || why.contains("It began because"));
        let activity = activity_label(world, entity);
        assert_no_identifiers(&activity);
        assert!(!activity.is_empty());
        let bars = needs(world, entity);
        assert_eq!(
            bars.map(|bar| bar.label),
            ["Food", "Water", "Warmth", "Rest", "Morale"]
        );
        // What they carry reads in display names, coins first.
        let (_, goods) = carried(world, entity);
        for (_, name, count) in &goods {
            assert_no_identifiers(name);
            assert!(*count > 0);
        }
    }
    assert!(reasons > 30, "only {reasons} residents had a stated reason");
    assert_eq!(state_hash(app.world_mut()), before, "the card only reads");
}

#[test]
fn the_card_follows_the_selection_while_overseeing() {
    let mut app = living_world();
    let (id, entity) = residents(app.world())[0];
    assert!(!open(app.world()), "nothing is selected");
    inspect(app.world_mut(), id);
    assert_eq!(subject(app.world()), Some((id, entity)));
    // Character play has no card; Oversee brings it back for the selection.
    app.insert_resource(IslandMode::Character);
    assert!(!open(app.world()));
    app.insert_resource(IslandMode::Inspector);
    assert!(open(app.world()));
    close(app.world_mut());
    assert!(!open(app.world()));
    assert_eq!(app.world().resource::<Selected>().0, None);
    // A settlement or a workplace is not a resident.
    let place = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| app.world().get::<Villager>(**entity).is_none())
        .map(|(id, _)| *id)
        .unwrap();
    app.world_mut().resource_mut::<Selected>().0 = Some(place);
    assert!(!open(app.world()));
}

#[test]
fn escape_closes_the_card_and_keeps_the_key_from_the_menu() {
    let mut app = living_world();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<InputFocus>();
    let (id, _) = residents(app.world())[0];
    inspect(app.world_mut(), id);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    keys(app.world_mut());
    assert!(!open(app.world()));
    assert!(
        !app.world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape),
        "the pause menu must not open on the same press"
    );
    // With no card the key is left for the ledger and the menu.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    keys(app.world_mut());
    assert!(
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape)
    );
}

#[test]
fn the_badge_names_the_most_pressing_need_in_words() {
    use crate::game_ui::tokens::Status;
    let fine = [("Food", NeedLevel::Fine), ("Warmth", NeedLevel::Fine)];
    assert_eq!(status_words(&fine), (Status::Good, "Doing fine".to_owned()));
    let low = [("Food", NeedLevel::Fine), ("Warmth", NeedLevel::Low)];
    assert_eq!(
        status_words(&low),
        (Status::Warn, "In need: warmth".to_owned())
    );
    // Danger outranks a lower need listed before it.
    let danger = [("Food", NeedLevel::Low), ("Water", NeedLevel::Critical)];
    assert_eq!(
        status_words(&danger),
        (Status::Crit, "In danger: water".to_owned())
    );
    assert!(status_sentence(Status::Crit).contains("drop everything"));
    assert_eq!(need_sentence(NeedLevel::Fine), "Fine.");
}

#[test]
fn why_splits_into_titles_reasons_and_checked_chips() {
    assert_eq!(
        title_and_reason(
            "stocking up on provisions: nothing they want is for sale at a price they can pay."
        ),
        (
            "Stocking up on provisions".to_owned(),
            Some("Nothing they want is for sale at a price they can pay.".to_owned())
        )
    );
    assert_eq!(title_and_reason("resting"), ("Resting".to_owned(), None));
    assert_eq!(
        checked_chips(
            "Checked first: not thirsty, not hungry, not tired, not cold, not bedtime yet, no meal due, no free time now."
        ),
        [
            "Thirst",
            "Hunger",
            "Tiredness",
            "Cold",
            "Bedtime",
            "Meal",
            "Free time"
        ]
    );
    // A phrase the table does not know still reads as a word.
    assert_eq!(
        checked_chips("Checked first: not night, not busy."),
        ["Night", "Busy"]
    );
    assert!(checked_chips("").is_empty());
}

#[test]
fn every_tab_lists_rows_and_job_and_home_are_commands_of_the_inspector() {
    use crate::colony_panel::{Action, Extra};
    let mut app = living_world();
    app.init_resource::<Inspector>()
        .init_resource::<IslandMode>()
        .init_resource::<Selected>();
    let before = npc_framework::kernel::hash::state_hash(app.world_mut());
    let (id, _) = residents(app.world())[0];
    app.world_mut().resource_mut::<Selected>().0 = Some(id);
    let rows = entries(app.world());
    // Overview: five needs with shapes and words, what they carry, why now.
    let meters: Vec<_> = rows
        .iter()
        .filter(|entry| matches!(entry.extra, Some(Extra::Meter { .. })))
        .collect();
    assert_eq!(meters.len(), 5);
    assert!(
        meters
            .iter()
            .all(|entry| entry.tip.is_some() && entry.icon.is_some())
    );
    assert!(rows.iter().any(|entry| entry.key == "carrying"));
    assert!(rows.iter().any(|entry| entry.key == "why_now"));
    // The Overview ends with the way to their job and home commands.
    assert_eq!(
        rows.last()
            .map(|entry| (entry.key.as_str(), entry.action.clone())),
        Some(("manage_link", Some(Action::InspectorTab(Tab::HomeAndJob))))
    );
    for tab in Tab::ALL {
        app.world_mut().resource_mut::<Inspector>().tab = tab;
        let rows = entries(app.world());
        assert!(!rows.is_empty(), "{tab:?}");
        for entry in &rows {
            assert_no_identifiers(&entry.text);
        }
    }
    // Home and job: the recorded commands live here now.
    app.world_mut().resource_mut::<Inspector>().tab = Tab::HomeAndJob;
    let rows = entries(app.world());
    assert!(
        rows.iter().any(|entry| matches!(
            entry.action,
            Some(Action::Home(..)) | Some(Action::Profession(..))
        )) || rows.iter().any(|entry| entry.key == "current_home"),
        "a home or a job can be arranged from the inspector"
    );
    // Day: the strip and what comes next.
    app.world_mut().resource_mut::<Inspector>().tab = Tab::Day;
    let rows = entries(app.world());
    assert!(matches!(rows[0].extra, Some(Extra::Strip(..))));
    assert_eq!(
        npc_framework::kernel::hash::state_hash(app.world_mut()),
        before,
        "reading the inspector changes nothing"
    );
}

#[test]
fn the_decision_view_reads_as_the_journal_record_does() {
    use testbed_content::village::resident::{ConsideredOption, DecisionView};
    let option = |node: &str, condition: Option<&str>, does_not_apply, reason: Option<&str>| {
        ConsideredOption {
            node: node.into(),
            does_not_apply,
            condition: condition.map(str::to_owned),
            reason: reason.map(str::to_owned),
            score: None,
            chosen: false,
        }
    };
    let view = DecisionView {
        tick: Some(120),
        recorded: true,
        tree: "resident".into(),
        branch: vec!["life".into(), "work".into(), "plan".into()],
        node: 14,
        action: "life.pursue_goal".into(),
        param: "work".into(),
        activity: Some("working to a plan"),
        considered: vec![
            option("thirst", Some("life.thirsty"), true, None),
            option(
                "harvest",
                Some("life.work_time"),
                false,
                Some("life.farm harvest: no plot is ripe"),
            ),
            ConsideredOption {
                score: Some(40),
                chosen: true,
                ..option("rest", None, false, None)
            },
        ],
        detail: Some(json!({"plan": {"goal": "fiber", "steps": 1, "first": "gather fiber"}})),
    };
    let record = record_of(&view);
    assert_eq!(record["branch"], "life > work > plan");
    assert_eq!(record["node"], 14);
    assert_eq!(record["action"], "life.pursue_goal");
    assert_eq!(record["activity"], "working to a plan");
    assert_eq!(record["considered"][0]["rejected"], "life.thirsty is false");
    assert_eq!(
        record["considered"][1]["rejected"],
        "life.work_time holds, but life.farm harvest: no plot is ripe"
    );
    assert_eq!(record["considered"][2]["score"], 40);
    assert_eq!(record["considered"][2]["chosen"], true);
    assert_eq!(
        record["plan"]["goal"], "fiber",
        "the detail sits beside the fields"
    );
}

#[test]
fn a_planned_purchase_names_its_seller() {
    assert_eq!(
        purchase_sentence("Bread", Some("Harbour market stall 1")),
        "They will buy bread at Harbour market stall 1."
    );
    assert_eq!(
        purchase_sentence("Planks", None),
        "Nobody offers planks at a price they can pay right now."
    );
}

#[test]
fn an_inspector_lets_go_of_somebody_who_was_laid_to_rest() {
    assert_eq!(
        gone_sentence(Some("Bram")),
        "Bram has been laid to rest. They are remembered on the People page."
    );
    assert_eq!(gone_sentence(None), "They are no longer on the island.");
}

#[test]
fn family_and_bed_read_a_fact_per_line() {
    use testbed_content::village::view::Repose;
    let family = Family {
        label: String::new(),
        stage: "adult".into(),
        age_years: 24,
        partner: Some("Bram".into()),
        courting: Some("Odo".into()),
        children: vec!["Ivy (4)".into(), "Tom (1)".into()],
        parents: vec!["Mara".into()],
    };
    assert_eq!(
        family_lines(&family),
        [
            "Adult \u{b7} 24 years old",
            "Partner: Bram",
            "Children: Ivy (4), Tom (1)",
            "Parents: Mara"
        ],
        "a partner is named; who they courted before no longer is"
    );
    let single = Family {
        stage: "elder".into(),
        age_years: 1,
        courting: Some("Odo".into()),
        ..Family::default()
    };
    assert_eq!(
        family_lines(&single),
        ["Elder \u{b7} 1 year old", "Courting: Odo"]
    );
    assert!(family_lines(&Family::default()).is_empty());
    // The simulation's age words come first when it has them.
    let labelled = Family {
        label: "Adult \u{b7} 34".into(),
        stage: "adult".into(),
        age_years: 2,
        ..Family::default()
    };
    assert_eq!(family_lines(&labelled), ["Adult \u{b7} 34"]);
    assert_eq!(
        bed_line("Harbour cottage 1", 0, 2, Repose::Asleep),
        "Bed 1 of 2 in Harbour cottage 1 \u{b7} asleep"
    );
    assert_eq!(
        bed_line("Harbour cottage 1", 1, 2, Repose::Resting),
        "Bed 2 of 2 in Harbour cottage 1 \u{b7} resting"
    );
    assert_eq!(bed_line("Inn", 3, 4, Repose::Awake), "Bed 4 of 4 in Inn");
    assert_eq!(repose_word(Repose::Awake), None);
}

#[test]
fn a_day_from_the_simulations_blocks_shows_the_narrower_block_and_wraps() {
    let blocks = [
        (BlockKind::Work, 100, 600),
        (BlockKind::Meal, 300, 340),
        (BlockKind::Sleep, 800, 50),
    ];
    let plan = plan_of_blocks(&blocks);
    let slot_at = |permille: i32| plan[current_segment(&plan, permille).unwrap()].slot;
    assert_eq!(slot_at(0), Slot::Sleep, "sleep wraps past sunrise");
    assert_eq!(slot_at(60), Slot::Free);
    assert_eq!(slot_at(200), Slot::Work);
    assert_eq!(
        slot_at(320),
        Slot::Meal,
        "a meal inside working hours shows"
    );
    assert_eq!(slot_at(700), Slot::Free);
    assert_eq!(slot_at(900), Slot::Sleep);
    assert_eq!(plan.first().unwrap().from, 0);
    assert_eq!(plan.last().unwrap().to, 1000);
    assert!(
        plan_of_blocks(&[])
            .iter()
            .all(|segment| segment.slot == Slot::Free)
    );
}

#[test]
fn the_friends_list_names_five_bonds_with_their_tier() {
    let bonds: Vec<(String, String, bool)> = [
        ("Traveller", "friend", true),
        ("Bram", "acquaintance", false),
        ("Odo", "stranger", false),
        ("", "friend", false),
        ("Ivy", "close friend", false),
        ("Mara", "stranger", false),
        ("Tom", "stranger", false),
    ]
    .into_iter()
    .map(|(name, tier, played)| (name.to_owned(), tier.to_owned(), played))
    .collect();
    assert_eq!(
        super::model::friend_lines(&bonds),
        [
            "Traveller (you) \u{b7} Friend",
            "Bram \u{b7} Acquaintance",
            "Odo \u{b7} Met",
            "Ivy \u{b7} Close friend",
            "Mara \u{b7} Met",
        ]
    );
    assert!(super::model::friend_lines(&[]).is_empty());
}

#[test]
fn asleep_at_the_inn_keeps_its_own_words_like_asleep_at_home() {
    use super::own_rest_label;
    assert_eq!(own_rest_label("asleep at home"), Some("asleep at home"));
    assert_eq!(
        own_rest_label("asleep at the inn"),
        Some("asleep at the inn")
    );
    assert_eq!(own_rest_label("resting at home"), Some("resting at home"));
    assert_eq!(
        own_rest_label("warming up at home"),
        Some("warming up at home")
    );
    assert_eq!(own_rest_label("going home"), None);
}
