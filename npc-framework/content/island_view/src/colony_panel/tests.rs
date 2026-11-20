use super::roster::{RosterRow, SortKey, arrange};
use super::*;
use crate::player_actions;
use crate::resident_inspector::model::NeedLevel;
use island_content::village::terrain::{Terraform, TerrainSurface};
use island_content::village::{Role, Villager};
use model::Entry;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::run_ticks;

fn fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<Ledger>()
        .init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<economy::StockTrends>()
        .init_resource::<player_actions::NativeActions>()
        .add_plugins(crate::island_control::plugin);
    app
}

#[test]
fn a_ground_right_click_releases_ledger_focus_without_consuming_the_order() {
    let mut app = fixture();
    app.init_resource::<ButtonInput<MouseButton>>();
    let control = app.world_mut().spawn((LedgerControl, Node::default())).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(control, FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    pointer(app.world_mut());
    assert!(app.world().resource::<InputFocus>().get().is_none());
    assert!(
        app.world()
            .resource::<ButtonInput<MouseButton>>()
            .just_pressed(MouseButton::Right)
    );
}

#[test]
fn inspecting_residents_and_plans_does_not_change_the_simulation() {
    let mut app = fixture();
    let resident = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| {
            app.world()
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role == Role::Farmer)
        })
        .map(|(id, _)| *id)
        .unwrap();
    app.insert_resource(Selected(Some(resident)));
    let before = state_hash(app.world_mut());
    assert!(
        entries(app.world())
            .iter()
            .any(|entry| entry.stats.as_ref().is_some_and(|stats| stats.len() == 6)),
        "the Village page opens on its stat chips"
    );
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Residents;
    let rows = entries(app.world());
    assert!(rows.iter().any(|entry| entry.text.contains("Ada")));
    // The People page lists and filters; a resident's job and home are
    // commands of their inspector, built from the same rows.
    assert!(
        !rows
            .iter()
            .any(|entry| matches!(entry.action, Some(Action::Home(..))))
    );
    let entity = app.world().resource::<SimIndex>().get(resident).unwrap();
    let rows = manage_entries(app.world(), resident, entity);
    assert!(
        rows.iter()
            .any(|entry| matches!(entry.action, Some(Action::Home(..))))
    );
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn the_village_and_projects_pages_keep_every_colony_lever() {
    let mut app = fixture();
    let rows = entries(app.world());
    let reserve = rows
        .iter()
        .find(|entry| entry.key == "reserve")
        .and_then(|entry| entry.choices.clone())
        .expect("the food reserve is one control with four steps");
    for days in 1..=4 {
        assert!(
            reserve
                .iter()
                .any(|(_, action)| *action == Action::Reserve(days)),
            "food reserve of {days} days stays reachable"
        );
    }
    assert!(
        rows.iter()
            .any(|entry| entry.action == Some(Action::Gatherings))
    );
    // Residents, housed, beds, morale, food and water at a glance.
    let stats = rows
        .iter()
        .find_map(|entry| entry.stats.clone())
        .expect("stat chips");
    let labels: Vec<&str> = stats.iter().map(|(_, label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Residents",
            "Housed",
            "Beds",
            "Morale",
            "Food per resident",
            "Water per resident"
        ]
    );
    // No page explains itself with permanent hint text or names a key.
    for tab in [Tab::Overview, Tab::Economy, Tab::Residents, Tab::Projects] {
        app.world_mut().resource_mut::<Ledger>().tab = tab;
        for entry in entries(app.world()) {
            for hint in ["F11", "Esc", "WASD", "Build \u{b7} B", "Choose one to see"] {
                assert!(!entry.text.contains(hint), "{tab:?}: {}", entry.text);
            }
        }
    }
    // Commissions moved to the Projects page with the building sites.
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Projects;
    assert!(
        entries(app.world())
            .iter()
            .any(|entry| matches!(entry.action, Some(Action::Commission(..))))
    );
}

#[test]
fn the_right_column_holds_the_ledger_or_the_inspector_never_both() {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<Ledger>()
        .init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<Selected>()
        .init_resource::<economy::StockTrends>()
        .init_resource::<player_actions::NativeActions>()
        .init_resource::<crate::resident_inspector::Inspector>()
        .init_resource::<crate::island_shell::ShellState>()
        .insert_resource(crate::PlayerExperience { enabled: true })
        .add_plugins(crate::island_control::plugin);
    process(app.world_mut());
    assert!(shell(app.world()));
    assert_eq!(page(app.world()), None, "closed by default");
    let before = state_hash(app.world_mut());
    // A dock tile opens its page; another switches; the same closes.
    toggle_page(app.world_mut(), LedgerPage::Economy);
    assert_eq!(page(app.world()), Some(LedgerPage::Economy));
    toggle_page(app.world_mut(), LedgerPage::People);
    assert_eq!(page(app.world()), Some(LedgerPage::People));
    // A total asks for its page and never closes it.
    open_page(app.world_mut(), LedgerPage::People);
    assert_eq!(page(app.world()), Some(LedgerPage::People));
    // Choosing a resident replaces the ledger with the inspector.
    let resident = roster_ids(&entries(app.world()))[0];
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Resident(resident));
    process(app.world_mut());
    assert_eq!(page(app.world()), None);
    assert!(crate::resident_inspector::open(app.world()));
    // Chosen from the roster, the inspector offers the way back to it: the
    // page returns and the resident stays selected under it.
    crate::resident_inspector::back_to_people(app.world_mut());
    assert_eq!(page(app.world()), Some(LedgerPage::People));
    assert_eq!(app.world().resource::<Selected>().0, Some(resident));
    toggle_page(app.world_mut(), LedgerPage::People);
    assert_eq!(page(app.world()), None, "the same tile closes the column");
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Resident(resident));
    process(app.world_mut());
    assert!(crate::resident_inspector::open(app.world()));
    // The People page over the inspector hides it; the resident stays
    // selected, so their row is still the marked one.
    toggle_page(app.world_mut(), LedgerPage::People);
    assert!(!crate::resident_inspector::open(app.world()));
    assert_eq!(app.world().resource::<Selected>().0, Some(resident));
    assert!(roster_ids(&entries(app.world())).contains(&resident));
    // Their row again is no new selection, and still hands the column back
    // to their inspector.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Resident(resident));
    process(app.world_mut());
    assert_eq!(page(app.world()), None);
    assert!(crate::resident_inspector::open(app.world()));
    assert_eq!(app.world().resource::<Selected>().0, Some(resident));
    toggle_page(app.world_mut(), LedgerPage::People);
    // Esc (Close) empties the column: the selection goes with it.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Close);
    process(app.world_mut());
    assert_eq!(page(app.world()), None);
    assert_eq!(app.world().resource::<Selected>().0, None);
    assert!(!crate::resident_inspector::open(app.world()));
    // F11 reopens the last page.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Toggle);
    process(app.world_mut());
    assert_eq!(page(app.world()), Some(LedgerPage::People));
    // The Journal page lists the feed with its three filters.
    app.init_resource::<crate::island_alerts::Alerts>();
    toggle_page(app.world_mut(), LedgerPage::Journal);
    let rows = entries(app.world());
    let filters = rows
        .iter()
        .find(|entry| entry.key == "journal_filter")
        .and_then(|entry| entry.choices.clone())
        .unwrap();
    assert_eq!(
        filters
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["Important", "Critical", "Everything"]
    );
    assert!(rows.iter().any(|entry| entry.key == "journal_empty"));
    // Leaving Oversee closes the column; nothing here touched the simulation.
    switch_mode(app.world_mut(), IslandMode::Editor);
    assert_eq!(page(app.world()), None);
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn the_season_card_closes_by_itself_after_a_day_of_running_time() {
    let day = 2400;
    assert!(
        !season_card_expired(false, Some(0), 10 * day, day),
        "not open"
    );
    assert!(
        !season_card_expired(true, None, 10 * day, day),
        "no opening tick"
    );
    assert!(!season_card_expired(true, Some(100), 100 + day - 1, day));
    assert!(season_card_expired(true, Some(100), 100 + day, day));
}

#[test]
fn colony_policy_button_queues_and_reports_the_actual_application() {
    let mut app = fixture();
    let before = state_hash(app.world_mut());
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Reserve(4));
    process(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<colony::ColonyState>()
            .food_reserve_days,
        2
    );
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "button queues rather than mutating authoritative state"
    );
    assert!(app.world().resource::<Ledger>().pending.is_some());
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<colony::ColonyState>()
            .food_reserve_days,
        4
    );
    assert!(app.world().resource::<Ledger>().pending.is_none());
    assert!(
        app.world()
            .resource::<Ledger>()
            .message
            .contains("set to 4 days")
    );
}

#[test]
fn blocked_terrain_edit_is_explained_without_queuing_it() {
    let mut app = fixture();
    let before = state_hash(app.world_mut());
    let command = Terraform {
        x: 100,
        z: 0,
        radius: 0,
        height_mm: 0,
        surface: TerrainSurface::Grass,
    };
    queue(
        app.world_mut(),
        SimCommand::content(&command),
        "unexpected success".into(),
    );
    assert!(
        app.world()
            .resource::<Ledger>()
            .message
            .starts_with("The village could not do that.")
    );
    assert!(app.world().resource::<Ledger>().pending.is_none());
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn closing_landscape_tools_ends_painting_without_mutating_the_world() {
    // Developer launches have no session bar: the ledger is still the editor.
    for action in [Action::Close, Action::Toggle] {
        let mut app = fixture();
        switch_mode(app.world_mut(), IslandMode::Editor);
        assert_eq!(app.world().resource::<Ledger>().tab, Tab::Landscape);
        let before = state_hash(app.world_mut());
        app.world_mut()
            .resource_mut::<Ledger>()
            .requests
            .push(action);
        process(app.world_mut());
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        assert!(!app.world().resource::<Ledger>().open);
        assert_eq!(before, state_hash(app.world_mut()));
    }
}

#[test]
fn housing_controls_offer_another_resident_when_the_carpenter_is_committed() {
    let mut app = fixture();
    // Commissions live on the Projects page, with the building sites.
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Projects;
    let first = entries(app.world())
        .into_iter()
        .find_map(|entry| match entry.action {
            Some(Action::Commission(builder, plot)) => Some((builder, plot)),
            _ => None,
        })
        .expect("a housing site can be commissioned");
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Commission(first.0, first.1));
    process(app.world_mut());
    run_ticks(app.world_mut(), 1);
    // The first pass settled the mode and reset the developer ledger's tab.
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Projects;
    let next = entries(app.world())
        .into_iter()
        .find_map(|entry| match entry.action {
            Some(Action::Commission(builder, plot)) => Some((builder, plot)),
            _ => None,
        })
        .expect("remaining empty sites are still offered");
    assert_ne!(
        first.0, next.0,
        "one carpenter cannot hide every future housing option"
    );
    assert_ne!(first.1, next.1);
}

fn roster_row(
    id: u32,
    name: &str,
    place: (u32, &str),
    job: &str,
    mood: i32,
    coins: u32,
) -> RosterRow {
    RosterRow {
        id: SimId(id),
        name: name.into(),
        settlement: Some(SimId(place.0)),
        place: place.1.into(),
        profession: job.into(),
        mood,
        coins,
        activity: "gathering".into(),
        need: NeedLevel::Fine,
    }
}

fn sample_roster() -> Vec<RosterRow> {
    vec![
        roster_row(4, "Mira", (90, "Harbour"), "fisher", 62_000, 9),
        roster_row(2, "ada", (91, "Alderwood"), "woodcutter", 97_000, 40),
        roster_row(3, "Bram", (90, "Harbour"), "baker", 31_000, 40),
        roster_row(1, "Cole", (92, "Stone Ridge"), "miner", 62_000, 0),
    ]
}

fn names_of(rows: &[RosterRow]) -> Vec<&str> {
    rows.iter().map(|row| row.name.as_str()).collect()
}

#[test]
fn the_roster_sorts_by_each_key_and_breaks_ties_by_name() {
    let sorted = |key| arrange(sample_roster(), key, None);
    assert_eq!(
        names_of(&sorted(SortKey::Name)),
        ["ada", "Bram", "Cole", "Mira"],
        "case does not decide the order"
    );
    assert_eq!(
        names_of(&sorted(SortKey::Settlement)),
        ["ada", "Bram", "Mira", "Cole"]
    );
    assert_eq!(
        names_of(&sorted(SortKey::Profession)),
        ["Bram", "Mira", "Cole", "ada"]
    );
    assert_eq!(
        names_of(&sorted(SortKey::Mood)),
        ["Bram", "Cole", "Mira", "ada"],
        "lowest mood first; equal moods by name"
    );
    assert_eq!(
        names_of(&sorted(SortKey::Wealth)),
        ["ada", "Bram", "Mira", "Cole"],
        "richest first; equal purses by name"
    );
    // The result does not depend on the order the rows came in.
    let mut reversed = sample_roster();
    reversed.reverse();
    for key in SortKey::ALL {
        assert_eq!(arrange(reversed.clone(), key, None), sorted(key));
    }
}

#[test]
fn the_roster_filters_by_settlement_before_sorting() {
    let harbour = arrange(sample_roster(), SortKey::Wealth, Some(SimId(90)));
    assert_eq!(names_of(&harbour), ["Bram", "Mira"]);
    assert!(arrange(sample_roster(), SortKey::Name, Some(SimId(7))).is_empty());
    assert_eq!(arrange(sample_roster(), SortKey::Name, None).len(), 4);
}

#[test]
fn a_roster_row_says_who_where_what_and_how_they_fare() {
    let mut row = roster_row(3, "Bram", (90, "Harbour"), "baker", 31_400, 1);
    row.activity = "buying water".into();
    assert_eq!(
        row.text(),
        "Bram · baker · Harbour\nbuying water · mood 31% · 1 coin"
    );
    row.need = NeedLevel::Critical;
    row.coins = 12;
    assert!(row.text().ends_with("12 coins · in danger"));
    row.need = NeedLevel::Low;
    assert!(row.text().ends_with("· in need"));
}

fn living_fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<Ledger>()
        .init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<Selected>()
        .init_resource::<economy::StockTrends>()
        .init_resource::<player_actions::NativeActions>()
        .add_plugins(crate::island_control::plugin);
    // The first pass settles the mode, which opens the ledger on its Village tab.
    process(app.world_mut());
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Residents;
    app
}

fn roster_ids(rows: &[Entry]) -> Vec<SimId> {
    rows.iter()
        .filter_map(|entry| match entry.action {
            Some(Action::Resident(id)) => Some(id),
            _ => None,
        })
        .collect()
}

#[test]
fn the_residents_tab_lists_everyone_with_activity_and_a_need_indicator() {
    let mut app = living_fixture();
    run_ticks(app.world_mut(), 200);
    let before = state_hash(app.world_mut());
    let rows = entries(app.world());
    let listed = roster_ids(&rows);
    assert_eq!(listed.len(), 42, "every living resident, never the visitor");
    for entry in rows
        .iter()
        .filter(|entry| matches!(entry.action, Some(Action::Resident(_))))
    {
        assert!(entry.indicator.is_some(), "{}", entry.text);
        assert_eq!(entry.text.lines().count(), 2, "{}", entry.text);
        assert!(entry.text.contains("mood "), "{}", entry.text);
        for jargon in ["life.", "village_", "finishing the current activity"] {
            assert!(!entry.text.contains(jargon), "{}", entry.text);
        }
    }
    // Sort and settlement choices are offered, and both narrow or reorder.
    let choices = |key: &str| {
        rows.iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.choices.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        choices("roster_sort")
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["Name", "Place", "Job", "Mood", "Wealth"]
    );
    let scopes = choices("roster_scopes");
    assert_eq!(scopes.len(), 4, "the island and its three settlements");
    let Action::Scope(Some(settlement)) = scopes[1].1 else {
        panic!("a settlement choice");
    };
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .extend([Action::Scope(Some(settlement)), Action::Sort(SortKey::Mood)]);
    process(app.world_mut());
    let filtered = entries(app.world());
    let ids = roster_ids(&filtered);
    assert_eq!(ids.len(), 14);
    let mood = |id: &SimId| {
        let entity = app.world().resource::<SimIndex>().get(*id).unwrap();
        app.world()
            .get::<colony::SocialLife>(entity)
            .unwrap()
            .morale
    };
    assert!(ids.windows(2).all(|pair| mood(&pair[0]) <= mood(&pair[1])));
    assert!(
        filtered
            .iter()
            .any(|entry| entry.key == "roster_note" && entry.text.contains("lowest mood first"))
    );
    assert_eq!(before, state_hash(app.world_mut()), "the roster only reads");
}

#[test]
fn choosing_a_roster_row_opens_the_inspector_where_job_and_home_are_arranged() {
    let mut app = living_fixture();
    app.init_resource::<crate::resident_inspector::Inspector>();
    let before = state_hash(app.world_mut());
    let resident = roster_ids(&entries(app.world()))[0];
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Resident(resident));
    process(app.world_mut());
    assert_eq!(app.world().resource::<Selected>().0, Some(resident));
    assert!(crate::resident_inspector::open(app.world()));
    let rows = entries(app.world());
    assert_eq!(roster_ids(&rows).len(), 42, "the roster stays in place");
    assert!(
        !rows.iter().any(|entry| matches!(
            entry.action,
            Some(Action::Home(..)) | Some(Action::Profession(..))
        )),
        "the People page lists and filters; it arranges nothing itself"
    );
    // One step from the row: the inspector's Home and job tab, opened by an
    // action that travels the ledger's own queue.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::InspectorTab(
            crate::resident_inspector::Tab::HomeAndJob,
        ));
    process(app.world_mut());
    crate::resident_inspector::process(app.world_mut());
    let rows = crate::resident_inspector::entries(app.world());
    assert!(rows.iter().any(|entry| entry.key == "employment"));
    assert!(roster_ids(&rows).is_empty());
    // Without a selection the inspector has nothing to list.
    app.world_mut().resource_mut::<Selected>().0 = None;
    assert!(crate::resident_inspector::entries(app.world()).is_empty());
    assert_eq!(roster_ids(&entries(app.world())).len(), 42);
    assert_eq!(before, state_hash(app.world_mut()));
}

/// A spot near the Harbour where the simulation accepts `building`.
fn buildable(app: &App, building: &str) -> (i32, i32, u8) {
    let world = app.world();
    let settlement = world
        .resource::<island_content::IslandIds>()
        .id("settlement_harbour");
    let def = village::civic::definition(world, building).expect("definition");
    village::civic::find_site(world, def, settlement, 36).expect("a buildable spot")
}

#[test]
fn the_projects_page_lists_projects_and_cancels_a_site_only_after_asking() {
    let mut app = living_fixture();
    app.init_resource::<crate::island_build::BuildTool>()
        .init_resource::<crate::island_build::CivicLog>();
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Projects;
    let rows = entries(app.world());
    let text = |rows: &[Entry], key: &str| {
        rows.iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.text.clone())
    };
    assert_eq!(
        text(&rows, "civic_title").as_deref(),
        Some("Building projects")
    );
    // The fund is two numbers, whatever the simulation's rules make them;
    // the sentence that explains them is the row's tooltip.
    let fund = |rows: &[Entry]| {
        rows.iter()
            .find(|entry| entry.key == "civic_fund")
            .and_then(|entry| entry.stats.clone())
            .expect("the civic fund's two numbers")
    };
    let numbers = fund(&rows);
    assert_eq!(
        numbers
            .iter()
            .map(|(_, label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["To spend", "Held for open sites"]
    );
    assert_eq!(
        numbers[0].2,
        village::civic::budget(app.world()).to_string()
    );
    assert_eq!(
        numbers[1].2,
        village::civic::fund_hold(app.world()).to_string()
    );
    assert!(
        rows.iter()
            .find(|entry| entry.key == "civic_fund")
            .is_some_and(|entry| entry.tip.is_some())
    );
    assert!(text(&rows, "civic_none").is_some(), "no site is open yet");
    // What the planner sees is folded away until asked for.
    let planner_lines = |rows: &[Entry]| {
        rows.iter()
            .filter(|entry| {
                entry.key.starts_with("civic_planner") && entry.key != "civic_planner_title"
            })
            .count()
    };
    assert_eq!(planner_lines(&rows), 0);
    assert!(matches!(
        rows.iter()
            .find(|entry| entry.key == "civic_planner_title")
            .and_then(|entry| entry.extra.clone()),
        Some(Extra::Fold(false))
    ));
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Fold("planner"));
    process(app.world_mut());
    assert_eq!(
        planner_lines(&entries(app.world())),
        3,
        "one planner line per settlement"
    );
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Fold("planner"));
    process(app.world_mut());
    assert_eq!(planner_lines(&entries(app.world())), 0);
    assert_eq!(
        rows.iter().position(|entry| entry.key == "civic_title"),
        Some(0),
        "the page opens on the building projects"
    );
    assert!(
        !rows.iter().any(|entry| entry.key == "policy_title"),
        "policies stay on the Village page"
    );

    // Place a stockpile through the same recorded command the build menu uses.
    let (x, z, rotation) = buildable(&app, "stockpile");
    let place = village::civic::PlaceBuilding {
        building: "stockpile".into(),
        x,
        z,
        rotation,
    };
    submit(app.world_mut(), None, SimCommand::content(&place)).expect("placement recorded");
    run_ticks(app.world_mut(), 2);
    let before = state_hash(app.world_mut());
    let rows = entries(app.world());
    let site = rows
        .iter()
        .find_map(|entry| match entry.action {
            Some(Action::Site(id)) => Some((id, entry.text.clone())),
            _ => None,
        })
        .expect("a row for the open site");
    assert!(site.1.contains("stockpile") && site.1.contains("logs 0/4"));
    // The fund's rules belong to the simulation: whatever they are, the row
    // says the fund cannot pay exactly when it has nothing to spend.
    assert_eq!(
        site.1.contains("the civic fund cannot pay"),
        village::civic::budget(app.world()) == 0,
        "the row follows the civic budget: {}",
        site.1
    );
    assert!(text(&rows, "civic_none").is_none());
    assert_eq!(
        fund(&rows)[1].2,
        village::civic::fund_hold(app.world()).to_string(),
        "what is held follows the simulation"
    );
    // The site row carries its progress, its materials and its state in words.
    let Some(Extra::Site {
        percent,
        materials,
        status,
    }) = rows
        .iter()
        .find(|entry| entry.action == Some(Action::Site(site.0)))
        .and_then(|entry| entry.extra.clone())
    else {
        panic!("a site row");
    };
    assert!(percent <= 100);
    assert!(
        materials.iter().any(|(_, text)| text == "logs 0/4"),
        "{materials:?}"
    );
    assert!(!status.is_empty());
    let cancel = rows
        .iter()
        .filter_map(|entry| entry.choices.as_ref())
        .flatten()
        .find(|(_, action)| *action == Action::CancelSite(site.0))
        .expect("the way to abandon the site, under its row");
    assert!(cancel.0.starts_with("Abandon "), "{}", cancel.0);
    assert_eq!(before, state_hash(app.world_mut()), "the panel only reads");

    // Cancel asks first; keeping the site records nothing.
    let site_choices = |app: &App| {
        entries(app.world())
            .iter()
            .filter(|entry| entry.key.starts_with("civic_"))
            .filter_map(|entry| entry.choices.clone())
            .flatten()
            .map(|(_, action)| action)
            .collect::<Vec<_>>()
    };
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::CancelSite(site.0));
    process(app.world_mut());
    // The safe answer comes first; the question names the site.
    assert_eq!(
        site_choices(&app),
        [Action::KeepSite, Action::ConfirmCancelSite(site.0)]
    );
    assert!(entries(app.world()).iter().any(|entry| {
        entry.key.starts_with("civic_ask")
            && entry.text.starts_with("Abandon ")
            && entry.text.ends_with('?')
    }));
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::KeepSite);
    process(app.world_mut());
    run_ticks(app.world_mut(), 2);
    assert_eq!(site_choices(&app), [Action::CancelSite(site.0)]);
    assert_eq!(crate::island_build::site_rows(app.world()).len(), 1);

    // Confirmed, it is one recorded CancelBuilding.
    app.world_mut().resource_mut::<Ledger>().requests.extend([
        Action::CancelSite(site.0),
        Action::ConfirmCancelSite(site.0),
    ]);
    process(app.world_mut());
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert!(crate::island_build::site_rows(app.world()).is_empty());
    assert!(
        app.world()
            .resource::<Ledger>()
            .message
            .starts_with("Site abandoned"),
        "{}",
        app.world().resource::<Ledger>().message
    );
    assert!(text(&entries(app.world()), "civic_none").is_some());

    // Choosing a site row frames it and opens its card.
    submit(app.world_mut(), None, SimCommand::content(&place)).expect("placement recorded");
    run_ticks(app.world_mut(), 2);
    let site = crate::island_build::site_rows(app.world())[0].0.id;
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Site(SimId(site)));
    process(app.world_mut());
    assert_eq!(
        crate::island_build::card_subject(app.world()),
        Some(SimId(site))
    );
}

#[test]
fn a_world_that_cannot_build_has_no_projects_section() {
    let mut app = fixture();
    process(app.world_mut());
    for tab in [Tab::Overview, Tab::Projects] {
        app.world_mut().resource_mut::<Ledger>().tab = tab;
        assert!(
            !entries(app.world())
                .iter()
                .any(|entry| entry.key.starts_with("civic_"))
        );
    }
}

#[test]
fn the_census_counts_families_and_says_what_changed_since_the_founding() {
    use island_content::village::view::Census;
    use std::collections::BTreeMap;
    let quiet = Census {
        residents: 42,
        children: 0,
        elders: 0,
        couples: 0,
        births: 0,
        came_of_age: 0,
        pairings: 0,
        arrivals: 0,
        deaths: BTreeMap::new(),
    };
    // A world the census does not count shows no block at all.
    assert!(
        model::census_entries(
            &Census {
                residents: 0,
                ..quiet.clone()
            },
            0
        )
        .is_empty()
    );
    // A young island has its families block and says nothing has changed.
    let rows = model::census_entries(&quiet, 42);
    assert!(
        rows.iter()
            .any(|entry| entry.key == "census_title" && entry.text == "Families")
    );
    let stats = rows
        .iter()
        .find(|entry| entry.key == "census")
        .and_then(|entry| entry.stats.clone())
        .expect("the four numbers");
    assert_eq!(
        stats
            .iter()
            .map(|(_, label, value)| format!("{label} {value}"))
            .collect::<Vec<_>>(),
        ["Households 42", "Couples 0", "Children 0", "Elders 0"]
    );
    assert!(rows.iter().any(
        |entry| entry.key == "census_changes" && entry.text.starts_with("Nobody has been born")
    ));
    let lived = Census {
        children: 3,
        couples: 5,
        births: 2,
        arrivals: 1,
        deaths: BTreeMap::from([("old age".to_owned(), 2), ("cold".to_owned(), 1)]),
        ..quiet
    };
    let rows = model::census_entries(&lived, 40);
    let changes = rows
        .iter()
        .find(|entry| entry.key == "census_changes")
        .expect("what changed");
    assert_eq!(
        changes.text,
        "Since the island was founded: 2 births · 1 arrival · 3 deaths."
    );
}

#[test]
fn the_people_page_remembers_the_departed_newest_first() {
    use island_content::village::view::Departed;
    assert!(model::departed_entries(&[], None).is_empty());
    let record = |name: &str, role: &str, seasons, cause: &str, day, settlement, rest| Departed {
        resident: (!role.is_empty()).then_some(SimId(7)),
        name: name.to_owned(),
        role: role.to_owned(),
        age_seasons: seasons,
        cause: cause.to_owned(),
        day,
        tick: day * 2400,
        household: None,
        settlement: Some(SimId(settlement)),
        laid_to_rest: rest,
    };
    let departed = [
        record("Bram", "smith", 38, "cold", 11, 1, true),
        record("Tilly", "", 3, "hunger", 14, 2, true),
        record("Odo", "fisher", 4, "old age", 20, 1, false),
    ];
    let rows = model::departed_entries(&departed, None);
    assert_eq!(
        rows.iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Departed",
            "Odo, fisher. Died of old age on day 21, aged 1 year. Not yet laid to rest.",
            "Tilly, a child. Died of hunger on day 15, aged 3 seasons.",
            "Bram, smith. Died of cold on day 12, aged 9 years.",
        ]
    );
    // A settlement's filter applies to them as to the roster.
    let harbour = model::departed_entries(&departed, Some(SimId(1)));
    assert_eq!(harbour.len(), 3);
    assert!(model::departed_entries(&departed, Some(SimId(9))).is_empty());
    assert_eq!(model::age_words(1), "1 season");
    assert_eq!(model::age_words(8), "2 years");
    // A long list is cut, and says how many it left out.
    let many: Vec<Departed> = (0..15)
        .map(|day| record("Ivo", "baker", 20, "old age", day, 1, true))
        .collect();
    let rows = model::departed_entries(&many, None);
    assert_eq!(rows.len(), 14);
    assert_eq!(rows.last().unwrap().text, "And 3 before them.");
}

#[test]
fn the_narrow_table_gives_its_numbers_room_for_the_sort_arrow() {
    for compact in [false, true] {
        let widths = widgets::column_widths(compact);
        assert_eq!(widths.iter().sum::<f32>(), 100.0, "the row is filled");
    }
    let (wide, narrow) = (widgets::column_widths(false), widgets::column_widths(true));
    // "Price" and its arrow need about 44 px: 18% of the narrow table's 276.
    assert!(narrow[1] > wide[1] && narrow[2] > wide[2]);
    assert!(narrow[2] * 2.76 >= 44.0);
    assert!(
        narrow[3] < wide[3],
        "the last header is one short word there"
    );
}

fn oversee_fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<Ledger>()
        .init_resource::<IslandMode>()
        .init_resource::<InputFocus>()
        .init_resource::<SimControl>()
        .init_resource::<economy::StockTrends>()
        .init_resource::<player_actions::NativeActions>()
        .add_plugins(crate::island_control::plugin);
    // The first pass settles the mode, which opens the ledger on its Village tab.
    process(app.world_mut());
    app
}

/// The first village of the living world, chosen in the Economy page.
fn open_village(app: &mut App, page: economy::EconomyPage) -> npc_framework::prelude::SimId {
    let village = economy::scopes(app.world())
        .into_iter()
        .find_map(|(scope, _)| scope)
        .expect("the living world has villages");
    let mut ledger = app.world_mut().resource_mut::<Ledger>();
    ledger.open = true;
    ledger.tab = Tab::Economy;
    ledger.village_scope = Some(village);
    ledger.economy = page;
    village
}

/// The chips of the Economy page's scope row, empty when it has none.
fn scope_chips(rows: &[Entry]) -> Vec<(String, Action)> {
    rows.iter()
        .find(|entry| entry.key == "stock_scopes")
        .and_then(|entry| entry.choices.clone())
        .unwrap_or_default()
}

#[test]
fn jobs_and_priorities_open_on_a_village() {
    let mut app = oversee_fixture();
    {
        let mut ledger = app.world_mut().resource_mut::<Ledger>();
        ledger.open = true;
        ledger.tab = Tab::Economy;
        ledger.economy = economy::EconomyPage::Goods;
        ledger.scope = None;
    }
    // Goods offers the whole island and every village.
    let goods = scope_chips(&entries(app.world()));
    assert_eq!(goods.len(), 4, "the island and its three villages");
    assert_eq!(goods[0].0, "Whole island");
    // Jobs: a village is chosen, the one with most residents when the
    // camera is over none, and the row holds the villages only.
    let most = economy::pick_village(app.world(), None).expect("a village");
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::EconomyPage(economy::EconomyPage::Jobs));
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().village_scope, Some(most));
    let rows = entries(app.world());
    assert!(rows.iter().any(|entry| entry.key == "targets_header"));
    assert!(rows.iter().all(|entry| !entry.key.ends_with("_choose")));
    let chips = scope_chips(&rows);
    assert_eq!(chips.len(), 3, "the villages only");
    assert!(
        chips
            .iter()
            .all(|(_, action)| !matches!(action, Action::Scope(None)))
    );
    // Another village's chip is remembered for Jobs and Priorities alike.
    let Some((_, Action::Scope(Some(other)))) = chips
        .iter()
        .find(|(_, action)| !matches!(action, Action::Scope(Some(id)) if *id == most))
        .cloned()
    else {
        panic!("another village");
    };
    app.world_mut().resource_mut::<Ledger>().requests.extend([
        Action::Scope(Some(other)),
        Action::EconomyPage(economy::EconomyPage::Priorities),
    ]);
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().village_scope, Some(other));
    let rows = entries(app.world());
    assert!(
        rows.iter()
            .any(|entry| entry.key == format!("priority_{}_housing_all", other.0))
    );
    // Goods returns to the scope it had: the whole island.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::EconomyPage(economy::EconomyPage::Goods));
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().scope, None);
    assert_eq!(scope_chips(&entries(app.world())).len(), 4);
    // A camera over a village picks that one.
    let centre = |id: SimId| {
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        npc_framework::view_helpers::sim_to_world(
            *app.world()
                .get::<npc_framework::prelude::SimPos>(entity)
                .unwrap(),
        )
    };
    let near = centre(other) + Vec3::new(20.0, 0.0, 0.0);
    assert_eq!(economy::pick_village(app.world(), Some(near)), Some(other));
}

/// The one recorded command waiting in the queue, as sent.
fn queued_command(app: &App) -> serde_json::Value {
    let queue = app
        .world()
        .resource::<npc_framework::kernel::commands::CommandQueue>();
    assert_eq!(queue.pending.len(), 1, "one command per press");
    serde_json::to_value(queue.pending.values().next().unwrap()).unwrap()
}

#[test]
fn a_villages_jobs_table_sends_the_wanted_headcount_and_hands_it_back() {
    use village::frontier::oversee;
    let mut app = oversee_fixture();
    let village = open_village(&mut app, economy::EconomyPage::Jobs);
    let rows = entries(app.world());
    assert!(rows.iter().any(|entry| entry.key == "targets_header"));
    let table = oversee::profession_table(app.world(), village);
    let trade = table
        .iter()
        .find(|row| row.adjustable)
        .expect("an adjustable trade")
        .clone();
    let key = format!("target_{}_{}", village.0, trade.key);
    let row = rows.iter().find(|entry| entry.key == key).expect("its row");
    let columns = row.columns.clone().unwrap();
    assert_eq!(
        columns[0].to_lowercase(),
        trade.display.to_lowercase(),
        "the trade by its display name"
    );
    assert_eq!(columns[1], trade.current.to_string());
    assert_eq!(columns[2], "\u{2013}", "the village decides at first");
    let controls = rows
        .iter()
        .find(|entry| entry.key == format!("{key}_set"))
        .and_then(|entry| entry.choices.clone())
        .expect("fewer, more and the village decides");
    assert_eq!(
        controls
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["\u{2212}", "+", "Village decides"]
    );
    // A service trade has no controls and says why, in the simulation's words.
    for fixed in table.iter().filter(|row| !row.adjustable) {
        let key = format!("target_{}_{}", village.0, fixed.key);
        assert!(!rows.iter().any(|entry| entry.key == format!("{key}_set")));
        assert!(
            rows.iter()
                .any(|entry| entry.key == format!("{key}_fixed") && !entry.text.is_empty())
        );
    }

    let before = state_hash(app.world_mut());
    let more = controls[1].1.clone();
    app.world_mut().resource_mut::<Ledger>().requests.push(more);
    process(app.world_mut());
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "the press records a command and changes nothing itself"
    );
    assert_eq!(
        queued_command(&app),
        serde_json::json!({
            "type": "SetProfessionTarget",
            "settlement": village.0,
            "profession": trade.key,
            "wanted": trade.current + 1,
        })
    );
    // The Paused pill counts the player's own order (closing phase, 2).
    assert_eq!(orders_waiting(app.world()), 1);
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert_eq!(orders_waiting(app.world()), 0, "applied: nothing waits");
    // A command the game submits itself is not the player's order.
    let Action::Target { step, .. } = controls[1].1.clone() else {
        panic!("a Target choice");
    };
    let mut command = economy::target_command(app.world(), village, &trade.key, step).unwrap();
    // The same number again: the order below still reads as before.
    command.wanted = Some(trade.current + 1);
    npc_framework::prelude::submit(app.world_mut(), None, SimCommand::content(&command)).unwrap();
    assert_eq!(
        orders_waiting(app.world()),
        0,
        "a raw submit is not counted"
    );
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert_eq!(
        oversee::target(app.world(), village, Role::from_key(&trade.key)),
        Some(trade.current + 1)
    );
    let rows = entries(app.world());
    let row = rows.iter().find(|entry| entry.key == key).unwrap();
    assert_eq!(
        row.columns.as_ref().unwrap()[2],
        (trade.current + 1).to_string()
    );
    let message = app.world().resource::<Ledger>().message.clone();
    assert!(message.contains("wanted"), "{message}");
    // Now and wanted differ: "Your orders" says how the job stands, once;
    // no second line under the row.
    let order = rows
        .iter()
        .find(|entry| entry.key == format!("order_{}_job_{}", village.0, trade.key))
        .expect("the order line");
    assert!(order.text.contains("wanted"), "{}", order.text);
    assert!(rows.iter().all(|entry| entry.key != format!("{key}_why")));

    // The village decides again.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(controls[2].1.clone());
    process(app.world_mut());
    assert_eq!(queued_command(&app)["wanted"], serde_json::Value::Null);
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert_eq!(
        oversee::target(app.world(), village, Role::from_key(&trade.key)),
        None
    );
    // Asking for what already holds records nothing and says so.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(controls[2].1.clone());
    process(app.world_mut());
    assert!(
        app.world()
            .resource::<npc_framework::kernel::commands::CommandQueue>()
            .pending
            .is_empty()
    );
    let message = app.world().resource::<Ledger>().message.clone();
    assert!(
        message.starts_with("The village already decides"),
        "{message}"
    );
}

#[test]
fn a_villages_priority_sends_its_level_and_says_what_it_changes() {
    use village::frontier::oversee::{self, PriorityLevel, PriorityTopic};
    let mut app = oversee_fixture();
    let village = open_village(&mut app, economy::EconomyPage::Priorities);
    let rows = entries(app.world());
    let key = format!("priority_{}_housing_all", village.0);
    let choices = rows
        .iter()
        .find(|entry| entry.key == key)
        .and_then(|entry| entry.choices.clone())
        .expect("the housing row");
    assert_eq!(
        choices
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["Low", "Normal", "High"]
    );
    let normal = rows
        .iter()
        .find(|entry| entry.key == format!("{key}_effect"))
        .unwrap()
        .text
        .clone();
    assert!(!normal.is_empty());
    for topic in PriorityTopic::ALL {
        let row = format!("priority_{}_{}_all", village.0, topic.key());
        assert!(rows.iter().any(|entry| entry.key == row), "{topic:?}");
    }

    let before = state_hash(app.world_mut());
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(choices[2].1.clone());
    process(app.world_mut());
    assert_eq!(before, state_hash(app.world_mut()));
    let sent = queued_command(&app);
    assert_eq!(sent["type"], "SetVillagePriority");
    assert_eq!(sent["settlement"], village.0);
    assert_eq!(sent["topic"], "housing");
    assert_eq!(sent["level"], "high");
    assert!(sent["profession"].is_null());
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    assert_eq!(
        oversee::level(app.world(), village, PriorityTopic::Housing, None),
        PriorityLevel::High
    );
    assert_eq!(
        economy::priority_level(app.world(), village, "housing", None),
        PriorityLevel::High,
        "the segment the ledger lights"
    );
    let high = entries(app.world())
        .into_iter()
        .find(|entry| entry.key == format!("{key}_effect"))
        .unwrap()
        .text;
    assert_ne!(high, normal, "the line says what the new level changes");
    assert_eq!(app.world().resource::<Ledger>().message, high);

    // The whole island has no priorities of its own: no chip offers it.
    let rows = entries(app.world());
    assert!(
        scope_chips(&rows)
            .iter()
            .all(|(_, action)| !matches!(action, Action::Scope(None)))
    );
}

/// The Goods page of the whole island with `good` chosen, its figures read.
fn goods_view(app: &mut App, good: &str, compact: bool, more: bool) -> Vec<Entry> {
    goods::refresh_cache(app.world_mut(), None, Some(good), None);
    economy::entries(
        app.world(),
        economy::View {
            page: economy::EconomyPage::Goods,
            good: Some(good),
            compact,
            more,
            ..Default::default()
        },
    )
}

/// A traded good of the living world.
fn a_traded_good(app: &App) -> String {
    economy::stock_rows(app.world(), None)
        .into_iter()
        .find(|row| row.price.is_some())
        .expect("a traded good")
        .kind
}

#[test]
fn the_card_sits_under_its_row() {
    let mut app = oversee_fixture();
    let good = a_traded_good(&app);
    let rows = goods_view(&mut app, &good, false, false);
    let keys: Vec<&str> = rows.iter().map(|entry| entry.key.as_str()).collect();
    let at = keys
        .iter()
        .position(|key| *key == format!("stock_{good}"))
        .expect("the good's row");
    assert_eq!(keys[at + 1], "stock_detail", "{keys:?}");
    let Some(Extra::Card(lines)) = &rows[at + 1].extra else {
        panic!("a card");
    };
    assert!(
        lines.iter().any(|line| line.starts_with("Price ")),
        "{lines:?}"
    );
    // One card, and the table goes on under it.
    assert_eq!(keys.iter().filter(|key| **key == "stock_detail").count(), 1);
}

#[test]
fn the_narrow_ledger_folds_the_rest_of_the_card() {
    let mut app = oversee_fixture();
    let good = a_traded_good(&app);
    let card = |rows: &[Entry], key: &str| {
        rows.iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| match &entry.extra {
                Some(Extra::Card(lines)) => Some(lines.clone()),
                _ => None,
            })
    };
    let folded = goods_view(&mut app, &good, true, false);
    let kept = card(&folded, "stock_detail").expect("the card");
    assert!(
        kept.iter().any(|line| line.starts_with("Price ")),
        "{kept:?}"
    );
    assert!(
        kept.iter()
            .all(|line| !line.starts_with("Made by") && !line.starts_with("Nobody makes")),
        "line 9 folds: {kept:?}"
    );
    let fold = format!("stock_more_{good}");
    assert!(folded.iter().any(|entry| entry.key == fold));
    assert!(card(&folded, "stock_detail_more").is_none());
    let open = goods_view(&mut app, &good, true, true);
    let rest = card(&open, "stock_detail_more").expect("the folded lines");
    assert!(
        rest.iter()
            .any(|line| line.starts_with("Made by") || line.starts_with("Nobody makes")),
        "{rest:?}"
    );
}

#[test]
fn a_priority_title_carries_its_figure() {
    use island_content::village::view::Outlook;
    let outlook = Outlook {
        day: 20,
        season: village::Season::Autumn,
        population: 10,
        food_days: Some(4),
        water_days: Some(9),
        fuel_days: Some(6),
        beds_free: 0,
        homeless: 2,
        idle_hands: 0,
        open_places: 3,
        cold_in_days: None,
        cold_now: false,
        without_coat: 11,
        next_shortage: None,
    };
    let figure = |topic| economy::topic_figure(topic, Some(&outlook), 3, Some(2));
    assert_eq!(figure("food_reserve").as_deref(), Some("4 days in store"));
    assert_eq!(figure("fuel_reserve").as_deref(), Some("6 days in store"));
    assert_eq!(figure("coats").as_deref(), Some("11 residents without"));
    assert_eq!(
        figure("housing").as_deref(),
        Some("2 residents without a home")
    );
    assert_eq!(figure("workplace").as_deref(), Some("3 open places"));
    assert_eq!(
        figure("boat_orders").as_deref(),
        Some("next call in 2 days")
    );
    assert_eq!(figure("storage"), None);
    assert_eq!(economy::topic_figure("coats", None, 0, None), None);
    // In the ledger: the Workplaces title carries its open places.
    let mut app = oversee_fixture();
    let village = open_village(&mut app, economy::EconomyPage::Priorities);
    let rows = entries(app.world());
    let title = rows
        .iter()
        .find(|entry| entry.key == format!("priority_{}_workplace_all_title", village.0))
        .expect("the Workplaces title");
    assert!(
        title.text.starts_with("Workplaces \u{b7} "),
        "{}",
        title.text
    );
    assert!(title.text.contains("open place"), "{}", title.text);
}

#[test]
fn one_job_can_have_its_own_workplace_level() {
    use village::frontier::oversee::PriorityLevel;
    let mut app = oversee_fixture();
    let village = open_village(&mut app, economy::EconomyPage::Priorities);
    let link = format!("priority_{}_workplace_all_one_job", village.0);
    assert!(entries(app.world()).iter().any(|entry| entry.key == link));
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::JobLevels);
    process(app.world_mut());
    let chips = entries(app.world())
        .into_iter()
        .find(|entry| entry.key == format!("priority_{}_workplace_all_jobs", village.0))
        .and_then(|entry| entry.choices)
        .expect("the jobs without their own level");
    let (_, chip) = chips[0].clone();
    let Action::Priority {
        profession: Some(job),
        level: PriorityLevel::High,
        ..
    } = chip.clone()
    else {
        panic!("a chip gives its job its own level: {chip:?}");
    };
    app.world_mut().resource_mut::<Ledger>().requests.push(chip);
    process(app.world_mut());
    run_ticks(app.world_mut(), 1);
    process(app.world_mut());
    let rows = entries(app.world());
    let own = rows
        .iter()
        .find(|entry| entry.key == format!("priority_{}_workplace_{job}", village.0))
        .and_then(|entry| entry.choices.clone())
        .expect("the job's own row");
    let same = own
        .iter()
        .find(|(label, _)| label == "Same as the others")
        .expect("Same as the others")
        .1
        .clone();
    assert_eq!(
        same,
        Action::Priority {
            settlement: village,
            topic: "workplace",
            profession: Some(job.clone()),
            level: PriorityLevel::Normal,
        }
    );
    // "Your orders" names the level in force.
    assert!(
        rows.iter()
            .any(|entry| entry.text.starts_with("Workplaces, ") && entry.text.contains(": high.")),
        "{:?}",
        rows.iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
    );
    // "Same as the others" takes the job's own level away again.
    app.world_mut().resource_mut::<Ledger>().requests.push(same);
    process(app.world_mut());
    run_ticks(app.world_mut(), 1);
    assert!(
        !village::frontier::oversee::priorities(app.world(), village)
            .iter()
            .any(|row| row.profession.as_deref() == Some(job.as_str()))
    );
}

/// A ledger in the player session over the living world, its right column
/// empty, with the seasons `held` (the newest is the one running).
fn season_fixture(held: &[(u32, &str)]) -> App {
    let mut app = oversee_fixture();
    app.insert_resource(crate::PlayerExperience { enabled: true })
        .init_resource::<crate::island_shell::ShellState>();
    set_seasons(&mut app, held);
    close(app.world_mut());
    process(app.world_mut());
    app
}

fn set_seasons(app: &mut App, held: &[(u32, &str)]) {
    use island_content::village::ledger::{SeasonLedger, SeasonTally};
    let seasons = held
        .iter()
        .map(|(season, name)| SeasonTally {
            season: *season,
            name: (*name).to_owned(),
            ..Default::default()
        })
        .collect();
    app.world_mut().insert_resource(SeasonLedger { seasons });
}

#[test]
fn the_season_card_opens_only_into_an_empty_column() {
    let mut app = season_fixture(&[(0, "Autumn")]);
    assert_eq!(app.world().resource::<Ledger>().season_card, None);
    // Autumn ends with the column empty: its card opens there by itself.
    set_seasons(&mut app, &[(0, "Autumn"), (1, "Winter")]);
    process(app.world_mut());
    process(app.world_mut());
    let ledger = app.world().resource::<Ledger>();
    assert_eq!(ledger.season_card, Some(0));
    assert!(ledger.open);
    let rows = entries(app.world());
    assert_eq!(rows[0].text, "Autumn has ended");
    // Winter ends while a ledger page is read: a notice, not the card.
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::SeasonClose);
    process(app.world_mut());
    open_page(app.world_mut(), LedgerPage::Economy);
    set_seasons(&mut app, &[(0, "Autumn"), (1, "Winter"), (2, "Spring")]);
    process(app.world_mut());
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().season_card, None);
    assert_eq!(
        crate::island_shell::host_toast_text(app.world()),
        Some((
            "Winter has ended.".to_owned(),
            Some("See the season".to_owned())
        ))
    );
    // Its button opens the card.
    open_last_season(app.world_mut());
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().season_card, Some(1));
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::SeasonJournal);
    process(app.world_mut());
    // The Journal offers it again.
    app.world_mut().resource_mut::<Ledger>().tab = Tab::Journal;
    let link = entries(app.world())
        .into_iter()
        .find(|entry| entry.key == "journal_season")
        .expect("the Journal's note");
    assert_eq!(link.text, "Winter ended. See the season.");
    assert_eq!(link.action, Some(Action::SeasonOpen(1)));
}

#[test]
fn the_season_card_never_pauses_time() {
    let mut app = season_fixture(&[(0, "Autumn")]);
    let before = *app.world().resource::<SimControl>();
    set_seasons(&mut app, &[(0, "Autumn"), (1, "Winter")]);
    process(app.world_mut());
    process(app.world_mut());
    assert_eq!(app.world().resource::<Ledger>().season_card, Some(0));
    assert_eq!(*app.world().resource::<SimControl>(), before);
}

#[test]
fn no_sentence_mentions_time_while_time_runs() {
    let mut app = oversee_fixture();
    assert!(!app.world().resource::<SimControl>().is_stopped());
    // The ledger's foot after an order.
    let village = open_village(&mut app, economy::EconomyPage::Priorities);
    app.world_mut()
        .resource_mut::<Ledger>()
        .requests
        .push(Action::Priority {
            settlement: village,
            topic: "housing",
            profession: None,
            level: village::frontier::oversee::PriorityLevel::High,
        });
    process(app.world_mut());
    let foot = app.world().resource::<Ledger>().message.clone();
    assert_eq!(foot, "Order given.");
    // "Your orders" while it waits for the next tick.
    let rows = entries(app.world());
    for entry in &rows {
        assert!(!entry.text.contains("time runs"), "{}", entry.text);
    }
    // The build tray and Shape say nothing about time either.
    assert!(!crate::game_ui::words::order_given(false).contains("time"));
    // Paused, the same order says it waits.
    assert_eq!(
        crate::game_ui::words::order_given(true),
        "Order queued. It starts when time runs (Space)."
    );
}

#[test]
fn the_foot_line_clears_after_six_seconds() {
    let mut app = oversee_fixture();
    app.init_resource::<Time>();
    app.world_mut().resource_mut::<Ledger>().message = "Order given.".into();
    process(app.world_mut());
    let advance = |app: &mut App, secs: f32| {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(secs));
        process(app.world_mut());
    };
    advance(&mut app, 5.0);
    assert_eq!(app.world().resource::<Ledger>().message, "Order given.");
    advance(&mut app, 1.5);
    assert_eq!(app.world().resource::<Ledger>().message, "");
}

#[test]
fn the_price_change_stands_apart_from_the_price() {
    assert_eq!(widgets::split_change("4 +1"), ("4", Some("+1")));
    assert_eq!(widgets::split_change("3 -2"), ("3", Some("-2")));
    assert_eq!(widgets::split_change("4"), ("4", None));
    assert_eq!(widgets::split_change("\u{2013}"), ("\u{2013}", None));
    assert_eq!(widgets::split_change("up 3"), ("up 3", None));
}

#[test]
fn job_chips_at_one_level_wrap_as_buttons_not_a_selector() {
    use village::frontier::oversee::PriorityLevel;
    let chip = |job: &str, level| {
        (
            job.to_owned(),
            Action::Priority {
                settlement: SimId(1),
                topic: "jobs",
                profession: Some(job.to_owned()),
                level,
            },
        )
    };
    let chips = [
        chip("miner", PriorityLevel::High),
        chip("smith", PriorityLevel::High),
        chip("baker", PriorityLevel::High),
    ];
    assert!(widgets::job_chips(&chips));
    // One topic's levels are one segmented control.
    let levels = [
        (
            "Low".to_owned(),
            Action::Priority {
                settlement: SimId(1),
                topic: "housing",
                profession: None,
                level: PriorityLevel::Low,
            },
        ),
        (
            "High".to_owned(),
            Action::Priority {
                settlement: SimId(1),
                topic: "housing",
                profession: None,
                level: PriorityLevel::High,
            },
        ),
    ];
    assert!(!widgets::job_chips(&levels));
}

/// A folded alert's "Show them" opened People under "3 residents in Harbour
/// are starving: 3 residents" (live capture, state-alerts): the count is
/// said once.
#[test]
fn a_people_filter_heading_counts_its_residents_once() {
    assert_eq!(
        model::filter_heading("3 residents in Harbour are starving", 3),
        "3 residents in Harbour are starving."
    );
    assert_eq!(
        model::filter_heading("Going without bread", 5),
        "Going without bread: 5 residents"
    );
    assert_eq!(
        model::filter_heading("Going without bread", 1),
        "Going without bread: 1 resident"
    );
    // A heading whose count no longer matches the list keeps the list's.
    assert_eq!(
        model::filter_heading("3 residents in Harbour are starving", 2),
        "3 residents in Harbour are starving: 2 residents"
    );
}

#[test]
fn an_open_card_is_scrolled_into_view_whole_when_it_fits() {
    // The visible ledger runs from 150 to 630.
    let area = (150.0, 630.0);
    // Its head showed at 500 and its "Nobody makes" line at 760: the card
    // moves up until its last part is in view, its head still in sight.
    assert_eq!(model::card_scroll(area, (500.0, 760.0)), 130.0);
    // Already in view: no scroll.
    assert_eq!(model::card_scroll(area, (200.0, 600.0)), 0.0);
    // Taller than the area: its head at the top.
    assert_eq!(model::card_scroll(area, (500.0, 1200.0)), 350.0);
    // Above the area (scrolled past): back down to its head.
    assert_eq!(model::card_scroll(area, (100.0, 300.0)), -50.0);
}
