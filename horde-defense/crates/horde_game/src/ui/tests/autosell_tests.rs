//! Egui harness tests: the Equipment Auto-sell panel, its confirmation
//! modal and the blocked-sale shortcuts (W3-21; design-loot-economy §9,
//! design-ux-flow §4 "Auto-sell panel"). The panel renders
//! `Game::auto_sell_preview` and every control dispatches
//! `Command::SetAutoSell`; "Apply rule to stash now…" sells nothing until
//! the modal's amber primary confirms the quoted batch; a blocked sale
//! shows `SaleBlock::label()` with the shortcut `Game::sale_resolution`
//! names.
use super::defender_tests::camp_with_recruits;
use super::loot_block_tests::{drop_item, painted_text, settle_run};
use super::settings_tests::painted_shapes;
use super::*;
use crate::settings::{Palette, QualityMarkers};
use crate::ui::toast::ToastKind;
use horde_core::{
    campaign::{AutoSellRule, SaleBlock},
    quality::Quality,
    sim::{AutoSellPreview, AutoSellScope},
};

/// A frame tall enough for the narrow layout to show the whole page.
const TALL: Vec2 = vec2(900.0, 2400.0);
const HEADER: &str = "Auto-sell";
const APPLY: &str = "Apply rule to stash now…";
const KEEP_ITEMS: &str = "Keep items";
const KEPT_NOTE: &str = "Locked, favourite, equipped, preset, recruit and shop items are kept.";
const REQUOTED: &str = "The stash changed. This is the new quote.";
/// The five threshold chips: label, the rule's `sell_below`, the best
/// quality it sells and the painted kind of that quality's shape.
const CHIPS: [(&str, Quality, Quality, &str); 5] = [
    ("Poor only", Quality::Common, Quality::Poor, "outline3"),
    (
        "Common and below (default)",
        Quality::Uncommon,
        Quality::Common,
        "disc",
    ),
    (
        "Uncommon and below",
        Quality::Rare,
        Quality::Uncommon,
        "fill4",
    ),
    ("Rare and below", Quality::Epic, Quality::Rare, "rect"),
    ("Epic and below", Quality::Legendary, Quality::Epic, "fill3"),
];

/// A level-1 find of `definition` at `quality` in the stash.
fn stash(f: &mut FrontendParts, definition: usize, quality: Quality) -> u64 {
    f.game
        .campaign_mut()
        .add_item_with_quality(definition, 1, Some(quality))
        .unwrap()
}

/// The stored rule with upgrades not kept, so only the threshold, the slots
/// and the sale guards decide.
fn plain_rule(f: &mut FrontendParts) -> AutoSellRule {
    let rule = AutoSellRule {
        keep_upgrades: false,
        ..f.game.campaign().auto_sell
    };
    f.game.apply(&Command::SetAutoSell(rule)).unwrap();
    rule
}

fn preview(f: &FrontendParts, scope: AutoSellScope) -> AutoSellPreview {
    f.game
        .auto_sell_preview(&f.game.campaign().auto_sell, scope)
}

/// Opens the panel's header and lets it finish opening.
fn open_panel(h: &Harness, f: &mut FrontendParts, view: View) {
    h.click_size(f, view, HEADER, TALL);
    for _ in 0..12 {
        h.frame_size(f, view, vec![], TALL);
    }
}

/// A journey on the Equipment tab of the whole App frame, where the modal
/// layer and the Escape router run.
fn equipment_frontend() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.act(|c| c.open_preparation(Tab::Armory));
    fixture
}

fn owned(f: &FrontendParts, ids: &[u64]) -> usize {
    ids.iter()
        .filter(|id| f.game.campaign().item(**id).is_some())
        .count()
}

fn modal_title(quote: &AutoSellPreview) -> String {
    format!("Sell {} items for {} coins?", quote.count, quote.coins)
}

fn sell_label(quote: &AutoSellPreview) -> String {
    format!("Sell {} · +{}", quote.count, quote.coins)
}

#[test]
fn equipment_autosell_panel_preview_counts_match_core() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    // One older cheap find, then a run that drops three cheap items and a
    // Rare one while the rule is off: the run "would have sold" three.
    stash(f, 6, Quality::Poor);
    drop_item(f, 4, Quality::Poor);
    drop_item(f, 5, Quality::Poor);
    drop_item(f, 6, Quality::Common);
    drop_item(f, 7, Quality::Rare);
    plain_rule(f);
    settle_run(f);
    f.screen = Screen::Camp;
    assert!(!f.game.campaign().auto_sell.enabled);
    open_panel(&h, f, View::Armory);
    let lines = |f: &FrontendParts| {
        let (now, last) = (
            preview(f, AutoSellScope::Stash),
            preview(f, AutoSellScope::LastRun),
        );
        (
            (now.count, last.count),
            format!("Would sell {} now · +{}", now.count, now.coins),
            format!("Last run would have sold {} · +{}", last.count, last.coins),
        )
    };
    let (counts, now, last) = lines(f);
    assert_eq!(counts, (4, 3));
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert_eq!(text_count(&output, &now), 1, "{now}");
    assert_eq!(text_count(&output, &last), 1, "{last}");

    // A chip dispatches SetAutoSell and both lines follow core.
    h.click_size(f, View::Armory, "Poor only", TALL);
    assert_eq!(f.game.campaign().auto_sell.sell_below, Quality::Common);
    let (counts, now, last) = lines(f);
    assert_eq!(counts, (3, 2));
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert_eq!(text_count(&output, &now), 1, "{now}");
    assert_eq!(text_count(&output, &last), 1, "{last}");

    // So does a slot checkbox.
    h.click_size(f, View::Armory, Slot::Ranged.label(), TALL);
    assert_eq!(f.game.campaign().auto_sell.slots, [false, true, true, true]);
    let (counts, now, last) = lines(f);
    assert_eq!(counts, (2, 1));
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert_eq!(text_count(&output, &now), 1, "{now}");
    assert_eq!(text_count(&output, &last), 1, "{last}");

    // The three switches are the rule's own fields.
    h.click_size(f, View::Armory, "Auto-sell new drops", TALL);
    h.click_size(
        f,
        View::Armory,
        "Keep upgrades for the defender who ran it",
        TALL,
    );
    h.click_size(f, View::Armory, "Also on expeditions and operations", TALL);
    assert_eq!(
        f.game.campaign().auto_sell,
        AutoSellRule {
            enabled: true,
            sell_below: Quality::Common,
            slots: [false, true, true, true],
            keep_upgrades: true,
            apply_to_npc_runs: false,
        }
    );
    // Nothing was sold by any of it.
    assert_eq!(f.game.campaign().coins, 0);
}

#[test]
fn autosell_hint_names_the_threshold_when_recent_drops_are_too_good() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    drop_item(f, 5, Quality::Uncommon);
    drop_item(f, 6, Quality::Rare);
    plain_rule(f);
    settle_run(f);
    f.screen = Screen::Camp;
    open_panel(&h, f, View::Armory);
    assert_eq!(preview(f, AutoSellScope::LastRun).count, 0);
    let hint = "Your recent drops are Uncommon or better; raise the threshold to keep selling";
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert_eq!(text_count(&output, hint), 1);
    // Raising it sells the Uncommon drop, and the hint goes.
    h.click_size(f, View::Armory, "Uncommon and below", TALL);
    assert_eq!(preview(f, AutoSellScope::LastRun).count, 1);
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert!(
        painted_text(&output)
            .iter()
            .all(|text| !text.contains("raise the threshold")),
    );
}

#[test]
fn apply_to_stash_requires_confirmation_and_escape_closes() {
    let mut fixture = equipment_frontend();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let ids = [
        stash(f, 5, Quality::Poor),
        stash(f, 6, Quality::Poor),
        stash(f, 7, Quality::Common),
    ];
    plain_rule(f);
    let quote = preview(f, AutoSellScope::Stash);
    assert_eq!(quote.count, 3);
    let coins = f.game.campaign().coins;
    open_panel(&h, f, View::Full);

    // The button only opens the confirmation.
    h.click_size(f, View::Full, APPLY, TALL);
    assert!(f.status.modal.is_some(), "the confirmation opens");
    assert_eq!((owned(f, &ids), f.game.campaign().coins), (3, coins));
    let output = h.frame_size(f, View::Full, vec![], TALL);
    assert_eq!(text_count(&output, &modal_title(&quote)), 1);
    assert_eq!(text_count(&output, KEPT_NOTE), 1);
    let modal_primaries: Vec<String> = h
        .primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect();
    assert_eq!(modal_primaries, vec![sell_label(&quote)]);

    // Escape closes it and nothing else: no sale, still on Equipment.
    h.key(f, View::Full, Key::Escape);
    assert!(f.status.modal.is_none(), "Escape closes the confirmation");
    assert_eq!((owned(f, &ids), f.game.campaign().coins), (3, coins));
    assert_eq!((f.screen, f.nav.tab), (Screen::Camp, Tab::Armory));

    // So does "Keep items".
    h.click_size(f, View::Full, APPLY, TALL);
    assert!(f.status.modal.is_some());
    h.click_topmost(f, View::Full, KEEP_ITEMS);
    assert!(f.status.modal.is_none());
    assert_eq!((owned(f, &ids), f.game.campaign().coins), (3, coins));

    // The amber primary sells exactly the quoted batch.
    h.click_size(f, View::Full, APPLY, TALL);
    h.click_topmost(f, View::Full, &sell_label(&quote));
    assert_eq!(owned(f, &ids), 0);
    assert_eq!(f.game.campaign().coins, coins + quote.coins);
    assert!(f.status.modal.is_none(), "the sale closes the confirmation");
    let toast = f.status.toasts.newest(1).next().expect("a toast");
    assert_eq!(
        (toast.text.as_str(), toast.kind),
        (
            format!("Sold {} · +{}", quote.count, quote.coins).as_str(),
            ToastKind::Success
        )
    );
}

#[test]
fn stale_quote_refreshes_modal() {
    let mut fixture = equipment_frontend();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let mut ids = vec![stash(f, 5, Quality::Poor), stash(f, 6, Quality::Poor)];
    plain_rule(f);
    let quote = preview(f, AutoSellScope::Stash);
    assert_eq!(quote.count, 2);
    let coins = f.game.campaign().coins;
    open_panel(&h, f, View::Full);
    h.click_size(f, View::Full, APPLY, TALL);
    assert!(f.status.modal.is_some());

    // The stash changes under the open modal (an expedition banks a find).
    ids.push(stash(f, 7, Quality::Poor));
    let fresh = preview(f, AutoSellScope::Stash);
    assert_eq!(fresh.count, 3);
    assert_ne!(fresh.coins, quote.coins);

    // The stale confirmation sells nothing and the modal re-quotes.
    h.click_topmost(f, View::Full, &sell_label(&quote));
    assert_eq!((owned(f, &ids), f.game.campaign().coins), (3, coins));
    assert!(f.status.modal.is_some(), "the modal stays open");
    let output = h.frame_size(f, View::Full, vec![], TALL);
    assert_eq!(text_count(&output, &modal_title(&fresh)), 1);
    assert_eq!(text_count(&output, &modal_title(&quote)), 0);
    assert_eq!(text_count(&output, REQUOTED), 1);

    // Confirming the new quote sells it.
    h.click_topmost(f, View::Full, &sell_label(&fresh));
    assert_eq!(owned(f, &ids), 0);
    assert_eq!(f.game.campaign().coins, coins + fresh.coins);
    assert!(f.status.modal.is_none());
}

#[test]
fn threshold_chips_show_text_and_shape_markers() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    open_panel(&h, f, View::Armory);
    // Every chip names its range and carries its best quality's shape in
    // the palette's colour, whatever the marker setting: the chips are the
    // control, so they never fall back to colour alone.
    for palette in [Palette::Standard, Palette::ColorBlind] {
        for markers in [QualityMarkers::ColorOnly, QualityMarkers::Shape] {
            f.settings.palette = palette;
            f.settings.quality_markers = markers;
            let output = h.frame_size(f, View::Armory, vec![], TALL);
            let shapes = painted_shapes(&output);
            for (label, _, best, kind) in CHIPS {
                assert_eq!(
                    text_count(&output, label),
                    1,
                    "{palette:?} {markers:?}: {label}"
                );
                let colour = crate::ui::chrome::quality_colour(best, palette);
                assert!(
                    shapes.contains(&(kind.to_owned(), colour)),
                    "{palette:?} {markers:?}: {label} has no {kind} in {colour:?}"
                );
            }
        }
    }
    // A chip sets the threshold; one that sells Rare or Epic finds warns in
    // amber and in words.
    assert_eq!(f.game.campaign().auto_sell.sell_below, Quality::Uncommon);
    let warns = |output: &FullOutput| {
        painted_text(output)
            .into_iter()
            .filter(|text| text.starts_with("Also sells"))
            .collect::<Vec<_>>()
    };
    let output = h.frame_size(f, View::Armory, vec![], TALL);
    assert_eq!(warns(&output), Vec::<String>::new());
    for (label, sell_below, _, _) in CHIPS {
        h.click_size(f, View::Armory, label, TALL);
        assert_eq!(
            f.game.campaign().auto_sell.sell_below,
            sell_below,
            "{label}"
        );
        let output = h.frame_size(f, View::Armory, vec![], TALL);
        let warning = match sell_below {
            Quality::Epic => Some("Also sells Rare finds."),
            Quality::Legendary => Some("Also sells Rare and Epic finds."),
            _ => None,
        };
        assert_eq!(
            warns(&output),
            warning.map(str::to_owned).into_iter().collect::<Vec<_>>(),
            "{label}"
        );
        if let Some(warning) = warning {
            let colour = output
                .shapes
                .iter()
                .find_map(|clipped| text_color(&clipped.shape, warning));
            assert_eq!(colour, Some(art::AMBER), "{label}");
        }
    }
}

fn blocked_line(block: &SaleBlock) -> String {
    format!("Sale blocked: {}", block.label())
}

#[test]
fn blocked_sale_shows_typed_reason_and_shortcut() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let starter = f.game.campaign().player.loadout;
    let id = f.game.campaign_mut().add_item(6).unwrap();
    let slot = content().items[6].slot;
    f.game.campaign_mut().set_lock(id, true).unwrap();
    f.game.campaign_mut().set_favorite(id, true).unwrap();
    f.game.campaign_mut().equip(Mode::Personal, id).unwrap();
    f.armory.selected_item = Some(id);
    // Each guard in core's order: its reason is on screen (not only on the
    // disabled button's hover) and its shortcut lifts exactly that guard.
    let equip_starter = format!("Equip starter {}", slot.label());
    for (block, shortcut) in [
        (SaleBlock::Locked, "Unlock item"),
        (SaleBlock::Favorite, "Remove favorite mark"),
        (SaleBlock::EquippedPersonal, equip_starter.as_str()),
    ] {
        assert_eq!(f.game.sale_eligibility(id), Err(block.clone()));
        let output = h.frame(f, View::Armory, vec![]);
        assert_eq!(text_count(&output, &blocked_line(&block)), 1, "{block:?}");
        h.click(f, View::Armory, shortcut);
    }
    assert!(f.game.sale_eligibility(id).is_ok());
    let item = f.game.campaign().item(id).unwrap();
    assert!(!item.locked && !item.favorite);
    assert_eq!(f.game.campaign().player.loadout, starter);
    let blocked = |output: &FullOutput| {
        painted_text(output)
            .into_iter()
            .filter(|text| text.starts_with("Sale blocked"))
            .collect::<Vec<_>>()
    };
    let output = h.frame(f, View::Armory, vec![]);
    assert_eq!(blocked(&output), Vec::<String>::new());

    // A protected starter has a reason and no way out.
    f.armory.selected_item = Some(starter[slot.index()]);
    let output = h.frame(f, View::Armory, vec![]);
    assert_eq!(blocked(&output), vec![blocked_line(&SaleBlock::Protected)]);
    let before = serde_json::to_value(&f.game).unwrap();
    for shortcut in [
        "Unlock item",
        "Remove favorite mark",
        equip_starter.as_str(),
    ] {
        assert_eq!(text_count(&output, shortcut), 0, "{shortcut}");
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn recruit_gear_block_points_to_operations() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    let rifle = f.game.campaign_mut().add_item(4).unwrap();
    f.game
        .apply(&Command::EquipRecruit {
            id: recruit,
            item: rifle,
        })
        .unwrap();
    let block = f.game.sale_eligibility(rifle).unwrap_err();
    assert!(matches!(block, SaleBlock::RecruitEquipped { .. }));
    f.act(|c| c.open_preparation(Tab::Armory));
    f.armory.selected_item = Some(rifle);
    let output = h.frame(f, View::Armory, vec![]);
    assert_eq!(text_count(&output, &blocked_line(&block)), 1);
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Armory, "Operations ›");
    assert_eq!(f.nav.tab, Tab::Operations);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn delete_preset_shortcut_dispatches_command() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let id = f.game.campaign_mut().add_item(6).unwrap();
    let starter = f.game.campaign().player.loadout;
    f.game.campaign_mut().equip(Mode::Personal, id).unwrap();
    f.game
        .campaign_mut()
        .save_preset("Patrol vest".into(), Mode::Personal)
        .unwrap();
    f.game
        .campaign_mut()
        .equip(Mode::Personal, starter[content().items[6].slot.index()])
        .unwrap();
    f.armory.selected_item = Some(id);
    let block = f.game.sale_eligibility(id).unwrap_err();
    assert_eq!(
        block,
        SaleBlock::Preset {
            names: vec!["Patrol vest".into()]
        }
    );
    let others: Vec<_> = f
        .game
        .campaign()
        .presets
        .iter()
        .filter(|p| p.name != "Patrol vest")
        .cloned()
        .collect();
    let output = h.frame(f, View::Armory, vec![]);
    assert_eq!(text_count(&output, &blocked_line(&block)), 1);
    // The shortcut is `Command::DeletePreset` on that preset: it goes, the
    // other presets and the item stay, and the sale opens.
    h.click(f, View::Armory, "Delete preset Patrol vest");
    assert_eq!(f.game.campaign().presets, others);
    assert!(f.game.campaign().item(id).is_some());
    assert!(f.game.sale_eligibility(id).is_ok());
    let price = content().items[6].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    assert_eq!(f.armory.pending_sale, Some(id));
}

#[test]
fn loot_only_item_names_its_source_instead_of_an_unlock_tier() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let paints = |output: &FullOutput, part: &str| {
        output.shapes.iter().any(|s| shape_has_text(&s.shape, part))
    };
    // A shop definition names the tier that unlocks it.
    let shop = f.game.campaign_mut().add_item(6).unwrap();
    f.armory.selected_item = Some(shop);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(paints(
        &output,
        &format!("unlock tier {}", content().items[6].tier)
    ));
    assert!(!paints(&output, horde_core::content::LOOT_ONLY_REASON));
    // A loot-only definition is never unlocked or bought: core's own reason
    // stands where the tier was.
    let definition = content()
        .items
        .iter()
        .position(|item| item.loot_only && item.slot == Slot::Outfit)
        .expect("a loot-only outfit");
    let found = stash(f, definition, Quality::Rare);
    f.armory.selected_item = Some(found);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(paints(
        &output,
        &format!("· {}", horde_core::content::LOOT_ONLY_REASON)
    ));
    assert!(!paints(&output, "unlock tier"));
}
