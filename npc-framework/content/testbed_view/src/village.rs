//! The played visitor's needs, diary and host-only idle supervisor controls.
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea};
use npc_framework::agent_io::scheduler::{
    self, SchedulerPolicy, SchedulerRuntime, SchedulerStatus,
};
use npc_framework::interaction::CarriedBy;
use npc_framework::prelude::{SimClock, SimCommand, SimConfig, SimId, SimIndex, submit};
use npc_framework::view_helpers::{Selected, ViewSystems};
use testbed_content::village::{
    self, DroppedDiary, VillageAct, VillageState, Villager, Vitality, Wallet,
};

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
            .get_resource::<testbed_content::world_runtime::ActiveWorld>()
            .is_some_and(|w| w.0.scene == testbed_content::worlds::WorldScene::IslandVillage)
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
    app.init_resource::<InputFocus>()
        .init_resource::<Reader>()
        .add_observer(activate)
        .add_observer(focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_systems(Startup, spawn)
        .add_systems(Update, (keys, shield).in_set(ViewSystems::OverlayInput))
        .add_systems(
            Update,
            (refresh, scheduler_caption, paint)
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
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
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
fn spawn(mut commands: Commands) {
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
                width: px(370),
                max_width: percent(94),
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(text("Island Village", 17.0, GOLD));
            p.spawn((HudText, text("", 12.0, TEXT)));
            p.spawn(Node {
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            })
            .with_children(|p| {
                button(p, "Found diaries", Action::Books);
                button(p, "AI scheduler", Action::Setup);
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
                bottom: px(12),
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
            .with_child((BookText, text("", 13.0, TEXT)));
        });
}
fn activate(
    event: On<Activate>,
    actions: Query<(Entity, &Action)>,
    mut reader: ResMut<Reader>,
    state: Res<VillageState>,
    mut policy: ResMut<SchedulerPolicy>,
    mut input: ResMut<InputFocus>,
    mut commands: Commands,
) {
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
#[derive(SystemParam)]
struct LiveData<'w, 's> {
    state: Res<'w, VillageState>,
    clock: Res<'w, SimClock>,
    config: Res<'w, SimConfig>,
    index: Res<'w, SimIndex>,
    people: Query<'w, 's, (&'static Villager, &'static Vitality, &'static Wallet)>,
    controllers: Query<'w, 's, &'static npc_framework::agent_io::ExternalController>,
    books: Query<'w, 's, (&'static SimId, &'static DroppedDiary, &'static CarriedBy)>,
    policy: Res<'w, SchedulerPolicy>,
    status: Option<Res<'w, SchedulerStatus>>,
    bridge: Option<Res<'w, village::host::DiaryBridge>>,
    session: Option<Res<'w, testbed_content::explorer::host::ExplorerSession>>,
    needs: Query<'w, 's, &'static npc_framework::needs::Needs>,
    registry: Res<'w, npc_framework::needs::NeedRegistry>,
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
        controllers,
        books,
        policy,
        status,
        bridge,
        session,
        needs,
        registry,
    } = data;
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
    let day = clock.tick / config.day_length.max(1);
    let season = match day / village::DAYS_PER_SEASON % 4 {
        0 => "Spring",
        1 => "Summer",
        2 => "Autumn",
        _ => "Winter",
    };
    let reserve = |name: &str| {
        registry
            .id(name)
            .and_then(|id| needs.get(e).ok().map(|n| n.get(id).value / 1000))
            .unwrap_or(100)
    };
    let supervisor = if vital.died_at.is_some() {
        "actor is permanently dead".into()
    } else if !policy.enabled {
        "nudges paused".into()
    } else {
        status
            .as_ref()
            .and_then(|s| s.agents.iter().find(|a| a.unit == state.played.0))
            .map(|a| format!("{} (idle {}s)", a.state, a.idle_seconds))
            .unwrap_or_else(|| {
                if policy.enabled {
                    "waiting for AI connection".into()
                } else {
                    "disabled".into()
                }
            })
    };
    let mut header = format!(
        "Day {} | {} | {}\n{} | life {}\n{} coins | Health {}%\nFood {}% | Water {}% | Energy {}%\nBread 6 coins | Water 1 | Bed 3\nAI scheduler: {}",
        day + 1,
        season,
        if config.is_night(clock.tick) {
            "Night"
        } else {
            "Day"
        },
        person.name,
        person.life,
        wallet.coins,
        vital.health / 1000,
        reserve("Hunger"),
        reserve("Thirst"),
        reserve("Energy"),
        supervisor
    );
    if vital.died_at.is_some() {
        header.push_str("\nPermanently dead. The village continues.\nYour dropped diary can be recovered by others.");
    } else {
        header.push_str("\nGather or craft, sell useful goods, then buy supplies.\nDiary drops on death; Memory and Proposals stay private.");
    }
    header.push_str("\nF1 controls | F7 notebook");
    if session.is_some()
        && person.life > 1
        && vital.died_at.is_none()
        && controllers.get(e).is_err()
    {
        header.push_str(
            "\nNew life has arrived. Re-enter the world, take control, then link its AI session.",
        );
    }
    if let Some(error) = bridge.as_ref().and_then(|b| b.error.as_deref()) {
        header.push_str(&format!("\n{error}"));
    }
    if let Some(error) = status.as_ref().and_then(|b| b.error.as_deref()) {
        header.push_str(&format!("\nAI scheduler: {error}"));
    }
    if !reader.message.is_empty() {
        header.push_str(&format!("\n{}", reader.message));
    }
    let body = if reader.setup {
        let link=session.as_ref().map(|s|format!("npcctl scheduler bind --home \"{}\" --agent \"{}\" --provider codex --session-id SESSION_UUID --turn-finished",s.dir.dir.parent().unwrap_or(&s.dir.dir).display(),s.meta.agent)).unwrap_or_else(||"npcctl scheduler bind --bindings FILE --controller visitor --unit ACTOR_ID --cwd AI_WORKSPACE --provider codex --session-id SESSION_UUID --turn-finished".into());
        format!(
            "AI scheduler\n\n{}\nIdle threshold: {} real seconds\nCooldown: {} seconds after a turn\nLimit: {} nudges per hour; {} at a time\n\nConnect the AI and let it take control. Link its exact existing CLI conversation once:\n\n{}\n\nUse --provider claude for Claude Code. Use --client-pid PID while the original one-shot CLI is still running. The supervisor resumes the linked conversation after the actor is idle; busy actions and a paused game wait.\n\nConfigure defaults under Worlds > AI scheduler. Pause / resume here changes this running session only. A different actor needs its own new link.\n\nStatus: {}",
            if policy.enabled {
                "Nudges enabled"
            } else {
                "Nudges paused"
            },
            policy.idle_seconds,
            policy.cooldown_seconds,
            policy.max_nudges_per_hour,
            policy.max_concurrent,
            link,
            supervisor
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
    controls: Query<(Entity, &Action, &Children)>,
    mut captions: Query<&mut Text>,
) {
    if !policy.is_changed() {
        return;
    }
    let caption = if policy.enabled {
        "Pause nudges"
    } else {
        "Resume nudges"
    };
    for (entity, action, children) in &controls {
        if *action != Action::PauseNudges {
            continue;
        }
        commands
            .entity(entity)
            .insert(AccessibleLabel::new(caption));
        for child in children {
            if let Ok(mut text) = captions.get_mut(*child) {
                text.0 = caption.into();
            }
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
    use npc_framework::prelude::{spawn_sim, state_hash};
    fn app() -> App {
        // Real hosts install the view before scenario entities / VillageState exist.
        let mut app = testbed_content::try_new_sim_app_with(3, "ai_village", |app| {
            app.add_plugins((
                MinimalPlugins,
                AssetPlugin::default(),
                bevy::text::TextPlugin,
            ))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Selected>()
            .init_resource::<npc_framework::view_helpers::command_ui::DragBox>()
            .configure_sets(
                Update,
                (
                    ViewSystems::OverlayInput,
                    ViewSystems::Input,
                    ViewSystems::Present,
                )
                    .chain(),
            );
            app.world_mut().resource_mut::<SchedulerPolicy>().enabled = false;
            scheduler::configure_host(
                app.world_mut(),
                std::env::temp_dir()
                    .join(format!("village-ui-test-{}", std::process::id()))
                    .join("absent-bindings.json"),
            );
            app.add_plugins(plugin);
        })
        .unwrap();
        app.update();
        app
    }
    fn press(app: &mut App, action: Action) {
        let e = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, a)| **a == action)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: e });
        app.world_mut().flush();
        app.update();
    }
    fn control(app: &mut App, wanted: Action) -> Entity {
        app.world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, action)| **action == wanted)
            .unwrap()
            .0
    }
    fn recovered_book(app: &mut App, title: &str, pages: usize) -> (SimId, Entity) {
        let actor = app.world().resource::<VillageState>().played;
        let holder = app.world().resource::<SimIndex>().get(actor).unwrap();
        let (id, entity) = spawn_sim(
            app.world_mut(),
            (
                npc_framework::interaction::Item::new(village::catalog::DIARY),
                DroppedDiary {
                    author: SimId(9999),
                    author_name: title.into(),
                    life: 1,
                    died_at: 0,
                    sealed: true,
                    pages: (0..pages)
                        .map(|n| village::DiaryPage {
                            tick: 0,
                            title: format!("{title} page {}", n + 1),
                            body: "A recovered memory.".into(),
                        })
                        .collect(),
                },
            ),
        );
        npc_framework::interaction::items::put_in_inventory(app.world_mut(), holder, entity);
        (id, entity)
    }
    #[test]
    fn diary_navigation_is_stable_bounded_and_resets_scroll() {
        let mut app = app();
        let (first, item) = recovered_book(&mut app, "First", 2);
        let (second, _) = recovered_book(&mut app, "Second", 1);
        let actor = app.world().resource::<VillageState>().played;
        let holder = app.world().resource::<SimIndex>().get(actor).unwrap();
        let other = app
            .world()
            .resource::<testbed_content::TestbedIds>()
            .id("Ada");
        let other = app.world().resource::<SimIndex>().get(other).unwrap();
        // Changing archetype iteration order must not change book order or selection.
        npc_framework::interaction::items::put_in_inventory(app.world_mut(), other, item);
        npc_framework::interaction::items::put_in_inventory(app.world_mut(), holder, item);
        press(&mut app, Action::Books);
        assert_eq!(app.world().resource::<Reader>().selected_diary, Some(first));
        let previous = control(&mut app, Action::Previous);
        assert!(app.world().get::<InteractionDisabled>(previous).is_some());
        let scroll = app
            .world_mut()
            .query_filtered::<Entity, With<BookScroll>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .get_mut::<ScrollPosition>(scroll)
            .unwrap()
            .0
            .y = 500.0;
        press(&mut app, Action::Next);
        assert_eq!(app.world().resource::<Reader>().page, 1);
        assert_eq!(
            app.world().get::<ScrollPosition>(scroll).unwrap().0,
            Vec2::ZERO
        );
        let next = control(&mut app, Action::Next);
        assert!(app.world().get::<InteractionDisabled>(next).is_some());
        press(&mut app, Action::NextBook);
        assert_eq!(
            app.world().resource::<Reader>().selected_diary,
            Some(second)
        );
        assert_eq!(app.world().resource::<Reader>().page, 0);
    }
    #[test]
    fn a_single_page_book_has_no_unavailable_navigation_and_scheduler_controls_name_the_action() {
        let mut app = app();
        recovered_book(&mut app, "Only", 1);
        press(&mut app, Action::Books);
        for action in [Action::Previous, Action::Next, Action::NextBook] {
            let button = control(&mut app, action);
            assert!(app.world().get::<InteractionDisabled>(button).is_some());
        }
        press(&mut app, Action::Setup);
        let button = control(&mut app, Action::PauseNudges);
        assert!(
            format!("{:?}", app.world().get::<AccessibleLabel>(button)).contains("Resume nudges")
        );
        press(&mut app, Action::PauseNudges);
        assert!(
            format!("{:?}", app.world().get::<AccessibleLabel>(button)).contains("Pause nudges")
        );
    }
    #[test]
    fn native_hud_and_setup_controls_are_semantic_and_never_change_the_sim() {
        use bevy::a11y::AccessibilityNode;
        let mut app = app();
        let before = state_hash(app.world_mut());
        press(&mut app, Action::Setup);
        assert!(app.world().resource::<Reader>().open);
        let focus = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(*app.world().get::<Action>(focus).unwrap(), Action::Close);
        assert!(app.world().get::<AccessibilityNode>(focus).is_some());
        assert!(app.world().get::<AccessibleLabel>(focus).is_some());
        press(&mut app, Action::Close);
        assert!(app.world().resource::<InputFocus>().get().is_none());
        assert_eq!(state_hash(app.world_mut()), before);
    }
    #[test]
    fn queued_successor_message_ends_when_the_actor_arrives_and_stays_gone_after_death() {
        let mut app = app();
        let old = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(old).unwrap();
        app.world_mut().get_mut::<Vitality>(entity).unwrap().died_at = Some(0);
        press(&mut app, Action::NewLife);
        assert!(
            app.world()
                .resource::<Reader>()
                .message
                .contains("New life requested")
        );
        npc_framework::prelude::run_ticks(app.world_mut(), 1);
        app.update();
        let successor = app.world().resource::<VillageState>().played;
        assert_ne!(successor, old);
        assert!(app.world().resource::<Reader>().message.is_empty());
        let entity = app.world().resource::<SimIndex>().get(successor).unwrap();
        app.world_mut().get_mut::<Vitality>(entity).unwrap().died_at = Some(1);
        app.world_mut().resource_mut::<Reader>().refresh = 1.0;
        app.update();
        let hud = app
            .world_mut()
            .query_filtered::<&Text, With<HudText>>()
            .single(app.world())
            .unwrap();
        assert!(hud.0.contains("Permanently dead"));
        assert!(!hud.0.contains("New life requested"));
        assert!(!hud.0.contains("New life has arrived"));
        assert!(!hud.0.contains("joining"));
    }
    #[test]
    fn reader_shows_only_a_diary_carried_by_the_current_actor() {
        let mut app = app();
        let u = app.world().resource::<VillageState>().played;
        let holder = app.world().resource::<SimIndex>().get(u).unwrap();
        let (_, book) = spawn_sim(
            app.world_mut(),
            (
                npc_framework::interaction::Item::new(village::catalog::DIARY),
                DroppedDiary {
                    author: SimId(9999),
                    author_name: "A former traveller".into(),
                    life: 1,
                    died_at: 0,
                    sealed: true,
                    pages: vec![village::DiaryPage {
                        tick: 0,
                        title: "The mill".into(),
                        body: "Grain sells here.".into(),
                    }],
                },
            ),
        );
        npc_framework::interaction::items::put_in_inventory(app.world_mut(), holder, book);
        press(&mut app, Action::Books);
        let body = app
            .world_mut()
            .query_filtered::<&Text, With<BookText>>()
            .single(app.world())
            .unwrap();
        assert!(body.0.contains("Grain sells here."));
        let other = app
            .world()
            .resource::<testbed_content::TestbedIds>()
            .id("Ada");
        let other = app.world().resource::<SimIndex>().get(other).unwrap();
        npc_framework::interaction::items::put_in_inventory(app.world_mut(), other, book);
        app.world_mut().resource_mut::<Reader>().refresh = 1.0;
        app.update();
        let body = app
            .world_mut()
            .query_filtered::<&Text, With<BookText>>()
            .single(app.world())
            .unwrap();
        assert!(!body.0.contains("Grain sells here."));
    }
}
