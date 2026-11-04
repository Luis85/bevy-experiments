//! Compact authoritative HUD readouts. Values are read directly from the battle;
//! no easing, wall-clock interpolation or gameplay mutations hide action timing.
use crate::art::{AMBER, GREEN, MUTED, RED};
use bevy_egui::egui::{self, FontId, RichText, Sense, Stroke, pos2, vec2};
mod view;
use view::{ActionReadout, Danger, DangerReadout, DpsReadout};
pub(super) use view::{BossReadout, dps as dps_readout, view_model};

pub(super) fn action(ui: &mut egui::Ui, readout: &ActionReadout) {
    let ActionReadout {
        label,
        progress,
        detail,
    } = readout;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().min(220.0), 20.0), Sense::hover());
    // Native epaint truncation keeps one ellipsized row without introducing a
    // child widget's intrinsic size or visibility/sizing pass into the HUD.
    let mut job = egui::text::LayoutJob::simple(
        label.clone(),
        FontId::proportional(12.0),
        MUTED,
        rect.width(),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect)
        .galley(rect.left_top(), galley, MUTED);
    if let Some(progress) = *progress {
        let y = rect.bottom() - 1.0;
        ui.painter().line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(2.0, AMBER.gamma_multiply(0.12)),
        );
        ui.painter().line_segment(
            [
                pos2(rect.left(), y),
                pos2(rect.left() + rect.width() * progress.clamp(0.0, 1.0), y),
            ],
            Stroke::new(2.0, AMBER),
        );
    }
    response.on_hover_text(format!("{label}\n{detail}"));
}
pub(super) fn danger(ui: &mut egui::Ui, readout: &DangerReadout) {
    let color = match readout.danger {
        Danger::Hit | Danger::Contact => RED,
        Danger::Closing => AMBER,
        Danger::Holding => GREEN,
    };
    ui.label(
        RichText::new(&readout.label)
            .size(11.0)
            .color(color)
            .strong(),
    )
    .on_hover_text(&readout.detail);
}

pub(super) fn dps(ui: &mut egui::Ui, readout: &DpsReadout) {
    let note = match readout.average_and_peak {
        None => {
            "Legacy checkpoint: earlier damage was not recorded; full-run average is unavailable."
                .to_owned()
        }
        Some((average, peak)) => format!("Average {average:.1} · peak (5s) {peak:.1}"),
    };
    let totals = readout.totals;
    ui.label(RichText::new(format!("DPS {:.1}", readout.live)).size(12.0).strong().color(AMBER))
        .on_hover_text(format!("Actual resolved damage over the last 5 simulation seconds. Playback speed does not multiply DPS.\n{note}\nTotal recorded: defender {} · defenses {} · ordnance {}",totals.defender,totals.structures,totals.ordnance));
}
