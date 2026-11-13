//! The toast layer (design-ux-flow §3 "ui/toast.rs"): short feedback that
//! never blocks and never takes focus. A toast's clock is the egui input
//! clock (wall time), not the battle tick, so it never advances a battle.
//!
//! Camp, Results and every other screen with no battle field stack up to
//! three toasts top-right under the header; a battle shows the newest one in
//! `BattleLayout.receipt_slot`, over the loot receipt (`battle_toast`).
use super::{Screen, UiCtx};
use crate::art::{GREEN, PANEL, RED, TEXT};
use bevy_egui::egui::{self, Color32, Rect, RichText, Stroke, pos2, vec2};
use std::collections::VecDeque;

/// How long an Info or Success toast stays, in seconds.
pub(super) const TOAST_SECS: f64 = 3.0;
/// How long an Error toast stays, in seconds.
pub(super) const TOAST_ERROR_SECS: f64 = 6.0;
/// The longer life an Info toast that must be read gets ("Saved · recovery
/// copies kept"): as long as an error.
pub(super) const TOAST_LONG_SECS: f64 = TOAST_ERROR_SECS;
/// The fade-out at the end of a toast's life; reduced effects has none.
const TOAST_FADE_SECS: f64 = 0.25;
/// The most toasts the top-right stack shows at once.
const STACK_VISIBLE: usize = 3;
/// The stack's offset from the screen's top-right corner.
const STACK_OFFSET: egui::Vec2 = vec2(-16.0, 52.0);
/// The stack's width.
const STACK_WIDTH: f32 = 280.0;
/// The queue holds at most this many toasts; older ones give way.
const QUEUE_CAP: usize = 8;

/// How the status line of a save that kept recovery copies begins
/// (`notices.rs`).
pub(super) const RECOVERY_COPIES: &str = "Recovery copies:";

/// What a toast reports, which sets its colour and its default lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Info,
    Success,
    Error,
}
impl ToastKind {
    fn secs(self) -> f64 {
        match self {
            Self::Info | Self::Success => TOAST_SECS,
            Self::Error => TOAST_ERROR_SECS,
        }
    }
    fn color(self) -> Color32 {
        match self {
            Self::Info => TEXT,
            Self::Success => GREEN,
            Self::Error => RED,
        }
    }
}

/// One toast. `shown_at` is the input-clock second its life began: `None`
/// until the first frame after it arrived (notices are read outside a pass).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) kind: ToastKind,
    secs: f64,
    shown_at: Option<f64>,
}
impl Toast {
    fn age(&self, now: f64) -> f64 {
        self.shown_at.map_or(0.0, |at| (now - at).max(0.0))
    }
    /// The toast's opacity at `now`: full, then a 0.25 s fade-out at the end
    /// of its life; always full in reduced effects.
    pub(super) fn alpha(&self, now: f64, reduced: bool) -> f32 {
        if reduced {
            return 1.0;
        }
        ((self.secs - self.age(now)) / TOAST_FADE_SECS).clamp(0.0, 1.0) as f32
    }
}

/// The toast queue, oldest first (`StatusUi.toasts`).
#[derive(Default)]
pub(crate) struct Toasts {
    queue: VecDeque<Toast>,
}
impl Toasts {
    /// Queues `text` for its kind's lifetime. The same text arriving again
    /// restarts its timer and becomes the newest toast instead of stacking.
    pub(crate) fn push(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.push_for(text, kind, kind.secs());
    }
    /// `push` with an explicit lifetime in seconds.
    pub(crate) fn push_for(&mut self, text: impl Into<String>, kind: ToastKind, secs: f64) {
        let text = text.into();
        self.queue.retain(|toast| toast.text != text);
        self.queue.push_back(Toast {
            text,
            kind,
            secs,
            shown_at: None,
        });
        while self.queue.len() > QUEUE_CAP {
            self.queue.pop_front();
        }
    }
    /// `push_for` from inside a pass at input-clock second `now`: the toast's
    /// life begins at once, not on the next frame.
    pub(crate) fn push_at(
        &mut self,
        text: impl Into<String>,
        kind: ToastKind,
        secs: f64,
        now: f64,
    ) {
        self.push_for(text, kind, secs);
        if let Some(toast) = self.queue.back_mut() {
            toast.shown_at = Some(now);
        }
    }
    /// Starts the clock of every new toast and drops every expired one.
    pub(super) fn advance(&mut self, now: f64) {
        for toast in &mut self.queue {
            toast.shown_at.get_or_insert(now);
        }
        self.queue.retain(|toast| toast.age(now) < toast.secs);
    }
    /// Up to `count` toasts, newest first.
    pub(super) fn newest(&self, count: usize) -> impl Iterator<Item = &Toast> {
        self.queue.iter().rev().take(count)
    }
    /// Keeps the toast reading `text` while the pointer is over it: its
    /// life restarts once the pointer leaves.
    fn hold(&mut self, text: &str, now: f64) {
        if let Some(toast) = self.queue.iter_mut().find(|toast| toast.text == text) {
            toast.shown_at = Some(now);
        }
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.queue.len()
    }
}

/// Before anything draws: new toasts start their clock, expired ones go.
pub(super) fn advance(ctx: &egui::Context, f: &mut UiCtx) {
    let now = ctx.input(|input| input.time);
    f.status.toasts.advance(now);
}

/// The top-right stack on every screen but the battle, which draws its
/// toast in the receipt slot. Runs at the end of `draw_screen`.
pub(super) fn toast_layer(ctx: &egui::Context, f: &mut UiCtx) {
    if f.screen == Screen::Battle {
        return;
    }
    let toasts: Vec<Toast> = f.status.toasts.newest(STACK_VISIBLE).cloned().collect();
    if toasts.is_empty() {
        return;
    }
    let now = ctx.input(|input| input.time);
    let reduced = f.settings.reduced_effects;
    // A plain layer, not an `Area`: an area's first frame is an invisible
    // sizing pass, which would hide every new toast for a frame.
    let screen = ctx.content_rect();
    let stack = Rect::from_min_max(
        pos2(
            screen.right() + STACK_OFFSET.x - STACK_WIDTH,
            screen.top() + STACK_OFFSET.y,
        ),
        pos2(screen.right() + STACK_OFFSET.x, screen.bottom()),
    );
    let id = egui::Id::new("lastlight_toasts");
    let mut ui = egui::Ui::new(
        ctx.clone(),
        id,
        egui::UiBuilder::new()
            .max_rect(stack)
            .layer_id(egui::LayerId::new(egui::Order::Tooltip, id))
            .layout(egui::Layout::top_down(egui::Align::Max)),
    );
    ui.spacing_mut().item_spacing.y = 6.0;
    let mut hovered = None;
    for toast in &toasts {
        if toast_card(&mut ui, toast, now, reduced, STACK_WIDTH).hovered() {
            hovered = Some(toast.text.clone());
        }
    }
    if let Some(text) = hovered {
        f.status.toasts.hold(&text, now);
    }
}

/// The battle has no status line (C-10): a status message the battle has not
/// shown yet becomes a toast, once; an error is an Error toast. The recovery
/// copies' paths are not one: the save's own toast already says copies were
/// kept, and the paths stay on the status line of every other screen.
pub(super) fn status_toast(f: &mut UiCtx) {
    let status = &mut *f.status;
    if status.message.is_empty() || status.message.starts_with(RECOVERY_COPIES) {
        return;
    }
    let key = (
        status.message.clone(),
        status.feedback_revision,
        status.message_error,
    );
    if status.toasted.as_ref() == Some(&key) {
        return;
    }
    let kind = if status.message_error {
        ToastKind::Error
    } else {
        ToastKind::Info
    };
    status.toasts.push(&*status.message, kind);
    status.toasted = Some(key);
}

/// The battle's one toast, the newest, drawn in `slot` (the receipt slot).
/// Returns whether one was drawn: a drawn toast takes the slot from the loot
/// receipt.
pub(super) fn battle_toast(ui: &mut egui::Ui, slot: Rect, f: &mut UiCtx) -> bool {
    let Some(toast) = f.status.toasts.newest(1).next().cloned() else {
        return false;
    };
    let now = ui.input(|input| input.time);
    let reduced = f.settings.reduced_effects;
    let hovered = ui
        .scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
            ui.set_clip_rect(slot.intersect(ui.clip_rect()));
            toast_card(ui, &toast, now, reduced, slot.width()).hovered()
        })
        .inner;
    if hovered {
        f.status.toasts.hold(&toast.text, now);
    }
    true
}

/// One toast: a PANEL card with a kind-coloured edge and one truncated line.
fn toast_card(
    ui: &mut egui::Ui,
    toast: &Toast,
    now: f64,
    reduced: bool,
    width: f32,
) -> egui::Response {
    let color = toast.kind.color();
    ui.scope(|ui| {
        ui.set_opacity(toast.alpha(now, reduced));
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0, color.gamma_multiply(0.7)))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width((width - 22.0).max(40.0));
                ui.add(
                    egui::Label::new(RichText::new(&toast.text).color(color).size(14.0))
                        .truncate()
                        .selectable(false),
                )
                .on_hover_text(&toast.text);
            })
            .response
            .interact(egui::Sense::hover())
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_restarts_and_moves_to_newest() {
        let mut toasts = Toasts::default();
        toasts.push("Saved", ToastKind::Success);
        toasts.push("Sold 2 · +40", ToastKind::Success);
        toasts.advance(10.0);
        toasts.push("Saved", ToastKind::Success);
        toasts.advance(12.0);
        let order: Vec<_> = toasts.newest(3).map(|t| t.text.as_str()).collect();
        assert_eq!(order, ["Saved", "Sold 2 · +40"]);
        // The refreshed "Saved" started at 12.0; the other at 10.0.
        toasts.advance(13.5);
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts.newest(1).next().unwrap().text, "Saved");
    }

    #[test]
    fn lifetimes_follow_the_kind_and_fade_only_in_normal_effects() {
        let mut toasts = Toasts::default();
        toasts.push("Saved", ToastKind::Success);
        toasts.push("Save failed", ToastKind::Error);
        toasts.advance(0.0);
        let success = toasts.newest(2).last().unwrap().clone();
        assert_eq!(success.alpha(1.0, false), 1.0);
        assert!((success.alpha(2.9, false) - 0.4).abs() < 1e-4);
        assert_eq!(success.alpha(2.9, true), 1.0);
        toasts.advance(3.0);
        assert_eq!(toasts.len(), 1, "Success lasts 3 s");
        toasts.advance(5.99);
        assert_eq!(toasts.len(), 1, "Error lasts 6 s");
        toasts.advance(6.0);
        assert_eq!(toasts.len(), 0);
    }

    #[test]
    fn the_queue_is_bounded() {
        let mut toasts = Toasts::default();
        for n in 0..20 {
            toasts.push(format!("toast {n}"), ToastKind::Info);
        }
        assert_eq!(toasts.len(), QUEUE_CAP);
        assert_eq!(toasts.newest(1).next().unwrap().text, "toast 19");
    }
}
