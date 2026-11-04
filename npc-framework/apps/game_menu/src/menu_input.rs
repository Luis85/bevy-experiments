//! Ordered keyboard dispatch for the menu. Keep Bevy's text/IME mapping, but expose
//! modifiers as they were at each raw event instead of the end of the whole batch.
use std::collections::{BTreeMap, BTreeSet};

use bevy::ecs::{message::MessageCursor, schedule::ScheduleCleanupPolicy};
use bevy::input::{
    ButtonState, InputSystems,
    keyboard::{Key, KeyboardFocusLost, KeyboardInput},
};
use bevy::input_focus::{
    FocusedInput, InputDispatchPlugin, InputFocus, InputFocusSystems, dispatch_focused_input,
};
use bevy::prelude::*;
use bevy::ui::ComputedUiTargetCamera;
use bevy::ui_widgets::TextInput;
use bevy::window::{Ime, PrimaryWindow, WindowRef};

/// IME text received in this raw batch already owns its confirming/editing keys,
/// even though Bevy applies the composition edits later in PostUpdate.
#[derive(Resource, Default)]
pub(crate) struct ImeBatch(BTreeSet<Entity>);
impl ImeBatch {
    pub(crate) fn contains(&self, window: Entity) -> bool {
        self.0.contains(&window)
    }
}

#[derive(Clone, Default)]
struct WindowKeys {
    logical: ButtonInput<Key>,
    physical: ButtonInput<KeyCode>,
}
impl WindowKeys {
    fn apply(&mut self, event: &KeyboardInput) {
        if event.state == ButtonState::Pressed {
            self.logical.press(event.logical_key.clone());
            self.physical.press(event.key_code);
        } else {
            self.logical.release(event.logical_key.clone());
            self.physical.release(event.key_code);
        }
        self.normalize_modifiers();
    }
    fn normalize_modifiers(&mut self) {
        for (logical, left, right) in [
            (Key::Control, KeyCode::ControlLeft, KeyCode::ControlRight),
            (Key::Shift, KeyCode::ShiftLeft, KeyCode::ShiftRight),
            (Key::Alt, KeyCode::AltLeft, KeyCode::AltRight),
            (Key::Super, KeyCode::SuperLeft, KeyCode::SuperRight),
        ] {
            let pressed = self.physical.pressed(left) || self.physical.pressed(right);
            if pressed && !self.logical.pressed(logical.clone()) {
                self.logical.press(logical);
            } else if !pressed && self.logical.pressed(logical.clone()) {
                self.logical.release(logical);
            }
        }
    }
    fn clear_edges(&mut self) {
        self.logical.clear();
        self.physical.clear();
    }
}

#[derive(Resource, Default)]
struct OrderedKeys {
    events: MessageCursor<KeyboardInput>,
    lost: MessageCursor<KeyboardFocusLost>,
    ime: MessageCursor<Ime>,
    windows: BTreeMap<Entity, WindowKeys>,
}

pub(super) fn install(app: &mut App) {
    if !app.is_plugin_added::<InputDispatchPlugin>() {
        return;
    }
    app.add_message::<Ime>().init_resource::<ImeBatch>();
    let removed = app
        .remove_systems_in_set(
            PreUpdate,
            dispatch_focused_input::<KeyboardInput>,
            ScheduleCleanupPolicy::RemoveSystemsOnly,
        )
        .expect("menu keyboard dispatcher is installed after Bevy InputDispatchPlugin");
    assert_eq!(removed, 1, "replace exactly one keyboard dispatcher");
    app.init_resource::<OrderedKeys>().add_systems(
        PreUpdate,
        dispatch
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems)
            .before(dispatch_focused_input::<bevy::input::mouse::MouseWheel>),
    );
}

fn target(world: &World, window: Entity, primary: Option<Entity>) -> Entity {
    let Some(focus) = world
        .resource::<InputFocus>()
        .get()
        .filter(|focus| world.entities().contains(*focus))
    else {
        return window;
    };
    if world.get::<Window>(focus).is_some() {
        return if focus == window { focus } else { window };
    }
    // Explicit window ancestry is useful for app-owned secondary-window controls.
    let mut ancestor = focus;
    for _ in 0..128 {
        if world.get::<Window>(ancestor).is_some() {
            return if ancestor == window { focus } else { window };
        }
        let Some(parent) = world.get::<ChildOf>(ancestor) else {
            break;
        };
        ancestor = parent.parent();
    }
    if let Some(camera) = world
        .get::<ComputedUiTargetCamera>(focus)
        .and_then(ComputedUiTargetCamera::get)
        && let Some(bevy::camera::RenderTarget::Window(reference)) =
            world.get::<bevy::camera::RenderTarget>(camera)
    {
        let destination = match reference {
            WindowRef::Primary => primary,
            WindowRef::Entity(entity) => Some(*entity),
        };
        return if destination == Some(window) {
            focus
        } else {
            window
        };
    }
    // Unlaid-out menu controls belong to the primary window. Never send another
    // window's key event to its currently focused text field.
    if primary == Some(window) {
        focus
    } else {
        window
    }
}

fn dispatch(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<OrderedKeys>| {
        let ime_windows = state
            .ime
            .read(world.resource::<Messages<Ime>>())
            .filter_map(|event| match event {
                Ime::Preedit { window, .. } | Ime::Commit { window, .. } => Some(*window),
                _ => None,
            })
            .collect();
        world.resource_mut::<ImeBatch>().0 = ime_windows;
        let events: Vec<_> = state
            .events
            .read(world.resource::<Messages<KeyboardInput>>())
            .cloned()
            .collect();
        let lost = state
            .lost
            .read(world.resource::<Messages<KeyboardFocusLost>>())
            .count()
            > 0;
        state
            .windows
            .retain(|window, _| world.get::<Window>(*window).is_some());
        for keys in state.windows.values_mut() {
            if lost {
                *keys = WindowKeys::default();
            } else {
                keys.clear_edges();
            }
        }
        if events.is_empty() {
            return;
        }
        let end = WindowKeys {
            logical: world.resource::<ButtonInput<Key>>().clone(),
            physical: world.resource::<ButtonInput<KeyCode>>().clone(),
        };
        let primary = world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(world)
            .next();
        // Seed an already-held modifier on the first frame. Reverse the complete
        // batch, rather than treating its final Ctrl state as true for every key.
        let mut initial = end.clone();
        for event in events.iter().rev() {
            if event.state == ButtonState::Released {
                initial.logical.press(event.logical_key.clone());
                initial.physical.press(event.key_code);
            } else if !event.repeat {
                initial.logical.release(event.logical_key.clone());
                initial.physical.release(event.key_code);
            }
        }
        initial.normalize_modifiers();
        initial.clear_edges();
        for event in events {
            if world.get::<Window>(event.window).is_none() {
                continue;
            }
            let keys = state.windows.entry(event.window).or_insert_with(|| {
                if !lost && primary == Some(event.window) {
                    initial.clone()
                } else {
                    WindowKeys::default()
                }
            });
            keys.apply(&event);
            let destination = target(world, event.window, primary);
            if world.get::<TextInput>(destination).is_some()
                && world.resource::<ImeBatch>().contains(event.window)
            {
                // Do not duplicate raw printable keys, edit shortcuts or the Enter
                // that confirms IME text. The original Ime reader is untouched.
                continue;
            }
            let mut logical = keys.logical.clone();
            // Windows AltGr is represented as left Ctrl + right Alt. Printable
            // text remains text, never Ctrl+A/C/V/X. IME composition stays with
            // Bevy's existing observer and is never interpreted here.
            let composing = world
                .get::<bevy::text::EditableText>(destination)
                .is_some_and(|text| text.is_composing());
            let altgr_text = world.get::<TextInput>(destination).is_some()
                && !composing
                && keys.physical.pressed(KeyCode::AltRight)
                && event
                    .text
                    .as_ref()
                    .is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
                && matches!(&event.logical_key, Key::Character(_) | Key::Unidentified(_));
            if altgr_text {
                logical.release(Key::Control);
                logical.release(Key::Alt);
            }
            *world.resource_mut::<ButtonInput<Key>>() = logical;
            *world.resource_mut::<ButtonInput<KeyCode>>() = keys.physical.clone();
            world.trigger(FocusedInput::new(destination, event.clone(), event.window));
            // Observer Commands (including focus changes or despawns) must land
            // before routing the next event in this same raw batch.
            world.flush();
        }
        *world.resource_mut::<ButtonInput<Key>>() = end.logical;
        *world.resource_mut::<ButtonInput<KeyCode>>() = end.physical;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::input_focus::{FocusCause, InputFocusVisible};
    use bevy::text::{EditableText, TextEdit};
    use bevy::ui_widgets::TextInputPlugin;

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((InputPlugin, InputDispatchPlugin))
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .add_message::<bevy::window::Ime>()
            .add_plugins(TextInputPlugin);
        install(&mut app);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let field = field(app.world_mut(), None);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        (app, window, field)
    }
    fn field(world: &mut World, parent: Option<Entity>) -> Entity {
        let mut text = EditableText::new("");
        text.pending_edits.clear();
        let mut entity = world.spawn((TextInput, text));
        if let Some(parent) = parent {
            entity.insert(ChildOf(parent));
        }
        entity.id()
    }
    fn event(
        window: Entity,
        code: KeyCode,
        logical: Key,
        pressed: bool,
        text: Option<&str>,
    ) -> KeyboardInput {
        KeyboardInput {
            window,
            key_code: code,
            logical_key: logical,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: text.map(Into::into),
            repeat: false,
        }
    }
    fn command() -> (KeyCode, Key) {
        if cfg!(target_os = "macos") {
            (KeyCode::SuperLeft, Key::Super)
        } else {
            (KeyCode::ControlLeft, Key::Control)
        }
    }
    fn chord(window: Entity, code: KeyCode, logical: &str) -> Vec<KeyboardInput> {
        let (modifier, key) = command();
        vec![
            event(window, modifier, key.clone(), true, None),
            event(window, code, Key::Character(logical.into()), true, None),
            event(window, code, Key::Character(logical.into()), false, None),
            event(window, modifier, key, false, None),
        ]
    }
    fn send(app: &mut App, events: impl IntoIterator<Item = KeyboardInput>) {
        for event in events {
            app.world_mut().write_message(event);
        }
        app.world_mut().run_schedule(PreUpdate);
    }
    fn edits(app: &App, field: Entity) -> &[TextEdit] {
        &app.world()
            .get::<EditableText>(field)
            .unwrap()
            .pending_edits
    }
    #[test]
    fn complete_command_chords_in_one_frame_keep_all_four_shortcuts_and_restore_global_keys() {
        let (mut app, window, field) = app();
        let mut batch = Vec::new();
        for (code, logical) in [
            (KeyCode::KeyA, "a"),
            (KeyCode::KeyC, "c"),
            (KeyCode::KeyV, "v"),
            (KeyCode::KeyX, "x"),
        ] {
            batch.extend(chord(window, code, logical));
        }
        send(&mut app, batch);
        assert_eq!(
            edits(&app, field),
            [
                TextEdit::SelectAll,
                TextEdit::Copy,
                TextEdit::Paste,
                TextEdit::Cut
            ]
        );
        let (modifier, key) = command();
        assert!(!app.world().resource::<ButtonInput<Key>>().pressed(key));
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(modifier)
        );
    }
    #[test]
    fn a_plain_letter_before_and_after_a_fast_chord_is_not_reinterpreted_as_a_shortcut() {
        let (mut app, window, field) = app();
        let mut batch = vec![
            event(
                window,
                KeyCode::KeyA,
                Key::Character("a".into()),
                true,
                Some("a"),
            ),
            event(
                window,
                KeyCode::KeyA,
                Key::Character("a".into()),
                false,
                None,
            ),
        ];
        batch.extend(chord(window, KeyCode::KeyA, "a"));
        batch.push(event(
            window,
            KeyCode::KeyB,
            Key::Character("b".into()),
            true,
            Some("b"),
        ));
        send(&mut app, batch);
        assert_eq!(
            edits(&app, field),
            [
                TextEdit::Insert("a".into()),
                TextEdit::SelectAll,
                TextEdit::Insert("b".into())
            ]
        );
    }
    #[test]
    fn a_modifier_already_held_on_the_first_frame_is_honored_then_released() {
        let (mut app, window, field) = app();
        let (modifier, key) = command();
        app.world_mut()
            .resource_mut::<ButtonInput<Key>>()
            .press(key.clone());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(modifier);
        send(
            &mut app,
            [
                event(
                    window,
                    KeyCode::KeyA,
                    Key::Character("a".into()),
                    true,
                    None,
                ),
                event(
                    window,
                    KeyCode::KeyA,
                    Key::Character("a".into()),
                    false,
                    None,
                ),
            ],
        );
        send(
            &mut app,
            [
                event(window, modifier, key, false, None),
                event(
                    window,
                    KeyCode::KeyC,
                    Key::Character("c".into()),
                    true,
                    Some("c"),
                ),
            ],
        );
        assert_eq!(
            edits(&app, field),
            [TextEdit::SelectAll, TextEdit::Insert("c".into())]
        );
    }
    #[test]
    fn focus_loss_does_not_leave_a_stuck_command_modifier() {
        let (mut app, window, field) = app();
        let (modifier, key) = command();
        send(&mut app, [event(window, modifier, key, true, None)]);
        app.world_mut().write_message(KeyboardFocusLost);
        send(
            &mut app,
            [event(
                window,
                KeyCode::KeyV,
                Key::Character("v".into()),
                true,
                Some("v"),
            )],
        );
        assert_eq!(edits(&app, field), [TextEdit::Insert("v".into())]);
    }
    #[test]
    fn foreign_windows_never_send_text_or_modifiers_to_the_primary_focused_field() {
        let (mut app, window, first) = app();
        let other = app.world_mut().spawn(Window::default()).id();
        let second = field(app.world_mut(), Some(other));
        let mut batch = chord(other, KeyCode::KeyA, "a");
        batch.push(event(
            window,
            KeyCode::KeyV,
            Key::Character("v".into()),
            true,
            Some("v"),
        ));
        send(&mut app, batch);
        assert_eq!(edits(&app, first), [TextEdit::Insert("v".into())]);
        assert!(edits(&app, second).is_empty());
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(second, FocusCause::Navigated);
        let mut batch = chord(window, KeyCode::KeyA, "a");
        batch.push(event(
            other,
            KeyCode::KeyE,
            Key::Character("é".into()),
            true,
            Some("é"),
        ));
        send(&mut app, batch);
        assert_eq!(edits(&app, second), [TextEdit::Insert("é".into())]);
    }
    #[test]
    fn tab_and_deferred_focus_changes_take_effect_before_later_keys_in_the_same_batch() {
        use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, handle_tab_navigation};
        let (mut app, window, first) = app();
        let group = app.world_mut().spawn(TabGroup::new(0)).id();
        app.world_mut().entity_mut(first).insert((
            ChildOf(group),
            TabIndex(0),
            InheritedVisibility::VISIBLE,
        ));
        let second = field(app.world_mut(), Some(group));
        app.world_mut()
            .entity_mut(second)
            .insert((TabIndex(1), InheritedVisibility::VISIBLE));
        app.world_mut()
            .entity_mut(window)
            .observe(handle_tab_navigation);
        send(
            &mut app,
            [
                event(window, KeyCode::Tab, Key::Tab, true, None),
                event(window, KeyCode::Tab, Key::Tab, false, None),
                event(
                    window,
                    KeyCode::KeyZ,
                    Key::Character("z".into()),
                    true,
                    Some("z"),
                ),
            ],
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(second));
        assert!(edits(&app, first).is_empty());
        assert_eq!(edits(&app, second), [TextEdit::Insert("z".into())]);
        app.world_mut().entity_mut(window).observe(
            move |input: On<FocusedInput<KeyboardInput>>, mut commands: Commands| {
                if input.input.key_code == KeyCode::F2 && input.input.state.is_pressed() {
                    commands.queue(move |world: &mut World| {
                        world
                            .resource_mut::<InputFocus>()
                            .set(first, FocusCause::Navigated)
                    });
                }
            },
        );
        send(
            &mut app,
            [
                event(window, KeyCode::F2, Key::F2, true, None),
                event(
                    window,
                    KeyCode::KeyX,
                    Key::Character("x".into()),
                    true,
                    Some("x"),
                ),
            ],
        );
        assert_eq!(edits(&app, first), [TextEdit::Insert("x".into())]);
    }
    #[test]
    fn logical_layout_and_altgr_produced_text_remain_intact() {
        let (mut app, window, field) = app();
        // Logical Latin C wins over a different physical key; Cyrillic falls back
        // to the conventional physical shortcut, exactly as Bevy maps it.
        let mut batch = chord(window, KeyCode::KeyV, "c");
        batch.extend(chord(window, KeyCode::KeyC, "с"));
        batch.extend([
            event(window, KeyCode::ControlLeft, Key::Control, true, None),
            event(window, KeyCode::AltRight, Key::Alt, true, None),
            event(
                window,
                KeyCode::KeyA,
                Key::Character("á".into()),
                true,
                Some("á"),
            ),
            event(window, KeyCode::AltRight, Key::Alt, false, None),
            event(window, KeyCode::ControlLeft, Key::Control, false, None),
        ]);
        send(&mut app, batch);
        assert_eq!(
            edits(&app, field),
            [TextEdit::Copy, TextEdit::Copy, TextEdit::Insert("á".into())]
        );
    }
    #[test]
    fn ime_composition_keeps_keyboard_shortcuts_and_commit_with_bevy() {
        use bevy::text::{FontCx, LayoutCx};
        let (mut app, window, field) = app();
        app.world_mut().write_message(bevy::window::Ime::Preedit {
            window,
            value: "composing".into(),
            cursor: None,
        });
        app.world_mut().run_schedule(PreUpdate);
        let mut fonts = FontCx::default();
        let mut layout = LayoutCx::default();
        let mut clipboard = bevy::clipboard::Clipboard::default();
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
        assert!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .is_composing()
        );
        send(&mut app, chord(window, KeyCode::KeyA, "a"));
        assert!(
            edits(&app, field).is_empty(),
            "composition owns keyboard input"
        );
        app.world_mut().write_message(bevy::window::Ime::Commit {
            window,
            value: "漢字".into(),
        });
        app.world_mut().run_schedule(PreUpdate);
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
        let text = app.world().get::<EditableText>(field).unwrap();
        assert_eq!(text.value().to_string(), "漢字");
        assert!(!text.is_composing());
        assert!(text.pending_edits.is_empty());
    }

    #[test]
    fn current_batch_ime_guard_is_window_scoped_and_clears_on_the_next_batch() {
        let (mut app, window, field) = app();
        let other = app.world_mut().spawn(Window::default()).id();
        app.world_mut().write_message(Ime::Preedit {
            window: other,
            value: String::new(),
            cursor: None,
        });
        send(&mut app, chord(window, KeyCode::KeyA, "a"));
        assert!(app.world().resource::<ImeBatch>().contains(other));
        assert!(!app.world().resource::<ImeBatch>().contains(window));
        assert_eq!(
            edits(&app, field)
                .iter()
                .filter(|edit| matches!(edit, TextEdit::SelectAll))
                .count(),
            1
        );
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .pending_edits
            .clear();
        send(&mut app, chord(window, KeyCode::KeyC, "c"));
        assert!(!app.world().resource::<ImeBatch>().contains(other));
        assert_eq!(edits(&app, field), [TextEdit::Copy]);
    }
}
