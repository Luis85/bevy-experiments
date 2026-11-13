//! The amber primary action and the per-frame primary registry (UX-02,
//! design-ux-flow §3 and decision 18).
//!
//! Every primary action draws through [`primary`] (a screen's one primary),
//! [`primary_in_bar`] (the battle bar's paused Begin or Resume, sized like the
//! bar's other buttons) or [`primary_in_modal`] (a modal's own primary). Each
//! records a [`PrimaryRecord`] in the context's frame registry, which
//! `draw_screen` clears before anything draws, so the registry always holds
//! exactly the primaries of the frame just drawn. Headless tests read it to
//! prove one amber primary per screen; nothing at runtime reads it.
use crate::art::{AMBER, INK, MUTED};
use bevy_egui::egui::{
    self, Color32, Response, RichText, Sense, Stroke, StrokeKind, TextStyle, TextWrapMode, Vec2,
    WidgetInfo, WidgetText, WidgetType, vec2,
};

/// The fill under a hovered or pressed primary.
pub(super) const PRIMARY_HOVER: Color32 = Color32::from_rgb(250, 200, 110);
/// The smallest screen primary, where the column is that wide.
pub(super) const PRIMARY_MIN_SIZE: Vec2 = vec2(190.0, 40.0);

/// Where a primary sits: the screen itself, or a modal drawn over it. A screen
/// has at most one screen primary; a modal has its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Screen,
    Modal,
}

/// One primary action drawn this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrimaryRecord {
    pub(crate) label: String,
    pub(crate) surface: Surface,
}

/// The primaries drawn since the registry was last cleared (egui temp data).
#[derive(Clone, Default)]
struct FramePrimaries(Vec<PrimaryRecord>);

fn registry_id() -> egui::Id {
    egui::Id::new("lastlight_frame_primaries")
}

/// Empties the registry. `draw_screen` calls this before anything draws, so
/// a frame without a primary never shows a stale one.
pub(super) fn clear_frame_primaries(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<FramePrimaries>(registry_id()));
}

/// The primaries recorded since the last clear, in draw order.
#[cfg(test)]
pub(crate) fn frame_primaries(ctx: &egui::Context) -> Vec<PrimaryRecord> {
    ctx.data(|data| data.get_temp::<FramePrimaries>(registry_id()))
        .map(|primaries| primaries.0)
        .unwrap_or_default()
}

/// The screen's primary action: AMBER fill, INK strong text, no stroke, at
/// least 190x40 where the column allows.
pub(super) fn primary(ui: &mut egui::Ui, text: &str) -> Response {
    let min = vec2(
        ui.available_width().min(PRIMARY_MIN_SIZE.x),
        PRIMARY_MIN_SIZE.y,
    );
    primary_button(ui, text, Surface::Screen, min)
}

/// The battle bar's paused primary (Begin or Resume defense): the screen
/// primary's look at the size of the bar's other buttons, so the bar wraps as
/// before.
pub(super) fn primary_in_bar(ui: &mut egui::Ui, text: &str) -> Response {
    primary_button(ui, text, Surface::Screen, Vec2::ZERO)
}

/// A modal's primary action: the screen primary's look, recorded on the
/// modal surface.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "W3-04's Quit and ReturnToMenu modals are its first callers"
    )
)]
pub(super) fn primary_in_modal(ui: &mut egui::Ui, text: &str) -> Response {
    let min = vec2(
        ui.available_width().min(PRIMARY_MIN_SIZE.x),
        PRIMARY_MIN_SIZE.y,
    );
    primary_button(ui, text, Surface::Modal, min)
}

fn primary_button(ui: &mut egui::Ui, text: &str, surface: Surface, min: Vec2) -> Response {
    let enabled = ui.is_enabled();
    let text_color = if enabled { INK } else { MUTED };
    let padding = ui.spacing().button_padding;
    let galley = WidgetText::from(RichText::new(text).strong().color(text_color)).into_galley(
        ui,
        Some(TextWrapMode::Wrap),
        (ui.available_width() - 2.0 * padding.x).max(0.0),
        TextStyle::Button,
    );
    let size = (galley.size() + 2.0 * padding)
        .max(min)
        .max(vec2(0.0, ui.spacing().interact_size.y));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, galley.text()));
    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            AMBER.gamma_multiply(0.4)
        } else if response.hovered() || response.is_pointer_button_down_on() {
            PRIMARY_HOVER
        } else {
            AMBER
        };
        let radius = ui.visuals().widgets.inactive.corner_radius;
        let painter = ui.painter();
        painter.rect_filled(rect, radius, fill);
        if response.has_focus() {
            painter.rect_stroke(rect, radius, Stroke::new(1.0, INK), StrokeKind::Inside);
        }
        painter.galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<FramePrimaries>(registry_id())
            .0
            .push(PrimaryRecord {
                label: text.to_owned(),
                surface,
            });
    });
    response
}
