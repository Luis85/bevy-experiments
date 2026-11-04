//! Compact authoritative HUD readouts. Values are read directly from the battle;
//! no easing, wall-clock interpolation or gameplay mutations hide action timing.
use crate::art::{AMBER, GREEN, MUTED, RED};
use bevy_egui::egui::{self, FontId, RichText, Sense, Stroke, pos2, vec2};
use horde_core::content::Mode;
use horde_core::sim::{Attempt, EnemyPhase, EventKind};

pub(super) fn action(ui: &mut egui::Ui, attempt: &Attempt) {
    let (label, progress) = if attempt.reload_left > 0 {
        let total = if attempt.mode == Mode::Personal {
            (attempt.profile.personal.reload.max(0.5) * 60.0).ceil()
        } else {
            attempt.profile.npc.reload_turns.max(1) as f32 * 60.0
        };
        (
            format!("Reload · {:.1}s", attempt.reload_left as f32 / 60.0),
            Some(1.0 - attempt.reload_left as f32 / total),
        )
    } else if attempt.melee_commit > 0 {
        (
            format!(
                "Melee recovery · {:.1}s",
                attempt.melee_commit as f32 / 60.0
            ),
            Some(1.0 - attempt.melee_commit as f32 / 30.0),
        )
    } else if attempt.mode == Mode::Npc {
        (attempt.action(), Some((attempt.tick % 60) as f32 / 60.0))
    } else if attempt.melee_cooldown > 0 {
        (
            format!(
                "Melee ready in {:.1}s",
                attempt.melee_cooldown as f32 / 60.0
            ),
            Some(1.0 - attempt.melee_cooldown as f32 / 72.0),
        )
    } else {
        (attempt.action(), None)
    };
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
    if let Some(progress) = progress {
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
    response.on_hover_text(format!("{label}\n{}", attempt.action()));
}
pub(super) fn danger(ui: &mut egui::Ui, attempt: &Attempt) {
    let mut lanes: Vec<_> = attempt
        .enemies
        .iter()
        .filter(|enemy| enemy.contact())
        .map(|enemy| enemy.lane + 1)
        .collect();
    lanes.sort_unstable();
    lanes.dedup();
    let recent_hit = attempt.events.iter().rev().find(|event| {
        event.kind == EventKind::Damage
            && event.to == attempt.defender_position()
            && attempt.tick.saturating_sub(event.tick) < 30
            && event.value > 0
    });
    let (label, color) = if let Some(hit) = recent_hit {
        (
            format!(
                "HIT · LANE {}",
                hit.lane.unwrap_or_else(|| hit.from[1]
                    .floor()
                    .clamp(0.0, f32::from(attempt.lane_count.saturating_sub(1)))
                    as u8)
                    + 1
            ),
            RED,
        )
    } else if !lanes.is_empty() {
        (
            format!(
                "{} CONTACT LANE{}",
                lanes.len(),
                if lanes.len() == 1 { "" } else { "S" }
            ),
            RED,
        )
    } else if attempt.enemies.iter().any(|enemy| enemy.x < 6.0) {
        ("CLOSING IN".into(), AMBER)
    } else {
        ("LINE HOLDING".into(), GREEN)
    };
    let winding = attempt
        .enemies
        .iter()
        .filter(|enemy| enemy.contact() && enemy.attack_phase == EnemyPhase::Windup)
        .count();
    let detail = if lanes.is_empty() {
        "No enemy is at the shared defense line.".into()
    } else {
        format!(
            "Contact lanes: {}. {winding} attacks winding up. Any contact lane can hurt the same defender; target that lane for melee.",
            lanes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    ui.label(RichText::new(label).size(11.0).color(color).strong())
        .on_hover_text(detail);
}

pub(super) fn dps(ui: &mut egui::Ui, attempt: &Attempt) {
    let legacy = attempt.gear_snapshot.is_empty();
    let note = if legacy {
        "Legacy checkpoint: earlier damage was not recorded; full-run average is unavailable."
            .to_owned()
    } else {
        format!(
            "Average {:.1} · peak (5s) {:.1}",
            attempt.average_dps(),
            attempt.peak_dps()
        )
    };
    ui.label(RichText::new(format!("DPS {:.1}", attempt.live_dps())).size(12.0).strong().color(AMBER))
        .on_hover_text(format!("Actual resolved damage over the last 5 simulation seconds. Playback speed does not multiply DPS.\n{note}\nTotal recorded: defender {} · defenses {} · ordnance {}",attempt.dps.totals.defender,attempt.dps.totals.structures,attempt.dps.totals.ordnance));
}
