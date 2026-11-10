//! Colony tools are a view of the live simulation. Every edit is a recorded command.
mod model;
use model::{Entry, entries};
use std::collections::BTreeMap;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea};
use npc_framework::kernel::commands::{CommandLog, SkippedCommands};
use npc_framework::prelude::{
    SimClock, SimCommand, SimConfig, SimControl, SimId, SimIndex, submit,
};
use npc_framework::view_helpers::command_ui::DragBox;
use npc_framework::view_helpers::{Selected, ViewSystems, sim_to_world};
use testbed_content::village::{
    self, colony,
    terrain::{Terraform, TerrainState, TerrainSurface},
};

use crate::IslandMode;

const PANEL: Color = Color::srgb(0.05, 0.06, 0.08);
const TEXT: Color = Color::srgb(0.93, 0.95, 0.90);
const MUTED: Color = Color::srgb(0.69, 0.77, 0.73);
const GOLD: Color = Color::srgb(0.95, 0.80, 0.45);
const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Overview,
    Residents,
    Landscape,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Toggle,
    Close,
    Mode(IslandMode),
    Tab(Tab),
    Pause,
    Speed(u32),
    Reserve(u32),
    Gatherings,
    Resident(SimId),
    Home(SimId, SimId),
    Profession(SimId, SimId),
    Settlement(SimId),
    Commission(SimId, SimId),
    CancelHome(SimId),
    Surface(TerrainSurface),
    Height(i32),
    Radius(i32),
}
#[derive(Resource)]
struct Ledger {
    open: bool,
    tab: Tab,
    surface: TerrainSurface,
    height_mm: i32,
    radius: u32,
    cell: Option<(i32, i32)>,
    preview_error: Option<String>,
    message: String,
    requests: Vec<Action>,
    pending: Option<((u64, u32), String)>,
    last_mode: Option<IslandMode>,
    control_revision: u64,
    rows: BTreeMap<String, Entity>,
    elapsed: f32,
    dirty: bool,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            open: true,
            tab: Tab::Overview,
            surface: TerrainSurface::Grass,
            height_mm: 0,
            radius: 1,
            cell: None,
            preview_error: None,
            message: "The village runs on its own. Select a resident to follow their day.".into(),
            requests: Vec::new(),
            pending: None,
            last_mode: None,
            control_revision: 0,
            rows: BTreeMap::new(),
            elapsed: 1.0,
            dirty: true,
        }
    }
}
#[derive(Component)]
pub(crate) struct LedgerRoot;
#[derive(Component)]
pub(crate) struct LedgerLauncher;
#[derive(Component)]
struct LedgerBody;
#[derive(Component)]
struct LedgerStatus;
#[derive(Component)]
struct LedgerMessage;
#[derive(Component, Clone)]
struct LedgerAction(Action);
#[derive(Component)]
struct LedgerControl;
#[derive(Component)]
struct LedgerTypography;
#[derive(Resource)]
struct LedgerFont(Handle<Font>);
pub(crate) fn plugin(app: &mut App) {
    if !colony::enabled(app.world()) {
        return;
    }
    crate::panel_scroll::install(app);
    app.init_resource::<IslandMode>()
        .init_resource::<Ledger>()
        .init_resource::<InputFocus>()
        .add_observer(activate)
        .add_observer(acquire_focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, pointer)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .run_if(not(crate::player_hud::active))
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (process, refresh, typography, paint, brush_gizmo)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::homestead_panel::refresh),
        );
}

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        LedgerTypography,
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
    )
}
fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}
fn button(world: &mut World, parent: Entity, label: &str, action: Action) -> Entity {
    let entity = world
        .spawn((
            Name::new(format!("Colony: {label}")),
            ChildOf(parent),
            WidgetButton,
            LedgerControl,
            LedgerAction(action),
            TabIndex(0),
            Hovered::default(),
            AccessibleLabel::new(label),
            Node {
                min_height: px(44),
                padding: UiRect::axes(px(9), px(6)),
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(BUTTON),
            BorderColor::all(Color::NONE),
        ))
        .id();
    world.spawn((ChildOf(entity), text(label, 15.0, TEXT)));
    entity
}
fn spawn(world: &mut World) {
    if let Some(mut fonts) = world.get_resource_mut::<Assets<Font>>() {
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        let handle = fonts.add(font);
        world.insert_resource(LedgerFont(handle));
    }
    let launcher = world
        .spawn((
            Name::new("Colony ledger launcher"),
            LedgerLauncher,
            Hovered::default(),
            TabGroup::new(6),
            GlobalZIndex(27),
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                bottom: px(12),
                ..default()
            },
        ))
        .id();
    button(world, launcher, "Colony & landscape [F11]", Action::Toggle);
    let root = world
        .spawn((
            Name::new("Island colony ledger"),
            LedgerRoot,
            Hovered::default(),
            TabGroup::new(6),
            GlobalZIndex(29),
            BackgroundColor(PANEL),
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                right: px(12),
                width: px(408),
                max_width: percent(94),
                height: percent(91),
                max_height: px(1100),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
        ))
        .id();
    let title = row(world, root);
    world.spawn((
        ChildOf(title),
        text("Island Village", 23.0, GOLD),
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    button(world, title, "Close", Action::Close);
    let modes = row(world, root);
    for (label, mode) in [
        ("Inspect", IslandMode::Inspector),
        ("Play character", IslandMode::Character),
        ("Terraform", IslandMode::Editor),
    ] {
        button(world, modes, label, Action::Mode(mode));
    }
    world.spawn((ChildOf(root), LedgerStatus, text("", 13.0, TEXT)));
    let clock = row(world, root);
    button(world, clock, "Pause / resume", Action::Pause);
    for speed in [1, 3, 10] {
        button(world, clock, &format!("{speed}x"), Action::Speed(speed));
    }
    let tabs = row(world, root);
    for (label, tab) in [
        ("Village", Tab::Overview),
        ("Residents", Tab::Residents),
        ("Landscape", Tab::Landscape),
    ] {
        button(world, tabs, label, Action::Tab(tab));
    }
    let body = world
        .spawn((
            ChildOf(root),
            LedgerBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(7),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    for control in
        crate::panel_scroll::attach(world, root, body, "Colony", ["Scroll up", "Scroll down"])
    {
        world.entity_mut(control).insert(LedgerControl);
    }
    world.spawn((ChildOf(root), LedgerMessage, text("", 12.0, GOLD)));
    let controls = if crate::player_hud::enabled(world) {
        "Menu: pause / save · F11: ledger\nWASD / arrows: pan · Wheel: zoom\nQ/E: orbit · Play character: join the island"
    } else {
        "F11: ledger · F2: home & farm\nWASD: pan / walk · arrows: pan\nWheel: zoom · Q/E: orbit"
    };
    world.spawn((ChildOf(root), text(controls, 12.0, MUTED)));
}
fn activate(
    event: On<Activate>,
    actions: Query<(&LedgerAction, Has<InteractionDisabled>)>,
    mut state: ResMut<Ledger>,
) {
    if let Ok((action, false)) = actions.get(event.entity)
        && (state.open || action.0 == Action::Toggle)
    {
        state.requests.push(action.0.clone());
    }
}
fn acquire_focus(
    mut event: On<AcquireFocus>,
    controls: Query<(), With<LedgerControl>>,
    mut focus: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        focus.set(event.focused_entity, FocusCause::Navigated);
    }
}
fn input_active(focus: Option<Res<InputFocus>>, controls: Query<(), With<LedgerControl>>) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| controls.contains(entity))
}
#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut state: ResMut<Ledger>,
    mut focus: ResMut<InputFocus>,
    controls: Query<(), With<LedgerControl>>,
    windows: Query<(), With<Window>>,
    roots: Query<Entity, With<LedgerRoot>>,
    nav: TabNavigation,
    mut commands: Commands,
) {
    let Some(keys) = keys else { return };
    if focus
        .get()
        .is_some_and(|entity| !controls.contains(entity) && !windows.contains(entity))
    {
        return;
    }
    if keys.just_pressed(KeyCode::F11) {
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

fn pointer(world: &mut World) {
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    let click = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| mouse.just_pressed(MouseButton::Left));
    if ui_hovered {
        if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            mouse.reset_all();
        }
        if let Some(mut scroll) = world.get_resource_mut::<AccumulatedMouseScroll>() {
            scroll.delta = Vec2::ZERO;
        }
        if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
            drag.start = None;
        }
        world.resource_mut::<Ledger>().cell = None;
        return;
    }
    let world_click = click
        || world
            .get_resource::<ButtonInput<MouseButton>>()
            .is_some_and(|mouse| mouse.just_pressed(MouseButton::Right));
    if world_click
        && world
            .resource::<InputFocus>()
            .get()
            .is_some_and(|e| world.get::<LedgerControl>(e).is_some())
    {
        world.resource_mut::<InputFocus>().clear();
    }
    if *world.resource::<IslandMode>() != IslandMode::Editor {
        return;
    }
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position);
    let ray = cursor.and_then(|cursor| {
        world
            .query_filtered::<(&Camera, &GlobalTransform), With<crate::RtsCamera>>()
            .iter(world)
            .find_map(|(camera, transform)| camera.viewport_to_world(transform, cursor).ok())
    });
    let cell = ray.and_then(|ray| {
        if village::frontier::enabled(world) {
            crate::colony_scene::living_scene::terrain_hit(world, ray)
        } else {
            terrain_hit(world.resource::<TerrainState>(), ray)
        }
    });
    let command = cell.map(|(x, z)| {
        let state = world.resource::<Ledger>();
        Terraform {
            x,
            z,
            radius: state.radius,
            height_mm: state.height_mm,
            surface: state.surface,
        }
    });
    let error = command
        .as_ref()
        .and_then(|command| command.check(world).err());
    {
        let mut state = world.resource_mut::<Ledger>();
        if state.cell != cell || state.preview_error != error {
            state.dirty = true;
        }
        state.cell = cell;
        state.preview_error = error;
    }
    if click && let Some(command) = command {
        queue(
            world,
            SimCommand::content(&command),
            format!("Landscape updated at {}, {}", command.x, command.z),
        );
    }
    // Painting must never also select a villager or issue a movement order.
    if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        mouse.reset_all();
    }
    if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
        drag.start = None;
    }
}

/// Intersect every bounded column's top, so raised cells stay pickable at their real elevation.
fn terrain_hit(terrain: &TerrainState, ray: Ray3d) -> Option<(i32, i32)> {
    if ray.direction.y.abs() < 0.000_01 {
        return None;
    }
    let mut best: Option<(f32, i32, i32)> = None;
    for z in -32..32 {
        for x in -32..32 {
            let Some(cell) = terrain.cell(x, z) else {
                continue;
            };
            let y = if cell.surface == TerrainSurface::Water {
                -0.12
            } else {
                cell.height_mm as f32 / 1000.0
            };
            let distance = (y - ray.origin.y) / ray.direction.y;
            if distance < 0.0 || best.is_some_and(|(old, _, _)| old <= distance) {
                continue;
            }
            let point = ray.get_point(distance);
            if point.x >= x as f32
                && point.x < (x + 1) as f32
                && point.z >= z as f32
                && point.z < (z + 1) as f32
            {
                best = Some((distance, x, z));
            }
        }
    }
    best.map(|(_, x, z)| (x, z))
}

fn queue(world: &mut World, command: SimCommand, success: String) {
    let result = submit(world, None, command);
    let mut state = world.resource_mut::<Ledger>();
    match result {
        Ok(key) => {
            state.pending = Some((key, success));
            state.message = "Change requested. Resume the world if paused.".into();
        }
        Err(error) => state.message = format!("Cannot apply: {error}"),
    }
    state.dirty = true;
}
fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Ledger>().requests);
    for request in requests {
        world.resource_mut::<Ledger>().dirty = true;
        match request {
            Action::Toggle => {
                let open = world.resource::<Ledger>().open;
                if open && *world.resource::<IslandMode>() == IslandMode::Editor {
                    switch_mode(world, IslandMode::Inspector);
                }
                world.resource_mut::<Ledger>().open = !open;
            }
            Action::Close => {
                if *world.resource::<IslandMode>() == IslandMode::Editor {
                    switch_mode(world, IslandMode::Inspector);
                }
                world.resource_mut::<Ledger>().open = false;
                world.resource_mut::<InputFocus>().clear();
            }
            Action::Mode(mode) => switch_mode(world, mode),
            Action::Tab(tab) => {
                world.resource_mut::<Ledger>().tab = tab;
                for mut scroll in world
                    .query_filtered::<&mut ScrollPosition, With<LedgerBody>>()
                    .iter_mut(world)
                {
                    scroll.0 = Vec2::ZERO;
                }
            }
            Action::Pause => {
                let mut control = world.resource_mut::<SimControl>();
                if control.is_stopped() {
                    control.paused = false;
                    control.multiplier = control.multiplier.max(1);
                } else {
                    control.paused = true;
                }
            }
            Action::Speed(speed) => {
                world.resource_mut::<SimControl>().multiplier = speed;
            }
            Action::Reserve(value) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "reserve_food".into(),
                    resident: None,
                    house: None,
                    value,
                }),
                format!("Household food reserve set to {value} days."),
            ),
            Action::Gatherings => {
                let value = u32::from(!world.resource::<colony::ColonyState>().gatherings);
                queue(
                    world,
                    SimCommand::content(&colony::ColonyAct {
                        verb: "gatherings".into(),
                        resident: None,
                        house: None,
                        value,
                    }),
                    "Gathering policy updated.".into(),
                );
            }
            Action::Resident(id) => {
                world.get_resource_or_init::<Selected>().0 = Some(id);
                let position = world
                    .resource::<SimIndex>()
                    .get(id)
                    .and_then(|e| world.get::<npc_framework::prelude::SimPos>(e))
                    .copied();
                if let Some(position) = position {
                    crate::island_camera::focus(world, sim_to_world(position), 12.0);
                }
            }
            Action::Profession(resident, workplace) => queue(
                world,
                SimCommand::content(&village::frontier::labor::AssignProfession {
                    resident,
                    workplace,
                }),
                "Assignment requested. The resident finishes committed work before changing jobs."
                    .into(),
            ),
            Action::Settlement(id) => {
                if let Some(position) = world
                    .resource::<SimIndex>()
                    .get(id)
                    .and_then(|entity| world.get::<npc_framework::prelude::SimPos>(entity))
                    .copied()
                {
                    crate::island_camera::focus(world, sim_to_world(position), 65.0);
                }
            }
            Action::Home(resident, house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "assign_home".into(),
                    resident: Some(resident),
                    house: Some(house),
                    value: 0,
                }),
                "Home assigned. This resident will rest at the new household.".into(),
            ),
            Action::Commission(resident, house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "commission_home".into(),
                    resident: Some(resident),
                    house: Some(house),
                    value: 0,
                }),
                "Home commissioned. The builder will procure materials and work on site.".into(),
            ),
            Action::CancelHome(house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "cancel_home".into(),
                    resident: None,
                    house: Some(house),
                    value: 0,
                }),
                "Housing commission cancelled; unused materials remain with the builder.".into(),
            ),
            Action::Surface(surface) => {
                let mut state = world.resource_mut::<Ledger>();
                state.surface = surface;
                state.height_mm = if surface == TerrainSurface::Water {
                    state.height_mm.min(0)
                } else {
                    state.height_mm.max(0)
                };
            }
            Action::Height(delta) => {
                let water = world.resource::<Ledger>().surface == TerrainSurface::Water;
                let regional = village::frontier::enabled(world);
                let value = world
                    .resource::<Ledger>()
                    .height_mm
                    .saturating_add(delta)
                    .clamp(
                        if water {
                            if regional { -2000 } else { -1000 }
                        } else {
                            0
                        },
                        if water {
                            0
                        } else if regional {
                            12000
                        } else {
                            3000
                        },
                    );
                world.resource_mut::<Ledger>().height_mm = value;
            }
            Action::Radius(delta) => {
                let value = world
                    .resource::<Ledger>()
                    .radius
                    .saturating_add_signed(delta)
                    .min(if village::frontier::enabled(world) {
                        8
                    } else {
                        3
                    });
                world.resource_mut::<Ledger>().radius = value;
            }
        }
    }
    if let Some((key, success)) = world.resource::<Ledger>().pending.clone() {
        let failed = world.get_resource::<SkippedCommands>().and_then(|log| {
            log.entries
                .iter()
                .rev()
                .find(|entry| (entry.tick, entry.seq) == key)
                .map(|entry| entry.error.to_string())
        });
        let applied = world.get_resource::<CommandLog>().is_some_and(|log| {
            log.entries
                .iter()
                .rev()
                .any(|(tick, seq, _)| (*tick, *seq) == key)
        });
        if failed.is_some() || applied {
            let mut state = world.resource_mut::<Ledger>();
            state.message = failed.map_or(success, |error| format!("Not applied: {error}"));
            state.pending = None;
            state.dirty = true;
        }
    }
    sync_mode(world);
    if let Some((revision, message)) = crate::island_control::status(world)
        && revision != world.resource::<Ledger>().control_revision
    {
        let (revision, message) = (revision, message.to_owned());
        let mut state = world.resource_mut::<Ledger>();
        state.control_revision = revision;
        state.message = message;
        state.dirty = true;
    }
}
fn switch_mode(world: &mut World, mode: IslandMode) {
    if let Err(error) = crate::island_control::request_mode(world, mode) {
        world.resource_mut::<Ledger>().message = error;
    }
    sync_mode(world);
}

/// Presentation follows the desired mode; controller receipts live in island_control.
fn sync_mode(world: &mut World) {
    let mode = *world.resource::<IslandMode>();
    if world.resource::<Ledger>().last_mode == Some(mode) {
        return;
    }
    if mode == IslandMode::Character {
        crate::island_camera::character(world);
        crate::homestead_panel::set_open(world, true);
        world.resource_mut::<Ledger>().open = false;
    } else {
        let initial = world.resource::<Ledger>().last_mode.is_none();
        crate::island_camera::overview(world, initial);
        crate::homestead_panel::set_open(world, false);
        let mut state = world.resource_mut::<Ledger>();
        state.open = true;
        state.tab = if mode == IslandMode::Editor {
            Tab::Landscape
        } else {
            Tab::Overview
        };
    }
    if let Some(mut follow) = world.get_resource_mut::<crate::village_world::FollowHero>()
        && follow.enabled != (mode == IslandMode::Character)
    {
        follow.toggle();
    }
    world.resource_mut::<InputFocus>().clear();
    let mut state = world.resource_mut::<Ledger>();
    state.last_mode = Some(mode);
    state.dirty = true;
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}
pub(crate) fn refresh(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let mut state = world.resource_mut::<Ledger>();
        state.elapsed += delta;
        if !state.dirty && state.elapsed < 0.25 {
            return;
        }
        state.dirty = false;
        state.elapsed = 0.0;
    }
    let (open, mode) = (
        world.resource::<Ledger>().open,
        *world.resource::<IslandMode>(),
    );
    let narrow = world
        .query::<&Window>()
        .iter(world)
        .next()
        .is_some_and(|window| window.width() < 1150.0);
    for mut node in world
        .query_filtered::<&mut Node, With<LedgerRoot>>()
        .iter_mut(world)
    {
        node.display = if open { Display::Flex } else { Display::None };
        node.width = px(if narrow { 352.0 } else { 384.0 });
    }
    for mut node in world
        .query_filtered::<&mut Node, With<LedgerLauncher>>()
        .iter_mut(world)
    {
        node.display = if open { Display::None } else { Display::Flex };
    }
    for mut node in world
        .query_filtered::<&mut Node, With<crate::village::Hud>>()
        .iter_mut(world)
    {
        node.display = if mode == IslandMode::Character && !open {
            Display::Flex
        } else {
            Display::None
        };
    }
    let clock = world.resource::<SimClock>();
    let config = world.resource::<SimConfig>();
    let (day, season, _) = village::calendar_at(clock.tick, config, true);
    let control = world.resource::<SimControl>();
    let run_status = if control.is_stopped() {
        "World paused".into()
    } else {
        format!("Village running · {}x speed", control.multiplier)
    };
    let status = format!(
        "Day {} · {:?} · {}\n{}",
        day + 1,
        season,
        if config.is_night(clock.tick) {
            "Night"
        } else {
            "Daylight"
        },
        run_status
    );
    set_text::<LedgerStatus>(world, &status);
    let message = world.resource::<Ledger>().message.clone();
    set_text::<LedgerMessage>(world, &message);
    if open {
        let rows = entries(world);
        reconcile(world, rows);
    }
    let state = world.resource::<Ledger>();
    let regional = village::frontier::enabled(world);
    let limits = if state.surface == TerrainSurface::Water {
        (if regional { -2000 } else { -1000 }, 0)
    } else {
        (0, if regional { 12000 } else { 3000 })
    };
    let disabled: Vec<_> = world
        .query::<(Entity, &LedgerAction)>()
        .iter(world)
        .map(|(entity, action)| {
            let state = world.resource::<Ledger>();
            (
                entity,
                match action.0 {
                    Action::Height(delta) => {
                        if delta < 0 {
                            state.height_mm <= limits.0
                        } else {
                            state.height_mm >= limits.1
                        }
                    }
                    Action::Radius(delta) => {
                        if delta < 0 {
                            state.radius == 0
                        } else {
                            state.radius == if regional { 8 } else { 3 }
                        }
                    }
                    _ => false,
                },
            )
        })
        .collect();
    for (entity, value) in disabled {
        if value {
            world.entity_mut(entity).insert(InteractionDisabled);
        } else {
            world.entity_mut(entity).remove::<InteractionDisabled>();
        }
    }
    if !open
        && world
            .resource::<InputFocus>()
            .get()
            .is_some_and(|entity| world.get::<LedgerControl>(entity).is_some())
    {
        world.resource_mut::<InputFocus>().clear();
    }
}
fn reconcile(world: &mut World, entries: Vec<Entry>) {
    let Some(body) = world
        .query_filtered::<Entity, With<LedgerBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old = std::mem::take(&mut world.resource_mut::<Ledger>().rows);
    let mut live = BTreeMap::new();
    let mut order = Vec::new();
    for entry in entries {
        let entity = old
            .get(&entry.key)
            .copied()
            .filter(|e| world.get_entity(*e).is_ok())
            .unwrap_or_else(|| {
                if let Some(action) = &entry.action {
                    button(world, body, &entry.text, action.clone())
                } else {
                    world
                        .spawn((
                            ChildOf(body),
                            text(
                                &entry.text,
                                if entry.heading { 16.0 } else { 13.0 },
                                if entry.heading { GOLD } else { TEXT },
                            ),
                            Node {
                                margin: UiRect::top(px(if entry.heading { 10 } else { 1 })),
                                flex_shrink: 0.0,
                                ..default()
                            },
                        ))
                        .id()
                }
            });
        if let Some(action) = entry.action {
            world
                .entity_mut(entity)
                .insert((LedgerAction(action), AccessibleLabel::new(&entry.text)));
            let children = world
                .get::<Children>(entity)
                .map(|c| c.to_vec())
                .unwrap_or_default();
            for child in children {
                if let Some(mut text) = world.get_mut::<Text>(child)
                    && text.0 != entry.text
                {
                    text.0 = entry.text.clone();
                }
            }
        } else if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != entry.text
        {
            text.0 = entry.text;
        }
        live.insert(entry.key, entity);
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
    world.resource_mut::<Ledger>().rows = live;
}
fn paint(
    state: Res<Ledger>,
    mode: Res<IslandMode>,
    focus: Res<InputFocus>,
    colony: Res<colony::ColonyState>,
    mut buttons: Query<
        (
            Entity,
            &LedgerAction,
            &Hovered,
            Has<InteractionDisabled>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<LedgerControl>,
    >,
) {
    for (entity, action, hovered, disabled, mut background, mut border) in &mut buttons {
        let selected = match action.0 {
            Action::Mode(value) => value == *mode,
            Action::Tab(value) => value == state.tab,
            Action::Surface(value) => value == state.surface,
            Action::Reserve(value) => value == colony.food_reserve_days,
            _ => false,
        };
        background.0 = if disabled {
            Color::srgb(0.10, 0.12, 0.15)
        } else if hovered.get() {
            Color::srgb(0.27, 0.32, 0.40)
        } else {
            BUTTON
        };
        *border = BorderColor::all(if selected || focus.get() == Some(entity) {
            GOLD
        } else {
            Color::NONE
        });
    }
}
fn typography(
    font: Option<Res<LedgerFont>>,
    mut text: Query<&mut TextFont, With<LedgerTypography>>,
) {
    let Some(font) = font else { return };
    for mut text in &mut text {
        if text.font != bevy::text::FontSource::Handle(font.0.clone()) {
            text.font = font.0.clone().into();
        }
    }
}
fn brush_gizmo(
    mode: Res<IslandMode>,
    state: Res<Ledger>,
    terrain: Option<Res<village::frontier::terrain::TerrainState>>,
    mut gizmos: Gizmos,
) {
    if *mode != IslandMode::Editor {
        return;
    }
    let Some((x, z)) = state.cell else { return };
    let y = if state.surface == TerrainSurface::Water {
        -0.10
    } else {
        state.height_mm as f32 / 1000.0 + 0.04
    };
    let color = if state.preview_error.is_some() {
        Color::srgb(1.0, 0.30, 0.23)
    } else {
        GOLD
    };
    let cells = terrain.as_ref().map_or_else(
        || brush_cells(x, z, state.radius),
        |terrain| regional_brush_cells(terrain.bounds(), x, z, state.radius),
    );
    for (x, z) in cells {
        gizmos.cube(
            Transform::from_xyz(x as f32 + 0.5, y, z as f32 + 0.5)
                .with_scale(Vec3::new(1.0, 0.06, 1.0)),
            color,
        );
    }
}

fn regional_brush_cells(
    bounds: npc_framework::prelude::GridBounds,
    cx: i32,
    cz: i32,
    radius: u32,
) -> Vec<(i32, i32)> {
    let radius = radius.min(8) as i32;
    (-radius..=radius)
        .flat_map(|z| (-radius..=radius).map(move |x| (x, z)))
        .filter(|(x, z)| x * x + z * z <= radius * radius)
        .map(|(x, z)| (cx.saturating_add(x), cz.saturating_add(z)))
        .filter(|(x, z)| bounds.contains(npc_framework::prelude::CellCoord::new(*x, *z)))
        .collect()
}

fn brush_cells(cx: i32, cz: i32, radius: u32) -> Vec<(i32, i32)> {
    let radius = radius.min(3) as i32;
    (-radius..=radius)
        .flat_map(|z| (-radius..=radius).map(move |x| (x, z)))
        .filter(|(x, z)| x * x + z * z <= radius * radius)
        .map(|(x, z)| (cx.saturating_add(x), cz.saturating_add(z)))
        .filter(|(x, z)| (-31..31).contains(x) && (-31..31).contains(z))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_actions;
    use npc_framework::kernel::hash::state_hash;
    use npc_framework::prelude::run_ticks;
    use testbed_content::village::terrain::TerrainCell;
    use testbed_content::village::{Role, Villager};

    fn fixture() -> App {
        let mut app = testbed_content::new_sim_app(7, "island_colony");
        app.init_resource::<Ledger>()
            .init_resource::<IslandMode>()
            .init_resource::<InputFocus>()
            .init_resource::<SimControl>()
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
    fn preview_cells_match_the_authoritative_circular_terraform_brush() {
        use std::collections::BTreeSet;
        for radius in 0..=3 {
            let mut app = fixture();
            let before = app.world().resource::<TerrainState>().cells.clone();
            let command = Terraform {
                x: 14,
                z: 16,
                radius,
                height_mm: 250,
                surface: TerrainSurface::Stone,
            };
            command.perform(app.world_mut()).unwrap();
            let changed: BTreeSet<_> = app
                .world()
                .resource::<TerrainState>()
                .cells
                .iter()
                .zip(&before)
                .enumerate()
                .filter(|(_, (after, before))| after != before)
                .map(|(index, _)| ((index % 64) as i32 - 32, (index / 64) as i32 - 32))
                .collect();
            let preview: BTreeSet<_> = brush_cells(command.x, command.z, radius)
                .into_iter()
                .collect();
            assert_eq!(
                preview, changed,
                "radius {radius} preview must outline exactly the cells the command modifies"
            );
        }
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
                .any(|entry| entry.text.contains("Homes & wellbeing"))
        );
        app.world_mut().resource_mut::<Ledger>().tab = Tab::Residents;
        let rows = entries(app.world());
        assert!(rows.iter().any(|entry| entry.text.contains("Ada")));
        assert!(
            rows.iter()
                .any(|entry| matches!(entry.action, Some(Action::Home(..))))
        );
        assert_eq!(before, state_hash(app.world_mut()));
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
                .starts_with("Cannot apply:")
        );
        assert!(app.world().resource::<Ledger>().pending.is_none());
        assert_eq!(before, state_hash(app.world_mut()));
    }

    #[test]
    fn closing_landscape_tools_ends_painting_without_mutating_the_world() {
        for action in [Action::Close, Action::Toggle] {
            let mut app = fixture();
            switch_mode(app.world_mut(), IslandMode::Editor);
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

    #[test]
    fn terrain_picking_hits_raised_cells_at_their_actual_height() {
        let mut terrain = TerrainState {
            cells: vec![
                TerrainCell {
                    height_mm: 0,
                    surface: TerrainSurface::Grass
                };
                4096
            ],
            revision: 0,
        };
        terrain.cells[32 * 64 + 32].height_mm = 3000;
        // At y=3 the oblique ray hits cell(0,0); at y=0 it would hit cell(1,0).
        let ray = Ray3d::new(
            Vec3::new(-0.8, 6.0, 0.5),
            Dir3::new(Vec3::new(0.4, -1.0, 0.0)).unwrap(),
        );
        assert_eq!(terrain_hit(&terrain, ray), Some((0, 0)));
        assert_eq!(
            terrain_hit(
                &terrain,
                Ray3d::new(Vec3::new(80.0, 10.0, 80.0), Dir3::NEG_Y)
            ),
            None
        );
    }
}
