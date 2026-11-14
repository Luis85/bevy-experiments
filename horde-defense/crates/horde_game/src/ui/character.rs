//! Character presentation. Game changes are queued typed intents.
use super::{Tab, UiCtx, chrome, muted, progress};
use crate::art::AMBER;
use bevy_egui::egui::{self, Sense, vec2};
use horde_core::{
    campaign::NpcTraining,
    command::{Command, MAX_RECALL_PERCENT, NpcTactics},
    content::{Mode, Policy, Slot},
    defenders::DefenderId,
};

pub(super) fn character(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::page_title(
        ui,
        "Character",
        "Personal combat develops through the journey upgrade tree.",
    );
    let p = f.game.campaign().player.clone();
    let profile = f.game.campaign().profile(Mode::Personal);
    let (preview, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(260.0), 110.0), Sense::hover());
    crate::visual_assets::defender_preview(
        ui.painter(),
        f.textures,
        &profile,
        preview.center(),
        vec2(100.0, 100.0),
        preview.right_center(),
    );
    ui.horizontal_wrapped(|ui| {
        if f.game.campaign().legacy_features {
            ui.strong(format!("{} legacy CP available in the tree", p.points));
            ui.add_space(16.0);
        }
        ui.label(format!(
            "Level {} · body development in the upgrade tree",
            p.level
        ));
        if f.game.campaign().legacy_features {
            ui.add_space(16.0);
            muted(
                ui,
                format!(
                    "Legacy CP payment rank limit {}",
                    f.game.campaign().body_cap()
                ),
            );
        }
    });
    if p.is_max_level() {
        progress(
            ui,
            1.0,
            format!("MAX · level {}", f.game.campaign().player_level_cap()),
            AMBER,
        );
        muted(
            ui,
            "Character level is capped. Victories still raise future encounter threat.",
        );
    } else {
        let needed = p.next_level_xp().max(1);
        progress(
            ui,
            p.xp as f32 / needed as f32,
            format!("{} / {} XP to next level", p.xp, needed),
            AMBER,
        );
    }
    ui.label(format!(
        "Next deployment: {} lanes · {:.0} yd path",
        f.game.campaign().arena_geometry().0,
        f.game.campaign().arena_geometry().1
    ));
    if let Some(level) = f.game.campaign().next_lane_unlock_level() {
        muted(
            ui,
            format!(
                "The next lane expansion is at character level {level}. Active encounters keep their current geometry."
            ),
        );
    } else {
        muted(
            ui,
            "No further lane expansion under the saved difficulty settings; active encounters keep their frozen geometry.",
        );
    }
    chrome::section(ui, "Development");
    if ui.button("Open journey upgrades").clicked() {
        f.open_upgrades();
    }
    let development = f.game.campaign().modifiers().body_development;
    for (index, name) in [
        "Vitality",
        "Strength",
        "Coordination",
        "Endurance",
        "Perception",
    ]
    .into_iter()
    .enumerate()
    {
        ui.label(format!("{name} · rank {}", {
            let attribute = horde_core::campaign::BodyAttribute::ALL[index];
            p.body[attribute].max(development[attribute])
        }));
    }
    chrome::section(ui, "Ready for the field");
    ui.label(format!(
        "{} HP · {} stamina · {} armor · {} melee damage · {:.2}s reload",
        profile.personal.health,
        profile.personal.stamina,
        profile.personal.armor,
        profile.personal.melee_damage,
        profile.personal.reload
    ));
    ui.horizontal_wrapped(|ui| {
        if ui.button("Change personal equipment").clicked() {
            f.campaign.mode = Mode::Personal;
            f.open_preparation(Tab::Armory);
        }
    });
    egui::CollapsingHeader::new("Current equipment & advancement rules").show(ui, |ui| {
        for slot in Slot::ALL { ui.label(format!("{:?}: {}", slot, f.game.campaign().equipped(Mode::Personal, slot).name)); }
        muted(ui, "Body-development nodes share one journey tree. Legacy point payments and refunds remain in that tree; Mara retains separate training.");
    });
}
/// Mara's tactical policy (`SetNpcPolicy`).
pub(super) fn policy(ui: &mut egui::Ui, f: &mut UiCtx) {
    tactics(ui, f, DefenderId::Mara);
}
/// The tactics of survivor defender `defender`, edited through core: Mara
/// with `SetNpcPolicy`, a recruit (in camp, or defending the main run) with
/// `SetRecruitTactics`. A recruit no longer on the roster shows nothing.
pub(super) fn tactics(ui: &mut egui::Ui, f: &mut UiCtx, defender: DefenderId) {
    let npc = match defender {
        DefenderId::Recruit(id) => {
            let Some(recruit) = f.game.strategy().roster.iter().find(|r| r.id == id) else {
                return;
            };
            &recruit.npc
        }
        DefenderId::Player | DefenderId::Mara => &f.game.campaign().npc,
    };
    let mut proposed = NpcTactics {
        policy: npc.policy,
        prefer_aim: npc.prefer_aim,
        early_reload: npc.early_reload,
        allow_risk: npc.allow_risk,
        recall_percent: npc.recall_percent,
    };
    let before = proposed.clone();
    egui::ComboBox::from_id_salt("policy")
        .selected_text(format!("{:?}", proposed.policy))
        .show_ui(ui, |ui| {
            for p in Policy::ALL {
                ui.selectable_value(&mut proposed.policy, p, format!("{p:?}"));
            }
        });
    ui.checkbox(&mut proposed.prefer_aim, "Aim when the line is safe");
    ui.checkbox(&mut proposed.early_reload, "Reload early when safe");
    ui.checkbox(&mut proposed.allow_risk, "Permit risky attacks")
        .on_hover_text(
            "Allows the authored risky maneuver when legal. Policy never changes dice results.",
        );
    ui.label("Recall below HP (0% disables)");
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 60.0).clamp(80.0, 160.0);
        ui.add(egui::Slider::new(&mut proposed.recall_percent, 0..=MAX_RECALL_PERCENT).suffix("%"));
    });
    if proposed != before {
        f.dispatch(
            match defender {
                DefenderId::Recruit(id) => Command::SetRecruitTactics {
                    id,
                    tactics: proposed,
                },
                DefenderId::Player | DefenderId::Mara => Command::SetNpcPolicy(proposed),
            },
            "Policy applies at the next legal decision.",
        );
    }
}

pub(super) fn survivor(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::page_title(
        ui,
        "Mara Vale",
        "Train and equip your autonomous defender, then review her tactical policy.",
    );
    if !f.game.campaign().npc_available() {
        ui.colored_label(AMBER, "Mara joins after your introductory attempt.");
    }
    let n = f.game.campaign().npc.clone();
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("{} CP available", n.unspent));
        ui.add_space(16.0);
        ui.label(format!(
            "{} / {} advancement awarded",
            n.awarded,
            f.game.campaign().allowance()
        ));
        ui.add_space(16.0);
        ui.label(format!(
            "Guns {} · Melee {} · HP {} · FP {} · Dodge {}",
            n.guns,
            n.melee,
            n.max_hp(),
            n.max_fp(),
            n.dodge()
        ));
    });
    chrome::section(ui, "Training");
    ui.horizontal_wrapped(|ui| {
        let buy = super::camp_prices::training_point(f.game.campaign(), &n);
        if ui
            .add_enabled(buy.enabled, egui::Button::new(buy.label))
            .on_disabled_hover_text(buy.disabled_hover)
            .clicked()
        {
            f.dispatch(Command::BuyNpcPoint, "Training point awarded.");
        }
        if ui.button("Change Mara's equipment").clicked() {
            f.campaign.mode = Mode::Npc;
            f.open_preparation(Tab::Armory);
        }
    });
    muted(
        ui,
        format!(
            "Earned objectives: {} / 20 ranged hits · {:.0} / 120 combat seconds. Earned awards are claimed before a paid point.",
            n.successful_hits.min(20),
            (n.combat_ticks as f32 / 60.0).min(120.0)
        ),
    );
    for row in training_rows(&n) {
        ui.push_id(row.label, |ui| {
            chrome::row(ui, |ui| {
                ui.strong(row.label);
                ui.label(format!("{} CP", row.cost));
                if ui
                    .add_enabled(row.enabled, egui::Button::new("Train"))
                    .on_disabled_hover_text(row.disabled_hover)
                    .clicked()
                {
                    f.dispatch(Command::TrainNpc(row.training), "Survivor trained.");
                }
            });
        });
    }
    if ui.button("Redistribute advancement for free").clicked() {
        f.dispatch(
            Command::RespecNpc,
            "Points refunded; historical awards preserved.",
        );
    }
    chrome::section(ui, "Tactical policy");
    policy(ui, f);
    egui::CollapsingHeader::new("Character ledger & combat rules").show(ui, |ui| {
        ui.label(format!("Creation budget {} CP · ST {} · DX {} · HT {}", n.creation.budget, n.st, n.dx, n.ht));
        muted(ui, "Free survivor with campaign recovery. Redistribution refunds advancement; historical awards and the campaign allowance remain intact. HP and FP derive from ST and HT.");
        muted(ui, "GURPS Fourth Edition Lite (August 2004), supported subset. Attack success precedes active defense and armor. A shot resolves once. Zero HP requires consciousness checks; it does not mean death. Stun and knockdown allow recovery.");
        for skill in [8, 10, 12, 14, 16] { ui.label(format!("Skill {skill}: {:.1}% attack-roll success, before defense/armor", horde_core::rules::attack_success_probability(skill) * 100.0)); }
    });
}

/// One training row of a survivor defender (Mara or a recruit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TrainingRow {
    pub training: NpcTraining,
    pub label: &'static str,
    pub cost: u32,
    pub enabled: bool,
    pub disabled_hover: &'static str,
}
/// The rows `npc` can train: core's `NpcTraining::TRAINABLE`, in its order,
/// for Mara's page and the recruit rows in Operations alike. A row is
/// enabled when the survivor has the CP and, for Strength, room under the
/// supported maximum.
pub(super) fn training_rows(npc: &horde_core::campaign::Npc) -> Vec<TrainingRow> {
    NpcTraining::TRAINABLE
        .into_iter()
        .map(|training| {
            let supported = training != NpcTraining::Strength || npc.st < 20;
            TrainingRow {
                training,
                label: training.name(),
                cost: training.cost(),
                enabled: supported && npc.unspent >= training.cost(),
                disabled_hover: if supported {
                    "Earn or buy enough training CP first."
                } else {
                    "Supported Strength maximum is 20."
                },
            }
        })
        .collect()
}

pub(super) fn mode_selector(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::mode_picker(ui, f);
}
