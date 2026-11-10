//! One player-facing workspace over the shared, actor-scoped action adapter.
//! This module owns presentation state only; the host owns menus and world lifecycle.
mod actions;
mod atlas;
mod build_preview;
mod life;
mod model;
#[cfg(test)]
mod research_tests;
#[cfg(test)]
mod tests;

use crate::{
    IslandMode, game_ui as ui,
    player_actions::{self, ControlState, NativeActions},
};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::{
    FocusCause, InputFocus,
    tab_navigation::{NavAction, TabGroup, TabNavigation},
};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, ScrollArea};
use npc_framework::prelude::{SimControl, SimId, SimPos};
use npc_framework::view_helpers::{Selected, ViewSystems, command_ui::DragBox};
use serde_json::Value;
use std::collections::BTreeMap;

/// Opt-in player presentation. The host leaves this enabled while its own menu is open.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PlayerExperience {
    pub enabled: bool,
}
/// Host-owned modal gate. It blocks gameplay input and hides gameplay UI, not the host shell.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FrontendGate(pub bool);
/// Top-level title/setup/pause roots opt out of gameplay-UI hiding.
#[derive(Component)]
pub struct FrontendUiRoot;
/// A request to the host. The view never saves, swaps worlds or changes host pause state.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PlayerMenuRequest(pub bool);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Pouch,
    Craft,
    Build,
    Garden,
    Trade,
    People,
    Explore,
}
impl Tab {
    fn title(self) -> &'static str {
        match self {
            Self::Pouch => "Your pouch",
            Self::Craft => "Gather & craft",
            Self::Build => "Build a home",
            Self::Garden => "Your garden",
            Self::Trade => "Village market",
            Self::People => "Neighbours",
            Self::Explore => "Explore the island",
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Self::Pouch => "Use what you carry, and keep yourself well.",
            Self::Craft => "Walk to a source or workshop. Work uses game time and materials.",
            Self::Build => "Choose a site, gather its materials, and make it yours.",
            Self::Garden => "Plant a real seed, water its bed, then return for the harvest.",
            Self::Trade => "Every exchange needs real goods, coins and a nearby buyer.",
            Self::People => "Meet the people whose work keeps the island alive.",
            Self::Explore => "Follow the coastal chart, or return to a place you have discovered.",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct WorkRequest {
    label: String,
    verb: String,
    target: Option<u32>,
    args: Value,
    approach: Option<SimPos>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    Open(Tab),
    Close,
    Menu,
    Inspect,
    Successor,
    WatchVillage,
    LifeChoices,
    Stop,
    Plot(u32),
    ChoosePlot,
    InspectPlot(u32),
    Bed(u32),
    Crop(String),
    Person(u32),
    ChoosePerson,
    Walk(SimPos),
    Work(WorkRequest),
    Goal,
}
#[derive(Resource)]
pub(super) struct PlayerState {
    tab: Option<Tab>,
    plot: Option<u32>,
    bed: u32,
    crop: String,
    person: Option<u32>,
    requests: Vec<Action>,
    rows: BTreeMap<String, Entity>,
    context: Option<model::Row>,
    stamp: Option<(u32, u64)>,
    elapsed: f32,
    dirty: bool,
    preview_follow: Option<bool>,
    successor_from: Option<SimId>,
    focus_workspace: bool,
}
impl Default for PlayerState {
    fn default() -> Self {
        Self {
            tab: None,
            plot: None,
            bed: 0,
            crop: testbed_content::village::catalog::GRAIN.into(),
            person: None,
            requests: Vec::new(),
            rows: BTreeMap::new(),
            context: None,
            stamp: None,
            elapsed: 1.0,
            dirty: true,
            preview_follow: None,
            successor_from: None,
            focus_workspace: false,
        }
    }
}
#[derive(Component)]
pub(crate) struct PlayerRoot;
#[derive(Component)]
struct HeaderRoot;
#[derive(Component)]
struct ToolbarRoot;
#[derive(Component)]
struct FeedbackRoot;
#[derive(Component)]
pub(crate) struct WorkspaceRoot;
#[derive(Component)]
pub(crate) struct JourneyRoot;
#[derive(Component)]
struct WorkspaceBody;
#[derive(Component)]
struct WorkspaceTitle;
#[derive(Component)]
struct WorkspaceSubtitle;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct CalendarText;
#[derive(Component)]
struct CoinsText;
#[derive(Component)]
struct JourneyTitle;
#[derive(Component)]
struct JourneyDetail;
#[derive(Component)]
struct ContextLabel;
#[derive(Component)]
struct ContextButton;
#[derive(Component)]
struct NeedLabel(&'static str);
#[derive(Component)]
struct NeedFill(&'static str);
#[derive(Component, Clone)]
struct PlayerAction(Action);
#[derive(Component)]
struct PlayerControl;
#[derive(Component)]
struct ToolbarTab(Tab);
#[derive(Component)]
struct RowTitle;
#[derive(Component)]
struct RowDetail;
#[derive(Component)]
struct DeathRoot;
#[derive(Component)]
struct DeathDetail;
#[derive(Component)]
struct SuccessorButton;
#[derive(Component)]
struct WatchButton;
#[derive(Component)]
struct SpectatorLifeRoot;
#[derive(Component)]
struct SpectatorLifeStatus;

pub(crate) fn enabled(world: &World) -> bool {
    world
        .get_resource::<PlayerExperience>()
        .is_some_and(|p| p.enabled)
}
pub(crate) fn active(world: &World) -> bool {
    enabled(world)
        && (world.get_resource::<IslandMode>() == Some(&IslandMode::Character)
            || life::needs_prompt(world))
        && !world
            .get_resource::<FrontendGate>()
            .is_some_and(|gate| gate.0)
}
pub(crate) fn dead(world: &World) -> bool {
    world
        .get_resource::<testbed_content::village::VillageState>()
        .and_then(|state| {
            world
                .get_resource::<npc_framework::prelude::SimIndex>()?
                .get(state.played)
        })
        .and_then(|actor| world.get::<testbed_content::village::Vitality>(actor))
        .is_some_and(|vital| vital.died_at.is_some())
}
pub(crate) fn gate_open(gate: Option<Res<FrontendGate>>) -> bool {
    !gate.is_some_and(|gate| gate.0)
}
/// Explicit spectator intent; this never revives the actor or changes survival rules.
pub(crate) fn watch_village(world: &mut World) -> Result<(), String> {
    life::watch(world)
}
pub(crate) fn plugin(app: &mut App) {
    if !testbed_content::village::colony::enabled(app.world()) {
        return;
    }
    ui::install(app);
    crate::panel_scroll::install(app);
    app.init_resource::<PlayerExperience>()
        .init_resource::<life::LifeView>()
        .init_resource::<FrontendGate>()
        .init_resource::<PlayerMenuRequest>()
        .init_resource::<PlayerState>()
        .init_resource::<InputFocus>()
        .init_resource::<HiddenRoots>()
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, keyboard_navigation, shield)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (actions::process, refresh)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::colony_panel::refresh),
        )
        .add_systems(Update, restore_roots.before(ViewSystems::Sync))
        .add_systems(Update, hide_roots.after(ViewSystems::Present));
    build_preview::install(app);
}
pub(crate) fn clear(world: &mut World) {
    if world.contains_resource::<PlayerState>() {
        actions::restore_follow(world);
        // replace_children detaches existing entities; it does not destroy rows.
        // Tear down the old actor's actions before dropping their identity map.
        let rows: Vec<_> = world
            .query_filtered::<&Children, With<WorkspaceBody>>()
            .iter(world)
            .flat_map(|children| children.iter())
            .collect();
        for entity in rows {
            world.despawn(entity);
        }
        *world.resource_mut::<PlayerState>() = PlayerState::default();
    }
    if let Some(mut menu) = world.get_resource_mut::<PlayerMenuRequest>() {
        menu.0 = false;
    }
    if let Some(mut input) = world.get_resource_mut::<InputFocus>() {
        input.clear();
    }
    crate::homestead_panel::set_open(world, false);
}
fn root(world: &mut World, name: &'static str, node: Node) -> Entity {
    world
        .spawn((
            Name::new(name),
            PlayerRoot,
            Hovered::default(),
            GlobalZIndex(27),
            TabGroup::new(7),
            ui::panel(node),
        ))
        .id()
}
fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id()
}
fn button(world: &mut World, parent: Entity, label: &str, action: Action, primary: bool) -> Entity {
    ui::button(
        world,
        parent,
        label,
        0,
        primary,
        (PlayerControl, PlayerAction(action)),
    )
}
fn spawn(world: &mut World) {
    let hud = root(
        world,
        "Player status",
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            right: px(12),
            min_height: px(62),
            padding: UiRect::axes(px(14), px(9)),
            column_gap: px(16),
            row_gap: px(6),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(hud).insert(HeaderRoot);
    world.spawn((
        ChildOf(hud),
        CalendarText,
        ui::text("Island Village", 17., ui::GOLD),
        Node {
            min_width: px(165),
            ..default()
        },
    ));
    for (key, title) in [
        ("health", "Health"),
        ("hunger", "Food"),
        ("thirst", "Water"),
        ("energy", "Energy"),
        ("warmth", "Warmth"),
    ] {
        let metric = world
            .spawn((
                ChildOf(hud),
                Node {
                    width: px(86),
                    row_gap: px(5),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ))
            .id();
        world.spawn((
            ChildOf(metric),
            NeedLabel(key),
            ui::text(title, 13., ui::TEXT),
        ));
        let track = world
            .spawn((
                ChildOf(metric),
                Node {
                    height: px(5),
                    width: percent(100),
                    ..default()
                },
                BackgroundColor(ui::FIELD),
            ))
            .id();
        world.spawn((
            ChildOf(track),
            NeedFill(key),
            Node {
                height: percent(100),
                width: percent(100),
                ..default()
            },
            BackgroundColor(ui::SUCCESS),
        ));
    }
    world.spawn((
        ChildOf(hud),
        CoinsText,
        ui::text("0 coins", 17., ui::GOLD),
        Node {
            flex_grow: 1.0,
            min_width: px(80),
            ..default()
        },
    ));
    button(world, hud, "Menu · Esc", Action::Menu, false);
    let toolbar = root(
        world,
        "Player toolbar",
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            right: px(12),
            bottom: px(12),
            padding: UiRect::all(px(5)),
            justify_content: JustifyContent::Center,
            column_gap: px(6),
            row_gap: px(5),
            flex_wrap: FlexWrap::Wrap,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(toolbar).insert(ToolbarRoot);
    for (tab, label) in [
        (Tab::Pouch, "Pouch · I"),
        (Tab::Craft, "Craft · C"),
        (Tab::Build, "Build · B"),
        (Tab::Garden, "Garden · G"),
        (Tab::Trade, "Trade · T"),
        (Tab::People, "People · N"),
        (Tab::Explore, "Explore · M"),
    ] {
        let entity = button(world, toolbar, label, Action::Open(tab), false);
        world.entity_mut(entity).insert(ToolbarTab(tab));
    }
    button(world, toolbar, "Stop · X", Action::Stop, false);
    let journal = root(
        world,
        "First days guidance",
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(90),
            width: px(270),
            max_width: percent(32),
            padding: UiRect::all(px(14)),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(journal).insert(JourneyRoot);
    world.spawn((
        ChildOf(journal),
        JourneyTitle,
        ui::text("Your first days", 20., ui::GOLD),
    ));
    world.spawn((ChildOf(journal), JourneyDetail, ui::text("", 14., ui::TEXT)));
    button(world, journal, "Take the next step", Action::Goal, true);
    let feedback = root(
        world,
        "Player context",
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            bottom: px(74),
            width: percent(45),
            max_width: px(540),
            padding: UiRect::all(px(9)),
            row_gap: px(5),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(feedback).insert(FeedbackRoot);
    let context = button(world, feedback, "E · Look around", Action::Goal, false);
    world.entity_mut(context).insert(ContextButton);
    for child in world
        .get::<Children>(context)
        .map(|c| c.to_vec())
        .unwrap_or_default()
    {
        world.entity_mut(child).insert(ContextLabel);
    }
    world.spawn((
        ChildOf(feedback),
        StatusText,
        ui::text(
            "WASD move · E interact · Q/R orbit · wheel zoom",
            13.,
            ui::MUTED,
        ),
    ));
    let workspace = root(
        world,
        "Player workspace",
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            top: px(90),
            width: px(430),
            max_width: percent(55),
            bottom: px(230),
            max_height: px(850),
            padding: UiRect::all(px(14)),
            row_gap: px(10),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(workspace).insert(WorkspaceRoot);
    let heading = row(world, workspace);
    world.spawn((
        ChildOf(heading),
        WorkspaceTitle,
        ui::text("Your pouch", 23., ui::GOLD),
        Node {
            flex_grow: 1.,
            ..default()
        },
    ));
    button(world, heading, "Close", Action::Close, false);
    world.spawn((
        ChildOf(workspace),
        WorkspaceSubtitle,
        ui::text("", 14., ui::MUTED),
    ));
    let body = world
        .spawn((
            ChildOf(workspace),
            WorkspaceBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_grow: 1.,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    for id in crate::game_ui::attach_panel_scroll_when_needed(
        world,
        workspace,
        body,
        "Player",
        ["More above", "More below"],
    ) {
        world.entity_mut(id).insert(PlayerControl);
    }
    let death = root(
        world,
        "A life remembered",
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(death).insert((
        DeathRoot,
        GlobalZIndex(36),
        BackgroundColor(Color::srgba(0.025, 0.035, 0.045, 0.82)),
    ));
    let card = world
        .spawn((
            ChildOf(death),
            ui::panel(Node {
                width: px(500),
                max_width: percent(88),
                padding: UiRect::all(px(26)),
                row_gap: px(16),
                flex_direction: FlexDirection::Column,
                ..default()
            }),
        ))
        .id();
    world.spawn((
        ChildOf(card),
        ui::text("This life has ended", 30., ui::GOLD),
    ));
    world.spawn((ChildOf(card),DeathDetail,ui::text("Your home, possessions and diary remain in the living village. A successor begins a new mortal life; the person you lost stays part of its history.",17.,ui::TEXT)));
    let successor = button(
        world,
        card,
        "Begin a successor life",
        Action::Successor,
        true,
    );
    world.entity_mut(successor).insert(SuccessorButton);
    let watch = button(
        world,
        card,
        "Keep watching the village",
        Action::WatchVillage,
        false,
    );
    world.entity_mut(watch).insert(WatchButton);
    button(world, card, "Return to the menu", Action::Menu, false);
    life::spawn(world);
}
fn activate(
    event: On<Activate>,
    buttons: Query<(&PlayerAction, Has<InteractionDisabled>)>,
    mut state: ResMut<PlayerState>,
) {
    if let Ok((action, false)) = buttons.get(event.entity) {
        state.requests.push(action.0.clone());
        state.dirty = true;
    }
}
fn keys(world: &mut World) {
    // Notes and other panels retain both text and non-text shortcut handling.
    if world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| {
            world.get::<bevy::text::EditableText>(entity).is_some()
                || (world.get::<PlayerControl>(entity).is_none()
                    && world.get::<Window>(entity).is_none())
        })
    {
        return;
    }
    if life::watching(world) {
        let requested = world
            .get_resource::<ButtonInput<KeyCode>>()
            .is_some_and(|keys| {
                let modifiers = [
                    KeyCode::ControlLeft,
                    KeyCode::ControlRight,
                    KeyCode::AltLeft,
                    KeyCode::AltRight,
                    KeyCode::SuperLeft,
                    KeyCode::SuperRight,
                ];
                keys.just_pressed(KeyCode::KeyL)
                    && !keys.any_pressed(modifiers)
                    && !keys.any_just_pressed(modifiers)
            });
        if requested {
            world
                .resource_mut::<PlayerState>()
                .requests
                .push(Action::LifeChoices);
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset(KeyCode::KeyL);
        }
        return;
    }
    if !active(world) {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if dead(world) {
        if keys.just_pressed(KeyCode::Escape) {
            world
                .resource_mut::<PlayerState>()
                .requests
                .push(Action::Menu);
            world
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset(KeyCode::Escape);
        }
        return;
    }
    let modifiers = [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ];
    if keys.any_pressed(modifiers) || keys.any_just_pressed(modifiers) {
        return;
    }
    let mut actions = Vec::new();
    let mut consumed = Vec::new();
    for (key, tab) in [
        (KeyCode::KeyI, Tab::Pouch),
        (KeyCode::KeyC, Tab::Craft),
        (KeyCode::KeyB, Tab::Build),
        (KeyCode::KeyG, Tab::Garden),
        (KeyCode::KeyT, Tab::Trade),
        (KeyCode::KeyN, Tab::People),
        (KeyCode::KeyM, Tab::Explore),
    ] {
        if keys.just_pressed(key) {
            actions.push(if world.resource::<PlayerState>().tab == Some(tab) {
                Action::Close
            } else {
                Action::Open(tab)
            });
            consumed.push(key);
        }
    }
    if keys.just_pressed(KeyCode::F2) {
        actions.push(if world.resource::<PlayerState>().tab.is_some() {
            Action::Close
        } else {
            Action::Open(Tab::Pouch)
        });
        consumed.push(KeyCode::F2);
    }
    if keys.just_pressed(KeyCode::Escape) {
        actions.push(if world.resource::<PlayerState>().tab.is_some() {
            Action::Close
        } else {
            Action::Menu
        });
        consumed.push(KeyCode::Escape);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        actions.push(Action::Stop);
        consumed.push(KeyCode::KeyX);
    }
    if keys.just_pressed(KeyCode::F11) {
        actions.push(Action::Inspect);
        consumed.push(KeyCode::F11);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        if let Some(context) = world
            .resource::<PlayerState>()
            .context
            .as_ref()
            .filter(|row| row.enabled)
            .and_then(|row| row.action.clone())
        {
            actions.push(context);
        }
        consumed.push(KeyCode::KeyE);
    }
    if keys.just_pressed(KeyCode::Tab) && world.resource::<PlayerState>().tab.is_none() {
        actions.push(Action::Open(Tab::Pouch));
        consumed.push(KeyCode::Tab);
    }
    world.resource_mut::<PlayerState>().requests.extend(actions);
    for key in consumed {
        world.resource_mut::<ButtonInput<KeyCode>>().reset(key);
    }
}
fn input_active(
    focus: Option<Res<InputFocus>>,
    controls: Query<(), With<PlayerControl>>,
    death: Query<&Node, With<DeathRoot>>,
) -> bool {
    death.iter().any(|node| node.display != Display::None)
        || focus
            .and_then(|focus| focus.get())
            .is_some_and(|entity| controls.contains(entity))
}
#[allow(clippy::too_many_arguments)]
fn keyboard_navigation(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut focus: ResMut<InputFocus>,
    nav: TabNavigation,
    mut commands: Commands,
    roots: Query<Entity, With<WorkspaceRoot>>,
    deaths: Query<(Entity, &Node), With<DeathRoot>>,
    controls: Query<(), With<PlayerControl>>,
    windows: Query<(), With<Window>>,
    state: Res<PlayerState>,
    experience: Res<PlayerExperience>,
    mode: Res<IslandMode>,
) {
    let death = deaths
        .iter()
        .find(|(_, node)| node.display != Display::None)
        .map(|(entity, _)| entity);
    if !experience.enabled
        || death.is_none() && (*mode != IslandMode::Character || state.tab.is_none())
    {
        return;
    }
    if focus
        .get()
        .is_some_and(|entity| !controls.contains(entity) && !windows.contains(entity))
    {
        return;
    }
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::Tab) {
        let direction = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        let next = if focus.get().is_some_and(|entity| controls.contains(entity)) {
            nav.navigate(&focus, direction)
        } else if let Some(root) = death.or_else(|| roots.single().ok()) {
            nav.initialize(root, direction)
        } else {
            return;
        };
        if let Ok(entity) = next {
            focus.set(entity, FocusCause::Navigated);
            commands.trigger(bevy::ui_widgets::ScrollIntoView { entity });
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn shield(
    roots: Query<(&Hovered, &Node, Has<DeathRoot>, Has<SpectatorLifeRoot>), With<PlayerRoot>>,
    controls: Query<(), With<PlayerControl>>,
    mut mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    mut scroll: Option<ResMut<AccumulatedMouseScroll>>,
    mut drag: Option<ResMut<DragBox>>,
    mut focus: ResMut<InputFocus>,
    experience: Res<PlayerExperience>,
    gate: Res<FrontendGate>,
    mode: Res<IslandMode>,
) {
    if !experience.enabled
        || gate.0
        || *mode != IslandMode::Character
            && !roots.iter().any(|(_, node, death, spectator)| {
                (death || spectator) && node.display != Display::None
            })
    {
        return;
    }
    if roots
        .iter()
        .any(|(hovered, node, _, _)| node.display != Display::None && hovered.get())
    {
        if let Some(mouse) = mouse.as_mut() {
            mouse.reset_all();
        }
        if let Some(scroll) = scroll.as_mut() {
            scroll.delta = Vec2::ZERO;
        }
        if let Some(drag) = drag.as_mut() {
            drag.start = None;
        }
    } else if mouse
        .as_ref()
        .is_some_and(|m| m.just_pressed(MouseButton::Left) || m.just_pressed(MouseButton::Right))
        && focus.get().is_some_and(|e| controls.contains(e))
    {
        focus.clear();
    }
}
fn text<M: Component>(world: &mut World, value: String) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0.clone_from(&value);
        }
    }
}
fn disabled(world: &mut World, entity: Entity, value: bool) {
    if value {
        world.entity_mut(entity).insert(InteractionDisabled);
    } else {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    }
}
pub(crate) fn refresh(world: &mut World) {
    adapt_layout(world);
    let active = active(world);
    let dead = dead(world);
    let open = world.resource::<PlayerState>().tab.is_some();
    for (mut node, workspace, death, spectator) in world
        .query_filtered::<(
            &mut Node,
            Has<WorkspaceRoot>,
            Has<DeathRoot>,
            Has<SpectatorLifeRoot>,
        ), With<PlayerRoot>>()
        .iter_mut(world)
    {
        node.display = if active
            && !spectator
            && if dead {
                death
            } else {
                !death && (!workspace || open)
            } {
            Display::Flex
        } else {
            Display::None
        };
    }
    life::refresh(world);
    if !active {
        return;
    }
    for mut node in world
        .query_filtered::<&mut Node, With<crate::village::Hud>>()
        .iter_mut(world)
    {
        node.display = Display::None;
    }
    for mut node in world
        .query_filtered::<&mut Node, Or<(
            With<crate::homestead_panel::FarmRoot>,
            With<crate::homestead_panel::FarmLauncher>,
            With<crate::colony_panel::LedgerRoot>,
            With<crate::colony_panel::LedgerLauncher>,
        )>>()
        .iter_mut(world)
    {
        node.display = Display::None;
    }
    let delta = world.get_resource::<Time>().map_or(0., Time::delta_secs);
    {
        let mut state = world.resource_mut::<PlayerState>();
        state.elapsed += delta;
        if !state.dirty && state.elapsed < 0.2 {
            return;
        }
        state.dirty = false;
        state.elapsed = 0.;
    }
    let Ok(observed) = player_actions::observe(world) else {
        return;
    };
    let stamp = (
        model::id(&observed["unit"]).unwrap_or(0),
        npc_framework::agent_io::api::epoch(world),
    );
    if world
        .resource::<PlayerState>()
        .stamp
        .is_some_and(|old| old != stamp)
    {
        clear(world);
    }
    world.resource_mut::<PlayerState>().stamp = Some(stamp);
    let selected_tab = world.resource::<PlayerState>().tab;
    for (tab, mut button) in world
        .query::<(&ToolbarTab, &mut ui::GameButton)>()
        .iter_mut(world)
    {
        button.primary = selected_tab == Some(tab.0);
    }
    let village = &observed["sections"]["village"];
    text::<CalendarText>(
        world,
        format!(
            "Day {} · {}\nPlay · {} · {}",
            village["calendar"]["day"],
            model::friendly(village["calendar"]["season"].as_str().unwrap_or("island")),
            if world.resource::<SimControl>().is_stopped() {
                "Paused".into()
            } else {
                format!("{}×", world.resource::<SimControl>().multiplier)
            },
            if village["calendar"]["night"] == true {
                "Night"
            } else {
                "Daylight"
            }
        ),
    );
    text::<CoinsText>(
        world,
        format!("{} coins", village["coins"].as_u64().unwrap_or(0)),
    );
    let values: BTreeMap<_, _> = [
        ("health", village["health"].as_u64().unwrap_or(0)),
        ("hunger", village["needs"]["hunger"].as_u64().unwrap_or(0)),
        ("thirst", village["needs"]["thirst"].as_u64().unwrap_or(0)),
        ("energy", village["needs"]["energy"].as_u64().unwrap_or(0)),
        ("warmth", village["needs"]["warmth"].as_u64().unwrap_or(0)),
    ]
    .into_iter()
    .collect();
    for (label, mut text) in world.query::<(&NeedLabel, &mut Text)>().iter_mut(world) {
        let title = match label.0 {
            "hunger" => "Food",
            "thirst" => "Water",
            "energy" => "Energy",
            "warmth" => "Warmth",
            _ => "Health",
        };
        text.0 = format!("{title} {}%", values[label.0] / 1000);
    }
    for (fill, mut node, mut color) in world
        .query::<(&NeedFill, &mut Node, &mut BackgroundColor)>()
        .iter_mut(world)
    {
        let value = values[fill.0].min(100000);
        node.width = percent(value as f32 / 1000.);
        color.0 = if value < 20000 {
            ui::ERROR
        } else if value < 45000 {
            ui::GOLD
        } else {
            ui::SUCCESS
        };
    }
    let stopped = world.resource::<SimControl>().is_stopped();
    let busy =
        village["activity"]["busy"] == true || world.resource::<NativeActions>().pending.is_some();
    let ready = player_actions::control(world) == ControlState::Native
        && !stopped
        && !busy
        && !crate::island_control::control_pending(world);
    let selected = world.get_resource::<Selected>().and_then(|s| s.0);
    let context = model::context(world, &observed, selected, ready);
    let context_text = context.as_ref().map_or(
        "Move near a resource, plot or neighbour · E interact".into(),
        |row| row.title.clone(),
    );
    text::<ContextLabel>(world, context_text);
    let context_buttons: Vec<_> = world
        .query_filtered::<Entity, With<ContextButton>>()
        .iter(world)
        .collect();
    for button in context_buttons {
        disabled(
            world,
            button,
            context.as_ref().is_none_or(|row| !row.enabled),
        );
        if let Some(action) = context.as_ref().and_then(|row| row.action.clone()) {
            world.entity_mut(button).insert(PlayerAction(action));
        }
    }
    world.resource_mut::<PlayerState>().context = context;
    let feedback = world
        .resource::<NativeActions>()
        .pending
        .as_ref()
        .map(|p| format!("Walking, then {}", p.label))
        .or_else(|| world.resource::<NativeActions>().feedback.clone())
        .unwrap_or_else(|| "WASD move · E interact · Q/R orbit · wheel zoom".into());
    text::<StatusText>(
        world,
        if stopped {
            "World paused. Open Menu to resume.".into()
        } else {
            feedback
        },
    );
    let (goal, detail) = actions::guidance(&observed);
    text::<JourneyTitle>(world, goal);
    text::<JourneyDetail>(world, detail);
    if let Some(tab) = world.resource::<PlayerState>().tab {
        text::<WorkspaceTitle>(world, tab.title().into());
        text::<WorkspaceSubtitle>(world, tab.subtitle().into());
    }
    let rows = if world.resource::<PlayerState>().tab.is_some() {
        model::rows(world, &observed, world.resource::<PlayerState>(), ready)
    } else {
        Vec::new()
    };
    reconcile(world, rows);
    atlas::refresh(world, &observed, ready);
}
/// Keep the workspace between the actual header and feedback heights. This also
/// handles toolbar/header wrapping without relying on a single window resolution.
fn adapt_layout(world: &mut World) {
    let mut header = 62.0_f32;
    let mut toolbar = 54.0_f32;
    let mut feedback = 140.0_f32;
    for (node, size, is_header, is_toolbar, is_feedback) in world
        .query::<(
            &Node,
            &ComputedNode,
            Has<HeaderRoot>,
            Has<ToolbarRoot>,
            Has<FeedbackRoot>,
        )>()
        .iter(world)
    {
        let height = size.size().y * size.inverse_scale_factor();
        if node.display == Display::None || height <= 0. {
            continue;
        }
        if is_header {
            header = height;
        }
        if is_toolbar {
            toolbar = height;
        }
        if is_feedback {
            feedback = height;
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<FeedbackRoot>>()
        .iter_mut(world)
    {
        node.bottom = px(24.0 + toolbar);
    }
    for (mut node,is_workspace) in world.query_filtered::<(&mut Node,Has<WorkspaceRoot>),Or<(With<WorkspaceRoot>,With<JourneyRoot>)>>().iter_mut(world) {
        node.top=px(24.0+header);
        if is_workspace {node.bottom=px(36.0+toolbar+feedback);}
    }
}
fn reconcile(world: &mut World, rows: Vec<model::Row>) {
    let Some(body) = world
        .query_filtered::<Entity, With<WorkspaceBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old = world.resource::<PlayerState>().rows.clone();
    let mut live = BTreeMap::new();
    let mut order = Vec::new();
    if let Some(mut node) = world.get_mut::<Node>(body) {
        // Full-width rows retain the vertical workspace layout; compact controls
        // share a row so direction/crop/bed choices do not become tall card stacks.
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.align_content = AlignContent::FlexStart;
        node.column_gap = px(8);
    }
    for (index, row) in rows.into_iter().enumerate() {
        let entity = if let Some(entity) = old.get(&row.key).copied() {
            entity
        } else {
            let entity = if let Some(action) = row.action.clone() {
                button(world, body, &row.title, action, false)
            } else {
                world
                    .spawn((
                        ChildOf(body),
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(4),
                            flex_shrink: 0.,
                            ..default()
                        },
                    ))
                    .id()
            };
            let previous = world
                .get::<Children>(entity)
                .map(|c| c.to_vec())
                .unwrap_or_default();
            for child in previous {
                world.despawn(child);
            }
            world.spawn((
                ChildOf(entity),
                RowTitle,
                ui::text(
                    &row.title,
                    if row.heading { 19. } else { 16. },
                    if row.heading { ui::GOLD } else { ui::TEXT },
                ),
            ));
            world.spawn((
                ChildOf(entity),
                RowDetail,
                ui::text(&row.detail, 13., ui::MUTED),
            ));
            entity
        };
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.flex_direction = FlexDirection::Column;
            node.align_items = AlignItems::Stretch;
            node.row_gap = px(5);
            node.width = if row.item {
                px(115)
            } else if row.compact {
                percent(48)
            } else {
                percent(100)
            };
            node.min_height = if row.item {
                px(86)
            } else if row.action.is_some() {
                px(44)
            } else {
                px(0)
            };
            node.padding = if row.action.is_some() {
                UiRect::all(px(10))
            } else {
                UiRect::axes(px(2), px(5))
            };
        }
        if let Some(action) = row.action {
            world.entity_mut(entity).insert((
                PlayerAction(action),
                bevy::ui::prelude::AccessibleLabel::new(if row.detail.is_empty() {
                    row.title.clone()
                } else {
                    format!("{}. {}", row.title, row.detail)
                }),
            ));
            disabled(world, entity, !row.enabled);
            world
                .entity_mut(entity)
                .insert(bevy::input_focus::tab_navigation::TabIndex(
                    100 + index as i32,
                ));
        }
        for child in world
            .get::<Children>(entity)
            .map(|c| c.to_vec())
            .unwrap_or_default()
        {
            let title = world.get::<RowTitle>(child).is_some();
            if let Some(mut text) = world.get_mut::<Text>(child) {
                text.0 = if title {
                    row.title.clone()
                } else {
                    row.detail.clone()
                };
            }
        }
        if let Some(mut button) = world.get_mut::<ui::GameButton>(entity) {
            button.primary = row.selected;
        }
        live.insert(row.key, entity);
        order.push(entity);
    }
    for (key, entity) in old {
        if !live.contains_key(&key) {
            if world.resource::<InputFocus>().get() == Some(entity) {
                world.resource_mut::<InputFocus>().clear();
            }
            world.despawn(entity);
        }
    }
    world.entity_mut(body).replace_children(&order);
    if std::mem::take(&mut world.resource_mut::<PlayerState>().focus_workspace) {
        let preferred = [
            "build_home",
            "claim_site",
            "plant",
            "water",
            "harvest",
            "greet",
            "open_garden",
        ]
        .into_iter()
        .filter_map(|key| live.get(key).copied())
        .find(|entity| world.get::<InteractionDisabled>(*entity).is_none());
        let bed = format!("bed_{}", world.resource::<PlayerState>().bed);
        let preferred = preferred.or_else(|| live.get(&bed).copied());
        let next = preferred.or_else(|| {
            order.iter().copied().find(|entity| {
                world.get::<PlayerAction>(*entity).is_some()
                    && world.get::<InteractionDisabled>(*entity).is_none()
            })
        });
        if let Some(entity) = next {
            world
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
            world.trigger(bevy::ui_widgets::ScrollIntoView { entity });
        }
    }
    world.resource_mut::<PlayerState>().rows = live;
}

#[derive(Resource, Default)]
struct HiddenRoots(BTreeMap<Entity, Display>);
fn restore_roots(world: &mut World) {
    if world
        .get_resource::<FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let roots = std::mem::take(&mut world.resource_mut::<HiddenRoots>().0);
    for (entity, display) in roots {
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.display = display;
        }
    }
}
fn hide_roots(world: &mut World) {
    if !world
        .get_resource::<FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<(Entity, &Node), (Without<ChildOf>, Without<FrontendUiRoot>)>()
        .iter(world)
        .map(|(entity, node)| (entity, node.display))
        .collect();
    for (entity, display) in roots {
        world
            .resource_mut::<HiddenRoots>()
            .0
            .entry(entity)
            .or_insert(display);
        world.get_mut::<Node>(entity).expect("root exists").display = Display::None;
    }
}
