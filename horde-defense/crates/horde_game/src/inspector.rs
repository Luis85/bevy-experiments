//! Read-only inspection of authoritative battlefield state. No rolls or simulation
//! mutations are performed when an object is hovered or inspected.
use crate::art::{AMBER, GREEN, MUTED, RED};
use bevy_egui::egui::{self, Response, RichText};
use horde_core::{
    content::{Mode, StructureKind, content},
    sim::{Attempt, Enemy, EnemyPhase, Structure},
};

pub enum Target<'a> {
    Enemy(&'a Enemy),
    Structure(&'a Structure),
}
pub fn target_at(attempt: &Attempt, aim: [f32; 2]) -> Option<Target<'_>> {
    if aim.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let enemy = attempt
        .enemies
        .iter()
        .filter_map(|enemy| {
            let dx = (enemy.x - aim[0]) / if enemy.boss { 0.8 } else { 0.55 };
            let dy = (enemy.y() - aim[1]) / 0.43;
            let distance = dx * dx + dy * dy;
            (distance <= 1.0).then_some((enemy, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)));
    if let Some((enemy, _)) = enemy {
        return Some(Target::Enemy(enemy));
    }
    attempt
        .structures
        .iter()
        .filter(|s| (s.x() - aim[0]).abs() <= 1.1 && (s.y() - aim[1]).abs() <= 0.46)
        .min_by_key(|s| s.id)
        .map(Target::Structure)
}
pub fn hover(response: &Response, attempt: &Attempt, aim: [f32; 2]) {
    if target_at(attempt, aim).is_some() {
        response.clone().on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            dossier(ui, attempt, aim);
        });
    }
}
pub fn dossier(ui: &mut egui::Ui, attempt: &Attempt, aim: [f32; 2]) {
    let Some(target) = target_at(attempt, aim) else {
        ui.colored_label(MUTED, "Hover a hostile or a defense to inspect it.");
        return;
    };
    match target {
        Target::Enemy(enemy) => {
            let definition = content().enemy(enemy.kind);
            let name = if enemy.boss {
                content().levels[attempt.level].boss_name.as_str()
            } else {
                enemy
                    .resolved
                    .as_ref()
                    .map_or(definition.name.as_str(), |profile| profile.name.as_str())
            };
            ui.strong(format!("{name} · lane {}", enemy.lane + 1));
            ui.label(format!(
                "HP {} / {} · protection {}",
                if attempt.mode == Mode::Npc {
                    enemy.injury.hp
                } else {
                    enemy.hp
                },
                enemy.max_hp,
                attempt.enemy_protection(enemy)
            ));
            if enemy.critically_wounded(attempt.mode) {
                ui.colored_label(AMBER, "CRITICAL · still fighting");
                ui.label("Conscious at zero or negative HP. NPC-mode HT checks and injury outcomes determine defeat; zero HP alone is not death.");
            }
            if enemy.boss {
                ui.colored_label(
                    if enemy.exposed_at(attempt.tick) {
                        GREEN
                    } else {
                        AMBER
                    },
                    if enemy.exposed_at(attempt.tick) {
                        "EXPOSED · protection lowered"
                    } else {
                        "ARMORED · watch for the exposure cue"
                    },
                );
            }
            let state = match enemy.attack_phase {
                EnemyPhase::Advance => "Approaching",
                EnemyPhase::Queued => "Queued behind a blocker",
                EnemyPhase::Windup => "Winding up an attack",
                EnemyPhase::Recovery => "Recovering after an attack",
            };
            ui.colored_label(
                if enemy.attack_phase == EnemyPhase::Windup {
                    RED
                } else {
                    MUTED
                },
                state,
            );
            if enemy.attack_phase == EnemyPhase::Windup {
                ui.add(
                    egui::ProgressBar::new(enemy.windup_progress())
                        .text(format!("Strike in {:.1}s", enemy.attack_left as f32 / 60.0)),
                );
            }
            if enemy.contact() {
                ui.colored_label(RED, "CLOSE ENGAGEMENT · can attack the defender");
                ui.label("Targeted melee answers this contact lane. Effective engagement distance: 1 yard.");
            } else if let Some(id) = enemy.target_structure {
                ui.label(format!("Blocked by defense #{id}"));
            } else {
                ui.label(format!(
                    "{:.1} yards from the shared line",
                    (enemy.x - 1.5).max(0.0)
                ));
            }
            if attempt.mode == Mode::Npc {
                let state = &enemy.injury;
                let conditions = [
                    (state.stunned, "stunned"),
                    (state.prone, "prone"),
                    (state.unconscious, "unconscious"),
                    (state.mortally_wounded, "mortal wound"),
                    (state.dead, "dead"),
                ]
                .into_iter()
                .filter_map(|(active, name)| active.then_some(name))
                .collect::<Vec<_>>();
                if !conditions.is_empty() {
                    ui.label(format!("Condition: {}", conditions.join(", ")));
                }
                ui.colored_label(
                    MUTED,
                    "Attack success, active defense and injury are separate resolution stages.",
                );
            }
        }
        Target::Structure(structure) => {
            let definition = content().structure(structure.kind);
            ui.strong(format!("{} · lane {}", definition.name, structure.lane + 1));
            ui.label(format!(
                "Durability {} / {}",
                structure.hp, structure.max_hp
            ));
            ui.add(
                egui::ProgressBar::new(structure.hp.max(0) as f32 / structure.max_hp.max(1) as f32)
                    .fill(GREEN.gamma_multiply(0.45)),
            );
            match structure.kind {
                StructureKind::Wall => {
                    ui.label("Blocks the lane; enemies attack it before advancing.");
                }
                StructureKind::Turret => {
                    ui.label(format!(
                        "Range {:.1} yards · nearest-line priority",
                        definition.range
                    ));
                    ui.label(format!(
                        "Next attack in {:.1}s",
                        structure.cooldown as f32 / 60.0
                    ));
                }
                StructureKind::Trap => {
                    ui.label(format!(
                        "{} charges remaining · traversable",
                        structure.charges
                    ));
                    ui.label(format!("Ready in {:.1}s", structure.cooldown as f32 / 60.0));
                }
            }
            let refund = if attempt.tick == 0 {
                structure.cost
            } else {
                (structure.cost as f32 * 0.5 * structure.hp.max(0) as f32
                    / structure.max_hp.max(1) as f32)
                    .floor() as u32
            };
            ui.label(format!(
                "Placement: {} supply · salvage now: {refund}",
                structure.cost
            ));
            ui.colored_label(MUTED,if attempt.tick==0 {"Full refund before combat. Construction and salvage can be used live."}else{"Salvage refunds half the remaining-health value. Destroyed defenses refund nothing."});
        }
    }
}
pub fn npc_decision(ui: &mut egui::Ui, attempt: &Attempt) {
    if attempt.mode != Mode::Npc {
        return;
    }
    ui.strong("Current survivor decision");
    ui.label(&attempt.decision);
    ui.colored_label(
        MUTED,
        format!(
            "Next legal decision in {:.1}s",
            attempt.next_decision_seconds()
        ),
    );
    if let Some(id) = attempt.decision_target {
        ui.label(format!("Committed target #{id}"));
    }
    if let Some(line) = attempt.log.iter().rev().find(|line| line.contains("3d6")) {
        egui::CollapsingHeader::new("Latest resolution")
            .default_open(
                crate::environment::string("LASTLIGHT_REVIEW_SCREEN").as_deref()
                    == Some("npc-expanded")
                    && !crate::environment::flag("LASTLIGHT_PRACTICE"),
            )
            .show(ui, |ui| {
                ui.label(RichText::new(line).size(12.0));
                ui.colored_label(
                    MUTED,
                    "This is a resolved action, not a prediction of the next roll.",
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::{content::EnemyKind, sim::Game};
    #[test]
    fn inspecting_actual_objects_does_not_change_state_or_randomness() {
        let mut game = Game::default();
        game.campaign.choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let mut attempt = game.attempt.take().unwrap();
        game.spawn_enemy(&mut attempt, EnemyKind::Runner, 0, false);
        attempt.enemies[0].x = 7.0;
        let before = serde_json::to_value(&attempt).unwrap();
        assert!(matches!(
            target_at(&attempt, [7.0, 0.5]),
            Some(Target::Enemy(_))
        ));
        assert!(target_at(&attempt, [7.0, 1.5]).is_none());
        assert!(target_at(&attempt, [f32::NAN, 0.5]).is_none());
        assert_eq!(serde_json::to_value(&attempt).unwrap(), before);
    }
}
