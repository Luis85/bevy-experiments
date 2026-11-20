//! Presentation-only input mapping. The server still receives bounded intent.
//! Bindings are local to the user, independent of any world or character save.
use bevy::{input::InputSystems, prelude::*};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Binding {
    Forward,
    Backward,
    Left,
    Right,
    SprintBrake,
    Handbrake,
    Ascend,
    Descend,
    Interact,
    Camera,
    Park,
    Lights,
    LeftSignal,
    RightSignal,
    Hazards,
}
impl Binding {
    pub const ALL: [Self; 15] = [
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::SprintBrake,
        Self::Handbrake,
        Self::Ascend,
        Self::Descend,
        Self::Interact,
        Self::Camera,
        Self::Park,
        Self::Lights,
        Self::LeftSignal,
        Self::RightSignal,
        Self::Hazards,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "Forward / accelerator",
            Self::Backward => "Backward / reverse",
            Self::Left => "Move / steer left",
            Self::Right => "Move / steer right",
            Self::SprintBrake => "Sprint / service brake",
            Self::Handbrake => "Handbrake",
            Self::Ascend => "Rise / climb",
            Self::Descend => "Lower / descend",
            Self::Interact => "Interact / service stop",
            Self::Camera => "Switch first / third person",
            Self::Park => "Parking brake",
            Self::Lights => "Headlights",
            Self::LeftSignal => "Left indicator",
            Self::RightSignal => "Right indicator",
            Self::Hazards => "Hazard lights",
        }
    }
}

// Stable wire indices in controls.json. Append only. Fixed interface shortcuts,
// modifiers, Escape, Tab and activation keys are intentionally not assignable.
const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyD, "D"),
    (KeyCode::ShiftLeft, "Left Shift"),
    (KeyCode::Space, "Space"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyK, "K"),
    (KeyCode::ShiftRight, "Right Shift"),
    (KeyCode::ArrowUp, "Up arrow"),
    (KeyCode::ArrowDown, "Down arrow"),
    (KeyCode::ArrowLeft, "Left arrow"),
    (KeyCode::ArrowRight, "Right arrow"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Numpad1, "Num 1"),
    (KeyCode::Numpad2, "Num 2"),
    (KeyCode::Numpad3, "Num 3"),
    (KeyCode::Numpad4, "Num 4"),
    (KeyCode::Numpad5, "Num 5"),
    (KeyCode::Numpad6, "Num 6"),
    (KeyCode::Numpad7, "Num 7"),
    (KeyCode::Numpad8, "Num 8"),
    (KeyCode::Numpad9, "Num 9"),
    (KeyCode::Numpad0, "Num 0"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
];

#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputSettings {
    pub schema_version: u32,
    bindings: [u8; 15],
    pub mouse_sensitivity: f32,
    pub gamepad_sensitivity: f32,
    pub deadzone: f32,
    pub invert_y: bool,
    pub gamepad_enabled: bool,
    pub text_scale: f32,
    pub first_person_fov: f32,
    pub driving_auto_center: bool,
    pub wheel: crate::wheel_input::WheelSettings,
    #[serde(skip)]
    pub wheel_live: crate::wheel_input::WheelLive,
    #[serde(skip)]
    pub listening: Option<Binding>,
    #[serde(skip)]
    pub notice: String,
}
impl Default for InputSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            bindings: std::array::from_fn(|i| i as u8),
            mouse_sensitivity: 1., gamepad_sensitivity: 1., deadzone: 0.15,
            invert_y: false, gamepad_enabled: true, text_scale: 1.,
            first_person_fov: 78., driving_auto_center: true, wheel: Default::default(), wheel_live: Default::default(), listening: None,
            notice: "Select a binding, then press a key. Escape cancels. Save controls keeps your choices for every world.".into(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingAction {
    Bind(Binding),
    MouseSensitivity,
    PadSensitivity,
    Deadzone,
    InvertY,
    EnableGamepad,
    TextScale,
    FieldOfView,
    AutoCenter,
    Wheel(crate::wheel_input::WheelAction),
    Reset,
    Save,
}
impl InputSettings {
    /// A bounded, validated process handoff keeps unsaved choices active while
    /// changing worlds. Only an explicit Save writes persistent preferences.
    pub fn to_handoff(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| error.to_string())
    }
    pub fn from_handoff(payload: &str) -> Result<Self, String> {
        if payload.len() > 4096 {
            return Err("Session controls exceed the 4 KiB handoff limit.".into());
        }
        let mut settings: Self =
            serde_json::from_str(payload).map_err(|error| error.to_string())?;
        settings.validate()?;
        settings.notice =
            "Session controls are active. Save controls keeps your choices after quitting.".into();
        Ok(settings)
    }
    pub fn cancel_capture(&mut self) {
        let wheel = self.wheel_live.cancel();
        if self.listening.take().is_some() {
            self.notice = "Binding cancelled. Your previous key remains active.".into();
        } else if wheel {
            self.notice = "Wheel calibration cancelled; previous valid ranges retained.".into();
        }
    }
    pub fn space_has_rts_action(&self) -> bool {
        Binding::ALL
            .into_iter()
            .any(|binding| binding != Binding::Handbrake && self.key(binding) == KeyCode::Space)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.wheel.validate()?;
        if self.schema_version != 1 {
            return Err("Unsupported controls version.".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in self.bindings {
            if key as usize >= KEYS.len() || !seen.insert(key) {
                return Err("Control bindings contain an invalid or duplicate key.".into());
            }
        }
        if !self.mouse_sensitivity.is_finite()
            || !(0.25..=3.).contains(&self.mouse_sensitivity)
            || !self.gamepad_sensitivity.is_finite()
            || !(0.25..=3.).contains(&self.gamepad_sensitivity)
            || !self.deadzone.is_finite()
            || !(0.05..=0.4).contains(&self.deadzone)
            || !self.text_scale.is_finite()
            || !(1.0..=1.5).contains(&self.text_scale)
            || !self.first_person_fov.is_finite()
            || !(55.0..=100.0).contains(&self.first_person_fov)
        {
            return Err("Control sensitivity or deadzone is outside its supported range.".into());
        }
        Ok(())
    }
    pub fn read(user_data: &Path) -> Self {
        let path = user_data.join("controls.json");
        if !path.exists() {
            return Self::default();
        }
        match crate::launcher::model::read_json::<Self>(&path)
            .and_then(|settings| settings.validate().map(|_| settings))
        {
            Ok(mut settings) => {
                settings.notice = Self::default().notice;
                settings
            }
            Err(error) => Self {
                notice: format!("Controls could not be loaded; defaults are active. {error}"),
                ..Self::default()
            },
        }
    }
    pub fn write(&self, user_data: &Path) -> Result<(), String> {
        self.validate()?;
        crate::launcher::model::private_directory(user_data)?;
        crate::launcher::model::atomic_json(&user_data.join("controls.json"), self)
    }
    pub fn key(&self, binding: Binding) -> KeyCode {
        KEYS[self.bindings[binding as usize] as usize].0
    }
    pub fn key_label(&self, binding: Binding) -> &'static str {
        KEYS[self.bindings[binding as usize] as usize].1
    }
    pub fn bind(&mut self, binding: Binding, key: KeyCode) -> Result<(), String> {
        let index = KEYS.iter().position(|(k, _)| *k == key).ok_or(
            "That key is reserved for menus or a fixed world shortcut. Choose another key.",
        )? as u8;
        if let Some(other) = Binding::ALL
            .into_iter()
            .find(|b| *b != binding && self.bindings[*b as usize] == index)
        {
            return Err(format!(
                "{} already uses {}. Rebind it first, or choose another key.",
                other.label(),
                KEYS[index as usize].1
            ));
        }
        self.bindings[binding as usize] = index;
        Ok(())
    }
    pub fn handle(&mut self, action: SettingAction, user_data: &Path) {
        self.listening = None;
        if !matches!(action, SettingAction::Wheel(_)) {
            self.wheel_live.cancel();
        }
        match action {
            SettingAction::Bind(binding) => {
                self.listening = Some(binding);
                self.notice = format!(
                    "Press a key for {}. Escape cancels; fixed menu shortcuts are reserved.",
                    binding.label()
                );
                return;
            }
            SettingAction::MouseSensitivity => {
                self.mouse_sensitivity =
                    cycle(self.mouse_sensitivity, &[0.5, 0.75, 1., 1.5, 2., 3.])
            }
            SettingAction::PadSensitivity => {
                self.gamepad_sensitivity =
                    cycle(self.gamepad_sensitivity, &[0.5, 0.75, 1., 1.5, 2., 3.])
            }
            SettingAction::Deadzone => {
                self.deadzone = cycle(self.deadzone, &[0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4])
            }
            SettingAction::InvertY => self.invert_y = !self.invert_y,
            SettingAction::EnableGamepad => self.gamepad_enabled = !self.gamepad_enabled,
            SettingAction::TextScale => {
                self.text_scale = cycle(self.text_scale, &[1., 1.15, 1.3, 1.5])
            }
            SettingAction::FieldOfView => {
                self.first_person_fov = cycle(self.first_person_fov, &[55., 65., 78., 90., 100.])
            }
            SettingAction::AutoCenter => self.driving_auto_center = !self.driving_auto_center,
            SettingAction::Wheel(action) => {
                self.notice = self.wheel.handle(&mut self.wheel_live, action);
                return;
            }
            SettingAction::Reset => *self = Self::default(),
            SettingAction::Save => {
                self.notice = match self.write(user_data) {
                    Ok(()) => "Controls saved for every world and future launch.".into(),
                    Err(error) => format!("Controls could not be saved: {error}"),
                };
                return;
            }
        }
        self.notice =
            "Controls updated for this session. Choose Save controls to keep them.".into();
    }
    pub fn rows(&self) -> Vec<(String, SettingAction)> {
        let mut rows = vec![
            (
                format!("Interface text · {:.0}%", self.text_scale * 100.),
                SettingAction::TextScale,
            ),
            (
                format!(
                    "First-person field of view · {:.0}° vertical",
                    self.first_person_fov
                ),
                SettingAction::FieldOfView,
            ),
            (
                format!(
                    "Driving camera auto-center · {}",
                    if self.driving_auto_center {
                        "On"
                    } else {
                        "Off"
                    }
                ),
                SettingAction::AutoCenter,
            ),
            (
                format!("Mouse look sensitivity · {:.2}×", self.mouse_sensitivity),
                SettingAction::MouseSensitivity,
            ),
            (
                format!(
                    "Invert vertical look · {}",
                    if self.invert_y { "On" } else { "Off" }
                ),
                SettingAction::InvertY,
            ),
            (
                format!(
                    "Gamepad · {}",
                    if self.gamepad_enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                ),
                SettingAction::EnableGamepad,
            ),
            (
                format!(
                    "Gamepad look sensitivity · {:.2}×",
                    self.gamepad_sensitivity
                ),
                SettingAction::PadSensitivity,
            ),
            (
                format!("Gamepad stick deadzone · {:.0}%", self.deadzone * 100.),
                SettingAction::Deadzone,
            ),
        ];
        rows.extend(
            self.wheel
                .rows(&self.wheel_live)
                .into_iter()
                .map(|(title, action)| (title, SettingAction::Wheel(action))),
        );
        rows.extend(Binding::ALL.map(|binding| {
            (
                format!(
                    "{} · {}{}",
                    binding.label(),
                    self.key_label(binding),
                    if self.listening == Some(binding) {
                        "  /  Press a key…"
                    } else {
                        ""
                    }
                ),
                SettingAction::Bind(binding),
            )
        }));
        rows.push(("Reset controls to defaults".into(), SettingAction::Reset));
        rows
    }
    pub fn movement_hint(&self, vehicle: bool) -> String {
        if vehicle {
            format!(
                "DRIVING · {}/{} pedals · {}/{} steer · {} brake · {} handbrake · {} view · {} use stop · V jobs · Y fleet",
                self.key_label(Binding::Forward),
                self.key_label(Binding::Backward),
                self.key_label(Binding::Left),
                self.key_label(Binding::Right),
                self.key_label(Binding::SprintBrake),
                self.key_label(Binding::Handbrake),
                self.key_label(Binding::Camera),
                self.key_label(Binding::Interact)
            )
        } else {
            format!(
                "ON FOOT · {}/{}/{}/{} move · {} sprint · Right-drag look · {} interact · {} view · Esc menu",
                self.key_label(Binding::Forward),
                self.key_label(Binding::Left),
                self.key_label(Binding::Backward),
                self.key_label(Binding::Right),
                self.key_label(Binding::SprintBrake),
                self.key_label(Binding::Interact),
                self.key_label(Binding::Camera)
            )
        }
    }
    pub fn rts_hint(&self) -> String {
        format!(
            "RTS · Drag select · Shift add · Right-click move · {}/{}/{}/{} pan · Wheel zoom · Middle-drag orbit · F2 / F3 possess",
            self.key_label(Binding::Forward),
            self.key_label(Binding::Left),
            self.key_label(Binding::Backward),
            self.key_label(Binding::Right)
        )
    }
}
fn cycle(value: f32, choices: &[f32]) -> f32 {
    choices[(choices
        .iter()
        .position(|v| (*v - value).abs() < 0.001)
        .unwrap_or(choices.len() - 1)
        + 1)
        % choices.len()]
}

#[derive(Resource, Default)]
pub struct PlayerInput {
    pub movement: Vec2,
    pub vertical: f32,
    pub sprint: bool,
    pub look: Vec2,
    pub drive: terra_core::DriveInput,
    pub connected_gamepads: usize,
    pub controller_name: String,
    pub menu_pressed: bool,
    pub menu_direction: IVec2,
    pub menu_accept: bool,
    pub menu_back: bool,
    pub menu_delete: bool,
    pub menu_shift: bool,
    pub release_pressed: bool,
    pub suppress: bool,
    /// Management focus consumes world intent without disabling menu controls.
    pub ui_navigation: bool,
    pub command_consumed: bool,
    pub command_toggle: bool,
    pub menu_page: i8,
    pub pad_available: bool,
    pub pad_move: Vec2,
    pub pad_look: Vec2,
    pub pad_accept_held: bool,
    pub pad_accept_released: bool,
    pub pad_additive: bool,
    pub pad_cycle: bool,
    pub pad_zoom: f32,
    pressed: [bool; 15],
}
impl PlayerInput {
    pub fn just_pressed(&self, binding: Binding) -> bool {
        !self.world_blocked() && self.pressed[binding as usize]
    }
    pub fn world_blocked(&self) -> bool {
        self.suppress || self.ui_navigation
    }
    pub fn clear_world_bindings(&mut self) {
        self.pressed.fill(false);
        self.release_pressed = false;
    }
}

pub fn capture_consumes_activation(
    settings: &InputSettings,
    input: &PlayerInput,
    keys: &ButtonInput<KeyCode>,
) -> bool {
    (settings.listening.is_some() || input.suppress)
        && (keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter))
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerInputSet;
pub struct PlayerInputPlugin;
#[derive(Component)]
pub struct ControllerStatus;
impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .add_plugins(crate::accessibility::AccessibleUiPlugin)
            .add_systems(
                PreUpdate,
                sample
                    .in_set(PlayerInputSet)
                    .after(InputSystems)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch),
            )
            .add_systems(Update, controller_status);
    }
}

fn controller_status(
    input: Res<PlayerInput>,
    settings: Res<InputSettings>,
    mut labels: Query<&mut Text, With<ControllerStatus>>,
) {
    let value = format!(
        "{} controller(s) detected{}{}\n{}",
        input.connected_gamepads,
        if input.controller_name.is_empty() {
            String::new()
        } else {
            format!(" · {}", input.controller_name)
        },
        if settings.gamepad_enabled {
            ""
        } else {
            " · Standard gamepad disabled"
        },
        settings.wheel_live.status(&settings.wheel)
    );
    for mut label in &mut labels {
        if label.0 != value {
            label.0.clone_from(&value);
        }
    }
}

/// Radial deadzone avoids directional bias and normalizes diagonal stick input.
pub fn stick(value: Vec2, deadzone: f32) -> Vec2 {
    if !value.is_finite() || !deadzone.is_finite() || !(0.0..1.0).contains(&deadzone) {
        return Vec2::ZERO;
    }
    let length = value.length();
    if length <= deadzone {
        Vec2::ZERO
    } else {
        value / length * ((length.min(1.) - deadzone) / (1. - deadzone))
    }
}
fn analog(value: Option<f32>) -> f32 {
    value.filter(|v| v.is_finite()).unwrap_or(0.).clamp(0., 1.)
}

/// Retain device ownership and require a neutral sample after a handoff.
/// Menus remain usable while world controls are waiting to rearm.
#[derive(Default)]
struct StandardPadArming {
    device: Option<Entity>,
    armed: bool,
}
impl StandardPadArming {
    fn observe(&mut self, device: Entity, pad: &Gamepad, deadzone: f32) -> bool {
        if self.device != Some(device) {
            self.device = Some(device);
            self.armed = false;
        }
        let neutral = stick(pad.left_stick(), deadzone) == Vec2::ZERO
            && stick(pad.right_stick(), deadzone) == Vec2::ZERO
            && analog(pad.get(GamepadButton::RightTrigger2)) == 0.
            && analog(pad.get(GamepadButton::LeftTrigger2)) == 0.
            && ![
                GamepadButton::South,
                GamepadButton::East,
                GamepadButton::West,
                GamepadButton::North,
                GamepadButton::Select,
                GamepadButton::LeftThumb,
                GamepadButton::LeftTrigger,
                GamepadButton::RightTrigger,
                GamepadButton::DPadLeft,
                GamepadButton::DPadRight,
                GamepadButton::DPadDown,
            ]
            .into_iter()
            .any(|button| pad.pressed(button));
        self.armed |= neutral;
        self.armed
    }
}

fn sample(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut settings: ResMut<InputSettings>,
    mut input: ResMut<PlayerInput>,
    mut pad_arming: Local<StandardPadArming>,
) {
    *input = PlayerInput::default();
    let profile = settings.wheel.clone();
    let devices_changed = settings
        .bypass_change_detection()
        .wheel_live
        .observe(pads.iter(), &profile);
    if devices_changed {
        settings.set_changed();
    }
    // Never retain pedal intent when the desktop window loses focus or a pad
    // disconnects. An absent pad naturally falls back to released keyboard input.
    if windows.iter().next().is_some_and(|window| !window.focused) {
        settings.bypass_change_detection().wheel_live.disarm();
        pad_arming.armed = false;
        input.suppress = true;
        return;
    }
    if let Some(binding) = settings.listening {
        pad_arming.armed = false;
        input.suppress = true;
        if keys.just_pressed(KeyCode::Escape)
            || (settings.gamepad_enabled
                && pads
                    .iter()
                    .any(|(_, pad, _)| pad.just_pressed(GamepadButton::East)))
        {
            settings.listening = None;
            settings.notice = "Binding cancelled. Your previous key remains active.".into();
        } else if let Some(key) = keys.get_just_pressed().copied().min() {
            match settings.bind(binding, key) {
                Ok(()) => {
                    settings.listening = None;
                    settings.notice = format!(
                        "{} now uses {}. Save controls to keep it.",
                        binding.label(),
                        settings.key_label(binding)
                    );
                }
                Err(error) => settings.notice = error,
            }
        }
        return;
    }
    let held = |b| keys.pressed(settings.key(b)) as u8 as f32;
    for b in Binding::ALL {
        input.pressed[b as usize] = keys.just_pressed(settings.key(b));
    }
    input.movement = Vec2::new(
        held(Binding::Right) - held(Binding::Left),
        held(Binding::Backward) - held(Binding::Forward),
    );
    input.vertical = held(Binding::Ascend) - held(Binding::Descend);
    input.sprint = held(Binding::SprintBrake) > 0.;
    input.drive = terra_core::DriveInput {
        throttle: (held(Binding::Forward) - held(Binding::Backward)) as f64,
        steering: input.movement.x as f64,
        brake: held(Binding::SprintBrake) as f64,
        handbrake: held(Binding::Handbrake) > 0.,
    };
    input.connected_gamepads = pads.iter().count();
    // A chosen wheel remains excluded from generic pad controls even while its
    // dedicated profile is disabled; disabling must not silently switch modes.
    let selected_wheel = settings.wheel_live.selected(&settings.wheel);
    let wheel_entity = selected_wheel.filter(|_| settings.wheel.enabled);
    let standard_pad = pads
        .iter()
        .filter(|(entity, _, _)| Some(*entity) != selected_wheel)
        .min_by_key(|(entity, _, _)| entity.to_bits());
    if let Some((entity, pad, name)) = standard_pad.filter(|_| settings.gamepad_enabled) {
        let armed = pad_arming.observe(entity, pad, settings.deadzone);
        input.pad_available = armed;
        input.controller_name = name
            .map(|n| n.as_str())
            .unwrap_or("Gamepad")
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if !armed {
            input
                .controller_name
                .push_str(" · release controls to resume world input");
        }
        let movement = stick(pad.left_stick(), settings.deadzone);
        if armed {
            input.pad_move = Vec2::new(movement.x, -movement.y);
            input.pad_look = stick(pad.right_stick(), settings.deadzone);
            input.pad_accept_held = pad.pressed(GamepadButton::South);
            input.pad_accept_released = pad.just_released(GamepadButton::South);
            input.pad_additive = pad.pressed(GamepadButton::RightTrigger);
            input.pad_cycle = pad.just_pressed(GamepadButton::LeftTrigger);
            input.pad_zoom = analog(pad.get(GamepadButton::RightTrigger2))
                - analog(pad.get(GamepadButton::LeftTrigger2));
        }
        input.command_toggle = pad.just_pressed(GamepadButton::RightThumb);
        input.menu_page = pad.just_pressed(GamepadButton::RightTrigger) as i8
            - pad.just_pressed(GamepadButton::LeftTrigger) as i8;
        if armed {
            input.movement =
                (input.movement + Vec2::new(movement.x, -movement.y)).clamp_length_max(1.);
            input.look = stick(pad.right_stick(), settings.deadzone) * settings.gamepad_sensitivity;
            input.vertical = (input.vertical
                + pad.pressed(GamepadButton::RightTrigger) as u8 as f32
                - pad.pressed(GamepadButton::LeftTrigger) as u8 as f32)
                .clamp(-1., 1.);
            input.sprint |= pad.pressed(GamepadButton::LeftThumb);
            input.drive.steering = (input.drive.steering + movement.x as f64).clamp(-1., 1.);
            let acceleration = analog(pad.get(GamepadButton::RightTrigger2));
            let reverse = pad.pressed(GamepadButton::West);
            input.drive.throttle = (input.drive.throttle
                + if reverse { -acceleration } else { acceleration } as f64)
                .clamp(-1., 1.);
            input.drive.brake = input
                .drive
                .brake
                .max(analog(pad.get(GamepadButton::LeftTrigger2)) as f64);
            input.drive.handbrake |= pad.pressed(GamepadButton::East);
        }
        input.menu_pressed = pad.just_pressed(GamepadButton::Start);
        input.menu_direction = IVec2::new(
            (pad.pressed(GamepadButton::DPadRight) || movement.x > 0.55) as i32
                - (pad.pressed(GamepadButton::DPadLeft) || movement.x < -0.55) as i32,
            (pad.pressed(GamepadButton::DPadDown) || movement.y < -0.55) as i32
                - (pad.pressed(GamepadButton::DPadUp) || movement.y > 0.55) as i32,
        );
        input.menu_accept = pad.just_pressed(GamepadButton::South);
        input.menu_back = pad.just_pressed(GamepadButton::East);
        input.menu_delete = pad.just_pressed(GamepadButton::West);
        input.menu_shift = pad.just_pressed(GamepadButton::North);
        if armed {
            input.release_pressed = pad.just_pressed(GamepadButton::Select);
            for (binding, button) in [
                (Binding::Interact, GamepadButton::South),
                (Binding::Camera, GamepadButton::North),
                (Binding::Park, GamepadButton::RightTrigger),
                (Binding::Lights, GamepadButton::LeftTrigger),
                (Binding::LeftSignal, GamepadButton::DPadLeft),
                (Binding::RightSignal, GamepadButton::DPadRight),
                (Binding::Hazards, GamepadButton::DPadDown),
            ] {
                input.pressed[binding as usize] |= pad.just_pressed(button);
            }
        }
    } else {
        *pad_arming = StandardPadArming::default();
    }
    if let Some((_, wheel, name)) = wheel_entity.and_then(|entity| pads.get(entity).ok()) {
        settings.wheel.merge_drive(
            &settings.wheel_live,
            &mut input.drive,
            wheel.pressed(GamepadButton::West),
            wheel.pressed(GamepadButton::East),
        );
        let wheel_name = name
            .map_or("Wheel / joystick", |name| name.as_str())
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect::<String>();
        if input.controller_name.is_empty() {
            input.controller_name = wheel_name;
        } else {
            input.controller_name.push_str(&format!(" + {wheel_name}"));
        }
    }
}

pub const GAMEPAD_HELP: &str = "Gamepad in menus\nD-pad / left stick · move focus   A / South · select\nB / East · back   Select a text field to open the keyboard\nKeyboard: X / West · delete   Y / North · capitals\nStart · apply text   B / East · discard text\n\nGamepad in the world\nAfter focus loss or changing controllers, release sticks, triggers and world buttons before resuming. Menus and keyboard remain available.\nLeft stick · move / steer   Right stick · look\nRT · accelerate   LT · brake   X / West + RT · reverse\nA / South · interact   B / East · handbrake\nY / North · switch camera   RB · park   LB · lights\nD-pad left / right · indicators   D-pad down · hazards\nLeft stick click · sprint   Select · RTS   Start · session menu\nOn aircraft: RB / LB · ascend / descend\nRight stick click / F6 · management focus. D-pad / arrows / Tab navigate; A / Enter chooses; RB/LB or Page Up/Down changes page; B / Escape returns. Names use the same on-screen keyboard. In RTS: left stick moves the reticle; hold A and move to select a group; RB adds to selection; X orders movement; B stops; Y possesses; LB cycles actors. Right stick pans, triggers zoom. During construction A places and B cancels. Move the mouse to return to pointer control. Wheel / joystick driving: enable the profile below, choose a detected device, choose steering and pedal axes, and capture neutral / endpoints for every channel. Combined pedals share one signed axis. Review live values before driving. Wheel X / West + pedal reverses; B / East is handbrake when mapped by the device. Keyboard controls remain available. Force feedback and hardware certification are not supplied.";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_pad_rearm_requires_zero_triggers_instead_of_stick_deadzone() {
        let mut arming = StandardPadArming::default();
        let device = Entity::PLACEHOLDER;
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.1);
        assert!(!arming.observe(device, &pad, 0.15));
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.);
        pad.analog_mut().set(GamepadButton::LeftTrigger2, 0.1);
        assert!(!arming.observe(device, &pad, 0.15));
        pad.analog_mut().set(GamepadButton::LeftTrigger2, 0.);
        assert!(arming.observe(device, &pad, 0.15));
    }

    #[test]
    fn standard_pad_neutral_rearm_preserves_menus_keyboard_and_device_handoffs() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .add_systems(Update, sample);
        let first = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        assert!(app.world().resource::<PlayerInput>().pad_available);
        app.world_mut()
            .get_mut::<Gamepad>(first)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.8);
        app.update();
        assert!(app.world().resource::<PlayerInput>().drive.throttle > 0.7);
        let window = app
            .world_mut()
            .spawn((
                bevy::window::PrimaryWindow,
                Window {
                    focused: false,
                    ..default()
                },
            ))
            .id();
        app.update();
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        {
            let mut pad = app.world_mut().get_mut::<Gamepad>(first).unwrap();
            pad.analog_mut().set(GamepadAxis::LeftStickX, 0.8);
            pad.digital_mut().press(GamepadButton::Start);
            pad.digital_mut().press(GamepadButton::South);
        }
        app.update();
        let input = app.world().resource::<PlayerInput>();
        assert_eq!(input.drive.throttle, 0.);
        assert_eq!(input.movement, Vec2::ZERO);
        assert_eq!(input.pad_move, Vec2::ZERO);
        assert_eq!(input.pad_zoom, 0.);
        assert!(!input.pad_available && !input.just_pressed(Binding::Interact));
        assert!(input.menu_pressed && input.menu_accept);
        assert_eq!(input.menu_direction, IVec2::X);
        assert!(input.controller_name.contains("release controls"));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 1.);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        {
            let mut pad = app.world_mut().get_mut::<Gamepad>(first).unwrap();
            pad.analog_mut().set(GamepadButton::RightTrigger2, 0.);
            pad.analog_mut().set(GamepadAxis::LeftStickX, 0.);
            pad.digital_mut().reset_all();
        }
        app.update();
        assert!(app.world().resource::<PlayerInput>().pad_available);
        let mut second_pad = Gamepad::default();
        second_pad
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.9);
        let second = app.world_mut().spawn(second_pad).id();
        app.world_mut().despawn(first);
        app.update();
        assert!(!app.world().resource::<PlayerInput>().pad_available);
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        app.world_mut()
            .get_mut::<Gamepad>(second)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.);
        app.update();
        assert!(app.world().resource::<PlayerInput>().pad_available);
        app.world_mut()
            .get_mut::<Gamepad>(second)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.9);
        app.update();
        assert!(app.world().resource::<PlayerInput>().drive.throttle > 0.8);
        app.world_mut()
            .resource_mut::<InputSettings>()
            .gamepad_enabled = false;
        app.update();
        app.world_mut()
            .resource_mut::<InputSettings>()
            .gamepad_enabled = true;
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        // Binding capture is another ownership handoff: no held pad movement
        // may resume when keyboard capture ends.
        app.world_mut()
            .get_mut::<Gamepad>(second)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.);
        app.update();
        app.world_mut().resource_mut::<InputSettings>().listening = Some(Binding::Forward);
        app.update();
        app.world_mut().resource_mut::<InputSettings>().listening = None;
        app.world_mut()
            .get_mut::<Gamepad>(second)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.9);
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
    }

    #[test]
    fn session_handoff_keeps_unsaved_choices_without_capturing_or_writing_a_key() {
        let mut settings = InputSettings::default();
        settings.bind(Binding::Forward, KeyCode::ArrowUp).unwrap();
        settings.listening = Some(Binding::Backward);
        settings.invert_y = true;
        let payload = settings.to_handoff().unwrap();
        assert!(payload.len() < 4096);
        assert!(!payload.contains("listening"));
        let resumed = InputSettings::from_handoff(&payload).unwrap();
        assert_eq!(resumed.key(Binding::Forward), KeyCode::ArrowUp);
        assert!(resumed.invert_y && resumed.listening.is_none());
        assert!(resumed.rts_hint().contains("Up arrow"));
        assert!(InputSettings::from_handoff(&" ".repeat(4097)).is_err());
        assert!(InputSettings::from_handoff(r#"{"schema_version":2}"#).is_err());
        let before = settings.key(Binding::Backward);
        settings.cancel_capture();
        assert!(settings.listening.is_none());
        assert_eq!(settings.key(Binding::Backward), before);
        assert!(!settings.space_has_rts_action());
        settings.bind(Binding::Handbrake, KeyCode::KeyQ).unwrap();
        settings.bind(Binding::Forward, KeyCode::Space).unwrap();
        assert!(settings.space_has_rts_action());
    }
    #[test]
    fn rebinding_rejects_conflicts_reserved_keys_and_invalid_saved_settings() {
        let mut s = InputSettings::default();
        let before = s.bindings;
        assert!(s.bind(Binding::Forward, KeyCode::Escape).is_err());
        assert!(s.bind(Binding::Forward, KeyCode::KeyS).is_err());
        assert_eq!(s.bindings, before);
        s.bind(Binding::Forward, KeyCode::ArrowUp).unwrap();
        assert_eq!(s.key(Binding::Forward), KeyCode::ArrowUp);
        let roundtrip: InputSettings =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        roundtrip.validate().unwrap();
        assert_eq!(roundtrip.key(Binding::Forward), KeyCode::ArrowUp);
        s.deadzone = f32::NAN;
        assert!(s.validate().is_err());
        s = InputSettings::default();
        s.bindings[0] = 255;
        assert!(s.validate().is_err());
    }
    #[test]
    fn radial_deadzone_rejects_noise_nonfinite_values_and_keeps_diagonals_bounded() {
        assert_eq!(stick(Vec2::new(0.1, 0.), 0.15), Vec2::ZERO);
        assert_eq!(stick(Vec2::splat(f32::NAN), 0.15), Vec2::ZERO);
        assert_eq!(stick(Vec2::X, 1.), Vec2::ZERO);
        let d = stick(Vec2::ONE, 0.15);
        assert!((d.length() - 1.).abs() < 1e-6);
        assert!((d.x - d.y).abs() < 1e-6);
        assert!(stick(Vec2::new(0.150001, 0.), 0.15).length() < 0.001);
    }
    #[test]
    fn control_file_roundtrip_is_independent_of_world_and_defaults_are_compatible() {
        let p = std::env::temp_dir().join(format!("terra-controls-{}", std::process::id()));
        let mut s = InputSettings::default();
        s.bind(Binding::Forward, KeyCode::KeyI).unwrap();
        s.invert_y = true;
        s.write(&p).unwrap();
        let loaded = InputSettings::read(&p);
        assert!(loaded.invert_y);
        assert_eq!(loaded.key(Binding::Forward), KeyCode::KeyI);
        assert!(loaded.listening.is_none());
        let _ = std::fs::remove_dir_all(p);
        serde_json::from_str::<InputSettings>("{}")
            .unwrap()
            .validate()
            .unwrap();
    }
    #[test]
    fn gamepad_disconnect_and_focus_loss_release_drive_and_do_not_replay_actions() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .add_systems(Update, sample);
        let entity = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        let mut pad = app.world_mut().get_mut::<Gamepad>(entity).unwrap();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.7);
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.5);
        drop(pad);
        app.update();
        assert!(app.world().resource::<PlayerInput>().drive.throttle > 0.69);
        assert!(app.world().resource::<PlayerInput>().drive.steering > 0.);
        app.world_mut().despawn(entity);
        app.update();
        let input = app.world().resource::<PlayerInput>();
        assert_eq!(input.drive.throttle, 0.);
        assert_eq!(input.connected_gamepads, 0);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut().spawn((
            bevy::window::PrimaryWindow,
            Window {
                focused: false,
                ..default()
            },
        ));
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        assert!(app.world().resource::<PlayerInput>().suppress);
    }
    #[test]
    fn selected_wheel_is_not_sampled_as_a_gamepad_and_focus_loss_requires_neutral() {
        use crate::wheel_input::{WheelDevice, WheelSettings};
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .add_systems(Update, sample);
        let entity = app
            .world_mut()
            .spawn((Gamepad::default(), Name::new("Test wheel")))
            .id();
        app.world_mut().resource_mut::<InputSettings>().wheel = WheelSettings {
            enabled: true,
            device: Some(WheelDevice {
                name: "Test wheel".into(),
                vendor: None,
                product: None,
                occurrence: 0,
            }),
            steering_calibrated: true,
            accelerator_calibrated: true,
            brake_calibrated: true,
            ..Default::default()
        };
        app.update();
        let mut pad = app.world_mut().get_mut::<Gamepad>(entity).unwrap();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.8);
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.6);
        drop(pad);
        app.update();
        let input = app.world().resource::<PlayerInput>();
        assert!(input.drive.throttle > 0.7 && input.drive.steering > 0.5);
        assert_eq!(input.movement, Vec2::ZERO);
        assert_eq!(input.menu_direction, IVec2::ZERO);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 1.);
        assert_eq!(app.world().resource::<PlayerInput>().drive.steering, -1.);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        let window = app
            .world_mut()
            .spawn((
                bevy::window::PrimaryWindow,
                Window {
                    focused: false,
                    ..Default::default()
                },
            ))
            .id();
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        let mut pad = app.world_mut().get_mut::<Gamepad>(entity).unwrap();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.);
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.);
        drop(pad);
        app.update();
        app.world_mut()
            .get_mut::<Gamepad>(entity)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.8);
        app.update();
        assert!(app.world().resource::<PlayerInput>().drive.throttle > 0.7);
        app.world_mut()
            .resource_mut::<InputSettings>()
            .wheel
            .enabled = false;
        app.world_mut()
            .get_mut::<Gamepad>(entity)
            .unwrap()
            .analog_mut()
            .set(GamepadAxis::LeftStickX, 0.6);
        app.update();
        let input = app.world().resource::<PlayerInput>();
        assert_eq!(input.drive.throttle, 0.);
        assert_eq!(input.drive.steering, 0.);
        assert_eq!(input.movement, Vec2::ZERO);
        assert_eq!(input.menu_direction, IVec2::ZERO);
        app.world_mut().despawn(entity);
        app.update();
        assert_eq!(app.world().resource::<PlayerInput>().drive.throttle, 0.);
        assert_eq!(app.world().resource::<PlayerInput>().drive.steering, 0.);
    }
}
