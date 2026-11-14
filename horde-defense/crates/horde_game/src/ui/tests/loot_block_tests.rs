//! Egui harness tests: the Results loot block and the no-sale-in-battle
//! guard (W3-20; design-loot-economy §9, design-ux-flow §4 "Results").
//! The block reads `Game::results_loot`: the drops by quality, the
//! settlement's auto-sold receipt and at most two "better than equipped"
//! rows. A row's "Equip" dispatches the offer's own command; a recruit's
//! row points to Operations. Sale status shows on Results only, never on
//! the battle screen or in the Operations observer window.
use super::defender_tests::{camp_with_recruits, takeover};
use super::home_tests::settle;
use super::*;
use horde_core::{
    campaign::AutoSellRule,
    defenders::DefenderId,
    quality::Quality,
    sim::{ResultsLoot, UpgradeOffer},
};

const AUTO_SELL_OFF: &str = "Auto-sell is off · Equipment › Auto-sell";

/// A new drop of `definition` at `quality` in the live main run, banked as
/// the drop site banks it: the stash item, `drop_ids`, `loot` and the
/// quality counts.
fn drop_item(f: &mut FrontendParts, definition: usize, quality: Quality) -> u64 {
    let id = f
        .game
        .campaign_mut()
        .add_item_with_quality(definition, 1, Some(quality))
        .unwrap();
    let a = f.game.attempt_mut().expect("a live run");
    a.drop_ids.push(id);
    a.loot += 1;
    a.loot_quality_counts[quality] += 1;
    id
}

/// An enabled rule that sells Poor and Common drops in every slot and keeps
/// no upgrades.
fn sell_cheap() -> AutoSellRule {
    AutoSellRule {
        enabled: true,
        keep_upgrades: false,
        ..AutoSellRule::default()
    }
}

/// Ends the live run with a withdrawal (settlement is the same for every
/// outcome) and shows its Results.
fn settle_run(f: &mut FrontendParts) {
    f.game.apply(&Command::Withdraw).unwrap();
    assert!(f.game.attempt().is_none());
    f.game.set_paused(true);
    f.screen = Screen::Results;
}

/// 2 Rare, 5 Uncommon, 6 Common and 1 Poor drop in the live run.
fn fourteen_drops(f: &mut FrontendParts) {
    for (count, quality) in [
        (2, Quality::Rare),
        (5, Quality::Uncommon),
        (6, Quality::Common),
        (1, Quality::Poor),
    ] {
        for n in 0..count {
            drop_item(f, 4 + n % 4, quality);
        }
    }
}

fn paints(output: &FullOutput, part: &str) -> bool {
    output.shapes.iter().any(|s| shape_has_text(&s.shape, part))
}

/// Every painted text run of the frame.
pub(super) fn painted_text(output: &FullOutput) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|c| collect(&c.shape, &mut out));
    out
}

/// The painted runs that talk about a sale.
fn sale_text(output: &FullOutput) -> Vec<String> {
    painted_text(output)
        .into_iter()
        .filter(|text| {
            let text = text.to_lowercase();
            text.contains("sold") || text.contains("sell") || text.contains("sale")
        })
        .collect()
}

fn loot_of(f: &FrontendParts) -> ResultsLoot {
    f.game.results_loot().expect("a reported run")
}

/// The row text of `offer` after the "Better than …:" lead and the item's
/// name: the slot and the rounded gain.
fn gain_text(offer: &UpgradeOffer) -> String {
    format!(
        "· {} +{}",
        offer.slot.label(),
        offer.gain.round().max(1.0) as u32
    )
}

fn item_name(f: &FrontendParts, id: u64) -> String {
    f.game
        .campaign()
        .item(id)
        .expect("an owned item")
        .definition()
        .name
        .clone()
}

#[test]
fn results_shows_drops_by_quality_and_auto_sold_line() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        fourteen_drops(f);
        f.game.apply(&Command::SetAutoSell(sell_cheap())).unwrap();
        settle_run(f);
        let loot = loot_of(f);
        assert_eq!((loot.total, loot.auto_sold.count), (14, 7));
        let sold = format!("Auto-sold 7 · +{}", loot.auto_sold.coins);
        let output = settle(&h, f);
        // "Loot 14 · 2 Rare · 5 Uncommon · 6 Common · 1 Poor", best first.
        let parts = ["Loot 14", "2 Rare", "5 Uncommon", "6 Common", "1 Poor"];
        for part in parts {
            assert_eq!(text_count(&output, part), 1, "{target:?}: {part}");
            assert!(fully_visible(&output, part), "{target:?}: {part}");
        }
        // Painted in that order (a wrapped row's labels share one origin,
        // so the paint order is the reading order).
        let painted = painted_text(&output);
        let order = parts.map(|part| painted.iter().position(|t| t == part).unwrap());
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        for absent in ["0 Epic", "0 Legendary"] {
            assert_eq!(text_count(&output, absent), 0, "{target:?}: {absent}");
        }
        assert_eq!(text_count(&output, &sold), 1, "{target:?}: {sold}");
        assert!(fully_visible(&output, &sold), "{target:?}");
        assert_eq!(text_count(&output, AUTO_SELL_OFF), 0, "{target:?}");

        // With the rule off the same drops are kept and the line says so.
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let f = &mut fixture.frontend;
        fourteen_drops(f);
        settle_run(f);
        assert!(loot_of(f).auto_sold.is_empty());
        let output = settle(&h, f);
        assert_eq!(text_count(&output, "Loot 14"), 1, "{target:?}");
        assert_eq!(text_count(&output, AUTO_SELL_OFF), 1, "{target:?}");
        assert_eq!(sale_text(&output), [AUTO_SELL_OFF], "{target:?}");

        // A run without drops has no loot block at all.
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let f = &mut fixture.frontend;
        f.game.apply(&Command::SetAutoSell(sell_cheap())).unwrap();
        settle_run(f);
        let output = settle(&h, f);
        assert_eq!(text_count(&output, "Loot 0"), 0, "{target:?}");
        assert_eq!(sale_text(&output), Vec::<String>::new(), "{target:?}");
    }
}

/// The player's run banks Epic finds of eight definitions; nothing more is
/// sold than the rule already set would sell.
fn player_run_with_upgrades(f: &mut FrontendParts) -> ResultsLoot {
    for definition in 4..12 {
        drop_item(f, definition, Quality::Epic);
    }
    settle_run(f);
    let loot = loot_of(f);
    assert!(
        loot.upgrades.len() >= 2,
        "the Epic finds beat the starter gear in two slots: {:?}",
        loot.upgrades
    );
    loot
}

#[test]
fn results_better_than_equipped_row_dispatches_equip_command() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        let loot = player_run_with_upgrades(f);
        let output = settle(&h, f);
        // The first two offers, largest gain first, one row each.
        assert_eq!(
            text_count(&output, "Better than equipped:"),
            2,
            "{target:?}"
        );
        assert_eq!(text_count(&output, "Equip"), 2, "{target:?}");
        for offer in &loot.upgrades[..2] {
            let gain = gain_text(offer);
            assert_eq!(text_count(&output, &gain), 1, "{target:?}: {gain}");
            let name = item_name(f, offer.item_id);
            assert!(paints(&output, &name), "{target:?}: {name}");
            assert_eq!(
                offer.command,
                Command::Equip {
                    mode: Mode::Personal,
                    id: offer.item_id
                }
            );
        }
        // "Equip" is inline content, never the screen's primary.
        let primaries = h.screen_primaries();
        assert_eq!(primaries.len(), 1, "{target:?}: {primaries:?}");
        assert_ne!(primaries[0], "Equip");

        // The first row's Equip applies that offer's command.
        let first = &loot.upgrades[0];
        let worn = item_name(f, first.item_id);
        assert_ne!(
            f.game.campaign().equipped(Mode::Personal, first.slot).name,
            worn
        );
        let coins = f.game.campaign().coins;
        h.click(f, View::Full, "Equip");
        assert_eq!(
            f.game.campaign().equipped(Mode::Personal, first.slot).name,
            worn,
            "{target:?}: {}",
            f.status.message
        );
        assert_eq!(f.game.campaign().coins, coins);
        assert!(f.screen == Screen::Results, "{target:?}: still on Results");
        let after = loot_of(f);
        assert!(
            after.upgrades.iter().all(|o| o.item_id != first.item_id),
            "{target:?}: the worn item is no longer offered"
        );
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(
            text_count(&output, "Better than equipped:"),
            after.upgrades.len().min(2),
            "{target:?}"
        );
    }
}

#[test]
fn recruit_upgrade_row_points_to_operations() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        let recruit = camp_with_recruits(f, &["FRA"])[0];
        takeover(f, recruit);
        for definition in 4..12 {
            drop_item(f, definition, Quality::Epic);
        }
        settle_run(f);
        let loot = loot_of(f);
        assert_eq!(
            f.game.last_result().unwrap().defender(),
            DefenderId::Recruit(recruit)
        );
        assert!(!loot.upgrades.is_empty(), "the recruit's run has upgrades");
        for offer in &loot.upgrades {
            assert!(matches!(offer.command, Command::EquipRecruit { .. }));
        }
        let name = f
            .game
            .defender_card(DefenderId::Recruit(recruit))
            .unwrap()
            .name;
        let lead = format!("Better than {name}'s gear:");
        let rows = loot.upgrades.len().min(2);
        let output = settle(&h, f);
        assert_eq!(text_count(&output, &lead), rows, "{target:?}: {lead}");
        assert_eq!(text_count(&output, "Operations ›"), rows, "{target:?}");
        assert!(fully_visible(&output, "Operations ›"), "{target:?}");
        // A recruit's gear is changed on Operations, not here.
        assert_eq!(text_count(&output, "Equip"), 0, "{target:?}");
        assert_eq!(text_count(&output, "Better than equipped:"), 0);
        for offer in &loot.upgrades[..rows] {
            let gain = gain_text(offer);
            assert_eq!(text_count(&output, &gain), 1, "{target:?}: {gain}");
        }

        let before = serde_json::to_value(&f.game).unwrap();
        h.click(f, View::Full, "Operations ›");
        assert!(f.screen == Screen::Camp, "{target:?}");
        assert!(f.nav.tab == Tab::Operations, "{target:?}");
        assert_eq!(
            serde_json::to_value(&f.game).unwrap(),
            before,
            "{target:?}: the row equips nothing"
        );
    }
}

/// The on-screen part of every painted run reading exactly `label`.
fn bounds_of(output: &FullOutput, label: &str) -> Vec<(Rect, Rect)> {
    output
        .shapes
        .iter()
        .filter_map(|c| labeled_bounds(&c.shape, label).map(|r| (r, c.clip_rect)))
        .collect()
}

#[test]
fn results_loot_block_fits_1050x700_at_1_35() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        // The tallest block: six qualities, a receipt and two upgrade rows.
        drop_item(f, 12, Quality::Legendary);
        drop_item(f, 13, Quality::Rare);
        drop_item(f, 14, Quality::Uncommon);
        for n in 0..3 {
            drop_item(f, 4 + n, Quality::Common);
            drop_item(f, 8 + n, Quality::Poor);
        }
        f.game.apply(&Command::SetAutoSell(sell_cheap())).unwrap();
        let loot = player_run_with_upgrades(f);
        assert_eq!(loot.auto_sold.count, 6);
        let sold = format!("Auto-sold 6 · +{}", loot.auto_sold.coins);
        let output = settle(&h, f);
        let screen = h.ctx.content_rect();
        let mut lines = vec![
            "Loot 17".to_owned(),
            "1 Legendary".into(),
            "8 Epic".into(),
            "1 Rare".into(),
            "1 Uncommon".into(),
            "3 Common".into(),
            "3 Poor".into(),
            sold,
        ];
        lines.extend(loot.upgrades[..2].iter().map(gain_text));
        // Every row's lead and control, and the actions the block sits
        // beside, stay on screen too.
        lines.extend(
            [
                "Better than equipped:",
                "Equip",
                "Try again",
                "Leave journey",
            ]
            .map(String::from),
        );
        let mut bottom = 0.0_f32;
        for line in &lines {
            let rects = bounds_of(&output, line);
            assert!(!rects.is_empty(), "{target:?}: {line} not painted");
            for (rect, clip) in rects {
                assert!(clip.contains_rect(rect), "{target:?}: {line} is clipped");
                assert!(
                    screen.contains_rect(rect),
                    "{target:?}: {line} at {rect:?} leaves {screen:?}"
                );
                bottom = bottom.max(rect.bottom());
            }
        }
        // The block is content above the tree: the tree keeps at least its
        // 150 pt minimum on screen.
        assert!(
            bottom < screen.bottom() - 150.0,
            "{target:?}: the block ends at {bottom} of {}",
            screen.bottom()
        );
    }
}

#[test]
fn battle_and_observer_never_render_sale_text() {
    for target in Target::BOTH {
        // The main battle: an enabled rule, drops that settlement will sell.
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        f.game.apply(&Command::SetAutoSell(sell_cheap())).unwrap();
        fourteen_drops(f);
        f.game.set_paused(true);
        for view in [View::Full, View::Battle] {
            h.frame(f, view, vec![]);
            let output = h.frame(f, view, vec![]);
            assert!(paints(&output, "HP "), "{target:?}: the battle is drawn");
            assert_eq!(
                sale_text(&output),
                Vec::<String>::new(),
                "{target:?}: sale text on the battle screen"
            );
        }
        // Contrast: the same run's Results does name the sale, so the
        // detector sees sale text where it exists.
        settle_run(f);
        let output = settle(&h, f);
        assert!(
            sale_text(&output)
                .iter()
                .any(|t| t.starts_with("Auto-sold 7")),
            "{target:?}: {:?}",
            sale_text(&output)
        );

        // The observer window: an expedition with a queued settlement sale.
        let mut fixture = IsolatedFrontend::new();
        let f = &mut fixture.frontend;
        let recruit = camp_with_recruits(f, &["FRA", "POL"])[0];
        f.game.apply(&Command::SetAutoSell(sell_cheap())).unwrap();
        let battle = f.game.deploy_survivor(recruit, "POL", 0).unwrap();
        let queued = f
            .game
            .campaign_mut()
            .add_item_with_quality(5, 1, Some(Quality::Poor))
            .unwrap();
        f.game
            .battles_mut()
            .iter_mut()
            .find(|b| b.id == battle)
            .unwrap()
            .pending_auto_sell
            .push(queued);
        f.game.set_paused(true);
        f.screen = Screen::Camp;
        f.nav.tab = Tab::Operations;
        f.campaign.observed = Some(battle);
        h.frame(f, View::Full, vec![]);
        let output = h.frame(f, View::Full, vec![]);
        assert!(
            paints(&output, "Save country plan"),
            "{target:?}: the observer window is open"
        );
        assert_eq!(
            sale_text(&output),
            Vec::<String>::new(),
            "{target:?}: sale text on Operations or in the observer"
        );
    }
}
