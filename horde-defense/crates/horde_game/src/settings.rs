use bevy_egui::egui::{self, Context, PointerButton};
use serde::{Deserialize, Serialize};
use std::path::Path;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
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
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            aim_assist: 0.0,
            auto_reload: true,
            toggle_fire: false,
            outlines: false,
            numbers: true,
            reduced_effects: false,
            show_guidance: true,
            blood: false,
            master: 0.6,
            effects: 0.7,
            ambience: 0.15,
            bindings: ["Mouse1", "Mouse2", "R", "1", "2", "B", "Escape", "F"].map(String::from),
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
    pub fn pressed(&self, ctx: &Context, action: Action, held: bool) -> bool {
        let binding = &self.bindings[action as usize];
        ctx.input(|i| match binding.as_str() {
            "Mouse1" => {
                if held {
                    i.pointer.button_down(PointerButton::Primary)
                } else {
                    i.pointer.button_pressed(PointerButton::Primary)
                }
            }
            "Mouse2" => {
                if held {
                    i.pointer.button_down(PointerButton::Secondary)
                } else {
                    i.pointer.button_pressed(PointerButton::Secondary)
                }
            }
            "Mouse3" => {
                if held {
                    i.pointer.button_down(PointerButton::Middle)
                } else {
                    i.pointer.button_pressed(PointerButton::Middle)
                }
            }
            name => egui::Key::from_name(name).is_some_and(|key| {
                if held {
                    i.key_down(key)
                } else {
                    i.events.iter().any(|event|matches!(event,egui::Event::Key{key:event_key,pressed:true,repeat:false,..} if *event_key==key))
                }
            }),
        })
    }
}
pub fn configure(ctx: &Context, scale: f32) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_zoom_factor(scale.clamp(0.75, 1.35));
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
    fonts.families.insert(
        egui::FontFamily::Name("display".into()),
        vec!["pixelify".into()],
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "pixelify".into());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "pixelify".into());
    ctx.set_fonts(fonts);
}
