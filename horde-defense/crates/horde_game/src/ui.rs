use crate::{
    art::{MUTED, TEXT},
    runner::ControlFrame,
    settings,
};
use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{self, Color32, RichText, Stroke, vec2},
};
use horde_core::{content::Mode, sim::Input};

#[cfg(test)]
mod app_tests;
mod context;
mod notices;
mod plugin;
mod screen;
mod state;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(feature = "review")]
pub(crate) use context::request_save;
pub(crate) use context::{UiCtx, UiParams, ambience_target};
pub use plugin::UiPlugin;
pub use screen::{Screen, ScreenAmbience, Tab};
pub(crate) use state::{
    ArmoryUi, BattleUi, CampaignUi, FrontendParts, MenuUi, Navigation, SettingsUi, StatusUi,
};

fn status(ui: &mut egui::Ui, f: &mut UiCtx) {
    feedback::status_line(ui, f);
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(MUTED));
}
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT).strong())
            .fill(Color32::BLACK)
            .stroke(Stroke::new(2.0, TEXT))
            .min_size(vec2(ui.available_width().min(190.0), 40.0))
            .wrap(),
    )
    .clicked()
}
fn progress(ui: &mut egui::Ui, value: f32, text: String, color: Color32) {
    ui.add(
        egui::ProgressBar::new(value.clamp(0.0, 1.0))
            .corner_radius(0)
            .fill(color.gamma_multiply(0.45))
            .text(text)
            .desired_height(18.0),
    );
}

/// Diagnostic metadata produced by the actual last rendered model review.
/// The screenshot service owns serialization and disk I/O.
#[cfg(feature = "review")]
pub(crate) fn player_model_review_report(ctx: &egui::Context) -> Option<review::NativeModelReport> {
    review::model_report(ctx)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    mut contexts: EguiContexts,
    mut params: UiParams,
    mut sounds: MessageWriter<crate::audio::Cue>,
    time: Res<Time>,
    frames: Res<bevy::diagnostic::FrameCount>,
    render_device: Res<bevy::render::renderer::RenderDevice>,
    mut initialized: Local<bool>,
) -> Result {
    let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Interface);
    let mut cues = Vec::new();
    let mut f = params.ctx(&mut cues);
    f.runtime.begin_frame(frames.0);
    let ctx = contexts.ctx_mut()?;
    if !*initialized {
        let texture_limit = render_device.limits().max_texture_dimension_2d as usize;
        ctx.input_mut(|input| {
            input.max_texture_side = texture_limit;
            input.raw.max_texture_side = Some(texture_limit);
        });
        crate::visual_assets::prepare_player_models(ctx).map_err(std::io::Error::other)?;
        settings::install_font(ctx);
        *initialized = true;
        ctx.request_repaint();
        return Ok(());
    }
    draw_screen(ctx, &mut f, time.elapsed_secs());
    sounds.write_batch(cues);
    ctx.request_repaint();
    Ok(())
}

/// One frame of the current screen plus the control frame it publishes.
fn draw_screen(ctx: &egui::Context, f: &mut UiCtx, now: f32) {
    settings::configure(ctx, f.settings.scale);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.animation_time = if f.settings.reduced_effects {
            0.0
        } else {
            0.12
        }
    });
    let view_accent = feedback::view_progress(ctx, f.screen, f.settings.reduced_effects);
    let mut root_ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("lastlight_root"),
        egui::UiBuilder::new()
            .max_rect(ctx.content_rect())
            .layer_id(egui::LayerId::background()),
    );
    let root = &mut root_ui;
    if f.screen == Screen::Battle {
        controls(ctx, f);
    }
    if f.screen.shows_header() {
        navigation::header(root, f, view_accent);
    }
    navigation::field_status(root, f);
    #[cfg(feature = "review")]
    if review::model_workshop(root, f) {
        publish_controls(ctx, f, false, false);
        return;
    }
    match f.screen {
        Screen::Menu => menu(root, f, now),
        Screen::SurvivorSelection => menu::survivor_selection(root, f, now),
        Screen::Camp => camp(root, f),
        Screen::Battle => battle(root, f),
        Screen::Results => results(root, f),
        Screen::History => history::show(root, f),
        Screen::Settings => settings_ui(root, f),
    }
    // A transition requested this frame ends live control immediately.
    let active =
        f.screen == Screen::Battle && !f.leaving() && f.game.attempt().is_some_and(|a| !a.paused);
    let strategy_active = f.screen == Screen::Camp
        && !f.leaving()
        && f.game.attempt().is_none()
        && !f.game.global_paused();
    publish_controls(ctx, f, active, strategy_active);
}

fn publish_controls(ctx: &egui::Context, f: &mut UiCtx, active: bool, strategy_active: bool) {
    let mut held = if active {
        gather_combat_input(ctx, f)
    } else {
        Input::default()
    };
    let mut pressed = std::mem::take(&mut f.battle.pending);
    let pressed_aims = std::mem::take(&mut f.battle.pending_attack_aims);
    pressed.reload |= held.reload;
    pressed.heal |= held.heal;
    pressed.brace |= held.brace;
    held.reload = false;
    held.heal = false;
    held.brace = false;
    let speed = if f.game.attempt().is_some_and(|a| a.mode == Mode::Personal) {
        1
    } else {
        f.battle.speed
    };
    let observed = f.observed_battle();
    f.runtime.observe_battle(observed);
    f.runtime.publish(ControlFrame {
        enabled: active || strategy_active,
        speed,
        held,
        pressed,
        pressed_aims,
    });
}

mod menu;
use self::menu::menu;
mod briefing;
mod camp;
mod chrome;
mod navigation;
use self::camp::camp;
#[cfg(test)]
use self::camp::country_dossier;
mod character;
use self::character::{character, mode_selector, policy, survivor};
mod armory;
use self::armory::armory;
mod battle;
pub(crate) mod damage;
pub(crate) mod loot;
mod progression;
mod reticle;
use self::battle::{battle, controls, gather_combat_input};
mod results;
use self::results::results;
mod history;
mod preferences;
use self::preferences::settings_ui;
mod operations;
use self::operations::operations;

mod review;
#[cfg(feature = "review")]
pub(crate) use review::schedule_gesture_review;

mod feedback;

mod hud;
