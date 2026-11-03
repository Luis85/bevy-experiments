//! Settings presentation. Changes persist through the existing save-and-return intent.
use super::*;

pub(super) fn settings_ui(root: &mut egui::Ui, f: &mut Frontend) {
    let ctx = root.ctx().clone();
    if !f.runtime.durable_transition_pending()
        && f.rebind.is_none()
        && !ctx.text_edit_focused()
        && !ctx.any_popup_open()
        && ctx.input(|input| input.key_pressed(egui::Key::Escape))
    {
        f.request_save(SaveIntent::SettingsBack);
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(20))
        .show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                chrome::page_title(ui, "Settings", "");
                ui.add_enabled_ui(!f.runtime.durable_transition_pending(), |ui| {
                    if primary(ui, f.settings_return_label()) {
                        f.request_save(SaveIntent::SettingsBack);
                    }
                    if !matches!(f.return_screen, Screen::Menu | Screen::SurvivorSelection | Screen::Settings)
                        && ui.button("Save and return to main menu").clicked()
                    {
                        f.save_settings_to_menu();
                    }
                });
            });
            muted(ui, format!("Escape: {}. All fields stay paused while you adjust settings.", f.settings_return_label()));
            if f.message_error {
                // Save failures must stay readable without hovering, opening a
                // transient details popup, or leaving this screen.
                ui.colored_label(RED, &f.message);
            } else if !f.message.is_empty() {
                status(ui, f);
            }
            ui.add_enabled_ui(!f.runtime.durable_transition_pending(),|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                gameplay_settings(ui, f);
                if ui.available_width() >= 800.0 {
                    ui.columns(2, |columns| {
                        display_settings(&mut columns[0], &mut f.settings);
                        combat_settings(&mut columns[0], &mut f.settings);
                        audio_settings(&mut columns[0], &mut f.settings);
                        remap_settings(&mut columns[1], f, &ctx);
                    });
                } else {
                    display_settings(ui, &mut f.settings);
                    combat_settings(ui, &mut f.settings);
                    audio_settings(ui, &mut f.settings);
                    remap_settings(ui, f, &ctx);
                }
                chrome::section(ui, "Profile");
                if ui.button("Restore display, audio & control defaults").clicked() {
                    f.settings = Settings::default();
                    f.rebind = None;
                }
                muted(ui, format!("Save location: {}", f.save_path.display()));
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
fn remap_settings(ui: &mut egui::Ui, f: &mut Frontend, ctx: &egui::Context) {
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
                    .button(if f.rebind == Some(action) {
                        "Press key or mouse button…".into()
                    } else {
                        f.settings.bindings[action as usize].clone()
                    })
                    .clicked()
                {
                    f.rebind = Some(action);
                    f.binding_armed = false;
                    clicked = true;
                }
                ui.end_row();
            }
        });
    if let Some(action) = f.rebind
        && !f.runtime.durable_transition_pending()
    {
        if f.binding_armed && !clicked {
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
            if let Some(key) = pressed {
                if let Some(other) = f
                    .settings
                    .bindings
                    .iter()
                    .position(|binding| *binding == key)
                {
                    f.settings.bindings.swap(other, action as usize);
                } else {
                    f.settings.bindings[action as usize] = key;
                }
                f.rebind = None;
            }
        } else if !clicked {
            f.binding_armed = true;
        }
    }
}

fn gameplay_settings(ui: &mut egui::Ui, f: &mut Frontend) {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    chrome::section(ui, "Gameplay · difficulty");
    ui.colored_label(
        AMBER,
        "Applies to the NEXT RUN. Current fields keep their saved settings.",
    );
    let (lanes, width) = f.game.campaign.arena_geometry();
    muted(
        ui,
        format!(
            "Saved next deployment: {} · {lanes} lanes · {width:.0} yd path",
            f.game.campaign.balance_settings.preset.name()
        ),
    );
    if let Some(a) = f.game.attempt.as_ref() {
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
        let committed = &f.game.campaign.balance_settings;
        let draft = f.balance_draft.get_or_insert_with(|| committed.clone());
        ui.horizontal_wrapped(|ui| {
            for preset in BalancePreset::ALL {
                if ui
                    .selectable_label(draft.preset == preset, preset.name())
                    .clicked()
                {
                    if preset == BalancePreset::Custom {
                        draft.preset = preset;
                    } else {
                        *draft = BalanceSettings::for_preset(preset);
                    }
                }
            }
        });
        let mut changed = false;
        for (label, value) in [
            ("Zombie speed", &mut draft.enemy_speed),
            ("Zombie health", &mut draft.enemy_health),
            ("Incoming damage", &mut draft.enemy_damage),
            ("Spawn pressure", &mut draft.spawn_rate),
            ("Coin rewards", &mut draft.reward),
        ] {
            changed |= ui
                .add(egui::Slider::new(value, 0.25..=3.0).suffix("×").text(label))
                .changed();
        }
        egui::CollapsingHeader::new("Arena size & growth").show(ui, |ui| {
            muted(ui, "The actual path and clearing grow with player level. A short opening is a smaller clearing, with the same world units.");
            for (label, value) in [("Opening lanes", &mut draft.start_lanes), ("Maximum lanes", &mut draft.max_lanes)] {
                changed |= ui.add(egui::Slider::new(value, 3..=17).step_by(2.0).text(label)).changed();
            }
            changed |= ui.add(egui::Slider::new(&mut draft.lanes_per_step, 0..=6).step_by(2.0).text("Lanes per growth step")).changed();
            changed |= ui.add(egui::Slider::new(&mut draft.lane_level_step, 1..=100).text("Player levels per lane step")).changed();
            for (label, value) in [("Opening path (yd)", &mut draft.start_width), ("Maximum path (yd)", &mut draft.max_width)] {
                changed |= ui.add(egui::Slider::new(value, 8..=30).text(label)).changed();
            }
            changed |= ui.add(egui::Slider::new(&mut draft.width_per_step, 0..=8).text("Yards per growth step")).changed();
            changed |= ui.add(egui::Slider::new(&mut draft.width_level_step, 1..=100).text("Player levels per path step")).changed();
        });
        egui::CollapsingHeader::new("Advanced combat tuning").show(ui, |ui| {
            for (label, value) in [
                ("Defender damage", &mut draft.player_damage),
                ("Defender health", &mut draft.defender_health),
                ("NPC spawn pressure", &mut draft.npc_spawn_rate),
                ("Boss arrival time", &mut draft.boss_time),
                ("Threat growth per victory", &mut draft.threat_growth),
            ] {
                changed |= ui.add(egui::Slider::new(value, 0.25..=3.0).suffix("×").text(label)).changed();
            }
            changed |= ui.add(egui::Slider::new(&mut draft.supply, -30..=100).text("Starting supply adjustment")).changed();
            muted(ui, "NPC attacks still use the documented injury and turn rules. Threat increases after victories; losses add no threat.");
        });
        if changed {
            draft.preset = BalancePreset::Custom;
        }
        let valid = draft.validate();
        if let Err(error) = &valid {
            ui.colored_label(RED, error);
        }
        let (draft_lanes, draft_width) = draft.geometry(f.game.campaign.player.level);
        muted(
            ui,
            format!(
                "Draft next deployment at your level: {draft_lanes} lanes · {draft_width:.0} yd path"
            ),
        );
        let changed = *draft != *committed;
        let apply = ui
            .add_enabled(
                valid.is_ok() && changed,
                egui::Button::new("Apply to next run"),
            )
            .clicked();
        if changed {
            muted(ui, "Unapplied draft. Apply before saving and returning.");
        }
        apply.then(|| draft.clone())
    };
    if let Some(settings) = apply {
        f.dispatch(
            Command::SetBalance(settings),
            "Difficulty saved for the next deployment. Active fields are unchanged.",
        );
    }
    ui.separator();
}
