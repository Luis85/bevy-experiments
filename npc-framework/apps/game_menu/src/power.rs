//! How often the menu wakes (Ruling: a menu is not a game loop).
//!
//! The menu renders reactively instead of continuously: a frame when a
//! window event (a key, the cursor over the window, a resize, focus) or a
//! redraw request comes, else every [`IDLE`] focused / [`IDLE_UNFOCUSED`]
//! unfocused. Like [`WinitSettings::desktop_app`], but low power in both
//! states: device events (the mouse moving anywhere on the desktop) do not
//! wake it. [`activity`] shortens the wait while something must be seen
//! without input:
//!
//! - the game runs (the window is hidden): a poll every [`POLL`], nothing else;
//! - a save check or a build runs (their screens poll the child), a detached
//!   world starts, stops or moves, or the Detached screen shows one (its tick
//!   is polled), the menu quits (the Quitting screen polls every process), the Resume screen checks `run.json` pids, a clipboard read or write is in flight, `--screenshot` is
//!   pending, or the menu just started: a frame every [`POLL`] or less;
//! - a text field has the focus: a frame every [`BLINK`], half of the caret's
//!   blink period, so the caret blinks.
//!
//! [`wake_after_changes`] asks for one more frame after a frame that changed
//! something the next frame shows (a pending screen change, an edited field,
//! a pressed key or button: the action it queued lands at the end of the
//! frame).

use std::time::Duration;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::window::RequestRedraw;
use bevy::winit::{UpdateMode, WinitSettings};

use crate::ScreenshotAt;
use crate::detached::{DetachedWorlds, PromptCopy};
use crate::explorer::SaveCheck;
use crate::paste::ClipboardJobs;
use crate::run::Running;
use crate::screens::Screen;

/// How often the menu wakes while a child runs (it polls `try_wait`).
pub const POLL: Duration = Duration::from_millis(250);

/// The frame interval while a clipboard operation is in flight (a paste
/// shows up this soon after the tool answered).
pub const CLIPBOARD_POLL: Duration = Duration::from_millis(50);

/// The frame interval while `--screenshot` waits, and right after start.
pub const SHOT_POLL: Duration = Duration::from_millis(100);

/// The frame interval while a text field has the focus: half of
/// `EditableText`'s default 1 s caret blink period.
pub const BLINK: Duration = Duration::from_millis(500);

/// The longest wait for a frame while the window has the focus.
pub const IDLE: Duration = Duration::from_secs(5);

/// The longest wait for a frame while the window does not have the focus.
pub const IDLE_UNFOCUSED: Duration = Duration::from_secs(60);

/// How long after start the menu keeps polling (the first frames lay the
/// UI out).
const STARTUP: Duration = Duration::from_secs(1);

/// What the menu is doing, for its update mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Waiting for input.
    Idle,
    /// A text field has the focus: the caret blinks.
    Typing,
    /// Something must be polled every `wait` (or sooner).
    Polling(Duration),
    /// A game runs and the menu window is hidden.
    Hidden,
}

/// The winit update mode for `activity` (always reactive, low power: see
/// the module docs).
pub fn winit_settings(activity: Activity) -> WinitSettings {
    let (focused, unfocused) = match activity {
        Activity::Idle => (IDLE, IDLE_UNFOCUSED),
        Activity::Typing => (BLINK, IDLE_UNFOCUSED),
        Activity::Polling(wait) => (wait, wait),
        Activity::Hidden => (POLL, POLL),
    };
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(focused),
        unfocused_mode: UpdateMode::reactive_low_power(unfocused),
    }
}

/// What wants frames, for [`activity`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wants {
    /// A game runs (the window is hidden).
    pub game: bool,
    /// A build or a save check runs.
    pub child: bool,
    /// A clipboard read or write is in flight.
    pub clipboard: bool,
    /// `--screenshot` is pending, or the menu just started.
    pub shot: bool,
    /// A text field has the focus.
    pub typing: bool,
}

/// The [`Activity`] for what wants frames: a hidden window only polls; else
/// the shortest poll anything needs; else typing; else idle.
pub fn activity(w: Wants) -> Activity {
    if w.game {
        return Activity::Hidden;
    }
    let wait = [
        (w.clipboard, CLIPBOARD_POLL),
        (w.shot, SHOT_POLL),
        (w.child, POLL),
    ]
    .into_iter()
    .filter_map(|(on, wait)| on.then_some(wait))
    .min();
    match wait {
        Some(wait) => Activity::Polling(wait),
        None if w.typing => Activity::Typing,
        None => Activity::Idle,
    }
}

/// Sets [`WinitSettings`] from what the menu is doing.
pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RequestRedraw>()
            .insert_resource(winit_settings(Activity::Polling(SHOT_POLL)))
            .add_systems(Last, (set_update_mode, wake_after_changes));
    }
}

/// What [`set_update_mode`] reads besides the settings.
type Inputs<'w> = (
    Option<Res<'w, Running>>,
    Option<Res<'w, SaveCheck>>,
    Option<Res<'w, ClipboardJobs>>,
    Option<Res<'w, ScreenshotAt>>,
    Res<'w, Time<Real>>,
);

/// The detached worlds, the prompt's Copy and the screen.
type DetachedInputs<'w> = (
    Option<Res<'w, DetachedWorlds>>,
    Option<Res<'w, PromptCopy>>,
    Option<Res<'w, State<Screen>>>,
    Option<Res<'w, crate::explorer::RunScan>>,
);

fn set_update_mode(
    (running, check, clipboard, shot, time): Inputs,
    (worlds, copy, screen, scan): DetachedInputs,
    focus: Option<Res<InputFocus>>,
    fields: Query<(), With<EditableText>>,
    mut settings: ResMut<WinitSettings>,
) {
    let detached = worlds.is_some_and(|w| w.wants_frames())
        || scan.is_some_and(|s| s.is_pending())
        || screen.is_some_and(|s| matches!(s.get(), Screen::ExplorerDetached | Screen::Quitting));
    let wants = Wants {
        game: running.as_ref().is_some_and(|r| !r.is_build()),
        child: running.is_some() || check.is_some() || detached,
        clipboard: clipboard.is_some_and(|c| !c.is_empty()) || copy.is_some_and(|c| c.is_pending()),
        shot: shot.is_some() || time.elapsed() < STARTUP,
        typing: focus
            .and_then(|f| f.get())
            .is_some_and(|e| fields.contains(e)),
    };
    let want = winit_settings(activity(wants));
    let now = settings.bypass_change_detection();
    if now.focused_mode != want.focused_mode || now.unfocused_mode != want.unfocused_mode {
        *settings = want;
    }
}

/// What [`wake_after_changes`] watches.
type Changes<'w, 's> = (
    Option<Res<'w, NextState<Screen>>>,
    Query<'w, 's, (), Changed<EditableText>>,
    Option<Res<'w, ButtonInput<KeyCode>>>,
    Option<Res<'w, ButtonInput<MouseButton>>>,
);

/// One more frame after a frame whose effects the next frame shows (see the
/// module docs).
fn wake_after_changes(
    (next, edited, keys, mouse): Changes,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let pending = next.is_some_and(|n| !matches!(*n, NextState::Unchanged));
    let pressed = keys.is_some_and(|k| {
        k.get_just_pressed().next().is_some() || k.get_just_released().next().is_some()
    }) || mouse.is_some_and(|m| {
        m.get_just_pressed().next().is_some() || m.get_just_released().next().is_some()
    });
    if pending || pressed || !edited.is_empty() {
        redraw.write(RequestRedraw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_waits_for_window_events() {
        let idle = winit_settings(Activity::Idle);
        assert_eq!(idle.focused_mode, UpdateMode::reactive_low_power(IDLE));
        assert_eq!(
            idle.unfocused_mode,
            UpdateMode::reactive_low_power(IDLE_UNFOCUSED)
        );
        // Never continuous.
        for a in [
            Activity::Idle,
            Activity::Typing,
            Activity::Polling(POLL),
            Activity::Hidden,
        ] {
            let s = winit_settings(a);
            assert_ne!(s.focused_mode, UpdateMode::Continuous, "{a:?}");
            assert_ne!(s.unfocused_mode, UpdateMode::Continuous, "{a:?}");
        }
        assert_eq!(activity(Wants::default()), Activity::Idle);
    }

    #[test]
    fn typing_wakes_for_the_caret() {
        let w = Wants {
            typing: true,
            ..default()
        };
        assert_eq!(activity(w), Activity::Typing);
        assert_eq!(
            winit_settings(Activity::Typing).focused_mode,
            UpdateMode::reactive_low_power(BLINK)
        );
    }

    #[test]
    fn polling_takes_the_shortest_wait() {
        let child = Wants {
            child: true,
            typing: true,
            ..default()
        };
        assert_eq!(activity(child), Activity::Polling(POLL));
        let shot = Wants {
            child: true,
            shot: true,
            ..default()
        };
        assert_eq!(activity(shot), Activity::Polling(SHOT_POLL));
        let paste = Wants {
            clipboard: true,
            shot: true,
            ..default()
        };
        assert_eq!(activity(paste), Activity::Polling(CLIPBOARD_POLL));
        let s = winit_settings(Activity::Polling(POLL));
        assert_eq!(s.focused_mode, UpdateMode::reactive_low_power(POLL));
        assert_eq!(s.unfocused_mode, UpdateMode::reactive_low_power(POLL));
    }

    #[test]
    fn a_running_game_only_polls() {
        let w = Wants {
            game: true,
            child: true,
            shot: true,
            ..default()
        };
        assert_eq!(activity(w), Activity::Hidden);
        let s = winit_settings(Activity::Hidden);
        assert_eq!(s.focused_mode, UpdateMode::reactive_low_power(POLL));
        assert_eq!(s.unfocused_mode, UpdateMode::reactive_low_power(POLL));
    }
}
