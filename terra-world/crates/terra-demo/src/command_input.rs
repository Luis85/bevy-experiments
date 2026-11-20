//! Explicit management focus and a bounded controller terrain cursor. These
//! produce the same UI actions, selection and placement intents as the mouse.
use crate::{
    accessibility::ControllerKeyboard,
    control::{ControlMode, ControlSet, PlayerControl, WorldViewport},
    interface::{Action, ActionButton, Panel, UiState},
    player_input::{PlayerInput, PlayerInputSet},
};
use bevy::{
    input::mouse::AccumulatedMouseMotion,
    input_focus::{FocusCause, InputFocus, InputFocusSystems, InputFocusVisible},
    prelude::*,
    ui::{InteractionDisabled, OverflowAxis},
    ui_widgets::Activate,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandCapture;

pub struct CommandInputPlugin;
impl Plugin for CommandInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandFocus>()
            .init_resource::<WorldPointer>()
            .add_systems(Startup, setup_pointer)
            .add_systems(
                PreUpdate,
                capture
                    .in_set(CommandCapture)
                    .after(PlayerInputSet)
                    .before(InputFocusSystems::Dispatch),
            )
            .add_systems(
                Update,
                navigate
                    .after(crate::accessibility::ControllerNavigation)
                    .after(crate::interface::InterfaceLayout)
                    .before(ControlSet::Input),
            )
            .add_systems(
                PostUpdate,
                (reveal_focus, display_pointer).after(bevy::ui::UiSystems::Layout),
            );
    }
}

#[derive(Resource, Default)]
pub struct CommandFocus {
    pub active: bool,
    direction: IVec2,
    repeat_at: f64,
    selected: Option<Entity>,
}
impl CommandFocus {
    fn direction(&mut self, direction: IVec2, now: f64) -> bool {
        let changed = self.direction != direction;
        self.direction = direction;
        if direction == IVec2::ZERO {
            return false;
        }
        if changed || now >= self.repeat_at {
            self.repeat_at = now + if changed { 0.35 } else { 0.14 };
            true
        } else {
            false
        }
    }
}

#[derive(Resource, Default)]
pub struct WorldPointer {
    pub active: bool,
    pub position: Vec2,
    pub select_pressed: bool,
    pub select_held: bool,
    pub select_released: bool,
    pub order_pressed: bool,
    pub additive: bool,
}
impl WorldPointer {
    pub fn cursor(&self, window: &Window) -> Option<Vec2> {
        if self.active {
            Some(self.position)
        } else {
            window.cursor_position()
        }
    }
    fn reset_edges(&mut self) {
        self.select_pressed = false;
        self.select_held = false;
        self.select_released = false;
        self.order_pressed = false;
        self.additive = false;
    }
}

const PAGES: [Panel; 11] = [
    Panel::Activities,
    Panel::Driving,
    Panel::Garage,
    Panel::Fleet,
    Panel::Command,
    Panel::Nature,
    Panel::Territory,
    Panel::Build,
    Panel::Market,
    Panel::Industry,
    Panel::Factions,
];

/// Runs before focused keyboard dispatch and pause handling, so Enter/Space,
/// Escape, pedals and mouse clicks cannot cross between control contexts.
#[allow(clippy::too_many_arguments)]
fn capture(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut input: ResMut<PlayerInput>,
    mut focus: ResMut<CommandFocus>,
    mut pointer: ResMut<WorldPointer>,
    mut keyboard: ResMut<ControllerKeyboard>,
    mut native_focus: ResMut<InputFocus>,
    mut ui: ResMut<UiState>,
    control: Res<PlayerControl>,
    viewport: Res<WorldViewport>,
    mut placement: ResMut<crate::interaction::Placement>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    pointer.reset_edges();
    let Some(window) = windows.iter().next() else {
        return;
    };
    if input.suppress || !window.focused {
        focus.active = false;
        focus.direction = IVec2::ZERO;
        pointer.active = false;
        native_focus.clear();
        return;
    }
    if flow.as_ref().is_some_and(|flow| flow.blocks_input()) {
        focus.active = false;
        pointer.active = false;
        if keyboard.is_world_editor() {
            ui.editing_gps = false;
        }
        return;
    }
    let toggle = input.command_toggle || keys.just_pressed(KeyCode::F6);
    if toggle && !keyboard.is_open() && !ui.editing_gps {
        focus.active = !focus.active;
        focus.direction = IVec2::ZERO;
        pointer.active = false;
        native_focus.clear();
        if focus.active {
            ui.context_panel = true;
            focus.selected = None;
        }
        // Closing is also captured; the same key/button must not control an actor.
        input.ui_navigation = true;
        input.command_consumed = true;
    }
    if focus.active && placement.is_active() {
        focus.active = false;
        pointer.active = input.pad_available;
        pointer.position = viewport.rect(window, false).center();
        native_focus.clear();
        input.ui_navigation = true;
    }
    if ui.editing_gps && (focus.active || keys.just_pressed(KeyCode::F4)) && !keyboard.is_open() {
        keyboard.open_world(ui.input_mode, &ui.gps_text);
        native_focus.clear();
    }
    if keyboard.is_open() {
        input.ui_navigation = true;
        return;
    }
    if focus.active {
        input.ui_navigation = true;
        if input.menu_back || keys.just_pressed(KeyCode::Escape) {
            focus.active = false;
            native_focus.clear();
            input.command_consumed = true;
            input.menu_back = false;
            return;
        }
        let page = input.menu_page + keys.just_pressed(KeyCode::PageDown) as i8
            - keys.just_pressed(KeyCode::PageUp) as i8;
        if page != 0 {
            let current = PAGES.iter().position(|page| *page == ui.panel).unwrap_or(0);
            ui.panel = PAGES[(current as isize + page.signum() as isize)
                .rem_euclid(PAGES.len() as isize) as usize];
            ui.context_panel = true;
            focus.selected = None;
            native_focus.clear();
        }
        if keys.just_pressed(KeyCode::Tab) {
            input.menu_direction =
                if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                    IVec2::NEG_Y
                } else {
                    IVec2::Y
                };
        } else {
            let arrows = IVec2::new(
                keys.pressed(KeyCode::ArrowRight) as i32 - keys.pressed(KeyCode::ArrowLeft) as i32,
                keys.pressed(KeyCode::ArrowDown) as i32 - keys.pressed(KeyCode::ArrowUp) as i32,
            );
            if arrows != IVec2::ZERO {
                input.menu_direction = arrows;
            }
        }
        return;
    }
    if input.command_consumed || ui.editing_gps || control.mode != ControlMode::Rts {
        pointer.active = false;
        return;
    }
    native_focus.clear();
    let had_pointer = pointer.active;
    if !input.pad_available
        || mouse.delta.length_squared() > 0.
        || buttons.get_just_pressed().next().is_some()
    {
        pointer.active = false;
        if had_pointer {
            input.ui_navigation = true;
        }
    } else if input.pad_move.length_squared() > 0.
        || input.pad_look.length_squared() > 0.
        || input.pad_zoom.abs() > 0.
        || input.menu_accept
        || input.menu_delete
        || input.pad_cycle
        || input.menu_shift
    {
        if !pointer.active {
            pointer.position = viewport.rect(window, false).center();
        }
        pointer.active = true;
    }
    if !pointer.active {
        return;
    }
    let rect = viewport.rect(window, false);
    pointer.position = (pointer.position + input.pad_move * time.delta_secs().min(0.1) * 430.)
        .clamp(
            rect.min + Vec2::splat(4.),
            (rect.max - Vec2::splat(4.)).max(rect.min + Vec2::splat(4.)),
        );
    pointer.select_pressed = input.menu_accept;
    pointer.select_held = input.pad_accept_held;
    pointer.select_released = input.pad_accept_released;
    pointer.order_pressed = input.menu_delete;
    pointer.additive = input.pad_additive;
    // In RTS, left stick belongs to the reticle; right stick pans the camera.
    input.movement = Vec2::new(input.pad_look.x, -input.pad_look.y);
    input.look = Vec2::ZERO;
    input.vertical = 0.;
    input.drive = terra_core::DriveInput::stopped();
    input.clear_world_bindings();
    if placement.is_active() && input.menu_back {
        placement.0 = None;
        pointer.reset_edges();
        input.command_consumed = true;
        input.ui_navigation = true;
    }
}

fn path_order(
    entity: Entity,
    parents: &Query<&ChildOf>,
    children: &Query<&Children>,
) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = entity;
    for _ in 0..64 {
        let Ok(parent) = parents.get(current) else {
            break;
        };
        let parent = parent.parent();
        path.push(
            children
                .get(parent)
                .ok()
                .and_then(|list| list.iter().position(|child| child == current))
                .unwrap_or(0),
        );
        current = parent;
    }
    path.push(current.to_bits() as usize);
    path.reverse();
    path
}
fn displayed(entity: Entity, parents: &Query<&ChildOf>, nodes: &Query<&Node>) -> bool {
    let mut current = entity;
    for _ in 0..64 {
        if nodes
            .get(current)
            .is_ok_and(|node| node.display == Display::None)
        {
            return false;
        }
        let Ok(parent) = parents.get(current) else {
            return true;
        };
        current = parent.parent();
    }
    false
}

#[derive(Default)]
struct InspectorReviewFocus {
    revision: u64,
    panel: Option<Panel>,
    open: bool,
    confirm: Option<Entity>,
    return_to: Option<Entity>,
}

#[allow(clippy::too_many_arguments)]
fn navigate(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut state: ResMut<CommandFocus>,
    mut input: ResMut<PlayerInput>,
    keyboard: Res<ControllerKeyboard>,
    mut focus: ResMut<InputFocus>,
    mut focus_visible: ResMut<InputFocusVisible>,
    ui: Res<UiState>,
    mut review_focus: Local<InspectorReviewFocus>,
    buttons: Query<(Entity, &ActionButton, &ComputedNode), Without<InteractionDisabled>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    nodes: Query<&Node>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    if !state.active
        || input.suppress
        || keyboard.is_open()
        || keyboard.consumed
        || flow.as_ref().is_some_and(|flow| flow.blocks_input())
    {
        return;
    }
    let mut candidates: Vec<_> = buttons
        .iter()
        .filter(|(entity, _, node)| !node.is_empty() && displayed(*entity, &parents, &nodes))
        .map(|(entity, action, _)| (entity, action.0, path_order(entity, &parents, &children)))
        .collect();
    candidates.sort_by(|a, b| a.2.cmp(&b.2));
    if candidates.is_empty() {
        focus.clear();
        state.selected = None;
        return;
    }
    let mut previous = focus.get().or(state.selected);
    let review_open = match ui.panel {
        Panel::Factions => crate::communities::has_review(&ui),
        Panel::Build => crate::building_review::has_review(&ui),
        _ => false,
    };
    let review_revision = if ui.panel == Panel::Build {
        ui.building_review.scroll_revision
    } else {
        ui.communities.scroll_revision
    };
    let confirm = candidates
        .iter()
        .find(|(_, action, _)| {
            matches!(
                action,
                Action::Community(crate::communities::CommunityAction::Confirm) if ui.panel == Panel::Factions
            ) || matches!(action, Action::ConfirmDamage if ui.panel == Panel::Build)
        })
        .map(|entry| entry.0);
    let cancel = candidates
        .iter()
        .find(|(_, action, _)| {
            matches!(
                action,
                Action::Community(crate::communities::CommunityAction::Cancel) if ui.panel == Panel::Factions
            ) || matches!(action, Action::CancelDamage if ui.panel == Panel::Build)
        })
        .map(|entry| entry.0);
    let mut review_transition = false;
    if review_open
        && ui.context_panel
        && (review_focus.revision != review_revision || review_focus.panel != Some(ui.panel))
    {
        // A fresh review enters on cancellation, regardless of child order.
        // Wait for Cancel's layout/availability instead of falling back to a
        // committing action or consuming the new-review revision prematurely.
        if let Some(target) = cancel {
            review_focus.return_to = previous;
            review_focus.confirm = confirm;
            review_focus.revision = review_revision;
            review_focus.panel = Some(ui.panel);
            review_focus.open = true;
            previous = Some(target);
            review_transition = true;
        } else {
            input.menu_accept = false;
            if previous.is_some() && (previous == confirm || previous == review_focus.confirm) {
                focus.clear();
                state.selected = None;
            }
            return;
        }
    } else if review_focus.open && !review_open {
        previous = review_focus
            .return_to
            .take()
            .filter(|entity| candidates.iter().any(|entry| entry.0 == *entity));
        review_focus.open = false;
        review_focus.confirm = None;
        review_transition = true;
    } else if review_open
        && previous.is_some()
        && previous == review_focus.confirm
        && confirm.is_none()
    {
        // A stale/expired review cannot retain focus on its now-disabled
        // confirmation. Cancellation stays reachable without forcing focus
        // again after the player deliberately navigates elsewhere.
        if let Some(target) = cancel {
            previous = Some(target);
            review_focus.confirm = None;
            review_transition = true;
        }
    }
    let mut index = previous
        .and_then(|selected| candidates.iter().position(|item| item.0 == selected))
        .unwrap_or_else(|| {
            candidates
                .iter()
                .position(
                    |(_, action, _)| matches!(action, Action::Panel(panel) if *panel == ui.panel),
                )
                .or_else(|| {
                    candidates
                        .iter()
                        .position(|(_, action, _)| matches!(action, Action::TogglePanelNavigation))
                })
                .unwrap_or(0)
        });
    let direction = input.menu_direction;
    if !review_transition && state.direction(direction, time.elapsed_secs_f64()) {
        let delta = if direction.y < 0 || direction.y == 0 && direction.x < 0 {
            -1
        } else {
            1
        };
        index = (index as isize + delta).rem_euclid(candidates.len() as isize) as usize;
    }
    let selected = candidates[index].0;
    focus.set(selected, FocusCause::Navigated);
    focus_visible.0 = true;
    state.selected = Some(selected);
    if review_transition {
        // Opening or closing a review must not replay the activation that
        // triggered the transition onto its newly focused control.
        input.menu_accept = false;
    } else if input.menu_accept {
        commands.trigger(Activate { entity: selected });
        input.menu_accept = false;
    }
}

/// Reveal through the measured bounds of a vertically scrolling ancestor.
/// Node also supplies ScrollPosition to ordinary non-scrolling pages and rows.
fn reveal_focus(
    state: Res<CommandFocus>,
    focus: Res<InputFocus>,
    keyboard: Res<ControllerKeyboard>,
    parents: Query<&ChildOf>,
    bounds: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut scrolls: Query<(&Node, &mut ScrollPosition)>,
) {
    if !state.active || keyboard.is_open() {
        return;
    }
    let Some(entity) = focus.get() else {
        return;
    };
    let Ok((node, transform)) = bounds.get(entity) else {
        return;
    };
    let top = transform.translation.y - node.size().y * 0.5;
    let bottom = top + node.size().y;
    let mut current = entity;
    for _ in 0..64 {
        let Ok(parent) = parents.get(current) else {
            break;
        };
        current = parent.parent();
        let Ok((style, mut scroll)) = scrolls.get_mut(current) else {
            continue;
        };
        if style.overflow.y != OverflowAxis::Scroll {
            continue;
        }
        let Ok((view, transform)) = bounds.get(current) else {
            continue;
        };
        let view_top = transform.translation.y - view.size().y * 0.5 + 3.;
        let view_bottom = transform.translation.y + view.size().y * 0.5 - 3.;
        let correction = if top < view_top {
            top - view_top
        } else if bottom > view_bottom {
            bottom - view_bottom
        } else {
            0.
        };
        let max = (view.content_size().y - view.size().y).max(0.) * view.inverse_scale_factor();
        scroll.y = (scroll.y + correction * view.inverse_scale_factor()).clamp(0., max);
        break;
    }
}

#[derive(Component)]
struct PointerVisual;
fn setup_pointer(mut commands: Commands) {
    commands.spawn((
        PointerVisual,
        GlobalZIndex(80),
        Node {
            position_type: PositionType::Absolute,
            width: px(22),
            height: px(22),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(percent(50)),
            display: Display::None,
            ..default()
        },
        BorderColor::all(crate::interface::ACCENT),
        Pickable::IGNORE,
    ));
}
fn display_pointer(pointer: Res<WorldPointer>, mut nodes: Query<&mut Node, With<PointerVisual>>) {
    for mut node in &mut nodes {
        node.display = if pointer.active {
            Display::Flex
        } else {
            Display::None
        };
        node.left = px(pointer.position.x - 11.);
        node.top = px(pointer.position.y - 11.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture_app() -> App {
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<PlayerInput>()
            .init_resource::<CommandFocus>()
            .init_resource::<WorldPointer>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<UiState>()
            .init_resource::<PlayerControl>()
            .init_resource::<WorldViewport>()
            .init_resource::<crate::interaction::Placement>()
            .add_systems(Update, capture);
        app.world_mut().spawn((
            Window {
                focused: true,
                resolution: (1440, 900).into(),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        app
    }
    #[test]
    fn management_capture_isolates_driving_and_escape_then_restores_rts_cursor() {
        let mut app = capture_app();
        app.world_mut().resource_mut::<PlayerInput>().command_toggle = true;
        app.update();
        assert!(app.world().resource::<CommandFocus>().active);
        assert!(app.world().resource::<PlayerInput>().world_blocked());
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.world_mut().resource_mut::<PlayerInput>().menu_back = true;
        app.update();
        assert!(!app.world().resource::<CommandFocus>().active);
        assert!(app.world().resource::<PlayerInput>().command_consumed);
        assert!(app.world().resource::<PlayerInput>().world_blocked());
        assert!(!app.world().resource::<PlayerInput>().menu_back);

        {
            let mut input = app.world_mut().resource_mut::<PlayerInput>();
            *input = PlayerInput::default();
            input.pad_available = true;
            input.pad_move = Vec2::X;
            input.pad_look = Vec2::Y;
            input.menu_accept = true;
            input.pad_accept_held = true;
            input.pad_additive = true;
            input.menu_delete = true;
            input.release_pressed = true;
            input.drive = terra_core::DriveInput {
                throttle: 1.,
                steering: 1.,
                ..default()
            };
        }
        app.update();
        let pointer = app.world().resource::<WorldPointer>();
        assert!(
            pointer.active
                && pointer.select_pressed
                && pointer.select_held
                && pointer.order_pressed
                && pointer.additive
        );
        let position = pointer.position;
        let window = app
            .world_mut()
            .query::<&Window>()
            .single(app.world())
            .unwrap();
        assert!(
            app.world()
                .resource::<WorldViewport>()
                .contains(window, position, false)
        );
        let input = app.world().resource::<PlayerInput>();
        assert_eq!(input.movement, Vec2::NEG_Y);
        assert_eq!(input.drive.throttle, 0.);
        assert!(!input.release_pressed && !input.world_blocked());

        app.world_mut()
            .resource_mut::<crate::interaction::Placement>()
            .0 = Some(crate::interaction::PlaceKind::Crop);
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.world_mut().resource_mut::<PlayerInput>().pad_available = true;
        app.world_mut().resource_mut::<PlayerInput>().menu_back = true;
        app.update();
        assert!(
            !app.world()
                .resource::<crate::interaction::Placement>()
                .is_active()
        );
        assert!(app.world().resource::<PlayerInput>().command_consumed);
        assert!(!app.world().resource::<WorldPointer>().order_pressed);
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.update();
        assert!(!app.world().resource::<WorldPointer>().active);
        assert!(
            app.world().resource::<PlayerInput>().world_blocked(),
            "disconnect cancels an unfinished drag before mouse control resumes"
        );
    }
    #[test]
    fn command_navigation_skips_hidden_disabled_actions_and_reveals_actual_scroll_bounds() {
        use crate::interface::UiQueue;
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .init_resource::<CommandFocus>()
            .init_resource::<PlayerInput>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .init_resource::<UiState>()
            .init_resource::<UiQueue>()
            .add_systems(Update, (navigate, reveal_focus).chain());
        app.world_mut().resource_mut::<CommandFocus>().active = true;
        let root = app
            .world_mut()
            .spawn((
                Node {
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ComputedNode {
                    size: Vec2::new(200., 100.),
                    content_size: Vec2::new(200., 400.),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
                UiGlobalTransform::from_xy(100., 50.),
                ScrollPosition::default(),
            ))
            .id();
        let button = |app: &mut App, action: Action| {
            app.world_mut()
                .spawn((
                    ActionButton(action),
                    Node::default(),
                    ComputedNode {
                        size: Vec2::new(100., 30.),
                        ..default()
                    },
                    UiGlobalTransform::from_xy(100., 220.),
                    ChildOf(root),
                    bevy::ui_widgets::observe(
                        move |_: On<Activate>, mut queue: ResMut<UiQueue>| {
                            queue.0.push_back(action);
                        },
                    ),
                ))
                .id()
        };
        let first = button(&mut app, Action::Panel(Panel::Activities));
        let disabled = button(&mut app, Action::Noon);
        app.world_mut()
            .entity_mut(disabled)
            .insert(InteractionDisabled);
        let hidden = button(&mut app, Action::Season);
        app.world_mut().get_mut::<Node>(hidden).unwrap().display = Display::None;
        let last = button(&mut app, Action::Speed);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(first));
        assert!(app.world().get::<ScrollPosition>(root).unwrap().y > 0.);
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::Y;
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(last));
        assert!(matches!(
            app.world_mut().resource_mut::<UiQueue>().0.pop_front(),
            Some(Action::Speed)
        ));
        app.update();
        assert!(
            app.world().resource::<UiQueue>().0.is_empty(),
            "held navigation must not replay activation"
        );
    }

    fn community_review_app() -> (App, terra_bevy::SandboxRuntime, Entity, Entity, Entity) {
        use crate::{communities::CommunityAction, interface::UiQueue};
        use terra_bevy::rules::Command;
        let origin = terra_core::GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = terra_bevy::SandboxRuntime::new(origin);
        sandbox.state.add_player(2, origin).unwrap();
        let faction = sandbox
            .state
            .apply(
                1,
                Command::CreateFaction {
                    name: "Focus Commons".into(),
                },
                &sandbox.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::InviteToFaction {
                    faction_id: faction,
                    player_id: 2,
                },
                &sandbox.catalog,
            )
            .unwrap();
        sandbox
            .state
            .apply(
                2,
                Command::AcceptFactionInvite {
                    faction_id: faction,
                },
                &sandbox.catalog,
            )
            .unwrap();
        sandbox.player_id = 2;
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .init_resource::<CommandFocus>()
            .init_resource::<PlayerInput>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .insert_resource(UiState {
                selected_faction: Some(faction),
                panel: Panel::Factions,
                ..Default::default()
            })
            .init_resource::<UiQueue>()
            .add_systems(Update, navigate);
        app.world_mut().resource_mut::<CommandFocus>().active = true;
        let root = app.world_mut().spawn(Node::default()).id();
        let mut button = |action: Action| {
            app.world_mut()
                .spawn((
                    ActionButton(action),
                    Node::default(),
                    ComputedNode {
                        size: Vec2::new(200., 33.),
                        ..default()
                    },
                    ChildOf(root),
                    bevy::ui_widgets::observe(
                        move |_: On<Activate>, mut queue: ResMut<UiQueue>| {
                            queue.0.push_back(action)
                        },
                    ),
                ))
                .id()
        };
        let cancel = button(Action::Community(CommunityAction::Cancel));
        let confirm = button(Action::Community(CommunityAction::Confirm));
        let origin_action = button(Action::Community(CommunityAction::Leave));
        for entity in [confirm, cancel] {
            app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::None;
        }
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(origin_action, FocusCause::Navigated);
        app.update();
        (app, sandbox, origin_action, confirm, cancel)
    }

    fn prepare_community_review(
        app: &mut App,
        sandbox: &mut terra_bevy::SandboxRuntime,
        confirm: Entity,
        cancel: Entity,
    ) {
        crate::communities::act(
            crate::communities::CommunityAction::Leave,
            &mut app.world_mut().resource_mut::<UiState>(),
            sandbox,
        );
        for entity in [confirm, cancel] {
            app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::Flex;
        }
    }

    #[test]
    fn new_community_review_gets_focus_once_without_replaying_accept_and_cancel_returns_to_origin()
    {
        use crate::{communities::CommunityAction, interface::UiQueue};
        let (mut app, mut sandbox, origin, confirm, cancel) = community_review_app();
        prepare_community_review(&mut app, &mut sandbox, confirm, cancel);
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        assert!(
            app.world().resource::<UiQueue>().0.is_empty(),
            "the activation that opened the review must not confirm it"
        );
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::Y;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(confirm));
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::ZERO;
        app.update();
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(confirm),
            "deliberate navigation is not pulled back to Cancel every frame"
        );
        crate::communities::act(
            CommunityAction::Cancel,
            &mut app.world_mut().resource_mut::<UiState>(),
            &mut sandbox,
        );
        for entity in [confirm, cancel] {
            app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::None;
        }
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(origin));
        assert!(
            app.world().resource::<UiQueue>().0.is_empty(),
            "cancel does not activate the restored origin action"
        );
    }

    #[test]
    fn expired_or_unavailable_community_confirmation_leaves_cancel_or_normal_navigation_reachable()
    {
        let (mut app, mut sandbox, origin, confirm, cancel) = community_review_app();
        prepare_community_review(&mut app, &mut sandbox, confirm, cancel);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::Y;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(confirm));
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::ZERO;
        sandbox.restore_revision += 1;
        assert!(crate::communities::unavailable(
            crate::communities::CommunityAction::Confirm,
            &sandbox,
            app.world().resource::<UiState>()
        ));
        app.world_mut()
            .entity_mut(confirm)
            .insert(InteractionDisabled);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        app.world_mut().get_mut::<Node>(cancel).unwrap().display = Display::None;
        app.update();
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(origin),
            "missing review controls must not trap navigation"
        );
    }

    #[test]
    fn building_review_enters_on_cancel_without_replayed_accept_and_keeps_recovery_reachable() {
        use crate::interface::UiQueue;
        use terra_bevy::rules::{Command, Structure};
        let (mut app, _, origin, confirm, cancel) = community_review_app();
        let position = terra_core::GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = terra_bevy::SandboxRuntime::new(position);
        sandbox
            .state
            .seed_living_demo(position, &[], &sandbox.catalog)
            .unwrap();
        let id = 100_001;
        sandbox.state.structures.insert(
            id,
            Structure {
                id,
                owner: 1,
                faction_owner: None,
                blueprint: "fence".into(),
                position,
                hitpoints: 50,
                max_hitpoints: 50,
                radius_m: 1.,
                housing: false,
                workbench: false,
            },
        );
        app.world_mut().get_mut::<ActionButton>(origin).unwrap().0 = Action::Damage;
        app.world_mut().get_mut::<ActionButton>(confirm).unwrap().0 = Action::ConfirmDamage;
        app.world_mut().get_mut::<ActionButton>(cancel).unwrap().0 = Action::CancelDamage;
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.panel = Panel::Build;
            crate::building_review::refresh_displayed(&sandbox, &mut ui);
            let Some(Action::ReviewDamage(token)) = crate::building_review::bind(&ui, false) else {
                panic!("displayed named target")
            };
            crate::building_review::begin(token, &mut ui, &mut sandbox);
        }
        for entity in [confirm, cancel] {
            app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::Flex;
        }
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        assert!(app.world().resource::<UiQueue>().0.is_empty());
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::Y;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(confirm));
        app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::ZERO;
        sandbox
            .state
            .apply(
                1,
                Command::Damage {
                    structure_id: id,
                    amount: 1,
                },
                &sandbox.catalog,
            )
            .unwrap();
        assert!(crate::building_review::unavailable(
            &sandbox,
            app.world().resource::<UiState>(),
            true
        ));
        app.world_mut()
            .entity_mut(confirm)
            .insert(InteractionDisabled);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        crate::building_review::cancel(
            &mut app.world_mut().resource_mut::<UiState>(),
            &mut sandbox,
        );
        for entity in [confirm, cancel] {
            app.world_mut().get_mut::<Node>(entity).unwrap().display = Display::None;
        }
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(origin));
        assert!(app.world().resource::<UiQueue>().0.is_empty());
    }

    #[test]
    fn f6_enters_a_prepared_dissolution_on_cancel_and_waits_until_cancel_is_available() {
        use crate::{communities::CommunityAction, interface::UiQueue};
        use terra_bevy::rules::Command;
        let (mut app, mut sandbox, _, confirm, cancel) = community_review_app();
        let faction = sandbox
            .state
            .apply(
                sandbox.player_id,
                Command::CreateFaction {
                    name: "Unused Focus Commons".into(),
                },
                &sandbox.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        app.world_mut().resource_mut::<UiState>().selected_faction = Some(faction);
        crate::communities::act(
            CommunityAction::Dissolve,
            &mut app.world_mut().resource_mut::<UiState>(),
            &mut sandbox,
        );
        let before = sandbox.state.clone();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<WorldPointer>()
            .init_resource::<PlayerControl>()
            .init_resource::<WorldViewport>()
            .init_resource::<crate::interaction::Placement>()
            .add_systems(PreUpdate, capture);
        app.world_mut().spawn((
            Window {
                focused: true,
                resolution: (1100, 700).into(),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        app.world_mut().resource_mut::<CommandFocus>().active = false;
        // Reproduce a fresh F6 handoff while Confirm has layout and Cancel is
        // still waiting. A transiently available Confirm cannot become default.
        app.world_mut().get_mut::<Node>(confirm).unwrap().display = Display::Flex;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F6);
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert!(app.world().resource::<CommandFocus>().active);
        assert!(app.world().resource::<InputFocus>().get().is_none());
        assert!(app.world().resource::<UiQueue>().0.is_empty());
        assert!(crate::communities::has_review(
            app.world().resource::<UiState>()
        ));

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut().get_mut::<Node>(cancel).unwrap().display = Display::Flex;
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        assert!(
            app.world().resource::<UiQueue>().0.is_empty(),
            "handoff consumes the triggering acceptance"
        );
        assert_eq!(sandbox.state, before);

        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        let queued = app.world_mut().resource_mut::<UiQueue>().0.pop_front();
        assert!(matches!(
            queued,
            Some(Action::Community(CommunityAction::Cancel))
        ));
        assert!(
            crate::communities::act(
                CommunityAction::Cancel,
                &mut app.world_mut().resource_mut::<UiState>(),
                &mut sandbox,
            )
            .is_none()
        );
        assert_eq!(
            sandbox.state, before,
            "Cancel preserves the faction and every ledger"
        );
        assert!(!crate::communities::has_review(
            app.world().resource::<UiState>()
        ));
    }

    #[test]
    fn focused_action_reveals_through_non_scrolling_page_and_horizontal_wrapper() {
        let mut app = App::new();
        app.init_resource::<CommandFocus>()
            .init_resource::<InputFocus>()
            .init_resource::<ControllerKeyboard>()
            .add_systems(Update, reveal_focus);
        app.world_mut().resource_mut::<CommandFocus>().active = true;
        let inspector = app
            .world_mut()
            .spawn((
                Node {
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ComputedNode {
                    size: Vec2::new(200., 100.),
                    content_size: Vec2::new(200., 400.),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
                UiGlobalTransform::from_xy(100., 50.),
            ))
            .id();
        let page = app
            .world_mut()
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(200., 400.),
                    content_size: Vec2::new(200., 400.),
                    ..default()
                },
                UiGlobalTransform::from_xy(100., 200.),
                ChildOf(inspector),
            ))
            .id();
        let row = app
            .world_mut()
            .spawn((
                Node {
                    overflow: Overflow::scroll_x(),
                    ..default()
                },
                ComputedNode {
                    size: Vec2::new(200., 30.),
                    content_size: Vec2::new(400., 30.),
                    ..default()
                },
                UiGlobalTransform::from_xy(100., 220.),
                ChildOf(page),
            ))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionButton(Action::Speed),
                Node::default(),
                ComputedNode {
                    size: Vec2::new(100., 30.),
                    ..default()
                },
                UiGlobalTransform::from_xy(100., 220.),
                ChildOf(row),
            ))
            .id();
        // Node requires ScrollPosition even on ordinary non-scrolling pages.
        assert!(app.world().get::<ScrollPosition>(page).is_some());
        assert!(app.world().get::<ScrollPosition>(row).is_some());
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(action, FocusCause::Navigated);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(inspector).unwrap().y, 69.);
        for container in [page, row] {
            assert_eq!(
                app.world().get::<ScrollPosition>(container).unwrap().0,
                Vec2::ZERO,
                "only the ancestor with vertical scrolling should move"
            );
        }
        // Apply the measured 2x-DPI layout movement. A now-visible action must
        // keep the same scroll instead of continually drifting while focused.
        for (entity, y) in [(page, 62.), (row, 82.), (action, 82.)] {
            *app.world_mut()
                .get_mut::<UiGlobalTransform>(entity)
                .unwrap() = UiGlobalTransform::from_xy(100., y);
        }
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(inspector).unwrap().y, 69.);
    }
    #[test]
    fn direction_repeat_and_pointer_edges_do_not_replay_stale_actions() {
        let mut focus = CommandFocus::default();
        assert!(focus.direction(IVec2::Y, 0.));
        assert!(!focus.direction(IVec2::Y, 0.01));
        assert!(focus.direction(IVec2::Y, 0.36));
        assert!(!focus.direction(IVec2::ZERO, 0.4));
        assert!(focus.direction(IVec2::Y, 0.41));
        let mut pointer = WorldPointer {
            active: true,
            select_held: true,
            order_pressed: true,
            ..default()
        };
        pointer.reset_edges();
        assert!(pointer.active && !pointer.select_held && !pointer.order_pressed);
    }
}
