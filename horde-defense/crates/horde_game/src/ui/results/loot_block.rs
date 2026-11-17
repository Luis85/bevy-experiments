//! The Results loot block (W3-20; design-loot-economy §9, design-ux-flow §4
//! "Results"): what the reported run dropped by quality, what settlement
//! auto-sold and at most [`UPGRADE_ROWS`] "better than equipped" rows. It
//! renders `Game::results_loot` and nothing else: the totals, the receipt,
//! the offers and each offer's equip command are core's. Inline content,
//! never one of the screen's actions; static, so reduced effects needs no
//! second form.
use crate::ui::{Tab, UiCtx, chrome, coins::grouped, muted};
use bevy_egui::egui::{self, RichText};
use horde_core::{
    campaign::AutoSoldReceipt,
    command::Command,
    content::Slot,
    defenders::DefenderId,
    quality::Quality,
    sim::{Game, ResultsLoot},
};

/// How many upgrade offers the block shows (core lists at most one a slot,
/// largest gain first).
const UPGRADE_ROWS: usize = 2;
/// The muted line of a run that sold nothing while the rule is off.
const AUTO_SELL_OFF: &str = "Auto-sell is off · Equipment › Auto-sell";

/// The auto-sell line under the drop counts.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SoldLine {
    /// "Auto-sold N · +C"; `lines` are the receipt's lines, shown on hover.
    Sold { text: String, lines: Vec<String> },
    /// The rule is on and settlement sold nothing.
    Nothing(String),
    /// The rule is off.
    Off,
}

/// What an upgrade row's control does.
#[derive(Clone, Debug, PartialEq)]
enum RowAction {
    /// "Equip": the offer's own command, for the player or Mara.
    Equip(Command),
    /// "Operations ›": a recruit's gear is changed there.
    Operations,
}

#[derive(Clone, Debug, PartialEq)]
struct UpgradeRow {
    /// "Better than equipped:" or "Better than <Name>'s gear:".
    lead: String,
    item: String,
    quality: Option<Quality>,
    /// "· Ranged +18".
    gain: String,
    action: RowAction,
}

#[derive(Clone, Debug, PartialEq)]
struct LootBlock {
    /// "Loot 14".
    total: String,
    /// The qualities that dropped, best first, with their counts.
    qualities: Vec<(Quality, u32)>,
    sold: SoldLine,
    upgrades: Vec<UpgradeRow>,
}

/// "· <Slot> +<gain>": the slot-score gain rounded, never shown as +0 (an
/// offer always clears core's upgrade margin).
fn gain_text(slot: Slot, gain: f32) -> String {
    format!("· {} +{}", slot.label(), gain.round().max(1.0) as u32)
}

/// One hover line per receipt line, then "and N more" for the sales the
/// bounded receipt does not list.
fn sold_lines(receipt: &AutoSoldReceipt) -> Vec<String> {
    let mut lines: Vec<String> = receipt
        .lines
        .iter()
        .map(|line| {
            format!(
                "{} · {} · Lv{} · +{}",
                line.definition
                    .definition()
                    .map_or("Unknown item", |d| d.name.as_str()),
                line.quality.name(),
                line.level,
                line.coins
            )
        })
        .collect();
    let more = (receipt.count as usize).saturating_sub(receipt.lines.len());
    if more > 0 {
        lines.push(format!("and {more} more"));
    }
    lines
}

fn sold_line(receipt: &AutoSoldReceipt, rule_enabled: bool) -> SoldLine {
    // Drops kept because the wallet was full are part of the receipt.
    let kept = if receipt.kept_on_overflow > 0 {
        format!(" · {} kept, wallet full", receipt.kept_on_overflow)
    } else {
        String::new()
    };
    let text = format!(
        "Auto-sold {} · +{}{kept}",
        receipt.count,
        grouped(receipt.coins)
    );
    if receipt.count > 0 {
        SoldLine::Sold {
            text,
            lines: sold_lines(receipt),
        }
    } else if rule_enabled || receipt.kept_on_overflow > 0 {
        SoldLine::Nothing(text)
    } else {
        SoldLine::Off
    }
}

fn upgrade_rows(game: &Game, loot: &ResultsLoot, defender: DefenderId) -> Vec<UpgradeRow> {
    let lead = match defender {
        DefenderId::Player => "Better than equipped:".to_owned(),
        other => format!(
            "Better than {}'s gear:",
            game.defender_card(other)
                .map_or_else(|| "the defender".to_owned(), |card| card.name)
        ),
    };
    loot.upgrades
        .iter()
        .take(UPGRADE_ROWS)
        .filter_map(|offer| {
            let item = game.campaign().item(offer.item_id)?;
            Some(UpgradeRow {
                lead: lead.clone(),
                item: item.definition().name.clone(),
                quality: item.quality,
                gain: gain_text(offer.slot, offer.gain),
                action: match defender {
                    DefenderId::Recruit(_) => RowAction::Operations,
                    DefenderId::Player | DefenderId::Mara => {
                        RowAction::Equip(offer.command.clone())
                    }
                },
            })
        })
        .collect()
}

/// The block of the reported run, or None when there is no report or it
/// banked no drops.
fn model(game: &Game) -> Option<LootBlock> {
    let loot = game.results_loot()?;
    if loot.total == 0 {
        return None;
    }
    let defender = game.last_result()?.defender();
    Some(LootBlock {
        total: format!("Loot {}", loot.total),
        qualities: Quality::ALL
            .into_iter()
            .rev()
            .map(|quality| (quality, loot.by_quality[quality]))
            .filter(|(_, count)| *count > 0)
            .collect(),
        sold: sold_line(&loot.auto_sold, game.campaign().auto_sell.enabled),
        upgrades: upgrade_rows(game, &loot, defender),
    })
}

/// Draws the block: the drop counts, the auto-sell line, then the rows.
pub(super) fn loot_block(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(block) = model(f.game) else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(&block.total).strong());
        for (quality, count) in &block.qualities {
            muted(ui, "·");
            chrome::quality_shape_icon(ui, *quality, f.settings);
            ui.colored_label(
                chrome::quality_colour(*quality, f.settings.palette),
                format!("{count} {}", quality.name()),
            );
        }
    });
    match &block.sold {
        SoldLine::Sold { text, lines } => {
            ui.label(text).on_hover_ui(|ui| {
                for line in lines {
                    ui.label(line);
                }
            });
        }
        SoldLine::Nothing(text) => muted(ui, text),
        SoldLine::Off => muted(ui, AUTO_SELL_OFF),
    }
    for row in block.upgrades {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // The row is as tall as its framed button, so the labels share
            // the button's centre line instead of its upper half (CF-3-19).
            let button = ui.text_style_height(&egui::TextStyle::Button)
                + 2.0 * ui.spacing().button_padding.y;
            ui.set_row_height(button.max(ui.spacing().interact_size.y));
            ui.label(&row.lead);
            match row.quality {
                Some(quality) => {
                    chrome::with_quality_shape(ui, quality, f.settings, |ui| {
                        ui.label(chrome::quality_item_text(&row.item, quality, f.settings));
                    });
                }
                None => {
                    ui.label(&row.item);
                }
            }
            ui.label(&row.gain);
            match row.action {
                RowAction::Equip(command) => {
                    if ui
                        .button("Equip")
                        .on_hover_text("Wear this find now. The replaced item stays in your stash.")
                        .clicked()
                    {
                        f.dispatch(command, &format!("Equipped {}.", row.item));
                    }
                }
                RowAction::Operations => {
                    if ui
                        .button("Operations ›")
                        .on_hover_text("A recruit's equipment is changed on Operations.")
                        .clicked()
                    {
                        f.open_preparation(Tab::Operations);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::{campaign::AutoSoldLine, content::content, kind_map::KindMap};

    fn receipt(count: u32, listed: usize) -> AutoSoldReceipt {
        let definition = content().items[5].id.clone();
        let mut by_quality = KindMap::<Quality, u32>::default();
        by_quality[Quality::Common] = count;
        AutoSoldReceipt {
            count,
            coins: u64::from(count) * 12,
            by_quality,
            lines: vec![
                AutoSoldLine {
                    definition,
                    level: 3,
                    quality: Quality::Common,
                    coins: 12,
                };
                listed
            ],
            kept_on_overflow: 0,
        }
    }

    #[test]
    fn sold_line_names_the_receipt_and_the_sales_it_does_not_list() {
        let name = &content().items[5].name;
        let SoldLine::Sold { text, lines } = sold_line(&receipt(50, 12), true) else {
            panic!("a receipt with sales is a Sold line");
        };
        assert_eq!(text, "Auto-sold 50 · +600");
        assert_eq!(lines.len(), 13);
        assert_eq!(lines[0], format!("{name} · Common · Lv3 · +12"));
        assert_eq!(lines[12], "and 38 more");
        // Every sale listed: no "more" line. The line shows whatever the
        // rule is now; the receipt is what happened.
        let SoldLine::Sold { lines, .. } = sold_line(&receipt(2, 2), false) else {
            panic!("a receipt with sales is a Sold line");
        };
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn empty_receipt_reads_off_or_nothing_sold() {
        let empty = AutoSoldReceipt::default();
        assert_eq!(sold_line(&empty, false), SoldLine::Off);
        assert_eq!(
            sold_line(&empty, true),
            SoldLine::Nothing("Auto-sold 0 · +0".into())
        );
        // Drops the full wallet kept are named even with the rule now off.
        let kept = AutoSoldReceipt {
            kept_on_overflow: 2,
            ..AutoSoldReceipt::default()
        };
        assert_eq!(
            sold_line(&kept, false),
            SoldLine::Nothing("Auto-sold 0 · +0 · 2 kept, wallet full".into())
        );
    }

    #[test]
    fn gain_text_rounds_and_never_reads_plus_zero() {
        assert_eq!(gain_text(Slot::Ranged, 17.6), "· Ranged +18");
        assert_eq!(gain_text(Slot::Outfit, 1.2), "· Outfit +1");
        assert_eq!(gain_text(Slot::Melee, 0.3), "· Melee +1");
    }
}
