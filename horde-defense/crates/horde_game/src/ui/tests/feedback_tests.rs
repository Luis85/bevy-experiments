//! Egui harness tests: feedback expiry and dismissal.
use super::*;

#[test]
fn feedback_expires_success_but_keeps_errors_and_never_changes_a_battle() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.reduced_effects = true;
    let before = serde_json::to_value(&f.game).unwrap();
    let base = h.ctx.input(|input| input.time) + 1.0;
    f.status.message = "Equipment changed.".into();
    f.status.feedback_revision += 1;
    let first = h.feedback_at(f, base);
    assert!(
        first
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Equipment changed."))
    );
    let expired = h.feedback_at(f, base + 6.0);
    assert!(
        expired
            .shapes
            .iter()
            .all(|shape| !shape_has_text(&shape.shape, "Equipment changed."))
    );
    f.status.message = "Save failed. Retry saving before closing.".into();
    f.status.message_error = true;
    f.status.feedback_revision += 1;
    h.feedback_at(f, base + 7.0);
    let error = h.feedback_at(f, base + 40.0);
    assert!(
        error
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Save failed. Retry saving"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn dismissing_an_error_restores_neutral_feedback_without_claiming_success() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.status.message = "An operation failed. Retry when ready.".into();
    f.status.message_error = true;
    f.status.feedback_revision += 1;
    h.click(f, View::Feedback, "Dismiss");
    let output = h.frame(f, View::Feedback, vec![]);
    let color = output
        .shapes
        .iter()
        .find_map(|shape| text_color(&shape.shape, "Coins and equipment bank as they are earned."))
        .expect("Neutral status should replace the dismissed acknowledgement");
    assert_eq!(color, MUTED);
    assert!(
        f.status.message_error,
        "Dismissal must not pretend the failed operation succeeded"
    );
}

/// The painted rect of the small button labelled `label`: the smallest filled
/// frame that holds the label's galley (the galley itself if none does).
fn button_rect(output: &FullOutput, label: &str) -> Rect {
    fn texts(shape: &egui::epaint::Shape, label: &str, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                out.push(text.galley.rect.translate(text.pos.to_vec2()));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, label, out)),
            _ => {}
        }
    }
    fn frames(shape: &egui::epaint::Shape, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Rect(rect) => out.push(rect.rect),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| frames(s, out)),
            _ => {}
        }
    }
    let mut labels = Vec::new();
    let mut rects = Vec::new();
    for clipped in &output.shapes {
        texts(&clipped.shape, label, &mut labels);
        frames(&clipped.shape, &mut rects);
    }
    let [text] = labels[..] else {
        panic!("expected one painted {label:?}, found {labels:?}");
    };
    rects
        .into_iter()
        .filter(|rect| rect.contains_rect(text) && rect.height() < 40.0)
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or(text)
}

/// CF-1-10: the critical save-failure notice reserved a fixed 120 pt for its
/// two small buttons, less than they draw, so "Dismiss" ran off the window.
#[test]
fn save_failure_notice_keeps_details_and_dismiss_inside_the_window_at_both_sizes() {
    use crate::runner::RuntimeNotice;
    for (scale, pixels) in [(1.0, vec2(1440.0, 900.0)), (1.35, vec2(1050.0, 700.0))] {
        let mut fixture = IsolatedFrontend::new();
        let f = &mut fixture.frontend;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.settings.scale = scale;
        f.notices.push(RuntimeNotice::Saved {
            intent: SaveIntent::Manual,
            result: Err("Access is denied. (os error 5)".into()),
        });
        f.apply_notices();
        assert!(f.status.message_error, "{:?}", f.status.message);
        let ctx = egui::Context::default();
        settings::install_font(&ctx);
        settings::configure(&ctx, scale);
        let mut warmup = ctx.run_ui(RawInput::default(), |_| {});
        warmup.textures_delta.clear();
        // Egui's raw screen rect is in points: the window's pixels over the scale.
        let screen = Rect::from_min_size(Pos2::ZERO, pixels / scale);
        let mut last = None;
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(screen),
                    ..RawInput::default()
                },
                |ui| draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
            );
            output.textures_delta.clear();
            last = Some(output);
        }
        let output = last.unwrap();
        for label in ["Details", "Dismiss"] {
            let rect = button_rect(&output, label);
            assert!(
                screen.contains_rect(rect),
                "{label} at {rect:?} leaves the {pixels:?}@{scale} window {screen:?}"
            );
        }
    }
}
