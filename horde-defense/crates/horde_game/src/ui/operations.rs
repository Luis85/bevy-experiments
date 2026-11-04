//! Operations presentation. Game changes are queued typed intents.
use super::{Screen, UiCtx, battle, chrome, damage, loot, muted, reticle};
use crate::art::{self, AMBER, RED};
use bevy_egui::egui::{self, Sense, vec2};
use horde_core::{
    campaign::NpcTraining,
    command::Command,
    content::{Policy, Slot, StructureKind, content},
    world::world,
};

pub(super) fn operations(ui: &mut egui::Ui, f: &mut UiCtx) {
    use horde_core::strategy::{OutbreakOutcome, RecruitStatus};
    let config = &world().strategy_config;
    chrome::page_title(
        ui,
        "Operations",
        "Watch live fields, respond to outbreaks and give orders to your survivor roster.",
    );
    muted(
        ui,
        if f.game.global_paused {
            "World clock paused · every field and outbreak timer is paused."
        } else {
            "World clock running · expeditions and outbreak deadlines advance together."
        },
    );
    if f.game.attempt.is_some() {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Your active field");
            if ui.button("Return to active defense").clicked() {
                f.go(Screen::Battle);
            }
        });
    }
    let outbreaks: Vec<_> = f
        .game
        .strategy
        .outbreaks
        .iter()
        .filter(|o| o.outcome == OutbreakOutcome::Active)
        .cloned()
        .collect();
    if !outbreaks.is_empty() {
        chrome::section(ui, "Urgent outbreaks");
        for outbreak in outbreaks {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    RED,
                    format!(
                        "{} · {} groups · {:.0}s remaining",
                        country_name(&outbreak.country_id),
                        outbreak.strength,
                        outbreak.deadline_tick.saturating_sub(f.game.strategy.tick) as f32 / 60.0
                    ),
                );
                if ui.button("Plan outbreak response").clicked() {
                    f.campaign.selected_country = outbreak.country_id.clone();
                    f.dispatch(
                        Command::EnterCountry(outbreak.country_id),
                        "Replay a defense here or dispatch a survivor to contain the outbreak.",
                    );
                }
            });
        }
    }
    chrome::section(ui, "Live expeditions");
    let summaries: Vec<_> = f
        .game
        .battles
        .iter()
        .map(|b| {
            (
                b.id,
                b.recruit_id,
                b.country_id.clone(),
                b.stage,
                b.settled,
                b.game
                    .attempt
                    .as_ref()
                    .map(|a| (a.hp, a.max_hp, a.elapsed(), a.kills, a.decision.clone())),
                b.game.last_result.clone(),
            )
        })
        .collect();
    let live = summaries
        .iter()
        .filter(|(_, _, _, _, settled, state, _)| !*settled && state.is_some())
        .count();
    if live == 0 {
        muted(
            ui,
            "No expeditions in progress. Ready survivors can be sent from the roster below once parallel battles unlock.",
        );
    }
    for (id, recruit, country, stage, settled, state, _) in &summaries {
        if *settled {
            continue;
        }
        let Some((hp, max, elapsed, kills, decision)) = state else {
            continue;
        };
        let name = f
            .game
            .strategy
            .roster
            .iter()
            .find(|r| r.id == *recruit)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Survivor".into());
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!(
                "{name} · {} · defense {}",
                country_name(country),
                stage + 1
            ));
            ui.label(format!("HP {hp}/{max} · {elapsed:.0}s · {kills} kills"));
            if ui.button(format!("Observe #{id}")).clicked() {
                f.campaign.observed = Some(*id);
            }
            if ui.button(format!("Recall #{id}")).clicked() {
                f.dispatch(
                    Command::RecallSurvivor(*id),
                    "Expedition recalled; earned rewards retained.",
                );
            }
        });
        muted(ui, decision);
        ui.separator();
    }
    if summaries.iter().any(|(_, _, _, _, settled, _, _)| *settled) {
        egui::CollapsingHeader::new("Completed expedition reports").show(ui, |ui| {
            for (id, _, country, stage, settled, _, report) in &summaries {
                if *settled && let Some(report) = report {
                    ui.label(format!(
                        "Expedition #{id} · {} · defense {} · {:?} · {} kills · {} coins",
                        country_name(country),
                        stage + 1,
                        report.outcome,
                        report.kills,
                        report.coins
                    ));
                }
            }
        });
    }
    chrome::section(ui, "Survivor orders");
    let recruit_unlocked = f.game.strategy.recruitment_unlocked(&f.game.campaign);
    let parallel = f.game.strategy.parallel_unlocked(&f.game.campaign);
    let strategy = f.game.strategy.grand_strategy_unlocked(&f.game.campaign);
    if !recruit_unlocked {
        muted(
            ui,
            format!(
                "Liberate {} country to recruit local survivors. Parallel battles unlock after {}; country operations and guards after {}.",
                config.recruitment_unlock_freed,
                config.parallel_unlock_freed,
                config.strategy_unlock_freed
            ),
        );
        if ui.button("Choose your next defense").clicked() {
            f.open_briefing();
        }
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.label("Orders for");
            egui::ComboBox::from_id_salt("operations_country").selected_text(country_name(&f.campaign.selected_country)).show_ui(ui, |ui| {
                for country in &world().countries {
                    if f.game.campaign.world.can_enter(&country.id) { ui.selectable_value(&mut f.campaign.selected_country, country.id.clone(), &country.name); }
                }
            });
            let liberated = f.game.campaign.world.cleared.contains(&f.campaign.selected_country);
            let space = f.game.strategy.roster.len() < config.max_roster;
            let can_recruit = liberated && space && f.game.campaign.coins >= config.recruit_cost && f.game.campaign.reserved.is_none();
            if ui.add_enabled(can_recruit, egui::Button::new(format!("Recruit survivor · {} coins", config.recruit_cost))).on_disabled_hover_text("Choose a liberated country, keep space in the roster and enough coins, and return the active defender to camp.").clicked() { f.dispatch(Command::Recruit(f.campaign.selected_country.clone()), "Survivor recruited with independent protected equipment."); }
        });
        muted(
            ui,
            "Recruits originate in liberated countries and bring independent protected equipment.",
        );
        if !parallel {
            muted(
                ui,
                format!(
                    "Parallel battles unlock after {} liberated countries. You can equip and train recruits now.",
                    config.parallel_unlock_freed
                ),
            );
        }
        if !strategy {
            muted(
                ui,
                format!(
                    "Country operations and guard duty unlock after {} liberated countries.",
                    config.strategy_unlock_freed
                ),
            );
        }
    }
    for recruit in f.game.strategy.roster.clone() {
        ui.push_id(recruit.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&recruit.name);
                ui.label(format!("{:?} · from {}", recruit.status, country_name(&recruit.origin)));
                if let Some(guard) = f.game.strategy.guards.iter().find(|g| g.recruit_id == recruit.id) { ui.colored_label(AMBER, format!("Guarding {}", country_name(&guard.country_id))); }
                if recruit.status == RecruitStatus::Guarding && ui.button("Release guard").clicked() { f.dispatch(Command::ReleaseGuard(recruit.id), "Guard recalled and recovered."); }
            });
            if let Ok(readiness) = f.game.strategy.readiness(&f.game.campaign, recruit.id) { muted(ui, &readiness.summary); }
            if recruit.status == RecruitStatus::Ready {
                ui.horizontal_wrapped(|ui| {
                    let stage = f.game.campaign.world.completed(&f.campaign.selected_country).min(world().country(&f.campaign.selected_country).map(|c| c.level_count().saturating_sub(1)).unwrap_or(0));
                    if parallel && ui.button("Deploy single defense").clicked() { f.dispatch(Command::DeploySurvivor { id: recruit.id, country: f.campaign.selected_country.clone(), stage }, "Expedition deployed. Its actual battle runs live."); }
                    if strategy {
                        if ui.button("Liberate country").on_hover_text("Chain real defenses until liberation, defeat, recall or invalid equipment. No automatic purchases or training.").clicked() { f.dispatch(Command::CountryOperation { id: recruit.id, country: f.campaign.selected_country.clone() }, "Country operation started; stops on liberation, defeat or recall."); }
                        if ui.button("Guard selected country").clicked() { f.dispatch(Command::AssignGuard { id: recruit.id, country: f.campaign.selected_country.clone() }, "Garrison assigned. Equipment stays reserved during duty."); }
                    }
                });
                egui::CollapsingHeader::new(format!("Equipment, training & policy · {} CP available", recruit.npc.unspent)).show(ui, |ui| {
                    ui.label(format!("{} / {} advancement awarded · Guns {} · Melee {} · HT {}", recruit.npc.awarded, f.game.campaign.allowance(), recruit.npc.guns, recruit.npc.melee, recruit.npc.ht));
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(f.game.campaign.coins >= 25 && recruit.npc.awarded < f.game.campaign.allowance(), egui::Button::new("Buy 1 training CP · 25 coins")).clicked() { f.dispatch(Command::BuyRecruitPoint(recruit.id), "Training point awarded to the selected survivor."); }
                        for training in [NpcTraining::Guns, NpcTraining::Melee, NpcTraining::Health, NpcTraining::Strength, NpcTraining::Dexterity] {
                            if ui.add_enabled(recruit.npc.unspent >= training.cost() && (training != NpcTraining::Strength || recruit.npc.st < 20), egui::Button::new(format!("{} · {} CP", training.name(), training.cost()))).clicked() { f.dispatch(Command::TrainRecruit { id: recruit.id, training }, "Selected survivor trained."); }
                        }
                    });
                    for slot in Slot::ALL {
                        egui::ComboBox::from_id_salt(("recruit_equipment", recruit.id, slot.index())).selected_text(format!("{:?}: {}", slot, f.game.campaign.item(recruit.npc.loadout[slot.index()]).map(|i| content().items[i.definition].name.as_str()).unwrap_or("Missing"))).show_ui(ui, |ui| {
                            for item in f.game.campaign.items.clone().iter().filter(|i| content().items[i.definition].slot == slot) {
                                let reserved = f.game.campaign.item_reserved(item.id);
                                if ui.add_enabled(!reserved, egui::Button::selectable(recruit.npc.loadout.contains(&item.id), format!("{} #{} · Lv {}{}", content().items[item.definition].name, item.id, item.level, if reserved { " · reserved" } else { "" }))).clicked() { f.dispatch(Command::EquipRecruit { id: recruit.id, item: item.id }, "Survivor equipment updated."); }
                            }
                        });
                    }
                    let mut proposed = recruit.npc.policy;
                    egui::ComboBox::from_id_salt(("recruit_policy", recruit.id)).selected_text(format!("{proposed:?}")).show_ui(ui, |ui| { for policy in Policy::ALL { ui.selectable_value(&mut proposed, policy, format!("{policy:?}")); } });
                    if proposed != recruit.npc.policy { f.dispatch(Command::SetRecruitPolicy { id: recruit.id, policy: proposed }, "Survivor policy updated."); }
                });
            }
            ui.separator();
        });
    }
    if let Some(id) = f.campaign.observed {
        let mut open = true;
        egui::Window::new(format!("Live battlefield #{id}"))
            .default_size(vec2(
                900.0_f32.min(ui.ctx().content_rect().width() - 32.0),
                540.0,
            ))
            .max_width((ui.ctx().content_rect().width() - 32.0).max(200.0))
            .max_height((ui.ctx().content_rect().height() - 32.0).max(200.0))
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(if f.game.global_paused {
                            "Resume all"
                        } else {
                            "Pause all"
                        })
                        .clicked()
                    {
                        f.dispatch(Command::SetPaused(!f.game.global_paused), "");
                    }
                    if ui.button("Save country plan").clicked() { f.dispatch(Command::SaveExpeditionLayout(id), "Country plan saved for future deployments."); }
                    if ui.button("Clear country plan").clicked() { f.dispatch(Command::ClearExpeditionLayout(id), "Country plan cleared; deployed structures remain."); }
                    muted(ui, "Build live: left click places, right click salvages. Pause all is optional.");
                    for kind in StructureKind::ALL {
                        ui.selectable_value(&mut f.battle.build_kind, kind, format!("{kind:?}"));
                    }
                });
                if let Some(b) = f.game.battles.iter().find(|b| b.id == id)
                    && let Some(a) = b.game.attempt.as_ref()
                {
                    muted(
                        ui,
                        format!(
                            "HP {}/{} · {} · supply {} · threat {} / groups {}",
                            a.hp,
                            a.max_hp,
                            a.action(),
                            a.supply, a.progression.rank, a.progression.batch_size
                        ),
                    );
                    let (r, _) = ui.allocate_exact_size(
                        vec2(
                            ui.available_width(),
                            (ui.ctx().content_rect().height() - 200.0).clamp(180.0, 360.0),
                        ),
                        Sense::click(),
                    );
                    let effects_clip=r;
                    let r=battle::clearing_rect(r,a.lane_count,a.width_yards);
                    let projection=art::ArenaProjection::for_attempt(r,a).with_effects_clip(effects_clip);
                    let response=ui.interact(r,ui.id().with(("observer-arena",id)),Sense::click());
                    let aim = response
                        .hover_pos()
                        .filter(|p| r.contains(*p))
                        .map(|p| projection.inverse(p))
                        .unwrap_or([a.width_yards*0.5,a.defender_position()[1]]);
                    art::world_backdrop(ui.painter(),effects_clip,projection,a.level,f.textures);
                    art::battlefield(
                        &ui.painter_at(effects_clip),
                        r,
                        a,
                        aim,
                        art::RenderOptions {
                            construction: true,
                            outlines: f.settings.outlines,
                            numbers: f.settings.numbers,
                            reduced: f.settings.reduced_effects,
                            blood: f.settings.blood,
                            theme: a.level,
                            textures: f.textures,
                        },
                    );
                    damage::draw_for(ui,a,projection,id,&mut f.battle.damage_feedback,f.settings,f.launch.damage_capture_mode());
                    if loot::draw_for(ui,projection,a,id,f.textures,&mut f.battle.loot_feedback,f.launch.loot_capture_mode()) { f.notify_rare_loot(); }
                    if response.hover_pos().is_some_and(|p| r.contains(p)) && ui.input(|i|i.raw.focused) && !ui.ctx().any_popup_open() {
                        f.battle.custom_cursor_requested=true;ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        reticle::paint(ui,a,projection,aim,battle::AttackSelection::Gun,false,f.settings.reduced_effects);
                    }
                    crate::inspector::hover(&response, a, aim);
                    let build_cell = a.build_cell_at(aim);
                    if response.secondary_clicked() {
                        let selected = a
                            .structures
                            .iter()
                            .find(|structure| {
                                (structure.x()-aim[0]).abs()<=structure.blocking_radius()
                                    && (structure.y() - aim[1]).abs() <= 0.46
                            })
                            .map(|structure| structure.id);
                        if let Some(structure) = selected {
                            f.dispatch(
                                Command::SalvageExpedition {
                                    battle: id,
                                    structure,
                                },
                                "Expedition defense salvaged.",
                            );
                        }
                    }
                    if response.clicked() {
                        if let Some([lane,column])=build_cell {
                            f.dispatch(
                                Command::PlaceExpedition {
                                    battle: id,
                                    kind: f.battle.build_kind,
                                    lane,
                                    column,
                                },
                                "Expedition defense placed.",
                            );
                        } else {
                            f.status.message = "Build only in the left-half construction zone.".into();
                            f.status.message_error = true;
                        }
                    }
                } else {
                    muted(
                        ui,
                        "Deployment complete. Its report remains in the expedition list.",
                    );
                }
            });
        if !open {
            f.campaign.observed = None;
        }
    }
    egui::CollapsingHeader::new("Strategic event log").show(ui, |ui| {
        for line in f.game.strategy.log.iter().rev() {
            muted(ui, line);
        }
    });
}

fn country_name(id: &str) -> &str {
    world()
        .country(id)
        .map(|country| country.name.as_str())
        .unwrap_or(id)
}
