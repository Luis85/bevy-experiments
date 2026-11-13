//! W3-05 (UX-07): one "Save" label for the manual save, and the toast layer
//! that says "Saved" (design-ux-flow §3 "ui/toast.rs", §4 "Save UX").
use super::*;
use crate::runner::RuntimeNotice;

/// Every name the manual save and its compound actions had before W3-05.
const OLD_SAVE_LABELS: [&str; 5] = [
    "Save campaign",
    "Save journey",
    "Save deployment",
    "Save and exit",
    "Save and return to menu",
];

/// The runner's answer to a save of `intent` that succeeded.
fn saved(f: &mut FrontendParts, intent: SaveIntent) {
    f.notices.push(RuntimeNotice::Saved {
        intent,
        result: Ok(()),
    });
    f.apply_notices();
}

/// One `draw_screen` frame at the harness target with the input clock at
/// `time` (seconds), as the App's egui pass runs it.
fn full_at(h: &Harness, f: &mut FrontendParts, time: f64) -> FullOutput {
    f.settings.scale = h.target.scale;
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.target.pixels)),
            time: Some(time),
            ..RawInput::default()
        },
        |ui| draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
    );
    output.textures_delta.clear();
    output
}

/// The drawn opacity of the text run reading exactly `label`: its shape's
/// opacity times the most opaque glyph vertex, which carries the painter's
/// opacity (`None` when it is not painted).
fn text_opacity(output: &FullOutput, label: &str) -> Option<f32> {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<f32> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                let alpha = text
                    .galley
                    .rows
                    .iter()
                    .flat_map(|row| row.visuals.mesh.vertices.iter())
                    .map(|vertex| vertex.color.a())
                    .max()
                    .map_or(0.0, |a| f32::from(a) / 255.0);
                Some(text.opacity_factor * alpha)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|c| find(&c.shape, label))
}

/// Camp home with a started journey and no live battle.
fn camp_home(fixture: &mut IsolatedFrontend) {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
}

#[test]
fn exactly_one_save_label() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    // Battle: Management holds the one manual save and the compound exit.
    let h = Harness::new();
    h.click(f, View::Battle, "Management");
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(
        text_count(&output, "Save"),
        1,
        "battle Management saves once"
    );
    assert_eq!(text_count(&output, "Save and return to main menu"), 1);
    for old in OLD_SAVE_LABELS {
        assert_eq!(text_count(&output, old), 0, "battle still names {old:?}");
    }
    // Results: the header carries "Save"; "More" no longer repeats it.
    f.game.withdraw();
    f.screen = Screen::Results;
    let h = Harness::new();
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Save"), 1, "Results header saves once");
    h.click(f, View::Full, "More");
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Save"), 1, "More adds no second save");
    for old in OLD_SAVE_LABELS {
        assert_eq!(text_count(&output, old), 0, "Results still names {old:?}");
    }
    // Camp home: the header again, never a footer "Save campaign".
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
    let h = Harness::new();
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Save"), 1, "Camp header saves once");
    for old in OLD_SAVE_LABELS {
        assert_eq!(text_count(&output, old), 0, "Camp still names {old:?}");
    }
    // The main menu's compound action is "Save and quit".
    f.screen = Screen::Menu;
    let h = Harness::new();
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Save and quit"), 1);
    for old in OLD_SAVE_LABELS {
        assert_eq!(text_count(&output, old), 0, "the menu still names {old:?}");
    }
}

#[test]
fn manual_save_shows_saved_toast_on_results() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.screen = Screen::Results;
    for target in Target::BOTH {
        let h = Harness::at(target);
        h.click(f, View::Full, "Save");
        assert!(!f.status.save_pending, "the click's frames commit the save");
        assert!(Game::load(&f.paths.campaign()).is_ok());
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(text_count(&output, "Saved"), 1, "{target:?}: one toast");
        let (rect, clip) = clipped_bounds(&output, "Saved").unwrap();
        assert!(
            clip.contains_rect(rect),
            "{target:?}: {rect:?} clipped by {clip:?}"
        );
        // The top-right stack: 16 pt in from the right, under the header.
        let screen = h.ctx.content_rect();
        assert!(
            rect.right() <= screen.right() - 16.0
                && rect.left() >= screen.right() - 16.0 - 280.0
                && rect.top() >= screen.top() + 52.0
                && rect.bottom() < screen.center().y,
            "{target:?}: toast at {rect:?} in {screen:?}"
        );
        // The toast says it; the pending text does not linger as status.
        assert!(!f.status.message.contains("Saving"), "{}", f.status.message);
        assert_eq!(text_count(&output, "Saving…"), 0);
    }
}

#[test]
fn battle_toast_draws_in_receipt_slot() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let quality = horde_core::quality::Quality::Rare;
    let id = f
        .game
        .campaign_mut()
        .add_item_with_quality(4, 2, Some(quality))
        .unwrap();
    let a = f.game.attempt_mut().unwrap();
    a.loot_events.push(horde_core::quality::LootEvent {
        item_id: id,
        definition: horde_core::content::ItemDefId::at(4),
        level: 2,
        quality,
        name: content().items[4].name.clone(),
        tick: a.tick,
        at: [20.0, 5.5],
    });
    let receipt = "Last loot · already banked";
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(
        text_count(&output, receipt),
        1,
        "the receipt holds the slot"
    );
    let a = f.game.attempt().unwrap();
    let slot = battle::BattleLayout::new(h.ctx.content_rect(), a.lane_count, a.width_yards, false)
        .receipt_slot;
    let before = serde_json::to_value(&f.game).unwrap();
    saved(f, SaveIntent::Manual);
    let output = h.frame(f, View::Battle, vec![]);
    let (rect, _) = clipped_bounds(&output, "Saved").expect("the battle toast is drawn");
    assert!(slot.contains_rect(rect), "toast {rect:?} outside {slot:?}");
    assert_eq!(
        text_count(&output, receipt),
        0,
        "the toast outranks the receipt"
    );
    // Battle shows one toast: the newest.
    f.notices
        .push(RuntimeNotice::RecoveryCopies(vec![PathBuf::from(
            "campaign.recovered-1.json",
        )]));
    saved(f, SaveIntent::Manual);
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(text_count(&output, "Saved · recovery copies kept"), 1);
    assert_eq!(
        text_count(&output, "Saved"),
        0,
        "one battle toast at a time"
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn toast_expires_after_3s_and_is_static_in_reduced_effects() {
    for reduced in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        camp_home(&mut fixture);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.settings.reduced_effects = reduced;
        let base = h.ctx.input(|input| input.time) + 1.0;
        full_at(&h, f, base - 0.5);
        let before = serde_json::to_value(&f.game).unwrap();
        saved(f, SaveIntent::Manual);
        let shown = full_at(&h, f, base);
        assert_eq!(
            text_opacity(&shown, "Saved"),
            Some(1.0),
            "reduced={reduced}"
        );
        let late = full_at(&h, f, base + 2.9);
        let opacity = text_opacity(&late, "Saved").expect("still shown at 2.9 s");
        if reduced {
            assert_eq!(opacity, 1.0, "reduced effects never fades a toast");
        } else {
            assert!(opacity < 0.75, "the toast fades out: {opacity}");
        }
        let gone = full_at(&h, f, base + 3.01);
        assert_eq!(text_opacity(&gone, "Saved"), None, "reduced={reduced}");
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}
