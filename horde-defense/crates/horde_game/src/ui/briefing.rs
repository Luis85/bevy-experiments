//! A planned deployment is UI intent, never an extra authoritative attempt.
use super::{Screen, Tab, UiCtx, chrome, muted, primary};
use crate::art::{self, AMBER, GREEN, RED};
use bevy_egui::egui::{self, Color32, RichText, Sense, vec2};
use horde_core::content::{Mode, Slot, content};

pub(super) fn briefing(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(country) = f.game.campaign.world.active_country() else {
        chrome::page_title(
            ui,
            "Choose a front first",
            "Your briefing appears after you select a starting country.",
        );
        return;
    };
    let id = country.id.clone();
    let name = country.name.clone();
    let levels = country.level_count();
    let completed = f.game.campaign.world.completed(&id);
    f.campaign.stage = f.campaign.stage.min(completed.min(levels - 1));
    chrome::page_title(
        ui,
        &format!("{} · Defense {}", name, f.campaign.stage + 1),
        "Review the encounter, choose a defender, then deploy into live combat. Pause whenever you need time.",
    );
    ui.horizontal_wrapped(|ui| {
        ui.label("Local defense");
        egui::ComboBox::from_id_salt("local_stage")
            .selected_text(format!("{} of {}", f.campaign.stage + 1, levels))
            .show_ui(ui, |ui| {
                for n in 0..=completed.min(levels - 1) {
                    ui.selectable_value(
                        &mut f.campaign.stage,
                        n,
                        format!("{} · {}", n + 1, content().levels[n % 5].name),
                    );
                }
            });
        chrome::mode_picker(ui, f);
    });
    let level = &content().levels[f.campaign.stage % 5];
    let scene = country.stage_theme(f.campaign.stage);
    if ui.available_width() >= 820.0 && ui.available_height() > 420.0 {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 76.0), Sense::hover());
        art::country_scene_textured(
            &ui.painter_at(rect),
            rect,
            &scene.biome,
            &country.culture,
            f.campaign.stage,
            f.textures,
        );
    }
    ui.add_space(12.0);
    ui.label(RichText::new(&level.name).size(22.0).strong());
    let pressure = match f.game.campaign.deployment_progression(f.campaign.mode) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            ui.colored_label(RED, error);
            return;
        }
    };
    ui.label(format!(
        "Boss: {} · arrives after {:.0}s",
        level.boss_name,
        level.boss_time.max(pressure.boss_time_floor) * f.game.campaign.balance_settings.boss_time
    ));
    let next_pressure = f
        .game
        .campaign
        .next_victory_progression(f.campaign.mode)
        .unwrap_or_else(|_| pressure.clone());
    let supply = (level.supply as i64
        + i64::from(country.modifiers.supply)
        + i64::from(pressure.supply_bonus)
        + i64::from(f.game.campaign.balance_settings.supply))
    .max(0);
    ui.label(format!(
        "{} build supply · {:.2}× regional difficulty",
        supply, country.difficulty
    ));
    ui.label(format!(
        "Threat rank {} · arrival groups {} · population cap {} (+ boss) · new gear Lv {}",
        pressure.rank, pressure.batch_size, pressure.enemy_cap, pressure.item_level
    ));
    ui.label(format!(
        "Next deployment: {} lanes · {:.0} yd path · defender lane {} · +{} character XP on victory",
        f.game.campaign.lane_count(),
        f.game.campaign.arena_geometry().1,
        f.game.campaign.lane_count() / 2 + 1,
        f.game.campaign.completion_xp_preview()
    ));
    if let Some(level) = f.game.campaign.next_lane_unlock_level() {
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
        ui.label(format!("Completion XP forecast: character {} → {} · lanes {} → {} · population cap {} → {}",f.game.campaign.player.level,f.game.campaign.level_after_victory(),f.game.campaign.lane_count(),f.game.campaign.lane_count_after_victory(),pressure.enemy_cap,next_pressure.enemy_cap));
        muted(ui,if f.game.campaign.player.is_max_level() {"Character MAX is reached; victories continue raising future threat."} else if f.campaign.mode==Mode::Npc {"NPC kills do not grant personal XP. The known victory completion reward develops the character."} else {"The forecast includes known victory XP; additional personal kill XP may develop the character further before completion."});
        ui.label(format!("Path forecast: {:.0} → {:.0} yd", f.game.campaign.arena_geometry().1, f.game.campaign.balance_settings.geometry(f.game.campaign.level_after_victory()).1));
        muted(ui, "Every victory raises future threat, including replays and NPC victories. Defeats and withdrawals do not. Threat keeps growing after character MAX; this deployment snapshots its lane count and pressure.");
    });
    muted(
        ui,
        format!("{} · {} · {}", scene.title, scene.weather, scene.lighting),
    );
    ui.add_space(8.0);
    let wide = ui.available_width() >= 820.0;
    if wide {
        let width = (ui.available_width() - 28.0) * 0.55;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                loadout(ui, f);
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                readiness(ui, f);
            });
        });
    } else {
        loadout(ui, f);
        readiness(ui, f);
    }
    egui::CollapsingHeader::new("Encounter conditions and field rules").show(ui,|ui|{
        for text in &country.boosts{ui.colored_label(AMBER,text);}
        for text in &country.buffs{ui.colored_label(GREEN,text);}
        for text in &country.debuffs{ui.colored_label(RED,text);}
        ui.label(format!("Enemies at any of the {} contact lanes can attack your survivor. Aim at that lane to strike back. The survivor stays in center lane {}.",f.game.campaign.lane_count(),f.game.campaign.lane_count()/2+1));
        ui.label("Survive until the boss arrives, then defeat it. The timer alone does not clear the defense. Coins and recovered items are banked as you earn them.");
    });
}
fn loadout(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Field loadout");
    egui::Grid::new("briefing_equipment")
        .spacing([20.0, 7.0])
        .show(ui, |ui| {
            for (slot, label) in [
                (Slot::Ranged, "Ranged"),
                (Slot::Melee, "Melee"),
                (Slot::Outfit, "Outfit"),
                (Slot::Accessory, "Accessory"),
            ] {
                muted(ui, label);
                let loadout = if f.campaign.mode == Mode::Personal {
                    f.game.campaign.player.loadout
                } else {
                    f.game.campaign.npc.loadout
                };
                let item = f
                    .game
                    .campaign
                    .item(loadout[slot.index()])
                    .expect("equipped item exists");
                ui.label(format!(
                    "{} · Lv {}",
                    f.game.campaign.equipped(f.campaign.mode, slot).name,
                    item.level
                ));
                ui.end_row();
            }
        });
    if ui.button("Review equipment").clicked() {
        f.open_preparation(Tab::Armory);
    }
}
fn readiness(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Before you deploy");
    if f.game.campaign.profile(f.campaign.mode).heal {
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
        let points = f.game.campaign.player.points;
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
            f.game.campaign.npc.name, f.game.campaign.npc.unspent
        ));
        if ui.button("Review survivor training").clicked() {
            f.open_preparation(Tab::Survivor);
        }
    }
    let owned = f.game.campaign.blueprints.iter().filter(|v| **v).count();
    if owned > 0 {
        ui.label(format!("{owned} fortification blueprints ready"));
        muted(
            ui,
            "Build defenses while combat runs. Pause explicitly when needed. Supply is separate from coins.",
        );
    } else {
        muted(
            ui,
            "Fortifications unlock through encounter clears. A starting layout is optional.",
        );
    }
}
pub(super) fn mission_actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    if f.tab != Tab::Briefing && f.game.attempt.is_none() && f.game.campaign.world.start.is_some() {
        if primary(
            ui,
            if f.game.campaign.journey_active {
                "Back to journey upgrades"
            } else {
                "Back to world map"
            },
        ) {
            if f.game.campaign.journey_active {
                f.open_upgrades();
            } else {
                f.open_campaign();
            }
        }
        return;
    }
    if f.game.attempt.is_some() {
        if primary(ui, "Return to active defense") {
            f.go(Screen::Battle);
        }
        return;
    }
    let Some(country) = f.game.campaign.world.active_country() else {
        if primary(ui, "Choose a country") {
            f.go_tab(Tab::World);
        }
        return;
    };
    let name = country.name.clone();
    f.campaign.stage = f.campaign.stage.min(
        f.game
            .campaign
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
        if f.tab != Tab::Briefing && ui.small_button("Back to briefing").clicked() {
            f.open_preparation(Tab::Briefing);
        }
    });
}
