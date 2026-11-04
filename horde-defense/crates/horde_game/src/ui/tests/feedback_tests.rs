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
