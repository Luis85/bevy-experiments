//! Egui harness tests: shared header grammar and the rows where labels sit
//! beside buttons.
use super::*;

/// Painted centre lines of every text run reading exactly `label`.
fn centres(output: &FullOutput, label: &str) -> Vec<f32> {
    fn collect(shape: &egui::epaint::Shape, label: &str, out: &mut Vec<f32>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                out.push(text.galley.rect.translate(text.pos.to_vec2()).center().y);
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, label, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, label, &mut out);
    }
    out
}

/// Asserts that each sibling's nearest painted run shares `anchor`'s centre line.
fn assert_one_line(output: &FullOutput, context: &str, anchor: &str, siblings: &[&str]) {
    let y = *centres(output, anchor)
        .first()
        .unwrap_or_else(|| panic!("{context}: {anchor} not painted"));
    for sibling in siblings {
        let nearest = centres(output, sibling)
            .into_iter()
            .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))
            .unwrap_or_else(|| panic!("{context}: {sibling} not painted"));
        assert!(
            (nearest - y).abs() < 1.0,
            "{context}: {sibling} at {nearest}, {anchor} at {y}"
        );
    }
}

fn sizes() -> [(Vec2, f32); 2] {
    [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)]
}

/// A between-runs journey with Mara and Operations unlocked.
fn unlocked(fixture: &mut IsolatedFrontend) -> &mut FrontendParts {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    f.game.campaign_mut().attempts = 1;
    f
}

#[test]
fn the_header_names_each_screen_once() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = unlocked(&mut fixture);
    // W3-03 rewrite: the tree moved from Results to `Screen::Upgrades`.
    // Upgrades is named by its selected row entry, so the header neither
    // repeats it nor calls it the camp; the camp keeps its area name.
    f.screen = Screen::Upgrades;
    let output = h.frame(f, View::Header, vec![]);
    assert_eq!(text_count(&output, "Journey upgrades"), 0);
    assert_eq!(text_count(&output, "Field command"), 0);
    let output = h.frame(f, View::UpgradesWithChrome, vec![]);
    assert_eq!(text_count(&output, "Journey upgrades"), 1);
    // Results has no subtitle either.
    f.screen = Screen::Results;
    let output = h.frame(f, View::Header, vec![]);
    assert_eq!(text_count(&output, "Field command"), 0);
    f.screen = Screen::Camp;
    let output = h.frame(f, View::Header, vec![]);
    assert_eq!(text_count(&output, "Field command"), 1);
    // Settings and run history carry their own page titles, so the header
    // does not repeat or rename them.
    f.screen = Screen::Settings;
    let output = h.frame(f, View::Header, vec![]);
    assert_eq!(text_count(&output, "Settings"), 0);
    f.screen = Screen::History;
    let output = h.frame(f, View::Header, vec![]);
    assert_eq!(text_count(&output, "Run analysis"), 0);
    assert_eq!(text_count(&output, "Run history"), 0);
    let output = h.frame(f, View::History, vec![]);
    assert_eq!(text_count(&output, "Run history"), 1);
}

#[test]
fn results_reach_history_through_the_header_only() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    // W3-03 rewrite: Results needs a settled report (without one it hands
    // over to Camp home).
    f.game.withdraw();
    f.screen = Screen::Results;
    let output = h.frame(f, View::Results, vec![]);
    assert_eq!(text_count(&output, "Run analysis"), 0);
    assert_eq!(text_count(&output, "Run history"), 0, "duplicate route");
    h.click(f, View::Header, "History");
    assert!(f.screen == Screen::History);
}

#[test]
fn camp_rows_keep_labels_on_their_buttons_centre_line() {
    for (size, scale) in sizes() {
        let context = format!("{size:?}");
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        settings::configure(&h.ctx, scale);
        let f = unlocked(&mut fixture);
        f.settings.scale = scale;

        // Navigation row, on Equipment.
        f.act(|c| c.open_preparation(Tab::Armory));
        let output = h.frame_size(f, View::WorldWithChrome, vec![], size);
        assert_one_line(
            &output,
            &context,
            "Journey upgrades",
            // W3-02 rewrite: Camp leads the row; Operations is a drill-down.
            &["Camp", "Briefing", "Equipment", "Character", "World map"],
        );

        // Briefing loadout heading beside its button.
        f.act(|c| c.open_preparation(Tab::Briefing));
        let output = h.frame_size(f, View::WorldWithChrome, vec![], size);
        assert_one_line(&output, &context, "Field loadout", &["Change equipment"]);

        // Mara's training rows.
        f.act(|c| c.open_preparation(Tab::Survivor));
        let output = h.frame_size(f, View::WorldWithChrome, vec![], size);
        assert_one_line(&output, &context, "ST +1", &["Train"]);
    }
}
