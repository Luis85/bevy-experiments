use super::{Action, Actions, MenuState, Screen, preview::PreviewImage, recovery};
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    input_focus::{
        AutoFocus, FocusedInput, InputFocus,
        tab_navigation::{TabGroup, TabIndex},
    },
    picking::hover::Hovered,
    prelude::*,
    text::{EditableText, EditableTextFilter, TextCursorStyle, TextEdit},
    ui::{InteractionDisabled, Pressed},
    ui_widgets::{Activate, Button, TextInput, observe},
};

const PANEL: Color = Color::srgba(0.055, 0.095, 0.083, 0.97);
const SURFACE: Color = Color::srgb(0.105, 0.188, 0.16);
const HOVER: Color = Color::srgb(0.165, 0.282, 0.231);
const SELECTED: Color = Color::srgb(0.22, 0.357, 0.263);
const TEXT: Color = Color::srgb(0.933, 0.945, 0.894);
const MUTED: Color = Color::srgb(0.698, 0.769, 0.710);
const ACCENT: Color = Color::srgb(0.702, 0.863, 0.557);
const ERROR: Color = Color::srgb(0.941, 0.612, 0.537);

#[cfg(test)]
mod text_tests {
    use super::*;
    #[test]
    fn launcher_button_rejects_native_activation_during_text_entry_and_its_closing_frame() {
        use crate::{
            accessibility::ControllerKeyboard,
            player_input::{InputSettings, PlayerInput},
        };
        let mut app = App::new();
        app.init_resource::<Actions>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ControllerKeyboard>()
            .add_plugins(bevy::ui_widgets::ButtonPlugin);
        app.world_mut()
            .commands()
            .spawn(Node::default())
            .with_children(|p| button(p, "Create world and play", Action::Create, true, None));
        app.world_mut().flush();
        app.update();
        let target = app
            .world_mut()
            .query_filtered::<Entity, With<ActionButton>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .resource_mut::<ControllerKeyboard>()
            .open_world(crate::interface::InputMode::Coordinates, "");
        for closing in [false, true] {
            if closing {
                *app.world_mut().resource_mut::<ControllerKeyboard>() =
                    ControllerKeyboard::default();
                app.world_mut()
                    .resource_mut::<ControllerKeyboard>()
                    .consumed = true;
            }
            for (key_code, logical_key) in
                [(KeyCode::Enter, Key::Enter), (KeyCode::Space, Key::Space)]
            {
                app.world_mut().trigger(FocusedInput::new(
                    target,
                    KeyboardInput {
                        key_code,
                        logical_key,
                        state: bevy::input::ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: target,
                    },
                    target,
                ));
                // ButtonPlugin queues Activate through Commands. Flush before
                // changing the guard state or inspecting its action queue.
                app.world_mut().flush();
            }
            assert!(app.world().resource::<Actions>().0.is_empty());
        }
        app.world_mut()
            .resource_mut::<ControllerKeyboard>()
            .consumed = false;
        app.world_mut().trigger(FocusedInput::new(
            target,
            KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: bevy::input::ButtonState::Pressed,
                text: None,
                repeat: false,
                window: target,
            },
            target,
        ));
        app.world_mut().flush();
        assert_eq!(
            app.world().resource::<Actions>().0.front(),
            Some(&Action::Create)
        );
    }

    #[test]
    fn captured_space_does_not_reactivate_the_native_binding_button_but_mouse_back_works() {
        use crate::player_input::{Binding, InputSettings, PlayerInput};
        let mut app = App::new();
        app.init_resource::<Actions>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Startup, |mut commands: Commands| {
                commands
                    .spawn(Node::default())
                    .with_children(|p| button(p, "Back", Action::Home, false, None));
            });
        app.update();
        let button = app
            .world_mut()
            .query_filtered::<Entity, With<ActionButton>>()
            .single(app.world())
            .unwrap();
        app.world_mut().resource_mut::<InputSettings>().listening = Some(Binding::Forward);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.world_mut().trigger(Activate { entity: button });
        assert!(app.world().resource::<Actions>().0.is_empty());
        // Sampling successfully captured Space, but the same frame is consumed.
        app.world_mut().resource_mut::<InputSettings>().listening = None;
        app.world_mut().resource_mut::<PlayerInput>().suppress = true;
        app.world_mut().trigger(Activate { entity: button });
        assert!(app.world().resource::<Actions>().0.is_empty());
        // Mouse activation while waiting for another key can cancel the editor.
        app.world_mut().resource_mut::<InputSettings>().listening = Some(Binding::Forward);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().trigger(Activate { entity: button });
        assert_eq!(
            app.world().resource::<Actions>().0.front(),
            Some(&Action::Home)
        );
    }
    #[test]
    fn native_named_space_and_shift_space_preserve_words_without_duplicate_insertion() {
        for (shift, control, text, expected) in [
            (false, false, None, "A B"),
            (true, false, Some(" "), "A B"),
            // The standard widget already owns this ordinary text event.
            (false, false, Some(" "), "AB"),
            (false, true, None, "AB"),
        ] {
            let mut app = App::new();
            app.init_resource::<ButtonInput<Key>>()
                .add_observer(complete_space_input);
            let field = app
                .world_mut()
                .spawn((Field::WorldName, EditableText::new("")))
                .id();
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .queue_edit(TextEdit::Insert("A".into()));
            if shift {
                app.world_mut()
                    .resource_mut::<ButtonInput<Key>>()
                    .press(Key::Shift);
            }
            if control {
                app.world_mut()
                    .resource_mut::<ButtonInput<Key>>()
                    .press(Key::Control);
            }
            app.world_mut().trigger(FocusedInput::new(
                field,
                KeyboardInput {
                    key_code: KeyCode::Space,
                    logical_key: Key::Space,
                    state: bevy::input::ButtonState::Pressed,
                    text: text.map(Into::into),
                    repeat: false,
                    window: field,
                },
                field,
            ));
            let mut input = app.world_mut().get_mut::<EditableText>(field).unwrap();
            input.queue_edit(TextEdit::Insert("B".into()));
            let mut fonts = bevy::text::FontCx::default();
            let mut layout = bevy::text::LayoutCx::default();
            let mut clipboard = Default::default();
            input.apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
            assert_eq!(input.value().to_string(), expected);
        }
    }
}

/// Bevy 0.20's text widget handles an unmodified Space only when the backend
/// supplies text. X11 can supply a named Space without text, and Shift may be
/// held while typing the next capital. Complete just these unhandled cases;
/// the widget remains responsible for ordinary text, selection, limits and IME.
pub(super) fn complete_space_input(
    mut event: On<FocusedInput<KeyboardInput>>,
    keys: Res<ButtonInput<Key>>,
    mut fields: Query<&mut EditableText, With<Field>>,
) {
    if event.input.logical_key != Key::Space
        || !event.input.state.is_pressed()
        || [Key::Control, Key::Alt, Key::Super]
            .iter()
            .any(|key| keys.pressed(key.clone()))
        || (!keys.pressed(Key::Shift)
            && event
                .input
                .text
                .as_ref()
                .is_some_and(|text| !text.is_empty()))
    {
        return;
    }
    if let Ok(mut field) = fields.get_mut(event.focused_entity) {
        if field.is_composing() {
            return;
        }
        field.queue_edit(TextEdit::Insert(" ".into()));
        event.propagate(false);
    }
}

#[derive(Component)]
pub(super) struct MenuRoot;
#[derive(Component)]
pub(super) struct ActionButton {
    primary: bool,
}
#[derive(Component)]
pub(super) struct CoverageLabel;
#[derive(Component, Clone, Copy)]
pub(super) enum Field {
    CharacterName,
    WorldName,
    RenameWorld,
    RecoveryTitle,
    Latitude,
    Longitude,
    Seed,
    Address,
    Session,
    TlsCa,
    TlsServerName,
    SourceConfig,
    BackupPath,
}

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        // Paths and backend errors can contain no whitespace. Keep them inside
        // their column while retaining the complete detail for scrolling.
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
fn button(
    parent: &mut ChildSpawnerCommands,
    title: impl Into<String>,
    action: Action,
    primary: bool,
    focus: Option<Action>,
) {
    let mut entity = parent.spawn((
        Button,
        TabIndex(0),
        ActionButton { primary },
        Hovered::default(),
        Node {
            width: percent(100),
            min_height: px(40),
            padding: UiRect::axes(px(14), px(10)),
            border: UiRect::all(px(1)),
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(5)),
            flex_shrink: 0.,
            ..default()
        },
        BackgroundColor(if primary { SELECTED } else { SURFACE }),
        BorderColor::all(Color::NONE),
        observe(
            move |_: On<Activate>,
                  mut actions: ResMut<Actions>,
                  settings: Res<crate::player_input::InputSettings>,
                  input: Res<crate::player_input::PlayerInput>,
                  keys: Res<ButtonInput<KeyCode>>,
                  keyboard: Option<Res<crate::accessibility::ControllerKeyboard>>| {
                if keyboard
                    .as_ref()
                    .is_some_and(|keyboard| keyboard.blocks_parent_actions())
                    || crate::player_input::capture_consumes_activation(&settings, &input, &keys)
                {
                    return;
                }
                if actions.0.len() < 32 {
                    actions.0.push_back(action);
                }
            },
        ),
    ));
    if focus == Some(action) {
        entity.insert(AutoFocus);
    }
    entity.with_children(|p| {
        let mut label = p.spawn(text(title, 14., TEXT));
        if action == Action::Radius {
            label.insert(CoverageLabel);
        }
    });
}
fn field(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: &str,
    key: Field,
    max: usize,
    autofocus: bool,
) {
    parent
        .spawn(Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            flex_shrink: 0.,
            ..default()
        })
        .with_children(|p| {
            p.spawn(text(label, 13., MUTED));
            let mut editable = EditableText::new(value);
            editable.max_characters = Some(max);
            editable.allow_newlines = false;
            let mut entity = p.spawn((
                key,
                crate::accessibility::ControllerTextField {
                    label: label.into(),
                    kind: match key {
                        Field::Latitude | Field::Longitude => {
                            crate::accessibility::TextKind::Coordinate
                        }
                        Field::Seed => crate::accessibility::TextKind::Digits,
                        _ => crate::accessibility::TextKind::Text,
                    },
                },
                TextInput,
                editable,
                TabIndex(0),
                TextLayout::no_wrap(),
                TextFont {
                    font_size: FontSize::Px(16.),
                    ..default()
                },
                TextColor(TEXT),
                TextCursorStyle {
                    color: ACCENT,
                    selection_color: SELECTED,
                    unfocused_selection_color: SURFACE,
                    selected_text_color: Some(TEXT),
                    selection_radius: 0.1,
                },
                EditableTextFilter::new(move |c| match key {
                    Field::Latitude | Field::Longitude => {
                        c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | ' ')
                    }
                    Field::Seed => c.is_ascii_digit(),
                    _ => !c.is_control(),
                }),
                Node {
                    width: percent(100),
                    min_height: px(40),
                    padding: UiRect::axes(px(12), px(8)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.075, 0.14, 0.12)),
                BorderColor::all(Color::srgb(0.3, 0.43, 0.35)),
            ));
            if autofocus {
                entity.insert(AutoFocus);
            }
        });
}
fn row(parent: &mut ChildSpawnerCommands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            width: percent(100),
            column_gap: px(12),
            flex_shrink: 0.,
            ..default()
        })
        .with_children(build);
}
fn column(
    parent: &mut ChildSpawnerCommands,
    width: f32,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            width: percent(width),
            flex_direction: FlexDirection::Column,
            row_gap: px(12),
            flex_shrink: 0.,
            ..default()
        })
        .with_children(build);
}
fn enabled(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn created_label(seconds: u64) -> String {
    if seconds == 0 || seconds > 253_402_300_799 {
        return "Creation date unavailable".into();
    }
    let stamp = terra_core::WorldClock {
        unix_seconds: seconds as f64,
        ..default()
    }
    .label();
    format!(
        "Created {}:{:02} UTC",
        stamp.trim_end_matches(" UTC"),
        seconds % 60
    )
}
fn footer(
    parent: &mut ChildSpawnerCommands,
    back: Action,
    next: Option<(&str, Action)>,
    focus: Option<Action>,
) {
    row(parent, |p| {
        column(p, 32., |p| button(p, "Back", back, false, focus));
        if let Some((title, action)) = next {
            column(p, 66., |p| button(p, title, action, true, focus));
        }
    });
}

pub fn sync_fields(
    inputs: Query<(&Field, &EditableText), Changed<EditableText>>,
    mut menu: ResMut<MenuState>,
    mut coverage_labels: Query<&mut Text, With<CoverageLabel>>,
) {
    for (field, editable) in &inputs {
        let value = editable.value().to_string();
        match field {
            Field::CharacterName => menu.draft.character.name = value,
            Field::WorldName => menu.draft.world_name = value,
            Field::RenameWorld => menu.world_name_draft = value,
            Field::RecoveryTitle => {
                if let Some(review) = menu.recovery_review.as_mut() {
                    review.restored_title = value;
                }
            }
            Field::Latitude => menu.draft.latitude = value,
            Field::Longitude => menu.draft.longitude = value,
            Field::Seed => menu.draft.seed = value,
            Field::Address => menu.draft.address = value,
            Field::Session => menu.draft.session_name = value,
            Field::TlsCa => menu.draft.tls_ca = value,
            Field::TlsServerName => menu.draft.tls_server_name = value,
            Field::SourceConfig => menu.draft.source_config = value,
            Field::BackupPath => menu.backup_path = value,
        }
    }
    // Updating this dependent value must not rebuild the active text control,
    // which would discard its cursor and keyboard selection during editing.
    if !coverage_labels.is_empty() {
        let value = format!("{}   /   Change", menu.draft.requested_coverage_label());
        for mut label in &mut coverage_labels {
            if label.0 != value {
                label.0.clone_from(&value);
            }
        }
    }
}

pub fn rebuild(
    mut commands: Commands,
    mut menu: ResMut<MenuState>,
    old: Query<Entity, With<MenuRoot>>,
    preview: Option<Res<PreviewImage>>,
    input_settings: Res<crate::player_input::InputSettings>,
    input: Res<crate::player_input::PlayerInput>,
    mut focus: ResMut<InputFocus>,
) {
    if !menu.dirty || preview.is_none() {
        return;
    }
    menu.dirty = false;
    focus.clear();
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let preview = preview.unwrap();
    let wide = !matches!(menu.screen, Screen::Home | Screen::Launching);
    let title = match menu.screen {
        Screen::Home => "Terra",
        Screen::Worlds => "Your worlds",
        Screen::RenameWorld => "Rename your world",
        Screen::Backups => "Backups and recovery",
        Screen::RecoveryReview => "Recover a separate world",
        Screen::Character => "Create your character",
        Screen::World => "Shape your world",
        Screen::Review => "Ready to begin",
        Screen::Join => "Join a server",
        Screen::Settings => "Settings",
        Screen::Controls => "Controls",
        Screen::Launching => "Starting your world",
    };
    commands.spawn((MenuRoot, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), padding: UiRect::all(px(32)), align_items: AlignItems::Center, justify_content: if wide { JustifyContent::Center } else { JustifyContent::Start }, ..default() }, BackgroundColor(Color::srgba(0.025, 0.055, 0.045, 0.32)))).with_children(|root| {
        root.spawn((Node { width: if wide { percent(94) } else { px(416) }, max_width: px(1080), max_height: percent(100), padding: UiRect::all(px(24)), flex_direction: FlexDirection::Column, row_gap: px(16), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(PANEL), TabGroup::modal())).with_children(|p| {
            p.spawn(text(title, if menu.screen == Screen::Home { 42. } else { 29. }, TEXT));
            let subtitle = match menu.screen {
                Screen::Home => "Explore, build, drive and make a living.",
                Screen::Worlds => "Each world keeps its own character, settings and durable progress.",
                Screen::RenameWorld => "Give this saved world a name you can recognize when you return.",
                Screen::Backups => "Keep a saved checkpoint with the source data needed to reopen it.",
                Screen::RecoveryReview => "Review the named backup before creating a separate, playable world.",
                Screen::Character => "Choose how your persistent avatar appears in the world.",
                Screen::World => "Local scenario data keeps the first session reproducible. More providers are available through the SDK and command line.",
                Screen::Review => "Create a separate world. Existing worlds and saves stay intact.",
                Screen::Join => "The server owns progress, physics and the shared economy.",
                Screen::Settings => "Defaults for new worlds and multiplayer clients. Each existing world keeps its own presentation settings.",
                Screen::Controls => "Switch between commanding your crew and controlling an actor.",
                Screen::Launching => "Opening the world and preparing its simulation…",
            };
            p.spawn(text(subtitle, 14., MUTED));
            crate::flow::scroll_panel(p, 66., 14., |p| {
                if let Some(progress) = &menu.recovery_progress { p.spawn(text(progress, 15., ACCENT)); }
                if let Some(notice) = &menu.notice { p.spawn(text(notice, 14., ACCENT)); }
                // Status belongs to the bounded content, never to the fixed
                // footer: long connection/path errors must not displace Back.
                if let Some(error) = &menu.error { p.spawn(text(error, 14., ERROR)); }
                if matches!(menu.screen, Screen::Home | Screen::Worlds | Screen::RenameWorld | Screen::Backups | Screen::RecoveryReview) {
                    if let Some(warning) = &menu.warning { p.spawn(text(warning, 12., MUTED)); }
                }
                match menu.screen {
                    Screen::Home => {
                        if let Some(world) = menu.recent.first() { button(p, format!("Continue · {}", world.profile.title), Action::Continue(0, menu.listing_revision), true, menu.focus.or(Some(Action::Continue(0, menu.listing_revision)))); }
                        button(p, "New world", Action::NewWorld, menu.recent.is_empty(), menu.focus.or(if menu.recent.is_empty() { Some(Action::NewWorld) } else { None }));
                        button(p, "Your worlds", Action::Worlds, false, menu.focus);
                        button(p, "Multiplayer", Action::Join, false, menu.focus);
                        row(p, |p| { column(p, 49., |p| button(p, "Settings", Action::Settings, false, menu.focus)); column(p, 49., |p| button(p, "Controls", Action::Controls, false, menu.focus)); });
                        button(p, "Quit", Action::Quit, false, menu.focus);
                        if menu.recent.is_empty() { p.spawn(text("No saved worlds yet. New world takes you through character creation and world settings.", 13., MUTED)); }
                    }
                    Screen::Worlds => {
                        button(p, "Backups and recovery", Action::Backups, false, menu.focus);
                        if menu.recent.is_empty() { p.spawn(text("No managed worlds found. Create a new world to begin; existing standalone JSON saves remain available through the command line.", 15., MUTED)); }
                        for (index, world) in menu.recent.iter().enumerate() {
                            let unavailable = recovery::unavailable(world);
                            row(p, |p| {
                                column(p, 71., |p| {
                                    button(p, format!("{}\n{} · {} · {}\n{}", world.profile.title, world.profile.character.name, world.profile.source_label(), if world.has_checkpoint_file { "Open existing world" } else { "Finish first launch" }, created_label(world.profile.created_unix_seconds)), Action::Continue(index, menu.listing_revision), index == 0, menu.focus.or(if index == 0 { Some(Action::Continue(0, menu.listing_revision)) } else { None }));
                                });
                                column(p, 27., |p| {
                                    button(p, "Rename", Action::RenameWorld(index, menu.listing_revision), false, menu.focus);
                                    if unavailable.is_none() { button(p, "Make backup", Action::BackupWorld(index, menu.listing_revision), false, menu.focus); }
                                });
                            });
                            if let Some(reason) = unavailable { p.spawn(text(format!("Backup unavailable · {reason}"), 12., MUTED)); }
                        }
                        row(p, |p| { column(p, 49., |p| button(p, "Refresh", Action::Refresh, false, menu.focus)); column(p, 49., |p| button(p, "New world", Action::NewWorld, true, menu.focus)); });
                    }
                    Screen::RenameWorld => {
                        if let Some(world) = &menu.rename_selection {
                            p.spawn(text(&world.profile.title, 22., TEXT));
                            p.spawn(text(format!("{} · {}\n{}", world.profile.character.name, world.profile.source_label(), created_label(world.profile.created_unix_seconds)), 14., MUTED));
                            field(p, "World name", &menu.world_name_draft, Field::RenameWorld, 48, matches!(menu.focus, Some(Action::RenameWorld(_, _))));
                            p.spawn(text("Choose a name that distinguishes this copy from your other worlds. Saving changes its menu name; progress, character, settings and existing backups stay as they are.", 14., MUTED));
                        } else { p.spawn(text("Return to Your worlds and choose the world to rename.", 15., MUTED)); }
                    }
                    Screen::Backups => {
                        if menu.backups.is_empty() { p.spawn(text("No completed backups found here. In Your worlds, use Make backup after saving a supported local regional world.", 15., MUTED)); }
                        for (index, backup) in menu.backups.iter().enumerate() {
                            let when = terra_core::WorldClock { unix_seconds: backup.record.created_unix_seconds as f64, ..default() }.label();
                            button(p, format!("Verify · {}\nCheckpoint {} · {} · {}", backup.record.title, backup.record.source_generation, backup.record.character.name, when), Action::ReviewBackup(index, menu.listing_revision), index == 0, menu.focus.or(if index == 0 { Some(Action::ReviewBackup(0, menu.listing_revision)) } else { None }));
                        }
                        button(p, "Refresh backups", Action::RefreshBackups, false, menu.focus);
                        if menu.recovery_progress.is_none() {
                            field(p, "Backup folder from another location", &menu.backup_path, Field::BackupPath, 1024, false);
                            button(p, "Verify this folder", Action::InspectBackupPath, false, menu.focus);
                            p.spawn(text("Choose a complete folder created by Make backup, including its world-backup.json record and bundle folder. Verification opens a review; it does not restore or launch a world.", 13., MUTED));
                        }
                        p.spawn(text("Local backups contain private world data. Keep a separate protected copy if you need recovery after losing this device. Terra does not remove older backups automatically.", 13., MUTED));
                    }
                    Screen::RecoveryReview => {
                        if let Some(review) = &menu.recovery_review {
                            let record = &review.backup.record;
                            let when = terra_core::WorldClock { unix_seconds: record.created_unix_seconds as f64, ..default() }.label();
                            p.spawn(text(&record.title, 22., TEXT));
                            p.spawn(text(format!("Checkpoint {} · backed up {}", review.verified.source_generation, when), 15., ACCENT));
                            field(p, "Recovered world name", &review.restored_title, Field::RecoveryTitle, 48, false);
                            p.spawn(text("Use a distinct name to recognize this copy later. You can also rename it in Your worlds.", 13., MUTED));
                            p.spawn(text(format!("Character · {}\nGraphics · {}\nSource · {}", record.character.name, record.quality.quality.label(), if review.verified.prepared_source { "Prepared local terrain" } else { "Synthetic frontier" }), 15., TEXT));
                            p.spawn(text("The complete saved world, character and presentation settings will be recovered into a new folder. Existing worlds and this backup stay intact. Open the recovered world from Your worlds when ready.", 14., MUTED));
                            p.spawn(text(format!("Verified local copy · {:.1} MiB", review.verified.total_bytes as f64 / (1024. * 1024.)), 13., MUTED));
                            p.spawn(text(format!("Backup folder · {}", review.backup.directory.display()), 12., MUTED));
                        } else { p.spawn(text("Select and verify a backup to review its checkpoint.", 15., MUTED)); }
                    }
                    Screen::Character => {
                        row(p, |p| {
                            column(p, 53., |p| {
                                field(p, "Character name", &menu.draft.character.name, Field::CharacterName, 32, menu.focus.is_none_or(|action| matches!(action, Action::NewWorld | Action::Character)));
                                button(p, format!("Build · {}", crate::character::BODY_NAMES[menu.draft.character.body_style as usize]), Action::Body, false, menu.focus);
                                button(p, format!("Skin tone · {}", crate::character::SKIN_NAMES[menu.draft.character.skin as usize]), Action::Skin, false, menu.focus);
                                button(p, format!("Hair color · {}", crate::character::HAIR_NAMES[menu.draft.character.hair as usize]), Action::Hair, false, menu.focus);
                                button(p, format!("Outfit · {}", crate::character::OUTFIT_NAMES[menu.draft.character.outfit as usize]), Action::Outfit, false, menu.focus);
                                p.spawn(text("Appearance changes presentation. Everyone begins with the same gameplay rules and opportunities.", 13., MUTED));
                            });
                            column(p, 44., |p| {
                                p.spawn((ImageNode::new(preview.0.clone()), Node { width: px(260), height: px(340), max_width: percent(100), align_self: AlignSelf::Center, border_radius: BorderRadius::all(px(8)), ..default() }));
                                button(p, "Rotate preview", Action::Rotate, false, menu.focus);
                            });
                        });
                    }
                    Screen::World => {
                        field(p, "World name", &menu.draft.world_name, Field::WorldName, 48, menu.focus == Some(Action::World) || menu.focus.is_none());
                        button(p, format!("World source · {}   /   Change", menu.draft.preset.title()), Action::Preset, false, menu.focus);
                        p.spawn(text(menu.draft.preset.description(), 13., MUTED));
                        button(p, format!("Storage · {}", if menu.draft.regional_storage { "Regional database · Automatic checkpoints" } else { "Portable JSON · Save with F5 and on clean exit" }), Action::Storage, false, menu.focus);
                        row(p, |p| { column(p, 49., |p| field(p, "Latitude", &menu.draft.latitude, Field::Latitude, 24, false)); column(p, 49., |p| field(p, "Longitude", &menu.draft.longitude, Field::Longitude, 24, false)); });
                        row(p, |p| { column(p, 49., |p| field(p, "World seed", &menu.draft.seed, Field::Seed, 20, false)); column(p, 49., |p| { p.spawn(text("Loaded region · width × depth", 13., MUTED)); button(p, format!("{}   /   Change", menu.draft.requested_coverage_label()), Action::Radius, false, menu.focus); }); });
                        row(p, |p| { column(p, 49., |p| button(p, format!("Time speed · {}×", menu.draft.time_scale), Action::TimeScale, false, menu.focus)); column(p, 49., |p| button(p, format!("Start time · {:02}:00", menu.draft.start_hour), Action::StartHour, false, menu.focus)); });
                        row(p, |p| { column(p, 49., |p| button(p, format!("Day and night · {}", enabled(menu.draft.day_night)), Action::DayNight, false, menu.focus)); column(p, 49., |p| button(p, format!("Seasonal visuals · {}", enabled(menu.draft.seasons)), Action::Seasons, false, menu.focus)); });
                        row(p, |p| { column(p, 49., |p| button(p, format!("Weather · {}", enabled(menu.draft.weather)), Action::Weather, false, menu.focus)); column(p, 49., |p| button(p, format!("Vegetation · {}", enabled(menu.draft.vegetation)), Action::Vegetation, false, menu.focus)); });
                        row(p, |p| { column(p, 49., |p| button(p, format!("Graphics · {}", menu.draft.quality.quality.label()), Action::Quality, false, menu.focus)); column(p, 49., |p| button(p, format!("Atmospheric sky · {}{}", enabled(menu.draft.quality.uses_atmosphere()), if menu.draft.quality.quality == terra_bevy::render::RenderQuality::Performance { format!(" · Performance\nSky preference · {}", enabled(menu.draft.quality.atmospheric_sky)) } else { String::new() }), Action::Sky, false, menu.focus)); });
                        p.spawn(text("Each world receives a separate save folder. Regional storage adds automatic checkpoints and dormant regions; portable JSON keeps a complete snapshot. Dated packs and heightmaps limit the view to available coverage.", 12., MUTED));
                    }
                    Screen::Review => {
                        let config = menu.draft.config(&menu.project_root);
                        let actual = config.as_ref().map(|config| super::model::region_coverage_label(config.focus, config.chunk_level, config.chunk_radius)).unwrap_or_else(|_| "Check the world settings below".into());
                        p.spawn(text(&menu.draft.world_name, 24., ACCENT));
                        p.spawn(text(format!("Character    {}\nWorld          {}\nLocation      {}, {}\nSeed            {}\nCoverage     {}\nClock           {:02}:00 at {}× speed\nGraphics      {}", menu.draft.character.name, menu.draft.preset.title(), menu.draft.latitude, menu.draft.longitude, menu.draft.seed, actual, menu.draft.start_hour, menu.draft.time_scale, menu.draft.quality.quality.label()), 16., TEXT));
                        p.spawn(text("Your first session\nTake control of an owned actor, find a driving job or supply request, and earn credits through work. Build and trade from the same inventory and wallet.", 15., MUTED));
                        p.spawn(text(crate::flow::save_contract_description(false, menu.draft.regional_storage, true), 13., MUTED));
                        if let Err(error) = config { p.spawn(text(error, 14., ERROR)); }
                    }
                    Screen::Join => {
                        field(p, "Server address (IP:port)", &menu.draft.address, Field::Address, 64, menu.focus.is_none_or(|action| action == Action::Join));
                        field(p, "Your session name", &menu.draft.session_name, Field::Session, 32, false);
                        field(p, "Local source configuration JSON · optional", &menu.draft.source_config, Field::SourceConfig, 1024, false);
                        p.spawn(text("Leave blank for the default frontier host. For a heightmap or historical host, choose its matching local WorldConfig JSON. Source paths inside it use the project folder; source files are not downloaded during connection.", 13., MUTED));
                        p.spawn(text("Reuse the same session name with the same server to resume your identity. Give each player a separate name. A new server world cannot resume another world's identities.", 14., MUTED));
                        button(p, format!("Connection security · {}", if menu.draft.tls { "Verified TLS" } else { "Plain TCP" }), Action::Tls, false, menu.focus);
                        if menu.draft.tls {
                            field(p, "CA certificate PEM file", &menu.draft.tls_ca, Field::TlsCa, 1024, false);
                            field(p, "Hostname on the server certificate", &menu.draft.tls_server_name, Field::TlsServerName, 253, false);
                            p.spawn(text("TLS checks the certificate against your CA and this hostname. It encrypts the connection; it does not create an online account.", 13., MUTED));
                        } else {
                            button(p, format!("Allow unencrypted remote connection · {}", enabled(menu.draft.allow_insecure_remote)), Action::InsecureRemote, false, menu.focus);
                            p.spawn(text("Plain TCP is enabled for localhost. Remote plaintext requires this explicit choice and is intended for a trusted network.", 13., MUTED));
                        }
                        button(p, format!("Client graphics · {}", menu.draft.quality.quality.label()), Action::Quality, false, menu.focus);
                        p.spawn(text("Connect to a trusted compatible Terra host. Server source settings are checked during admission; local world settings do not override the host.", 13., MUTED));
                    }
                    Screen::Settings => {
                        button(p, format!("Graphics quality · {}", menu.draft.quality.quality.label()), Action::Quality, false, menu.focus.or(Some(Action::Quality)));
                        button(p, format!("Atmospheric sky · {}{}", enabled(menu.draft.quality.uses_atmosphere()), if menu.draft.quality.quality == terra_bevy::render::RenderQuality::Performance { format!(" · Performance\nSky preference · {}", enabled(menu.draft.quality.atmospheric_sky)) } else { String::new() }), Action::Sky, false, menu.focus);
                        p.spawn(text("Performance reduces terrain detail and disables the atmospheric sky. Balanced and High require a capable GPU. These controls never change the simulation or source heights.", 14., MUTED));
                        button(p, "View controls", Action::Controls, false, menu.focus);
                        p.spawn(text("Playtest diagnostics", 19., TEXT));
                        button(p, format!("Local playtest reports · {}", enabled(menu.playtest_reports)), Action::PlaytestReports, false, menu.focus);
                        p.spawn(text("Applies when you next open a world or join a host from this menu. Save defaults to keep this choice. Every 10 real seconds, retain up to 16 captures; a final capture is attempted on exit. Turning Off keeps any existing report.", 14., MUTED));
                        p.spawn(text("Reports stay on this computer. They include timing, source and build identifiers, session state, asset readiness and checkpoint status. No automatic upload, raw game logs, inventory or private session keys. Review the file before sharing it.", 14., MUTED));
                        p.spawn(text(format!("Report file · {}", crate::telemetry_capture::menu_report_path(&menu.user_data).display()), 13., MUTED));
                        p.spawn(text(&menu.report_summary, 14., MUTED));
                        button(p, "Refresh report status", Action::RefreshReport, false, menu.focus);
                    }
                    Screen::Controls => {
                        p.spawn(text("F1 · RTS   F2 · Third person   F3 · First person\nClick / drag to select · Right-click to command\nJ · Activities   N · Nature   T · Territory   Y · Fleet\nV · Driving   G · Garage   M · Atlas\nF5 · Checkpoint   F9 · Restore   F10–F12 · World / Inspector / Performance\nEscape · Back / pause   Tab · Menu focus   Enter · Activate", 14., TEXT));
                        p.spawn(text(&input_settings.notice, 14., ACCENT));
                        for (title, action) in input_settings.rows() { button(p, title, Action::InputSetting(action), false, menu.focus); }
                        p.spawn(text(format!("{} controller(s) detected{}", input.connected_gamepads, if input.controller_name.is_empty() { String::new() } else { format!(" · {}", input.controller_name) }), 13., MUTED)).insert(crate::player_input::ControllerStatus);
                        p.spawn(text(crate::player_input::GAMEPAD_HELP, 14., TEXT));
                    }
                    Screen::Launching => { p.spawn(text("The game opens in a fresh native session. Loading progress appears before control is enabled.", 15., MUTED)); }
                }
            });
            match menu.screen {
                Screen::Character => footer(p, Action::Home, Some(("World settings", Action::World)), menu.focus),
                Screen::World => footer(p, Action::Character, Some(("Review world", Action::Review)), menu.focus),
                Screen::Review => footer(p, Action::World, Some(("Create world and play", Action::Create)), menu.focus.or(Some(Action::Create))),
                Screen::Join => footer(p, Action::Home, Some(("Connect", Action::Connect)), menu.focus),
                Screen::Settings => footer(p, Action::Home, Some(("Save defaults", Action::SaveSettings)), menu.focus),
                Screen::Worlds => footer(p, Action::Home, None, menu.focus),
                Screen::RenameWorld => footer(p, Action::Worlds, menu.rename_selection.as_ref().map(|_| ("Save name", Action::SaveWorldName(menu.review_revision))), menu.focus),
                Screen::Backups => footer(p, Action::Worlds, None, menu.focus),
                Screen::RecoveryReview => footer(p, Action::Backups, menu.recovery_review.as_ref().map(|_| ("Recover as new world", Action::RestoreBackup(menu.review_revision))), menu.focus),
                Screen::Controls => footer(p, Action::Home, Some(("Save controls", Action::InputSetting(crate::player_input::SettingAction::Save))), menu.focus.or(Some(Action::Home))),
                _ => {},
            }
            p.spawn(text("Tab to navigate · Enter to choose · Escape to go back", 12., MUTED));
        });
        root.spawn(text(format!("Terra {} · Synthetic terrain preview", env!("CARGO_PKG_VERSION")), 12., TEXT)).insert(Node { position_type: PositionType::Absolute, right: px(32), bottom: px(14), ..default() });
    });
}

pub fn style(
    focus: Res<InputFocus>,
    mut buttons: Query<(
        Entity,
        &ActionButton,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut fields: Query<(Entity, &mut BorderColor), (With<Field>, Without<ActionButton>)>,
) {
    for (entity, button, hover, pressed, disabled, mut background, mut border) in &mut buttons {
        background.0 = if disabled {
            PANEL
        } else if pressed {
            SELECTED
        } else if hover.get() {
            HOVER
        } else if button.primary {
            SELECTED
        } else {
            SURFACE
        };
        *border = BorderColor::all(if focus.get() == Some(entity) {
            ACCENT
        } else {
            Color::NONE
        });
    }
    for (entity, mut border) in &mut fields {
        *border = BorderColor::all(if focus.get() == Some(entity) {
            ACCENT
        } else {
            Color::srgb(0.30, 0.43, 0.35)
        });
    }
}

#[cfg(test)]
mod status_layout_tests {
    use super::*;
    use bevy::ui_widgets::ScrollArea;
    use std::sync::{Arc, Mutex};

    fn scroll_ancestor(world: &World, mut entity: Entity) -> Option<Entity> {
        while let Some(parent) = world.get::<ChildOf>(entity) {
            entity = parent.parent();
            if world.get::<ScrollArea>(entity).is_some() {
                return Some(entity);
            }
        }
        None
    }

    #[test]
    fn long_status_stays_in_scroll_content_and_footer_actions_keep_their_identity() {
        let error = format!(
            "Could not open {}: permission denied\nRetry after correcting the path.",
            "x".repeat(1024)
        );
        let warning = format!("Skipped invalid profile {}", "w".repeat(1024));
        let notice = format!("Previous operation completed at {}", "n".repeat(1024));
        let progress = format!("Verifying {}", "p".repeat(1024));
        let report = format!("Report could not be read at {}", "r".repeat(1024));
        for (screen, footer_action) in [
            (Screen::Home, None),
            (Screen::Worlds, Some(("Back", Action::Home))),
            (Screen::RenameWorld, Some(("Back", Action::Worlds))),
            (Screen::Backups, Some(("Back", Action::Worlds))),
            (Screen::RecoveryReview, Some(("Back", Action::Backups))),
            (Screen::Character, Some(("World settings", Action::World))),
            (Screen::World, Some(("Review world", Action::Review))),
            (
                Screen::Review,
                Some(("Create world and play", Action::Create)),
            ),
            (Screen::Join, Some(("Connect", Action::Connect))),
            (
                Screen::Settings,
                Some(("Save defaults", Action::SaveSettings)),
            ),
            (
                Screen::Controls,
                Some((
                    "Save controls",
                    Action::InputSetting(crate::player_input::SettingAction::Save),
                )),
            ),
            (Screen::Launching, None),
        ] {
            let mut app = App::new();
            app.insert_resource(MenuState {
                project_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
                user_data: std::path::PathBuf::from(format!("{}diagnostics", "d".repeat(1024))),
                screen,
                draft: Default::default(),
                playtest_reports: false,
                report_summary: report.clone(),
                recent: Vec::new(),
                backups: Vec::new(),
                completed_world: None,
                completed_backup: None,
                listing_revision: 0,
                recovery_context: 0,
                review_revision: 0,
                recovery_review: None,
                rename_selection: None,
                world_name_draft: String::new(),
                backup_path: String::new(),
                recovery_progress: Some(progress.clone()),
                notice: Some(notice.clone()),
                warning: Some(warning.clone()),
                error: Some(error.clone()),
                dirty: true,
                focus: None,
                preview_angle: 0.,
                output: Arc::new(Mutex::new(None)),
                exit_frames: None,
                screenshot: None,
                frames: 0,
            })
            .insert_resource(PreviewImage(Handle::default()))
            .init_resource::<InputFocus>()
            .init_resource::<Actions>()
            .init_resource::<crate::player_input::InputSettings>()
            .init_resource::<crate::player_input::PlayerInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, rebuild);
            app.update();
            let labels: Vec<_> = app
                .world_mut()
                .query::<(Entity, &Text)>()
                .iter(app.world())
                .map(|(entity, text)| (entity, text.0.clone()))
                .collect();
            let mut expected = vec![&error, &notice, &progress];
            if matches!(
                screen,
                Screen::Home
                    | Screen::Worlds
                    | Screen::RenameWorld
                    | Screen::Backups
                    | Screen::RecoveryReview
            ) {
                expected.push(&warning);
            }
            if screen == Screen::Settings {
                expected.push(&report);
            }
            for value in expected {
                let matching: Vec<_> = labels.iter().filter(|(_, text)| text == value).collect();
                assert_eq!(
                    matching.len(),
                    1,
                    "{screen:?}: status must appear exactly once"
                );
                let entity = matching[0].0;
                let scroll = scroll_ancestor(app.world(), entity).unwrap_or_else(|| {
                    panic!("{screen:?}: status must scroll before it can displace the footer")
                });
                let scroll_node = app.world().get::<Node>(scroll).unwrap();
                assert_eq!(scroll_node.min_height, px(0.));
                assert_eq!(scroll_node.max_height, vh(66.));
                assert_eq!(
                    app.world().get::<TextLayout>(entity).unwrap().linebreak,
                    bevy::text::LineBreak::WordOrCharacter
                );
            }
            if screen == Screen::Settings {
                let path = labels
                    .iter()
                    .find(|(_, text)| text.starts_with("Report file · "))
                    .unwrap()
                    .0;
                assert!(scroll_ancestor(app.world(), path).is_some());
                assert_eq!(
                    app.world().get::<TextLayout>(path).unwrap().linebreak,
                    bevy::text::LineBreak::WordOrCharacter
                );
            }
            if let Some((title, action)) = footer_action {
                let label = labels.iter().find(|(_, text)| text == title).unwrap().0;
                assert!(
                    scroll_ancestor(app.world(), label).is_none(),
                    "{screen:?}: footer stays outside the status scroll"
                );
                let button = app.world().get::<ChildOf>(label).unwrap().parent();
                assert!(app.world().get::<Button>(button).is_some());
                assert!(app.world().get::<TabIndex>(button).is_some());
                app.world_mut().trigger(Activate { entity: button });
                assert_eq!(
                    app.world_mut().resource_mut::<Actions>().0.pop_front(),
                    Some(action)
                );
                assert!(app.world().resource::<Actions>().0.is_empty());
            }
        }
    }
}
