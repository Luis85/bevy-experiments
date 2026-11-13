use super::*;
use npc_framework::kernel::hash::state_hash;
use serde_json::json;

#[test]
fn regional_map_disabled_keeps_landscape_care_without_rendering_a_chart() {
    let world = World::new();
    let observed = json!({"sections":{"living_world":{"chart":[],"nearby_sites":[{
        "id":17,"kind":"log","remaining":10,"capacity":40,"position":{"x":0,"z":0},
        "vitality_percent":60,"soil_percent":80,"disturbance_percent":20,
        "actions":[{"verb":"plant_tree","available":true}]
    }]}}});
    let state = PlayerState {
        tab: Some(Tab::Explore),
        ..default()
    };
    let rows = model::rows(&world, &observed, &state, true);
    assert!(
        rows.iter()
            .all(|row| row.key != "regional_chart" && !row.key.starts_with("destination_"))
    );
    assert!(
        rows.iter()
            .any(|row| row.key == "plant_tree_17" && row.enabled)
    );
}

#[test]
fn player_camera_anchor_stays_centered_clear_of_header_workspace_and_toolbar() {
    for (width, height, workspace) in [(1024., 768., true), (1280., 720., false)] {
        let viewport = Rect::new(0., 0., width, height);
        let mut panels = vec![
            Rect::new(12., 12., width - 12., 74.),
            Rect::new(12., 86., 282., 232.),
            Rect::new(12., height - 66., width - 12., height - 12.),
            Rect::new(width * 0.55, height - 161., width - 12., height - 78.),
        ];
        if workspace {
            panels.push(Rect::new(width - 442., 86., width - 12., height - 173.));
        }
        let anchor = crate::village_world::player_screen_anchor(viewport, &panels);
        assert!(
            panels
                .iter()
                .all(|panel| !panel.inflate(12.).contains(anchor))
        );
        assert!(
            anchor.y < height * 0.75,
            "hero must not be pinned to the toolbar edge: {anchor:?}"
        );
        assert!(anchor.y > 150.);
    }
}

#[test]
fn player_tab_navigation_respects_foreign_controls_and_can_start_from_window_focus() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input_focus::tab_navigation::TabIndex;
    let mut world = World::new();
    world.insert_resource(PlayerExperience { enabled: true });
    world.insert_resource(IslandMode::Character);
    world.insert_resource(PlayerState {
        tab: Some(Tab::Pouch),
        ..default()
    });
    world.init_resource::<InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Tab);
    let player = world
        .spawn((WorkspaceRoot, Node::default(), TabGroup::new(7)))
        .id();
    let first = world
        .spawn((
            ChildOf(player),
            PlayerControl,
            TabIndex(1),
            InheritedVisibility::VISIBLE,
        ))
        .id();
    let second = world
        .spawn((
            ChildOf(player),
            PlayerControl,
            TabIndex(2),
            InheritedVisibility::VISIBLE,
        ))
        .id();
    let foreign = world.spawn((Node::default(), TabGroup::new(8))).id();
    let field = world
        .spawn((ChildOf(foreign), TabIndex(1), InheritedVisibility::VISIBLE))
        .id();
    world.spawn((ChildOf(foreign), TabIndex(2), InheritedVisibility::VISIBLE));
    world
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Navigated);
    world.run_system_once(keyboard_navigation).unwrap();
    assert_eq!(
        world.resource::<InputFocus>().get(),
        Some(field),
        "the other panel owns its Tab navigation"
    );
    world
        .resource_mut::<InputFocus>()
        .set(first, FocusCause::Navigated);
    world.run_system_once(keyboard_navigation).unwrap();
    assert_eq!(world.resource::<InputFocus>().get(), Some(second));
    let window = world.spawn(Window::default()).id();
    world
        .resource_mut::<InputFocus>()
        .set(window, FocusCause::Navigated);
    world.run_system_once(keyboard_navigation).unwrap();
    assert_eq!(
        world.resource::<InputFocus>().get(),
        Some(first),
        "the ordinary window focus must initialize the open workspace"
    );
}

#[test]
fn undiscovered_world_offers_exploration_without_inventing_claims_or_targets() {
    let mut app = island_content::new_sim_app(7, "island_colony");
    let observed = json!({"unit":42,"sections":{"self":{"pos":[1200,3400]},"village":{"environment":{}},"homestead":{"plots":[]}}});
    let before = state_hash(app.world_mut());
    for tab in [Tab::Craft, Tab::Build, Tab::Explore] {
        let state = PlayerState {
            tab: Some(tab),
            ..default()
        };
        let rows = model::rows(app.world(), &observed, &state, true);
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row.action, Some(Action::Walk(_))))
                .count(),
            if tab == Tab::Explore { 4 } else { 0 }
        );
        if tab != Tab::Explore {
            assert!(
                rows.iter()
                    .any(|row| row.action == Some(Action::Open(Tab::Explore))),
                "workspaces must keep exploration directly reachable"
            );
        }
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row.action, Some(Action::Work(_))))
        );
    }
    assert!(model::context(app.world(), &observed, Some(SimId(u32::MAX)), true).is_none());
    // Reading the goal and what its button would do changes nothing.
    let _ = island_content::village::player_journey::project(&observed);
    let _ = actions::goal_action(app.world(), &observed);
    assert_eq!(before, state_hash(app.world_mut()));
    let observed = json!({"sections":{"homestead":{"plots":[{"id":77,"use_kind":"housing","name":"Remembered site","state_known":false,"owner":null,"front_door":[100,200]}]}}});
    let state = PlayerState {
        tab: Some(Tab::Build),
        plot: Some(77),
        ..default()
    };
    let rows = model::rows(app.world(), &observed, &state, true);
    assert!(
        !rows
            .iter()
            .find(|row| row.key == "claim_site")
            .unwrap()
            .enabled
    );
    assert!(
        rows.iter()
            .any(|row| matches!(row.action, Some(Action::Walk(_))))
    );
}

#[test]
fn trade_quotes_and_physical_inventory_are_carried_into_actions() {
    let app = island_content::new_sim_app(7, "island_colony");
    let observed = json!({"sections":{"inventory":{"items":[{"id":1,"kind":"village_warm_clothes","stack":1,"slot":"body"},{"id":2,"kind":"log","stack":4}]},"village":{"nearby_market":[{"id":55,"name":"Neighbour","offers":[{"kind":"log","direction":"buy","unit_price":17,"available":true}]}]}}});
    let stock = model::inventory(&observed);
    assert_eq!(stock.get("log"), Some(&4));
    assert!(!stock.contains_key("village_warm_clothes"));
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::Trade),
            ..default()
        },
        true,
    );
    let request = rows
        .iter()
        .find_map(|row| match &row.action {
            Some(Action::Work(work)) if work.verb == "buy" => Some(work),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.target, Some(55));
    assert_eq!(request.args["limit_price"], 17);
    assert_eq!(request.args["quantity"], 1);
    assert!(
        rows.iter()
            .find(|row| row.key == "trade_55_buy_log")
            .unwrap()
            .detail
            .contains("17 coins")
    );
}

#[test]
fn workspace_reset_despawns_old_actor_actions_instead_of_detaching_them() {
    let mut world = World::new();
    world.init_resource::<PlayerState>();
    world.init_resource::<InputFocus>();
    let body = world.spawn((WorkspaceBody, Node::default())).id();
    for generation in 0..3 {
        let observed =
            json!({"sections":{"self":{"pos":[generation*100,0]},"homestead":{"plots":[]}}});
        world.resource_mut::<PlayerState>().tab = Some(Tab::Build);
        let rows = model::rows(&world, &observed, world.resource::<PlayerState>(), true);
        reconcile(&mut world, rows);
        let old = world.get::<Children>(body).unwrap().to_vec();
        let focused = old
            .iter()
            .copied()
            .find(|entity| world.get::<PlayerAction>(*entity).is_some())
            .unwrap();
        world
            .resource_mut::<InputFocus>()
            .set(focused, FocusCause::Navigated);
        clear(&mut world);
        assert!(old.iter().all(|entity| world.get_entity(*entity).is_err()));
        assert!(
            world
                .get::<Children>(body)
                .is_none_or(|children| children.is_empty())
        );
        assert!(world.query::<&PlayerAction>().iter(&world).next().is_none());
        assert!(world.resource::<InputFocus>().get().is_none());
    }
}

#[test]
fn host_overlay_hides_game_roots_without_erasing_host_focus_or_original_display() {
    let mut world = World::new();
    world.insert_resource(FrontendGate(true));
    world.init_resource::<HiddenRoots>();
    world.init_resource::<InputFocus>();
    let normal = world
        .spawn(Node {
            display: Display::Grid,
            ..default()
        })
        .id();
    let hidden = world
        .spawn(Node {
            display: Display::None,
            ..default()
        })
        .id();
    let host = world.spawn((FrontendUiRoot, Node::default())).id();
    world
        .resource_mut::<InputFocus>()
        .set(host, FocusCause::Navigated);
    hide_roots(&mut world);
    hide_roots(&mut world);
    assert_eq!(world.get::<Node>(normal).unwrap().display, Display::None);
    assert_eq!(world.get::<Node>(host).unwrap().display, Display::Flex);
    assert_eq!(world.resource::<InputFocus>().get(), Some(host));
    world.resource_mut::<FrontendGate>().0 = false;
    restore_roots(&mut world);
    assert_eq!(world.get::<Node>(normal).unwrap().display, Display::Grid);
    assert_eq!(world.get::<Node>(hidden).unwrap().display, Display::None);
}

#[test]
fn people_use_authored_topics_and_known_positions_only() {
    let app = island_content::new_sim_app(7, "island_colony");
    let observed = json!({"unit":42,"sections":{"perception":{"remembered":[{"id":5,"kind":"actor","name":"Ada","pos":[1200,3400]}]},"village":{"social":{"relationships":[{"id":5,"name":"Ada","affinity":17000}],"topics":[{"id":"garden","label":"Growing crops","text":"How do I start a garden?"}]}}}});
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::People),
            person: Some(5),
            ..default()
        },
        true,
    );
    let request = rows
        .iter()
        .find_map(|row| match &row.action {
            Some(Action::Work(work)) if row.key == "topic_garden" => Some(work),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.target, Some(5));
    assert_eq!(request.approach, Some(SimPos::new(1200, 3400)));
    assert_eq!(request.args["text"], "How do I start a garden?");
    assert!(rows.iter().any(|row| row.detail.contains("17%")));
}

#[test]
fn needs_warn_with_a_shape_and_keep_their_percentage_for_the_tooltip() {
    use crate::game_ui::tokens::Status;
    assert_eq!(hud::need_status(100_000), Status::Good);
    assert_eq!(hud::need_status(45_000), Status::Good);
    assert_eq!(hud::need_status(44_999), Status::Warn);
    assert_eq!(hud::need_status(19_999), Status::Crit);
    assert_eq!(hud::need_sentence("Water", 83_000), "Water is at 83%.");
    assert_eq!(
        hud::need_sentence("Water", 30_000),
        "Water is at 30%: low. See to it soon."
    );
    assert!(hud::need_sentence("Food", 5_000).contains("dangerously low"));
    assert_eq!(
        hud::NEEDS.map(|(_, name, _)| name),
        ["Health", "Food", "Water", "Energy", "Warmth"]
    );
}

#[test]
fn the_hotbar_is_centred_and_the_workspace_takes_the_specs_widths() {
    use crate::game_ui::scale::LayoutClass;
    let (left, tile, total) = hud::hotbar_rect(1280.0, LayoutClass::Wide);
    assert_eq!(tile, 64.0);
    assert_eq!(left, ((1280.0 - total) * 0.5_f32).round());
    assert!(left > 232.0, "clear of the needs panel, which ends at 224");
    let (left, tile, total) = hud::hotbar_rect(1024.0, LayoutClass::Standard);
    assert_eq!(tile, 56.0);
    assert!(
        left > 232.0 && left + total < 1024.0 - 8.0,
        "{left} {total}"
    );
    // One pane, or two for Craft and Trade with the pouch beside them.
    assert_eq!(
        workspace::width(Tab::Pouch, LayoutClass::Wide),
        (360.0, None)
    );
    assert_eq!(
        workspace::width(Tab::Craft, LayoutClass::Wide),
        (600.0, Some(276.0))
    );
    assert_eq!(
        workspace::width(Tab::Trade, LayoutClass::Standard),
        (540.0, Some(244.0))
    );
    assert_eq!(
        workspace::width(Tab::People, LayoutClass::Standard),
        (340.0, None)
    );
    assert_eq!(
        workspace::width(Tab::Craft, LayoutClass::Compact),
        (320.0, None)
    );
    assert_eq!(Tab::ALL.map(Tab::key), ["I", "C", "B", "G", "T", "N", "M"]);
    assert_eq!(Tab::ALL.map(Tab::label)[0], "Pouch");
}

#[test]
fn every_picture_only_control_of_the_live_hud_has_a_tooltip() {
    let mut world = World::new();
    world.init_resource::<PlayerState>();
    world.init_resource::<hud::Hud>();
    world.init_resource::<workspace::Frame>();
    hud::spawn(&mut world);
    workspace::spawn(&mut world);
    assert_eq!(
        crate::game_ui::widgets::unexplained_icon_controls(&mut world),
        Vec::<String>::new()
    );
}

#[test]
fn workspace_rows_and_recipe_chips_explain_themselves_on_two_levels() {
    let stock = BTreeMap::from([("village_plank".to_owned(), 1_u64)]);
    let inputs = model::recipe_inputs(&stock, &json!([{"kind":"village_plank","count":3}]));
    let world = World::new();
    let observed = json!({"sections":{
    "inventory":{"items":[{"id":5,"kind":"village_plank","stack":1}]},
    "village":{"environment":{"workstations":[{"id":9,"name":"Carpenter's bench","recipes":[
        {"name":"chest","inputs":[{"kind":"village_plank","count":3}],"outputs":[{"kind":"chest","count":1}],"duration_ticks":400}
    ]}]}}}});
    let state = PlayerState {
        tab: Some(Tab::Craft),
        ..default()
    };
    let rows = model::rows(&world, &observed, &state, true);
    let chest = rows.iter().find(|row| row.key == "recipe_9_chest").unwrap();
    let tip = workspace::row_tip(chest);
    assert_eq!(
        tip.levels(),
        2,
        "what it does, then what it takes and why not"
    );
    // A row with nothing more to say has one level.
    let plain = rows.iter().find(|row| row.key == "craft_all").unwrap();
    assert_eq!(workspace::row_tip(plain).levels(), 1);
    let (name, owned, wanted) = &inputs[0];
    assert_eq!(workspace::chip_tip(name, *owned, *wanted).levels(), 1);
    assert_ne!(
        workspace::chip_tip(name, *owned, *wanted),
        workspace::chip_tip(name, 3, 3),
        "a missing input says how many more"
    );
}

#[test]
fn the_prompt_keeps_clear_of_a_wide_workspace() {
    // Above the hotbar, centred, while nothing wide is open.
    assert_eq!(hud::prompt_lane(1280.0, 0.0), (0.0, 0.0, 80.0));
    assert_eq!(hud::prompt_lane(1280.0, 360.0), (0.0, 0.0, 80.0));
    assert_eq!(hud::prompt_lane(1024.0, 340.0), (0.0, 0.0, 80.0));
    // Beside two panes: above the needs, in what the workspace leaves free.
    let (left, right, bottom) = hud::prompt_lane(1280.0, 600.0);
    assert_eq!((left, right), (8.0, 608.0));
    assert!(bottom > 160.0, "above the needs: {bottom}");
    let (left, right, _) = hud::prompt_lane(1024.0, 540.0);
    assert!(1024.0 - left - right >= 340.0, "room for the longest pill");
}

#[test]
fn the_prompt_is_there_only_for_something_in_reach_or_under_way() {
    use hud::Prompt;
    assert_eq!(
        hud::prompt(None, None, None),
        Prompt::None,
        "no box while nothing is in reach"
    );
    assert_eq!(
        hud::prompt(Some(("Draw water from the well", true)), None, None),
        Prompt::Reach {
            text: "Draw water from the well".into(),
            enabled: true
        }
    );
    // Work under way comes before what is in reach.
    assert_eq!(
        hud::prompt(Some(("Talk to Ada", true)), None, Some("Gathering")),
        Prompt::Working {
            text: "Gathering".into()
        }
    );
    assert_eq!(
        hud::prompt(None, Some("Gather one item"), Some("Walking")),
        Prompt::Working {
            text: "Walking, then: gather one item".into()
        }
    );
    assert_eq!(model::working_words("crafting"), "Making something");
    assert_eq!(model::working_words("something new"), "Busy");
    // The goal's button names what it does.
    assert_eq!(hud::goal_button(&Action::Open(Tab::Build)), "Open Build");
    assert_eq!(hud::goal_button(&Action::Goal), "Take the next step");
}

#[test]
fn a_recipe_shows_what_it_takes_and_says_what_is_missing() {
    let stock = BTreeMap::from([
        ("village_plank".to_owned(), 1_u64),
        ("thread".to_owned(), 2),
    ]);
    let inputs = model::recipe_inputs(
        &stock,
        &json!([{"kind":"village_plank","count":3},{"kind":"thread","count":2}]),
    );
    assert_eq!(
        inputs,
        [("Plank".to_owned(), 1, 3), ("Thread".to_owned(), 2, 2)]
    );
    assert_eq!(
        model::recipe_reason(&inputs, true),
        "You need 2 more plank."
    );
    let enough = [("Plank".to_owned(), 3, 3)];
    assert_eq!(
        model::recipe_reason(&enough, false),
        "You have not found the way to this workshop yet."
    );
    assert_eq!(model::recipe_reason(&enough, true), "");
    // Craft lists every recipe, or only what can be made now; a recipe that
    // cannot be made keeps its place, disabled, with its reason.
    let world = World::new();
    let observed = json!({"sections":{
    "inventory":{"items":[{"id":5,"kind":"village_plank","stack":1}]},
    "village":{"environment":{"workstations":[{"id":9,"name":"Carpenter's bench","recipes":[
        {"name":"rod","inputs":[{"kind":"village_plank","count":1}],"outputs":[{"kind":"fishing_rod","count":1}],"duration_ticks":200},
        {"name":"chest","inputs":[{"kind":"village_plank","count":4}],"outputs":[{"kind":"chest","count":1}],"duration_ticks":400}
    ]}]}}}});
    let state = PlayerState {
        tab: Some(Tab::Craft),
        ..default()
    };
    let rows = model::rows(&world, &observed, &state, true);
    let recipes: Vec<_> = rows
        .iter()
        .filter(|row| row.key.starts_with("recipe_"))
        .collect();
    assert_eq!(recipes.len(), 2);
    let chest = recipes
        .iter()
        .find(|row| row.key == "recipe_9_chest")
        .unwrap();
    assert!(!chest.enabled);
    assert_eq!(chest.inputs, [("Plank".to_owned(), 1, 4)]);
    assert_eq!(chest.reason, "You need 3 more plank.");
    assert!(
        chest.detail.ends_with("20 s at 1\u{d7}"),
        "{}",
        chest.detail
    );
    let narrowed = PlayerState {
        tab: Some(Tab::Craft),
        craft_all: false,
        ..default()
    };
    let rows = model::rows(&world, &observed, &narrowed, true);
    assert_eq!(
        rows.iter()
            .filter(|row| row.key.starts_with("recipe_"))
            .map(|row| row.key.as_str())
            .collect::<Vec<_>>(),
        ["recipe_9_rod"],
        "Can make hides what is short"
    );
    assert!(
        rows.iter()
            .any(|row| row.key == "craft_can" && row.selected)
    );
    // Workspace badges come from the observation alone.
    use crate::game_ui::tokens::Status;
    assert_eq!(
        model::badge(&observed, Tab::Craft, None),
        Some((Status::Info, "1 workshop known".to_owned()))
    );
    // Standing at a known workshop, the badge says where.
    assert_eq!(
        model::badge(&observed, Tab::Craft, Some("Harbour carpenter")),
        Some((Status::Good, "At the Harbour carpenter".to_owned()))
    );
    assert_eq!(
        model::badge(&observed, Tab::Trade, None),
        Some((Status::Idle, "No merchant in reach".to_owned()))
    );
    assert_eq!(model::badge(&observed, Tab::Pouch, None), None);
}

#[test]
fn a_pouch_item_is_chosen_first_and_then_offers_what_can_be_done_with_it() {
    let app = island_content::new_sim_app(7, "island_colony");
    let observed = json!({"sections":{
        "inventory":{"items":[{"id":5,"kind":"bread","stack":2},{"id":6,"kind":"village_plank","stack":3}]},
        "village":{}}});
    let state = PlayerState {
        tab: Some(Tab::Pouch),
        ..default()
    };
    let rows = model::rows(app.world(), &observed, &state, true);
    let items: Vec<_> = rows.iter().filter(|row| row.item).collect();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|row| matches!(row.action, Some(Action::Item(_))) && !row.selected)
    );
    assert!(
        !rows.iter().any(|row| row.key == "item_detail"),
        "nothing is offered before an item is chosen"
    );
    let chosen = PlayerState {
        tab: Some(Tab::Pouch),
        item: Some("bread".into()),
        ..default()
    };
    let rows = model::rows(app.world(), &observed, &chosen, true);
    assert!(
        rows.iter()
            .any(|row| row.key == "item_bread" && row.selected)
    );
    // One card for the chosen item, with a named button per thing to do.
    let card = rows
        .iter()
        .find(|row| row.key == "item_detail")
        .expect("the chosen item's card");
    assert_eq!(
        (card.title.as_str(), card.detail.as_str()),
        ("Bread", "2 in your pouch.")
    );
    assert!(card.action.is_none(), "the card itself does nothing");
    let eat = card.buttons.first().expect("what to do with bread");
    assert_eq!(eat.title, "Eat");
    assert!(eat.enabled && matches!(&eat.action, Action::Work(work) if work.verb == "eat"));
    // An item that is no longer carried has no detail.
    let gone = PlayerState {
        tab: Some(Tab::Pouch),
        item: Some("coat".into()),
        ..default()
    };
    assert!(
        !model::rows(app.world(), &observed, &gone, true)
            .iter()
            .any(|row| row.key == "item_detail")
    );
}

#[test]
fn live_depth_words_are_plain_and_counted() {
    assert_eq!(depth::counted(3, 7), "3 of 10");
    assert_eq!(depth::pack_words(3_200, 20_000), "3.2 of 20 kg");
    assert_eq!(depth::pack_words(0, 15_000), "0 of 15 kg");
    assert_eq!(depth::sentence("move within 1.5 m"), "Move within 1.5 m.");
    assert_eq!(depth::sentence("Done."), "Done.");
    assert_eq!(depth::reward_words(&[]), "");
    assert_eq!(
        depth::reward_words(&["a storage chest".into()]),
        "Unlocks a storage chest"
    );
    assert_eq!(
        depth::reward_words(&["a chest".into(), "a shed".into(), "a plot".into()]),
        "Unlocks a chest, a shed and a plot"
    );
    let friend = depth::Tier {
        name: "friend".into(),
        score: 20_000,
        discount: 10,
        helps: true,
        invites: false,
    };
    assert_eq!(
        depth::tier_gives(&friend),
        "10% off what you buy from them \u{b7} lends a hand at your homestead."
    );
    let stranger = depth::Tier {
        discount: 0,
        helps: false,
        ..friend
    };
    assert!(depth::tier_gives(&stranger).starts_with("Nothing yet"));
    assert_eq!(depth::tier_word("close friend"), "Close friend");
    assert!(depth::helps(
        "thirst",
        island_content::village::catalog::WATER
    ));
    assert!(depth::helps("hunger", "bread") && !depth::helps("hunger", "log"));
    assert!(!depth::helps("energy", "bread"));
}

#[test]
fn the_goal_tracker_counts_steps_and_says_what_a_goal_unlocks() {
    let chain = depth::Chain {
        step: 2,
        total: 7,
        done: 1,
        reward: "Unlocks a storage chest".into(),
        current: Some("Build a storage chest".into()),
        next: Some("Lay out a second plot".into()),
        last_done: Some("Build a cottage of your own".into()),
    };
    assert_eq!(hud::chain_words(&chain), "Step 2 of 7");
    assert_eq!(
        hud::chain_reward(&chain),
        "Goal: Build a storage chest. Unlocks a storage chest."
    );
    let all = depth::Chain {
        done: 7,
        current: None,
        ..chain
    };
    assert_eq!(hud::chain_words(&all), "All 7 done");
    assert_eq!(hud::chain_reward(&all), "");
}

#[test]
fn a_critical_need_pulses_unless_motion_is_reduced() {
    let steady: Vec<f32> = [0.0, 0.5, 1.0, 1.7]
        .iter()
        .map(|at| hud::pulse_alpha(*at, true))
        .collect();
    assert!(steady.iter().all(|alpha| (*alpha - 0.9).abs() < 1e-6));
    let low = hud::pulse_alpha(0.0, false);
    let peak = hud::pulse_alpha(1.0, false);
    assert!(low >= 0.35 && peak > 0.9, "{low} {peak}");
    // Once every two seconds.
    assert!((hud::pulse_alpha(1.0, false) - hud::pulse_alpha(3.0, false)).abs() < 1e-4);
}

#[test]
fn live_outcomes_are_said_in_plain_words_for_the_played_character() {
    use npc_framework::prelude::SimId;
    let player = SimId(9);
    let name = |id: SimId| {
        if id.0 == 4 {
            "Ada".to_owned()
        } else {
            "Bram".to_owned()
        }
    };
    let say = |kind: &str, effect: serde_json::Value, subject: Option<SimId>| {
        let read = |key: &str| effect.get(key).cloned();
        depth::outcome(kind, &read, subject, player, &name)
    };
    assert_eq!(
        say(
            "VillageGift",
            json!({"to":4,"kind":"bread","taste":"liked"}),
            Some(player)
        )
        .as_deref(),
        Some("Ada loved the bread.")
    );
    assert_eq!(
        say(
            "VillageGift",
            json!({"to":4,"kind":"village_fish","taste":"disliked"}),
            Some(player)
        )
        .as_deref(),
        Some("Ada did not care for the fish.")
    );
    assert_eq!(
        say(
            "VillageBondTier",
            json!({"with":4,"tier":"friend","up":true}),
            Some(player)
        )
        .as_deref(),
        Some("Ada is now your friend.")
    );
    assert_eq!(
        say(
            "VillageGiftReturned",
            json!({"to":9,"kind":"plank"}),
            Some(SimId(4))
        )
        .as_deref(),
        Some("Ada gave you some plank.")
    );
    assert_eq!(
        say(
            "VillageRepeatStopped",
            json!({"reason":"count_done"}),
            Some(player)
        )
        .as_deref(),
        Some("Your repeat order is done.")
    );
    // Somebody else's gift is not the player's news.
    assert_eq!(
        say(
            "VillageGift",
            json!({"to":4,"kind":"bread"}),
            Some(SimId(5))
        ),
        None
    );
    assert_eq!(say("VillageTrade", json!({}), Some(player)), None);
}

#[test]
fn the_day_summary_fits_one_screen_and_ends_with_the_next_step() {
    use island_content::village::live::DaySummary;
    use std::collections::BTreeMap;
    let summary = DaySummary {
        day: 6,
        earned: 12,
        spent: 5,
        gathered: BTreeMap::from([("log".into(), 3), ("village_berries".into(), 2)]),
        made: BTreeMap::new(),
        // Three loaves came in, two of them bought: Received says the third.
        received: BTreeMap::from([("bread".into(), 3)]),
        met: vec![npc_framework::prelude::SimId(4)],
        low: BTreeMap::from([("Thirst".into(), 31_000), ("Hunger".into(), 52_400)]),
        slept: true,
        bought: BTreeMap::from([("bread".into(), 2)]),
        sold: BTreeMap::new(),
        bought_coins: 8,
        sold_coins: 0,
    };
    let events = depth::DayEvents {
        friends: vec!["Ada became your friend.".into()],
        homestead: Vec::new(),
    };
    let lines = depth::summary_lines(
        &summary,
        &|_| "Ada".to_owned(),
        Some("Build a storage chest"),
        &events,
    );
    let labels: Vec<&str> = lines.iter().map(|(label, _)| *label).collect();
    assert_eq!(
        labels,
        [
            "Coins",
            "Bought",
            "Gathered",
            "Received",
            "People met",
            "Friends",
            "Lowest",
            "Next"
        ],
        "an empty list (Sold, Made, Homestead) is left out"
    );
    assert_eq!(lines[0].1, "Earned 12, spent 5");
    assert_eq!(lines[1].1, "2 bread for 8 coins");
    assert_eq!(lines[2].1, "3 logs, 2 berries");
    assert_eq!(lines[3].1, "1 bread");
    assert_eq!(lines[4].1, "Ada");
    assert_eq!(lines[5].1, "Ada became your friend.");
    assert_eq!(lines[6].1, "Food 52% \u{b7} Water 31%");
    let mut world = World::new();
    world.init_resource::<InputFocus>();
    rest::spawn(&mut world);
    rest::open_for_test(&mut world, summary);
    assert!(rest::summary_open(&world));
    rest::dismiss(&mut world);
    assert!(!rest::summary_open(&world));
}

#[test]
fn an_empty_pouch_is_one_line_and_the_list_takes_the_width() {
    use crate::game_ui::scale::LayoutClass;
    use workspace::{PouchLayout, pouch_layout};
    let (_, pane) = workspace::width(Tab::Craft, LayoutClass::Wide);
    assert_eq!(pouch_layout(pane, true), PouchLayout::Line);
    assert_eq!(pouch_layout(pane, false), PouchLayout::Pane(276.0));
    let (_, none) = workspace::width(Tab::Pouch, LayoutClass::Wide);
    assert_eq!(pouch_layout(none, true), PouchLayout::Hidden);
    assert!(workspace::POUCH_EMPTY.ends_with("shows here."));
}

#[test]
fn an_upgrade_site_says_what_it_waits_for_what_is_ordered_and_who_builds() {
    use island_content::village::view::UpgradeSiteView;
    let mut site = UpgradeSiteView {
        tier: "larger_cottage".into(),
        name: "a larger cottage".into(),
        cost: vec![("plank".into(), 6), ("log".into(), 4)],
        delivered: vec![("plank".into(), 3)],
        ordered: vec![("log".into(), 2)],
        escrow: 12,
        supplied: false,
        work_ticks: 900,
        remaining_ticks: 900,
        builder: None,
        order_prices: vec![("plank".into(), 4), ("log".into(), 3)],
        hire_cost: 20,
        workers: Vec::new(),
        ordered_at: Some(1_000),
    };
    let words = model::site_words(&site, Some("3 hours ago"));
    assert!(words.starts_with("Carry materials to your door"), "{words}");
    assert!(
        words.contains(
            "
No one has taken this order yet (ordered 3 hours ago): 2 logs.
"
        ),
        "an order nobody has taken says so: {words}"
    );
    assert!(words.ends_with(model::MEANWHILE_WORDS), "{words}");
    for line in words.lines() {
        assert_eq!(line.matches(". ").count(), 0, "one sentence a row: {line}");
    }
    // A hauler takes it, and a hired builder waits for working hours.
    let worker =
        |id: u32, name: &str, job: &'static str, state: &'static str, kind: &str, count| {
            island_content::village::live::UpgradeWorkerView {
                id: SimId(id),
                name: name.into(),
                job,
                state,
                kind: kind.into(),
                count,
                text: String::new(),
            }
        };
    site.workers = vec![
        worker(7, "Oak", "hauler", "bringing", "log", 2),
        worker(9, "Jun", "builder", "hired", "", 0),
    ];
    site.builder = Some(SimId(9));
    let words = model::site_words(&site, None);
    assert!(!words.contains("No one has taken"), "{words}");
    assert!(words.contains("Oak is bringing 2 logs."), "{words}");
    assert!(
        words.contains("Jun is hired and will come in working hours."),
        "{words}"
    );
    site.workers = vec![worker(9, "Jun", "builder", "working", "", 0)];
    site.ordered.clear();
    site.supplied = true;
    let words = model::site_words(&site, None);
    assert!(words.contains("hired builder"), "{words}");
    assert!(words.contains("Jun is building at your door."), "{words}");
}

#[test]
fn an_order_says_how_long_ago_it_was_placed() {
    // 1 000 ticks an island hour.
    assert_eq!(
        model::waited_words(5_000, 5_400, 1_000),
        "less than an hour ago"
    );
    assert_eq!(model::waited_words(5_000, 6_200, 1_000), "an hour ago");
    assert_eq!(model::waited_words(0, 7_000, 1_000), "7 hours ago");
    assert_eq!(model::waited_words(0, 30_000, 1_000), "a day ago");
    assert_eq!(model::waited_words(0, 50_000, 1_000), "2 days ago");
}

#[test]
fn gather_and_craft_says_why_no_recipe_is_listed() {
    let (title, detail) = model::recipe_empty(false, false);
    assert!(title.starts_with("No workshop") && detail.ends_with("listed here."));
    assert!(model::recipe_empty(true, false).1.contains("choose All"));
    assert!(model::recipe_empty(true, true).0.starts_with("No recipe"));
}

#[test]
fn the_sleeping_strip_says_when_morning_comes() {
    // 24 000 ticks a day; the day turns at 06:00 on the clock face.
    assert_eq!(
        rest::morning_words(48_000, 24_000),
        "\u{b7} morning at 06:00"
    );
    assert_eq!(
        rest::morning_words(6_000, 24_000),
        "\u{b7} morning at 12:00"
    );
}

#[test]
fn a_builder_at_work_is_said_to_build_not_to_be_on_the_way() {
    use island_content::village::live::UpgradeWorkerView;
    use island_content::village::view::UpgradeSiteView;
    let site = UpgradeSiteView {
        tier: "storage".into(),
        name: "a storage chest".into(),
        cost: vec![("plank".into(), 4), ("log".into(), 2)],
        delivered: vec![("plank".into(), 4), ("log".into(), 2)],
        ordered: Vec::new(),
        escrow: 0,
        supplied: true,
        work_ticks: 180,
        remaining_ticks: 90,
        builder: Some(SimId(9)),
        order_prices: Vec::new(),
        hire_cost: 0,
        // The simulation's own view of a hired builder at the door.
        workers: vec![UpgradeWorkerView {
            id: SimId(9),
            name: "Jun".into(),
            job: "builder",
            state: "working",
            kind: String::new(),
            count: 0,
            text: "Jun is building at your door.".into(),
        }],
        ordered_at: None,
    };
    let lines = model::site_lines(&site, None);
    assert!(
        lines
            .iter()
            .any(|line| line == "Jun is building at your door."),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("on the way")),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("is hired and will come")),
        "{lines:?}"
    );
}

#[test]
fn a_builder_between_stages_is_said_to_wait_not_to_be_on_the_way() {
    use island_content::village::live::UpgradeWorkerView;
    let worker = UpgradeWorkerView {
        id: SimId(9),
        name: "Jun".into(),
        job: "builder",
        state: "waiting",
        kind: String::new(),
        count: 0,
        text: String::new(),
    };
    assert_eq!(
        model::worker_sentence(&worker),
        "Jun is between stages of work on your homestead."
    );
    // A friend on the way says so in their own words, apart from the builder.
    let friend = UpgradeWorkerView {
        name: "Bram".into(),
        job: "friend",
        state: "walking",
        ..worker
    };
    assert_eq!(
        model::worker_sentence(&friend),
        "Bram is on the way to lend a hand."
    );
}

#[test]
fn a_coin_sounds_when_the_days_trade_grows_and_only_then() {
    use super::traded;
    assert!(
        !traded(None, Some((3, 10))),
        "the first look only remembers"
    );
    assert!(traded(Some((3, 10)), Some((3, 21))));
    assert!(!traded(Some((3, 21)), Some((3, 21))));
    assert!(
        !traded(Some((3, 21)), Some((4, 0))),
        "a new day starts again"
    );
}
