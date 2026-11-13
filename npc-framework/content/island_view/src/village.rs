//! The played visitor's needs, diary and host-only idle supervisor controls.
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea};
use island_content::village::{
    self, DroppedDiary, VillageAct, VillageState, Villager, Vitality, Wallet,
};
use npc_framework::agent_io::scheduler::{
    self, SchedulerPolicy, SchedulerRuntime, SchedulerStatus,
};
use npc_framework::interaction::CarriedBy;
use npc_framework::kernel::commands::CommandQueue;
use npc_framework::kernel::journal::{Journal, JournalEntry, JournalKind};
use npc_framework::prelude::{SimClock, SimCommand, SimConfig, SimId, SimIndex, submit};
use npc_framework::view_helpers::{Selected, ViewSystems};

const PANEL: Color = Color::srgb(0.05, 0.06, 0.08);
const TEXT: Color = Color::srgb(0.93, 0.95, 0.90);
const GOLD: Color = Color::srgb(0.95, 0.80, 0.45);
#[derive(Resource, Default)]
struct Reader {
    open: bool,
    setup: bool,
    book: usize,
    selected_diary: Option<SimId>,
    page: usize,
    reset_scroll: bool,
    last_actor: Option<SimId>,
    message: String,
    refresh: f32,
}
/// A bounded presentation snapshot. Never registered as simulation state.
#[derive(Resource, Default)]
struct ObserverFeedback {
    activity: String,
    next: String,
    last_result: String,
}
#[derive(Component)]
struct VillageControl;
#[derive(Component)]
pub(crate) struct Hud;
#[derive(Component)]
struct HudText;
#[derive(Component)]
pub(crate) struct BookPanel;
#[derive(Component)]
struct BookScroll;
#[derive(Component)]
struct BookText;
#[derive(Component)]
struct VillageTypography;
#[derive(Resource)]
struct ColonyHudFont(Handle<Font>);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Books,
    Setup,
    Close,
    Previous,
    Next,
    NextBook,
    NewLife,
    PauseNudges,
    FollowHero,
}

pub(crate) fn plugin(app: &mut App) {
    // The supervisor is host-side in every scene; the island HUD is opt-in.
    if !app.world().contains_resource::<SchedulerRuntime>() {
        scheduler::configure_host(app.world_mut(), scheduler::default_bindings_path());
    }
    app.add_systems(PostUpdate, background);
    if !app.world().contains_resource::<VillageState>()
        && !app
            .world()
            .get_resource::<island_content::world_runtime::ActiveWorld>()
            .is_some_and(|w| w.0.scene == island_content::worlds::WorldScene::IslandVillage)
    {
        return;
    }
    if !app.is_plugin_added::<bevy::ui_widgets::ButtonPlugin>() {
        app.add_plugins(bevy::ui_widgets::ButtonPlugin);
    }
    if !app
        .world()
        .contains_resource::<crate::overlay::OverlayState>()
    {
        app.insert_resource(crate::overlay::OverlayState { visible: false });
    }
    if !app.is_plugin_added::<bevy::ui_widgets::ScrollAreaPlugin>() {
        app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    }
    app.add_systems(Startup, load_colony_font)
        .add_systems(Update, colony_typography.in_set(ViewSystems::Present));
    app.init_resource::<InputFocus>()
        .init_resource::<Reader>()
        .init_resource::<ObserverFeedback>()
        .init_resource::<super::village_world::FollowHero>()
        .add_observer(activate)
        .add_observer(focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_systems(Startup, spawn)
        .add_systems(Update, (keys, shield).in_set(ViewSystems::OverlayInput))
        .add_systems(
            Update,
            (observer_feedback, refresh, scheduler_caption, paint)
                .chain()
                .in_set(ViewSystems::Present),
        );
}
fn background(world: &mut World) {
    village::host::frame(world);
    scheduler::frame(world);
}
fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        VillageTypography,
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}
fn load_colony_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        commands.insert_resource(ColonyHudFont(fonts.add(font)));
    }
}
fn colony_typography(
    font: Option<Res<ColonyHudFont>>,
    mut text: Query<&mut TextFont, With<VillageTypography>>,
) {
    let Some(font) = font else { return };
    for mut text in &mut text {
        if text.font != bevy::text::FontSource::Handle(font.0.clone()) {
            text.font = font.0.clone().into();
            if text.font_size == FontSize::Px(12.0) {
                text.font_size = FontSize::Px(13.0);
            }
        }
    }
}
fn button(p: &mut ChildSpawnerCommands, label: &str, action: Action) {
    p.spawn((
        WidgetButton,
        VillageControl,
        action,
        TabIndex(0),
        Hovered::default(),
        AccessibleLabel::new(label),
        Node {
            padding: UiRect::axes(px(9), px(5)),
            min_height: px(28),
            flex_shrink: 0.0,
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.20, 0.24, 0.30)),
        BorderColor::all(Color::NONE),
    ))
    .with_child(text(label, 12.0, TEXT));
}
fn spawn(mut commands: Commands, active: Option<Res<island_content::world_runtime::ActiveWorld>>) {
    let colony = active.as_ref().is_some_and(|world| world.0.colony);
    commands
        .spawn((
            Name::new("village status"),
            Hud,
            Hovered::default(),
            TabGroup::new(3),
            GlobalZIndex(8),
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                top: px(12),
                width: px(if colony { 300 } else { 370 }),
                max_width: percent(94),
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(text(
                active
                    .as_ref()
                    .map_or("Island Village", |active| active.0.name.as_str()),
                17.0,
                GOLD,
            ));
            p.spawn((
                HudText,
                text("", 12.0, TEXT),
                TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
            ));
            p.spawn(Node {
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            })
            .with_children(|p| {
                button(p, "Found diaries", Action::Books);
                if !colony {
                    button(p, "AI scheduler", Action::Setup);
                }
                button(p, "Follow hero", Action::FollowHero);
                button(p, "New life", Action::NewLife);
            });
        });
    commands
        .spawn((
            Name::new("village diary reader"),
            BookPanel,
            Hovered::default(),
            TabGroup::modal(),
            GlobalZIndex(24),
            Node {
                position_type: PositionType::Absolute,
                right: percent(2),
                // The native AI-client status/footer remains visible below modal reader content.
                bottom: px(44),
                width: percent(92),
                max_width: px(740),
                height: percent(60),
                max_height: px(650),
                padding: UiRect::all(px(14)),
                row_gap: px(10),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
        ))
        .with_children(|p| {
            p.spawn(Node {
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|p| {
                button(p, "Close", Action::Close);
                button(p, "Previous page", Action::Previous);
                button(p, "Next page", Action::Next);
                button(p, "Next diary", Action::NextBook);
                button(p, "Pause / resume nudges", Action::PauseNudges);
            });
            p.spawn((
                ScrollArea,
                BookScroll,
                Node {
                    flex_grow: 1.0,
                    min_height: px(0),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
            ))
            .with_child((
                BookText,
                text("", 13.0, TEXT),
                TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
            ));
        });
}
#[derive(SystemParam)]
struct ActivationUi<'w> {
    reader: ResMut<'w, Reader>,
    follow: ResMut<'w, super::village_world::FollowHero>,
    input: ResMut<'w, InputFocus>,
}

fn activate(
    event: On<Activate>,
    actions: Query<(Entity, &Action)>,
    ui: ActivationUi,
    state: Res<VillageState>,
    mut policy: ResMut<SchedulerPolicy>,
    mut commands: Commands,
) {
    let ActivationUi {
        mut reader,
        mut follow,
        mut input,
    } = ui;
    let Ok((_, action)) = actions.get(event.entity) else {
        return;
    };
    match action {
        Action::Books => {
            reader.open = true;
            reader.setup = false;
            reader.refresh = 1.0;
            reader.reset_scroll = true;
        }
        Action::Setup => {
            reader.open = true;
            reader.setup = true;
            reader.refresh = 1.0;
            reader.reset_scroll = true;
        }
        Action::Close => {
            reader.open = false;
            input.clear();
        }
        Action::Previous => {
            reader.page = reader.page.saturating_sub(1);
            reader.refresh = 1.0;
            reader.reset_scroll = true;
        }
        Action::Next => {
            reader.page = reader.page.saturating_add(1);
            reader.refresh = 1.0;
            reader.reset_scroll = true;
        }
        Action::NextBook => {
            reader.book = reader.book.saturating_add(1);
            reader.page = 0;
            reader.selected_diary = None;
            reader.refresh = 1.0;
            reader.reset_scroll = true;
        }
        Action::PauseNudges => {
            policy.enabled = !policy.enabled;
            reader.refresh = 1.0;
        }
        Action::FollowHero => follow.toggle(),
        Action::NewLife => {
            let actor = state.played;
            commands.queue(move |world: &mut World| {
                let command = VillageAct::new(actor, "new_life");
                let result = submit(world, None, SimCommand::content(&command));
                world.resource_mut::<Reader>().message = match result {
                    Ok(_) => {
                        "New life requested. Resume the simulation to let the actor arrive.".into()
                    }
                    Err(e) => e.to_string(),
                };
            });
        }
    }
    if matches!(action, Action::Books | Action::Setup)
        && let Some((entity, _)) = actions.iter().find(|(_, a)| **a == Action::Close)
    {
        input.set(entity, FocusCause::Navigated);
    }
}
fn focus(
    mut event: On<AcquireFocus>,
    controls: Query<(), With<VillageControl>>,
    mut input: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        input.set(event.focused_entity, FocusCause::Navigated);
    }
}
fn input_active(input: Option<Res<InputFocus>>, controls: Query<(), With<VillageControl>>) -> bool {
    input
        .and_then(|f| f.get())
        .is_some_and(|e| controls.contains(e))
}
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut reader: ResMut<Reader>,
    mut input: ResMut<InputFocus>,
    controls: Query<(), With<VillageControl>>,
    nav: TabNavigation,
    mut commands: Commands,
) {
    if !input.get().is_some_and(|e| controls.contains(e)) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        reader.open = false;
        input.clear();
    }
    if keys.just_pressed(KeyCode::Tab) {
        let dir = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        if let Ok(next) = nav.navigate(&input, dir) {
            input.set(next, FocusCause::Navigated);
            commands.trigger(bevy::ui_widgets::ScrollIntoView { entity: next });
        }
    }
}
fn shield(
    roots: Query<&Hovered, Or<(With<Hud>, With<BookPanel>)>>,
    mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    scroll: Option<ResMut<AccumulatedMouseScroll>>,
    mut drag: ResMut<npc_framework::view_helpers::command_ui::DragBox>,
) {
    if !roots.iter().any(Hovered::get) {
        return;
    }
    if let Some(mut m) = mouse {
        m.reset_all();
    }
    if let Some(mut s) = scroll {
        s.delta = Vec2::ZERO;
    }
    drag.start = None;
}
fn compact(value: &str, limit: usize) -> String {
    let line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        line
    } else {
        let mut short: String = line.chars().take(limit.saturating_sub(1)).collect();
        short.push('…');
        short
    }
}
fn outcome(kind: &JournalKind) -> Option<String> {
    match kind {
        JournalKind::ActionRejected { verb, reason, .. } => {
            Some(format!("{verb} rejected: {reason}; AI: observe and retry"))
        }
        JournalKind::Interacted {
            verb,
            outcome,
            reason,
            ..
        } if outcome != "started" => Some(format!(
            "{verb}: {outcome}{}",
            reason
                .as_ref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default()
        )),
        JournalKind::ExternalAction { verb, status, .. }
            if matches!(
                status.as_str(),
                "done" | "failed" | "cancelled" | "rejected"
            ) =>
        {
            Some(format!("{verb}: {status}"))
        }
        JournalKind::OrderDone { order } => Some(format!("{order}: done")),
        JournalKind::OrderFailed { order, reason, .. } => Some(format!("{order}: {reason}")),
        JournalKind::CraftDone { recipe, .. } => Some(format!("{recipe}: crafted")),
        JournalKind::CraftFailed { recipe, reason, .. } => Some(format!("{recipe}: {reason}")),
        _ => None,
    }
}
fn actor_outcome(entry: &JournalEntry, actor: SimId) -> Option<String> {
    if let JournalKind::Effect {
        effect,
        applied: true,
    } = &entry.kind
    {
        let kind = effect.get("type")?.as_str()?;
        let good = || {
            effect
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|name| compact(name.trim_start_matches("village_"), 32))
        };
        if kind == "VillageTrade" {
            // A sale is journaled on the buyer. Read the two public participants, rather than
            // dropping the visitor's successful sale or displaying an unrelated resident trade.
            let id = u64::from(actor.0);
            let direction = if effect.get("buyer")?.as_u64()? == id {
                "Bought"
            } else if effect.get("seller")?.as_u64()? == id {
                "Sold"
            } else {
                return None;
            };
            return Some(format!(
                "{direction} {} {} for {} coins",
                effect.get("quantity")?.as_u64()?,
                good()?,
                effect.get("coins")?.as_u64()?,
            ));
        }
        if entry.unit != Some(actor) {
            return None;
        }
        return match kind {
            "VillageSaleCompleted" => Some(format!(
                "Sold {} {} for {} coins",
                effect.get("quantity")?.as_u64()?,
                good()?,
                effect.get("coins")?.as_u64()?,
            )),
            "VillageGathered" => Some(format!(
                "Gathered {} {}",
                effect.get("quantity")?.as_u64()?,
                good()?,
            )),
            "VillageConsumed" => Some(format!("Consumed {}", good()?)),
            "VillageBedRented" => Some(format!(
                "Rented bed for {} coins ({} ticks)",
                effect.get("coins")?.as_u64()?,
                effect.get("ticks")?.as_u64()?,
            )),
            "VillageDiaryRead" => Some("Read a recovered diary".into()),
            "VillageConversation" => Some("Talked with a resident; offers observed".into()),
            "VillageNewLife" => Some("New life arrived; enter_world and take_control".into()),
            "VillageDeath" => Some("Permanently died; the village continues".into()),
            _ => None,
        };
    }
    (entry.unit == Some(actor))
        .then(|| outcome(&entry.kind))
        .flatten()
}

/// Plain, bounded labels for player-facing results. Unknown registered action
/// names remain generic rather than exposing protocol identifiers or arguments.
pub(crate) fn player_action_label(verb: &str) -> &'static str {
    match verb {
        "stop_work" | "cancel" => "Stop",
        "execute_orders" | "order" | "move" | "walk" => "Walking",
        "buy" | "sell" | "offer_goods" | "withdraw_offer" => "Trade",
        "gather" => "Gathering",
        "hunt" => "Hunting",
        "eat" => "Eating",
        "drink" => "Drinking",
        "rest" | "rest_home" | "rest_ground" => "Rest",
        "talk" => "Conversation",
        "claim_plot" => "Land claim",
        "build_cottage" => "Building",
        "plant" | "water_crop" | "harvest" => "Gardening",
        _ => "Action",
    }
}

fn player_outcome(entry: &JournalEntry, actor: SimId) -> Option<String> {
    if matches!(entry.kind, JournalKind::Effect { .. }) {
        return actor_outcome(entry, actor).map(|result| {
            result
                .replace(
                    "New life arrived; enter_world and take_control",
                    "A new life is ready. Choose Play character.",
                )
                .replace("; offers observed", "")
        });
    }
    if entry.unit != Some(actor) {
        return None;
    }
    match &entry.kind {
        JournalKind::ExternalAction { verb, status, .. }
            if matches!(
                status.as_str(),
                "done" | "failed" | "cancelled" | "rejected"
            ) =>
        {
            if verb == "execute_orders" || verb == "order" {
                return None;
            }
            let action = player_action_label(verb);
            Some(match status.as_str() {
                "done" if verb == "stop_work" => "Stopped. Choose your next activity.".into(),
                "done" => format!("{action} finished."),
                "cancelled" => format!("{action} cancelled."),
                _ => format!("{action} could not finish. Choose another action."),
            })
        }
        JournalKind::OrderDone { .. } => Some("Arrived at the destination.".into()),
        JournalKind::OrderFailed { .. } => {
            Some("The route is blocked. Choose a nearby destination.".into())
        }
        JournalKind::CraftDone { .. } => Some("Crafting finished. The goods are ready.".into()),
        JournalKind::CraftFailed { .. } => {
            Some("Crafting could not finish. Check your supplies and try again.".into())
        }
        JournalKind::Interacted { verb, outcome, .. } if outcome != "started" => Some(format!(
            "{} {}.",
            player_action_label(verb),
            if outcome == "done" || outcome == "success" {
                "finished"
            } else {
                "ended"
            }
        )),
        JournalKind::ActionRejected { .. } => Some("That action is unavailable.".into()),
        _ => None,
    }
}

fn player_recovery(activity: &str, dead: bool, pending: bool, paused: bool) -> String {
    if dead {
        "This life has ended. Choose New life to return to the island.".into()
    } else if pending && paused {
        "Resume the world to start playing.".into()
    } else if paused {
        "World paused. Resume to walk, work and recover.".into()
    } else if pending {
        "Getting ready to play.".into()
    } else {
        match activity {
            "blocked" => "The route is blocked. Choose Stop, then a nearby destination.",
            "resting" => "Rest restores energy. Choose Stop when you are ready.",
            "building" | "gathering" | "hunting" | "crafting" | "walking" | "working" => {
                "Your activity is underway. Choose Stop to change it."
            }
            "awaiting_arrival" => "Choose Play character to join the island.",
            _ => "WASD walks. F2 opens home, work and trade.",
        }
        .into()
    }
}

fn observer_feedback(world: &mut World) {
    // Match the HUD's twice-per-second refresh, including while paused.
    if world.resource::<Reader>().refresh + world.resource::<Time<Real>>().delta_secs() < 0.5 {
        return;
    }
    let actor = world.resource::<VillageState>().played;
    let Some(entity) = world.resource::<SimIndex>().get(actor) else {
        return;
    };
    let (activity, _, _recovery) = village::activity(world, entity);
    let activity_name = activity;
    let activity = {
        match activity {
            "idle" => "Ready to explore",
            "awaiting_arrival" => "Ready to join",
            "resting" => "Resting",
            "building" => "Building",
            "gathering" => "Gathering",
            "hunting" => "Hunting",
            "crafting" => "Crafting",
            "walking" => "Walking",
            "blocked" => "Route blocked",
            "working" => "Working",
            "dead" => "Life ended",
            _ => "On the island",
        }
        .into()
    };
    let dead = world
        .get::<Vitality>(entity)
        .is_some_and(|v| v.died_at.is_some());
    let paused = world
        .get_resource::<npc_framework::prelude::SimControl>()
        .is_some_and(|s| s.is_stopped());
    let pending = world.get_resource::<CommandQueue>().is_some_and(|q| {
        q.pending.values().any(|c| {
            matches!(c, SimCommand::Content { kind, payload }
            if kind == "AttachController"
                && payload.get("unit").and_then(|v| v.as_u64()) == Some(u64::from(actor.0)))
        })
    });
    let next = player_recovery(activity_name, dead, pending, paused);
    let last_result = world
        .get_resource::<Journal>()
        .and_then(|j| {
            j.entries.iter().rev().find_map(|entry| {
                (player_outcome(entry, actor)).map(|mut result| {
                    if let JournalKind::ActionRejected {
                        controller,
                        verb,
                        reason,
                    } = &entry.kind
                    {
                        let details = world
                            .get_resource::<npc_framework::agent_io::stats::AgentStats>()
                            .and_then(|stats| stats.controllers.get(controller))
                            .and_then(|stats| stats.last_rejection.as_ref())
                            .filter(|details| {
                                details.unit == actor.0
                                    && details.tick == entry.tick
                                    && details.verb == *verb
                                    && details.reason == *reason
                            });
                        result = public_refusal(
                            world,
                            verb,
                            reason,
                            details.map(|details| details.message.as_str()),
                        );
                    }
                    compact(&result, 120)
                })
            })
        })
        .unwrap_or_else(|| "Explore, meet your neighbours and choose a place to settle.".into());
    *world.resource_mut::<ObserverFeedback>() = ObserverFeedback {
        activity,
        next,
        last_result,
    };
}
fn public_refusal(world: &World, verb: &str, reason: &str, details: Option<&str>) -> String {
    let Some(verb) = super::village_world::public_verb(world, verb) else {
        return "That action is unavailable. Choose an activity in Home & farm.".into();
    };
    // Handler errors may echo unknown argument keys/values, affordance IDs or input text.
    // Only exact static scene guidance is suitable for a public stream HUD. Full diagnostics
    // remain available in the authorized API response and host tooling.
    let static_guidance = details
        .filter(|message| {
            reason == "unavailable"
                && matches!(
                    *message,
                    "buy bread first"
                        | "buy water first"
                        | "buy village_water first"
                        | "move within 1.5 m of the trader"
                        | "move within 1.5 m of the trader, then observe again"
                        | "merchant cannot afford this trade; try another buyer or wait for trade"
                )
        })
        .map(|message| {
            if message == "buy village_water first" {
                "buy clean water first"
            } else {
                message
            }
        });
    let category = match reason {
        "invalid_args" => "invalid arguments",
        "stale" => "observation is outdated",
        "read_only" | "not_controller" | "not_attached" => "control is unavailable",
        _ => "unavailable",
    };
    format!(
        "{} is unavailable: {}. Choose another activity in Home & farm.",
        player_action_label(verb),
        static_guidance.unwrap_or(category)
    )
}
fn supervisor_feedback(state: &str, idle_seconds: u64, threshold: u64) -> String {
    match state {
        "unlinked" => {
            "Unlinked. F10: Start Codex / Start Claude; manual linking is only for legacy sessions."
                .into()
        }
        "idle timer" => format!(
            "Idle: {}s until nudge (scheduler)",
            threshold.saturating_sub(idle_seconds)
        ),
        "running" => "Running a continuation (AI); no overlapping turn.".into(),
        "cooldown" => "Cooldown / failure backoff; scheduler waits.".into(),
        "ready" => "Ready; scheduler checks on its next poll.".into(),
        "game paused" => "World paused. Operator: resume the world.".into(),
        "registration revoked or disconnected" => {
            "Disconnected. AI: enter_world and take_control again.".into()
        }
        "hourly limit" => "Hourly cap reached; scheduler waits for a slot.".into(),
        state if state.starts_with("busy:") => {
            format!("{}; waiting for the actor.", compact(state, 70))
        }
        state => compact(state, 100),
    }
}
#[derive(SystemParam)]
struct LiveData<'w, 's> {
    state: Res<'w, VillageState>,
    clock: Res<'w, SimClock>,
    config: Res<'w, SimConfig>,
    index: Res<'w, SimIndex>,
    people: Query<'w, 's, (&'static Villager, &'static Vitality, &'static Wallet)>,
    books: Query<'w, 's, (&'static SimId, &'static DroppedDiary, &'static CarriedBy)>,
    policy: Res<'w, SchedulerPolicy>,
    status: Option<Res<'w, SchedulerStatus>>,
    bridge: Option<Res<'w, village::host::DiaryBridge>>,
    needs: Query<'w, 's, &'static npc_framework::needs::Needs>,
    registry: Res<'w, npc_framework::needs::NeedRegistry>,
    feedback: Res<'w, ObserverFeedback>,
    market: Option<Res<'w, village::market::MarketState>>,
    colony: Option<Res<'w, village::colony::ColonyState>>,
}
#[allow(clippy::too_many_arguments)]
fn refresh(
    time: Res<Time<Real>>,
    data: LiveData,
    mut reader: ResMut<Reader>,
    mut texts: Query<(&mut Text, Has<HudText>), Or<(With<HudText>, With<BookText>)>>,
    mut panels: Query<&mut Node, With<BookPanel>>,
    mut buttons: Query<(Entity, &Action, &mut Node), Without<BookPanel>>,
    mut commands: Commands,
    mut input: ResMut<InputFocus>,
    mut selected: ResMut<Selected>,
    mut scroll: Query<&mut ScrollPosition, With<BookScroll>>,
) {
    let LiveData {
        state,
        clock,
        config,
        index,
        people,
        books,
        policy,
        status,
        bridge,
        needs,
        registry,
        feedback,
        market,
        colony,
    } = data;
    let colony = colony.is_some();
    reader.refresh += time.delta_secs();
    if reader.last_actor != Some(state.played) {
        if reader.last_actor.is_some() {
            reader.message.clear();
            selected.0 = Some(state.played);
            input.clear();
            reader.open = false;
            reader.page = 0;
            reader.book = 0;
            reader.selected_diary = None;
        }
        reader.last_actor = Some(state.played);
    }
    for mut node in &mut panels {
        node.display = if reader.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    let Some(e) = index.get(state.played) else {
        return;
    };
    let Ok((person, vital, wallet)) = people.get(e) else {
        return;
    };
    let mut held: Vec<_> = books
        .iter()
        .filter(|(_, _, c)| c.holder == state.played)
        .map(|(id, b, _)| (*id, b))
        .collect();
    held.sort_by_key(|(id, _)| *id);
    if held.is_empty() {
        reader.book = 0;
        reader.page = 0;
        reader.selected_diary = None;
    } else {
        if let Some(index) = held
            .iter()
            .position(|(id, _)| Some(*id) == reader.selected_diary)
        {
            reader.book = index;
        } else {
            reader.book %= held.len();
            reader.page = 0;
            reader.reset_scroll = true;
        }
        reader.selected_diary = Some(held[reader.book].0);
        reader.page = reader
            .page
            .min(held[reader.book].1.pages.len().saturating_sub(1));
    }
    if reader.reset_scroll {
        for mut position in &mut scroll {
            position.0 = Vec2::ZERO;
        }
        reader.reset_scroll = false;
    }
    for (entity, action, mut node) in &mut buttons {
        if *action == Action::NewLife {
            node.display = if vital.died_at.is_some() {
                Display::Flex
            } else {
                Display::None
            };
        }
        if *action == Action::Books {
            if held.is_empty() {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
        if matches!(action, Action::Previous | Action::Next | Action::NextBook) {
            node.display = if reader.setup {
                Display::None
            } else {
                Display::Flex
            };
            let disabled = match action {
                Action::Previous => held.is_empty() || reader.page == 0,
                Action::Next => {
                    held.is_empty() || reader.page + 1 >= held[reader.book].1.pages.len()
                }
                Action::NextBook => held.len() <= 1,
                _ => false,
            };
            if disabled {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
        if *action == Action::PauseNudges {
            node.display = if reader.setup {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    if reader.refresh < 0.5 {
        return;
    }
    reader.refresh = 0.0;
    let (day, season, _) = village::calendar_at(clock.tick, &config, market.is_some());
    let season = match season {
        village::Season::Spring => "Spring",
        village::Season::Summer => "Summer",
        village::Season::Autumn => "Autumn",
        village::Season::Winter => "Winter",
    };
    let reserve = |name: &str| {
        registry
            .id(name)
            .and_then(|id| {
                needs.get(e).ok().map(|n| {
                    let need = n.get(id);
                    let urgency = match need.level {
                        npc_framework::prelude::NeedLevel::Ok => "",
                        npc_framework::prelude::NeedLevel::Low => " low",
                        npc_framework::prelude::NeedLevel::Urgent => " URGENT",
                    };
                    format!("{}%{urgency}", need.value / 1000)
                })
            })
            .unwrap_or_else(|| "n/a".into())
    };
    let supervisor = if vital.died_at.is_some() {
        "actor is permanently dead".into()
    } else if !policy.enabled {
        "nudges paused".into()
    } else {
        status
            .as_ref()
            .and_then(|s| s.agents.iter().find(|a| a.unit == state.played.0))
            .map(|a| supervisor_feedback(&a.state, a.idle_seconds, policy.idle_seconds))
            .unwrap_or_else(|| {
                "Waiting for AI control; operator connects, AI takes control.".into()
            })
    };
    let price = |kind: &str| {
        market
            .as_ref()
            .and_then(|market| market.price(kind))
            .or_else(|| village::catalog::price(kind))
            .map(|p| p.to_string())
            .unwrap_or_else(|| "n/a".into())
    };
    let mut header = if colony {
        format!(
            "Day {} · {} · {}\n{} · {} coins · Health {}%\nFood {} · Water {}\nEnergy {} · Warmth {}\n{}\n{}\n{}\nF2 home & farm · F7 notebook",
            day + 1,
            season,
            if config.is_night(clock.tick) {
                "Night"
            } else {
                "Daylight"
            },
            compact(&person.name, 24),
            wallet.coins,
            vital.health / 1000,
            reserve("Hunger"),
            reserve("Thirst"),
            reserve("Energy"),
            reserve("Warmth"),
            feedback.activity,
            feedback.last_result,
            feedback.next,
        )
    } else {
        let mut header = format!(
            "Day {} | {} | {}\n{} | life {}\n{} coins | Health {}%\nFood {} | Water {} | Energy {}\nBread {} coins | Water {} | Bed {}\n{}\n{}\n{}\nLegacy scheduler: {}",
            day + 1,
            season,
            if config.is_night(clock.tick) {
                "Night"
            } else {
                "Day"
            },
            compact(&person.name, 40),
            person.life,
            wallet.coins,
            vital.health / 1000,
            reserve("Hunger"),
            reserve("Thirst"),
            reserve("Energy"),
            price("bread"),
            price(village::catalog::WATER),
            village::catalog::BED_PRICE,
            feedback.activity,
            feedback.last_result,
            feedback.next,
            supervisor
        );
        if vital.died_at.is_some() {
            header.push_str("\nPermanently dead. The village continues.\nYour dropped diary can be recovered by others.");
        } else {
            header.push_str("\nGather / craft > sell > buy > eat, drink, rest.\nDiary drops on death; private books stay private.");
        }
        if market.is_some() {
            header.push_str(&format!(
                "\nWarmth {} | Needs-driven market\nGather, hunt, craft or post goods for sale.",
                reserve("Warmth")
            ));
        }
        header.push_str("\nF1 controls | F7 notebook");
        header
    };
    if let Some(error) = bridge.as_ref().and_then(|b| b.error.as_deref()) {
        header.push_str(&format!("\n{}", compact(error, 160)));
    }
    if !colony && let Some(error) = status.as_ref().and_then(|b| b.error.as_deref()) {
        header.push_str(&format!("\nLegacy scheduler: {}", compact(error, 160)));
    }
    if !reader.message.is_empty() {
        header.push_str(&format!("\n{}", compact(&reader.message, 160)));
    }
    let body = if reader.setup {
        let client_help = "Run the game with --remote for AI agent control over agent/*.";
        let event_status = status
            .as_ref()
            .and_then(|s| s.agents.iter().find(|a| a.unit == state.played.0));
        let event_feedback = format!(
            "Event dispatch: {} | {} pending groups\nQueued / last trigger: {} | missed/evicted events: {}",
            if policy.events_enabled {
                "enabled"
            } else {
                "disabled"
            },
            event_status.map_or(0, |a| a.pending_events),
            event_status
                .and_then(|a| a.trigger.as_deref())
                .unwrap_or("none"),
            event_status.map_or(0, |a| a.event_overflow),
        );
        format!(
            "AI agent supervision\n\n{}\n\nLegacy host scheduler: {}\n{}\nIdle threshold: {} real seconds\nCooldown: {} seconds after a turn (longer after failures)\nLimit: {} nudges per hour; {} at a time\n\nRelevant events can continue an AI conversation before the idle threshold. Busy actors, active turns and paused worlds wait. The agent observes current state before acting. The dedicated client owns sessions separately and starts from F10, even when legacy host nudges are paused.\n\nConfigure timing defaults under Worlds > AI scheduler. Pause / resume nudges here affects only the legacy host supervisor. Advanced compatibility commands are in docs/ai-agent-game-client.md.\n\nLegacy status: {}\nLast legacy continuation: {}",
            client_help,
            if policy.enabled {
                "Nudges enabled"
            } else {
                "Nudges paused"
            },
            event_feedback,
            policy.idle_seconds,
            policy.cooldown_seconds,
            policy.max_nudges_per_hour,
            policy.max_concurrent,
            supervisor,
            status
                .as_ref()
                .and_then(|s| s.agents.iter().find(|a| a.unit == state.played.0))
                .and_then(|a| a.last_result.as_deref())
                .map(|s| compact(s, 160))
                .unwrap_or_else(|| "none".into())
        )
    } else if held.is_empty() {
        "Found diaries\n\nPick up a dropped diary, or ask a villager who recovered one. Your own F7 Diary will become a readable item on death; Memory and Proposals stay private.".into()
    } else {
        reader.book %= held.len();
        let (id, book) = held[reader.book];
        if book.pages.is_empty() {
            format!(
                "{}'s diary | life {} | #{}\n\n{}",
                book.author_name,
                book.life,
                id.0,
                if book.sealed {
                    "No pages were written."
                } else {
                    "Diary pages are being published. Original files remain preserved."
                }
            )
        } else {
            reader.page = reader.page.min(book.pages.len() - 1);
            let page = &book.pages[reader.page];
            format!(
                "{}'s diary | life {}\nDiary {} of {} | Page {} of {} | tick {}\n\n{}\n\n{}",
                book.author_name,
                book.life,
                reader.book + 1,
                held.len(),
                reader.page + 1,
                book.pages.len(),
                page.tick,
                page.title,
                page.body
            )
        }
    };
    for (mut text, hud) in &mut texts {
        let wanted = if hud { &header } else { &body };
        if text.0 != *wanted {
            text.0.clone_from(wanted);
        }
    }
}
fn scheduler_caption(
    mut commands: Commands,
    policy: Res<SchedulerPolicy>,
    follow: Res<super::village_world::FollowHero>,
    controls: Query<(Entity, &Action, &Children)>,
    mut captions: Query<&mut Text>,
) {
    if !policy.is_changed() && !follow.is_changed() {
        return;
    }
    for (entity, action, children) in &controls {
        let caption = match action {
            Action::PauseNudges => {
                if policy.enabled {
                    "Pause nudges"
                } else {
                    "Resume nudges"
                }
            }
            Action::FollowHero => {
                if follow.enabled {
                    "Stop following"
                } else {
                    "Follow hero"
                }
            }
            _ => continue,
        };
        let mut changed = false;
        for child in children {
            if let Ok(mut text) = captions.get_mut(*child)
                && text.0 != caption
            {
                text.0 = caption.into();
                changed = true;
            }
        }
        if changed {
            commands
                .entity(entity)
                .insert(AccessibleLabel::new(caption));
        }
    }
}
fn paint(
    input: Res<InputFocus>,
    mut buttons: Query<
        (
            Entity,
            &Hovered,
            Has<InteractionDisabled>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<VillageControl>,
    >,
) {
    for (e, hovered, disabled, mut bg, mut border) in &mut buttons {
        bg.0 = if disabled {
            Color::srgb(0.10, 0.12, 0.15)
        } else if hovered.get() {
            Color::srgb(0.27, 0.32, 0.40)
        } else {
            Color::srgb(0.20, 0.24, 0.30)
        };
        *border = BorderColor::all(if input.get() == Some(e) {
            GOLD
        } else {
            Color::NONE
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::prelude::state_hash;
    #[test]
    fn colony_feedback_is_plain_read_only_and_does_not_echo_unknown_actions() {
        let mut app = island_content::new_sim_app(7, "living_world");
        app.init_resource::<Time<Real>>()
            .init_resource::<Reader>()
            .init_resource::<ObserverFeedback>();
        let actor = app.world().resource::<VillageState>().played;
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&village::colony::JoinIsland { unit: actor.0 }),
        )
        .unwrap();
        npc_framework::prelude::run_ticks(app.world_mut(), 1);
        app.insert_resource(npc_framework::prelude::SimControl {
            paused: false,
            multiplier: 0,
            ..default()
        });
        {
            let mut journal = app.world_mut().resource_mut::<Journal>();
            journal.enabled = true;
            journal.push(
                2,
                Some(actor),
                JournalKind::ExternalAction {
                    handle: 1,
                    verb: "stop_work".into(),
                    status: "done".into(),
                    latency_ticks: None,
                },
            );
        }
        let before = state_hash(app.world_mut());
        app.world_mut().resource_mut::<Reader>().refresh = 1.0;
        observer_feedback(app.world_mut());
        let feedback = app.world().resource::<ObserverFeedback>();
        assert!(
            feedback.next.contains("World paused"),
            "zero speed is stopped even without the pause flag"
        );
        assert!(feedback.last_result.contains("Stopped"));
        assert!(!feedback.last_result.contains("stop_work"));
        assert!(!feedback.next.contains("AI:") && !feedback.next.contains("Operator:"));
        assert_eq!(before, state_hash(app.world_mut()));
        app.world_mut().resource_mut::<Journal>().push(
            3,
            Some(actor),
            JournalKind::ActionRejected {
                controller: "visitor".into(),
                verb: "unknown-private-action".into(),
                reason: "private-argument".into(),
            },
        );
        app.world_mut().resource_mut::<Reader>().refresh = 1.0;
        observer_feedback(app.world_mut());
        let result = &app.world().resource::<ObserverFeedback>().last_result;
        assert!(result.contains("Home & farm"));
        assert!(!result.contains("unknown-private-action") && !result.contains("private-argument"));
    }
    #[test]
    fn observer_summarizes_needs_actions_without_diary_content() {
        let actor = SimId(1);
        let entry = |kind: &str, payload: serde_json::Value| JournalEntry {
            tick: 1,
            seq: 0,
            unit: Some(actor),
            kind: JournalKind::effect(kind, &payload),
        };
        assert_eq!(
            actor_outcome(
                &entry(
                    "VillageConsumed",
                    serde_json::json!({"kind":"village_water","need":"Thirst"})
                ),
                actor
            )
            .as_deref(),
            Some("Consumed water")
        );
        assert_eq!(
            actor_outcome(
                &entry(
                    "VillageBedRented",
                    serde_json::json!({"coins":3,"ticks":300})
                ),
                actor
            )
            .as_deref(),
            Some("Rented bed for 3 coins (300 ticks)")
        );
        assert_eq!(
            actor_outcome(
                &entry(
                    "VillageDiaryRead",
                    serde_json::json!({"entry":{"body":"private body not for the HUD"}})
                ),
                actor
            )
            .as_deref(),
            Some("Read a recovered diary")
        );
        assert!(
            actor_outcome(
                &entry("VillageConsumed", serde_json::json!({"kind":"bread"})),
                SimId(2)
            )
            .is_none()
        );
    }
    #[test]
    fn supervisor_recovery_countdown_and_long_feedback_are_bounded() {
        let unlinked = supervisor_feedback("unlinked", 0, 120);
        assert!(unlinked.contains("F10: Start Codex / Start Claude"));
        assert!(unlinked.contains("manual linking is only for legacy sessions"));
        assert!(supervisor_feedback("idle timer", 90, 120).contains("30s until nudge"));
        assert!(supervisor_feedback("idle timer", 125, 120).contains("0s until nudge"));
        assert!(supervisor_feedback("running", 0, 120).contains("AI"));
        assert!(supervisor_feedback("cooldown", 0, 120).contains("backoff"));
        assert_eq!(
            compact("\nfull   inventory\r\ntry another buyer", 100),
            "full inventory try another buyer"
        );
        let long = "長".repeat(300);
        assert_eq!(compact(&long, 40).chars().count(), 40);
        assert!(compact(&long, 40).ends_with('…'));
        assert!(supervisor_feedback(&long, 0, 120).chars().count() <= 100);
    }
}
