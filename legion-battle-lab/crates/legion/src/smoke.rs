//! Opt-in native integration checks using real UI observers and GPU screenshots.
//! Worker completion, render preparation and screenshot writes are asynchronous;
//! a frame counter alone is never treated as proof that those jobs completed.

use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use battle_core::{MapKind, ScenarioConfig, Team, UnitCategory, UnitKind};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    ui::InteractionDisabled,
    ui_widgets::Activate,
};

use crate::{
    app::{AppAction, AppSet, LaunchOptions, ScenarioDraft, Screen, Session},
    crowd::{CrowdInstances, CrowdRenderHealth, InstanceData},
    runtime::BattleSnapshot,
    ui::{ActionButton, CountStepButton},
    world::RenderCounts,
};

const CHECK_TIMEOUT: Duration = Duration::from_secs(240);

pub struct SmokeTestPlugin;

#[derive(Resource, Default)]
struct SmokeState {
    frame: u32,
    updates: u64,
    started_at: Option<Instant>,
    captures: usize,
    captures_requested: usize,
    captures_pending: usize,
    capture_files: Vec<String>,
    checks: Vec<String>,
    errors: Vec<String>,
    finished: bool,
    paused_tick: u64,
    paused_since: Option<Instant>,
    saved: Option<ScenarioConfig>,
    category_cursor: usize,
    category_pending: bool,
    count_baseline: [u32; 2],
    fast_forward_target: u64,
    restart_generation: u64,
    menu_generation: u64,
    render_stable_frames: u32,
    render_paused: bool,
    render_snapshot: Option<serde_json::Value>,
    render_details: Option<serde_json::Value>,
}

impl Plugin for SmokeTestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SmokeState>()
            .add_systems(Update, exercise_flow.in_set(AppSet::Input));
    }
}

fn check(state: &mut SmokeState, condition: bool, label: &str) {
    if condition {
        state.checks.push(label.into());
        info!("SMOKE OK: {label}");
    } else {
        state.errors.push(label.into());
        error!("SMOKE FAILED: {label}");
    }
}

fn activate(world: &mut World, state: &mut SmokeState, action: AppAction) {
    let target = world
        .query::<(Entity, &ActionButton, Has<InteractionDisabled>)>()
        .iter(world)
        .find(|(_, button, disabled)| button.0 == action && !*disabled)
        .map(|(entity, _, _)| entity);
    if let Some(entity) = target {
        world.trigger(Activate { entity });
        state.checks.push(format!("UI activation: {action:?}"));
    } else {
        state
            .errors
            .push(format!("Missing enabled UI control: {action:?}"));
    }
}

fn activate_count_step(world: &mut World, state: &mut SmokeState, team: Team, step: i32) {
    let target = world
        .query::<(Entity, &CountStepButton, Has<InteractionDisabled>)>()
        .iter(world)
        .find(|(_, button, disabled)| button.team == team && button.step == step && !*disabled)
        .map(|(entity, _, _)| entity);
    if let Some(entity) = target {
        world.trigger(Activate { entity });
        state
            .checks
            .push(format!("UI count-step activation: {team:?}, {step}"));
    } else {
        state.errors.push(format!(
            "Missing enabled count-step control: {team:?}, {step}"
        ));
    }
}

fn capture(world: &mut World, smoke: &mut SmokeState, folder: &std::path::Path, name: &str) {
    let path = folder.join(name);
    let name = name.to_owned();
    smoke.captures_requested += 1;
    smoke.captures_pending += 1;
    world.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>, mut smoke: ResMut<SmokeState>| {
            let result = event
                .image
                .clone()
                .try_into_dynamic()
                .map_err(|error| error.to_string())
                .and_then(|image| {
                    image
                        .to_rgb8()
                        .save(&path)
                        .map_err(|error| error.to_string())
                });
            smoke.captures_pending = smoke.captures_pending.saturating_sub(1);
            match result {
                Ok(()) => {
                    // A requested screenshot is not a successful screenshot.
                    smoke.captures += 1;
                    smoke.capture_files.push(name.clone());
                    info!("SMOKE screenshot saved: {}", path.display());
                }
                Err(error) => smoke
                    .errors
                    .push(format!("Screenshot {}: {error}", path.display())),
            }
        },
    );
}

fn exercise_flow(world: &mut World) {
    let options = world.resource::<LaunchOptions>().clone();
    if !options.smoke_test && !options.render_check {
        return;
    }
    world.resource_scope(|world, mut smoke: Mut<SmokeState>| {
        if smoke.finished {
            return;
        }
        smoke.updates += 1;
        let began = *smoke.started_at.get_or_insert_with(Instant::now);
        if began.elapsed() >= CHECK_TIMEOUT {
            let message = format!(
                "Timed out after {} seconds at flow frame {}: {}",
                CHECK_TIMEOUT.as_secs(),
                smoke.frame,
                current_status(world),
            );
            smoke.errors.push(message);
            finish(world, &options, &mut smoke);
            return;
        }
        let session = world.resource::<Session>();
        if session.faulted {
            smoke
                .errors
                .push(format!("Simulation worker failed: {}", session.status));
            finish(world, &options, &mut smoke);
            return;
        }
        if let Some(message) = shader_error(world) {
            smoke
                .errors
                .push(format!("Scene shader/pipeline failed: {message}"));
            finish(world, &options, &mut smoke);
            return;
        }
        if options.render_check {
            exercise_render_check(world, &options, &mut smoke);
        } else {
            exercise_ui_flow(world, &options, &mut smoke);
        }
    });
}

/// The original five-screen flow retains frame spacing for UI transitions;
/// these gates hold that flow until the actual asynchronous operation finishes.
fn exercise_ui_flow(world: &mut World, options: &LaunchOptions, smoke: &mut SmokeState) {
    let session = world.resource::<Session>();
    let ready = session.is_ready();
    let ready_idle = ready && !session.busy;
    let paused = session.paused;
    let single_step = session.single_step;
    let generation = session.generation;
    let tick = session
        .battle
        .as_ref()
        .map_or(0, |battle| battle.stats.tick);
    let terminal = session
        .battle
        .as_ref()
        .is_some_and(|battle| battle.outcome.is_some());
    let frame = smoke.frame;
    match frame {
        29 if !ready_idle => return,
        29 if !render_settled(world, smoke) => return,
        51 if !exercise_configuration(world, smoke) => return,
        89 if !ready || tick == 0 => return,
        93 if !ready_idle || !paused => return,
        99 if !ready_idle
            || smoke
                .paused_since
                .is_none_or(|began| began.elapsed() < Duration::from_millis(100)) =>
        {
            return;
        }
        103 if !ready_idle || single_step || tick <= smoke.paused_tick => return,
        111 | 139 if !ready_idle || (!terminal && tick < smoke.fast_forward_target) => return,
        111 | 139 if !render_settled(world, smoke) => return,
        147 if !ready || generation == smoke.restart_generation => return,
        157 if !ready_idle
            || generation == smoke.menu_generation
            || *world.resource::<State<Screen>>().get() != Screen::Menu =>
        {
            return;
        }
        _ => {}
    }
    smoke.frame += 1;
    match smoke.frame {
        30 => {
            check(
                smoke,
                *world.resource::<State<Screen>>().get() == Screen::Menu,
                "main menu entered",
            );
            capture(world, smoke, &options.capture_dir, "01-menu.png");
        }
        36 => activate(world, smoke, AppAction::Configure),
        50 => {
            check(
                smoke,
                *world.resource::<State<Screen>>().get() == Screen::Configure,
                "configuration entered",
            );
            capture(world, smoke, &options.capture_dir, "02-configuration.png");
        }
        54 => activate(world, smoke, AppAction::SetMap(MapKind::Crossing)),
        58 => activate(
            world,
            smoke,
            AppAction::SetKind(Team::Blue, UnitKind::Heavy),
        ),
        62 => activate(world, smoke, AppAction::ChangeSeed(1)),
        66 => activate(world, smoke, AppAction::SaveScenario),
        70 => {
            smoke.saved = Some(world.resource::<ScenarioDraft>().0.clone());
            activate(world, smoke, AppAction::ChangeSeed(1));
        }
        74 => activate(world, smoke, AppAction::LoadScenario),
        78 => {
            let restored = smoke.saved.as_ref() == Some(&world.resource::<ScenarioDraft>().0);
            check(smoke, restored, "scenario save/load roundtrip");
            activate(world, smoke, AppAction::StartBattle);
        }
        90 => {
            check(
                smoke,
                *world.resource::<State<Screen>>().get() == Screen::Battle,
                "battle entered",
            );
            check(smoke, tick > 0, "fixed-tick simulation advances");
            activate(world, smoke, AppAction::TogglePause);
        }
        94 => {
            // Pause may let the current fixed tick finish. Record the baseline
            // only after its final publication has been received.
            smoke.paused_tick = tick;
            smoke.paused_since = Some(Instant::now());
            check(
                smoke,
                ready_idle && world.resource::<Session>().paused,
                "pause waits for the worker to become idle",
            );
        }
        100 => {
            let stayed_paused = tick == smoke.paused_tick;
            check(smoke, stayed_paused, "pause freezes authoritative tick");
            activate(world, smoke, AppAction::Step);
        }
        104 => {
            let one_step = tick == smoke.paused_tick + 1;
            check(smoke, one_step, "single-step advances exactly one tick");
            submit_fast_forward(world, smoke, 380);
        }
        112 => {
            check(
                smoke,
                !terminal,
                "combat capture precedes the terminal result",
            );
            capture(world, smoke, &options.capture_dir, "03-battle.png");
        }
        118 => activate(world, smoke, AppAction::ToggleHelp),
        124 => {
            check(smoke, world.resource::<Session>().help, "help panel opened");
            capture(world, smoke, &options.capture_dir, "04-help.png");
        }
        128 => activate(world, smoke, AppAction::ToggleHelp),
        132 => {
            check(
                smoke,
                !world.resource::<Session>().help,
                "help panel closed",
            );
            submit_fast_forward(world, smoke, 4000);
        }
        140 => {
            check(smoke, terminal, "battle reaches terminal outcome");
            capture(world, smoke, &options.capture_dir, "05-result.png");
        }
        146 => {
            smoke.restart_generation = world.resource::<Session>().generation;
            activate(world, smoke, AppAction::Restart);
        }
        148 => {
            let session = world.resource::<Session>();
            let restarted = session.is_ready()
                && session.generation != smoke.restart_generation
                && session
                    .battle
                    .as_ref()
                    .is_some_and(|battle| battle.outcome.is_none() && battle.stats.tick < 30);
            check(
                smoke,
                restarted,
                "restart publishes a fresh live generation",
            );
        }
        152 => {
            smoke.menu_generation = world.resource::<Session>().generation;
            activate(world, smoke, AppAction::MainMenu);
        }
        158 => check(
            smoke,
            *world.resource::<State<Screen>>().get() == Screen::Menu,
            "return to menu",
        ),
        _ => {}
    }
    if smoke.frame >= 170 && smoke.captures_pending == 0 {
        let all_captured = smoke.captures == 5;
        check(
            smoke,
            all_captured,
            "all five screenshots were written successfully",
        );
        finish(world, options, smoke);
    }
}

fn exercise_configuration(world: &mut World, smoke: &mut SmokeState) -> bool {
    let category_checks = Team::ALL.len() * UnitCategory::ALL.len();
    if smoke.category_cursor < category_checks {
        let team = Team::ALL[smoke.category_cursor / UnitCategory::ALL.len()];
        let category = UnitCategory::ALL[smoke.category_cursor % UnitCategory::ALL.len()];
        if smoke.category_pending {
            let draft = &world.resource::<ScenarioDraft>().0;
            let selected = match team {
                Team::Blue => draft.blue.category,
                Team::Red => draft.red.category,
            };
            check(
                smoke,
                selected == category,
                &format!("category selector applies {} to {team:?}", category.label()),
            );
            smoke.category_cursor += 1;
            smoke.category_pending = false;
        } else {
            activate(world, smoke, AppAction::SetCategory(team, category));
            smoke.category_pending = true;
        }
        return false;
    }
    match smoke.category_cursor - category_checks {
        0 => {
            // Keep the familiar Creature scenario for the five UI captures.
            for team in Team::ALL {
                activate(
                    world,
                    smoke,
                    AppAction::SetCategory(team, UnitCategory::Creature),
                );
                activate_count_step(world, smoke, team, 1);
            }
            let draft = &world.resource::<ScenarioDraft>().0;
            smoke.count_baseline = [draft.blue.count, draft.red.count];
        }
        1 => {
            let draft = &world.resource::<ScenarioDraft>().0;
            check(
                smoke,
                draft.blue.category == UnitCategory::Creature
                    && draft.red.category == UnitCategory::Creature,
                "both armies restored to Creature",
            );
            for team in Team::ALL {
                activate(world, smoke, AppAction::ChangeCount(team, 1));
            }
        }
        2 => {
            let draft = &world.resource::<ScenarioDraft>().0;
            let actual = [draft.blue.count, draft.red.count];
            for team in Team::ALL {
                let incremented = actual[team.index()] == smoke.count_baseline[team.index()] + 1;
                check(
                    smoke,
                    incremented,
                    &format!("one-unit count step increments {team:?} by exactly one"),
                );
                activate(world, smoke, AppAction::ChangeCount(team, -1));
            }
        }
        3 => {
            let draft = &world.resource::<ScenarioDraft>().0;
            let restored = [draft.blue.count, draft.red.count] == smoke.count_baseline;
            check(smoke, restored, "one-unit decrement restores both counts");
            smoke.category_cursor += 1;
            return true;
        }
        _ => return true,
    }
    smoke.category_cursor += 1;
    false
}

fn submit_fast_forward(world: &mut World, smoke: &mut SmokeState, ticks: u32) {
    let mut session = world.resource_mut::<Session>();
    let before = session
        .battle
        .as_ref()
        .map_or(0, |battle| battle.stats.tick);
    smoke.fast_forward_target = before + u64::from(ticks);
    match session.submit_ticks(ticks) {
        Ok(()) => smoke
            .checks
            .push(format!("worker fast-forward submitted: {ticks} ticks")),
        Err(error) => {
            // Continue to the capture so the final report includes the failure,
            // rather than waiting forever for a job that was not accepted.
            smoke.fast_forward_target = before;
            smoke.errors.push(format!(
                "Could not submit {ticks}-tick fast-forward: {error}"
            ));
        }
    }
}

fn exercise_render_check(world: &mut World, options: &LaunchOptions, smoke: &mut SmokeState) {
    smoke.frame = smoke.frame.saturating_add(1);
    if smoke.captures_requested != 0 {
        if smoke.captures_pending == 0 {
            let captured = smoke.captures == 1;
            check(smoke, captured, "render-check PNG was written successfully");
            finish(world, options, smoke);
        }
        return;
    }
    if !smoke.render_paused {
        let session = world.resource::<Session>();
        let advanced = session.is_ready()
            && *world.resource::<State<Screen>>().get() == Screen::Battle
            && session
                .battle
                .as_ref()
                .is_some_and(|battle| battle.stats.tick >= 2);
        if !advanced {
            return;
        }
        world.resource_mut::<Session>().set_paused(true);
        smoke.render_paused = true;
        check(
            smoke,
            true,
            "selected scenario completed at least two fixed ticks",
        );
        return;
    }
    let session = world.resource::<Session>();
    if !session.is_ready() || session.busy || !session.paused {
        return;
    }
    let snapshot = session
        .battle
        .as_ref()
        .expect("ready session has a snapshot")
        .clone();
    if smoke.render_snapshot.is_none() {
        match inspect_snapshot(&snapshot) {
            Ok(details) => {
                smoke.render_snapshot = Some(details);
                check(
                    smoke,
                    true,
                    "all stored unit arrays, numeric values and population counters are valid",
                );
            }
            Err(error) => {
                smoke.errors.push(error);
                finish(world, options, smoke);
                return;
            }
        }
    }
    if !render_settled(world, smoke) {
        return;
    }
    match inspect_render(world, &snapshot) {
        Ok(details) => {
            smoke.render_details = Some(details);
            check(
                smoke,
                true,
                "CPU render tiers and GPU point population match the stable snapshot",
            );
            check(
                smoke,
                true,
                "crowd and scene pipelines are ready and two additional render updates settled",
            );
            capture(world, smoke, &options.capture_dir, "render-check.png");
        }
        Err(error) => {
            smoke.errors.push(error);
            finish(world, options, smoke);
        }
    }
}

fn shader_error(world: &World) -> Option<String> {
    let health = world.get_resource::<CrowdRenderHealth>()?;
    health
        .0
        .shader_error
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn render_matches(world: &World) -> bool {
    let session = world.resource::<Session>();
    let Some(snapshot) = &session.battle else {
        return false;
    };
    let Some(counts) = world.get_resource::<RenderCounts>() else {
        return false;
    };
    let Some(health) = world.get_resource::<CrowdRenderHealth>() else {
        return false;
    };
    let alive = snapshot
        .stats
        .alive
        .iter()
        .map(|&count| count as usize)
        .sum::<usize>();
    // A terminal UI-smoke draw can legitimately leave no crowd draw calls.
    // The selected-scenario render check still requires compiled crowd pipelines.
    let pipelines_ready = health.0.pipelines_ready.load(Ordering::Acquire)
        || (alive == 0 && !world.resource::<LaunchOptions>().render_check);
    session.is_ready()
        && !session.busy
        && !counts.packing
        && counts.generation == session.generation
        && counts.source_tick == snapshot.stats.tick
        && counts.represented_units == alive
        && counts.submitted_points == alive
        && health.0.generation.load(Ordering::Acquire) == session.generation
        && health.0.source_tick.load(Ordering::Acquire) == snapshot.stats.tick
        && health.0.uploaded_points.load(Ordering::Acquire) == alive
        && pipelines_ready
}

fn render_settled(world: &World, smoke: &mut SmokeState) -> bool {
    if !render_matches(world) {
        smoke.render_stable_frames = 0;
        return false;
    }
    smoke.render_stable_frames += 1;
    // First observation establishes agreement; the next two application updates
    // allow two render iterations before requesting the GPU screenshot.
    if smoke.render_stable_frames < 3 {
        return false;
    }
    smoke.render_stable_frames = 0;
    true
}

fn inspect_snapshot(snapshot: &BattleSnapshot) -> Result<serde_json::Value, String> {
    let units = &snapshot.units;
    let total = snapshot.config.total_units() as usize;
    for (name, length) in [
        ("positions", units.positions.len()),
        ("previous_positions", units.previous_positions.len()),
        ("velocities", units.velocities.len()),
        ("headings", units.headings.len()),
        ("teams", units.teams.len()),
        ("kinds", units.kinds.len()),
        ("categories", units.categories.len()),
        ("health", units.health.len()),
        ("alive", units.alive.len()),
        ("attack_flash", units.attack_flash.len()),
    ] {
        if length != total {
            return Err(format!(
                "Snapshot {name} has {length} entries; expected {total}."
            ));
        }
    }
    let mut counted = [0_u32; 2];
    let mut alive = [0_u32; 2];
    let mut casualties = [0_u32; 2];
    let armies = [&snapshot.config.blue, &snapshot.config.red];
    for index in 0..total {
        for value in [
            units.positions[index].x,
            units.positions[index].y,
            units.previous_positions[index].x,
            units.previous_positions[index].y,
            units.velocities[index].x,
            units.velocities[index].y,
            units.headings[index],
            units.health[index],
            units.attack_flash[index],
        ] {
            if !value.is_finite() {
                return Err(format!("Unit {index} contains a nonfinite numeric value."));
            }
        }
        if units.health[index] < 0.0 || units.alive[index] != (units.health[index] > 0.0) {
            return Err(format!("Unit {index} has inconsistent health/alive state."));
        }
        let team = units.teams[index].index();
        if units.categories[index] != armies[team].category
            || units.kinds[index] != armies[team].kind
        {
            return Err(format!(
                "Unit {index} category/profile disagrees with its army configuration."
            ));
        }
        counted[team] += 1;
        if units.alive[index] {
            alive[team] += 1;
        } else {
            casualties[team] += 1;
        }
    }
    let expected = [snapshot.config.blue.count, snapshot.config.red.count];
    if counted != expected
        || alive != snapshot.stats.alive
        || casualties != snapshot.stats.casualties
    {
        return Err(format!(
            "Population mismatch: expected {expected:?}; stored {counted:?}; alive {alive:?} vs {:?}; casualties {casualties:?} vs {:?}.",
            snapshot.stats.alive, snapshot.stats.casualties
        ));
    }
    if [
        snapshot.stats.elapsed_secs,
        snapshot.stats.step_ms,
        snapshot.snapshot_ms,
    ]
    .iter()
    .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("Snapshot timing diagnostics contain invalid numeric values.".into());
    }
    if u64::from(snapshot.stats.full_detail_units) + u64::from(snapshot.stats.strategic_units)
        > total as u64
    {
        return Err("Simulation detail tiers exceed the stored population.".into());
    }
    Ok(serde_json::json!({
        "scenario": snapshot.config,
        "tick": snapshot.stats.tick,
        "elapsed_simulation_seconds": snapshot.stats.elapsed_secs,
        "population": {
            "requested_total": total, "stored_total": units.len(),
            "stored_by_team": counted, "alive_by_team": alive, "casualties_by_team": casualties,
            "alive_total": u64::from(alive[0]) + u64::from(alive[1]),
        },
        "category_labels": {"blue": snapshot.config.blue.category.label(), "red": snapshot.config.red.category.label()},
        "simulation_tiers_last_tick": {"full_detail": snapshot.stats.full_detail_units, "strategic": snapshot.stats.strategic_units},
        "memory_bytes": {"simulation_snapshot_vectors": units.allocated_bytes(), "estimated_authoritative_backend": snapshot.stats.estimated_state_bytes},
        "last_tick_cpu_ms": snapshot.stats.step_ms, "snapshot_copy_ms": snapshot.snapshot_ms,
        "snapshot_age_ms_at_inspection": snapshot.completed_at.elapsed().as_secs_f64() * 1000.0,
        "numeric_and_population_invariants_valid": true,
    }))
}

fn inspect_render(
    world: &mut World,
    snapshot: &BattleSnapshot,
) -> Result<serde_json::Value, String> {
    let generation = world.resource::<Session>().generation;
    let alive = snapshot
        .stats
        .alive
        .iter()
        .map(|&count| count as usize)
        .sum::<usize>();
    let mut points = 0_usize;
    let mut models = 0_usize;
    let mut batches = 0_usize;
    let mut data_bytes = 0_usize;
    for instances in world.query::<&CrowdInstances>().iter(world) {
        if instances.data.is_empty() {
            continue;
        }
        if instances.generation != generation || instances.source_tick != snapshot.stats.tick {
            return Err("A nonempty render batch refers to a stale snapshot.".into());
        }
        batches += 1;
        data_bytes += instances.data.len() * size_of::<InstanceData>();
        if instances.point_lod {
            points += instances.data.len();
        } else {
            models += instances.data.len();
        }
        for instance in instances.data.iter() {
            if instance
                .positions
                .iter()
                .chain(instance.appearance.iter())
                .chain(instance.behavior.iter())
                .any(|value| !value.is_finite())
            {
                return Err("Packed render instances contain a nonfinite value.".into());
            }
        }
    }
    let counts = world.resource::<RenderCounts>();
    if points != alive
        || points != counts.submitted_points
        || models != counts.detail_models
        || batches != counts.draw_batches
        || data_bytes != counts.snapshot_bytes
        || counts.represented_units != alive
        || models > alive
        || counts.detail_models.checked_add(counts.point_only_units) != Some(alive)
        || !counts.packing_ms.is_finite()
        || counts.packing_ms < 0.0
    {
        return Err(format!(
            "Render population/tier invariant failed: actual points={points}, models={models}, batches={batches}, bytes={data_bytes}, alive={alive}; counters={counts:?}."
        ));
    }
    if !render_matches(world) || shader_error(world).is_some() {
        return Err(
            "GPU generation, tick, point count or pipeline readiness changed before capture."
                .into(),
        );
    }
    let health = world.resource::<CrowdRenderHealth>();
    Ok(serde_json::json!({
        "generation": generation, "source_tick": snapshot.stats.tick,
        "represented_units": counts.represented_units, "submitted_points": points,
        "detail_models": models, "point_only_units": counts.point_only_units,
        "draw_batches": batches, "render_instance_bytes": data_bytes,
        "packing_ms": counts.packing_ms,
        "gpu_generation": health.0.generation.load(Ordering::Acquire),
        "gpu_source_tick": health.0.source_tick.load(Ordering::Acquire),
        "gpu_uploaded_points": health.0.uploaded_points.load(Ordering::Acquire),
        "pipelines_ready": health.0.pipelines_ready.load(Ordering::Acquire),
        "settled_updates_after_first_match": 2,
        "packed_numeric_values_valid": true,
        "count_semantics": "Submitted point instances before frustum, depth and occlusion tests; nearby detail models supplement those points. This is not a visible-pixel or FPS measurement.",
    }))
}

fn current_status(world: &World) -> serde_json::Value {
    let session = world.resource::<Session>();
    let counts = world.get_resource::<RenderCounts>();
    let health = world.get_resource::<CrowdRenderHealth>();
    serde_json::json!({
        "generation": session.generation, "tick": session.battle.as_ref().map(|battle| battle.stats.tick),
        "stored_population": session.battle.as_ref().map(|battle| battle.units.len()),
        "alive_by_team": session.battle.as_ref().map(|battle| battle.stats.alive),
        "preparing": session.preparing, "busy": session.busy, "paused": session.paused,
        "status": session.status,
        "render": counts.map(|counts| serde_json::json!({
            "generation": counts.generation, "source_tick": counts.source_tick,
            "packing": counts.packing, "points": counts.submitted_points,
            "represented_units": counts.represented_units,
        })),
        "gpu": health.map(|health| serde_json::json!({
            "generation": health.0.generation.load(Ordering::Acquire),
            "source_tick": health.0.source_tick.load(Ordering::Acquire),
            "uploaded_points": health.0.uploaded_points.load(Ordering::Acquire),
            "pipelines_ready": health.0.pipelines_ready.load(Ordering::Acquire),
        })),
    })
}

fn finish(world: &mut World, options: &LaunchOptions, smoke: &mut SmokeState) {
    smoke.finished = true;
    let expected_captures = if options.render_check { 1 } else { 5 };
    let passed = smoke.errors.is_empty() && smoke.captures == expected_captures;
    let report = serde_json::json!({
        "passed": passed,
        "mode": if options.render_check { "render-check" } else { "ui-smoke" },
        "requested_scenario": world.resource::<ScenarioDraft>().0,
        "frames": smoke.frame, "application_updates": smoke.updates,
        "wall_seconds": smoke.started_at.map(|began| began.elapsed().as_secs_f64()),
        "screenshots_requested": smoke.captures_requested,
        "screenshots_completed": smoke.captures, "screenshots_pending": smoke.captures_pending,
        "screenshot_files": smoke.capture_files,
        "checks": smoke.checks, "errors": smoke.errors,
        "snapshot": smoke.render_snapshot, "render": smoke.render_details,
        "final_status": current_status(world),
        "supported_category_labels": UnitCategory::ALL.map(|category| category.label()),
        "build_profile": if cfg!(debug_assertions) { "development" } else { "release" },
        "method": if options.render_check {
            "Native Bevy application, the full selected scenario, immutable worker publications, CPU/GPU population agreement and GPU screenshot readback. Successful PNG writes are counted. No FPS claim."
        } else {
            "Native Bevy application, real UI Activate observers, immutable worker publications and GPU screenshot readback. Successful PNG writes are counted. No FPS claim."
        },
    });
    let name = if options.render_check {
        "render-check.json"
    } else {
        "smoke-report.json"
    };
    let path = options.capture_dir.join(name);
    let saved = serde_json::to_vec_pretty(&report)
        .map_err(|error| error.to_string())
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|error| error.to_string()));
    if let Err(error) = &saved {
        error!(
            "SMOKE report could not be written: {}: {error}",
            path.display()
        );
    }
    info!(
        "SMOKE RESULT: passed={}, report={}",
        passed && saved.is_ok(),
        path.display()
    );
    world.write_message(if passed && saved.is_ok() {
        AppExit::Success
    } else {
        AppExit::error()
    });
}
