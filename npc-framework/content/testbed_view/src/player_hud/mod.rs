//! One player-facing workspace over the shared, actor-scoped action adapter.
//! This module owns presentation state only; the host owns menus and world lifecycle.
mod actions;
mod atlas;
mod build_preview;
mod depth;
mod hud;
mod life;
mod model;
#[cfg(test)]
mod research_tests;
mod rest;
mod session;
#[cfg(test)]
mod tests;
mod workspace;

#[cfg(test)]
use workspace::reconcile;

pub(crate) use session::{leave_character_view, show_life_choices, toolbar_clearance};

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
/// Host opt-in (`--dev-keys`) that keeps the developer toggles (F1, F3-F8)
/// reachable inside the player session. Developer launches ignore it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DeveloperKeys(pub bool);
/// Developer toggles work everywhere except an unflagged player session.
pub(crate) fn developer_keys_allowed(
    experience: Option<&PlayerExperience>,
    developer: Option<&DeveloperKeys>,
) -> bool {
    !experience.is_some_and(|experience| experience.enabled)
        || developer.is_some_and(|developer| developer.0)
}
/// Run condition form of [`developer_keys_allowed`].
pub(crate) fn developer_keys(
    experience: Option<Res<PlayerExperience>>,
    developer: Option<Res<DeveloperKeys>>,
) -> bool {
    developer_keys_allowed(experience.as_deref(), developer.as_deref())
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
    /// In hotbar order.
    pub(super) const ALL: [Self; 7] = [
        Self::Pouch,
        Self::Craft,
        Self::Build,
        Self::Garden,
        Self::Trade,
        Self::People,
        Self::Explore,
    ];
    /// The word on the hotbar tile.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Pouch => "Pouch",
            Self::Craft => "Craft",
            Self::Build => "Build",
            Self::Garden => "Garden",
            Self::Trade => "Trade",
            Self::People => "People",
            Self::Explore => "Explore",
        }
    }
    /// The key that opens it, as its key cap shows it.
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Pouch => "I",
            Self::Craft => "C",
            Self::Build => "B",
            Self::Garden => "G",
            Self::Trade => "T",
            Self::People => "N",
            Self::Explore => "M",
        }
    }
    pub(super) fn icon(self) -> crate::game_ui::icon::Icon {
        use crate::game_ui::icon::Icon;
        match self {
            Self::Pouch => Icon::Pouch,
            Self::Craft => Icon::Craft,
            Self::Build => Icon::Build,
            Self::Garden => Icon::Garden,
            Self::Trade => Icon::Trade,
            Self::People => Icon::People,
            Self::Explore => Icon::Explore,
        }
    }
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
    /// A hotbar tile: open its workspace, or close it when it is the open one.
    Toggle(Tab),
    Close,
    /// Choose an item of the pouch; choosing it again lets go.
    Item(String),
    /// Craft: list every recipe (`true`) or only what can be made now.
    CraftAll(bool),
    /// Fold the goal tracker to its title, or open it again.
    GoalFold,
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
    /// A recorded live-mode command: sleep, wake, gift, repeat, upgrade.
    Live(depth::LiveRequest),
    /// Choose a gift for this resident, or put the picker away.
    GiftFor(Option<u32>),
    /// Open the pouch at what helps this need (`None` shows everything).
    NeedHelp(Option<&'static str>),
    /// How many times a gathering is repeated.
    Repeat(u32),
    /// Put the day's account away.
    DismissSummary,
}
#[derive(Resource)]
pub(super) struct PlayerState {
    tab: Option<Tab>,
    plot: Option<u32>,
    bed: u32,
    crop: String,
    person: Option<u32>,
    /// The item chosen in the pouch.
    item: Option<String>,
    /// Craft lists every recipe, not only what can be made now.
    craft_all: bool,
    goal_folded: bool,
    requests: Vec<Action>,
    rows: BTreeMap<String, Entity>,
    context: Option<model::Row>,
    stamp: Option<(u32, u64)>,
    elapsed: f32,
    dirty: bool,
    preview_follow: Option<bool>,
    successor_from: Option<SimId>,
    focus_workspace: bool,
    /// The resident a gift is being chosen for.
    gift_for: Option<u32>,
    /// The need the pouch is filtered to, from a click on that need.
    need_filter: Option<&'static str>,
    /// Gatherings per order: 1, 5 or 10.
    repeat: u32,
}
impl Default for PlayerState {
    fn default() -> Self {
        Self {
            tab: None,
            plot: None,
            bed: 0,
            crop: testbed_content::village::catalog::GRAIN.into(),
            person: None,
            item: None,
            craft_all: true,
            goal_folded: false,
            requests: Vec::new(),
            rows: BTreeMap::new(),
            context: None,
            stamp: None,
            elapsed: 1.0,
            dirty: true,
            preview_follow: None,
            successor_from: None,
            focus_workspace: false,
            gift_for: None,
            need_filter: None,
            repeat: 1,
        }
    }
}
#[derive(Component)]
pub(crate) struct PlayerRoot;
/// The hotbar: the alert stack and the camera keep clear of it.
#[derive(Component)]
struct ToolbarRoot;
#[derive(Component)]
pub(crate) struct WorkspaceRoot;
/// The goal tracker.
#[derive(Component)]
pub(crate) struct JourneyRoot;
#[derive(Component)]
struct WorkspaceBody;
#[derive(Component, Clone)]
struct PlayerAction(Action);
#[derive(Component)]
struct PlayerControl;
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
/// Whether Tab belongs to the character workspace or a life prompt right now.
pub(crate) fn owns_tab(world: &World) -> bool {
    world
        .get_resource::<PlayerState>()
        .is_some_and(|state| state.tab.is_some())
        || life::needs_prompt(world)
}
/// Where the goal tracker ends, with its margin: chrome beside it starts here.
pub(crate) const GOAL_EDGE: f32 = 316.0;
/// Width of the open Live workspace, for chrome that must keep clear of it;
/// zero while none is open.
pub(crate) fn workspace_width(world: &World) -> f32 {
    if !active(world) || dead(world) {
        return 0.0;
    }
    world
        .get_resource::<PlayerState>()
        .and_then(|state| state.tab)
        .map_or(0.0, |tab| {
            workspace::width(tab, crate::game_ui::scale::layout(world).class).0
        })
}
/// A click on your own cottage in Live opens it: the Build workspace with
/// its tiers, upgrade and chest, and the cut-away home (the homes view opens
/// the selected building). Closing the workspace lets the cottage go again.
fn home_click(world: &mut World) {
    if !active(world) || dead(world) {
        return;
    }
    let Some(home) = depth::played(world)
        .and_then(|(unit, _)| testbed_content::village::colony::home_of(world, unit))
    else {
        return;
    };
    let selected = world
        .get_resource::<Selected>()
        .and_then(|selected| selected.0);
    if selected == Some(home) && world.resource::<PlayerState>().tab != Some(Tab::Build) {
        world.resource_mut::<Selected>().0 = None;
    }
    let clicked = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| mouse.just_pressed(MouseButton::Left));
    if !clicked || crate::island_build::building_at_cursor(world) != Some(home) {
        return;
    }
    if let Some(mut chosen) = world.get_resource_mut::<Selected>() {
        chosen.0 = Some(home);
    }
    if world.resource::<PlayerState>().tab != Some(Tab::Build) {
        world
            .resource_mut::<PlayerState>()
            .requests
            .push(Action::Open(Tab::Build));
    }
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
/// Whether the played character is asleep, by the simulation's account.
pub(crate) fn player_asleep(world: &World) -> bool {
    rest::asleep(world)
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
        .init_resource::<hud::Hud>()
        .init_resource::<workspace::Frame>()
        .init_resource::<InputFocus>()
        .init_resource::<HiddenRoots>()
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, keyboard_navigation, shield, home_click)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .after(crate::island_shell::ShellInput)
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (actions::process, refresh)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::colony_panel::refresh),
        )
        // After the kit has painted its controls: the need row is a control,
        // and the paint would put its own outline back over the pulse.
        .add_systems(
            Update,
            hud::pulse
                .after(ViewSystems::Present)
                .after(crate::game_ui::widgets::paint),
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
/// A top-level piece of the Live HUD: `skin` is the kit's `chrome` or
/// `panel` bundle for its node.
fn root(world: &mut World, name: &'static str, skin: impl Bundle) -> Entity {
    world
        .spawn((
            Name::new(name),
            PlayerRoot,
            GlobalZIndex(crate::game_ui::tokens::layer::CHROME),
            TabGroup::new(7),
            skin,
        ))
        .id()
}
/// A kit button of the HUD's own panels (the life choices, the death modal).
fn button(world: &mut World, parent: Entity, label: &str, action: Action, primary: bool) -> Entity {
    use crate::game_ui::tokens::Surface;
    use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach};
    w::button(
        world,
        parent,
        ButtonSpec::new(
            label,
            if primary {
                Kind::Primary
            } else {
                Kind::Secondary
            },
            Surface::Paper,
        )
        .reach(Reach::Tab(0)),
        (PlayerControl, PlayerAction(action)),
    )
}
fn spawn(world: &mut World) {
    use crate::game_ui::tokens as t;
    use crate::game_ui::widgets as w;
    hud::spawn(world);
    workspace::spawn(world);
    rest::spawn(world);
    // The end of a life: a modal over a scrim, in the kit's own style.
    let death = world
        .spawn((
            Name::new("A life remembered"),
            PlayerRoot,
            DeathRoot,
            Hovered::default(),
            TabGroup::new(7),
            GlobalZIndex(t::layer::SCRIM),
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
            BackgroundColor(t::SCRIM),
        ))
        .id();
    let card = w::modal_panel(world, death, "This life has ended", "");
    let detail = w::paragraph(
        world,
        card,
        "",
        t::BODY,
        w::Tone::secondary(t::Surface::Paper),
    );
    world.entity_mut(detail).insert(DeathDetail);
    let choices = world
        .spawn((
            ChildOf(card),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                margin: UiRect::top(px(6)),
                ..default()
            },
        ))
        .id();
    let successor = button(
        world,
        choices,
        "Begin a successor life",
        Action::Successor,
        true,
    );
    world.entity_mut(successor).insert(SuccessorButton);
    let watch = button(
        world,
        choices,
        "Keep watching the village",
        Action::WatchVillage,
        false,
    );
    world.entity_mut(watch).insert(WatchButton);
    button(world, choices, "Return to the menu", Action::Menu, false);
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
            // A focused hotbar tile or need is still the HUD's own: its
            // keys keep working. Another panel's control keeps the keyboard.
            world.get::<bevy::text::EditableText>(entity).is_some()
                || (world.get::<PlayerControl>(entity).is_none()
                    && world.get::<crate::game_ui::KeyFocus>(entity).is_none()
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
    // The day's account takes Enter and Esc; nothing else reaches the world under it.
    if rest::summary_open(world) {
        let close = [KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter]
            .into_iter()
            .filter(|key| keys.just_pressed(*key))
            .collect::<Vec<_>>();
        if !close.is_empty() {
            world
                .resource_mut::<PlayerState>()
                .requests
                .push(Action::DismissSummary);
            for key in close {
                world.resource_mut::<ButtonInput<KeyCode>>().reset(key);
            }
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
    hud::place(world);
    let active = active(world);
    let dead = dead(world);
    let open = world.resource::<PlayerState>().tab;
    // The HUD's own roots: the chrome while a living character is played,
    // the workspace while it is open, the modal for a life that has ended.
    for (mut node, workspace, death, spectator) in world
        .query_filtered::<(
            &mut Node,
            Has<WorkspaceRoot>,
            Has<DeathRoot>,
            Has<SpectatorLifeRoot>,
        ), (With<PlayerRoot>, Without<ChildOf>)>()
        .iter_mut(world)
    {
        let display = if active
            && !spectator
            && if dead {
                death
            } else {
                !death && (!workspace || open.is_some())
            } {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    if !active || dead {
        for mut node in world
            .query_filtered::<&mut Node, With<hud::PromptRoot>>()
            .iter_mut(world)
        {
            node.display = Display::None;
        }
    }
    life::refresh(world);
    rest::refresh(world, active && !dead);
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
    let village = &observed["sections"]["village"];
    let need = |name: &str| village["needs"][name].as_u64().unwrap_or(0);
    let values = [
        village["health"].as_u64().unwrap_or(0),
        need("hunger"),
        need("thirst"),
        need("energy"),
        need("warmth"),
    ];
    let stopped = world.resource::<SimControl>().is_stopped();
    let pending = world
        .resource::<NativeActions>()
        .pending
        .as_ref()
        .map(|pending| pending.label.clone());
    let busy = village["activity"]["busy"] == true;
    let ready = player_actions::control(world) == ControlState::Native
        && !stopped
        && !busy
        && pending.is_none()
        && !crate::island_control::control_pending(world);
    let selected = world.get_resource::<Selected>().and_then(|s| s.0);
    // At your own door from evening on, sleep comes first.
    let sleep = depth::LiveRequest::new("sleep");
    // Asleep, E wakes; the sleeping strip stands in the prompt's place.
    let asleep = rest::asleep(world);
    let context = if asleep {
        Some(model::wake_row())
    } else if depth::check(world, &sleep).is_ok() {
        Some(model::sleep_row(sleep, ready))
    } else {
        model::context(world, &observed, selected, ready)
    };
    let working = (busy && !asleep)
        .then(|| model::working_words(village["activity"]["kind"].as_str().unwrap_or("")));
    let prompt = if asleep {
        hud::prompt(None, None, None)
    } else {
        hud::prompt(
            context
                .as_ref()
                .map(|row| (row.title.as_str(), row.enabled)),
            pending.as_deref(),
            working,
        )
    };
    let context_action = context.as_ref().and_then(|row| row.action.clone());
    world.resource_mut::<PlayerState>().context = context;
    let journey = testbed_content::village::player_journey::project(&observed);
    let feedback = world.resource::<NativeActions>().feedback.clone();
    let mut goal_action = actions::goal_action(world, &observed);
    // A homestead upgrade under way takes island hours; when nothing more
    // urgent calls, the goal says what is happening and what to do meanwhile.
    let site_goal = depth::played(world)
        .and_then(|(unit, _)| testbed_content::village::view::upgrade_site(world, unit))
        .map(|site| {
            let day = world
                .get_resource::<npc_framework::prelude::SimConfig>()
                .map_or(2_400, |config| config.day_length.max(24));
            model::upgrade_goal(&site, day / 24)
        });
    // The upgrade is the goal when nothing else calls; beside a provision
    // or garden step it is one sentence after that step.
    let upgrade = site_goal
        .clone()
        .filter(|_| matches!(journey.next.id, "neighbors" | "trade" | "village_life"));
    let aside = site_goal
        .filter(|_| {
            matches!(
                journey.next.id,
                "water" | "food" | "seed" | "plant" | "water_crop" | "harvest"
            )
        })
        .map(|(title, _)| model::meanwhile_at_home(&title));
    if upgrade.is_some() {
        goal_action = Action::Open(Tab::Build);
    }
    // Asleep, the goal card folds to its title: the strip says the rest.
    let folded = world.resource::<PlayerState>().goal_folded || asleep;
    let reasons = hud::NEEDS.map(|(key, ..)| depth::need_reason(world, key));
    let chain = depth::chain(world);
    let work = depth::work_fraction(world);
    let repeat = depth::repeat_words(world);
    for line in depth::outcomes(world) {
        crate::island_shell::notice(world, line);
    }
    trade_sound(world);
    hud::refresh(
        world,
        hud::Facts {
            values,
            coins: village["coins"].as_u64().unwrap_or(0),
            journey: journey.title,
            // Asleep, the folded card says so, not "finish your activity".
            goal: if asleep {
                "Asleep until morning".to_owned()
            } else if let Some((title, _)) = &upgrade {
                title.clone()
            } else {
                journey.next.title.to_owned()
            },
            detail: match (&upgrade, &aside) {
                (Some((_, detail)), _) => detail.clone(),
                (None, Some(aside)) => format!("{} {aside}", journey.next.detail),
                (None, None) => journey.next.detail.clone(),
            },
            progress: if upgrade.is_some() {
                None
            } else {
                journey
                    .next
                    .progress
                    .map(|progress| (progress.current, progress.target, progress.unit))
            },
            goal_action,
            prompt,
            context: context_action,
            feedback: feedback.as_deref(),
            open,
            folded,
            chain,
            reasons,
            work,
            repeat,
        },
    );
    let rows = if let Some(tab) = open {
        let stock = model::inventory(&observed);
        let pouch: Vec<(String, String, u64)> = stock
            .iter()
            .map(|(kind, count)| (kind.clone(), model::friendly(kind), *count))
            .collect();
        let at = (tab == Tab::Craft)
            .then(|| model::station_in_reach(world, &observed))
            .flatten();
        workspace::frame(
            world,
            tab,
            model::badge(&observed, tab, at.as_deref()),
            &pouch,
        );
        model::rows(world, &observed, world.resource::<PlayerState>(), ready)
    } else {
        Vec::new()
    };
    workspace::reconcile(world, rows);
    atlas::refresh(world, &observed);
}

/// The played character's trade so far today, as the coin sound last heard
/// it: `(day, coins bought and sold)`.
#[derive(Resource, Default)]
pub(crate) struct TradeHeard(Option<(u64, u64)>);

/// A coin when a trade of the played character completes: the day's coins
/// bought and sold grew. A new day, or the first look, only remembers.
fn trade_sound(world: &mut World) {
    let Some((unit, _)) = depth::played(world) else {
        return;
    };
    let now = testbed_content::village::live::day_so_far(world, unit).map(|day| {
        (
            day.day,
            u64::from(day.bought_coins) + u64::from(day.sold_coins),
        )
    });
    let heard = world.get_resource::<TradeHeard>().and_then(|heard| heard.0);
    if traded(heard, now) {
        crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Coin);
    }
    world.insert_resource(TradeHeard(now));
}

/// Whether the day's trade grew since it was last heard.
pub(crate) fn traded(heard: Option<(u64, u64)>, now: Option<(u64, u64)>) -> bool {
    matches!((heard, now), (Some((day, before)), Some((today, after))) if day == today && after > before)
}

#[derive(Resource, Default)]
pub(crate) struct HiddenRoots(BTreeMap<Entity, Display>);
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
pub(crate) fn hide_roots(world: &mut World) {
    if !world
        .get_resource::<FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<(Entity, &Node), (
            Without<ChildOf>,
            Without<FrontendUiRoot>,
            // A tooltip card is a root too; a menu control's card must show.
            Without<crate::game_ui::tooltip::TooltipRoot>,
        )>()
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
