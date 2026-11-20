//! Local presentation preferences and controller navigation. Text entered with
//! the controller passes through the same bounded native editor as typed text.
use crate::player_input::{InputSettings, PlayerInput};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{
        FocusCause, InputFocus, InputFocusVisible,
        tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation},
    },
    prelude::*,
    text::{EditableText, TextEdit},
    ui::InteractionDisabled,
    ui_widgets::{Activate, Button, observe},
};
use std::collections::VecDeque;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControllerNavigation;

pub struct AccessibleUiPlugin;
impl Plugin for AccessibleUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ControllerKeyboard>()
            .add_message::<KeyboardInput>()
            .add_systems(
                PreUpdate,
                capture_keyboard_focus
                    .after(crate::player_input::PlayerInputSet)
                    .after(crate::command_input::CommandCapture)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch),
            )
            .add_systems(
                Update,
                (navigate, keyboard_view, style_keyboard_focus)
                    .chain()
                    .in_set(ControllerNavigation),
            )
            .add_systems(PostUpdate, scale_text.before(bevy::ui::UiSystems::Content));
    }
}

#[derive(Component)]
struct UnscaledFont(f32);
fn scale_text(
    mut commands: Commands,
    settings: Res<InputSettings>,
    mut fonts: Query<(Entity, &mut TextFont, Option<&UnscaledFont>), With<Node>>,
) {
    for (entity, mut font, original) in &mut fonts {
        if let FontSize::Px(value) = font.font_size {
            let base = original.map_or(value, |font| font.0);
            if original.is_none() {
                commands.entity(entity).insert(UnscaledFont(base));
            }
            let wanted = base * settings.text_scale;
            if (value - wanted).abs() > 0.01 {
                font.font_size = FontSize::Px(wanted);
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum TextKind {
    #[default]
    Text,
    Coordinate,
    Digits,
}
impl TextKind {
    fn accepts(self, value: char) -> bool {
        match self {
            Self::Text => !value.is_control(),
            Self::Coordinate => {
                value.is_ascii_digit() || matches!(value, '.' | ',' | '-' | '+' | ' ')
            }
            Self::Digits => value.is_ascii_digit(),
        }
    }
}
#[derive(Component)]
pub struct ControllerTextField {
    pub label: String,
    pub kind: TextKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyAction {
    Character(usize),
    Delete,
    Clear,
    Shift,
    Apply,
    Cancel,
}
#[derive(Resource, Default)]
pub struct ControllerKeyboard {
    target: Option<Entity>,
    world_target: Option<crate::interface::InputMode>,
    label: String,
    draft: String,
    original: String,
    max: usize,
    kind: TextKind,
    selected: usize,
    upper: bool,
    pending: VecDeque<KeyAction>,
    previous_direction: IVec2,
    repeat_at: f64,
    dirty: bool,
    notice: String,
    /// Consumes the frame that closes text entry before the parent handles Back.
    pub consumed: bool,
}
impl ControllerKeyboard {
    pub fn is_open(&self) -> bool {
        self.target.is_some() || self.world_target.is_some()
    }
    pub fn is_world_editor(&self) -> bool {
        self.world_target.is_some()
    }
    /// Parent actions must not receive the event that edits or closes a draft.
    /// Keep the closing frame protected even after its overlay has disappeared.
    pub(crate) fn blocks_parent_actions(&self) -> bool {
        self.is_open() || self.consumed
    }
    pub fn open_world(&mut self, mode: crate::interface::InputMode, value: &str) {
        use crate::interface::InputMode;
        if self.is_open() {
            return;
        }
        let (label, kind, max) = match mode {
            InputMode::Coordinates => ("Latitude, longitude", TextKind::Coordinate, 64),
            InputMode::FactionName => ("Faction name", TextKind::Text, 48),
            InputMode::CompanyName => ("Company name", TextKind::Text, 32),
            InputMode::DriverName => ("Employee name", TextKind::Text, 32),
            InputMode::RouteName => ("Route name", TextKind::Text, 32),
            InputMode::SettlementName => ("Settlement name", TextKind::Text, 48),
            InputMode::InvitePlayer => ("Player ID to invite", TextKind::Digits, 20),
            InputMode::PromotePlayer => ("Player ID for builder role", TextKind::Digits, 20),
            InputMode::DemotePlayer => ("Player ID for member role", TextKind::Digits, 20),
            InputMode::RevokeInvite => {
                ("Player ID whose invitation to revoke", TextKind::Digits, 20)
            }
            InputMode::RemoveMember => {
                ("Player ID to remove; review follows", TextKind::Digits, 20)
            }
            InputMode::TransferLeader => (
                "Next leader's player ID; review follows",
                TextKind::Digits,
                20,
            ),
        };
        self.world_target = Some(mode);
        self.original = value.into();
        self.draft = value.into();
        self.label = label.into();
        self.kind = kind;
        self.max = max;
        self.selected = 0;
        self.pending.clear();
        self.notice.clear();
        self.dirty = true;
        self.consumed = true;
    }
    fn close(&mut self) {
        self.target = None;
        self.world_target = None;
        self.pending.clear();
        self.draft.clear();
        self.original.clear();
        self.dirty = true;
        self.consumed = true;
    }
    fn direction(&mut self, value: IVec2, now: f64) -> Option<IVec2> {
        let changed = value != self.previous_direction;
        self.previous_direction = value;
        if value == IVec2::ZERO {
            return None;
        }
        if changed || now >= self.repeat_at {
            self.repeat_at = now + if changed { 0.38 } else { 0.14 };
            Some(value)
        } else {
            None
        }
    }
    fn insert(&mut self, index: usize) {
        let Some(mut value) = CHARACTERS.chars().nth(index) else {
            return;
        };
        if self.upper {
            value = value.to_ascii_uppercase();
        }
        if !self.kind.accepts(value) {
            self.notice = "This field does not accept that character.".into();
        } else if self.draft.chars().count() >= self.max {
            self.notice = format!(
                "Maximum {} characters. Delete a character before adding another.",
                self.max
            );
        } else {
            self.draft.push(value);
            self.notice.clear();
        }
    }
}
const CHARACTERS: &str = "1234567890qwertyuiopasdfghjkl-zxcvbnm,.'@:/\\_+=[] ";

fn navigate(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut input: ResMut<PlayerInput>,
    flow: Option<Res<crate::flow::GameFlow>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard: ResMut<ControllerKeyboard>,
    nav: TabNavigation,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
    buttons: Query<Option<&KeyboardFocus>, With<Button>>,
    disabled: Query<(), With<InteractionDisabled>>,
    computed: Query<&ComputedNode>,
    mut fields: Query<(&mut EditableText, Option<&ControllerTextField>)>,
    mut world_ui: Option<ResMut<crate::interface::UiState>>,
    mut world_actions: Option<ResMut<crate::interface::UiQueue>>,
    mut typed: MessageReader<KeyboardInput>,
) {
    keyboard.consumed = false;
    let menu = flow
        .as_ref()
        .is_none_or(|flow| flow.controller_menu_active())
        || keyboard.world_target.is_some();
    let action_focused = focus
        .get()
        .and_then(|entity| buttons.get(entity).ok().flatten())
        .is_some_and(|target| matches!(target, KeyboardFocus::Action(_)));
    // The overlay owns a draft, so native typing must reach that draft instead
    // of disappearing when management focus opens the on-screen keyboard.
    // Drain even while closed/suppressed to prevent stale key replay on reopen.
    for event in typed.read() {
        if !menu
            || input.suppress
            || !keyboard.is_open()
            || event.state != ButtonState::Pressed
            // Space activates a focused keyboard action through ButtonPlugin.
            // It is text only while the draft/character grid owns focus.
            || (action_focused && event.key_code == KeyCode::Space)
            || [
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
                KeyCode::SuperLeft,
                KeyCode::SuperRight,
                KeyCode::AltLeft,
                KeyCode::AltRight,
            ]
            .iter()
            .any(|key| keys.pressed(*key))
        {
            continue;
        }
        let text = match &event.logical_key {
            Key::Character(text) => text.as_str(),
            Key::Space => " ",
            _ => continue,
        };
        for value in text.chars() {
            if keyboard.kind.accepts(value) && keyboard.draft.chars().count() < keyboard.max {
                keyboard.draft.push(value);
                keyboard.dirty = true;
                keyboard.notice.clear();
            }
        }
    }
    if !menu || input.suppress {
        keyboard.previous_direction = IVec2::ZERO;
        keyboard.pending.clear();
        if keyboard.is_open() && !menu {
            keyboard.close();
        }
        return;
    }
    if keyboard.is_open() {
        let target = keyboard.target;
        let world_target = keyboard.world_target;
        if target.is_some_and(|target| !fields.contains(target) || disabled.contains(target))
            || world_target.is_some_and(|mode| {
                world_ui
                    .as_ref()
                    .is_none_or(|ui| !ui.editing_gps || ui.input_mode != mode)
            })
        {
            keyboard.close();
            return;
        }
        // The text draft is private to this overlay until Apply. Back discards it.
        if input.menu_back || keys.just_pressed(KeyCode::Escape) {
            keyboard.pending.push_back(KeyAction::Cancel);
        } else if input.menu_pressed || (keys.just_pressed(KeyCode::Enter) && !action_focused) {
            keyboard.pending.push_back(KeyAction::Apply);
        }
        if input.menu_delete || keys.just_pressed(KeyCode::Backspace) {
            keyboard.pending.push_back(KeyAction::Delete);
        }
        if input.menu_shift {
            keyboard.pending.push_back(KeyAction::Shift);
        }
        let arrows = IVec2::new(
            keys.pressed(KeyCode::ArrowRight) as i32 - keys.pressed(KeyCode::ArrowLeft) as i32,
            keys.pressed(KeyCode::ArrowDown) as i32 - keys.pressed(KeyCode::ArrowUp) as i32,
        );
        let direction = if arrows != IVec2::ZERO {
            arrows
        } else {
            input.menu_direction
        };
        if let Some(direction) = keyboard.direction(direction, time.elapsed_secs_f64()) {
            let x = (keyboard.selected as i32 % 10 + direction.x).rem_euclid(10);
            let y = (keyboard.selected as i32 / 10 + direction.y).rem_euclid(5);
            keyboard.selected = (y * 10 + x) as usize;
            keyboard.dirty = true;
        }
        if input.menu_accept {
            let index = keyboard.selected;
            keyboard.pending.push_back(KeyAction::Character(index));
        }
        for _ in 0..16 {
            let Some(action) = keyboard.pending.pop_front() else {
                break;
            };
            keyboard.dirty = true;
            match action {
                KeyAction::Character(index) => keyboard.insert(index),
                KeyAction::Delete => {
                    keyboard.draft.pop();
                    keyboard.notice.clear();
                }
                KeyAction::Clear => {
                    keyboard.draft.clear();
                    keyboard.notice.clear();
                }
                KeyAction::Shift => keyboard.upper = !keyboard.upper,
                KeyAction::Cancel => {
                    if world_target.is_some() {
                        if let Some(ui) = world_ui.as_mut() {
                            ui.editing_gps = false;
                        }
                    }
                    keyboard.close();
                    if let Some(target) = target {
                        focus.set(target, FocusCause::Navigated);
                    }
                    break;
                }
                KeyAction::Apply => {
                    if let Some(target) = target
                        && let Ok((mut field, _)) = fields.get_mut(target)
                    {
                        // A simultaneous external edit must not be silently overwritten.
                        if field.value().to_string() != keyboard.original {
                            keyboard.notice = "The field changed while the keyboard was open. Go back and reopen it.".into();
                            continue;
                        }
                        field.queue_edit(TextEdit::SelectAll);
                        if keyboard.draft.is_empty() {
                            field.queue_edit(TextEdit::Backspace);
                        } else {
                            field.queue_edit(TextEdit::Insert(keyboard.draft.clone().into()));
                        }
                    }
                    if world_target.is_some() {
                        let Some(ui) = world_ui.as_mut() else {
                            continue;
                        };
                        let Some(queue) = world_actions.as_mut() else {
                            continue;
                        };
                        if ui.gps_text != keyboard.original
                            || ui.input_mode != world_target.unwrap()
                        {
                            keyboard.notice =
                                "The field changed. Discard this draft and reopen the editor."
                                    .into();
                            continue;
                        }
                        if queue.0.len() >= 64 {
                            keyboard.notice =
                                "Commands are busy. Wait, then apply this draft again.".into();
                            continue;
                        }
                        ui.gps_text.clone_from(&keyboard.draft);
                        ui.editing_gps = false;
                        queue.0.push_back(crate::interface::Action::JumpGps);
                    }
                    keyboard.close();
                    if let Some(target) = target {
                        focus.set(target, FocusCause::Navigated);
                    }
                    break;
                }
            }
        }
        input.menu_back = false;
        input.menu_pressed = false;
        input.menu_accept = false;
        return;
    }
    let navigation = keyboard.direction(input.menu_direction, time.elapsed_secs_f64());
    if let Some(direction) = navigation {
        let action = if direction.y < 0 || (direction.y == 0 && direction.x < 0) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        // Skip disabled and display:none controls even if they still have tab indices.
        let mut candidate = focus.clone();
        for _ in 0..256 {
            let Ok(entity) = nav.navigate(&candidate, action) else {
                break;
            };
            candidate.set(entity, FocusCause::Navigated);
            if disabled.contains(entity)
                || computed
                    .get(entity)
                    .is_ok_and(|node| node.size().min_element() <= 0.)
            {
                continue;
            }
            focus.set(entity, FocusCause::Navigated);
            visible.0 = true;
            break;
        }
    }
    if input.menu_accept || keys.just_pressed(KeyCode::F4) {
        let Some(entity) = focus.get() else {
            return;
        };
        if disabled.contains(entity) {
            return;
        }
        if let Ok((field, descriptor)) = fields.get_mut(entity) {
            if field.is_composing() {
                return;
            }
            keyboard.target = Some(entity);
            keyboard.original = field.value().to_string();
            keyboard.draft = keyboard.original.clone();
            keyboard.max = field.max_characters.unwrap_or(256).min(4096);
            keyboard.label =
                descriptor.map_or_else(|| "Edit text".into(), |field| field.label.clone());
            keyboard.kind = descriptor.map_or(TextKind::Text, |field| field.kind);
            keyboard.selected = 0;
            keyboard.notice.clear();
            keyboard.dirty = true;
            focus.clear();
        } else if input.menu_accept && buttons.contains(entity) {
            commands.trigger(Activate { entity });
        }
    }
}

#[derive(Component)]
struct KeyboardOverlay;
/// The draft and its five editing actions form one compact native tab group.
/// Character selection continues to use the existing arrows/controller grid.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum KeyboardFocus {
    Draft,
    Action(KeyAction),
}

fn capture_keyboard_focus(
    keyboard: Res<ControllerKeyboard>,
    input: Res<PlayerInput>,
    mut focus: ResMut<InputFocus>,
    targets: Query<(Entity, &KeyboardFocus)>,
) {
    if input.suppress || !keyboard.is_open() {
        return;
    }
    // Clicking a non-focusable part of the overlay can clear native focus.
    // Reacquire the draft before native dispatch so Tab cannot escape and
    // physical typing cannot edit the underlying field while a draft is open.
    if focus.get().is_none_or(|entity| !targets.contains(entity))
        && let Some((entity, _)) = targets
            .iter()
            .find(|(_, target)| **target == KeyboardFocus::Draft)
    {
        focus.set(entity, FocusCause::Navigated);
    }
}

fn keyboard_text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Node {
            flex_shrink: 0.,
            max_width: percent(100),
            ..default()
        },
    )
}
fn keyboard_button(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    action: KeyAction,
    selected: bool,
) -> Entity {
    let is_space =
        matches!(action, KeyAction::Character(index) if CHARACTERS.chars().nth(index) == Some(' '));
    let mut entity = parent.spawn((
        Button,
        Node {
            // Keep the word label on one line at enlarged text without
            // reducing its font size or widening every single-character key.
            flex_grow: if is_space { 1.5 } else { 1. },
            flex_basis: px(0),
            min_width: px(0),
            min_height: px(32),
            padding: UiRect::all(px(6)),
            border: UiRect::all(px(1)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(5)),
            ..default()
        },
        BackgroundColor(if selected {
            Color::srgb(0.35, 0.50, 0.29)
        } else {
            Color::srgb(0.105, 0.188, 0.16)
        }),
        BorderColor::all(Color::NONE),
        observe(
            move |_: On<Activate>, mut keyboard: ResMut<ControllerKeyboard>| {
                if keyboard.is_open() && keyboard.pending.len() < 16 {
                    keyboard.pending.push_back(action);
                }
            },
        ),
    ));
    if !matches!(action, KeyAction::Character(_)) {
        entity.insert((TabIndex(0), KeyboardFocus::Action(action)));
    }
    entity.with_children(|p| {
        p.spawn(keyboard_text(title, 14., Color::srgb(0.933, 0.945, 0.894)));
    });
    entity.id()
}
fn keyboard_view(
    mut commands: Commands,
    mut keyboard: ResMut<ControllerKeyboard>,
    old: Query<Entity, With<KeyboardOverlay>>,
    targets: Query<&KeyboardFocus>,
    mut focus: ResMut<InputFocus>,
) {
    if !keyboard.dirty {
        return;
    }
    keyboard.dirty = false;
    for entity in &old {
        commands.entity(entity).despawn();
    }
    if !keyboard.is_open() {
        return;
    }
    let retained_focus = focus
        .get()
        .and_then(|entity| targets.get(entity).ok())
        .copied()
        .unwrap_or(KeyboardFocus::Draft);
    let mut next_focus = None;
    commands.spawn((KeyboardOverlay, TabGroup::modal(), GlobalZIndex(1100), Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, padding: UiRect::all(px(16)), ..default() }, BackgroundColor(Color::srgba(0.025, 0.055, 0.045, 0.86)))).with_children(|p| {
        p.spawn((Node { width: px(740), max_width: percent(100), max_height: percent(100), padding: UiRect::all(px(20)), flex_direction: FlexDirection::Column, row_gap: px(8), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(Color::srgb(0.055, 0.095, 0.083)))).with_children(|p| {
            p.spawn(keyboard_text(&keyboard.label, 22., Color::srgb(0.933, 0.945, 0.894)));
            // Bound the visible preview as well as the editor; long paths cannot
            // push the keyboard controls beyond the window.
            let tail: String = keyboard.draft.chars().rev().take(74).collect::<String>().chars().rev().collect();
            let preview = if keyboard.draft.is_empty() { "Empty — choose characters below".into() } else if keyboard.draft.chars().count() > 74 { format!("…{tail}") } else { tail };
            let draft = p.spawn((keyboard_text(preview, 16., Color::srgb(0.702, 0.863, 0.557)), TabIndex(0), KeyboardFocus::Draft, BorderColor::all(Color::NONE))).insert(Node { flex_shrink: 0., max_width: percent(100), border: UiRect::all(px(1)), padding: UiRect::all(px(4)), ..default() }).id();
            if retained_focus == KeyboardFocus::Draft { next_focus = Some(draft); }
            p.spawn(keyboard_text(format!("{} / {} characters · {}", keyboard.draft.chars().count(), keyboard.max, if keyboard.upper { "Capitals" } else { "Lower case" }), 12., Color::srgb(0.698, 0.769, 0.710)));
            for row in 0..5 {
                p.spawn(Node { width: percent(100), column_gap: px(6), flex_shrink: 0., ..default() }).with_children(|p| {
                    for col in 0..10 {
                        let index = row * 10 + col;
                        let mut character = CHARACTERS.chars().nth(index).unwrap();
                        if keyboard.upper { character = character.to_ascii_uppercase(); }
                        let label = if character == ' ' { "Space".into() } else { character.to_string() };
                        keyboard_button(p, &label, KeyAction::Character(index), keyboard.selected == index);
                    }
                });
            }
            p.spawn(Node { width: percent(100), column_gap: px(6), flex_shrink: 0., ..default() }).with_children(|p| {
                for (label, action) in [("Delete", KeyAction::Delete), ("Clear", KeyAction::Clear), ("Capitals", KeyAction::Shift), ("Discard", KeyAction::Cancel), ("Apply", KeyAction::Apply)] {
                    let entity = keyboard_button(p, label, action, false);
                    if retained_focus == KeyboardFocus::Action(action) { next_focus = Some(entity); }
                }
            });
            p.spawn(keyboard_text("Type normally or use D-pad / stick · choose   A · type\nX / Backspace · delete   Start · apply   B / Escape · discard\nTab · editing actions   Enter · apply draft or choose focused action", 12., Color::srgb(0.698, 0.769, 0.710)));
            if !keyboard.notice.is_empty() { p.spawn(keyboard_text(&keyboard.notice, 13., Color::srgb(0.941, 0.612, 0.537))); }
        });
    });
    if let Some(entity) = next_focus {
        focus.set(entity, FocusCause::Navigated);
    }
}

fn style_keyboard_focus(
    focus: Res<InputFocus>,
    mut targets: Query<(Entity, &mut BorderColor), With<KeyboardFocus>>,
) {
    for (entity, mut border) in &mut targets {
        *border = BorderColor::all(if focus.get() == Some(entity) {
            Color::srgb(0.702, 0.863, 0.557)
        } else {
            Color::NONE
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        input_focus::{
            FocusedInput, InputDispatchPlugin, InputFocusPlugin,
            tab_navigation::TabNavigationPlugin,
        },
        ui_widgets::ButtonPlugin,
        window::PrimaryWindow,
    };

    fn native_key(app: &mut App, window: Entity, code: KeyCode, key: Key, state: ButtonState) {
        // This event fixture has no renderer/visibility propagation. Admit the
        // existing UI tree for focus traversal without claiming pixel layout.
        let world = app.world_mut();
        let mut visibility = world.query::<&mut InheritedVisibility>();
        for mut inherited in visibility.iter_mut(world) {
            *inherited = InheritedVisibility::VISIBLE;
        }
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: key,
            state,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    }

    fn tap_key(app: &mut App, window: Entity, code: KeyCode, key: Key) {
        native_key(app, window, code, key.clone(), ButtonState::Pressed);
        native_key(app, window, code, key, ButtonState::Released);
    }

    #[test]
    fn native_keyboard_tab_boundary_blocks_parent_events_and_applies_only_the_draft() {
        use crate::interface::{Action, UiQueue};
        let mut app = App::new();
        app.add_plugins((
            bevy::input::InputPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
            TabNavigationPlugin,
            ButtonPlugin,
            AccessibleUiPlugin,
        ))
        .init_resource::<InputSettings>()
        .init_resource::<PlayerInput>()
        .init_resource::<Time<Real>>()
        .init_resource::<UiQueue>();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let parent = app
            .world_mut()
            .spawn((Node::default(), TabGroup::modal()))
            .id();
        let field = app
            .world_mut()
            .spawn((
                Node::default(),
                EditableText::new(""),
                TabIndex(0),
                ChildOf(parent),
                ControllerTextField {
                    label: "Name".into(),
                    kind: TextKind::Text,
                },
            ))
            .id();
        app.world_mut()
            .commands()
            .entity(parent)
            .with_children(|p| {
                crate::interface::button(p, "Save", Action::Save);
            });
        app.world_mut().flush();
        app.update();
        let parent_button = app
            .world_mut()
            .query_filtered::<Entity, With<crate::interface::ActionButton>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        tap_key(&mut app, window, KeyCode::F4, Key::F4);
        assert!(app.world().resource::<ControllerKeyboard>().is_open());
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(
            app.world().get::<KeyboardFocus>(focused),
            Some(&KeyboardFocus::Draft)
        );
        assert_eq!(
            app.world().get::<TextLayout>(focused).unwrap().linebreak,
            bevy::text::LineBreak::WordOrCharacter
        );
        tap_key(&mut app, window, KeyCode::KeyQ, Key::Character("q".into()));
        assert_eq!(app.world().resource::<ControllerKeyboard>().draft, "q");
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            ""
        );

        // Real native Tab dispatch must stay in the keyboard even when wrapping.
        for _ in 0..12 {
            tap_key(&mut app, window, KeyCode::Tab, Key::Tab);
            let focused = app.world().resource::<InputFocus>().get().unwrap();
            assert!(app.world().get::<KeyboardFocus>(focused).is_some());
        }
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(
            app.world().get::<KeyboardFocus>(focused),
            Some(&KeyboardFocus::Draft)
        );

        // Losing native focus to a background click or an underlying field is
        // repaired before the next event is dispatched.
        for underlying in [None, Some(field)] {
            if let Some(entity) = underlying {
                app.world_mut()
                    .resource_mut::<InputFocus>()
                    .set(entity, FocusCause::Navigated);
            } else {
                app.world_mut().resource_mut::<InputFocus>().clear();
            }
            tap_key(&mut app, window, KeyCode::Tab, Key::Tab);
            let focused = app.world().resource::<InputFocus>().get().unwrap();
            assert_eq!(
                app.world().get::<KeyboardFocus>(focused),
                Some(&KeyboardFocus::Action(KeyAction::Delete))
            );
            native_key(
                &mut app,
                window,
                KeyCode::ShiftLeft,
                Key::Shift,
                ButtonState::Pressed,
            );
            tap_key(&mut app, window, KeyCode::Tab, Key::Tab);
            native_key(
                &mut app,
                window,
                KeyCode::ShiftLeft,
                Key::Shift,
                ButtonState::Released,
            );
        }

        // A stale focused event already addressed to a parent button is rejected
        // at the real ButtonPlugin -> Activate boundary as well.
        for code in [KeyCode::Enter, KeyCode::Space] {
            app.world_mut().trigger(FocusedInput::new(
                parent_button,
                KeyboardInput {
                    key_code: code,
                    logical_key: if code == KeyCode::Enter {
                        Key::Enter
                    } else {
                        Key::Space
                    },
                    state: ButtonState::Pressed,
                    text: None,
                    repeat: false,
                    window,
                },
                window,
            ));
            // Direct World triggers do not apply ButtonPlugin's queued
            // Activate until the command queue is flushed.
            app.world_mut().flush();
        }
        assert!(app.world().resource::<UiQueue>().0.is_empty());

        // Shift+Tab selects Apply. Space activates it; it must not append a
        // space or also activate the now-uncovered parent during this frame.
        native_key(
            &mut app,
            window,
            KeyCode::ShiftLeft,
            Key::Shift,
            ButtonState::Pressed,
        );
        tap_key(&mut app, window, KeyCode::Tab, Key::Tab);
        native_key(
            &mut app,
            window,
            KeyCode::ShiftLeft,
            Key::Shift,
            ButtonState::Released,
        );
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(
            app.world().get::<KeyboardFocus>(focused),
            Some(&KeyboardFocus::Action(KeyAction::Apply))
        );
        native_key(
            &mut app,
            window,
            KeyCode::Space,
            Key::Space,
            ButtonState::Pressed,
        );
        assert!(!app.world().resource::<ControllerKeyboard>().is_open());
        assert!(app.world().resource::<ControllerKeyboard>().consumed);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        app.world_mut().trigger(Activate {
            entity: parent_button,
        });
        assert!(app.world().resource::<UiQueue>().0.is_empty());
        let mut fonts = bevy::text::FontCx::default();
        let mut layout = bevy::text::LayoutCx::default();
        let mut clipboard = Default::default();
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            "q"
        );
        native_key(
            &mut app,
            window,
            KeyCode::Space,
            Key::Space,
            ButtonState::Released,
        );

        // Discard via native Enter preserves the original and restores focus.
        tap_key(&mut app, window, KeyCode::F4, Key::F4);
        tap_key(&mut app, window, KeyCode::KeyZ, Key::Character("z".into()));
        native_key(
            &mut app,
            window,
            KeyCode::ShiftLeft,
            Key::Shift,
            ButtonState::Pressed,
        );
        for _ in 0..2 {
            tap_key(&mut app, window, KeyCode::Tab, Key::Tab);
        }
        native_key(
            &mut app,
            window,
            KeyCode::ShiftLeft,
            Key::Shift,
            ButtonState::Released,
        );
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(
            app.world().get::<KeyboardFocus>(focused),
            Some(&KeyboardFocus::Action(KeyAction::Cancel))
        );
        native_key(
            &mut app,
            window,
            KeyCode::Enter,
            Key::Enter,
            ButtonState::Pressed,
        );
        assert!(!app.world().resource::<ControllerKeyboard>().is_open());
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            "q"
        );
        app.world_mut().trigger(Activate {
            entity: parent_button,
        });
        assert!(app.world().resource::<UiQueue>().0.is_empty());
        native_key(
            &mut app,
            window,
            KeyCode::Enter,
            Key::Enter,
            ButtonState::Released,
        );
        app.world_mut().trigger(FocusedInput::new(
            parent_button,
            KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window,
            },
            window,
        ));
        app.world_mut().flush();
        assert!(matches!(
            app.world().resource::<UiQueue>().0.front(),
            Some(Action::Save)
        ));
    }

    #[test]
    fn world_keyboard_commits_one_action_and_preserves_conflicting_or_busy_drafts() {
        use crate::interface::{Action, InputMode, UiQueue, UiState};
        let mut app = App::new();
        app.init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .init_resource::<UiState>()
            .init_resource::<UiQueue>()
            .add_message::<KeyboardInput>()
            .add_systems(Update, navigate);
        // Keyboard access must remain available when physical gamepads are disabled.
        app.world_mut()
            .resource_mut::<InputSettings>()
            .gamepad_enabled = false;
        let open = |app: &mut App, original: &str, draft: &str| {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.editing_gps = true;
            ui.input_mode = InputMode::CompanyName;
            ui.gps_text = original.into();
            let mut keyboard = app.world_mut().resource_mut::<ControllerKeyboard>();
            keyboard.open_world(InputMode::CompanyName, original);
            keyboard.draft = draft.into();
        };
        open(&mut app, "", "");
        for logical_key in [
            Key::Character("Valley".into()),
            Key::Space,
            Key::Character("Freight".into()),
        ] {
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::KeyV,
                logical_key,
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowRight);
        app.update();
        assert_eq!(
            app.world().resource::<ControllerKeyboard>().draft,
            "Valley Freight"
        );
        assert_eq!(app.world().resource::<ControllerKeyboard>().selected, 1);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<PlayerInput>().menu_pressed = true;
        app.update();
        assert_eq!(app.world().resource::<UiState>().gps_text, "Valley Freight");
        assert!(!app.world().resource::<UiState>().editing_gps);
        assert!(!app.world().resource::<ControllerKeyboard>().is_open());
        assert!(matches!(
            app.world_mut().resource_mut::<UiQueue>().0.pop_front(),
            Some(Action::JumpGps)
        ));
        app.update();
        assert!(app.world().resource::<UiQueue>().0.is_empty());

        open(&mut app, "Original", "Discard me");
        app.world_mut().resource_mut::<PlayerInput>().menu_back = true;
        app.update();
        assert_eq!(app.world().resource::<UiState>().gps_text, "Original");
        assert!(app.world().resource::<UiQueue>().0.is_empty());

        open(&mut app, "Original", "Stale draft");
        app.world_mut().resource_mut::<UiState>().gps_text = "External edit".into();
        app.world_mut().resource_mut::<PlayerInput>().menu_pressed = true;
        app.update();
        assert_eq!(app.world().resource::<UiState>().gps_text, "External edit");
        assert!(app.world().resource::<ControllerKeyboard>().is_open());
        assert!(app.world().resource::<UiQueue>().0.is_empty());

        open(&mut app, "Original", "Retry later");
        app.world_mut()
            .resource_mut::<UiQueue>()
            .0
            .extend(std::iter::repeat_n(Action::Noon, 64));
        app.world_mut().resource_mut::<PlayerInput>().menu_pressed = true;
        app.update();
        assert_eq!(app.world().resource::<UiQueue>().0.len(), 64);
        assert_eq!(app.world().resource::<UiState>().gps_text, "Original");
        assert_eq!(
            app.world().resource::<ControllerKeyboard>().draft,
            "Retry later"
        );
        app.world_mut().resource_mut::<UiQueue>().0.clear();
        app.world_mut().resource_mut::<PlayerInput>().menu_pressed = true;
        app.update();
        assert_eq!(app.world().resource::<UiQueue>().0.len(), 1);
        assert_eq!(app.world().resource::<UiState>().gps_text, "Retry later");
        assert!(TextKind::Coordinate.accepts(','));
        assert!(!TextKind::Coordinate.accepts('a'));
    }
    #[test]
    fn text_preference_applies_once_to_new_controls_and_reverses_without_drift() {
        let mut app = App::new();
        app.init_resource::<InputSettings>()
            .add_systems(Update, scale_text);
        let entity = app
            .world_mut()
            .spawn((
                Node::default(),
                TextFont {
                    font_size: FontSize::Px(14.),
                    ..default()
                },
            ))
            .id();
        app.world_mut().resource_mut::<InputSettings>().text_scale = 1.5;
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(21.)
        );
        app.world_mut().resource_mut::<InputSettings>().text_scale = 1.;
        app.update();
        assert_eq!(
            app.world().get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(14.)
        );
    }
    #[test]
    fn controller_keyboard_is_bounded_filtered_and_cancel_drops_the_draft() {
        assert_eq!(CHARACTERS.chars().count(), 50);
        let mut keyboard = ControllerKeyboard {
            target: Some(Entity::PLACEHOLDER),
            max: 3,
            kind: TextKind::Digits,
            ..default()
        };
        keyboard.insert(10);
        assert!(keyboard.draft.is_empty());
        for _ in 0..10 {
            keyboard.insert(0);
        }
        assert_eq!(keyboard.draft, "111");
        keyboard.close();
        assert!(!keyboard.is_open() && keyboard.draft.is_empty() && keyboard.consumed);
        assert_eq!(keyboard.direction(IVec2::Y, 0.), Some(IVec2::Y));
        assert_eq!(keyboard.direction(IVec2::Y, 0.01), None);
        assert_eq!(keyboard.direction(IVec2::Y, 0.4), Some(IVec2::Y));
        assert_eq!(keyboard.direction(IVec2::ZERO, 0.5), None);
        assert_eq!(keyboard.direction(IVec2::Y, 0.51), Some(IVec2::Y));
    }
    #[test]
    fn controller_text_apply_uses_real_editor_and_back_never_commits() {
        let mut app = App::new();
        app.init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .add_message::<KeyboardInput>()
            .add_systems(Update, navigate);
        let field = app
            .world_mut()
            .spawn((
                EditableText::new(""),
                ControllerTextField {
                    label: "Name".into(),
                    kind: TextKind::Text,
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert!(app.world().resource::<ControllerKeyboard>().is_open());
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        app.world_mut().resource_mut::<ControllerKeyboard>().draft = "Mika Rivers".into();
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.world_mut().resource_mut::<PlayerInput>().menu_pressed = true;
        app.update();
        assert!(!app.world().resource::<ControllerKeyboard>().is_open());
        assert!(!app.world().resource::<PlayerInput>().menu_pressed);
        let mut fonts = bevy::text::FontCx::default();
        let mut layout = bevy::text::LayoutCx::default();
        let mut clipboard = Default::default();
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            "Mika Rivers"
        );
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        app.world_mut().resource_mut::<ControllerKeyboard>().draft = "Discard this".into();
        *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
        app.world_mut().resource_mut::<PlayerInput>().menu_back = true;
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            "Mika Rivers"
        );
        assert!(!app.world().resource::<PlayerInput>().menu_back);
        assert!(app.world().resource::<ControllerKeyboard>().consumed);
    }
    #[test]
    fn disabled_text_and_focus_loss_do_not_open_or_replay_queued_edits() {
        let mut app = App::new();
        app.init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ControllerKeyboard>()
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .add_message::<KeyboardInput>()
            .add_systems(Update, navigate);
        let field = app
            .world_mut()
            .spawn((EditableText::new(""), InteractionDisabled))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
        app.update();
        assert!(!app.world().resource::<ControllerKeyboard>().is_open());
        app.world_mut()
            .entity_mut(field)
            .remove::<InteractionDisabled>();
        app.update();
        assert!(app.world().resource::<ControllerKeyboard>().is_open());
        {
            let mut keyboard = app.world_mut().resource_mut::<ControllerKeyboard>();
            keyboard.draft = "Preserve draft".into();
            keyboard
                .pending
                .extend([KeyAction::Character(0), KeyAction::Apply]);
        }
        app.world_mut().resource_mut::<PlayerInput>().suppress = true;
        app.update();
        let keyboard = app.world().resource::<ControllerKeyboard>();
        assert!(keyboard.pending.is_empty() && keyboard.is_open());
        assert_eq!(keyboard.draft, "Preserve draft");
        assert!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string()
                .is_empty()
        );
    }
}
