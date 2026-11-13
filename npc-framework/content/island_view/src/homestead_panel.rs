//! Native player input for the shared Island Village homestead actions.
//! UI entities and pending navigation are view/host state; mutations use recorded actions.
//!
//! `refresh` builds a full actor observation, which is the most expensive
//! thing any panel does. It therefore runs only while the panel is open and
//! one of its inputs changed (`Inputs`): a closed panel builds nothing, and
//! an open one over a paused or idle world keeps what it already shows.
mod model;
use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea};
use island_content::village::{VillageState, geography};
use npc_framework::agent_io::api::ActStep;
use npc_framework::prelude::SimControl;
use npc_framework::view_helpers::command_ui::{DragBox, select_units};
use npc_framework::view_helpers::{Selected, ViewSystems};

use crate::player_actions::{self, ControlState, NativeActions};
use model::{Action, Context, Entry, Tab};

// The incumbent native village palette; this is a control extension, not a new skin.
// A developer panel: the design system's chrome colours, its own layout.
const PANEL: Color = crate::game_ui::tokens::BARK_900;
const TEXT: Color = crate::game_ui::tokens::CREAM_50;
const GOLD: Color = crate::game_ui::tokens::HONEY_500;
const BUTTON: Color = crate::game_ui::tokens::BARK_700;
const HOVER: Color = crate::game_ui::tokens::BARK_600;
const DISABLED: Color = crate::game_ui::tokens::BARK_800;

#[derive(Resource)]
struct FarmState {
    open: bool,
    tab: Tab,
    plot: Option<u32>,
    bed: u32,
    quantity: u32,
    actor: Option<(u32, u64)>,
    waiting_control: bool,
    message: String,
    requests: Vec<Action>,
    rows: BTreeMap<String, Entity>,
    /// The inputs the open panel currently shows; `None` forces a rebuild.
    shown: Option<Inputs>,
    /// Real seconds since the panel was last rebuilt.
    since_refresh: f32,
    /// Full rebuilds so far, for tests and reviews.
    rebuilds: u64,
    /// Which of the panel's resting looks is already applied to its nodes.
    look: Look,
}

/// A resting look is applied once; after that the frame costs a few lookups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Look {
    #[default]
    Unknown,
    /// Player session or a menu: neither panel nor launcher.
    Hidden,
    /// Launcher shown, panel hidden.
    Closed,
    Open,
}

/// Seconds after which an open panel is rebuilt even with unchanged inputs:
/// a bound on staleness should an input ever be missing from `Inputs`.
const RECHECK_SECS: f32 = 1.0;

/// Everything the panel's contents depend on. The actor observation is a
/// function of simulation state, which changes only when a tick runs; the
/// rest is host and view state read directly.
#[derive(Clone, Debug, PartialEq)]
struct Inputs {
    tick: u64,
    epoch: u64,
    stopped: bool,
    control: ControlState,
    control_pending: bool,
    pending: Option<String>,
    feedback: Option<String>,
    tab: Tab,
    plot: Option<u32>,
    bed: u32,
    quantity: u32,
    waiting: bool,
    message: String,
    narrow: bool,
}

fn narrow(world: &mut World) -> bool {
    world
        .query::<&Window>()
        .iter(world)
        .next()
        .is_some_and(|window| window.width() < 1200.0)
}

fn inputs(world: &mut World) -> Inputs {
    let narrow = narrow(world);
    let state = world.resource::<FarmState>();
    let native = world.get_resource::<NativeActions>();
    Inputs {
        tick: world
            .get_resource::<npc_framework::prelude::SimClock>()
            .map_or(0, |clock| clock.tick),
        epoch: npc_framework::agent_io::api::epoch(world),
        stopped: world
            .get_resource::<SimControl>()
            .is_some_and(|control| control.is_stopped()),
        control: player_actions::control(world),
        control_pending: crate::island_control::control_pending(world),
        pending: native
            .and_then(|native| native.pending.as_ref())
            .map(|pending| pending.label.clone()),
        feedback: native.and_then(|native| native.feedback.clone()),
        tab: state.tab,
        plot: state.plot,
        bed: state.bed,
        quantity: state.quantity,
        waiting: state.waiting_control,
        message: state.message.clone(),
        narrow,
    }
}

impl Default for FarmState {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Land,
            plot: None,
            bed: 0,
            quantity: 1,
            actor: None,
            waiting_control: false,
            message: String::new(),
            requests: Vec::new(),
            rows: BTreeMap::new(),
            shown: None,
            since_refresh: 0.0,
            rebuilds: 0,
            look: Look::Unknown,
        }
    }
}
#[derive(Component)]
pub(crate) struct FarmRoot;
#[derive(Component)]
pub(crate) struct FarmLauncher;
#[derive(Component)]
struct FarmBody;
#[derive(Component)]
struct FarmStatus;
#[derive(Component)]
struct FarmMessage;
#[derive(Component, Clone)]
struct FarmAction(Action);
#[derive(Component)]
struct FarmControl;
#[derive(Component)]
struct FarmLabel;
#[derive(Component)]
struct FarmTypography;
#[derive(Resource)]
struct FarmFont(Handle<Font>);

pub(crate) fn plugin(app: &mut App) {
    crate::panel_scroll::install(app);
    if !app.is_plugin_added::<bevy::ui_widgets::ButtonPlugin>() {
        app.add_plugins(bevy::ui_widgets::ButtonPlugin);
    }
    if !app.is_plugin_added::<bevy::ui_widgets::ScrollAreaPlugin>() {
        app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    }
    if island_content::village::colony::enabled(app.world()) {
        app.add_systems(Startup, load_farm_font);
    }
    app.init_resource::<FarmState>()
        .init_resource::<NativeActions>()
        .init_resource::<InputFocus>()
        .add_observer(activate)
        .add_observer(acquire_focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, shield)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .run_if(not(crate::player_hud::enabled)),
        )
        .add_systems(
            Update,
            (player_actions::tick, process, refresh, typography, paint)
                .chain()
                .in_set(ViewSystems::Present),
        );
}

fn text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        FarmTypography,
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
    )
}
fn load_farm_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        commands.insert_resource(FarmFont(fonts.add(font)));
    }
}
fn typography(font: Option<Res<FarmFont>>, mut texts: Query<&mut TextFont, With<FarmTypography>>) {
    let Some(font) = font else { return };
    for mut text in &mut texts {
        if text.font != bevy::text::FontSource::Handle(font.0.clone()) {
            text.font = font.0.clone().into();
            if text.font_size == FontSize::Px(12.0) {
                text.font_size = FontSize::Px(13.0);
            } else if text.font_size == FontSize::Px(18.0) {
                text.font_size = FontSize::Px(20.0);
            }
        }
    }
}
fn button(world: &mut World, parent: Entity, label: &str, action: Action) -> Entity {
    let entity = world
        .spawn((
            Name::new(format!("Native farm: {label}")),
            ChildOf(parent),
            WidgetButton,
            FarmControl,
            FarmAction(action),
            TabIndex(0),
            Hovered::default(),
            AccessibleLabel::new(label),
            Node {
                min_height: px(30),
                padding: UiRect::axes(px(9), px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(BUTTON),
            BorderColor::all(Color::NONE),
        ))
        .id();
    world.spawn((ChildOf(entity), FarmLabel, text(label, 12.0, TEXT)));
    entity
}
fn spawn(world: &mut World) {
    if !geography::is_expanded(world) {
        return;
    }
    let launcher = world
        .spawn((
            Name::new("Home and farm launcher"),
            FarmLauncher,
            Hovered::default(),
            GlobalZIndex(16),
            TabGroup::new(4),
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(12),
                max_width: percent(94),
                ..default()
            },
        ))
        .id();
    button(world, launcher, "Home & farm [F2]", Action::Toggle);
    let root = world
        .spawn((
            Name::new("Home and farm panel"),
            FarmRoot,
            Hovered::default(),
            GlobalZIndex(26),
            TabGroup::new(4),
            BackgroundColor(PANEL),
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                left: px(12),
                width: px(400),
                max_width: percent(94),
                height: percent(88),
                max_height: px(940),
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let header = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.spawn((ChildOf(header), text("Home & farm", 18.0, GOLD)));
    button(world, header, "Close [F2]", Action::Close);
    world.spawn((ChildOf(root), FarmStatus, text("", 12.0, TEXT)));
    let controls = world
        .spawn((
            ChildOf(root),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    for (label, action) in [
        ("Play as hero", Action::Take),
        ("Stop", Action::Stop),
        ("Release controls", Action::Release),
        ("Resume world", Action::Resume),
    ] {
        button(world, controls, label, action);
    }
    let tabs = world
        .spawn((
            ChildOf(root),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    for (label, tab) in [
        ("Land & crops", Tab::Land),
        ("Supplies", Tab::Supplies),
        ("Food & trade", Tab::Living),
    ] {
        button(world, tabs, label, Action::Tab(tab));
    }
    let quantity = world
        .spawn((
            ChildOf(root),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    button(world, quantity, "Quantity -", Action::Quantity(-1));
    button(world, quantity, "Quantity +", Action::Quantity(1));
    let body = world
        .spawn((
            ChildOf(root),
            FarmBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    for control in crate::panel_scroll::attach(
        world,
        root,
        body,
        "Home & farm",
        ["More above", "More below"],
    ) {
        world.entity_mut(control).insert(FarmControl);
    }
    world.spawn((ChildOf(root), FarmMessage, text("", 12.0, GOLD)));
    world.spawn((
        ChildOf(root),
        text(
            "Tab / Shift+Tab: choose | Enter / Space: act\nEsc: return to the world",
            11.0,
            TEXT,
        ),
    ));
}

fn activate(
    event: On<Activate>,
    actions: Query<(&FarmAction, Has<InteractionDisabled>)>,
    mut state: ResMut<FarmState>,
) {
    if let Ok((action, false)) = actions.get(event.entity)
        && (state.open || action.0 == Action::Toggle)
    {
        state.requests.push(action.0.clone());
    }
}
fn acquire_focus(
    mut event: On<AcquireFocus>,
    controls: Query<(), With<FarmControl>>,
    mut focus: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        focus.set(event.focused_entity, FocusCause::Navigated);
    }
}
fn input_active(focus: Option<Res<InputFocus>>, controls: Query<(), With<FarmControl>>) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| controls.contains(entity))
}
#[derive(SystemParam)]
struct Keyboard<'w, 's> {
    focus: ResMut<'w, InputFocus>,
    controls: Query<'w, 's, (), With<FarmControl>>,
    windows: Query<'w, 's, (), With<Window>>,
    roots: Query<'w, 's, Entity, With<FarmRoot>>,
    nav: TabNavigation<'w, 's>,
}
fn keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut state: ResMut<FarmState>,
    keyboard: Keyboard,
    mut commands: Commands,
    experience: Option<Res<crate::PlayerExperience>>,
    mode: Option<Res<crate::IslandMode>>,
) {
    if experience.is_some_and(|experience| experience.enabled)
        && mode.as_deref() == Some(&crate::IslandMode::Character)
    {
        return;
    }
    let Keyboard {
        mut focus,
        controls,
        windows,
        roots,
        nav,
    } = keyboard;
    let Some(keys) = keys else { return };
    if focus
        .get()
        .is_some_and(|entity| !controls.contains(entity) && !windows.contains(entity))
    {
        return;
    }
    if keys.just_pressed(KeyCode::F2) && !roots.is_empty() {
        state.requests.push(Action::Toggle);
    }
    if !state.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.requests.push(Action::Close);
    }
    if keys.just_pressed(KeyCode::Tab) {
        let dir = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        let next = if focus.get().is_some_and(|entity| controls.contains(entity)) {
            nav.navigate(&focus, dir)
        } else if let Ok(root) = roots.single() {
            nav.initialize(root, dir)
        } else {
            return;
        };
        if let Ok(next) = next {
            focus.set(next, FocusCause::Navigated);
            commands.trigger(bevy::ui_widgets::ScrollIntoView { entity: next });
        }
    }
}
fn shield(
    roots: Query<&Hovered, Or<(With<FarmRoot>, With<FarmLauncher>)>>,
    mut mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    mut scroll: Option<ResMut<AccumulatedMouseScroll>>,
    mut drag: Option<ResMut<DragBox>>,
    mut focus: ResMut<InputFocus>,
    controls: Query<(), With<FarmControl>>,
) {
    if roots.iter().any(Hovered::get) {
        if let Some(mouse) = mouse.as_mut() {
            mouse.reset_all();
        }
        if let Some(scroll) = scroll.as_mut() {
            scroll.delta = Vec2::ZERO;
        }
        if let Some(drag) = drag.as_mut() {
            drag.start = None;
        }
    } else if mouse.as_ref().is_some_and(|mouse| {
        mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)
    }) && focus.get().is_some_and(|entity| controls.contains(entity))
    {
        focus.clear();
    }
}

fn process(world: &mut World) {
    if crate::player_hud::enabled(world) {
        let mut state = world.resource_mut::<FarmState>();
        state.requests.clear();
        state.open = false;
        return;
    }
    let requests = std::mem::take(&mut world.resource_mut::<FarmState>().requests);
    for request in requests {
        let mut result = Ok(());
        match request {
            Action::Toggle => {
                let open = world.resource::<FarmState>().open;
                world.resource_mut::<FarmState>().open = !open;
            }
            Action::Close => {
                world.resource_mut::<FarmState>().open = false;
                world.resource_mut::<InputFocus>().clear();
            }
            Action::Tab(tab) => {
                world.resource_mut::<FarmState>().tab = tab;
                reset_scroll(world);
            }
            Action::Plot(id) => {
                let mut state = world.resource_mut::<FarmState>();
                state.plot = Some(id);
                state.bed = 0;
            }
            Action::Bed(index) => world.resource_mut::<FarmState>().bed = index,
            Action::Quantity(delta) => {
                let quantity = world
                    .resource::<FarmState>()
                    .quantity
                    .saturating_add_signed(delta)
                    .clamp(1, 20);
                world.resource_mut::<FarmState>().quantity = quantity;
            }
            Action::Take => {
                result = if crate::island_control::available(world) {
                    crate::island_control::request_mode(world, crate::IslandMode::Character)
                } else {
                    player_actions::take_control(world).map(|_| ())
                };
                if result.is_ok() {
                    world.resource_mut::<FarmState>().waiting_control = true;
                    if !crate::island_control::available(world)
                        && let Some(id) = world
                            .get_resource::<VillageState>()
                            .map(|state| state.played)
                    {
                        select_units(world, [id], false);
                        world.get_resource_or_init::<Selected>().0 = Some(id);
                        if let Some(mut follow) =
                            world.get_resource_mut::<crate::village_world::FollowHero>()
                            && !follow.enabled
                        {
                            follow.toggle();
                        }
                    }
                }
            }
            Action::Release => {
                result = if crate::island_control::available(world) {
                    crate::island_control::request_mode(world, crate::IslandMode::Inspector)
                } else {
                    player_actions::release(world).map(|_| ())
                };
            }
            Action::Stop => result = player_actions::stop(world).map(|_| ()),
            Action::Resume => {
                if let Some(mut control) = world.get_resource_mut::<SimControl>() {
                    control.paused = false;
                    control.multiplier = control.multiplier.max(1);
                }
            }
            Action::Walk(pos) => result = player_actions::walk(world, pos).map(|_| ()),
            Action::Work {
                verb,
                target,
                args,
                approach,
                label,
            } => {
                let action = ActStep {
                    verb: Some(verb),
                    target,
                    args: Some(args),
                    ..default()
                };
                result = match approach {
                    Some(position) => player_actions::walk_then_act(world, position, action),
                    None => player_actions::act(world, action),
                }
                .map(|_| ());
                if result.is_ok() {
                    let message = format!("Requested: {label}");
                    world.resource_mut::<FarmState>().message = message.clone();
                    let mut native = world.resource_mut::<NativeActions>();
                    native.feedback = Some(message);
                    if let Some(pending) = native.pending.as_mut() {
                        pending.label = label;
                    }
                }
            }
        }
        if let Err(error) = result {
            world.resource_mut::<FarmState>().message = error.clone();
            world.resource_mut::<NativeActions>().feedback = Some(error);
        }
    }
}
fn reset_scroll(world: &mut World) {
    let mut query = world.query_filtered::<&mut ScrollPosition, With<FarmBody>>();
    for mut position in query.iter_mut(world) {
        position.0 = Vec2::ZERO;
    }
}
fn set_text<M: Component>(world: &mut World, value: &str) {
    let mut query = world.query_filtered::<&mut Text, With<M>>();
    for mut text in query.iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}
pub(crate) fn set_open(world: &mut World, open: bool) {
    if let Some(mut state) = world.get_resource_mut::<FarmState>() {
        state.open = open;
    }
}

pub(crate) fn refresh(world: &mut World) {
    if !geography::is_expanded(world) || !world.contains_resource::<VillageState>() {
        return;
    }
    if crate::player_hud::enabled(world)
        || world
            .get_resource::<crate::FrontendGate>()
            .is_some_and(|gate| gate.0)
    {
        if world.resource::<FarmState>().look != Look::Hidden {
            for mut node in world
                .query_filtered::<&mut Node, Or<(With<FarmRoot>, With<FarmLauncher>)>>()
                .iter_mut(world)
            {
                node.display = Display::None;
            }
            let mut state = world.resource_mut::<FarmState>();
            state.look = Look::Hidden;
            state.shown = None;
        }
        return;
    }
    if !world.resource::<FarmState>().open {
        // Closed: show the launcher and build nothing. What the panel holds
        // is rebuilt on the frame it opens.
        closed(world);
        return;
    }
    world.resource_mut::<FarmState>().look = Look::Open;
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let wanted = inputs(world);
    {
        let mut state = world.resource_mut::<FarmState>();
        state.since_refresh += delta;
        if state.shown.as_ref() == Some(&wanted) && state.since_refresh < RECHECK_SECS {
            return;
        }
        state.since_refresh = 0.0;
        state.rebuilds += 1;
    }
    let Ok(observation) = player_actions::observe(world) else {
        return;
    };
    let actor = (
        observation["unit"].as_u64().unwrap_or(0) as u32,
        observation["epoch"].as_u64().unwrap_or(0),
    );
    {
        let mut state = world.resource_mut::<FarmState>();
        if state.actor != Some(actor) {
            state.actor = Some(actor);
            state.plot = None;
            state.bed = 0;
            state.waiting_control = false;
            state.message.clear();
        }
        let plots = observation["sections"]["homestead"]["plots"].as_array();
        if !plots.is_some_and(|plots| {
            plots
                .iter()
                .any(|plot| plot["id"].as_u64() == state.plot.map(u64::from))
        }) {
            state.plot = plots
                .and_then(|plots| {
                    plots
                        .iter()
                        .find(|plot| plot["own"] == true)
                        .or_else(|| plots.first())
                })
                .and_then(|plot| plot["id"].as_u64())
                .and_then(|id| u32::try_from(id).ok());
            state.bed = 0;
        }
    }
    let control = player_actions::control(world);
    if control != ControlState::Free {
        world.resource_mut::<FarmState>().waiting_control = false;
    }
    let paused = world
        .get_resource::<SimControl>()
        .is_some_and(|control| control.is_stopped());
    let busy = observation["sections"]["village"]["activity"]["busy"] == true
        || world.resource::<NativeActions>().pending.is_some();
    let ready = control == ControlState::Native
        && !busy
        && !paused
        && !crate::island_control::control_pending(world);
    let state = world.resource::<FarmState>();
    let (open, tab, plot, bed, quantity, waiting) = (
        state.open,
        state.tab,
        state.plot,
        state.bed,
        state.quantity,
        state.waiting_control,
    );
    let rows = if open {
        model::entries(Context {
            world,
            observation: &observation,
            tab,
            plot,
            bed,
            quantity,
            ready,
        })
    } else {
        Vec::new()
    };
    let village = &observation["sections"]["village"];
    let pct = |name: &str| {
        village["needs"][name]
            .as_u64()
            .map_or("?".into(), |n| (n / 1000).to_string())
    };
    let colony = island_content::village::colony::enabled(world);
    let activity = village["activity"]["kind"].as_str().unwrap_or("idle");
    let caption = format!(
        "{} | {} coins | {}\nFood {}% | Water {}% | Energy {}%\n{}{}",
        village["name"].as_str().unwrap_or("Hero"),
        village["coins"],
        if colony {
            model::player_activity_caption(activity)
        } else {
            activity
        },
        pct("hunger"),
        pct("thirst"),
        pct("energy"),
        if waiting {
            if colony {
                if paused {
                    "Resume the world to start playing."
                } else {
                    "Getting ready to play."
                }
            } else {
                "Waiting for the next world tick to take control."
            }
        } else if colony {
            model::player_control_caption(&control)
        } else {
            model::control_caption(&control)
        },
        if paused {
            "\nWorld paused. Resume to move, work and grow crops."
        } else {
            ""
        }
    );
    set_text::<FarmStatus>(world, &caption);
    let feedback = world
        .resource::<NativeActions>()
        .pending
        .as_ref()
        .map(|pending| format!("Walking, then {}", pending.label))
        .or_else(|| world.resource::<NativeActions>().feedback.clone())
        .unwrap_or_else(|| world.resource::<FarmState>().message.clone());
    let feedback = if (feedback.starts_with("Craft ") || feedback.starts_with("Homestead "))
        && !world.resource::<FarmState>().message.is_empty()
    {
        world.resource::<FarmState>().message.clone()
    } else {
        feedback
    };
    let message = if matches!(tab, Tab::Land) {
        feedback
    } else {
        format!("Trade quantity: {quantity} · Gathering collects one item\n{feedback}")
    };
    set_text::<FarmMessage>(world, &message);
    let mut panels = world.query_filtered::<&mut Node, With<FarmRoot>>();
    for mut node in panels.iter_mut(world) {
        let display = if open { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    let mut launchers = world.query_filtered::<&mut Node, With<FarmLauncher>>();
    for mut node in launchers.iter_mut(world) {
        let display = if open { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
    }
    let narrow = narrow(world);
    let mut hud = world.query_filtered::<&mut Visibility, With<crate::village::Hud>>();
    for mut visibility in hud.iter_mut(world) {
        let wanted = if open && narrow {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let mut actions = world.query::<(Entity, &FarmAction)>();
    let static_buttons: Vec<_> = actions
        .iter(world)
        .map(|(entity, action)| (entity, action.0.clone()))
        .collect();
    for (entity, action) in static_buttons {
        let (visible, enabled) = match action {
            Action::Take => (
                true,
                control == ControlState::Free && !waiting && player_actions::available(world),
            ),
            Action::Release => (
                control == ControlState::Native,
                control == ControlState::Native,
            ),
            Action::Stop => (
                control == ControlState::Native,
                control == ControlState::Native && busy,
            ),
            Action::Resume => (paused, paused),
            Action::Quantity(delta) => (
                tab != Tab::Land,
                if delta < 0 {
                    quantity > 1
                } else {
                    quantity < 20
                },
            ),
            _ => continue,
        };
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(mut node) = world.get_mut::<Node>(entity)
            && node.display != display
        {
            node.display = display;
        }
        disabled(world, entity, !enabled);
    }
    if open {
        reconcile(world, rows);
    }
    // The rebuild itself settles the plot selection and the waiting flag.
    let shown = inputs(world);
    world.resource_mut::<FarmState>().shown = Some(shown);
}

/// The closed panel: launcher shown, panel hidden, the status strip back and
/// keyboard focus released. No observation is built.
fn closed(world: &mut World) {
    if world
        .resource::<InputFocus>()
        .get()
        .is_some_and(|entity| world.get::<FarmControl>(entity).is_some())
    {
        world.resource_mut::<InputFocus>().clear();
    }
    // Applied once per close; after that a closed panel costs two lookups.
    if world.resource::<FarmState>().look == Look::Closed {
        return;
    }
    {
        let mut state = world.resource_mut::<FarmState>();
        state.shown = None;
        state.look = Look::Closed;
    }
    for mut node in world
        .query_filtered::<&mut Node, With<FarmRoot>>()
        .iter_mut(world)
    {
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<FarmLauncher>>()
        .iter_mut(world)
    {
        if node.display != Display::Flex {
            node.display = Display::Flex;
        }
    }
    for mut visibility in world
        .query_filtered::<&mut Visibility, With<crate::village::Hud>>()
        .iter_mut(world)
    {
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
    }
}
fn disabled(world: &mut World, entity: Entity, value: bool) {
    if world.get::<InteractionDisabled>(entity).is_some() == value {
        return;
    }
    if value {
        world.entity_mut(entity).insert(InteractionDisabled);
    } else {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    }
}
fn reconcile(world: &mut World, rows: Vec<Entry>) {
    let Some(body) = world
        .query_filtered::<Entity, With<FarmBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old = std::mem::take(&mut world.resource_mut::<FarmState>().rows);
    let mut live = BTreeMap::new();
    let mut order = Vec::new();
    for row in rows {
        let entity = old
            .get(&row.key)
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
            .unwrap_or_else(|| {
                if let Some(action) = &row.action {
                    button(world, body, &row.text, action.clone())
                } else {
                    world
                        .spawn((
                            ChildOf(body),
                            text(
                                &row.text,
                                if row.heading { 14.0 } else { 12.0 },
                                if row.heading { GOLD } else { TEXT },
                            ),
                            Node {
                                margin: UiRect::top(px(if row.heading { 8 } else { 2 })),
                                flex_shrink: 0.0,
                                ..default()
                            },
                        ))
                        .id()
                }
            });
        if let Some(action) = row.action {
            world
                .entity_mut(entity)
                .insert((FarmAction(action), AccessibleLabel::new(&row.text)));
            disabled(world, entity, !row.enabled);
            let children = world
                .get::<Children>(entity)
                .map(|children| children.to_vec())
                .unwrap_or_default();
            for child in children {
                if let Some(mut text) = world.get_mut::<Text>(child)
                    && text.0 != row.text
                {
                    text.0 = row.text.clone();
                }
            }
        } else if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != row.text
        {
            text.0 = row.text;
        }
        if row.selected {
            world.entity_mut(entity).insert(SelectedRow);
        } else {
            world.entity_mut(entity).remove::<SelectedRow>();
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
    if world
        .get::<Children>(body)
        .is_none_or(|children| children.as_ref() != order.as_slice())
    {
        world.entity_mut(body).replace_children(&order);
    }
    world.resource_mut::<FarmState>().rows = live;
}
#[derive(Component)]
struct SelectedRow;
fn paint(
    focus: Res<InputFocus>,
    state: Res<FarmState>,
    mut buttons: Query<
        (
            Entity,
            &FarmAction,
            &Hovered,
            Has<InteractionDisabled>,
            Has<SelectedRow>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<FarmControl>,
    >,
) {
    for (entity, action, hovered, disabled, selected, mut background, mut border) in &mut buttons {
        let color = if disabled {
            DISABLED
        } else if hovered.get() {
            HOVER
        } else {
            BUTTON
        };
        if background.0 != color {
            background.0 = color;
        }
        let selected = selected || matches!(action.0,Action::Tab(tab) if tab==state.tab);
        let color = BorderColor::all(if focus.get() == Some(entity) || selected {
            GOLD
        } else {
            Color::NONE
        });
        if *border != color {
            *border = color;
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod pause_tests {
    use super::*;
    #[test]
    fn resume_control_unfreezes_a_zero_multiplier_world() {
        let mut world = World::new();
        world.insert_resource(FarmState {
            requests: vec![Action::Resume],
            ..default()
        });
        world.insert_resource(SimControl {
            paused: false,
            multiplier: 0,
            ..default()
        });
        process(&mut world);
        let control = world.resource::<SimControl>();
        assert!(!control.is_stopped());
        assert_eq!(control.multiplier, 1);
    }
}
