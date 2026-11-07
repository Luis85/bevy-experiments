//! Explicit isolated screenshot fixtures. These startup-only staged campaigns
//! are never claimed as balance proof or used by the ordinary user profile.
#[cfg(feature = "review")]
use super::BattleUi;
use super::FrontendParts;
#[cfg(feature = "review")]
use super::battle;
#[cfg(feature = "review")]
use horde_core::content::Mode;
#[cfg(any(test, feature = "review"))]
use horde_core::{
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::{Game, Input},
};

#[cfg(feature = "review")]
mod player_model;
#[cfg(feature = "review")]
mod workshop;
#[cfg(feature = "review")]
pub(super) use workshop::{NativeModelReport, model_report, model_workshop};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureFixture {
    Throws,
    Melee,
}

fn gesture_fixture(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    practice: bool,
) -> Option<GestureFixture> {
    if !motion || !sequence || practice {
        return None;
    }
    match scene {
        Some("gesture-throws") => Some(GestureFixture::Throws),
        Some("gesture-melee") => Some(GestureFixture::Melee),
        _ => None,
    }
}

#[cfg(any(test, feature = "review"))]
fn scheduled_gesture_input(
    kind: GestureFixture,
    frame: u32,
    attempt: &horde_core::sim::Attempt,
) -> Option<Input> {
    let lane = attempt.lane_count / 2;
    let aim = [attempt.width_yards * 0.6, f32::from(lane) + 0.5];
    let mut input = Input {
        aim,
        ..Default::default()
    };
    match (kind, frame) {
        (GestureFixture::Throws, 64) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: [(attempt.width_yards * 0.6).floor() as u8, lane],
            })
        }
        (GestureFixture::Throws, 96) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: [(attempt.width_yards * 0.7).floor() as u8, lane],
            })
        }
        (GestureFixture::Melee, 64) => input.melee = true,
        _ => return None,
    }
    Some(input)
}

/// Capture-service timing bridge. Only explicit isolated gesture exports
/// can enqueue these ordinary controls; fixed runtime/core owns all effects.
#[cfg(feature = "review")]
pub(crate) fn schedule_gesture_review(
    launch: &crate::launch::LaunchConfig,
    game: &Game,
    battle: &mut BattleUi,
    frame: u32,
) {
    let Some(kind) = gesture_fixture(
        launch.scenario.as_deref(),
        launch.motion_review(),
        launch.sequence_path().is_some(),
        launch.practice,
    ) else {
        return;
    };
    let Some(attempt) = game
        .attempt()
        .filter(|attempt| attempt.mode == Mode::Personal && !attempt.paused)
    else {
        return;
    };
    let Some(input) = scheduled_gesture_input(kind, frame, attempt) else {
        return;
    };
    battle.aim = input.aim;
    battle.pending.aim = input.aim;
    if input.melee {
        battle.pending.melee = true;
        battle.pending_attack_aims.melee = Some(input.aim);
    }
    if let Some(request) = input.throw {
        battle.attack_selection = match request.kind {
            OrdnanceKind::Grenade => battle::AttackSelection::Grenade,
            OrdnanceKind::Molotov => battle::AttackSelection::Molotov,
        };
        battle.pending.throw = Some(request);
    }
}

/// What one review still showed, written beside its PNG as the capture
/// sidecar (`<capture>.json`). Rects are `[left, top, right, bottom]` in egui
/// points. The battle field and its effects clip come from the rects the
/// battle frame recorded (`BattleUi::arena_rect` / `effects_rect`), the
/// worker observer's from `observer_arena_rect`; nothing is re-measured here.
/// `hud_rows` stays None until ux-flow's bar measurement lands (C-10, wave 3).
/// The review tools fill `canvas_rect` and `windows_scale` from the window,
/// and own all of the IO.
#[cfg(feature = "review")]
#[derive(bevy::prelude::Resource, Clone, Debug, Default, PartialEq, serde::Serialize)]
pub(crate) struct ReviewFrameFacts {
    pub(crate) scene: String,
    pub(crate) screen: String,
    pub(crate) tab: Option<String>,
    pub(crate) overlays: Vec<&'static str>,
    pub(crate) canvas_rect: Option<[f32; 4]>,
    pub(crate) arena_rect: Option<[f32; 4]>,
    pub(crate) effects_rect: Option<[f32; 4]>,
    pub(crate) observer_arena_rect: Option<[f32; 4]>,
    pub(crate) lanes: Option<u8>,
    pub(crate) width_yards: Option<f32>,
    pub(crate) hud_rows: Option<u8>,
    pub(crate) reduced_effects: bool,
    pub(crate) ui_scale: f32,
    pub(crate) windows_scale: f32,
}
#[cfg(feature = "review")]
impl ReviewFrameFacts {
    /// The facts of the last drawn pass. Field facts are reported only on
    /// the battle screen, so a stale rect from an earlier battle never is.
    pub(crate) fn collect(
        game: &Game,
        launch: &crate::launch::LaunchConfig,
        settings: &crate::settings::Settings,
        screen: super::Screen,
        tab: super::Tab,
        battle: &BattleUi,
    ) -> Self {
        let corners = |r: bevy_egui::egui::Rect| [r.left(), r.top(), r.right(), r.bottom()];
        let in_battle = screen == super::Screen::Battle;
        let attempt = game.attempt().filter(|_| in_battle);
        let mut overlays = Vec::new();
        if in_battle && battle.construction {
            overlays.push("construction");
        }
        if in_battle && battle.inspect_log {
            overlays.push("inspect-log");
        }
        if battle.observer_arena_rect.is_some() {
            overlays.push("observer");
        }
        Self {
            scene: launch.scenario.clone().unwrap_or_default(),
            screen: format!("{screen:?}"),
            tab: (screen == super::Screen::Camp).then(|| format!("{tab:?}")),
            overlays,
            canvas_rect: None,
            arena_rect: battle.arena_rect.filter(|_| in_battle).map(corners),
            effects_rect: battle.effects_rect.filter(|_| in_battle).map(corners),
            observer_arena_rect: battle.observer_arena_rect.map(corners),
            lanes: attempt.map(|a| a.lane_count),
            width_yards: attempt.map(|a| a.width_yards),
            hud_rows: None,
            reduced_effects: settings.reduced_effects,
            ui_scale: settings.scale,
            windows_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LootCaptureMode {
    Disabled,
    Motion,
    Still,
}
fn select_loot_capture_mode(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    still: bool,
    practice: bool,
) -> LootCaptureMode {
    if scene != Some("quality-loot") || practice {
        LootCaptureMode::Disabled
    } else if motion && sequence {
        LootCaptureMode::Motion
    } else if !motion && !sequence && still {
        LootCaptureMode::Still
    } else {
        LootCaptureMode::Disabled
    }
}
impl crate::launch::LaunchConfig {
    pub(super) fn reticle_capture_preview(&self) -> bool {
        matches!(
            self.scenario.as_deref(),
            Some("reticle-reload" | "damage" | "damage-4x")
        ) && !self.practice
            && (self.capture_path().is_some()
                || (self.motion_review() && self.sequence_path().is_some()))
    }
    pub(super) fn damage_capture_mode(&self) -> LootCaptureMode {
        let allowed = matches!(self.scenario.as_deref(), Some("damage" | "damage-4x"));
        select_loot_capture_mode(
            if allowed { Some("quality-loot") } else { None },
            self.motion_review(),
            self.sequence_path().is_some(),
            self.capture_path().is_some(),
            self.practice,
        )
    }
    pub(super) fn loot_capture_mode(&self) -> LootCaptureMode {
        select_loot_capture_mode(
            self.scenario.as_deref(),
            self.motion_review(),
            self.sequence_path().is_some(),
            self.capture_path().is_some(),
            self.practice,
        )
    }
}

/// Review-only presentation override. Persistent and ordinary profiles keep
/// their chosen preference, and no game clock or random state is inspected.
pub(super) fn apply_reduced_override(
    f: &mut FrontendParts,
    scenario_present: bool,
    practice: bool,
    force_reduced: bool,
) {
    if scenario_present && !practice && force_reduced {
        f.settings.reduced_effects = true;
    }
}

/// Explicit automated-motion fixtures use a separate temporary profile.
/// This changes startup state only; fixed systems still own every tick. The
/// stress fixture keeps its staged disclaimer (no gameplay claim) instead of
/// the generic live-motion note.
pub(super) fn apply_motion_override(f: &mut FrontendParts, config: &crate::launch::LaunchConfig) {
    if config.scenario.is_some() && !config.practice && config.motion_review() {
        f.game.set_paused(false);
        f.battle.set_construction(false);
        if !matches!(config.scenario.as_deref(), Some("stress" | "fx-stress")) {
            f.status.message =
                "Live motion review · progress is isolated from your campaign".into();
        }
    }
}

mod fixtures;
mod matrix;
pub(super) use matrix::prepare;
#[cfg(test)]
mod tests;
