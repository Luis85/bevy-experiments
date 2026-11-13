use super::*;
use island_content::village::Role;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{record_effect, set_journal};
use npc_framework::prelude::run_ticks;
use serde_json::json;

fn fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    set_journal(app.world_mut(), true, false);
    app.init_resource::<Alerts>()
        .init_resource::<IslandMode>()
        .init_resource::<crate::island_shell::ShellState>()
        .insert_resource(crate::PlayerExperience { enabled: true });
    app
}

fn resident(app: &App, role: Role) -> (SimId, Entity) {
    app.world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| {
            app.world()
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role == role)
        })
        .map(|(id, entity)| (*id, *entity))
        .expect("resident")
}

fn kinds(app: &App) -> Vec<AlertKind> {
    app.world()
        .resource::<Alerts>()
        .feed
        .iter()
        .map(|alert| alert.kind)
        .collect()
}

#[test]
fn the_feed_starts_at_the_present_and_reads_new_journal_effects_only() {
    let mut app = fixture();
    let (farmer, _) = resident(&app, Role::Farmer);
    // History recorded before the session looked at the journal.
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageExhausted",
        &json!({"recovery": "rest"}),
    );
    run_ticks(app.world_mut(), 3);
    track(app.world_mut());
    assert!(kinds(&app).is_empty(), "a loaded journal is not replayed");

    let before = state_hash(app.world_mut());
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageDeath",
        &json!({"permanent": true, "pos": [4000, -2500], "coins_dropped": 3}),
    );
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageGathered",
        &json!({"kind": "village_grain"}),
    );
    track(app.world_mut());
    track(app.world_mut());
    assert_eq!(
        kinds(&app),
        [AlertKind::Death],
        "read once; everyday work is not an alert"
    );
    let alert = app
        .world()
        .resource::<Alerts>()
        .feed
        .iter()
        .next()
        .unwrap()
        .clone();
    assert_eq!(alert.subject, Some(farmer.0));
    assert_eq!(
        alert.focus,
        Some((4000, -2500)),
        "the recorded place of death"
    );
    assert!(alert.text.contains("Ada"), "{}", alert.text);
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "harvesting alerts never changes simulation state"
    );
}

#[test]
fn repeated_exposure_folds_and_selecting_an_alert_focuses_the_resident() {
    let mut app = fixture();
    let (smith, _) = resident(&app, Role::Smith);
    track(app.world_mut());
    for _ in 0..3 {
        record_effect(
            app.world_mut(),
            Some(smith),
            "VillageColdExposure",
            &json!({"warmth": 100, "threshold": 20000, "health_per_tick": -3}),
        );
        run_ticks(app.world_mut(), 1);
        track(app.world_mut());
    }
    let (serial, count) = {
        let feed = &app.world().resource::<Alerts>().feed;
        assert_eq!(feed.len(), 1, "one folded row, not three");
        let alert = feed.iter().next().unwrap();
        (alert.serial, alert.count)
    };
    assert_eq!(count, 3);

    app.world_mut()
        .resource_mut::<Alerts>()
        .requests
        .push(Request::Row(RowKey::Alert(serial)));
    process(app.world_mut());
    assert_eq!(
        app.world().resource::<Selected>().0,
        Some(smith),
        "the resident is selected for the ledger and framed by the camera"
    );
    // A stale row id is ignored.
    app.world_mut()
        .resource_mut::<Alerts>()
        .requests
        .push(Request::Row(RowKey::Alert(9999)));
    process(app.world_mut());
}

#[test]
fn a_new_world_epoch_clears_the_feed() {
    let mut app = fixture();
    let (farmer, _) = resident(&app, Role::Farmer);
    track(app.world_mut());
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageCottageBuilt",
        &json!({"plot": 1}),
    );
    track(app.world_mut());
    assert_eq!(kinds(&app), [AlertKind::Built]);
    reset(app.world_mut(), 99);
    assert!(kinds(&app).is_empty());
    track(app.world_mut());
    assert!(
        kinds(&app).is_empty(),
        "the old world's journal is not read again after the epoch reset"
    );
}

#[test]
fn developer_launches_do_not_track_alerts() {
    let mut app = fixture();
    app.insert_resource(crate::PlayerExperience { enabled: false });
    let (farmer, _) = resident(&app, Role::Farmer);
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageDeath",
        &json!({"permanent": true}),
    );
    track(app.world_mut());
    assert!(kinds(&app).is_empty());
    assert!(app.world().resource::<Alerts>().epoch.is_none());
}

/// The Journal page's rows, as the ledger shows them.
fn shown(app: &App) -> Vec<String> {
    journal(app.world())
        .2
        .into_iter()
        .map(|line| line.text)
        .collect()
}

#[test]
fn job_changes_fold_into_one_line_that_a_click_unfolds() {
    let mut app = fixture();
    track(app.world_mut());
    let residents: Vec<SimId> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| app.world().get::<Villager>(**entity).is_some())
        .map(|(id, _)| *id)
        .take(4)
        .collect();
    assert_eq!(residents.len(), 4);
    for resident in &residents {
        record_effect(
            app.world_mut(),
            Some(*resident),
            "RegionalOccupationChanged",
            &json!({"from": "hunter", "to": "tailor"}),
        );
    }
    record_effect(
        app.world_mut(),
        Some(residents[0]),
        "VillageDeath",
        &json!({"permanent": true}),
    );
    // Three trades by three buyers are one note, not three rows.
    for buyer in &residents[1..] {
        record_effect(
            app.world_mut(),
            Some(*buyer),
            "VillageTrade",
            &json!({"kind": "village_bread", "quantity": 1, "coins": 3}),
        );
    }
    track(app.world_mut());
    let feed = &app.world().resource::<Alerts>().feed;
    assert_eq!(feed.len(), 4 + 1 + 1);
    let trade = feed
        .iter()
        .find(|alert| alert.kind == AlertKind::Trade)
        .unwrap();
    assert_eq!((trade.subject, trade.count), (None, 3));

    // Default view: the death, and one line for the day's seven notes.
    let folded = shown(&app);
    assert_eq!(folded.len(), 2, "{folded:?}");
    assert!(
        folded[0].ends_with("7 notes · show\n4 job changes, 3 trades"),
        "{}",
        folded[0]
    );
    assert!(
        journal(app.world()).2[0].fold,
        "the day is the row's heading"
    );
    assert!(
        folded[1].ends_with("has died\nCritical"),
        "severity is a word as well as a shape: {}",
        folded[1]
    );
    // The Oversee stack shows what needs attention; the notes are one click away.
    let stack = stack(app.world());
    assert_eq!((stack.critical, stack.warning, stack.more), (1, 0, 0));
    assert_eq!(stack.rows.len(), 1);
    assert!(stack.rows[0].2.ends_with("has died"));
    assert!(stack.rows[0].3.starts_with("Critical \u{b7} day "));
    assert!(
        stack.journal.ends_with("7 notes in the journal"),
        "{}",
        stack.journal
    );

    // Activating the fold line unfolds it in place; again folds it back.
    let day = day_of(app.world(), 0);
    let fold = Request::Row(RowKey::Fold(day));
    app.world_mut().resource_mut::<Alerts>().requests.push(fold);
    process(app.world_mut());
    let open = shown(&app);
    assert_eq!(open.len(), 2 + 5, "{open:?}");
    assert!(open[0].contains("· hide\n"), "{}", open[0]);
    assert!(
        open[1].contains("bought 1 bread for 3 coins\nNote \u{b7} \u{d7}3"),
        "{}",
        open[1]
    );
    assert!(
        open[2..6]
            .iter()
            .all(|line| line.contains("changed from hunter to tailor"))
    );
    assert!(
        app.world().get_resource::<Selected>().is_none(),
        "a fold line selects nobody and moves no camera"
    );
    app.world_mut().resource_mut::<Alerts>().requests.push(fold);
    process(app.world_mut());
    assert_eq!(shown(&app).len(), 2);

    // The filter control: emergencies only, or every row.
    for (filter, rows) in [
        (Filter::Critical, 1),
        (Filter::All, 6),
        (Filter::Attention, 2),
    ] {
        app.world_mut()
            .resource_mut::<Alerts>()
            .requests
            .push(Request::Filter(filter));
        process(app.world_mut());
        assert_eq!(shown(&app).len(), rows, "{filter:?}");
    }
    assert!(
        empty_text(Filter::Critical, 6).starts_with("No emergencies"),
        "an empty filter says why"
    );
    assert!(empty_text(Filter::Attention, 0).starts_with("Nothing needs"));
    assert_eq!(empty_text(Filter::All, 6), "");
}

#[test]
fn collapse_and_filter_are_view_choices_that_outlive_a_new_world() {
    let mut app = fixture();
    track(app.world_mut());
    let before = state_hash(app.world_mut());
    let revision = app.world().resource::<Alerts>().view_revision;
    app.world_mut()
        .resource_mut::<Alerts>()
        .requests
        .extend([Request::Collapse, Request::Filter(Filter::All)]);
    process(app.world_mut());
    {
        let state = app.world().resource::<Alerts>();
        assert!(state.collapsed);
        assert_eq!(state.filter, Filter::All);
        assert_ne!(state.view_revision, revision, "the widget redraws");
    }
    reset(app.world_mut(), 99);
    let state = app.world().resource::<Alerts>();
    assert!(state.collapsed && state.filter == Filter::All);
    assert!(state.open_days.is_empty());
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn homelessness_and_shortages_are_warnings_reported_once_per_onset() {
    let mut app = fixture();
    scan(app.world_mut());
    assert!(
        kinds(&app).is_empty(),
        "the authored village is housed and fed: {:?}",
        kinds(&app)
    );
    // Fixture shortcut: empty one home, as an eviction or a death would.
    let (home, evicted) = {
        let world = app.world_mut();
        let mut homes = world.query::<(Entity, &colony::Household)>();
        homes
            .iter(world)
            .find(|(_, home)| !home.residents.is_empty())
            .map(|(entity, home)| (entity, home.residents.clone()))
            .expect("an occupied home")
    };
    app.world_mut()
        .get_mut::<colony::Household>(home)
        .unwrap()
        .residents
        .clear();
    scan(app.world_mut());
    scan(app.world_mut());
    let feed = &app.world().resource::<Alerts>().feed;
    let homeless: Vec<_> = feed
        .iter()
        .filter(|alert| alert.kind == AlertKind::Homeless)
        .collect();
    assert_eq!(
        homeless.len(),
        evicted.len(),
        "once each, not once per scan"
    );
    assert!(homeless.iter().all(|alert| alert.count == 1
        && alert.text.ends_with("has no home")
        && alert.kind.severity() == Severity::Warning));
    assert!(
        evicted
            .iter()
            .all(|id| homeless.iter().any(|alert| alert.subject == Some(id.0)))
    );

    // Shortage: below one unit a head, per good; an empty village has none.
    assert!(shortage_lines(42, 42, 42).is_empty());
    assert!(shortage_lines(0, 0, 0).is_empty());
    assert_eq!(
        shortage_lines(42, 10, 80),
        [(
            "food",
            "The village is short of food: 10 portions for 42 residents".to_owned()
        )]
    );
    assert_eq!(
        shortage_lines(42, 0, 0)
            .iter()
            .map(|(good, _)| *good)
            .collect::<Vec<_>>(),
        ["food", "clean water"]
    );
    // The live summary feeds the same rule; this village is supplied.
    let summary = colony::summary(app.world());
    assert!(summary.residents > 0);
    assert_eq!(
        shortages(app.world()),
        shortage_lines(summary.residents, summary.food_units, summary.water_units)
    );
    let reported = feed
        .iter()
        .filter(|alert| alert.kind == AlertKind::Shortage)
        .count();
    assert_eq!(reported, shortages(app.world()).len());
}

// The regional world journals the onset of starvation and thirst and the
// emptying of a patch; there the effects are the source and the scan is not.

fn regional(journal: bool) -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    set_journal(app.world_mut(), journal, false);
    app.init_resource::<Alerts>()
        .init_resource::<IslandMode>()
        .init_resource::<crate::island_shell::ShellState>()
        .insert_resource(crate::PlayerExperience { enabled: true });
    app
}

fn stocked_patch(app: &App) -> (SimId, Entity) {
    app.world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| {
            app.world()
                .get::<ResourcePatch>(**entity)
                .is_some_and(|patch| patch.remaining > 0)
        })
        .map(|(id, entity)| (*id, *entity))
        .expect("a stocked resource patch")
}

fn about(app: &App, kind: AlertKind) -> Vec<model::Alert> {
    app.world()
        .resource::<Alerts>()
        .feed
        .iter()
        .filter(|alert| alert.kind == kind)
        .cloned()
        .collect()
}

#[test]
fn journaled_onsets_are_the_only_source_of_hunger_thirst_and_empty_patches() {
    let mut app = regional(true);
    assert!(onsets_journaled(app.world()));
    let (farmer, farmer_entity) = resident(&app, Role::Farmer);
    let (patch, patch_entity) = stocked_patch(&app);
    track(app.world_mut());
    assert!(
        kinds(&app).is_empty(),
        "the authored world begins fed, housed and stocked: {:?}",
        kinds(&app)
    );

    // Fixture shortcut: the state alone, as if no effect had been journaled.
    // Where onsets are journaled the scan must not turn it into alerts.
    {
        let mut vital = app.world_mut().get_mut::<Vitality>(farmer_entity).unwrap();
        vital.starved_ticks = 5 * CRITICAL_TICKS;
        vital.dehydrated_ticks = 5 * CRITICAL_TICKS;
    }
    app.world_mut()
        .get_mut::<ResourcePatch>(patch_entity)
        .unwrap()
        .remaining = 0;
    scan(app.world_mut());
    scan(app.world_mut());
    assert!(
        kinds(&app).is_empty(),
        "no scan of needs or patches here: {:?}",
        kinds(&app)
    );

    // The simulation's own effects, as `vitals` and gathering record them.
    let before = state_hash(app.world_mut());
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageStarvationOnset",
        &json!({"hunger": 0, "health_loss_after_ticks": 600}),
    );
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageThirstOnset",
        &json!({"thirst": 0, "health_loss_after_ticks": 600}),
    );
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillagePatchDepleted",
        &json!({"kind": "log", "patch": patch}),
    );
    for _ in 0..3 {
        track(app.world_mut());
        scan(app.world_mut());
    }
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "reading is not writing"
    );
    let mut seen = kinds(&app);
    seen.sort();
    assert_eq!(
        seen,
        [
            AlertKind::Starving,
            AlertKind::Dehydrated,
            AlertKind::Depleted
        ],
        "each condition once, however often the feed is refreshed"
    );
    let name = app
        .world()
        .get::<Villager>(farmer_entity)
        .unwrap()
        .name
        .clone();
    let starving = &about(&app, AlertKind::Starving)[0];
    assert_eq!((starving.subject, starving.count), (Some(farmer.0), 1));
    assert_eq!(starving.text, format!("{name} is starving"));
    let thirsty = &about(&app, AlertKind::Dehydrated)[0];
    assert_eq!((thirsty.subject, thirsty.count), (Some(farmer.0), 1));
    assert_eq!(thirsty.text, format!("{name} is dangerously thirsty"));
    // The patch, not the gatherer, is the subject and the place to look.
    let depleted = &about(&app, AlertKind::Depleted)[0];
    assert_eq!((depleted.subject, depleted.count), (Some(patch.0), 1));
    let site = *app.world().get::<SimPos>(patch_entity).unwrap();
    assert_eq!(depleted.focus, Some((site.x, site.z)));
    assert!(
        depleted.text.starts_with("The ") && depleted.text.ends_with("is depleted"),
        "{}",
        depleted.text
    );
    assert!(!depleted.text.contains(&name), "{}", depleted.text);

    // A patch that regrows and empties again is a new event on the same row.
    run_ticks(app.world_mut(), 1);
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillagePatchDepleted",
        &json!({"kind": "log", "patch": patch}),
    );
    track(app.world_mut());
    let depleted = about(&app, AlertKind::Depleted);
    assert_eq!(depleted.len(), 1);
    assert_eq!(depleted[0].count, 2);
}

#[test]
fn a_need_that_began_before_the_session_is_read_once_and_its_effect_not_repeated() {
    let mut app = regional(true);
    let (farmer, farmer_entity) = resident(&app, Role::Farmer);
    // Before the feed first looks: the onset is history in the journal and
    // the resident is still at the critical level.
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageStarvationOnset",
        &json!({"hunger": 0, "health_loss_after_ticks": 600}),
    );
    app.world_mut()
        .get_mut::<Vitality>(farmer_entity)
        .unwrap()
        .starved_ticks = 40;
    for _ in 0..3 {
        track(app.world_mut());
        scan(app.world_mut());
    }
    let starving = about(&app, AlertKind::Starving);
    assert_eq!(starving.len(), 1, "read from the world once");
    assert_eq!(
        (starving[0].subject, starving[0].count),
        (Some(farmer.0), 1)
    );
    assert!(about(&app, AlertKind::Dehydrated).is_empty());

    // The same onset reaching the journal within the tick that was read is
    // the same event, not a second alert.
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageStarvationOnset",
        &json!({"hunger": 0, "health_loss_after_ticks": 600}),
    );
    track(app.world_mut());
    assert_eq!(about(&app, AlertKind::Starving)[0].count, 1);

    // A later onset is a new event; it folds into the resident's row.
    run_ticks(app.world_mut(), 2);
    record_effect(
        app.world_mut(),
        Some(farmer),
        "VillageStarvationOnset",
        &json!({"hunger": 0, "health_loss_after_ticks": 600}),
    );
    track(app.world_mut());
    track(app.world_mut());
    let starving: Vec<_> = about(&app, AlertKind::Starving)
        .into_iter()
        .filter(|alert| alert.subject == Some(farmer.0))
        .collect();
    assert_eq!(starving.len(), 1);
    assert_eq!(starving[0].count, 2);
}

fn living_fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    set_journal(app.world_mut(), true, false);
    app.init_resource::<Alerts>()
        .init_resource::<crate::island_build::BuildTool>()
        .init_resource::<crate::island_build::CivicLog>()
        .init_resource::<crate::island_shell::ShellState>()
        .insert_resource(IslandMode::Inspector)
        .insert_resource(crate::PlayerExperience { enabled: true });
    app
}

/// Place `building` near a settlement through the recorded command.
fn place(app: &mut App, settlement: &str, building: &str) -> village::civic::SiteView {
    use npc_framework::kernel::commands::{SimCommand, submit};
    let (x, z, rotation) = {
        let world = app.world();
        let settlement = world
            .resource::<island_content::TestbedIds>()
            .id(settlement);
        let def = village::civic::definition(world, building).expect("definition");
        village::civic::find_site(world, def, settlement, 36).expect("a buildable spot")
    };
    let command = village::civic::PlaceBuilding {
        building: building.into(),
        x,
        z,
        rotation,
    };
    submit(app.world_mut(), None, SimCommand::content(&command)).expect("placement recorded");
    run_ticks(app.world_mut(), 2);
    village::civic::sites(app.world())
        .into_iter()
        .next_back()
        .expect("an open site")
}

fn of_kind(app: &App, kind: AlertKind) -> Vec<model::Alert> {
    app.world()
        .resource::<Alerts>()
        .feed
        .iter()
        .filter(|alert| alert.kind == kind)
        .cloned()
        .collect()
}

#[test]
fn a_project_is_reported_when_it_starts_and_when_it_is_cancelled() {
    use npc_framework::kernel::commands::{SimCommand, submit};
    let mut app = living_fixture();
    track(app.world_mut());
    assert!(of_kind(&app, AlertKind::ProjectStarted).is_empty());
    let site = place(&mut app, "settlement_harbour", "stockpile");
    let before = state_hash(app.world_mut());
    track(app.world_mut());
    let started = of_kind(&app, AlertKind::ProjectStarted);
    assert_eq!(started.len(), 1);
    assert_eq!(
        started[0].subject,
        Some(site.id.0),
        "the row is about the site"
    );
    assert_eq!(started[0].focus, Some((site.door.x, site.door.z)));
    assert_eq!(
        started[0].text,
        format!(
            "You placed {}; residents will supply and build it",
            site.label
        )
    );
    assert_eq!(before, state_hash(app.world_mut()), "alerts only read");

    // Selecting the row frames the site and opens its card.
    let serial = started[0].serial;
    app.world_mut()
        .resource_mut::<Alerts>()
        .requests
        .push(Request::Row(RowKey::Alert(serial)));
    process(app.world_mut());
    assert_eq!(
        crate::island_build::card_subject(app.world()),
        Some(site.id)
    );

    // The cancelled site is gone from the world; the feed still names it.
    let cancel = village::civic::CancelBuilding { site: site.id };
    submit(app.world_mut(), None, SimCommand::content(&cancel)).expect("cancellation recorded");
    run_ticks(app.world_mut(), 2);
    track(app.world_mut());
    let cancelled = of_kind(&app, AlertKind::ProjectCancelled);
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        cancelled[0].text,
        format!("{} was cancelled: cancelled by the overseer", site.label)
    );
    assert_eq!(cancelled[0].focus, Some((site.door.x, site.door.z)));
    assert_eq!(cancelled[0].kind.severity(), Severity::Info);
}

#[test]
fn a_site_is_not_reported_as_waiting_before_a_whole_day_has_passed() {
    let mut app = living_fixture();
    track(app.world_mut());
    let site = place(&mut app, "settlement_stone_ridge", "mill");
    app.world_mut().resource_mut::<Alerts>().scan_elapsed = SCAN_SECS;
    track(app.world_mut());
    assert!(of_kind(&app, AlertKind::SiteWaiting).is_empty());
    let rows = crate::island_build::site_rows(app.world());
    assert_eq!(rows[0].0.waiting_days, 0);
    // The feed remembers the site, so a later cancellation can name it.
    assert_eq!(
        app.world().resource::<Alerts>().sites.get(&site.id.0),
        Some(&(site.label.clone(), (site.door.x, site.door.z)))
    );
    // The wording and the once-per-day rule are the model's.
    let missing = rows[0].0.materials_line();
    assert!(model::waiting_text(&site.label, 2, &missing).contains("waited 2 days"));
    assert!(model::waiting_due(1, None) && !model::waiting_due(1, Some(1)));
}

#[test]
fn the_stack_keeps_three_rows_and_counts_the_rest() {
    let mut app = fixture();
    track(app.world_mut());
    let residents: Vec<SimId> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| app.world().get::<Villager>(**entity).is_some())
        .map(|(id, _)| *id)
        .take(5)
        .collect();
    for resident in &residents {
        record_effect(
            app.world_mut(),
            Some(*resident),
            "VillageDeath",
            &json!({"permanent": true}),
        );
    }
    track(app.world_mut());
    let stack = stack(app.world());
    assert_eq!(stack.rows.len(), STACK_ROWS);
    assert_eq!((stack.critical, stack.more), (5, 2));
    // "Day N · the journal" when the day has no routine notes yet.
    assert!(stack.journal.ends_with("the journal"), "{}", stack.journal);
    // A sentence with a parenthesis splits into a title and one line of detail.
    assert_eq!(
        title_and_detail(&model::waiting_text(
            "Harbour cottage 3",
            2,
            "planks 3/8 \u{b7} logs 4/4"
        )),
        (
            "Harbour cottage 3 has waited 2 days for materials",
            Some("planks 3/8 \u{b7} logs 4/4")
        )
    );
    assert_eq!(
        title_and_detail("Bram is starving"),
        ("Bram is starving", None)
    );
    // The stack shortens a waiting site to its name, the wait and what is missing.
    assert_eq!(
        model::waiting_parts("Harbour cottage 3 has waited 2 days for materials"),
        Some(("Harbour cottage 3", "2 days"))
    );
    assert_eq!(model::waiting_parts("Bram is starving"), None);
    assert_eq!(title_and_detail("()"), ("()", None));
}

#[test]
fn what_the_first_scan_after_a_load_finds_makes_no_alert_sound() {
    use crate::game_ui::sound::{PlayUiSound, UiSound};
    let mut app = regional(true);
    if !app.world().contains_resource::<Messages<PlayUiSound>>() {
        app.add_message::<PlayUiSound>();
    }
    let (_, farmer_entity) = resident(&app, Role::Farmer);
    // Already starving when the world is opened.
    app.world_mut()
        .get_mut::<Vitality>(farmer_entity)
        .unwrap()
        .starved_ticks = 40;
    for _ in 0..3 {
        track(app.world_mut());
        scan(app.world_mut());
        super::alert_sound(app.world_mut());
    }
    assert!(
        !about(&app, AlertKind::Starving).is_empty(),
        "the alert is listed"
    );
    let heard: Vec<UiSound> = app
        .world_mut()
        .resource_mut::<Messages<PlayUiSound>>()
        .drain()
        .map(|message| message.0)
        .collect();
    assert!(
        !heard.contains(&UiSound::Alert),
        "the baseline is silent: {heard:?}"
    );
    assert!(
        super::alert_due(Some(2), 3) && !super::alert_due(Some(3), 3) && !super::alert_due(None, 3)
    );
}
