use super::*;
use npc_framework::kernel::hash::state_hash;
use serde_json::json;

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
    let mut app = testbed_content::new_sim_app(7, "island_colony");
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
    let _ = actions::guidance(&observed);
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
    let app = testbed_content::new_sim_app(7, "island_colony");
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
    let app = testbed_content::new_sim_app(7, "island_colony");
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
