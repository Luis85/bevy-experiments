//! The Escape router (UX-05, design-ux-flow §3 "ui/escape.rs", decisions 7
//! and 8). Escape used to mean six things depending on which widget read it
//! first. Now one pure [`escape_action`] decides what a press means from an
//! [`EscapeContext`], and [`escape_router`] (run by `draw_screen` before
//! anything else reads the keyboard) applies it and consumes the key in this
//! one place. Nothing else in the frontend reads `Key::Escape` (pinned by
//! `escape_is_read_only_by_the_router`).
//!
//! The topmost overlay answers first, in [`Overlay`]'s order. With no overlay
//! the screen answers: the menu asks to quit, a camp tab goes back to Camp
//! home, Upgrades goes back to Results (or to Camp home with no settled
//! report), Camp home and Results ask to return to the main menu, and the battle
//! cancels a retreat hold, else closes construction or, only when Pause is bound to Escape (decision 18),
//! passes the press on to the Pause binding. Settings saves and returns,
//! unless the difficulty draft is unapplied: then the press opens the draft
//! warning, and the next one closes it ("Keep editing").
use super::{
    Screen, Tab, UiCtx,
    modal::{AppModal, ModalKind},
    preferences::DraftState,
};
use crate::{runner::SaveIntent, settings::Action};
use bevy_egui::egui;

/// What an Escape press is read against: the screen and tab, the topmost
/// overlay and the few campaign facts the targets depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EscapeContext {
    pub(super) screen: Screen,
    pub(super) tab: Tab,
    pub(super) overlay: Overlay,
    /// The campaign holds a settled report (`Game::last_result`).
    pub(super) settled_report: bool,
    /// `Campaign::journey_active`.
    pub(super) journey_active: bool,
    /// The journey has a start country (`world.start`).
    pub(super) journey_started: bool,
    /// A main defense exists (`Game::attempt`).
    pub(super) attempt_live: bool,
    /// A durable transition (menu, quit or settings-return save, a reset) or
    /// a save is pending: Escape does not navigate.
    pub(super) durable_pending: bool,
    /// The Settings difficulty draft is unapplied (`DraftState` is not
    /// `Clean`).
    pub(super) draft_dirty: bool,
    /// `bindings[Pause] == "Escape"`.
    pub(super) pause_is_escape: bool,
    /// The battle's retreat button is being held (`BattleUi::retreat_holding`).
    pub(super) retreat_hold: bool,
    /// The battle's construction overlay is open.
    pub(super) construction: bool,
}

/// The topmost thing over the screen, resolved topmost-first in exactly this
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Overlay {
    /// A Settings rebind prompt waits for a key.
    Rebind,
    /// A text field has keyboard focus.
    TextFocus,
    /// An application modal (`StatusUi.modal`).
    AppModal(ModalKind),
    /// The menu's new-journey confirmation.
    NewJourneyModal,
    /// The "Operation details" window of the status line.
    Details,
    /// An egui popup or menu is open.
    Popup,
    /// The Operations observer window.
    Observer,
    /// An armory sale confirmation.
    ArmoryConfirm,
    None,
}

/// What one Escape press does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EscapeAction {
    /// egui handles it (a focused text field or an open popup).
    LeaveToEgui,
    CancelRebind,
    CloseModal,
    CloseNewJourney,
    CloseDetails,
    CloseObserver,
    CancelArmoryConfirm,
    /// Cancels the battle's retreat hold; nothing closes and nothing pauses.
    CancelRetreat,
    /// Closes the construction overlay without pausing (decision 18).
    CloseConstruction,
    GoTab(Tab),
    Go(Screen),
    OpenModal(ModalKind),
    CloseHistory,
    /// Settings save and return, with no unapplied difficulty draft.
    SettingsBack,
    /// Left to the Pause binding, which is Escape.
    PassToPause,
    Ignore,
}
impl EscapeAction {
    /// Whether the router consumes the key, hiding it from egui and the
    /// bindings. A press egui owns, a press left to Pause and an ignored one
    /// stay visible.
    pub(super) fn consumes(self) -> bool {
        !matches!(self, Self::LeaveToEgui | Self::PassToPause | Self::Ignore)
    }
    /// Whether the action leaves the current screen, tab or page state, which
    /// a pending durable transition forbids.
    pub(super) fn navigates(self) -> bool {
        matches!(
            self,
            Self::GoTab(_)
                | Self::Go(_)
                | Self::OpenModal(_)
                | Self::CloseHistory
                | Self::SettingsBack
        )
    }
}

/// What an Escape press means in `cx`. Pure.
pub(super) fn escape_action(cx: &EscapeContext) -> EscapeAction {
    use EscapeAction as A;
    let action = match cx.overlay {
        Overlay::Rebind => A::CancelRebind,
        Overlay::TextFocus | Overlay::Popup => A::LeaveToEgui,
        Overlay::AppModal(_) => A::CloseModal,
        // The journey reset may already be under way.
        Overlay::NewJourneyModal if cx.durable_pending => A::Ignore,
        Overlay::NewJourneyModal => A::CloseNewJourney,
        Overlay::Details => A::CloseDetails,
        Overlay::Observer => A::CloseObserver,
        Overlay::ArmoryConfirm => A::CancelArmoryConfirm,
        Overlay::None => screen_action(cx),
    };
    if cx.durable_pending && action.navigates() {
        A::Ignore
    } else {
        action
    }
}

/// The screen's own Escape target, with no overlay open (design-ux-flow §1).
fn screen_action(cx: &EscapeContext) -> EscapeAction {
    use EscapeAction as A;
    match cx.screen {
        Screen::Menu => A::OpenModal(ModalKind::Quit),
        Screen::SurvivorSelection => A::Go(Screen::Menu),
        // Before the first start the camp is the world map, and there is no
        // journey to return to.
        Screen::Camp if !cx.journey_started => A::Go(Screen::Menu),
        Screen::Camp if cx.tab == Tab::Home => {
            if cx.settled_report && cx.journey_active && !cx.attempt_live {
                A::Go(Screen::Results)
            } else {
                A::OpenModal(ModalKind::ReturnToMenu)
            }
        }
        Screen::Camp => A::GoTab(Tab::Home),
        Screen::Battle if cx.retreat_hold => A::CancelRetreat,
        Screen::Battle if cx.construction => A::CloseConstruction,
        Screen::Battle if cx.pause_is_escape => A::PassToPause,
        Screen::Battle => A::Ignore,
        Screen::Results => A::OpenModal(ModalKind::ReturnToMenu),
        // Upgrades goes back to the debrief it was opened from, or to Camp
        // home when no round has settled.
        Screen::Upgrades if cx.settled_report => A::Go(Screen::Results),
        Screen::Upgrades => A::GoTab(Tab::Home),
        Screen::History => A::CloseHistory,
        // An unapplied difficulty draft is never dropped silently (UX-09).
        Screen::Settings if cx.draft_dirty => A::OpenModal(ModalKind::DraftWarning),
        Screen::Settings => A::SettingsBack,
    }
}

/// Who owns the keyboard this frame, for the screen's own key bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyboardOwner {
    /// The screen's bindings may read keys.
    Free,
    /// A focused text field or an open popup.
    Egui,
    /// A rebind prompt, a modal or the details window.
    Overlay,
}

/// Who owns the keyboard: battle `controls`, `camp_controls` and
/// `gather_combat_input` read their bindings only when it is `Free`.
pub(super) fn keyboard_owner(ctx: &egui::Context, f: &UiCtx) -> KeyboardOwner {
    match overlay(ctx, f) {
        Overlay::TextFocus | Overlay::Popup => KeyboardOwner::Egui,
        Overlay::Rebind | Overlay::AppModal(_) | Overlay::NewJourneyModal | Overlay::Details => {
            KeyboardOwner::Overlay
        }
        Overlay::Observer | Overlay::ArmoryConfirm | Overlay::None => KeyboardOwner::Free,
    }
}

/// The topmost overlay of this frame, read before the frame draws.
fn overlay(ctx: &egui::Context, f: &UiCtx) -> Overlay {
    if f.screen == Screen::Settings && f.prefs.rebinding().is_some() {
        Overlay::Rebind
    } else if ctx.text_edit_focused() {
        Overlay::TextFocus
    } else if let Some(modal) = &f.status.modal {
        Overlay::AppModal(modal.kind())
    } else if f.screen == Screen::Menu && f.menu.new_journey_confirm {
        Overlay::NewJourneyModal
    } else if f.status.feedback.details_open {
        Overlay::Details
    } else if ctx.any_popup_open() {
        Overlay::Popup
    } else if f.screen == Screen::Camp && f.tab == Tab::Operations && f.campaign.observed.is_some()
    {
        Overlay::Observer
    } else if f.screen == Screen::Camp
        && f.tab == Tab::Armory
        && (f.armory.sell_confirm || f.armory.pending_sale.is_some())
    {
        Overlay::ArmoryConfirm
    } else {
        Overlay::None
    }
}

/// This frame's [`EscapeContext`].
pub(super) fn escape_context(ctx: &egui::Context, f: &UiCtx) -> EscapeContext {
    let campaign = f.game.campaign();
    EscapeContext {
        screen: f.screen,
        tab: f.tab,
        overlay: overlay(ctx, f),
        settled_report: f.game.last_result().is_some(),
        journey_active: campaign.journey_active,
        journey_started: campaign.world.start.is_some(),
        attempt_live: f.game.attempt().is_some(),
        durable_pending: f.runtime.durable_transition_pending()
            || f.runtime.reset_in_progress()
            || f.status.save_pending,
        draft_dirty: f.screen == Screen::Settings && f.balance_draft_state() != DraftState::Clean,
        pause_is_escape: f.settings.bindings[Action::Pause as usize] == "Escape",
        retreat_hold: f.screen == Screen::Battle
            && f.battle.retreat_holding(ctx.cumulative_frame_nr()),
        construction: f.battle.construction,
    }
}

/// A fresh Escape press this pass: pressed, not a key repeat, no modifiers.
fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.is_none()
            )
        })
    })
}

fn consume_escape(ctx: &egui::Context) {
    ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
}

/// The press this frame already handled: a discarded pass reruns the frame
/// with the same input, so the rerun consumes the key again but does not act
/// twice.
#[derive(Clone, Copy)]
struct Handled {
    frame: u64,
    consumed: bool,
}

/// Reads this frame's Escape press, applies its action and consumes the key
/// where the action says so. `draw_screen` runs it before the controls and
/// the screen draw.
pub(super) fn escape_router(ctx: &egui::Context, f: &mut UiCtx) {
    if !escape_pressed(ctx) {
        return;
    }
    let id = egui::Id::new("lastlight_escape_router");
    let frame = ctx.cumulative_frame_nr();
    if let Some(handled) = ctx.data(|data| data.get_temp::<Handled>(id))
        && handled.frame == frame
    {
        if handled.consumed {
            consume_escape(ctx);
        }
        return;
    }
    let action = escape_action(&escape_context(ctx, f));
    let consumed = action.consumes();
    if consumed {
        consume_escape(ctx);
    }
    ctx.data_mut(|data| data.insert_temp(id, Handled { frame, consumed }));
    apply(action, f);
}

/// Carries out `action` through the routes the screens' own buttons use.
fn apply(action: EscapeAction, f: &mut UiCtx) {
    use EscapeAction as A;
    match action {
        A::LeaveToEgui | A::PassToPause | A::Ignore => {}
        A::CancelRebind => f.prefs.cancel_rebind(),
        A::CloseModal => f.status.close_modal(),
        A::CloseNewJourney => f.menu.new_journey_confirm = false,
        A::CloseDetails => f.status.feedback.details_open = false,
        A::CloseObserver => {
            f.campaign.observed = None;
            f.battle.observer_arena_rect = None;
        }
        A::CancelArmoryConfirm => {
            f.armory.sell_confirm = false;
            f.armory.pending_sale = None;
        }
        A::CancelRetreat => f.battle.cancel_retreat_hold(),
        A::CloseConstruction => f.battle.set_construction(false),
        A::GoTab(Tab::Home) => f.open_camp_home(None),
        A::GoTab(tab) => f.go_tab(tab),
        // Survivor selection holds nothing to save; the camp saves first,
        // as its header's Menu does.
        A::Go(Screen::Menu) if f.screen == Screen::SurvivorSelection => f.go(Screen::Menu),
        A::Go(Screen::Menu) => f.save_settings_to_menu(),
        A::Go(Screen::Results) => f.open_results(),
        A::Go(screen) => f.go(screen),
        A::OpenModal(kind) => {
            if let Some(modal) = AppModal::confirmation(kind) {
                f.status.modal = Some(modal);
            }
        }
        A::CloseHistory => f.close_history(),
        A::SettingsBack => f.request_save(SaveIntent::SettingsBack),
    }
}

#[cfg(test)]
mod tests;
