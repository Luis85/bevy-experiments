use super::*;
use bevy::input_focus::FocusCause;

fn fixture(player_session: bool) -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<player_actions::NativeActions>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ShellState>()
        .init_resource::<PlayerMenuRequest>()
        .insert_resource(crate::PlayerExperience {
            enabled: player_session,
        })
        .add_plugins(crate::island_control::plugin);
    app
}

fn press(app: &mut App, pressed: &[KeyCode]) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    for key in pressed {
        input.press(*key);
    }
    keys(app.world_mut());
    process(app.world_mut());
}

fn just_pressed(app: &App, key: KeyCode) -> bool {
    app.world()
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(key)
}

#[test]
fn hotkeys_switch_all_three_modes_through_the_control_coordinator() {
    let mut app = fixture(true);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
    press(&mut app, &[KeyCode::Digit3]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Editor);
    assert!(
        !just_pressed(&app, KeyCode::Digit3),
        "the key is consumed before control groups or other panels see it"
    );
    press(&mut app, &[KeyCode::Digit2]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);

    // Live asks the coordinator for the character; it owns arrival and attach.
    assert!(matches!(
        life_state(app.world()),
        LifeState::AwaitingArrival | LifeState::Playable
    ));
    press(&mut app, &[KeyCode::Digit1]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Character);
    assert!(app.world().resource::<ShellState>().notice.is_none());

    // A shortcut chord belongs to its owner (Ctrl+1 is a control group).
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::Digit2]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Character);
}

#[test]
fn space_minus_and_equal_drive_one_clock_in_every_mode() {
    for mode in [
        IslandMode::Inspector,
        IslandMode::Editor,
        IslandMode::Character,
    ] {
        let mut app = fixture(true);
        app.insert_resource(mode);
        press(&mut app, &[KeyCode::Space]);
        assert!(app.world().resource::<SimControl>().paused, "{mode:?}");
        assert!(
            !just_pressed(&app, KeyCode::Space),
            "consumed, so the host speed keys cannot toggle it back"
        );
        assert!(clock(app.world()).stopped);
        press(&mut app, &[KeyCode::Space]);
        assert!(!app.world().resource::<SimControl>().paused);
        press(&mut app, &[KeyCode::Equal]);
        assert_eq!(app.world().resource::<SimControl>().multiplier, 3);
        press(&mut app, &[KeyCode::NumpadAdd]);
        assert_eq!(app.world().resource::<SimControl>().multiplier, 10);
        press(&mut app, &[KeyCode::Minus]);
        assert_eq!(app.world().resource::<SimControl>().multiplier, 3);
        assert_eq!(clock(app.world()).active_speed(), Some(3));
    }
}

#[test]
fn space_activates_a_focused_control_instead_of_pausing() {
    let mut app = fixture(true);
    let control = app.world_mut().spawn(Node::default()).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(control, FocusCause::Navigated);
    press(&mut app, &[KeyCode::Space]);
    assert!(!app.world().resource::<SimControl>().paused);
    assert!(just_pressed(&app, KeyCode::Space), "left for the widget");
    // The speed keys have no widget meaning and still work.
    press(&mut app, &[KeyCode::Equal]);
    assert_eq!(app.world().resource::<SimControl>().multiplier, 3);
}

#[test]
fn clock_buttons_and_menu_act_like_their_keys() {
    let mut app = fixture(true);
    app.world_mut().resource_mut::<SimControl>().paused = true;
    app.world_mut()
        .resource_mut::<ShellState>()
        .requests
        .extend([Action::Clock(ClockAction::Speed(10)), Action::Menu]);
    process(app.world_mut());
    let control = app.world().resource::<SimControl>();
    assert!(!control.paused, "choosing a speed resumes");
    assert_eq!(control.multiplier, 10);
    assert!(
        app.world().resource::<PlayerMenuRequest>().0,
        "the host decides what its menu does"
    );
}

#[test]
fn developer_launches_keep_their_keys_and_have_no_bar() {
    let mut app = fixture(false);
    assert!(!enabled(app.world()));
    press(&mut app, &[KeyCode::Digit3, KeyCode::Space]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
    assert!(!app.world().resource::<SimControl>().paused);
    assert!(just_pressed(&app, KeyCode::Digit3) && just_pressed(&app, KeyCode::Space));
    assert_eq!(content_top(app.world_mut()), 12.0);
}

fn tab(app: &mut App, shift: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    if shift {
        input.press(KeyCode::ShiftLeft);
    }
    input.press(KeyCode::Tab);
    focus::keys(app.world_mut());
}

fn focused(app: &App) -> Option<Entity> {
    app.world().resource::<InputFocus>().get()
}

#[test]
fn tab_walks_the_bar_in_layout_order_and_wraps() {
    let mut app = fixture(true);
    spawn(app.world_mut());
    refresh(app.world_mut());
    assert_eq!(focus::surface(app.world()), focus::Surface::Oversee);
    let bar = focus::ring(app.world_mut(), focus::Surface::Oversee);
    let actions: Vec<Action> = bar
        .iter()
        .map(|entity| app.world().get::<ShellAction>(*entity).unwrap().0.clone())
        .collect();
    // Menu, modes, totals, date, speed: reading order, left to right.
    let mut expected = vec![Action::Menu];
    expected.extend(ModeTab::ALL.map(Action::Mode));
    expected.extend(TotalKind::ALL.map(Action::Total));
    expected.push(Action::Calendar);
    expected.push(Action::Clock(ClockAction::TogglePause));
    expected.extend(SPEEDS.map(|speed| Action::Clock(ClockAction::Speed(speed))));
    assert_eq!(actions, expected);
    for control in &bar {
        assert!(
            app.world().get::<focus::KeyReach>(*control).is_some()
                && app
                    .world()
                    .get::<bevy::input_focus::tab_navigation::TabIndex>(*control)
                    .is_none(),
            "reached by Tab, never focused by a click"
        );
        assert!(
            app.world()
                .get::<Name>(*control)
                .is_some_and(|name| name.as_str().starts_with("Game: ")),
            "every control is named, icon-only ones too"
        );
    }
    // No label on the bar carries a key: bindings are key caps and tooltips.
    let world = app.world_mut();
    for text in world.query::<&Text>().iter(world) {
        for key in ["Esc", "Space", "F11", "Ctrl"] {
            assert!(
                !text.0.contains(&format!("\u{b7} {key}")),
                "{:?} names a key",
                text.0
            );
        }
    }
    // From the world, Tab lands on Menu and walks left to right.
    for expected in &bar {
        tab(&mut app, false);
        assert_eq!(focused(&app), Some(*expected));
        assert!(
            !just_pressed(&app, KeyCode::Tab),
            "consumed, so no panel moves focus a second time"
        );
    }
    tab(&mut app, false);
    assert_eq!(focused(&app), Some(bar[0]), "wraps");
    tab(&mut app, true);
    assert_eq!(focused(&app), bar.last().copied(), "Shift+Tab goes back");
    // A focused control keeps Space for itself, as in the ledger.
    press(&mut app, &[KeyCode::Space]);
    assert!(!app.world().resource::<SimControl>().paused);
    // Esc hands the keyboard back to the world and goes no further; then
    // Space pauses again.
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    input.press(KeyCode::Escape);
    focus::escape(app.world_mut());
    assert_eq!(focused(&app), None);
    assert!(!just_pressed(&app, KeyCode::Escape), "one step per press");
    press(&mut app, &[KeyCode::Space]);
    assert!(app.world().resource::<SimControl>().paused);
    // Paused is shown twice: the lit segment and the pill.
    refresh(app.world_mut());
    let world = app.world_mut();
    let pill = world
        .query_filtered::<&Node, With<PausedPill>>()
        .single(world)
        .unwrap()
        .display;
    assert_eq!(pill, Display::Flex);
    let lit = world
        .query::<(&ShellAction, &w::Control)>()
        .iter(world)
        .filter(|(action, control)| {
            action.0 == Action::Clock(ClockAction::TogglePause) && control.on
        })
        .count();
    assert_eq!(lit, 1);
}

#[test]
fn totals_show_while_overseeing_only_and_open_their_ledger_page() {
    let mut app = fixture(true);
    app.init_resource::<crate::colony_panel::StockTrends>();
    spawn(app.world_mut());
    refresh(app.world_mut());
    let shown = |app: &mut App| {
        let world = app.world_mut();
        world
            .query_filtered::<&Node, With<TotalsGroup>>()
            .iter(world)
            .all(|node| node.display != Display::None)
    };
    assert!(shown(&mut app));
    // The numbers are the simulation's own.
    let summary = colony::summary(app.world());
    let totals = model::totals(&totals_input(app.world()));
    assert_eq!(totals[0].value, summary.residents.to_string());
    assert_eq!(totals[0].unit, Some(format!("/ {}", summary.beds)));
    assert_eq!(totals[2].value, summary.food_units.to_string());
    let residents = app.world().resource::<ShellState>().totals[0].1;
    assert_eq!(
        app.world().get::<Text>(residents.value).unwrap().0,
        summary.residents.to_string()
    );
    assert!(
        app.world()
            .get::<Tooltip>(residents.entity)
            .is_some_and(|tip| tip.levels() == 2 && tip.title == "Residents"),
        "every total explains itself at two levels"
    );
    // Compact: the totals move to a row of their own under the bar, the bar
    // loses its rule, and everything placed under the bar moves down with it.
    assert!(!model::totals_on_second_row(LayoutClass::Standard, true));
    assert!(!model::totals_on_second_row(LayoutClass::Wide, true));
    assert!(!model::totals_on_second_row(LayoutClass::Compact, false));
    assert!(model::totals_on_second_row(LayoutClass::Compact, true));
    let (bar, group, row, place) = app.world().resource::<ShellState>().totals_home.unwrap();
    let parent = |app: &App| app.world().get::<ChildOf>(group).map(ChildOf::parent);
    assert_eq!(parent(&app), Some(bar));
    let top = content_top(app.world_mut());
    refresh_totals(app.world_mut(), true, LayoutClass::Compact);
    assert_eq!(parent(&app), Some(row));
    assert_eq!(app.world().get::<Node>(row).unwrap().display, Display::Flex);
    assert_eq!(content_top(app.world_mut()), top + model::TOTALS_ROW);
    assert!(shown(&mut app), "the totals themselves stay");
    let reachable = focus::ring(app.world_mut(), focus::Surface::Oversee);
    assert!(
        reachable.contains(&residents.entity),
        "nothing leaves the Tab ring"
    );
    refresh_totals(app.world_mut(), true, LayoutClass::Standard);
    assert_eq!(parent(&app), Some(bar));
    assert_eq!(
        app.world()
            .get::<Children>(bar)
            .and_then(|children| children.iter().position(|child| child == group)),
        Some(place),
        "back where it was on the bar"
    );
    assert_eq!(app.world().get::<Node>(row).unwrap().display, Display::None);
    assert_eq!(content_top(app.world_mut()), top);
    // Live and Shape keep the bar but not the totals.
    for mode in [IslandMode::Editor, IslandMode::Character] {
        app.insert_resource(mode);
        refresh(app.world_mut());
        assert!(!shown(&mut app), "{mode:?}");
    }
    // Reading the totals never touches the simulation.
    let before = npc_framework::kernel::hash::state_hash(app.world_mut());
    app.insert_resource(IslandMode::Inspector);
    app.world_mut().resource_mut::<ShellState>().totals_elapsed = 1.0;
    refresh(app.world_mut());
    assert_eq!(
        npc_framework::kernel::hash::state_hash(app.world_mut()),
        before
    );
}

#[test]
fn live_keeps_tab_for_the_workspace_and_reaches_the_bar_with_shift_tab() {
    let mut app = fixture(true);
    spawn(app.world_mut());
    press(&mut app, &[KeyCode::Digit1]);
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Character);
    refresh(app.world_mut());
    assert!(matches!(
        focus::surface(app.world()),
        focus::Surface::Live { .. }
    ));
    // Tab from the world is left for the workspace (it opens the Pouch).
    tab(&mut app, false);
    assert_eq!(focused(&app), None);
    assert!(just_pressed(&app, KeyCode::Tab), "not consumed");
    // Shift+Tab reaches the bar's last control; Tab past it returns.
    tab(&mut app, true);
    let last = focused(&app).expect("the bar is reachable in Live");
    assert!(app.world().get::<focus::KeyReach>(last).is_some());
    tab(&mut app, false);
    assert_eq!(focused(&app), None, "off the end: back to the world");
    // Developer launches have no bar and keep Tab entirely.
    let mut developer = fixture(false);
    spawn(developer.world_mut());
    tab(&mut developer, false);
    assert_eq!(focused(&developer), None);
    assert!(just_pressed(&developer, KeyCode::Tab));
}

fn notice_text(app: &mut App) -> String {
    let world = app.world_mut();
    let shown = world
        .query_filtered::<&Node, With<ShellNotice>>()
        .single(world)
        .unwrap()
        .display
        != Display::None;
    let text = world
        .query_filtered::<&Text, With<NoticeText>>()
        .single(world)
        .unwrap()
        .0
        .clone();
    if shown { text } else { String::new() }
}

#[test]
fn the_live_tip_is_a_tooltip_and_only_a_refusal_shows_under_the_bar() {
    let mut app = fixture(true);
    spawn(app.world_mut());
    refresh(app.world_mut());
    assert_eq!(notice_text(&mut app), "", "nothing sits under the bar");
    let live = {
        let world = app.world_mut();
        world
            .query::<(Entity, &ShellAction)>()
            .iter(world)
            .find(|(_, action)| action.0 == Action::Mode(ModeTab::Live))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let tip = app.world().get::<Tooltip>(live).unwrap().clone();
    assert_eq!(
        tip.key.as_deref(),
        Some("1"),
        "the key is a cap in the tooltip"
    );
    assert_eq!(
        tip.body,
        model::mode_tooltip(
            IslandMode::Inspector,
            ModeTab::Live,
            life_state(app.world())
        )
    );
    if life_state(app.world()) == LifeState::AwaitingArrival {
        assert_eq!(
            Some(tip.body.as_str()),
            model::live_tip(IslandMode::Inspector, LifeState::AwaitingArrival)
        );
    }
    // A refused request is a toast for a few seconds, then it goes.
    notice(app.world_mut(), "Live is unavailable right now.");
    refresh(app.world_mut());
    assert_eq!(notice_text(&mut app), "Live is unavailable right now.");
    app.insert_resource(crate::FrontendGate(true));
    refresh(app.world_mut());
    assert_eq!(notice_text(&mut app), "", "hidden under a menu");
    app.insert_resource(crate::FrontendGate(false));
    app.world_mut().resource_mut::<ShellState>().notice = None;
    refresh(app.world_mut());
    assert_eq!(notice_text(&mut app), "");
}

#[test]
fn the_clock_opens_a_read_only_calendar_that_escape_closes() {
    let mut app = fixture(true);
    app.init_resource::<ButtonInput<MouseButton>>();
    spawn(app.world_mut());
    let before = npc_framework::kernel::hash::state_hash(app.world_mut());
    app.world_mut()
        .resource_mut::<ShellState>()
        .requests
        .push(Action::Calendar);
    process(app.world_mut());
    refresh(app.world_mut());
    assert_eq!(
        app.world().resource::<ShellState>().popover,
        Popover::Calendar
    );
    let world = app.world_mut();
    let shown = world
        .query_filtered::<&Node, With<CalendarRoot>>()
        .single(world)
        .unwrap()
        .display;
    assert_eq!(shown, Display::Flex);
    let now = world
        .query_filtered::<&Text, With<CalendarNow>>()
        .single(world)
        .unwrap()
        .0
        .clone();
    let next = world
        .query_filtered::<&Text, With<CalendarNext>>()
        .single(world)
        .unwrap()
        .0
        .clone();
    // The island opens on calendar day 19: late autumn, winter in two days.
    assert!(
        now.starts_with(
            "Autumn · day 6 of 7
Sunrise 06:00 · sunset 22:48"
        ),
        "{now}"
    );
    assert!(now.contains("Cold: only at night."), "{now}");
    assert!(next.starts_with("Winter begins in 2 days"), "{next}");
    assert!(next.contains("Cold: all day and night"), "{next}");
    // Esc closes it and keeps the key from the pause menu.
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    input.press(KeyCode::Escape);
    popover_input(app.world_mut());
    assert_eq!(app.world().resource::<ShellState>().popover, Popover::None);
    assert!(!just_pressed(&app, KeyCode::Escape));
    refresh(app.world_mut());
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<&Node, With<CalendarRoot>>()
            .single(world)
            .unwrap()
            .display,
        Display::None
    );
    // A click on the world closes it too; the calendar never records anything.
    app.world_mut().resource_mut::<ShellState>().popover = Popover::Calendar;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    popover_input(app.world_mut());
    assert_eq!(app.world().resource::<ShellState>().popover, Popover::None);
    assert_eq!(
        npc_framework::kernel::hash::state_hash(app.world_mut()),
        before
    );
}

#[test]
fn build_is_on_the_dock_in_oversee_only_and_b_opens_the_menu() {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<player_actions::NativeActions>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ShellState>()
        .init_resource::<dock::Docks>()
        .init_resource::<PlayerMenuRequest>()
        .init_resource::<crate::island_build::BuildTool>()
        .init_resource::<crate::island_build::CivicLog>()
        .insert_resource(crate::PlayerExperience { enabled: true })
        .add_plugins(crate::island_control::plugin);
    spawn(app.world_mut());
    dock::spawn(app.world_mut());
    // The build tool reads the world's catalogue once.
    crate::island_build::process(app.world_mut());
    let tiles = |app: &mut App| -> Vec<String> {
        dock::refresh(app.world_mut());
        let world = app.world_mut();
        let shown = world
            .query_filtered::<&Node, With<dock::DockRoot>>()
            .iter(world)
            .filter(|node| node.display != Display::None)
            .count();
        if shown < 3 {
            return Vec::new();
        }
        crate::island_build::groups(world)
            .into_iter()
            .map(|group| dock::group_label(group).to_owned())
            .collect()
    };
    assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
    // Farms is a tile only where the catalogue has a building with crop plots.
    let mut offered = vec!["Homes", "Work", "Storage", "Market"];
    if island_content::village::civic::catalog(app.world())
        .iter()
        .any(|def| def.field.is_some())
    {
        offered.push("Farms");
    }
    assert_eq!(
        tiles(&mut app),
        offered,
        "the categories the catalogue offers; an empty one is left out"
    );
    // A category tile opens the build menu on it; the same tile closes it.
    use crate::island_build::Group;
    app.world_mut()
        .resource_mut::<dock::Docks>()
        .requests
        .push(dock::DockAction::Build(Group::Storage));
    dock::process(app.world_mut());
    crate::island_build::process(app.world_mut());
    assert_eq!(
        crate::island_build::open_group(app.world()),
        Some(Group::Storage)
    );
    app.world_mut()
        .resource_mut::<dock::Docks>()
        .requests
        .push(dock::DockAction::Build(Group::Homes));
    dock::process(app.world_mut());
    crate::island_build::process(app.world_mut());
    assert_eq!(
        crate::island_build::open_group(app.world()),
        Some(Group::Homes)
    );
    app.world_mut()
        .resource_mut::<dock::Docks>()
        .requests
        .push(dock::DockAction::Build(Group::Homes));
    dock::process(app.world_mut());
    crate::island_build::process(app.world_mut());
    assert!(!crate::island_build::palette_open(app.world()));
    // B opens and closes it from the keyboard, and is consumed.
    for open in [true, false] {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(KeyCode::KeyB);
        crate::island_build::keys(app.world_mut());
        assert!(!just_pressed(&app, KeyCode::KeyB));
        crate::island_build::process(app.world_mut());
        assert_eq!(crate::island_build::palette_open(app.world()), open);
    }
    // With a building in hand, R turns it and Esc puts it down before
    // anything else sees the key.
    {
        let mut tool = app
            .world_mut()
            .resource_mut::<crate::island_build::BuildTool>();
        tool.open = true;
        tool.selected = Some("cottage".into());
    }
    // The totals a building in hand draws on are marked on the bar.
    assert_eq!(totals_input(app.world()).in_hand, ["plank", "log"]);
    press_only(&mut app, KeyCode::KeyR);
    crate::island_build::keys(app.world_mut());
    crate::island_build::process(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<crate::island_build::BuildTool>()
            .rotation,
        1
    );
    for (holding, open) in [(false, true), (false, false)] {
        press_only(&mut app, KeyCode::Escape);
        crate::island_build::keys(app.world_mut());
        assert!(
            !just_pressed(&app, KeyCode::Escape),
            "Esc never reaches the pause menu while the tool or the menu is up"
        );
        crate::island_build::process(app.world_mut());
        let tool = app.world().resource::<crate::island_build::BuildTool>();
        assert_eq!((tool.selected.is_some(), tool.open), (holding, open));
    }
    // With nothing of the build tool open, Esc is left for the menu.
    press_only(&mut app, KeyCode::Escape);
    crate::island_build::keys(app.world_mut());
    assert!(just_pressed(&app, KeyCode::Escape));
    // Shape has its own palette and no docks.
    app.insert_resource(IslandMode::Editor);
    assert!(tiles(&mut app).is_empty());
}

fn press_only(app: &mut App, key: KeyCode) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    input.press(key);
}

#[test]
fn the_calendar_tells_the_days_ahead_only_where_the_world_has_them() {
    use model::{BoatOutlook, Outlook, outlook_lines};
    // A world with neither weather nor a boat: nothing is invented.
    assert!(
        outlook_lines(&Outlook {
            growth_percent: 100,
            warmth_loss_percent: 100,
            ..Outlook::default()
        })
        .is_empty()
    );
    let outlook = Outlook {
        today: "rain".into(),
        ahead: vec!["clear".into(), "snow".into(), "storm".into()],
        growth_percent: 120,
        warmth_loss_percent: 100,
        shock: Some("harsh winter".into()),
        boat: Some(BoatOutlook::Due { days: 3 }),
        boat_orders: Vec::new(),
    };
    assert_eq!(
        outlook_lines(&outlook),
        [
            "Today: rain. Crops grow at 120%.",
            "Tomorrow clear \u{b7} in 2 days snow \u{b7} in 3 days storm.",
            "This season brings a harsh winter.",
            "The merchant boat calls in 3 days.",
        ]
    );
    let boat = |boat| {
        outlook_lines(&Outlook {
            growth_percent: 100,
            warmth_loss_percent: 100,
            boat: Some(boat),
            ..Outlook::default()
        })
    };
    assert_eq!(
        boat(BoatOutlook::In {
            until: "14:30".into()
        }),
        ["The merchant boat is in at the trading post until 14:30."]
    );
    assert_eq!(
        boat(BoatOutlook::Due { days: 0 }),
        ["The merchant boat calls later today."]
    );
    assert_eq!(
        boat(BoatOutlook::Due { days: 1 }),
        ["The merchant boat calls tomorrow."]
    );
    // The living world has weather, so its calendar has a forecast.
    let app = island_content::new_sim_app(7, "living_world");
    let lines = outlook_lines(&super::outlook(app.world()));
    assert!(lines[0].starts_with("Today: "), "{lines:?}");
}

#[test]
fn every_picture_only_control_of_the_bar_and_the_docks_has_a_tooltip() {
    let mut app = fixture(true);
    app.init_resource::<dock::Docks>();
    spawn(app.world_mut());
    dock::spawn(app.world_mut());
    // The totals and the date take their numbers and tooltips on a refresh.
    refresh(app.world_mut());
    assert_eq!(
        w::unexplained_icon_controls(app.world_mut()),
        Vec::<String>::new()
    );
}

/// Every stop of the Tab ring that shows only a picture, without a tooltip.
fn untold_on_the_ring(world: &mut World, surface: focus::Surface) -> Vec<String> {
    let ring = focus::ring(world, surface);
    assert!(!ring.is_empty(), "the ring has stops");
    let told: Vec<String> = w::unexplained_icon_controls(world);
    ring.into_iter()
        .filter_map(|entity| world.get::<Name>(entity).map(|name| name.to_string()))
        .filter(|name| told.contains(name))
        .collect()
}

#[test]
fn walking_the_tab_ring_of_every_panel_finds_no_picture_without_a_tooltip() {
    // The panels that were not covered by the bar test: the Shape palette,
    // the inspector, the building card, the build tray, the alert stack and
    // the ledger. Each is shown, then the ring of its mode is walked.
    let mut app = fixture(true);
    app.init_resource::<dock::Docks>();
    let world = app.world_mut();
    spawn(world);
    dock::spawn(world);
    crate::shape_tools::palette::spawn_for_tests(world);
    crate::resident_inspector::spawn_for_tests(world);
    crate::island_build::card::spawn_for_tests(world);
    crate::island_build::palette::spawn_for_tests(world);
    crate::island_alerts::spawn_for_tests(world);
    crate::colony_panel::spawn_for_tests(world);
    refresh(world);
    // Show every region's root, so the ring reaches into each.
    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<focus::FocusRegion>>()
        .iter(world)
        .collect();
    for root in roots {
        if let Some(mut node) = world.get_mut::<Node>(root) {
            node.display = Display::Flex;
        }
    }
    *world.resource_mut::<IslandMode>() = IslandMode::Inspector;
    assert_eq!(
        untold_on_the_ring(world, focus::Surface::Oversee),
        Vec::<String>::new(),
        "Oversee"
    );
    *world.resource_mut::<IslandMode>() = IslandMode::Editor;
    assert_eq!(
        untold_on_the_ring(world, focus::Surface::Shape),
        Vec::<String>::new(),
        "Shape"
    );
    // And nothing anywhere, hidden or not.
    assert_eq!(w::unexplained_icon_controls(world), Vec::<String>::new());
}

#[test]
fn a_host_toast_has_a_shape_for_its_tone_and_a_failure_stays_with_its_button() {
    use crate::game_ui::icon::Icon;
    use crate::game_ui::tokens::Status;
    assert_eq!(toast_look(ToastTone::Done), (Status::Good, Icon::Done));
    assert_eq!(toast_look(ToastTone::Note), (Status::Info, Icon::Hint));
    assert_eq!(toast_look(ToastTone::Failed).0, Status::Crit);
    let mut world = World::new();
    world.init_resource::<ShellState>();
    host_toast(&mut world, ToastTone::Done, "Island saved", false, false);
    assert!(
        world
            .resource::<ShellState>()
            .host
            .as_ref()
            .unwrap()
            .left
            .is_some()
    );
    host_toast(
        &mut world,
        ToastTone::Failed,
        "Could not save: disk full",
        true,
        true,
    );
    let toast = world.resource::<ShellState>().host.clone().unwrap();
    assert!(
        toast.left.is_none() && toast.button.as_deref() == Some("Try again"),
        "it stays until replaced"
    );
    assert!(!take_toast_retry(&mut world));
    world.resource_mut::<ShellState>().retry = true;
    assert!(take_toast_retry(&mut world) && !take_toast_retry(&mut world));
    clear_host_toast(&mut world);
    assert!(world.resource::<ShellState>().host.is_none());
    host_toast_with(
        &mut world,
        ToastTone::Done,
        "World exported",
        false,
        "Start a new game on it",
    );
    let toast = world.resource::<ShellState>().host.clone().unwrap();
    assert_eq!(toast.left, Some(HOST_BUTTON_STAY), "long enough to reach");
    assert_eq!(toast.button.as_deref(), Some("Start a new game on it"));
}

#[test]
fn the_toast_lane_keeps_clear_of_a_wide_live_workspace() {
    // No workspace, or one the centred pill clears: the whole window.
    assert_eq!(model::toast_lane(1280.0, 0.0, 316.0), (0.0, 0.0, 0.0));
    assert_eq!(model::toast_lane(1280.0, 360.0, 316.0), (0.0, 0.0, 0.0));
    assert_eq!(model::toast_lane(1024.0, 340.0, 316.0), (0.0, 0.0, 0.0));
    // Two panes: between the goal tracker and the workspace.
    assert_eq!(model::toast_lane(1280.0, 600.0, 316.0), (316.0, 608.0, 0.0));
    // At 1024 that is too narrow for a sentence: under the folded goal.
    let (left, right, drop) = model::toast_lane(1024.0, 540.0, 316.0);
    assert!(
        drop > 60.0 && 1024.0 - right - left >= 320.0,
        "{left} {right}"
    );
}

#[test]
fn a_crowded_standard_bar_gives_up_the_weather_chip_then_the_mode_words() {
    use model::bar_crowding;
    assert_eq!(bar_crowding(LayoutClass::Wide, 7), (false, false));
    assert_eq!(bar_crowding(LayoutClass::Standard, 5), (false, false));
    assert_eq!(bar_crowding(LayoutClass::Standard, 6), (true, false));
    assert_eq!(
        bar_crowding(LayoutClass::Standard, 7),
        (true, true),
        "logs and planks beside the five: the fund number keeps its place"
    );
    assert_eq!(bar_crowding(LayoutClass::Compact, 0), (true, true));
}

#[test]
fn notices_said_together_are_each_shown_in_turn() {
    use std::collections::VecDeque;
    let (mut current, mut queued) = (None, VecDeque::new());
    for text in [
        "Work on a larger cottage has begun.",
        "A larger cottage is finished.",
        "Goal done: Enlarge the cottage.",
        "A larger cottage is finished.",
    ] {
        model::queue_notice(&mut current, &mut queued, text.to_owned(), 6.0);
    }
    assert_eq!(queued.len(), 2, "the repeat is not queued twice");
    let mut seen = vec![current.clone().unwrap().0];
    for _ in 0..200 {
        let before = current.clone().map(|(text, _)| text);
        model::advance_notice(&mut current, &mut queued, 0.1, 6.0);
        let after = current.clone().map(|(text, _)| text);
        if after != before
            && let Some(text) = after
        {
            seen.push(text);
        }
    }
    assert_eq!(
        seen,
        [
            "Work on a larger cottage has begun.",
            "A larger cottage is finished.",
            "Goal done: Enlarge the cottage."
        ]
    );
    assert!(current.is_none());
}

#[test]
fn the_boats_next_order_reads_as_rows_with_plural_goods() {
    use super::model::{BoatOutlook, Outlook, boat_order_lines, outlook_lines};
    assert!(
        boat_order_lines(&[], false).is_empty(),
        "nothing when it wants nothing"
    );
    assert_eq!(
        boat_order_lines(&[("plank".into(), 10, 14, 0)], false),
        ["Next call wants 10 planks at 14 coins each."]
    );
    assert_eq!(
        boat_order_lines(
            &[("plank".into(), 10, 14, 3), ("ingot".into(), 1, 1, 0)],
            true
        ),
        [
            "In port, it wants 10 planks at 14 coins each: 3 sold so far.",
            "In port, it wants 1 ingot at 1 coin each: 0 sold so far."
        ]
    );
    // The calendar's outlook says them after the boat's own line.
    let lines = outlook_lines(&Outlook {
        boat: Some(BoatOutlook::Due { days: 2 }),
        boat_orders: boat_order_lines(&[("log".into(), 6, 4, 0)], false),
        ..Outlook::default()
    });
    assert_eq!(
        lines[lines.len() - 2..],
        [
            "The merchant boat calls in 2 days.".to_owned(),
            "Next call wants 6 logs at 4 coins each.".to_owned()
        ]
    );
}

#[test]
fn the_paused_pill_words_and_the_lane_beside_the_column() {
    assert_eq!(model::paused_words(0), "Paused");
    assert_eq!(model::paused_words(1), "Paused \u{b7} 1 order waiting");
    assert_eq!(model::paused_words(3), "Paused \u{b7} 3 orders waiting");
    // The lane never runs under an open right column.
    assert_eq!(model::beside_column(0.0, 452.0), 452.0);
    assert_eq!(model::beside_column(600.0, 452.0), 600.0);
    assert_eq!(model::beside_column(0.0, 0.0), 0.0);
}

#[test]
fn a_toast_with_a_button_is_not_replaced_until_it_expires() {
    // Nothing showing, or a plain note: anything takes its place.
    assert!(model::replaces_toast(None, false));
    assert!(model::replaces_toast(Some((false, Some(2.0))), false));
    // "See the season" with seconds left: a plain note waits.
    assert!(!model::replaces_toast(Some((true, Some(3.0))), false));
    // Another toast with a button may replace it; an expired one goes.
    assert!(model::replaces_toast(Some((true, Some(3.0))), true));
    assert!(model::replaces_toast(Some((true, Some(0.0))), false));
    // In the shell: the season toast stays when a plain host toast comes.
    let mut app = fixture(true);
    season_toast(app.world_mut(), "Autumn has ended.".into());
    host_toast(
        app.world_mut(),
        ToastTone::Done,
        "Island saved",
        false,
        false,
    );
    assert_eq!(
        host_toast_text(app.world()),
        Some((
            "Autumn has ended.".to_owned(),
            Some("See the season".to_owned())
        ))
    );
}

#[test]
fn a_toast_in_a_narrow_lane_caps_its_words_to_the_lane() {
    // No layout yet: nothing is capped.
    assert_eq!(toast_words_room(0.0, None), None);
    // 1024 wide beside an open column (a lane of 362): the words get what
    // the padding, border, icon and gap leave; a button takes its share.
    assert_eq!(toast_words_room(362.0, None), Some(308.0));
    assert_eq!(toast_words_room(362.0, Some(100.0)), Some(200.0));
    // Never squeezed to nothing.
    assert_eq!(toast_words_room(90.0, Some(100.0)), Some(80.0));

    // The host toast's words carry the cap once the lane is known.
    let mut app = fixture(true);
    app.world_mut().resource_mut::<ShellState>().lane_width = 362.0;
    let slot = app.world_mut().spawn((HostToastSlot, Node::default())).id();
    host_toast(
        app.world_mut(),
        ToastTone::Failed,
        "Behaviour not changed: the file is not a behaviour document.\nDetails: key must be a string",
        true,
        false,
    );
    refresh_host_toast(app.world_mut(), false, 0.0);
    let toast = app.world().get::<Children>(slot).unwrap()[0];
    let words = *app.world().get::<Children>(toast).unwrap().last().unwrap();
    assert!(app.world().get::<ToastWords>(words).is_some());
    assert_eq!(app.world().get::<Node>(words).unwrap().max_width, px(308.0));
}
