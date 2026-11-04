//! A planned deployment is UI intent, never an extra authoritative attempt.
use super::{Screen, Tab, UiCtx, chrome, muted, primary};
use crate::art::{self, AMBER, GREEN, RED};
use bevy_egui::egui::{self, Color32, RichText, Sense, vec2};
use horde_core::content::{Mode, content};
mod view;
use view::BriefingView;

pub(super) fn briefing(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(view) = view::view_model(f.game, f.campaign.mode, f.campaign.stage) else {
        chrome::page_title(
            ui,
            "Choose a front first",
            "Your briefing appears after you select a starting country.",
        );
        return;
    };
    let (country, mode) = (view.country, f.campaign.mode);
    f.campaign.stage = view.stage;
    // One header row: the defense, its picker and the defender. Readiness
    // and the field loadout follow directly, above the encounter detail.
    ui.horizontal_wrapped(|ui| {
        // Labels and pickers share the row's centre line with the title.
        chrome::begin_filter_row(ui);
        ui.label(
            RichText::new(format!("{} · Defense {}", country.name, view.stage + 1))
                .font(egui::FontId::new(28.0, egui::FontFamily::Name("display".into()))),
        )
        .on_hover_text(
            "Review the encounter, choose a defender, then deploy into live combat. Pause whenever you need time.",
        );
        ui.add_space(16.0);
        ui.label("Local defense");
        egui::ComboBox::from_id_salt("local_stage")
            .selected_text(format!("{} of {}", f.campaign.stage + 1, view.levels))
            .show_ui(ui, |ui| {
                for n in 0..=view.max_stage {
                    ui.selectable_value(
                        &mut f.campaign.stage,
                        n,
                        format!("{} · {}", n + 1, content().levels[n % 5].name),
                    );
                }
            });
        ui.add_space(16.0);
        chrome::mode_picker(ui, f);
    });
    ui.add_space(6.0);
    // A stage or defender picked this frame renders immediately.
    let view = if (f.campaign.stage, f.campaign.mode) == (view.stage, mode) {
        view
    } else {
        view::view_model(f.game, f.campaign.mode, f.campaign.stage).expect("active country")
    };
    if ui.available_width() >= 820.0 {
        let width = (ui.available_width() - 28.0) * 0.55;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                loadout(ui, f, &view);
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                readiness(ui, f, &view);
            });
        });
    } else {
        loadout(ui, f, &view);
        ui.add_space(6.0);
        readiness(ui, f, &view);
    }
    ui.add_space(12.0);
    let level = view.level;
    let scene = country.stage_theme(view.stage);
    if ui.available_width() >= 820.0 && ui.clip_rect().height() > 420.0 {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 76.0), Sense::hover());
        art::country_scene_textured(
            &ui.painter_at(rect),
            rect,
            &scene.biome,
            &country.culture,
            view.stage,
            f.textures,
        );
    }
    ui.add_space(12.0);
    ui.label(RichText::new(&level.name).size(22.0).strong());
    let forecast = match &view.forecast {
        Ok(forecast) => forecast,
        Err(error) => {
            ui.colored_label(RED, error);
            return;
        }
    };
    let (pressure, next_pressure) = (&forecast.pressure, &forecast.next);
    ui.label(format!(
        "Boss: {} · arrives after {:.0}s",
        level.boss_name, forecast.boss_seconds
    ));
    ui.label(format!(
        "{} build supply · {:.2}× regional difficulty",
        forecast.supply, country.difficulty
    ));
    ui.label(format!(
        "Threat rank {} · arrival groups {} · population cap {} (+ boss) · new gear Lv {}",
        pressure.rank, pressure.batch_size, pressure.enemy_cap, pressure.item_level
    ));
    ui.label(format!(
        "Next deployment: {} lanes · {:.0} yd path · defender lane {} · +{} character XP on victory",
        view.lanes, view.path_yards, view.defender_lane, view.completion_xp
    ));
    if let Some(level) = view.next_lane_unlock_level {
        muted(
            ui,
            format!(
                "Lane expansion at character level {level}; deployment geometry stays fixed during an attempt."
            ),
        );
    }
    egui::CollapsingHeader::new("Next victory").show(ui, |ui| {
        ui.label(format!("Rank {} → {} · groups {} → {} · new gear Lv {} → {}", pressure.rank, next_pressure.rank, pressure.batch_size, next_pressure.batch_size, pressure.item_level, next_pressure.item_level));
        muted(ui, format!("Threat-only enemy health {:.2}× → {:.2}× · contact damage {:.2}× → {:.2}× · rewards {:.2}× → {:.2}×", pressure.enemy_health, next_pressure.enemy_health, pressure.enemy_damage, next_pressure.enemy_damage, pressure.reward_multiplier, next_pressure.reward_multiplier));
        ui.label(format!("Completion XP forecast: character {} → {} · lanes {} → {} · population cap {} → {}",view.character_level,view.level_after_victory,view.lanes,view.lanes_after_victory,pressure.enemy_cap,next_pressure.enemy_cap));
        muted(ui,if view.max_level {"Character MAX is reached; victories continue raising future threat."} else if f.campaign.mode==Mode::Npc {"NPC kills do not grant personal XP. The known victory completion reward develops the character."} else {"The forecast includes known victory XP; additional personal kill XP may develop the character further before completion."});
        ui.label(format!("Path forecast: {:.0} → {:.0} yd", view.path_yards, view.next_path_yards));
        muted(ui, "Every victory raises future threat, including replays and NPC victories. Defeats and withdrawals do not. Threat keeps growing after character MAX; this deployment snapshots its lane count and pressure.");
    });
    muted(
        ui,
        format!(
            "{} · {} · {}",
            scene.title,
            chrome::display_label(&scene.weather),
            chrome::display_label(&scene.lighting)
        ),
    );
    egui::CollapsingHeader::new("Encounter conditions and field rules").show(ui,|ui|{
        for text in &country.boosts{ui.colored_label(AMBER,text);}
        for text in &country.buffs{ui.colored_label(GREEN,text);}
        for text in &country.debuffs{ui.colored_label(RED,text);}
        ui.label(format!("Enemies at any of the {} contact lanes can attack your survivor. Aim at that lane to strike back. The survivor stays in center lane {}.",view.lanes,view.defender_lane));
        ui.label("Survive until the boss arrives, then defeat it. The timer alone does not clear the defense. Coins and recovered items are banked as you earn them.");
    });
}
/// A briefing column heading; the loadout's carries its one action.
fn heading(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(18.0).strong());
}
fn loadout(ui: &mut egui::Ui, f: &mut UiCtx, view: &BriefingView) {
    chrome::row(ui, |ui| {
        heading(ui, "Field loadout");
        ui.add_space(12.0);
        if ui.button("Change equipment").clicked() {
            f.open_preparation(Tab::Armory);
        }
    });
    // Two slots per row keep the loadout to two lines above the fold.
    let per_row = if ui.available_width() >= 520.0 { 2 } else { 1 };
    egui::Grid::new("briefing_equipment")
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for (index, line) in view.loadout.iter().enumerate() {
                muted(ui, line.label);
                ui.label(format!("{} · Lv {}", line.name, line.level));
                if (index + 1) % per_row == 0 {
                    ui.end_row();
                }
            }
        });
}
fn readiness(ui: &mut egui::Ui, f: &mut UiCtx, view: &BriefingView) {
    heading(ui, "Before you deploy");
    if view.heal_ready {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            crate::visual_assets::prop(
                ui.painter(),
                f.textures,
                crate::visual_assets::Prop::Medical,
                r.center(),
                r.size(),
                Color32::WHITE,
            );
            ui.label("Emergency aid ready");
        });
    }
    if f.campaign.mode == Mode::Personal {
        let points = view.character_points;
        if points > 0 {
            ui.colored_label(AMBER, format!("{points} character points available"));
            if ui.button("Improve your character").clicked() {
                f.open_preparation(Tab::Character);
            }
        } else {
            muted(
                ui,
                "Your equipped starter items are protected. You can enter without making a purchase.",
            );
        }
    } else {
        ui.label(format!(
            "{} · {} unspent training points",
            view.npc_name, view.npc_unspent
        ));
        if ui.button("Review survivor training").clicked() {
            f.open_preparation(Tab::Survivor);
        }
    }
    let owned = view.blueprints;
    if owned > 0 {
        ui.label(format!("{owned} fortification blueprints ready"));
        muted(
            ui,
            "Build defenses while combat runs. Pause explicitly when needed. Supply is separate from coins.",
        );
    } else {
        muted(
            ui,
            f.game
                .campaign()
                .construction_blocker()
                .unwrap_or_else(|| "A starting layout is optional.".into()),
        );
    }
}
/// The camp footer's primary action. A live main defense always offers the
/// one return to it; other Prepare pages lead to the briefing, and only the
/// briefing deploys.
pub(super) fn mission_actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    if f.game.attempt().is_some() {
        if primary(ui, "Return to active defense") {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        }
        return;
    }
    let Some(country) = f.game.campaign().world.active_country() else {
        if primary(ui, "Choose a country") {
            f.open_campaign();
        }
        return;
    };
    if f.tab != Tab::Briefing {
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "Go to briefing") {
                f.open_preparation(Tab::Briefing);
            }
            muted(
                ui,
                format!("{} · {}", country.name, chrome::defender_name(f)),
            );
        });
        return;
    }
    let name = country.name.clone();
    f.campaign.stage = f.campaign.stage.min(
        f.game
            .campaign()
            .world
            .completed(&country.id)
            .min(country.level_count() - 1),
    );
    ui.horizontal_wrapped(|ui| {
        if primary(
            ui,
            if f.campaign.mode == Mode::Personal {
                "Deploy personally"
            } else {
                "Deploy survivor"
            },
        ) {
            f.begin();
        }
        ui.label(format!(
            "{name} · Defense {} · {}",
            f.campaign.stage + 1,
            chrome::defender_name(f)
        ));
    });
}
