//! Presentation-only feedback and navigation motion. Time here never advances
//! a battle, a reward, recovery or a strategic deadline.
use super::{Screen, UiCtx};
use crate::art::{AMBER, GREEN, MUTED, RED};
use bevy_egui::egui::{self, RichText, Stroke, pos2, vec2};

#[derive(Default)]
pub(super) struct FeedbackState {
    key: Option<(String, u64, bool, bool)>,
    started: f64,
    dismissed: bool,
    pub(super) details_open: bool,
    details: String,
}
impl FeedbackState {
    fn observe(&mut self, now: f64, message: &str, revision: u64, error: bool, pending: bool) {
        let key = (message.to_owned(), revision, error, pending);
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.started = now;
            self.dismissed = false;
        }
    }
    fn visible(&self, now: f64, sticky: bool) -> bool {
        !self.dismissed && (sticky || now - self.started < 5.0)
    }
}
pub(super) fn status_line(ui: &mut egui::Ui, f: &mut UiCtx) {
    let now = ui.input(|input| input.time);
    f.status.feedback.observe(
        now,
        &f.status.message,
        f.status.feedback_revision,
        f.status.message_error,
        f.status.save_pending,
    );
    let critical = f.status.message_error
        || f.status.message.starts_with("Save could not be read:")
        || f.status.message.contains(super::toast::RECOVERY_COPIES)
        || f.status.message.starts_with("Original unreadable");
    let applying = f.runtime.pending_commands() > 0;
    let working = f.status.save_pending || applying;
    // A review capture pins the line (CF-3-13): its message stays at full
    // strength for the whole launch, so a still never depends on how long
    // the launch took to reach its capture frame.
    let pinned = f.launch.review_capture();
    let visible = working
        || !f.status.message.is_empty()
            && f.status
                .feedback
                .visible(now, critical || f.status.save_pending || pinned);
    let text = if applying && !f.status.save_pending {
        "Applying changes…".into()
    } else if visible {
        f.status.message.clone()
    } else if f.status.practice_profile {
        "Practice · separate campaign".into()
    } else {
        "Coins and equipment bank as they are earned.".into()
    };
    let alpha = if !visible || critical || working || pinned || f.settings.reduced_effects {
        1.0
    } else {
        ((5.0 - (now - f.status.feedback.started)) / 0.25).clamp(0.0, 1.0) as f32
    };
    ui.horizontal(|ui| {
        if working {
            ui.spinner();
        }
        let details = visible && !working && (critical || text.chars().count() > 95);
        let buttons = if visible && !working {
            let labels: &[&str] = if details {
                &["Details", "Dismiss"]
            } else {
                &["Dismiss"]
            };
            small_buttons_width(ui, labels)
        } else {
            0.0
        };
        let width = (ui.available_width() - buttons - ui.spacing().item_spacing.x).max(60.0);
        let color = if !visible {
            MUTED
        } else if working {
            AMBER
        } else if f.status.message_error {
            RED
        } else if critical {
            AMBER
        } else if visible && !f.status.practice_profile {
            GREEN
        } else {
            MUTED
        };
        ui.scope(|ui| {
            ui.set_opacity(alpha);
            ui.add_sized(
                vec2(width, 22.0),
                egui::Label::new(RichText::new(&text).color(color)).truncate(),
            )
            .on_hover_text(&text);
        });
        if visible && !working {
            if details && ui.small_button("Details").clicked() {
                f.status.feedback.details = text.clone();
                f.status.feedback.details_open = true;
            }
            if ui.small_button("Dismiss").clicked() {
                f.status.feedback.dismissed = true;
            }
        }
    });
    let mut open = f.status.feedback.details_open;
    if open {
        let details = f.status.feedback.details.clone();
        egui::Window::new("Operation details")
            .id(egui::Id::new("feedback_details"))
            .default_width(440.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.set_max_width(500.0);
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        ui.label(details);
                    });
            });
    }
    f.status.feedback.details_open = open;
}
/// The drawn width of a row of small buttons, spacing included, measured by
/// laying them out in an invisible, disabled probe beside the row: no fixed
/// reservation can follow the font, the UI scale and the button style.
fn small_buttons_width(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let mut probe = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("feedback_button_probe")
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .invisible(),
    );
    for label in labels {
        let _ = probe.small_button(*label);
    }
    probe.min_rect().width()
}
pub(super) fn tab_marker(
    ui: &egui::Ui,
    response: &egui::Response,
    key: &str,
    selected: bool,
    reduced: bool,
) {
    let amount = if reduced {
        if selected { 1.0 } else { 0.0 }
    } else {
        ui.ctx().animate_bool_with_time_and_easing(
            egui::Id::new(("tab_accent", key)),
            selected,
            0.14,
            egui::emath::easing::cubic_out,
        )
    };
    if amount > 0.001 {
        let rect = response.rect;
        let y = rect.bottom() - 1.0;
        ui.painter().line_segment(
            [
                pos2(rect.left(), y),
                pos2(rect.left() + rect.width() * amount, y),
            ],
            Stroke::new(1.0, AMBER.gamma_multiply(amount)),
        );
    }
}
pub(super) fn view_progress(ctx: &egui::Context, screen: Screen, reduced: bool) -> f32 {
    let mut active = 1.0;
    for view in Screen::ALL {
        let selected = view == screen;
        let value = if reduced {
            if selected { 1.0 } else { 0.0 }
        } else {
            ctx.animate_bool_with_time_and_easing(
                egui::Id::new(("view_accent", view as u8)),
                selected,
                0.16,
                egui::emath::easing::cubic_out,
            )
        };
        if selected {
            active = value;
        }
    }
    active
}
