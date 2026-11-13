use bevy_egui::egui::{self, Context, PointerButton};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// The `version` every settings file is written with. A file without the key
/// predates it and reads as 0. Wave 6 raises it to 2 for the mix migration.
pub const SETTINGS_VERSION: u32 = 1;

/// Default preference values. Audio owns the volume values; `WAVE1_*` keep
/// the wave-1 defaults so a later default change can tell an untouched
/// slider from a chosen one.
pub mod defaults {
    // The mix defaults live with the mix table; the offline loudness gate
    // renders at the same constants.
    pub const MASTER: f32 = horde_audio::mix::DEFAULT_MASTER;
    pub const EFFECTS: f32 = horde_audio::mix::DEFAULT_EFFECTS;
    pub const AMBIENCE: f32 = horde_audio::mix::DEFAULT_AMBIENCE;
    pub const MUSIC: f32 = horde_audio::mix::DEFAULT_MUSIC;
    pub const UI: f32 = horde_audio::mix::DEFAULT_UI;
    #[allow(dead_code, reason = "the wave-6 mix migration compares against these")]
    pub const WAVE1_MASTER: f32 = 0.6;
    #[allow(dead_code, reason = "the wave-6 mix migration compares against these")]
    pub const WAVE1_EFFECTS: f32 = 0.7;
    #[allow(dead_code, reason = "the wave-6 mix migration compares against these")]
    pub const WAVE1_AMBIENCE: f32 = 0.15;
    #[allow(dead_code, reason = "the wave-6 mix migration compares against these")]
    pub const WAVE1_MUSIC: f32 = 0.7;
    #[allow(dead_code, reason = "the wave-6 mix migration compares against these")]
    pub const WAVE1_UI: f32 = 0.7;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Fire,
    Melee,
    Reload,
    Heal,
    Brace,
    Build,
    Pause,
    Toggle,
}
impl Action {
    pub const ALL: [Self; 8] = [
        Self::Fire,
        Self::Melee,
        Self::Reload,
        Self::Heal,
        Self::Brace,
        Self::Build,
        Self::Pause,
        Self::Toggle,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Fire => "Fire",
            Self::Melee => "Contact melee",
            Self::Reload => "Reload",
            Self::Heal => "Emergency aid",
            Self::Brace => "Brace",
            Self::Build => "Construction",
            Self::Pause => "Pause",
            Self::Toggle => "Toggle firing",
        }
    }
}

/// Keys for actions outside the eight core bindings. `""` is Unbound and never
/// fires. A new action is a new field, so older files keep their keys.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraBindings {
    pub world_clock: String,
    pub cycle_attack: String,
}
impl Default for ExtraBindings {
    fn default() -> Self {
        Self {
            world_clock: "P".into(),
            cycle_attack: "Q".into(),
        }
    }
}
impl ExtraBindings {
    fn slots_mut(&mut self) -> [&mut String; 2] {
        [&mut self.world_clock, &mut self.cycle_attack]
    }
    pub fn get(&self, action: ExtraAction) -> &str {
        match action {
            ExtraAction::WorldClock => &self.world_clock,
            ExtraAction::CycleAttack => &self.cycle_attack,
        }
    }
    fn get_mut(&mut self, action: ExtraAction) -> &mut String {
        match action {
            ExtraAction::WorldClock => &mut self.world_clock,
            ExtraAction::CycleAttack => &mut self.cycle_attack,
        }
    }
}

/// An action outside the eight core bindings; its key is a field of
/// [`ExtraBindings`]. Variants are append-only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExtraAction {
    /// Pauses and resumes the world clock; Camp home only.
    WorldClock,
    /// Selects the next unlocked attack (Gun, Grenade, Molotov) in a battle.
    CycleAttack,
}
impl ExtraAction {
    pub const ALL: [Self; 2] = [Self::WorldClock, Self::CycleAttack];
    pub fn name(self) -> &'static str {
        match self {
            Self::WorldClock => "World clock (camp home)",
            Self::CycleAttack => "Cycle attack",
        }
    }
}

/// One row of the Controls table: a core action or an extra one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingId {
    Core(Action),
    Extra(ExtraAction),
}
impl BindingId {
    /// Every binding, core rows first: the order of the Controls table.
    pub fn all() -> impl Iterator<Item = Self> {
        Action::ALL
            .into_iter()
            .map(Self::Core)
            .chain(ExtraAction::ALL.into_iter().map(Self::Extra))
    }
}

/// What a binding shows in place of its key when it has none.
pub const UNBOUND: &str = "Unbound";

/// The text a binding shows: its key, or [`UNBOUND`].
pub fn binding_text(binding: &str) -> &str {
    if binding.is_empty() { UNBOUND } else { binding }
}

/// Whether the key or mouse button named `binding` is down (`held`) or was
/// pressed this pass. `""` (Unbound) and an unknown name never fire.
pub fn binding_pressed(ctx: &Context, binding: &str, held: bool) -> bool {
    let button = |i: &egui::InputState, button| {
        if held {
            i.pointer.button_down(button)
        } else {
            i.pointer.button_pressed(button)
        }
    };
    ctx.input(|i| match binding {
        "" => false,
        "Mouse1" => button(i, PointerButton::Primary),
        "Mouse2" => button(i, PointerButton::Secondary),
        "Mouse3" => button(i, PointerButton::Middle),
        name => egui::Key::from_name(name).is_some_and(|key| {
            if held {
                i.key_down(key)
            } else {
                i.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key { key: event_key, pressed: true, repeat: false, .. }
                            if *event_key == key
                    )
                })
            }
        }),
    })
}

/// Variants are append-only: an unknown variant falls back alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum WindowModeSetting {
    #[default]
    Windowed,
    BorderlessFullscreen,
}

/// How item quality is shown besides colour. Variants are append-only.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum QualityMarkers {
    #[default]
    ColorOnly,
    Text,
    Shape,
    TextAndShape,
}
impl QualityMarkers {
    pub const ALL: [Self; 4] = [Self::ColorOnly, Self::Text, Self::Shape, Self::TextAndShape];
    /// The Settings chip for this choice.
    pub fn label(self) -> &'static str {
        match self {
            Self::ColorOnly => "Colour only",
            Self::Text => "Text",
            Self::Shape => "Shape",
            Self::TextAndShape => "Text and shape",
        }
    }
    /// An item label that shows quality by colour alone also names it.
    pub fn text(self) -> bool {
        matches!(self, Self::Text | Self::TextAndShape)
    }
    /// A quality is drawn with its shape beside it.
    pub fn shape(self) -> bool {
        matches!(self, Self::Shape | Self::TextAndShape)
    }
}

/// Variants are append-only.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Palette {
    #[default]
    Standard,
    ColorBlind,
}

/// The user settings resource: display, audio, controls and accessibility.
#[derive(bevy::prelude::Resource, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Field-level default: a file without the key reads as 0, never as the
    /// current version, so the load migrations can tell old files apart.
    #[serde(default)]
    pub version: u32,
    pub scale: f32,
    pub aim_assist: f32,
    pub auto_reload: bool,
    pub toggle_fire: bool,
    pub outlines: bool,
    pub numbers: bool,
    pub reduced_effects: bool,
    pub show_guidance: bool,
    pub blood: bool,
    pub master: f32,
    pub effects: f32,
    pub ambience: f32,
    pub bindings: [String; 8],
    pub extra_bindings: ExtraBindings,
    pub window_mode: WindowModeSetting,
    pub vsync: bool,
    pub low_latency: bool,
    /// "Music & stingers".
    pub music: f32,
    /// "Interface sounds".
    pub ui: f32,
    pub mono_audio: bool,
    pub quality_markers: QualityMarkers,
    pub palette: Palette,
    /// Onboarding callouts already shown; None is a file from before them.
    pub seen_callouts: Option<BTreeSet<String>>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            scale: 1.0,
            aim_assist: 0.0,
            auto_reload: true,
            toggle_fire: false,
            outlines: false,
            numbers: true,
            reduced_effects: false,
            show_guidance: true,
            blood: false,
            master: defaults::MASTER,
            effects: defaults::EFFECTS,
            ambience: defaults::AMBIENCE,
            bindings: ["Mouse1", "Mouse2", "R", "1", "2", "B", "Escape", "F"].map(String::from),
            extra_bindings: ExtraBindings::default(),
            window_mode: WindowModeSetting::Windowed,
            vsync: true,
            low_latency: false,
            music: defaults::MUSIC,
            ui: defaults::UI,
            mono_audio: false,
            quality_markers: QualityMarkers::ColorOnly,
            palette: Palette::Standard,
            seen_callouts: None,
        }
    }
}

/// The pure result of reading settings bytes.
#[derive(Clone, PartialEq, Debug)]
pub struct SettingsDecode {
    pub settings: Settings,
    /// Keys whose values could not be read; each took its default alone.
    pub dropped_keys: Vec<String>,
    /// The bytes were not a JSON object, so every value is a default.
    pub not_json: bool,
}

impl Settings {
    /// Reads settings without preserving an unreadable file. The startup read
    /// is `profile::read_settings`.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .map(|bytes| Self::decode(&bytes).settings)
            .unwrap_or_default()
    }
    /// Reads settings bytes, in order: per-key salvage; before version 1,
    /// Interface sounds take the effects volume they rode on; volumes are
    /// clamped and a non-finite one takes its default; `normalise`; the
    /// version becomes `SETTINGS_VERSION`. Pure: preserving the original is
    /// `profile::read_settings`.
    pub fn decode(bytes: &[u8]) -> SettingsDecode {
        let (mut settings, dropped_keys, not_json) =
            match serde_json::from_slice::<serde_json::Value>(bytes) {
                Ok(serde_json::Value::Object(object)) => {
                    let (settings, dropped_keys) = Self::salvage(object);
                    (settings, dropped_keys, false)
                }
                _ => (Self::default(), Vec::new(), true),
            };
        if settings.version < 1 {
            settings.ui = settings.effects;
        }
        settings.clamp_volumes();
        settings.normalise();
        settings.version = SETTINGS_VERSION;
        SettingsDecode {
            settings,
            dropped_keys,
            not_json,
        }
    }
    /// Keeps every key that still deserializes; each other key takes its
    /// default alone, so one bad value never resets the bindings.
    fn salvage(mut object: serde_json::Map<String, serde_json::Value>) -> (Self, Vec<String>) {
        // serde_json writes a non-finite f32 as null: that reads as the
        // default, not as an unreadable key.
        const FLOATS: [&str; 7] = [
            "scale",
            "aim_assist",
            "master",
            "effects",
            "ambience",
            "music",
            "ui",
        ];
        object.retain(|key, value| !(value.is_null() && FLOATS.contains(&key.as_str())));
        let parse = |object: &serde_json::Map<String, serde_json::Value>| {
            serde_json::from_value::<Self>(serde_json::Value::Object(object.clone()))
        };
        if let Ok(settings) = parse(&object) {
            return (settings, Vec::new());
        }
        let mut kept = serde_json::Map::new();
        let mut dropped = Vec::new();
        for (key, value) in object {
            kept.insert(key.clone(), value);
            if parse(&kept).is_err() {
                kept.remove(&key);
                dropped.push(key);
            }
        }
        // Every kept key parsed alongside the others, so this cannot fail.
        (parse(&kept).unwrap_or_default(), dropped)
    }
    /// The volume sliders as the mix reads them, one per bus.
    pub fn sliders(&self) -> horde_audio::mix::Sliders {
        horde_audio::mix::Sliders {
            master: self.master,
            effects: self.effects,
            ui: self.ui,
            music: self.music,
            ambience: self.ambience,
        }
    }
    fn clamp_volumes(&mut self) {
        for (volume, default) in [
            (&mut self.master, defaults::MASTER),
            (&mut self.effects, defaults::EFFECTS),
            (&mut self.ambience, defaults::AMBIENCE),
            (&mut self.music, defaults::MUSIC),
            (&mut self.ui, defaults::UI),
        ] {
            *volume = if volume.is_finite() {
                volume.clamp(0.0, 1.0)
            } else {
                default
            };
        }
    }
    /// Unbinds any extra key that a core action, or an earlier extra action,
    /// already holds, so one key never drives two actions.
    pub fn normalise(&mut self) {
        let mut taken: Vec<String> = self.bindings.to_vec();
        for slot in self.extra_bindings.slots_mut() {
            if slot.is_empty() {
                continue;
            }
            if taken.contains(slot) {
                slot.clear();
            } else {
                taken.push(slot.clone());
            }
        }
    }
    /// The key of `id`; `""` is Unbound.
    pub fn binding(&self, id: BindingId) -> &str {
        match id {
            BindingId::Core(action) => &self.bindings[action as usize],
            BindingId::Extra(action) => self.extra_bindings.get(action),
        }
    }
    fn binding_mut(&mut self, id: BindingId) -> &mut String {
        match id {
            BindingId::Core(action) => &mut self.bindings[action as usize],
            BindingId::Extra(action) => self.extra_bindings.get_mut(action),
        }
    }
    /// Binds `id` to `key`. A key another action holds, in either table,
    /// swaps over: that action takes `id`'s previous key (Unbound when `id`
    /// had none), so one key never drives two actions.
    pub fn set_binding(&mut self, id: BindingId, key: String) {
        let holder = if key.is_empty() {
            None
        } else {
            BindingId::all().find(|&other| other != id && self.binding(other) == key)
        };
        let previous = std::mem::replace(self.binding_mut(id), key);
        if let Some(holder) = holder {
            *self.binding_mut(holder) = previous;
        }
    }
    /// "Restore defaults": every preference takes its default and the
    /// version the current one. The onboarding seen-set is memory, not a
    /// preference, so it is kept (ruling S4).
    pub fn restore_defaults(&mut self) {
        let seen_callouts = self.seen_callouts.take();
        *self = Self {
            seen_callouts,
            ..Self::default()
        };
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
    pub fn pressed(&self, ctx: &Context, action: Action, held: bool) -> bool {
        binding_pressed(ctx, &self.bindings[action as usize], held)
    }
    /// [`Settings::pressed`] for an extra action. An unbound one never fires.
    pub fn pressed_extra(&self, ctx: &Context, action: ExtraAction, held: bool) -> bool {
        binding_pressed(ctx, self.extra_bindings.get(action), held)
    }
}
/// The egui zoom `configure` applies for an interface `scale`: the
/// preferences slider's range, whatever a settings file or review asks for.
pub fn applied_scale(scale: f32) -> f32 {
    scale.clamp(0.75, 1.35)
}
pub fn configure(ctx: &Context, scale: f32) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_zoom_factor(applied_scale(scale));
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(crate::art::TEXT);
    style.visuals.panel_fill = crate::art::INK;
    style.visuals.window_fill = crate::art::PANEL;
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(13, 20, 21);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(35, 45, 43);
    style.visuals.selection.bg_fill = crate::art::AMBER.gamma_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, crate::art::AMBER);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(43, 54, 49);
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(8, 12, 13);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, crate::art::MUTED);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, crate::art::TEXT);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, crate::art::TEXT);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(67, 77, 58);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, crate::art::AMBER);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(88, 88, 56);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
    style.spacing.item_spacing = egui::vec2(9.0, 7.0);
    style.spacing.button_padding = egui::vec2(11.0, 6.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(24.0, egui::FontFamily::Name("display".into())),
    );
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub fn install_font(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "pixelify".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/PixelifySans.ttf"
        ))),
    );
    fonts.font_data.insert(
        "atkinson".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/AtkinsonHyperlegible-Regular.ttf"
        ))),
    );
    // Every face places glyphs on whole pixels (no subpixel bins, W1-64), so
    // a glyph has one bitmap per size: text whose digits change (damage
    // popups, DPS, coin pop-ups, trap charges, HUD counters) stops adding
    // glyph variants to the font atlas once each size has been seen, and
    // `ui::damage` can warm the battle set once. bevy_egui re-uploads the
    // whole atlas on every change.
    for data in fonts.font_data.values_mut() {
        let mut face = (**data).clone();
        face.tweak.subpixel_binning = Some(false);
        *data = std::sync::Arc::new(face);
    }
    // Pixelify carries the pixel identity on headings and the wordmark only;
    // body text, buttons and labels use a face built for legibility.
    fonts.families.insert(
        egui::FontFamily::Name("display".into()),
        vec!["pixelify".into()],
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "atkinson".into());
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tests/fixtures/settings_v0.json`: a settings file as the pre-v2 build
    /// writes it (`Settings::save`, 13 keys, no version), with custom
    /// bindings and every preference away from its default. Cut by
    /// `write_settings_v0_fixture` when it was added (b82658a), on a build
    /// whose `Settings` still had those 13 keys. The file is frozen: the
    /// writer now refuses to run, since this build cannot cut it.
    const SETTINGS_V0: &str = include_str!("../tests/fixtures/settings_v0.json");

    fn customised() -> Settings {
        Settings {
            scale: 1.15,
            aim_assist: 0.25,
            auto_reload: false,
            toggle_fire: true,
            outlines: true,
            numbers: false,
            reduced_effects: true,
            show_guidance: false,
            blood: true,
            master: 0.8,
            effects: 0.45,
            ambience: 0.3,
            bindings: ["Mouse1", "Mouse3", "T", "Q", "E", "C", "P", "G"].map(String::from),
            ..Settings::default()
        }
    }

    fn decode(json: &str) -> SettingsDecode {
        Settings::decode(json.as_bytes())
    }

    #[test]
    fn absent_version_key_reads_zero_not_current() {
        let absent: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.version, 0);
        assert_eq!(Settings::default().version, SETTINGS_VERSION);
        let written: Settings =
            serde_json::from_slice(&serde_json::to_vec(&Settings::default()).unwrap()).unwrap();
        assert_eq!(written.version, SETTINGS_VERSION);
    }

    #[test]
    fn old_13_key_settings_file_loads_with_new_defaults() {
        let raw: Settings = serde_json::from_str(SETTINGS_V0).unwrap();
        assert_eq!(raw.version, 0);
        let decoded = decode(SETTINGS_V0);
        assert!(decoded.dropped_keys.is_empty() && !decoded.not_json);
        let settings = decoded.settings;
        let old = customised();
        assert_eq!(settings.bindings, old.bindings);
        assert_eq!(
            (settings.scale, settings.aim_assist, settings.master),
            (old.scale, old.aim_assist, old.master)
        );
        assert_eq!((settings.effects, settings.ambience), (0.45, 0.3));
        assert!(!settings.auto_reload && settings.toggle_fire && settings.outlines);
        assert!(!settings.numbers && settings.reduced_effects && !settings.show_guidance);
        assert!(settings.blood);
        // Interface sounds rode the effects bus before version 1.
        assert_eq!(settings.ui, 0.45);
        assert_eq!(settings.version, SETTINGS_VERSION);
        // This file binds Heal to Q and Pause to P, so both default extra
        // keys are taken and stay unbound.
        assert_eq!(settings.extra_bindings.world_clock, "");
        assert_eq!(settings.extra_bindings.cycle_attack, "");
        assert_eq!(settings.window_mode, WindowModeSetting::Windowed);
        assert_eq!(
            (settings.vsync, settings.low_latency, settings.mono_audio),
            (true, false, false)
        );
        assert_eq!(settings.music, defaults::MUSIC);
        assert_eq!(settings.quality_markers, QualityMarkers::ColorOnly);
        assert_eq!(settings.palette, Palette::Standard);
        assert_eq!(settings.seen_callouts, None);
    }

    #[test]
    fn one_bad_key_falls_back_alone() {
        let decoded = decode(
            r#"{"master": 0.3, "effects": "loud", "blood": true,
                "bindings": ["Mouse1", "Mouse3", "T", "Q", "E", "C", "P", "G"]}"#,
        );
        assert_eq!(decoded.dropped_keys, ["effects"]);
        assert!(!decoded.not_json);
        let settings = decoded.settings;
        assert_eq!(settings.master, 0.3);
        assert!(settings.blood);
        assert_eq!(settings.bindings, customised().bindings);
        assert_eq!(settings.effects, defaults::EFFECTS);
    }

    #[test]
    fn not_json_or_not_an_object_takes_every_default() {
        for bytes in [&b"master = 0.3"[..], b"[1, 2]", b"", b"\xff\xfe"] {
            let decoded = Settings::decode(bytes);
            assert!(decoded.not_json, "{bytes:?}");
            assert!(decoded.dropped_keys.is_empty());
            assert_eq!(decoded.settings, Settings::default());
        }
    }

    #[test]
    fn unknown_window_mode_variant_is_salvaged_alone() {
        let decoded = decode(
            r#"{"version": 1, "window_mode": "ExclusiveFullscreen", "palette": "ColorBlind",
                "quality_markers": "Shape", "vsync": false,
                "bindings": ["Mouse1", "Mouse3", "T", "Q", "E", "C", "P", "G"]}"#,
        );
        assert_eq!(decoded.dropped_keys, ["window_mode"]);
        let settings = decoded.settings;
        assert_eq!(settings.window_mode, WindowModeSetting::Windowed);
        assert_eq!(settings.palette, Palette::ColorBlind);
        assert_eq!(settings.quality_markers, QualityMarkers::Shape);
        assert!(!settings.vsync);
        assert_eq!(settings.bindings, customised().bindings);
    }

    #[test]
    fn out_of_range_and_null_volumes_take_clamp_or_default() {
        let decoded = decode(
            r#"{"version": 1, "master": 1.7, "effects": -0.2, "ambience": null,
                "music": null, "ui": 0.4}"#,
        );
        // serde_json writes a non-finite f32 as null: that is a default, not
        // an unreadable key.
        assert!(
            decoded.dropped_keys.is_empty(),
            "{:?}",
            decoded.dropped_keys
        );
        let settings = decoded.settings;
        assert_eq!(settings.master, 1.0);
        assert_eq!(settings.effects, 0.0);
        assert_eq!(settings.ambience, defaults::AMBIENCE);
        assert_eq!(settings.music, defaults::MUSIC);
        assert_eq!(settings.ui, 0.4);
        let mut written = Settings {
            master: f32::NAN,
            ui: f32::INFINITY,
            ..Settings::default()
        };
        let reread = Settings::decode(&serde_json::to_vec(&written).unwrap());
        assert!(reread.dropped_keys.is_empty());
        written.master = defaults::MASTER;
        written.ui = defaults::UI;
        assert_eq!(reread.settings, written);
    }

    #[test]
    fn extra_binding_equal_to_core_binding_is_unbound() {
        let decoded = decode(
            r#"{"version": 1, "extra_bindings": {"world_clock": "B", "cycle_attack": "Q"}}"#,
        );
        assert_eq!(decoded.settings.extra_bindings.world_clock, "");
        assert_eq!(decoded.settings.extra_bindings.cycle_attack, "Q");
        let mut settings = Settings::default();
        settings.extra_bindings.world_clock = "Escape".into();
        settings.normalise();
        assert_eq!(settings.extra_bindings.world_clock, "");
        // Two extra actions never share a key either.
        settings.extra_bindings.world_clock = "K".into();
        settings.extra_bindings.cycle_attack = "K".into();
        settings.normalise();
        assert_eq!(settings.extra_bindings.world_clock, "K");
        assert_eq!(settings.extra_bindings.cycle_attack, "");
    }

    #[test]
    fn core_rebind_to_an_extra_key_swaps_across_the_tables() {
        let mut settings = Settings::default();
        settings.set_binding(BindingId::Core(Action::Reload), "P".into());
        assert_eq!(settings.bindings[Action::Reload as usize], "P");
        assert_eq!(settings.extra_bindings.world_clock, "R");
        settings.set_binding(BindingId::Core(Action::Reload), "B".into());
        assert_eq!(settings.bindings[Action::Reload as usize], "B");
        assert_eq!(settings.bindings[Action::Build as usize], "P");
        assert_eq!(settings.extra_bindings.world_clock, "R");
        settings.set_binding(BindingId::Core(Action::Build), "K".into());
        assert_eq!(settings.bindings[Action::Build as usize], "K");
        assert_eq!(settings.extra_bindings.world_clock, "R");
    }

    #[test]
    fn every_new_field_round_trips_at_a_non_default_value() {
        let fresh = Settings::default();
        let changed = Settings {
            extra_bindings: ExtraBindings {
                world_clock: "K".into(),
                cycle_attack: "".into(),
            },
            window_mode: WindowModeSetting::BorderlessFullscreen,
            vsync: false,
            low_latency: true,
            music: 0.25,
            ui: 0.35,
            mono_audio: true,
            quality_markers: QualityMarkers::TextAndShape,
            palette: Palette::ColorBlind,
            seen_callouts: Some(BTreeSet::from(["feature:companion".to_string()])),
            ..customised()
        };
        assert_ne!(changed.extra_bindings, fresh.extra_bindings);
        assert_ne!(changed.window_mode, fresh.window_mode);
        assert_ne!(changed.vsync, fresh.vsync);
        assert_ne!(changed.low_latency, fresh.low_latency);
        assert_ne!(changed.music, fresh.music);
        assert_ne!(changed.ui, fresh.ui);
        assert_ne!(changed.mono_audio, fresh.mono_audio);
        assert_ne!(changed.quality_markers, fresh.quality_markers);
        assert_ne!(changed.palette, fresh.palette);
        assert_ne!(changed.seen_callouts, fresh.seen_callouts);
        let bytes = serde_json::to_vec_pretty(&changed).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], SETTINGS_VERSION);
        let decoded = Settings::decode(&bytes);
        assert!(decoded.dropped_keys.is_empty() && !decoded.not_json);
        assert_eq!(decoded.settings, changed);
        for markers in [
            QualityMarkers::ColorOnly,
            QualityMarkers::Text,
            QualityMarkers::Shape,
            QualityMarkers::TextAndShape,
        ] {
            let settings = Settings {
                quality_markers: markers,
                ..Settings::default()
            };
            let bytes = serde_json::to_vec(&settings).unwrap();
            assert_eq!(Settings::decode(&bytes).settings, settings);
        }
    }

    #[test]
    fn rebind_conflict_spans_core_and_extra_tables() {
        use BindingId::{Core, Extra};
        let mut settings = Settings::default();
        // An extra action takes a core key: the core action takes its old key.
        settings.set_binding(Extra(ExtraAction::CycleAttack), "R".into());
        assert_eq!(settings.binding(Extra(ExtraAction::CycleAttack)), "R");
        assert_eq!(settings.binding(Core(Action::Reload)), "Q");
        assert_eq!(settings.extra_bindings.cycle_attack, "R");
        assert_eq!(settings.bindings[Action::Reload as usize], "Q");
        // Two extra actions swap with one another.
        settings.set_binding(Extra(ExtraAction::WorldClock), "R".into());
        assert_eq!(settings.extra_bindings.world_clock, "R");
        assert_eq!(settings.extra_bindings.cycle_attack, "P");
        // A core action takes an extra key.
        settings.set_binding(Core(Action::Build), "P".into());
        assert_eq!(settings.bindings[Action::Build as usize], "P");
        assert_eq!(settings.extra_bindings.cycle_attack, "B");
        // A free key displaces nothing.
        let before = settings.clone();
        settings.set_binding(Extra(ExtraAction::WorldClock), "K".into());
        assert_eq!(settings.extra_bindings.world_clock, "K");
        assert_eq!(settings.bindings, before.bindings);
        assert_eq!(settings.extra_bindings.cycle_attack, "B");
        // Rebinding an action to its own key changes nothing.
        let before = settings.clone();
        settings.set_binding(Extra(ExtraAction::WorldClock), "K".into());
        assert_eq!(settings, before);
        // An unbound extra action that takes a held key leaves the other
        // action unbound: the two never share it.
        settings.extra_bindings.world_clock.clear();
        settings.set_binding(Extra(ExtraAction::WorldClock), "B".into());
        assert_eq!(settings.extra_bindings.world_clock, "B");
        assert_eq!(settings.extra_bindings.cycle_attack, "");
        // After any sequence every bound key has exactly one action, so
        // `normalise` has nothing to unbind.
        let mut normalised = settings.clone();
        normalised.normalise();
        assert_eq!(normalised, settings);
        let mut keys: Vec<&str> = BindingId::all()
            .map(|id| settings.binding(id))
            .filter(|key| !key.is_empty())
            .collect();
        let bound = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), bound);
    }

    #[test]
    fn extra_actions_cover_every_extra_binding_slot() {
        let mut bindings = ExtraBindings::default();
        assert_eq!(bindings.slots_mut().len(), ExtraAction::ALL.len());
        assert_eq!(bindings.get(ExtraAction::WorldClock), "P");
        assert_eq!(bindings.get(ExtraAction::CycleAttack), "Q");
        assert_eq!(BindingId::all().count(), 10);
        assert_eq!(binding_text(""), "Unbound");
        assert_eq!(binding_text("Q"), "Q");
    }

    fn key_press(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn binding_pressed_reads_keys_and_never_fires_unbound() {
        let ctx = Context::default();
        let mut fired = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key_press(egui::Key::Q)],
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                let mut settings = Settings::default();
                fired.push(settings.pressed_extra(ctx, ExtraAction::CycleAttack, false));
                fired.push(settings.pressed_extra(ctx, ExtraAction::CycleAttack, true));
                fired.push(settings.pressed_extra(ctx, ExtraAction::WorldClock, false));
                settings.extra_bindings.cycle_attack.clear();
                fired.push(settings.pressed_extra(ctx, ExtraAction::CycleAttack, false));
                fired.push(settings.pressed_extra(ctx, ExtraAction::CycleAttack, true));
                fired.push(binding_pressed(ctx, "", false));
                fired.push(binding_pressed(ctx, "Q", false));
                fired.push(binding_pressed(ctx, "NoSuchKey", false));
                settings.bindings[Action::Heal as usize] = "Q".into();
                fired.push(settings.pressed(ctx, Action::Heal, false));
            },
        );
        // A headless pass has no texture uploader.
        output.textures_delta.clear();
        assert_eq!(
            fired,
            [true, true, false, false, false, false, true, false, true]
        );
    }

    #[test]
    fn bindings_stay_eight_core_actions() {
        assert_eq!(Action::ALL.len(), 8);
        let bindings: [String; 8] = Settings::default().bindings;
        assert_eq!(bindings.len(), Action::ALL.len());
    }

    #[test]
    fn settings_v0_fixture_is_a_13_key_file_with_custom_bindings() {
        let value: serde_json::Value = serde_json::from_str(SETTINGS_V0).unwrap();
        let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys.len(), 13, "{keys:?}");
        assert!(value.get("version").is_none());
        assert_eq!(value["effects"], 0.45);
        let settings: Settings = serde_json::from_str(SETTINGS_V0).unwrap();
        assert_eq!(settings.bindings, customised().bindings);
        assert_eq!(settings.effects, 0.45);
        assert_ne!(settings.bindings, Settings::default().bindings);
    }

    #[test]
    #[ignore = "writes tests/fixtures/settings_v0.json; set LASTLIGHT_WRITE_SETTINGS_FIXTURE"]
    fn write_settings_v0_fixture() {
        let Some(path) = std::env::var_os("LASTLIGHT_WRITE_SETTINGS_FIXTURE") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        assert!(!path.exists(), "refusing to overwrite {}", path.display());
        let keys = serde_json::to_value(Settings::default()).unwrap();
        let keys = keys.as_object().unwrap();
        assert!(
            keys.len() == 13 && !keys.contains_key("version"),
            "this writer cuts the pre-version-1, 13-key file; settings_v0.json is frozen \
             (this build writes {} keys)",
            keys.len()
        );
        customised().save(&path).unwrap();
    }
}
