//! Player settings: sound, interface scale, window mode and the island camera.
//!
//! Host data only. The file sits next to the island save, is never part of a
//! snapshot, hash or replay, and a missing or damaged file falls back to
//! defaults field by field instead of blocking the game.
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use testbed_view::game_ui::scale::InterfaceScale;
use testbed_view::game_ui::sound::AudioLevels;
use testbed_view::game_ui::tokens::{PanelStyle, Theme};
use testbed_view::game_ui::tooltip::TooltipDelay;
use testbed_view::{
    AmbientMotion, IslandAudioSettings, IslandCameraSettings, IslandTips, ReduceMotion,
};

pub(crate) const FILE_NAME: &str = "island-settings.json";
const VOLUME_STEP: i32 = 10;
const SCALE_STEP: i32 = 10;
const SCALE_RANGE: (i32, i32) = (80, 140);
const PAN_STEP: i32 = 25;
/// The same range as `IslandCameraSettings::PAN_SPEED_RANGE`, in percent.
const PAN_RANGE: (i32, i32) = (50, 200);

/// Percentages keep the file readable and hand-editable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostSettings {
    pub(crate) master: u8,
    pub(crate) ambience: u8,
    pub(crate) effects: u8,
    /// Music, percent.
    pub(crate) music: u8,
    /// The interface's own sounds (clicks, pages, alerts), percent.
    pub(crate) interface_sounds: u8,
    pub(crate) muted: bool,
    pub(crate) ui_scale: u8,
    /// Follow the window: the interface size is the automatic factor times
    /// `ui_scale`. Off, `ui_scale` stands alone.
    pub(crate) ui_scale_auto: bool,
    /// Dark "Dusk" panels instead of parchment.
    pub(crate) dusk: bool,
    pub(crate) borderless: bool,
    /// Pan the island camera when the cursor rests at a window edge.
    pub(crate) edge_pan: bool,
    /// Keyboard and edge pan speed, percent of the authored speed.
    pub(crate) pan_speed: u8,
    /// Pulses and slides are replaced by plain changes.
    pub(crate) reduce_motion: bool,
    /// How long the pointer rests before a tooltip: 0 short, 1 normal, 2 long.
    pub(crate) tooltip_delay: u8,
    /// First-run tips and the goal tracker's hints.
    pub(crate) show_tips: bool,
    /// The first-run tips already dismissed, one bit each (see `IslandTips`).
    pub(crate) tips_seen: u16,
}

impl Default for HostSettings {
    fn default() -> Self {
        // The same levels as `IslandAudioSettings::default()`.
        Self {
            master: 60,
            ambience: 45,
            effects: 55,
            music: 40,
            interface_sounds: 50,
            muted: false,
            ui_scale: 100,
            ui_scale_auto: true,
            dusk: false,
            borderless: false,
            edge_pan: true,
            pan_speed: 100,
            reduce_motion: false,
            tooltip_delay: 1,
            show_tips: true,
            tips_seen: 0,
        }
    }
}

/// One step of one control on the Settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    Master(i8),
    Ambience(i8),
    Effects(i8),
    Music(i8),
    InterfaceSounds(i8),
    Mute,
    UiScale(i8),
    EdgePan,
    PanSpeed(i8),
    /// A slider set a value outright (percent).
    SetMaster(u8),
    SetAmbience(u8),
    SetEffects(u8),
    SetMusic(u8),
    SetInterfaceSounds(u8),
    SetUiScale(u8),
    SetPanSpeed(u8),
    /// A segmented control chose one of its values.
    SetUiScaleAuto(bool),
    SetDusk(bool),
    SetBorderless(bool),
    SetTooltipDelay(u8),
    ReduceMotion,
    ShowTips,
    /// A first-run tip was dismissed.
    TipSeen(u8),
    /// Show every first-run tip again.
    ResetTips,
}

impl Change {
    /// A slider's own change: it arrives many times while the thumb is
    /// dragged, so the screen must not be rebuilt under the pointer.
    pub(crate) fn continuous(self) -> bool {
        matches!(
            self,
            Self::SetMaster(_)
                | Self::SetAmbience(_)
                | Self::SetEffects(_)
                | Self::SetMusic(_)
                | Self::SetInterfaceSounds(_)
                | Self::SetUiScale(_)
                | Self::SetPanSpeed(_)
        )
    }
}

/// A value from a slider, snapped to the control's step and range.
fn snapped(value: u8, step: i32, range: (i32, i32)) -> u8 {
    let value = i32::from(value).clamp(range.0, range.1);
    (((value - range.0 + step / 2) / step) * step + range.0).clamp(range.0, range.1) as u8
}

fn stepped(value: u8, direction: i8, step: i32, range: (i32, i32)) -> u8 {
    (i32::from(value) + i32::from(direction.signum()) * step).clamp(range.0, range.1) as u8
}

impl HostSettings {
    /// Parse leniently: any field that is absent, mistyped or out of range
    /// takes its default (or the nearest allowed value); other fields survive.
    pub(crate) fn from_json(text: &str) -> Self {
        let defaults = Self::default();
        let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(text) else {
            return defaults;
        };
        let percent = |key: &str, default: u8, range: (i32, i32)| {
            fields
                .get(key)
                .and_then(Value::as_i64)
                .map_or(default, |value| {
                    value.clamp(i64::from(range.0), i64::from(range.1)) as u8
                })
        };
        let flag =
            |key: &str, default: bool| fields.get(key).and_then(Value::as_bool).unwrap_or(default);
        Self {
            master: percent("master_volume", defaults.master, (0, 100)),
            ambience: percent("ambience_volume", defaults.ambience, (0, 100)),
            effects: percent("effects_volume", defaults.effects, (0, 100)),
            music: percent("music_volume", defaults.music, (0, 100)),
            interface_sounds: percent("interface_volume", defaults.interface_sounds, (0, 100)),
            muted: flag("muted", defaults.muted),
            ui_scale: percent("ui_scale", defaults.ui_scale, SCALE_RANGE),
            ui_scale_auto: flag("ui_scale_auto", defaults.ui_scale_auto),
            dusk: fields
                .get("panel_style")
                .and_then(Value::as_str)
                .map_or(defaults.dusk, |style| style == "dusk"),
            borderless: fields
                .get("window_mode")
                .and_then(Value::as_str)
                .map_or(defaults.borderless, |mode| mode == "borderless"),
            edge_pan: flag("edge_pan", defaults.edge_pan),
            pan_speed: percent("pan_speed", defaults.pan_speed, PAN_RANGE),
            reduce_motion: flag("reduce_motion", defaults.reduce_motion),
            tooltip_delay: fields.get("tooltip_delay").and_then(Value::as_str).map_or(
                defaults.tooltip_delay,
                |delay| match delay {
                    "short" => 0,
                    "long" => 2,
                    _ => 1,
                },
            ),
            show_tips: flag("show_tips", defaults.show_tips),
            tips_seen: fields.get("tips_seen").and_then(Value::as_array).map_or(
                defaults.tips_seen,
                |seen| {
                    seen.iter()
                        .filter_map(Value::as_str)
                        .filter_map(|name| IslandTips::NAMES.iter().position(|tip| *tip == name))
                        .fold(0, |bits, index| bits | (1 << index))
                },
            ),
        }
    }
    pub(crate) fn to_json(self) -> String {
        let delay = ["short", "normal", "long"][usize::from(self.tooltip_delay.min(2))];
        let tips: Vec<&str> = IslandTips::NAMES
            .iter()
            .enumerate()
            .filter(|(index, _)| self.tips_seen & (1 << index) != 0)
            .map(|(_, name)| *name)
            .collect();
        let value = json!({
            "master_volume": self.master,
            "ambience_volume": self.ambience,
            "effects_volume": self.effects,
            "music_volume": self.music,
            "interface_volume": self.interface_sounds,
            "muted": self.muted,
            "ui_scale": self.ui_scale,
            "ui_scale_auto": self.ui_scale_auto,
            "panel_style": if self.dusk { "dusk" } else { "parchment" },
            "window_mode": if self.borderless { "borderless" } else { "windowed" },
            "edge_pan": self.edge_pan,
            "pan_speed": self.pan_speed,
            "reduce_motion": self.reduce_motion,
            "tooltip_delay": delay,
            "show_tips": self.show_tips,
            "tips_seen": tips,
        });
        serde_json::to_string_pretty(&value).expect("settings serialize")
    }
    pub(crate) fn changed(mut self, change: Change) -> Self {
        match change {
            Change::Master(by) => self.master = stepped(self.master, by, VOLUME_STEP, (0, 100)),
            Change::Ambience(by) => {
                self.ambience = stepped(self.ambience, by, VOLUME_STEP, (0, 100));
            }
            Change::Effects(by) => self.effects = stepped(self.effects, by, VOLUME_STEP, (0, 100)),
            Change::Music(by) => self.music = stepped(self.music, by, VOLUME_STEP, (0, 100)),
            Change::InterfaceSounds(by) => {
                self.interface_sounds = stepped(self.interface_sounds, by, VOLUME_STEP, (0, 100));
            }
            Change::Mute => self.muted = !self.muted,
            Change::UiScale(by) => {
                self.ui_scale = stepped(self.ui_scale, by, SCALE_STEP, SCALE_RANGE);
            }
            Change::EdgePan => self.edge_pan = !self.edge_pan,
            Change::PanSpeed(by) => {
                self.pan_speed = stepped(self.pan_speed, by, PAN_STEP, PAN_RANGE);
            }
            Change::SetMaster(value) => self.master = snapped(value, VOLUME_STEP, (0, 100)),
            Change::SetAmbience(value) => self.ambience = snapped(value, VOLUME_STEP, (0, 100)),
            Change::SetEffects(value) => self.effects = snapped(value, VOLUME_STEP, (0, 100)),
            Change::SetMusic(value) => self.music = snapped(value, VOLUME_STEP, (0, 100)),
            Change::SetInterfaceSounds(value) => {
                self.interface_sounds = snapped(value, VOLUME_STEP, (0, 100));
            }
            Change::SetUiScale(value) => self.ui_scale = snapped(value, SCALE_STEP, SCALE_RANGE),
            Change::SetPanSpeed(value) => self.pan_speed = snapped(value, PAN_STEP, PAN_RANGE),
            Change::SetUiScaleAuto(auto) => self.ui_scale_auto = auto,
            Change::SetDusk(dusk) => self.dusk = dusk,
            Change::SetBorderless(borderless) => self.borderless = borderless,
            Change::SetTooltipDelay(delay) => self.tooltip_delay = delay.min(2),
            Change::ReduceMotion => self.reduce_motion = !self.reduce_motion,
            Change::ShowTips => self.show_tips = !self.show_tips,
            Change::TipSeen(index) => {
                if usize::from(index) < IslandTips::NAMES.len() {
                    self.tips_seen |= 1 << index;
                }
            }
            Change::ResetTips => {
                self.tips_seen = 0;
                self.show_tips = true;
            }
        }
        self
    }
    pub(crate) fn tooltips(self) -> TooltipDelay {
        match self.tooltip_delay {
            0 => TooltipDelay::Short,
            2 => TooltipDelay::Long,
            _ => TooltipDelay::Normal,
        }
    }
    pub(crate) fn tips(self) -> (bool, u16) {
        (self.show_tips, self.tips_seen)
    }
    /// Whether a step in this direction would still change the value.
    pub(crate) fn can(self, change: Change) -> bool {
        self.changed(change) != self
    }
    pub(crate) fn audio(self, forced_mute: bool) -> IslandAudioSettings {
        IslandAudioSettings {
            master: f32::from(self.master) / 100.0,
            ambience: f32::from(self.ambience) / 100.0,
            effects: f32::from(self.effects) / 100.0,
            muted: self.muted || forced_mute,
        }
    }
    /// Every channel's level, 0 to 1, as the island's sound reads them; a
    /// mute (the setting or a forced one) silences all of them.
    pub(crate) fn levels(self, forced_mute: bool) -> [f32; 5] {
        if self.muted || forced_mute {
            return [0.0; 5];
        }
        [
            self.master,
            self.ambience,
            self.effects,
            self.music,
            self.interface_sounds,
        ]
        .map(|percent| f32::from(percent) / 100.0)
    }
    pub(crate) fn interface(self) -> InterfaceScale {
        InterfaceScale {
            auto: self.ui_scale_auto,
            percent: self.ui_scale,
        }
    }
    pub(crate) fn theme(self) -> Theme {
        Theme {
            style: if self.dusk {
                PanelStyle::Dusk
            } else {
                PanelStyle::Parchment
            },
        }
    }
    pub(crate) fn camera(self) -> IslandCameraSettings {
        IslandCameraSettings {
            edge_pan: self.edge_pan,
            pan_speed: f32::from(self.pan_speed) / 100.0,
        }
    }
}

pub(crate) fn file_beside(save: &Path) -> PathBuf {
    save.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(FILE_NAME)
}

pub(crate) fn load(path: &Path) -> HostSettings {
    std::fs::read_to_string(path).map_or_else(
        |_| HostSettings::default(),
        |text| HostSettings::from_json(&text),
    )
}

/// Write beside the save through a temporary file, so a failed write never
/// leaves a truncated settings file behind.
pub(crate) fn store(path: &Path, settings: HostSettings) -> Result<(), String> {
    let problem = |error: std::io::Error| format!("Settings were not saved: {error}");
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(problem)?;
    }
    let staged = path.with_extension("json.tmp");
    std::fs::write(&staged, settings.to_json()).map_err(problem)?;
    std::fs::rename(&staged, path).map_err(problem)
}

#[derive(Resource, Default)]
pub(crate) struct SettingsState {
    pub(crate) current: HostSettings,
    path: Option<PathBuf>,
    /// `--mute` and capture runs stay silent whatever the file says.
    forced_mute: bool,
    /// [`TIPS_SEEN_ENV`] is set: every first-run tip counts as seen for this
    /// run, whatever the file says (captures of prepared saves). Never
    /// written to the file.
    tips_seen_all: bool,
    applied: Option<(HostSettings, bool)>,
}

/// The environment variable that starts a run with every first-run tip
/// seen: `ISLAND_TIPS_SEEN=all` (also `1` or `true`). Capture scripts set it
/// so a tip never covers the scene; the settings file is not changed.
pub(crate) const TIPS_SEEN_ENV: &str = "ISLAND_TIPS_SEEN";

/// Whether the variable's value asks for every tip to count as seen.
pub(crate) fn tips_seen_all(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "all" | "1" | "true"
        )
    })
}

pub(crate) fn install(app: &mut App) {
    let forced_mute = app
        .world()
        .get_resource::<IslandAudioSettings>()
        .is_some_and(|audio| audio.muted);
    // Host data, read once: which tips this run has seen.
    let tips_seen_all = tips_seen_all(std::env::var(TIPS_SEEN_ENV).ok().as_deref());
    app.insert_resource(SettingsState {
        forced_mute,
        tips_seen_all,
        ..default()
    })
    .add_systems(Startup, read)
    .add_systems(Update, (remember_tips, apply).chain());
}

/// The save path is known once the session is installed, before Startup.
fn read(world: &mut World) {
    let Some(path) = crate::island::save_path(world).map(|save| file_beside(&save)) else {
        return;
    };
    let mut state = world.resource_mut::<SettingsState>();
    state.current = load(&path);
    state.path = Some(path);
}

/// Change one setting and persist it. A failed write keeps the new value for
/// this session and reports the problem to the caller.
pub(crate) fn change(world: &mut World, change: Change) -> Result<(), String> {
    let mut state = world.resource_mut::<SettingsState>();
    state.current = state.current.changed(change);
    let (settings, path) = (state.current, state.path.clone());
    path.map_or(Ok(()), |path| store(&path, settings))
}

fn apply(world: &mut World) {
    let (settings, forced_mute) = {
        let state = world.resource::<SettingsState>();
        if state.applied == Some((state.current, state.forced_mute)) {
            return;
        }
        (state.current, state.forced_mute)
    };
    world.insert_resource(settings.audio(forced_mute));
    // Every channel the island's sound mixes, a mute as silence on all.
    let [master, ambience, effects, music, ui] = settings.levels(forced_mute);
    let levels = AudioLevels {
        master,
        ambience,
        effects,
        music,
        ui,
    };
    if world.get_resource::<AudioLevels>() != Some(&levels) {
        world.insert_resource(levels);
    }
    if world.get_resource::<IslandCameraSettings>() != Some(&settings.camera()) {
        world.insert_resource(settings.camera());
    }
    // The view turns the setting and the window into the UI scale.
    if world.get_resource::<InterfaceScale>() != Some(&settings.interface()) {
        world.insert_resource(settings.interface());
    }
    if world.get_resource::<Theme>() != Some(&settings.theme()) {
        world.insert_resource(settings.theme());
    }
    world.insert_resource(settings.tooltips());
    world.insert_resource(ReduceMotion(settings.reduce_motion));
    // The island's own motion (wind in trees, water, clouds, boats) stops
    // with Reduce motion; the simulation's clock is untouched.
    let ambient = AmbientMotion {
        scale: if settings.reduce_motion { 0.0 } else { 1.0 },
    };
    if world.get_resource::<AmbientMotion>() != Some(&ambient) {
        world.insert_resource(ambient);
    }
    {
        let (show, seen) = settings.tips();
        let seen = if world.resource::<SettingsState>().tips_seen_all {
            u16::MAX >> (16 - IslandTips::NAMES.len().min(16))
        } else {
            seen
        };
        let mut tips = world.get_resource_or_insert_with(IslandTips::default);
        if (tips.show, tips.seen) != (show, seen) {
            tips.show = show;
            tips.seen = seen;
        }
    }
    let mode = if settings.borderless {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    for mut window in world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .iter_mut(world)
    {
        if window.mode != mode {
            window.mode = mode;
        }
    }
    world.resource_mut::<SettingsState>().applied = Some((settings, forced_mute));
}

/// A first-run tip the player dismissed on the island is remembered in the
/// settings file, so it never comes back unless they ask for the tips again.
pub(crate) fn remember_tips(world: &mut World) {
    let dismissed = world
        .get_resource_mut::<IslandTips>()
        .map(|mut tips| std::mem::take(&mut tips.dismissed))
        .unwrap_or_default();
    for index in dismissed {
        // A failed write keeps the tip dismissed for this session.
        let _ = change(world, Change::TipSeen(index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_their_file() {
        let folder = std::env::temp_dir().join(format!("npc-settings-{}", std::process::id()));
        let path = file_beside(&folder.join("living-world.world.json"));
        assert_eq!(path.file_name().unwrap(), FILE_NAME);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(
            load(&path),
            HostSettings::default(),
            "a missing file is fine"
        );

        let chosen = HostSettings::default()
            .changed(Change::Master(-1))
            .changed(Change::Effects(1))
            .changed(Change::Music(1))
            .changed(Change::InterfaceSounds(-1))
            .changed(Change::Mute)
            .changed(Change::UiScale(1))
            .changed(Change::SetUiScaleAuto(false))
            .changed(Change::SetDusk(true))
            .changed(Change::SetBorderless(true))
            .changed(Change::EdgePan)
            .changed(Change::PanSpeed(1))
            .changed(Change::ReduceMotion)
            .changed(Change::SetTooltipDelay(2))
            .changed(Change::ShowTips)
            .changed(Change::TipSeen(0))
            .changed(Change::TipSeen(4));
        assert_eq!(
            chosen,
            HostSettings {
                master: 50,
                ambience: 45,
                effects: 65,
                music: 50,
                interface_sounds: 40,
                muted: true,
                ui_scale: 110,
                ui_scale_auto: false,
                dusk: true,
                borderless: true,
                edge_pan: false,
                pan_speed: 125,
                reduce_motion: true,
                tooltip_delay: 2,
                show_tips: false,
                tips_seen: 0b1_0001,
            }
        );
        store(&path, chosen).unwrap();
        assert_eq!(load(&path), chosen);
        // The new settings are words in the file, not numbers.
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("\"tooltip_delay\": \"long\""), "{written}");
        assert!(written.contains("\"oversee_totals\"") && written.contains("\"live\""));
        // A slider sets a value outright, snapped to the control's step.
        assert_eq!(chosen.changed(Change::SetMaster(74)).master, 70);
        assert_eq!(chosen.changed(Change::SetUiScale(200)).ui_scale, 140);
        assert_eq!(chosen.changed(Change::SetPanSpeed(60)).pan_speed, 50);
        assert!(Change::SetMaster(1).continuous() && !Change::Master(1).continuous());
        // Showing the tips again forgets what was dismissed and switches them on.
        let again = chosen.changed(Change::ResetTips);
        assert_eq!(again.tips(), (true, 0));
        assert_eq!(chosen.tooltips(), TooltipDelay::Long);
        // The file names both camera fields in plain words.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"edge_pan\": false") && text.contains("\"pan_speed\": 125"));
        assert!(text.contains("\"ui_scale_auto\": false"));
        // Music and the interface's sounds are saved and read back too.
        assert!(text.contains("\"music_volume\": 50") && text.contains("\"interface_volume\": 40"));
        assert_eq!(
            chosen.levels(false),
            [0.0; 5],
            "a mute silences every channel"
        );
        let loud = chosen.changed(Change::Mute);
        assert_eq!(loud.levels(false), [0.5, 0.45, 0.65, 0.5, 0.4]);
        assert_eq!(loud.levels(true), [0.0; 5], "a forced mute wins");
        assert!(text.contains("\"panel_style\": \"dusk\""));
        assert_eq!(
            chosen.interface(),
            InterfaceScale {
                auto: false,
                percent: 110
            }
        );
        assert_eq!(chosen.theme().style, PanelStyle::Dusk);
        assert_eq!(
            chosen.camera(),
            IslandCameraSettings {
                edge_pan: false,
                pan_speed: 1.25
            }
        );
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_field_by_field() {
        let defaults = HostSettings::default();
        for broken in ["", "{", "[1, 2]", "null", "\u{0}\u{1}not json"] {
            assert_eq!(HostSettings::from_json(broken), defaults, "{broken:?}");
        }
        let partial = HostSettings::from_json(
            r#"{"master_volume": 900, "ambience_volume": "loud", "muted": true,
                "ui_scale": 10, "window_mode": "borderless", "unknown": [1]}"#,
        );
        assert_eq!(
            partial,
            HostSettings {
                master: 100,
                ambience: defaults.ambience,
                effects: defaults.effects,
                music: defaults.music,
                interface_sounds: defaults.interface_sounds,
                muted: true,
                ui_scale: 80,
                ui_scale_auto: defaults.ui_scale_auto,
                dusk: defaults.dusk,
                borderless: true,
                edge_pan: defaults.edge_pan,
                pan_speed: defaults.pan_speed,
                reduce_motion: defaults.reduce_motion,
                tooltip_delay: defaults.tooltip_delay,
                show_tips: defaults.show_tips,
                tips_seen: defaults.tips_seen,
            },
            "valid fields survive; bad ones are clamped or defaulted"
        );
        // A settings file written before the camera fields existed keeps
        // edge panning on at the authored speed; bad values are held in range.
        let old = HostSettings::from_json(r#"{"master_volume": 30, "window_mode": "windowed"}"#);
        assert!(old.edge_pan && old.pan_speed == 100 && old.master == 30);
        // A file written before automatic interface size keeps its own
        // percentage and gains the automatic factor; parchment is the default.
        let before_auto = HostSettings::from_json(r#"{"ui_scale": 120}"#);
        assert!(before_auto.ui_scale_auto && before_auto.ui_scale == 120 && !before_auto.dusk);
        assert!((before_auto.interface().factor(1920.0, 1080.0) - 1.8).abs() < 1e-5);
        let fixed = HostSettings::from_json(
            r#"{"ui_scale": 120, "ui_scale_auto": false, "panel_style": "dusk"}"#,
        );
        assert_eq!(fixed.interface().factor(1920.0, 1080.0), 1.2);
        assert!(fixed.dusk);
        assert!(!HostSettings::from_json(r#"{"panel_style": "neon", "ui_scale_auto": 3}"#).dusk);
        let odd = HostSettings::from_json(r#"{"edge_pan": "no", "pan_speed": 900}"#);
        assert!(odd.edge_pan, "a mistyped flag takes its default");
        assert_eq!(odd.pan_speed, 200);
        assert_eq!(
            HostSettings::from_json(r#"{"edge_pan": false, "pan_speed": 1}"#).camera(),
            IslandCameraSettings {
                edge_pan: false,
                pan_speed: 0.5
            }
        );
        assert!(!HostSettings::from_json(r#"{"window_mode": "cinema"}"#).borderless);
    }

    #[test]
    fn steps_stop_at_their_limits_and_mute_wins_over_volume() {
        let mut settings = HostSettings::default();
        for _ in 0..20 {
            settings = settings
                .changed(Change::Ambience(1))
                .changed(Change::UiScale(1));
        }
        assert_eq!((settings.ambience, settings.ui_scale), (100, 140));
        assert!(!settings.can(Change::Ambience(1)) && settings.can(Change::Ambience(-1)));
        for _ in 0..20 {
            settings = settings
                .changed(Change::Ambience(-1))
                .changed(Change::UiScale(-1));
        }
        assert_eq!((settings.ambience, settings.ui_scale), (0, 80));
        assert!(!settings.can(Change::UiScale(-1)));
        for _ in 0..20 {
            settings = settings.changed(Change::PanSpeed(1));
        }
        assert_eq!(settings.pan_speed, 200);
        assert!(!settings.can(Change::PanSpeed(1)) && settings.can(Change::PanSpeed(-1)));
        for _ in 0..20 {
            settings = settings.changed(Change::PanSpeed(-1));
        }
        assert_eq!(settings.pan_speed, 50);
        assert!(settings.can(Change::EdgePan), "a toggle always changes");
        assert_eq!(
            HostSettings::default().camera(),
            IslandCameraSettings::default()
        );

        let audio = HostSettings::default().audio(false);
        assert_eq!(audio, IslandAudioSettings::default());
        assert!(
            HostSettings::default().audio(true).muted,
            "--mute stays muted"
        );
        assert!(
            HostSettings::default()
                .changed(Change::Mute)
                .audio(false)
                .muted
        );
    }

    #[test]
    fn a_change_is_applied_and_saved_and_a_bad_folder_is_reported() {
        let folder = std::env::temp_dir().join(format!("npc-settings-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join(FILE_NAME);
        let mut world = World::new();
        world.insert_resource(SettingsState {
            path: Some(path.clone()),
            ..default()
        });
        change(&mut world, Change::Master(1)).unwrap();
        apply(&mut world);
        assert_eq!(load(&path).master, 70);
        assert_eq!(world.resource::<IslandAudioSettings>().master, 0.7);
        assert_eq!(
            *world.resource::<InterfaceScale>(),
            InterfaceScale::default(),
            "automatic interface size is the default"
        );
        assert_eq!(world.resource::<Theme>().style, PanelStyle::Parchment);
        assert!(world.resource::<IslandCameraSettings>().edge_pan);
        change(&mut world, Change::EdgePan).unwrap();
        change(&mut world, Change::PanSpeed(-1)).unwrap();
        apply(&mut world);
        assert_eq!(
            *world.resource::<IslandCameraSettings>(),
            IslandCameraSettings {
                edge_pan: false,
                pan_speed: 0.75
            },
            "the view's camera reads the same values the file holds"
        );
        let saved = load(&path);
        assert!(!saved.edge_pan && saved.pan_speed == 75);

        // A file where the folder should be: the write fails, the game goes on.
        let blocked = folder.join("not-a-folder");
        std::fs::write(&blocked, "x").unwrap();
        world.resource_mut::<SettingsState>().path = Some(blocked.join(FILE_NAME));
        let error = change(&mut world, Change::Mute).unwrap_err();
        assert!(error.starts_with("Settings were not saved"), "{error}");
        apply(&mut world);
        assert!(world.resource::<IslandAudioSettings>().muted);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_tips_seen_variable_marks_every_tip_seen_without_writing_the_file() {
        assert!(
            tips_seen_all(Some("all")) && tips_seen_all(Some(" 1 ")) && tips_seen_all(Some("TRUE"))
        );
        assert!(!tips_seen_all(None) && !tips_seen_all(Some("0")) && !tips_seen_all(Some("")));
        let folder = std::env::temp_dir().join(format!("npc-settings-tips-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join(FILE_NAME);
        let mut world = World::new();
        world.insert_resource(SettingsState {
            path: Some(path.clone()),
            tips_seen_all: true,
            ..default()
        });
        apply(&mut world);
        let tips = world.resource::<IslandTips>();
        let all = (1_u16 << IslandTips::NAMES.len()) - 1;
        assert_eq!(tips.seen & all, all, "every tip counts as seen");
        assert!(!path.exists(), "the file is not written");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn volumes_reach_the_islands_sound_and_survive_a_restart() {
        let folder =
            std::env::temp_dir().join(format!("npc-settings-audio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join(FILE_NAME);
        let mut world = World::new();
        world.insert_resource(SettingsState {
            path: Some(path.clone()),
            ..default()
        });
        // Each of the five sliders reaches its own channel.
        change(&mut world, Change::SetMaster(90)).unwrap();
        change(&mut world, Change::SetAmbience(30)).unwrap();
        change(&mut world, Change::SetEffects(70)).unwrap();
        change(&mut world, Change::SetMusic(80)).unwrap();
        change(&mut world, Change::InterfaceSounds(-1)).unwrap();
        apply(&mut world);
        let levels = *world.resource::<AudioLevels>();
        assert_eq!(
            [
                levels.master,
                levels.ambience,
                levels.effects,
                levels.music,
                levels.ui
            ],
            [0.9, 0.3, 0.7, 0.8, 0.4]
        );
        // A restart reads the file: the same levels.
        let mut again = World::new();
        again.insert_resource(SettingsState {
            current: load(&path),
            path: Some(path.clone()),
            ..default()
        });
        apply(&mut again);
        assert_eq!(*again.resource::<AudioLevels>(), levels);
        // Mute silences every channel.
        change(&mut again, Change::Mute).unwrap();
        apply(&mut again);
        let muted = *again.resource::<AudioLevels>();
        assert_eq!(
            [
                muted.master,
                muted.ambience,
                muted.effects,
                muted.music,
                muted.ui
            ],
            [0.0; 5]
        );
        // A session launched muted (`--mute`, a screenshot run) stays silent
        // whatever the file says.
        let mut forced = World::new();
        forced.insert_resource(SettingsState {
            current: load(&path).changed(Change::Mute),
            forced_mute: true,
            ..default()
        });
        apply(&mut forced);
        assert_eq!(forced.resource::<AudioLevels>().master, 0.0);
        assert!(!forced.resource::<SettingsState>().current.muted);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn reduce_motion_stills_the_island_and_survives_a_restart() {
        let folder =
            std::env::temp_dir().join(format!("npc-settings-motion-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join(FILE_NAME);
        let mut world = World::new();
        world.insert_resource(SettingsState {
            path: Some(path.clone()),
            ..default()
        });
        apply(&mut world);
        assert_eq!(
            world.resource::<AmbientMotion>().scale,
            1.0,
            "off by default"
        );
        change(&mut world, Change::ReduceMotion).unwrap();
        apply(&mut world);
        assert_eq!(world.resource::<AmbientMotion>().scale, 0.0);
        assert!(world.resource::<ReduceMotion>().0);
        // A restart reads the file: the island stays still.
        let mut again = World::new();
        again.insert_resource(SettingsState {
            current: load(&path),
            path: Some(path.clone()),
            ..default()
        });
        apply(&mut again);
        assert_eq!(again.resource::<AmbientMotion>().scale, 0.0);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
