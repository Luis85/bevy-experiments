//! Egui harness tests: history screen.
use super::*;

#[test]
fn history_is_reachable_from_menu_camp_and_results_without_repaying_settled_runs() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..120 {
        f.game.tick(Input::default());
    }
    f.game.withdraw();
    assert_eq!(f.game.campaign().run_history.len(), 1);
    f.game.set_paused(true);
    let before = serde_json::to_value(&f.game).unwrap();
    f.screen = Screen::Menu;
    h.click(f, View::Menu, "History");
    assert!(f.screen == Screen::History);
    for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
        let output = h.frame_size(f, View::History, vec![], size);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Run history"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Retreated"))
        );
    }
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Menu);
    f.screen = Screen::Camp;
    h.click(f, View::Header, "History");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Camp);
    f.screen = Screen::Results;
    h.click(f, View::Header, "History");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Results);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

fn text_centre_y(output: &FullOutput, exact: &str) -> f32 {
    fn find(shape: &egui::epaint::Shape, exact: &str) -> Option<f32> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == exact => {
                Some(text.galley.rect.translate(text.pos.to_vec2()).center().y)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, exact)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| find(&clipped.shape, exact))
        .unwrap_or_else(|| panic!("{exact} not painted"))
}

/// A ledger with one settled withdrawal, opened from the camp.
fn one_run(fixture: &mut IsolatedFrontend) -> &mut FrontendParts {
    fixture.deploy();
    let f = &mut fixture.frontend;
    for _ in 0..120 {
        f.game.tick(Input::default());
    }
    f.game.withdraw();
    f.game.set_paused(true);
    f.screen = Screen::History;
    f
}

#[test]
fn history_filters_share_one_centre_line_with_the_run_count() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = one_run(&mut fixture);
    let output = h.frame(f, View::History, vec![]);
    let row = text_centre_y(&output, "All modes");
    for sibling in [
        "All outcomes",
        "Filter front / country",
        "Clear filters",
        "1 of 1 runs · newest first",
    ] {
        let y = text_centre_y(&output, sibling);
        assert!(
            (y - row).abs() < 1.0,
            "{sibling} at {y}, All modes at {row}"
        );
    }
}

/// Height of the smallest painted frame around the text reading `label`:
/// the widget's own background.
fn frame_height(output: &egui::FullOutput, label: &str) -> f32 {
    let (text, _) = clipped_bounds(output, label).unwrap_or_else(|| panic!("{label} not painted"));
    fn rects(shape: &egui::epaint::Shape, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Rect(r) => out.push(r.rect),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| rects(s, out)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    output.shapes.iter().for_each(|c| rects(&c.shape, &mut all));
    all.into_iter()
        .filter(|r| r.contains_rect(text) && r.height() < 80.0)
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("{label} has no frame"))
        .height()
}

#[test]
fn clear_filters_is_as_tall_as_the_dropdowns_beside_it() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = one_run(&mut fixture);
    let output = h.frame(f, View::History, vec![]);
    let dropdown = frame_height(&output, "All modes");
    let clear = frame_height(&output, "Clear filters");
    assert!(
        (dropdown - clear).abs() < 1.0,
        "Clear filters {clear} vs dropdown {dropdown}"
    );
}

#[test]
fn history_title_and_back_share_one_row_at_both_review_sizes() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = one_run(&mut fixture);
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::History, vec![], size);
        let title = text_centre_y(&output, "Run history");
        let back = text_centre_y(&output, "Back");
        assert!((title - back).abs() < 8.0, "{size:?}: {title} vs {back}");
        // The header already says where you are; the page doesn't repeat it.
        assert_eq!(text_count(&output, "Analysis"), 0, "{size:?}");
        let first = f.game.campaign().run_history[0].id;
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, &format!("#{first} · Retreated"))),
            "{size:?}"
        );
    }
}

#[test]
fn empty_history_offers_one_next_step_without_idle_filters() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert!(f.game.campaign().run_history.is_empty());
    f.screen = Screen::History;
    let output = h.frame(f, View::History, vec![]);
    assert_eq!(text_count(&output, EMPTY_NEXT_STEP_TEXT), 1);
    for idle in ["All modes", "All outcomes", "Clear filters", "0 of 0 runs"] {
        assert!(
            !output.shapes.iter().any(|s| shape_has_text(&s.shape, idle)),
            "{idle} shown on an empty ledger"
        );
    }
    assert!(fully_visible(&output, "Back"));
    h.click(f, View::History, "Back");
    assert!(f.screen != Screen::History);
}

const EMPTY_NEXT_STEP_TEXT: &str = "Finish or retreat from a defense to record your first run.";
