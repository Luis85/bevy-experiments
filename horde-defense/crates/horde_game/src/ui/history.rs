//! Read-only run analysis from the authoritative, persistent settlement ledger.
use super::{UiCtx, armory, chrome, muted};
use crate::art::{AMBER, GREEN, INK, MUTED, PANEL, RED};
use bevy_egui::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, pos2, vec2};
use horde_core::stats::RunRecord;
use horde_core::{content::Mode, sim::Outcome};

#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct HistoryUi {
    mode: Option<Mode>,
    outcome: Option<Outcome>,
    front: String,
    selected: Option<u64>,
}
/// The one next step offered while the settlement ledger is empty.
const EMPTY_NEXT_STEP: &str = "Finish or retreat from a defense to record your first run.";
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Personal => "Personal",
        Mode::Npc => "NPC",
    }
}
fn matching<'a>(records: &'a [RunRecord], state: &HistoryUi) -> Vec<&'a RunRecord> {
    let front = state.front.trim().to_lowercase();
    records
        .iter()
        .filter(|r| {
            state.mode.is_none_or(|m| r.mode == m)
                && state.outcome.is_none_or(|o| r.outcome == o)
                && (front.is_empty()
                    || r.country_name.to_lowercase().contains(&front)
                    || r.country_id
                        .as_ref()
                        .is_some_and(|id| id.to_lowercase().contains(&front)))
        })
        .collect()
}
pub(super) fn show(root: &mut egui::Ui, f: &mut UiCtx) {
    let mut close = false;
    egui::CentralPanel::default().frame(egui::Frame::new().fill(INK).inner_margin(20)).show(root, |ui| {
        // The page title names the screen (the global header does not
        // repeat it) and shares a row with its one way back.
        chrome::row(ui, |ui| {
            ui.label(RichText::new("Run history").font(FontId::new(28.0, egui::FontFamily::Name("display".into()))));
            ui.add_space(16.0);
            if ui.button("Back").on_hover_text("Return to the page you opened history from").clicked() { close = true; }
        });
        muted(ui, "Each settled run is recorded once; its rewards are already banked. Fields stay paused.");
        ui.add_space(6.0);
        let records = &f.game.campaign().run_history;
        let state = &mut f.history;
        if records.is_empty() {
            chrome::section(ui, "Your first report starts here");
            ui.label("Completed defenses, defeats and withdrawals are recorded automatically. Older saves begin with an empty ledger; past results are not invented.");
            ui.add_space(6.0);
            ui.colored_label(AMBER, EMPTY_NEXT_STEP);
            return;
        }
        let shown = matching(records, state).len();
        ui.horizontal_wrapped(|ui| {
            let row = chrome::begin_filter_row(ui);
            egui::ComboBox::from_id_salt("history-mode").selected_text(state.mode.map_or("All modes", mode_name)).show_ui(ui, |ui| {
                ui.selectable_value(&mut state.mode, None, "All modes");
                for mode in [Mode::Personal, Mode::Npc] { ui.selectable_value(&mut state.mode, Some(mode), mode_name(mode)); }
            });
            egui::ComboBox::from_id_salt("history-outcome").selected_text(state.outcome.map_or("All outcomes", Outcome::label)).show_ui(ui, |ui| {
                ui.selectable_value(&mut state.outcome, None, "All outcomes");
                for outcome in [Outcome::Victory, Outcome::Defeat, Outcome::Withdrawal] { ui.selectable_value(&mut state.outcome, Some(outcome), outcome.label()); }
            });
            ui.add(egui::TextEdit::singleline(&mut state.front).hint_text("Filter front / country").desired_width(180.0).min_size(vec2(180.0, row)).vertical_align(egui::Align::Center));
            // Same height as the dropdowns beside it; nothing to clear, nothing to press.
            let filtered = state.mode.is_some() || state.outcome.is_some() || !state.front.is_empty();
            if ui.add_enabled(filtered, egui::Button::new("Clear filters").min_size(vec2(0.0, row))).clicked() { state.mode = None; state.outcome = None; state.front.clear(); }
            muted(ui, format!("{shown} of {} runs · newest first", records.len()));
        });
        // A filter changed this frame applies before the list renders.
        let rows = matching(records, state);
        if !rows.iter().any(|r| Some(r.id) == state.selected) { state.selected = rows.last().map(|r| r.id); }
        if rows.is_empty() { ui.label("No runs match these filters."); return; }
        egui::ScrollArea::vertical().id_salt("history-page").show(ui, |ui| {
            if ui.available_width() >= 760.0 {
                ui.columns(2, |cols| {
                    run_list(&mut cols[0], &rows, &mut state.selected);
                    if let Some(run) = rows.iter().find(|r| Some(r.id) == state.selected) { detail(&mut cols[1], run); }
                });
            } else {
                run_list(ui, &rows, &mut state.selected);
                if let Some(run) = rows.iter().find(|r| Some(r.id) == state.selected) { detail(ui, run); }
            }
            chrome::section(ui, "Run trends");
            muted(ui, "Last 40 matching runs · oldest to newest · simulation seconds, including at 4× playback");
            let recent = &rows[rows.len().saturating_sub(40)..];
            let dps: Vec<_> = recent.iter().filter(|r|!r.legacy).map(|r| (r.id, f64::from(r.average_dps))).collect();
            let coins: Vec<_> = recent.iter().map(|r| (r.id, r.coins as f64)).collect();
            let threat: Vec<_> = recent.iter().map(|r| (r.id, f64::from(r.threat_rank))).collect();
            if ui.available_width() >= 660.0 {
                ui.columns(3, |cols| { trend(&mut cols[0], "Average DPS", &dps, RED); trend(&mut cols[1], "Banked coins", &coins, AMBER); trend(&mut cols[2], "Deployment threat", &threat, GREEN); });
            } else { trend(ui, "Average DPS", &dps, RED); trend(ui, "Banked coins", &coins, AMBER); trend(ui, "Deployment threat", &threat, GREEN); }
        });
    });
    if close
        || (root.input(|i| i.key_pressed(egui::Key::Escape))
            && !root.ctx().text_edit_focused()
            && !root.ctx().any_popup_open())
    {
        f.close_history();
    }
}
fn run_list(ui: &mut egui::Ui, rows: &[&RunRecord], selected: &mut Option<u64>) {
    egui::ScrollArea::vertical()
        .id_salt("run-list")
        .max_height(250.0)
        .show_rows(ui, 58.0, rows.len(), |ui, range| {
            for index in range {
                let run = rows[rows.len() - 1 - index];
                let color = match run.outcome {
                    Outcome::Victory => GREEN,
                    Outcome::Defeat => RED,
                    Outcome::Withdrawal => AMBER,
                };
                let dps = if run.legacy {
                    "DPS —".to_owned()
                } else {
                    format!("{:.1} DPS", super::hud::shown_dps(run.average_dps))
                };
                let label = format!(
                    "#{} · {} · {}\n{} · defense {} · {:.0}s · {}",
                    run.id,
                    run.outcome.label(),
                    mode_name(run.mode),
                    run.country_name,
                    run.stage + 1,
                    run.duration_seconds(),
                    dps
                );
                // Left-aligned like the detail column it selects.
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).color(color))
                            .selected(*selected == Some(run.id))
                            .min_size(vec2(ui.available_width(), 56.0)),
                    )
                    .clicked()
                {
                    *selected = Some(run.id);
                }
            }
        });
}
fn detail(ui: &mut egui::Ui, r: &RunRecord) {
    chrome::section(ui, &format!("Run #{} · {}", r.id, r.outcome.label()));
    ui.label(format!(
        "{} · {} · defense {} · template {}",
        r.defender_name,
        r.country_name,
        r.stage + 1,
        r.template + 1
    ));
    muted(
        ui,
        format!(
            "{} · {:.1}s · {} lanes / {:.0} yd · threat {}",
            mode_name(r.mode),
            r.duration_seconds(),
            r.lane_count,
            r.width_yards,
            r.threat_rank
        ),
    );
    if let Some(worker) = r.worker_id {
        muted(ui, format!("Parallel field #{worker}"));
    }
    if r.legacy {
        ui.label(format!("{} kills · full-run DPS not recorded", r.kills));
    } else {
        ui.label(format!(
            "DPS {:.1} average · {:.1} peak (5s) · {} kills",
            super::hud::shown_dps(r.average_dps),
            super::hud::shown_dps(r.peak_dps),
            r.kills
        ));
    }
    muted(
        ui,
        format!(
            "Resolved damage: defender {} · defenses {} · ordnance {}",
            r.damage_by_source.defender, r.damage_by_source.structures, r.damage_by_source.ordnance
        ),
    );
    ui.label(format!(
        "{} coins · {} kill XP + {} completion XP · {} items",
        r.coins, r.kill_xp, r.completion_xp, r.loot
    ));
    muted(
        ui,
        format!(
            "{} shots · {} resolved hits · {} misses · end HP {}/{}",
            r.shots, r.hits, r.misses, r.end_hp, r.max_hp
        ),
    );
    if !r.cause.is_empty() {
        ui.label(&r.cause);
    }
    if r.legacy {
        muted(
            ui,
            "Legacy checkpoint: damage before this checkpoint was not recorded. Full-run DPS is omitted.",
        );
    }
    egui::CollapsingHeader::new("Frozen balance")
        .id_salt(r.id)
        .show(ui, |ui| {
            ui.label(r.balance.preset.name());
            ui.label(format!(
                "Zombie health {:.2}× · damage {:.2}× · speed {:.2}×",
                r.balance.enemy_health, r.balance.enemy_damage, r.balance.enemy_speed
            ));
            ui.label(format!(
                "Spawn {:.2}× · NPC spawn {:.2}× · rewards {:.2}×",
                r.balance.spawn_rate, r.balance.npc_spawn_rate, r.balance.reward
            ));
            ui.label(format!(
                "Defender damage {:.2}× · defender health {:.2}× · boss time {:.2}×",
                r.balance.player_damage, r.balance.defender_health, r.balance.boss_time
            ));
            ui.label(format!(
                "Supply {:+} · victory threat {:.2}×",
                r.balance.supply, r.balance.threat_growth
            ));
        });
    egui::CollapsingHeader::new("Deployed equipment")
        .id_salt((r.id, "gear"))
        .default_open(true)
        .show(ui, |ui| {
            for gear in &r.gear {
                ui.colored_label(
                    armory::quality_color(gear.quality),
                    format!(
                        "{} · {} · Lv{} · {:?}",
                        gear.name,
                        gear.quality.name(),
                        gear.level,
                        gear.slot
                    ),
                );
                if let Some(class) = gear.weapon_class {
                    muted(ui, format!("{} proficiency", class.name()));
                }
            }
            if r.gear.is_empty() {
                muted(ui, "No equipment snapshot available.");
            }
        });
    ui.horizontal_wrapped(|ui| {
        for quality in horde_core::quality::Quality::ALL {
            if r.loot_by_quality[quality] > 0 {
                ui.colored_label(
                    armory::quality_color(quality),
                    format!("{} {}", r.loot_by_quality[quality], quality.name()),
                );
            }
        }
    });
}
fn trend(ui: &mut egui::Ui, title: &str, values: &[(u64, f64)], color: Color32) {
    ui.label(RichText::new(title).strong());
    if values.is_empty() {
        muted(ui, "Not recorded");
        return;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 86.0), Sense::hover());
    let plot = rect.shrink2(vec2(8.0, 10.0));
    let max = values.iter().map(|p| p.1).fold(0.0_f64, f64::max).max(1.0);
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, p)| {
            pos2(
                plot.left()
                    + plot.width() * i as f32 / (values.len().saturating_sub(1).max(1)) as f32,
                plot.bottom() - plot.height() * (p.1 / max) as f32,
            )
        })
        .collect();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, PANEL);
    painter.line_segment(
        [plot.left_bottom(), plot.right_bottom()],
        Stroke::new(1.0, MUTED.gamma_multiply(0.4)),
    );
    if points.len() > 1 {
        painter.add(egui::Shape::line(points.clone(), Stroke::new(2.0, color)));
    }
    for point in &points {
        painter.circle_filled(*point, 2.5, color);
    }
    painter.text(
        plot.left_top(),
        Align2::LEFT_TOP,
        format!("{max:.1}"),
        FontId::proportional(10.0),
        MUTED,
    );
    if let Some(pointer) = response.hover_pos()
        && !values.is_empty()
    {
        let index = (((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0)
            * values.len().saturating_sub(1) as f32)
            .round() as usize;
        response.on_hover_text(format!(
            "Run #{} · {title}: {:.1}",
            values[index].0, values[index].1
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::sim::Game;
    use horde_core::sim::Input;
    use horde_core::survivors::SurvivorId;
    #[test]
    fn filters_preserve_settlement_order_and_read_real_records_without_mutation() {
        let mut game = Game::default();
        game.campaign_mut().legacy_features = true;
        game.campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        for mode in [Mode::Personal, Mode::Npc, Mode::Personal] {
            game.deploy_country_stage(0, mode).unwrap();
            for _ in 0..120 {
                game.tick(Input::default());
            }
            game.withdraw();
        }
        assert_eq!(game.campaign().run_history.len(), 3);
        let before = serde_json::to_value(&game).unwrap();
        let state = HistoryUi {
            mode: Some(Mode::Personal),
            outcome: Some(Outcome::Withdrawal),
            front: "deu".into(),
            selected: None,
        };
        let rows = matching(&game.campaign().run_history, &state);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].id < rows[1].id);
        assert_eq!(rows[0].duration_ticks, 120);
        assert_eq!((rows[0].lane_count, rows[0].width_yards), (3, 12.0));
        assert_eq!(rows[0].average_dps, 0.0);
        let missing = HistoryUi {
            front: "no such country".into(),
            ..HistoryUi::default()
        };
        assert!(matching(&game.campaign().run_history, &missing).is_empty());
        let npc = HistoryUi {
            mode: Some(Mode::Npc),
            ..HistoryUi::default()
        };
        assert_eq!(matching(&game.campaign().run_history, &npc).len(), 1);
        assert_eq!(serde_json::to_value(&game).unwrap(), before);
    }
}
