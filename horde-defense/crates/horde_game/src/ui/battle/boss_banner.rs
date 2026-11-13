//! The boss banner (UX-11, G2.7; design-ux-flow §4 "Boss banner", ruling
//! C-11): "BOSS APPROACHING" when the director warns, "<BOSS> HAS ARRIVED"
//! when the boss is on the field. It is the visual twin of the wave and boss
//! sounds, so it has to be readable: the old field callout aged with the
//! battle clock and lasted 0.25 s of wall time at 4x.
//!
//! The banner's clock is the egui input clock. A new warning or arrival
//! (`battle_view::BossNotice`, a pure core read) starts it at zero, and it
//! counts only frames in which the battle runs, so a paused battle holds the
//! banner where it is and every observation speed shows it equally long.
//! Nothing here touches the simulation.
use crate::{
    art::{self, AMBER, PANEL, TEXT},
    battle_view::BossNotice,
};
use bevy_egui::egui::{self, Color32, FontId, Galley, Painter, Rect, pos2};
use std::sync::Arc;

/// How long "BOSS APPROACHING" shows, in seconds of running wall time.
pub(in crate::ui) const BOSS_BANNER_SECS: f32 = 2.0;
/// How long "<BOSS> HAS ARRIVED" shows, in seconds of running wall time.
pub(in crate::ui) const BOSS_ARRIVED_SECS: f32 = 1.5;
/// The fade at each end in normal effects; reduced effects has none.
const FADE_IN_SECS: f32 = 0.15;
const FADE_OUT_SECS: f32 = 0.30;
/// The most one frame adds to the banner's clock, so a stalled frame never
/// swallows the banner.
const MAX_FRAME_SECS: f32 = 0.1;
/// How old a warning or an arrival may be, in battle ticks, when it is first
/// seen and still start a banner: 2 s of battle time. A battle loaded or
/// staged long after its warning shows none.
const FRESH_TICKS: u64 = 120;
/// The banner's size: `WIDTH_SHARE` of its area up to `MAX_WIDTH`, by `HEIGHT`.
const MAX_WIDTH: f32 = 380.0;
const WIDTH_SHARE: f32 = 0.6;
const HEIGHT: f32 = 34.0;
/// The banner's gap under the area's top edge.
pub(in crate::ui) const BOSS_BANNER_TOP: f32 = 6.0;
/// The banner's gap under the first-defense hint callout.
pub(in crate::ui) const BOSS_BANNER_CALLOUT_GAP: f32 = 4.0;
/// The PANEL fill's alpha at full strength.
const FILL_ALPHA: f32 = 220.0;
const TITLE_SIZE: f32 = 20.0;
const SUBLINE_SIZE: f32 = 10.0;
/// The smallest size a line is shrunk to before it is clipped instead.
const MIN_TEXT_SIZE: f32 = 9.0;
/// The text's margin to the banner's left and right edges.
const TEXT_MARGIN: f32 = 8.0;

/// One banner's presentation state: which warning or arrival it announces
/// and how long it has been shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct BossBanner {
    /// The attempt and the tick of the warning or the arrival.
    pub(in crate::ui) key: (u64, u64),
    pub(in crate::ui) arrived: bool,
    /// Seconds of running wall time shown so far.
    pub(in crate::ui) shown: f32,
    /// The egui frame that last stepped it: a discarded pass reruns the
    /// frame and adds no time.
    pub(in crate::ui) frame: u64,
}
impl BossBanner {
    /// How long this banner shows.
    pub(in crate::ui) fn duration(&self) -> f32 {
        if self.arrived {
            BOSS_ARRIVED_SECS
        } else {
            BOSS_BANNER_SECS
        }
    }
    pub(in crate::ui) fn visible(&self) -> bool {
        self.shown < self.duration()
    }
}

/// The banner's rect in `area` with its top edge at `top`: centred, `min(380,
/// 0.6 x area width)` by 34. Left of an `occluder` (the open construction
/// panel) it moves left until it ends `art::OCCLUDER_GAP` short of it, and
/// narrows only where the area leaves no room.
pub(in crate::ui) fn banner_rect(area: Rect, top: f32, occluder: Option<Rect>) -> Rect {
    let width = (area.width() * WIDTH_SHARE).min(MAX_WIDTH);
    let mut right = area.center().x + width * 0.5;
    if let Some(occluder) = occluder {
        right = right.min(occluder.left() - art::OCCLUDER_GAP);
    }
    let left = (right - width).max(area.left()).min(right);
    Rect::from_min_max(pos2(left, top), pos2(right, top + HEIGHT))
}

/// The banner's rect in the observer window's field box: centred,
/// `BOSS_BANNER_TOP` under its top edge. The box has no hint callout and no
/// construction panel.
pub(in crate::ui) fn observer_banner_rect(area: Rect) -> Rect {
    banner_rect(area, area.top() + BOSS_BANNER_TOP, None)
}

/// One frame of the banner. `banner` is the state before this frame, `notice`
/// the attempt's boss facts, `running` whether the battle runs this frame,
/// `dt` the wall seconds since the last frame and `frame` the egui frame
/// number. Pure.
///
/// A warning or an arrival the banner does not announce yet replaces it and
/// starts at zero, if it is at most `FRESH_TICKS` old. Otherwise the banner
/// gains `dt` (at most `MAX_FRAME_SECS`) while the battle runs. A banner of
/// another attempt is dropped; a finished banner is kept, so its event never
/// starts a second one.
pub(in crate::ui) fn banner_step(
    banner: Option<BossBanner>,
    notice: &BossNotice,
    running: bool,
    dt: f32,
    frame: u64,
) -> Option<BossBanner> {
    let banner = banner.filter(|banner| banner.key.0 == notice.attempt);
    if let Some(event) = notice.event {
        let key = (notice.attempt, event.tick);
        let announced = banner.is_some_and(|b| b.key == key && b.arrived == event.arrived);
        if !announced && notice.tick.saturating_sub(event.tick) <= FRESH_TICKS {
            return Some(BossBanner {
                key,
                arrived: event.arrived,
                shown: 0.0,
                frame,
            });
        }
    }
    banner.map(|mut banner| {
        if running && banner.frame != frame && banner.visible() {
            banner.shown += dt.clamp(0.0, MAX_FRAME_SECS);
        }
        banner.frame = frame;
        banner
    })
}

/// The banner's strength after `shown` of `duration` seconds: 0 once it is
/// over. Reduced effects shows it at full strength for its whole life;
/// normal effects fades it in over 0.15 s and out over 0.30 s.
pub(in crate::ui) fn banner_alpha(shown: f32, duration: f32, reduced: bool) -> f32 {
    if shown >= duration {
        0.0
    } else if reduced {
        1.0
    } else {
        (shown / FADE_IN_SECS)
            .min((duration - shown) / FADE_OUT_SECS)
            .clamp(0.0, 1.0)
    }
}

/// The banner's two lines: the title, and the subline under an approaching
/// boss.
pub(in crate::ui) fn banner_text(notice: &BossNotice, arrived: bool) -> (String, Option<String>) {
    if arrived {
        (format!("{} HAS ARRIVED", notice.name.to_uppercase()), None)
    } else {
        (
            "BOSS APPROACHING".to_owned(),
            Some(format!("{} · arrives in {} s", notice.name, notice.seconds)),
        )
    }
}

/// `text` on one line at `size`, shrunk (to `MIN_TEXT_SIZE` at the least)
/// where it is wider than `width`.
fn fitted(painter: &Painter, text: String, size: f32, width: f32, color: Color32) -> Arc<Galley> {
    let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(size), color);
    if galley.size().x <= width {
        return galley;
    }
    let size = (size * width / galley.size().x).max(MIN_TEXT_SIZE);
    painter.layout_no_wrap(text, FontId::proportional(size), color)
}

/// Steps `state` by this frame and paints the banner in `rect` under `clip`
/// while it shows. Returns the painted rect, `None` when nothing is painted.
pub(in crate::ui) fn boss_banner(
    ui: &egui::Ui,
    clip: Rect,
    rect: Rect,
    state: &mut Option<BossBanner>,
    notice: &BossNotice,
    running: bool,
    reduced: bool,
) -> Option<Rect> {
    let dt = ui.input(|input| input.unstable_dt);
    *state = banner_step(*state, notice, running, dt, ui.ctx().cumulative_frame_nr());
    let banner = (*state)?;
    let alpha = banner_alpha(banner.shown, banner.duration(), reduced);
    if !banner.visible() {
        return None;
    }
    if running {
        // The banner ends on the wall clock, not on the next input.
        ui.ctx().request_repaint();
    }
    if alpha <= 0.0 {
        return None;
    }
    // Full strength is the palette colour itself, bit for bit.
    let fade = |color: Color32| {
        if alpha >= 1.0 {
            color
        } else {
            color.gamma_multiply(alpha)
        }
    };
    let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
    painter.rect_filled(
        rect,
        2.0,
        Color32::from_rgba_unmultiplied(
            PANEL.r(),
            PANEL.g(),
            PANEL.b(),
            (FILL_ALPHA * alpha) as u8,
        ),
    );
    let (title, subline) = banner_text(notice, banner.arrived);
    let width = (rect.width() - 2.0 * TEXT_MARGIN).max(1.0);
    let title = fitted(&painter, title, TITLE_SIZE, width, fade(AMBER));
    let centred = |galley: &Galley, top: f32| pos2(rect.center().x - galley.size().x * 0.5, top);
    match subline {
        Some(subline) => {
            // The title hangs from the top edge and the subline stands on
            // the bottom edge; the capitals of the one clear the other.
            let subline = fitted(&painter, subline, SUBLINE_SIZE, width, fade(TEXT));
            let at = centred(&title, rect.top());
            painter.galley(at, title, AMBER);
            let at = centred(&subline, rect.bottom() - subline.size().y);
            painter.galley(at, subline, TEXT);
        }
        None => {
            let at = centred(&title, rect.center().y - title.size().y * 0.5);
            painter.galley(at, title, AMBER);
        }
    }
    Some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle_view::BossEvent;
    use bevy_egui::egui::vec2;

    fn notice(attempt: u64, tick: u64, event: Option<(bool, u64)>) -> BossNotice {
        BossNotice {
            attempt,
            tick,
            name: "Gate Warden",
            seconds: 4,
            event: event.map(|(arrived, tick)| BossEvent { arrived, tick }),
        }
    }

    #[test]
    fn banner_step_counts_running_wall_time_only() {
        let frame_secs = 1.0 / 60.0;
        // No event: no banner.
        assert_eq!(
            banner_step(None, &notice(7, 10, None), true, 0.016, 1),
            None
        );
        // A fresh warning starts at zero whatever the frame took.
        let warned = notice(7, 100, Some((false, 98)));
        let start = banner_step(None, &warned, true, 5.0, 1).unwrap();
        assert_eq!(
            start,
            BossBanner {
                key: (7, 98),
                arrived: false,
                shown: 0.0,
                frame: 1
            }
        );
        // The battle clock does not matter: four ticks a frame or one.
        let mut fast = Some(start);
        let mut slow = Some(start);
        for frame in 2..=61 {
            let ticks = 100 + 4 * (frame - 1);
            fast = banner_step(
                fast,
                &notice(7, ticks, Some((false, 98))),
                true,
                frame_secs,
                frame,
            );
            slow = banner_step(slow, &warned, true, frame_secs, frame);
        }
        assert_eq!(fast, slow);
        assert!((fast.unwrap().shown - 1.0).abs() < 1e-3);
        // A paused frame, and a rerun of the same frame, add nothing.
        let held = banner_step(fast, &warned, false, frame_secs, 62).unwrap();
        assert_eq!(held.shown, fast.unwrap().shown);
        let rerun = banner_step(Some(held), &warned, true, frame_secs, 62).unwrap();
        assert_eq!(rerun.shown, held.shown);
        // One frame adds at most 0.1 s.
        let stalled = banner_step(Some(held), &warned, true, 30.0, 63).unwrap();
        assert!((stalled.shown - held.shown - 0.1).abs() < 1e-6);
        // It ends at 2.0 s and stays ended: the same warning never restarts.
        let mut banner = Some(stalled);
        let mut frame = 63;
        while banner.unwrap().visible() {
            frame += 1;
            banner = banner_step(banner, &warned, true, frame_secs, frame);
            assert!(frame < 200);
        }
        assert!(banner.unwrap().shown >= BOSS_BANNER_SECS);
        // 1.1 s were shown; the other 0.9 s take 54 frames.
        assert!((53..=55).contains(&(frame - 63)), "{frame}");
        let after = banner_step(
            banner,
            &notice(7, 400, Some((false, 98))),
            true,
            0.016,
            frame + 1,
        );
        assert_eq!(after.unwrap().shown, banner.unwrap().shown);
        // The arrival replaces the warning, mid-banner too, for 1.5 s.
        let arrived = notice(7, 398, Some((true, 398)));
        let swapped = banner_step(Some(held), &arrived, true, 0.016, 70).unwrap();
        assert_eq!(
            (swapped.key, swapped.arrived, swapped.shown),
            ((7, 398), true, 0.0)
        );
        assert_eq!(swapped.duration(), BOSS_ARRIVED_SECS);
        assert_eq!(start.duration(), BOSS_BANNER_SECS);
        assert_eq!((BOSS_BANNER_SECS, BOSS_ARRIVED_SECS), (2.0, 1.5));
    }

    #[test]
    fn banner_step_ignores_stale_events_and_other_attempts() {
        // A warning first seen more than 2 s of battle time late: no banner.
        let stale = notice(7, 98 + FRESH_TICKS + 1, Some((false, 98)));
        assert_eq!(banner_step(None, &stale, true, 0.016, 1), None);
        let edge = notice(7, 98 + FRESH_TICKS, Some((false, 98)));
        assert!(banner_step(None, &edge, true, 0.016, 1).is_some());
        // A stale arrival leaves a running warning banner alone.
        let running = BossBanner {
            key: (7, 98),
            arrived: false,
            shown: 0.5,
            frame: 1,
        };
        let late = notice(7, 1_000, Some((true, 398)));
        let kept = banner_step(Some(running), &late, true, 0.05, 2).unwrap();
        assert_eq!((kept.key, kept.arrived), ((7, 98), false));
        assert!((kept.shown - 0.55).abs() < 1e-6);
        // Another attempt's banner is dropped, even with the same tick.
        assert_eq!(
            banner_step(Some(running), &notice(8, 98, None), true, 0.016, 2),
            None
        );
        let next = banner_step(
            Some(running),
            &notice(8, 99, Some((false, 98))),
            true,
            0.016,
            2,
        );
        assert_eq!(next.unwrap().key, (8, 98));
        assert_eq!(next.unwrap().shown, 0.0);
    }

    #[test]
    fn banner_alpha_fades_in_normal_and_is_static_in_reduced() {
        for duration in [BOSS_BANNER_SECS, BOSS_ARRIVED_SECS] {
            for n in 0..200 {
                let shown = duration * n as f32 / 200.0;
                assert_eq!(banner_alpha(shown, duration, true), 1.0, "{shown}");
                let alpha = banner_alpha(shown, duration, false);
                assert!((0.0..=1.0).contains(&alpha));
                // Away from the two ramp ends, where rounding decides.
                if (shown - 0.15).abs() > 0.01 && (shown - (duration - 0.30)).abs() > 0.01 {
                    let full = shown > 0.15 && shown < duration - 0.30;
                    assert_eq!(alpha == 1.0, full, "{shown} of {duration}: {alpha}");
                }
            }
            for reduced in [false, true] {
                assert_eq!(banner_alpha(duration, duration, reduced), 0.0);
                assert_eq!(banner_alpha(duration + 1.0, duration, reduced), 0.0);
            }
            assert!((banner_alpha(0.075, duration, false) - 0.5).abs() < 1e-5);
            assert!((banner_alpha(duration - 0.15, duration, false) - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn banner_rect_is_centred_and_dodges_the_occluder() {
        let area = Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0));
        let rect = banner_rect(area, 130.0, None);
        assert_eq!(
            rect,
            Rect::from_min_size(pos2(530.0, 130.0), vec2(380.0, 34.0))
        );
        // A narrow area: 0.6 of its width.
        let narrow = Rect::from_min_size(pos2(10.0, 0.0), vec2(300.0, 200.0));
        let rect = banner_rect(narrow, 6.0, None);
        assert!((rect.width() - 180.0).abs() < 1e-4 && rect.height() == 34.0);
        assert!((rect.center().x - narrow.center().x).abs() < 1e-4);
        // An occluder it would meet: it ends OCCLUDER_GAP short, full width.
        let panel = Rect::from_min_size(pos2(800.0, 132.0), vec2(280.0, 600.0));
        let rect = banner_rect(area, 130.0, Some(panel));
        assert_eq!(rect.right(), 800.0 - art::OCCLUDER_GAP);
        assert!((rect.width() - 380.0).abs() < 1e-4);
        // An occluder it clears: unmoved.
        let far = Rect::from_min_size(pos2(1144.0, 132.0), vec2(280.0, 600.0));
        assert_eq!(
            banner_rect(area, 130.0, Some(far)),
            banner_rect(area, 130.0, None)
        );
        // No room: it narrows and stays in the area.
        let tight = Rect::from_min_size(pos2(100.0, 0.0), vec2(200.0, 400.0));
        let rect = banner_rect(narrow, 6.0, Some(tight));
        assert_eq!(rect.left(), narrow.left());
        assert_eq!(rect.right(), 100.0 - art::OCCLUDER_GAP);
    }

    #[test]
    fn banner_text_names_the_boss() {
        let warned = notice(1, 0, Some((false, 0)));
        assert_eq!(
            banner_text(&warned, false),
            (
                "BOSS APPROACHING".to_owned(),
                Some("Gate Warden · arrives in 4 s".to_owned())
            )
        );
        assert_eq!(
            banner_text(&warned, true),
            ("GATE WARDEN HAS ARRIVED".to_owned(), None)
        );
    }
}
