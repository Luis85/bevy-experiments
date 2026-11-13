//! Settings presentation. Changes persist through the existing save-and-return intent.
use super::{
    Screen, UiCtx, chrome,
    modal::{AppModal, FIX_DRAFT_FIRST, LeaveSettings},
    muted, primary, status,
};
use crate::{
    art::{AMBER, INK, RED},
    runner::SaveIntent,
    settings::{Action, Settings},
};
use bevy_egui::egui;
use horde_core::{campaign::Campaign, command::Command};

pub(super) fn settings_ui(root: &mut egui::Ui, f: &mut UiCtx) {
    // Escape saves and returns through the router (ui/escape.rs). Every
    // leave route meets the draft warning when the difficulty draft is
    // unapplied (`leave_settings`).
    let ctx = root.ctx().clone();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(20))
        .show(root, |ui| {
            chrome::row(ui, |ui| {
                chrome::page_title(ui, "Settings", "");
                ui.add_enabled_ui(!f.runtime.durable_transition_pending(), |ui| {
                    if primary(ui, f.settings_return_label()).clicked() {
                        f.leave_settings(LeaveSettings::Back);
                    }
                    if !matches!(f.nav.return_screen, Screen::Menu | Screen::SurvivorSelection | Screen::Settings)
                        && ui.button("Save and return to main menu").clicked()
                    {
                        f.leave_settings(LeaveSettings::Menu);
                    }
                });
            });
            muted(ui, format!("Escape: {}. All fields stay paused while you adjust settings.", f.settings_return_label()));
            if f.status.message_error {
                // Save failures must stay readable without hovering, opening a
                // transient details popup, or leaving this screen.
                ui.colored_label(RED, &f.status.message);
            } else if !f.status.message.is_empty() {
                status(ui, f);
            }
            ui.add_enabled_ui(!f.runtime.durable_transition_pending(),|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                gameplay_settings(ui, f);
                if ui.available_width() >= 800.0 {
                    ui.columns(2, |columns| {
                        display_settings(&mut columns[0], f.settings);
                        combat_settings(&mut columns[0], f.settings);
                        audio_settings(&mut columns[0], f.settings);
                        remap_settings(&mut columns[1], f, &ctx);
                    });
                } else {
                    display_settings(ui, f.settings);
                    combat_settings(ui, f.settings);
                    audio_settings(ui, f.settings);
                    remap_settings(ui, f, &ctx);
                }
                chrome::section(ui, "Profile");
                if ui.button("Restore display, audio & control defaults").clicked() {
                    *f.settings = Settings::default();
                    f.prefs.rebind = None;
                }
                muted(ui, format!("Save location: {}", f.paths.campaign().display()));
                muted(ui, "Every important sound has a visual cue. The camera is fixed; aiming never shakes.");
            });});
        });
}
fn display_settings(ui: &mut egui::Ui, settings: &mut Settings) {
    chrome::section(ui, "Readability & effects");
    ui.add(egui::Slider::new(&mut settings.scale, 0.75..=1.35).text("Interface scale"));
    ui.checkbox(&mut settings.outlines, "Outline enemy silhouettes");
    ui.checkbox(&mut settings.numbers, "Show damage numbers");
    ui.checkbox(&mut settings.reduced_effects, "Reduce flashes and movement");
    ui.checkbox(
        &mut settings.show_guidance,
        "Show contextual field guidance",
    );
    ui.checkbox(&mut settings.blood, "Blood effects");
}
fn combat_settings(ui: &mut egui::Ui, settings: &mut Settings) {
    chrome::section(ui, "Aiming & firing");
    ui.add(egui::Slider::new(&mut settings.aim_assist, 0.0..=0.75).text("Aim assistance (yards)"));
    ui.checkbox(
        &mut settings.auto_reload,
        "Automatically reload an empty magazine",
    );
    ui.checkbox(&mut settings.toggle_fire, "Primary click toggles fire");
    muted(
        ui,
        "Melee interrupts an unfinished reload. Toggle firing is also available on the field.",
    );
}
fn audio_settings(ui: &mut egui::Ui, settings: &mut Settings) {
    chrome::section(ui, "Audio");
    ui.add(egui::Slider::new(&mut settings.master, 0.0..=1.0).text("Master"));
    ui.add(egui::Slider::new(&mut settings.effects, 0.0..=1.0).text("Combat effects"));
    ui.add(egui::Slider::new(&mut settings.ambience, 0.0..=1.0).text("Atmosphere"));
}
fn remap_settings(ui: &mut egui::Ui, f: &mut UiCtx, ctx: &egui::Context) {
    chrome::section(ui, "Controls");
    muted(
        ui,
        "Choose an action, then press a key or mouse button. Duplicate bindings swap.",
    );
    let mut clicked = false;
    egui::Grid::new("bindings")
        .num_columns(2)
        .spacing([24.0, 6.0])
        .show(ui, |ui| {
            for action in Action::ALL {
                ui.label(action.name());
                if ui
                    .button(if f.prefs.rebind == Some(action) {
                        "Press key or mouse button…".into()
                    } else {
                        f.settings.bindings[action as usize].clone()
                    })
                    .clicked()
                {
                    f.prefs.rebind = Some(action);
                    f.prefs.binding_armed = false;
                    clicked = true;
                }
                ui.end_row();
            }
        });
    if let Some(action) = f.prefs.rebind
        && !f.runtime.durable_transition_pending()
    {
        if f.prefs.binding_armed && !clicked {
            let pressed = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => Some(key.name().to_string()),
                    egui::Event::PointerButton {
                        button,
                        pressed: true,
                        ..
                    } => Some(
                        match button {
                            egui::PointerButton::Primary => "Mouse1",
                            egui::PointerButton::Secondary => "Mouse2",
                            _ => "Mouse3",
                        }
                        .into(),
                    ),
                    _ => None,
                })
            });
            if pressed.as_deref() == Some("Escape") {
                // Escape cancels the rebind; it is never captured as a binding
                // from this prompt.
                f.prefs.rebind = None;
            } else if let Some(key) = pressed {
                f.settings.bind(action, key);
                f.prefs.rebind = None;
            }
        } else if !clicked {
            f.prefs.binding_armed = true;
        }
    }
}

fn gameplay_settings(ui: &mut egui::Ui, f: &mut UiCtx) {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    chrome::section(ui, "Gameplay · difficulty");
    ui.colored_label(
        AMBER,
        "Applies to the NEXT RUN. Current fields keep their saved settings.",
    );
    let (lanes, width) = f.game.campaign().arena_geometry();
    muted(
        ui,
        format!(
            "Saved next deployment: {} · {lanes} lanes · {width:.0} yd path",
            f.game.campaign().balance_settings.preset.name()
        ),
    );
    if let Some(a) = f.game.attempt() {
        muted(
            ui,
            format!(
                "Active field stays {} · {} lanes · {:.0} yd",
                a.balance.preset.name(),
                a.lane_count,
                a.width_yards
            ),
        );
    }
    let apply = {
        let campaign = f.game.campaign();
        let committed = &campaign.balance_settings;
        let prefs = &mut *f.prefs;
        let custom_chosen = &mut prefs.balance_custom_chosen;
        let draft = prefs.balance_draft.get_or_insert_with(|| committed.clone());
        chrome::row(ui, |ui| {
            // An edited named draft keeps its preset, so the core keeps that
            // preset's extras when it normalises the edit to Custom; it is
            // shown as Custom. Choosing Custom sets the highlight and switches
            // the preview and Apply to the core's Custom steps (R21), and
            // never overwrites the named preset the draft came from.
            let shown = if !*custom_chosen && *draft == BalanceSettings::for_preset(draft.preset) {
                draft.preset
            } else {
                BalancePreset::Custom
            };
            for preset in BalancePreset::ALL {
                if ui
                    .selectable_label(shown == preset, preset.name())
                    .clicked()
                    && shown != preset
                {
                    *custom_chosen = preset == BalancePreset::Custom;
                    if preset != BalancePreset::Custom {
                        *draft = BalanceSettings::for_preset(preset);
                    }
                }
            }
        });
        for (label, value) in [
            ("Zombie speed", &mut draft.enemy_speed),
            ("Zombie health", &mut draft.enemy_health),
            ("Incoming damage", &mut draft.enemy_damage),
            ("Spawn pressure", &mut draft.spawn_rate),
            ("Coin rewards", &mut draft.reward),
        ] {
            ui.add(egui::Slider::new(value, 0.25..=3.0).suffix("×").text(label));
        }
        egui::CollapsingHeader::new("Arena size & growth").show(ui, |ui| {
            muted(ui, "The actual path and clearing grow with player level. A short opening is a smaller clearing, with the same world units.");
            for (label, value) in [("Opening lanes", &mut draft.start_lanes), ("Maximum lanes", &mut draft.max_lanes)] {
                ui.add(egui::Slider::new(value, 3..=17).step_by(2.0).text(label));
            }
            ui.add(egui::Slider::new(&mut draft.lanes_per_step, 0..=6).step_by(2.0).text("Lanes per growth step"));
            ui.add(egui::Slider::new(&mut draft.lane_level_step, 1..=100).text("Player levels per lane step"));
            for (label, value) in [("Opening path (yd)", &mut draft.start_width), ("Maximum path (yd)", &mut draft.max_width)] {
                ui.add(egui::Slider::new(value, 8..=30).text(label));
            }
            ui.add(egui::Slider::new(&mut draft.width_per_step, 0..=8).text("Yards per growth step"));
            ui.add(egui::Slider::new(&mut draft.width_level_step, 1..=100).text("Player levels per path step"));
        });
        egui::CollapsingHeader::new("Advanced combat tuning").show(ui, |ui| {
            for (label, value) in [
                ("Defender damage", &mut draft.player_damage),
                ("Defender health", &mut draft.defender_health),
                ("NPC spawn pressure", &mut draft.npc_spawn_rate),
                ("Boss arrival time", &mut draft.boss_time),
                ("Threat growth per victory", &mut draft.threat_growth),
            ] {
                ui.add(egui::Slider::new(value, 0.25..=3.0).suffix("×").text(label));
            }
            ui.add(egui::Slider::new(&mut draft.supply, -30..=100).text("Starting supply adjustment"));
            muted(ui, "NPC attacks still use the documented injury and turn rules. Threat increases after victories; losses add no threat.");
        });
        let valid = draft.validate();
        if let Err(error) = &valid {
            ui.colored_label(RED, error);
        }
        let (draft_lanes, draft_width) = draft.geometry(f.game.campaign().player.level);
        muted(
            ui,
            format!(
                "Draft next deployment at your level: {draft_lanes} lanes · {draft_width:.0} yd path"
            ),
        );
        let state = draft_state(campaign, draft, *custom_chosen);
        let changed = state != DraftState::Clean;
        let apply = ui
            .add_enabled(
                valid.is_ok() && changed,
                egui::Button::new("Apply to next run"),
            )
            .on_disabled_hover_text(if valid.is_err() {
                FIX_DRAFT_FIRST
            } else {
                "Nothing to apply: the draft matches the next run."
            })
            .clicked();
        if changed {
            muted(ui, UNAPPLIED_DRAFT);
        }
        apply.then_some(state)
    };
    if let Some(state) = apply {
        f.apply_balance_draft(state);
    }
    ui.separator();
}

/// The note under an unapplied difficulty draft. Leaving with one opens the
/// draft warning.
pub(super) const UNAPPLIED_DRAFT: &str = "Unapplied draft. Apply before saving and returning.";

/// How the difficulty draft stands against what the next run uses.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum DraftState {
    /// Applying it would change nothing.
    Clean,
    /// Applying it would change the saved settings or the extras basis:
    /// the `SetBalance` settings to send, in order.
    Pending(Vec<horde_core::balance::BalanceSettings>),
    /// It fails validation, so it cannot be applied.
    Invalid,
}

/// The state of `draft`. It is pending when applying would change the saved
/// settings or the extras basis (the same values edited from another
/// preset). With the Custom chip chosen, an unedited named draft applies as
/// Custom on that preset's basis (R21), in the core's steps.
pub(super) fn draft_state(
    campaign: &Campaign,
    draft: &horde_core::balance::BalanceSettings,
    custom_chosen: bool,
) -> DraftState {
    let outcome = if custom_chosen {
        campaign.custom_balance_outcome(draft.clone())
    } else {
        campaign.balance_outcome(draft.clone())
    };
    match outcome {
        Err(_) => DraftState::Invalid,
        Ok(outcome)
            if outcome
                == (
                    campaign.balance_settings.clone(),
                    campaign.balance_extras_basis,
                ) =>
        {
            DraftState::Clean
        }
        Ok(_) if custom_chosen => DraftState::Pending(Campaign::custom_balance_steps(draft)),
        Ok(_) => DraftState::Pending(vec![draft.clone()]),
    }
}

impl UiCtx<'_> {
    /// The state of the difficulty draft; clean before Settings first opens.
    pub(super) fn balance_draft_state(&self) -> DraftState {
        self.prefs
            .balance_draft
            .as_ref()
            .map_or(DraftState::Clean, |draft| {
                draft_state(
                    self.game.campaign(),
                    draft,
                    self.prefs.balance_custom_chosen,
                )
            })
    }
    /// Queues the `SetBalance` steps of a pending draft, in order. Only the
    /// last step reports success.
    pub(super) fn apply_balance_draft(&mut self, state: DraftState) {
        let DraftState::Pending(steps) = state else {
            return;
        };
        let last = steps.len().saturating_sub(1);
        for (step, settings) in steps.into_iter().enumerate() {
            self.dispatch(
                Command::SetBalance(settings),
                if step == last {
                    "Difficulty saved for the next deployment. Active fields are unchanged."
                } else {
                    ""
                },
            );
        }
    }
    /// Drops the difficulty draft: it shows the next run's settings again.
    pub(super) fn discard_balance_draft(&mut self) {
        self.prefs.balance_draft = Some(self.game.campaign().balance_settings.clone());
        self.prefs.balance_custom_chosen = false;
    }
    /// A Settings leave route (the primary, "Save and return to main menu",
    /// Escape): an unapplied difficulty draft opens the draft warning
    /// (UX-09) instead of being dropped silently; otherwise it leaves.
    pub(super) fn leave_settings(&mut self, then: LeaveSettings) {
        if self.balance_draft_state() == DraftState::Clean {
            self.finish_leaving_settings(then);
        } else {
            // A waiting rebind prompt would capture the modal's click.
            self.prefs.rebind = None;
            self.status.modal = Some(AppModal::DraftWarning { then });
        }
    }
    /// Saves and leaves Settings by `then`, whatever the draft holds.
    pub(super) fn finish_leaving_settings(&mut self, then: LeaveSettings) {
        match then {
            LeaveSettings::Back => self.request_save(SaveIntent::SettingsBack),
            LeaveSettings::Menu => self.save_settings_to_menu(),
        }
    }
}
