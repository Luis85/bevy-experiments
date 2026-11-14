//! The Equipment Auto-sell panel (W3-21; design-loot-economy §9,
//! design-ux-flow §4 "Auto-sell panel"). Render only: the rule shown is the
//! campaign's, every control dispatches `Command::SetAutoSell` with the
//! changed rule, and the counts are `Game::auto_sell_preview`. "Apply rule
//! to stash now…" only opens the confirmation (`AppModal::ApplyAutoSell`),
//! which carries the quote the player confirms.
use crate::art::{AMBER, INK, TEXT};
use crate::settings::Settings;
use crate::ui::{UiCtx, chrome, modal::AppModal, muted};
use bevy_egui::egui::{
    self, Response, Sense, Stroke, StrokeKind, TextStyle, TextWrapMode, WidgetInfo, WidgetText,
    WidgetType, vec2,
};
use horde_core::{
    campaign::{AutoSellRule, SaleBlock},
    command::{Command, CommandError},
    content::Slot,
    quality::Quality,
    sim::{AutoSellPreview, AutoSellScope, Game},
};

const HEADER: &str = "Auto-sell";
const ENABLED: &str = "Auto-sell new drops";
const ENABLED_HOVER: &str = "When a defense, an expedition or an operation is settled, its drops \
     that match this rule are sold. Nothing is sold during a battle.";
const KEEP_UPGRADES: &str = "Keep upgrades for the defender who ran it";
const NPC_RUNS: &str = "Also on expeditions and operations";
const APPLY: &str = "Apply rule to stash now…";
const APPLY_HOVER: &str = "Asks first. Sells what the rule matches in your stash right now, \
     whether or not auto-sell is on.";

/// One threshold chip: its label, the rule's `sell_below` and the best
/// quality that threshold sells (the chip's swatch and shape).
struct Chip {
    label: &'static str,
    sell_below: Quality,
    best: Quality,
}
/// The five thresholds `SetAutoSell` accepts, cheapest first.
const CHIPS: [Chip; 5] = [
    Chip {
        label: "Poor only",
        sell_below: Quality::Common,
        best: Quality::Poor,
    },
    Chip {
        label: "Common and below (default)",
        sell_below: Quality::Uncommon,
        best: Quality::Common,
    },
    Chip {
        label: "Uncommon and below",
        sell_below: Quality::Rare,
        best: Quality::Uncommon,
    },
    Chip {
        label: "Rare and below",
        sell_below: Quality::Epic,
        best: Quality::Rare,
    },
    Chip {
        label: "Epic and below",
        sell_below: Quality::Legendary,
        best: Quality::Epic,
    },
];

/// What the panel shows besides the rule's own switches.
#[derive(Debug, PartialEq)]
struct PanelModel {
    rule: AutoSellRule,
    /// The stash preview: the quote "Apply rule to stash now…" confirms.
    now: AutoSellPreview,
    /// "Would sell N now · +C".
    would_sell: String,
    /// What else the stash preview counted, for the line's hover.
    would_sell_detail: Vec<String>,
    /// "Last run would have sold N · +C", while that run's drops are owned.
    last_run: Option<String>,
    /// The threshold sells Rare or Epic finds.
    warning: Option<String>,
    /// The recent drops are all too good for the threshold.
    hint: Option<String>,
    /// Why the stash apply cannot run, in core's words.
    apply_blocked: Option<String>,
}

/// The amber warning under the chips: a threshold that sells Rare finds, or
/// Rare and Epic ones, says so in words.
fn threshold_warning(sell_below: Quality) -> Option<String> {
    let dear: Vec<&str> = [Quality::Rare, Quality::Epic]
        .into_iter()
        .filter(|quality| *quality < sell_below)
        .map(Quality::name)
        .collect();
    (!dear.is_empty()).then(|| format!("Also sells {} finds.", dear.join(" and ")))
}

/// "N Quality" for each quality the preview sells, best first.
fn sold_by_quality(preview: &AutoSellPreview) -> Vec<String> {
    Quality::ALL
        .into_iter()
        .rev()
        .filter(|quality| preview.by_quality[*quality] > 0)
        .map(|quality| format!("{} {}", preview.by_quality[quality], quality.name()))
        .collect()
}

fn panel_model(game: &Game) -> PanelModel {
    let rule = game.campaign().auto_sell;
    let now = game.auto_sell_preview(&rule, AutoSellScope::Stash);
    let mut would_sell_detail = sold_by_quality(&now);
    if now.kept_upgrades > 0 {
        would_sell_detail.push(format!("{} kept as upgrades", now.kept_upgrades));
    }
    if now.blocked > 0 {
        would_sell_detail.push(format!(
            "{} kept: locked, favourite, equipped, preset or recruit gear",
            now.blocked
        ));
    }
    // The reported run's drops still in the stash (none for a worker's
    // report, whose ids are its own).
    let kept = game
        .results_loot()
        .map(|loot| loot.kept)
        .unwrap_or_default();
    let last = (!kept.is_empty()).then(|| game.auto_sell_preview(&rule, AutoSellScope::LastRun));
    let too_good = last.as_ref().is_some_and(|last| last.count == 0)
        && kept.iter().all(|drop| {
            drop.quality
                .is_some_and(|quality| quality >= rule.sell_below)
        });
    let hint = too_good
        .then(|| {
            Quality::ALL
                .into_iter()
                .find(|quality| *quality > rule.sell_below)
        })
        .flatten()
        .filter(|next| {
            let raised = AutoSellRule {
                sell_below: *next,
                ..rule
            };
            game.auto_sell_preview(&raised, AutoSellScope::Stash).count > 0
        })
        .map(|_| {
            format!(
                "Your recent drops are {} or better; raise the threshold to keep selling",
                rule.sell_below.name()
            )
        });
    let apply_blocked = if game.campaign().reserved.is_some() {
        Some(SaleBlock::InBattle.label())
    } else if now.count == 0 {
        Some(CommandError::NothingToSell.to_string())
    } else {
        None
    };
    PanelModel {
        rule,
        would_sell: format!("Would sell {} now · +{}", now.count, now.coins),
        would_sell_detail,
        last_run: last
            .map(|last| format!("Last run would have sold {} · +{}", last.count, last.coins)),
        warning: threshold_warning(rule.sell_below),
        hint,
        apply_blocked,
        now,
    }
}

/// A threshold chip: the best quality's shape in the palette's colour (its
/// swatch and its decision-18 marker in one) and the range in words. The
/// shape is painted whatever the marker setting: the chips are the control
/// that picks a quality, so they never rely on colour alone. The selected
/// chip has an amber outline on an ink fill.
fn threshold_chip(ui: &mut egui::Ui, chip: &Chip, selected: bool, settings: &Settings) -> Response {
    let padding = ui.spacing().button_padding;
    let side = chrome::QUALITY_SHAPE_SIDE;
    let gap = 6.0;
    let galley = WidgetText::from(chip.label).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let size = (vec2(side + gap + galley.size().x, galley.size().y.max(side)) + 2.0 * padding)
        .max(vec2(0.0, ui.spacing().interact_size.y));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::SelectableLabel, enabled, selected, chip.label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if selected {
            INK
        } else if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let radius = visuals.widgets.inactive.corner_radius;
        let painter = ui.painter();
        painter.rect_filled(rect, radius, fill);
        if selected {
            painter.rect_stroke(rect, radius, Stroke::new(1.5, AMBER), StrokeKind::Inside);
        }
        let centre = egui::pos2(rect.left() + padding.x + side * 0.5, rect.center().y);
        chrome::paint_quality_shape(
            painter,
            chip.best,
            centre,
            side,
            chrome::quality_colour(chip.best, settings.palette),
        );
        painter.galley(
            egui::pos2(
                rect.left() + padding.x + side + gap,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            TEXT,
        );
    }
    response
}

/// The collapsed "Auto-sell" section under "Manage equipment".
pub(super) fn panel(ui: &mut egui::Ui, f: &mut UiCtx) {
    egui::CollapsingHeader::new(HEADER)
        .id_salt("auto_sell_panel")
        .show(ui, |ui| body(ui, f));
}

fn body(ui: &mut egui::Ui, f: &mut UiCtx) {
    let model = panel_model(f.game);
    let mut rule = model.rule;
    ui.checkbox(&mut rule.enabled, ENABLED)
        .on_hover_text(ENABLED_HOVER);
    ui.horizontal_wrapped(|ui| {
        for chip in &CHIPS {
            if threshold_chip(ui, chip, rule.sell_below == chip.sell_below, f.settings).clicked() {
                rule.sell_below = chip.sell_below;
            }
        }
    });
    if let Some(warning) = &model.warning {
        ui.colored_label(AMBER, warning);
    }
    ui.horizontal_wrapped(|ui| {
        for slot in Slot::ALL {
            ui.checkbox(&mut rule.slots[slot.index()], slot.label());
        }
    });
    ui.checkbox(&mut rule.keep_upgrades, KEEP_UPGRADES);
    ui.checkbox(&mut rule.apply_to_npc_runs, NPC_RUNS);
    if rule != model.rule {
        // The switch itself is the feedback: no status text follows.
        f.dispatch(Command::SetAutoSell(rule), "");
    }
    let line = ui.label(&model.would_sell);
    if !model.would_sell_detail.is_empty() {
        line.on_hover_ui(|ui| {
            for detail in &model.would_sell_detail {
                ui.label(detail);
            }
        });
    }
    if let Some(last_run) = &model.last_run {
        muted(ui, last_run);
    }
    if let Some(hint) = &model.hint {
        ui.colored_label(AMBER, hint);
    }
    let apply = ui
        .add_enabled(model.apply_blocked.is_none(), egui::Button::new(APPLY))
        .on_hover_text(APPLY_HOVER)
        .on_disabled_hover_text(model.apply_blocked.clone().unwrap_or_default());
    if apply.clicked() {
        f.status.modal = Some(AppModal::auto_sell(model.now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chips are exactly the thresholds core accepts, in order, each
    /// showing the quality just under its threshold.
    #[test]
    fn chips_cover_every_threshold_core_accepts() {
        let accepted: Vec<Quality> = Quality::ALL
            .into_iter()
            .filter(|quality| {
                AutoSellRule {
                    sell_below: *quality,
                    ..AutoSellRule::default()
                }
                .validate()
                .is_ok()
            })
            .collect();
        let chips: Vec<Quality> = CHIPS.iter().map(|chip| chip.sell_below).collect();
        assert_eq!(chips, accepted);
        for chip in &CHIPS {
            let below: Vec<Quality> = Quality::ALL
                .into_iter()
                .filter(|quality| *quality < chip.sell_below)
                .collect();
            assert_eq!(below.last(), Some(&chip.best), "{}", chip.label);
            assert!(chip.label.contains(chip.best.name()), "{}", chip.label);
        }
        let default = AutoSellRule::default().sell_below;
        for chip in &CHIPS {
            assert_eq!(
                chip.label.ends_with("(default)"),
                chip.sell_below == default,
                "{}",
                chip.label
            );
        }
    }

    #[test]
    fn warning_names_the_rare_and_epic_finds_a_threshold_sells() {
        for quiet in [Quality::Common, Quality::Uncommon, Quality::Rare] {
            assert_eq!(threshold_warning(quiet), None, "{quiet:?}");
        }
        assert_eq!(
            threshold_warning(Quality::Epic).as_deref(),
            Some("Also sells Rare finds.")
        );
        assert_eq!(
            threshold_warning(Quality::Legendary).as_deref(),
            Some("Also sells Rare and Epic finds.")
        );
    }

    #[test]
    fn a_fresh_journey_has_nothing_to_apply_and_no_last_run() {
        let game = Game::default();
        let model = panel_model(&game);
        assert_eq!(model.rule, AutoSellRule::default());
        assert_eq!(model.would_sell, "Would sell 0 now · +0");
        assert_eq!(model.last_run, None);
        assert_eq!(model.hint, None);
        assert_eq!(
            model.apply_blocked,
            Some(CommandError::NothingToSell.to_string())
        );
    }
}
