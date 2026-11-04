//! Read-only, real-time presentation of already-resolved combat results.
use super::{UiCtx, review};
use crate::{
    art::{self, AMBER, GREEN, MUTED, RED, TEXT},
    settings::Settings,
};
use bevy_egui::egui::{self, Color32, FontId, Rect, pos2, vec2};
use horde_core::sim::{Attempt, Event, EventKind};
use std::collections::{BTreeMap, BTreeSet};
const LIFE: f64 = 0.9;
const LIMIT: usize = 24;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Hit,
    Weak,
    Incoming,
    Armor,
    Dodge,
    Defend,
    Miss,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Actor(u64),
    Defender,
    Point([u32; 2]),
}
#[derive(Clone, Debug)]
struct Bubble {
    kind: Kind,
    value: i32,
    at: [f32; 2],
    target: Target,
    started: f64,
}
fn classify(a: &Attempt, event: &Event) -> Option<Bubble> {
    let kind = match event.kind {
        EventKind::Hit | EventKind::Melee if event.value > 0 => Kind::Hit,
        EventKind::WeakPoint if event.value > 0 => Kind::Weak,
        EventKind::Damage if event.value > 0 => Kind::Incoming,
        EventKind::Armor => Kind::Armor,
        EventKind::Defended => {
            if event
                .target_id
                .is_some_and(|id| a.enemies.iter().any(|e| e.id == id))
            {
                Kind::Dodge
            } else {
                Kind::Defend
            }
        }
        EventKind::Miss => Kind::Miss,
        _ => return None,
    };
    let target = if let Some(id) = event.target_id {
        Target::Actor(id)
    } else if event.to == a.defender_position() {
        Target::Defender
    } else {
        Target::Point(event.to.map(f32::to_bits))
    };
    Some(Bubble {
        kind,
        value: event.value,
        at: event.to,
        target,
        started: 0.0,
    })
}
fn positive(kind: Kind) -> bool {
    matches!(kind, Kind::Hit | Kind::Weak | Kind::Incoming)
}
#[derive(Default)]
struct Stream {
    attempt: Option<u64>,
    /// Event sequence numbers already presented; unique within an attempt.
    seen: BTreeSet<u64>,
    active: Vec<Bubble>,
    last_seen: f64,
}
impl Stream {
    fn observe(&mut self, a: &Attempt, now: f64, enabled: bool, replay: bool) {
        if self.attempt != Some(a.id) {
            self.attempt = Some(a.id);
            self.seen.clear();
            self.active.clear();
        }
        if replay {
            self.seen.clear();
            self.active.clear();
        }
        self.last_seen = now;
        self.active.retain(|b| now - b.started < LIFE);
        let current: BTreeSet<_> = a.events.iter().map(|event| event.seq).collect();
        self.seen.retain(|seq| current.contains(seq));
        for event in &a.events {
            if !self.seen.insert(event.seq)
                || !enabled
                || (!replay && a.tick.saturating_sub(event.tick) > 120)
            {
                continue;
            }
            let Some(mut next) = classify(a, event) else {
                continue;
            };
            if replay && a.tick.saturating_sub(event.tick) > 240 {
                continue;
            }
            if let Some(old) = self.active.iter_mut().find(|old| {
                old.target == next.target
                    && positive(old.kind)
                    && positive(next.kind)
                    && now - old.started < 0.15
            }) {
                old.value = old.value.saturating_add(next.value);
                old.at = next.at;
                if next.kind == Kind::Weak {
                    old.kind = Kind::Weak;
                }
                continue;
            }
            self.active.retain(|old| old.target != next.target);
            next.started = now;
            self.active.push(next);
        }
        if !enabled {
            self.active.clear();
        }
        self.active.sort_by(|a, b| {
            priority(b.kind)
                .cmp(&priority(a.kind))
                .then(b.started.total_cmp(&a.started))
        });
        self.active.truncate(LIMIT);
    }
}
fn priority(kind: Kind) -> u8 {
    match kind {
        Kind::Weak => 4,
        Kind::Incoming => 3,
        Kind::Hit => 2,
        _ => 1,
    }
}
#[derive(Default)]
pub(super) struct Feedback {
    streams: BTreeMap<u64, Stream>,
    review_start: Option<u64>,
}
fn motion(age: f64, reduced: bool) -> (f32, f32, f32) {
    let t = (age / LIFE).clamp(0.0, 1.0) as f32;
    let scale = if reduced {
        1.0
    } else if age < 0.06 {
        0.85 + 0.45 * (age as f32 / 0.06)
    } else if age < 0.18 {
        1.0 + 0.30 * (1.0 - (age as f32 - 0.06) / 0.12).powi(2)
    } else {
        1.0
    };
    let rise = if reduced { 0.0 } else { t * 38.0 };
    let fade = if t < 0.6 {
        1.0
    } else {
        ((1.0 - t) / 0.4).clamp(0.0, 1.0)
    };
    (scale, rise, fade)
}
pub(super) fn draw(ui: &egui::Ui, arena: Rect, f: &mut UiCtx) {
    if let Some(a) = f.game.attempt.as_ref() {
        draw_for(
            ui,
            a,
            art::ArenaProjection::for_attempt(arena, a)
                .with_effects_clip(f.battle.effects_rect.unwrap_or(arena)),
            0,
            &mut f.battle.damage_feedback,
            f.settings,
            f.launch.damage_capture_mode(),
        );
    }
}
pub(super) fn draw_for(
    ui: &egui::Ui,
    a: &Attempt,
    arena: art::ArenaProjection,
    battle: u64,
    state: &mut Feedback,
    settings: &Settings,
    mode: review::LootCaptureMode,
) {
    let (enabled, reduced) = (settings.numbers, settings.reduced_effects);
    let frame = ui.ctx().cumulative_frame_nr();
    let replay = battle == 0
        && mode != review::LootCaptureMode::Disabled
        && state.review_start.is_none()
        && (mode == review::LootCaptureMode::Still || frame >= 56);
    if replay {
        state.review_start = Some(frame);
    }
    let now = if mode != review::LootCaptureMode::Disabled && battle == 0 {
        state
            .review_start
            .map(|start| {
                if mode == review::LootCaptureMode::Still {
                    0.06
                } else {
                    frame.saturating_sub(start) as f64 / 60.0
                }
            })
            .unwrap_or_else(|| ui.input(|i| i.time))
    } else {
        ui.input(|i| i.time)
    };
    if !state.streams.contains_key(&battle)
        && state.streams.len() >= 9
        && let Some(key) = state
            .streams
            .iter()
            .min_by(|a, b| a.1.last_seen.total_cmp(&b.1.last_seen))
            .map(|(key, _)| *key)
    {
        state.streams.remove(&key);
    }
    let stream = state.streams.entry(battle).or_default();
    stream.observe(a, now, enabled, replay);
    let p = ui.painter_at(arena.effects_clip);
    for bubble in &stream.active {
        let age = (now - bubble.started).max(0.0);
        let (scale, rise, fade) = motion(age, reduced);
        let (text, color, size) = match bubble.kind {
            Kind::Hit => (
                bubble.value.to_string(),
                TEXT,
                if reduced { 23.0 } else { 27.0 },
            ),
            Kind::Weak => (
                format!("{}!", bubble.value),
                AMBER,
                if reduced { 27.0 } else { 33.0 },
            ),
            Kind::Incoming => (format!("-{}", bubble.value), RED, 26.0),
            Kind::Armor => ("ARMOR".into(), MUTED, 18.0),
            Kind::Dodge => ("DODGE".into(), GREEN, 19.0),
            Kind::Defend => ("DEFENDED".into(), GREEN, 18.0),
            Kind::Miss => ("MISS".into(), MUTED, 18.0),
        };
        let color = color.gamma_multiply(fade);
        let mut job = egui::text::LayoutJob::simple(
            text,
            FontId::new(size * scale, egui::FontFamily::Name("display".into())),
            color,
            (arena.rect.width() - 12.0).clamp(1.0, 220.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = p.layout_job(job);
        let world = arena.world(bubble.at[0], bubble.at[1]);
        let center = pos2(
            world.x.clamp(
                arena.effects_clip.left() + galley.size().x * 0.5 + 4.0,
                arena.effects_clip.right() - galley.size().x * 0.5 - 4.0,
            ),
            (world.y - 62.0 - rise).max(arena.effects_clip.top() + galley.size().y * 0.5 + 4.0),
        );
        let origin = center - galley.size() * 0.5;
        let outline = Color32::from_black_alpha((240.0 * fade) as u8);
        let border = if reduced { 1.0 } else { 2.0 };
        for direction in [
            vec2(-1.0, -1.0),
            vec2(0.0, -1.0),
            vec2(1.0, -1.0),
            vec2(-1.0, 0.0),
            vec2(1.0, 0.0),
            vec2(-1.0, 1.0),
            vec2(0.0, 1.0),
            vec2(1.0, 1.0),
        ] {
            p.galley_with_override_text_color(origin + direction * border, galley.clone(), outline);
        }
        p.galley(origin, galley, color);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::content::Mode;
    use horde_core::sim::Game;
    fn fixture() -> Game {
        let mut game = Game::default();
        game.campaign.choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        game
    }
    fn hit(a: &Attempt, value: i32, kind: EventKind, seq: u64) -> Event {
        Event {
            seq,
            kind,
            from: a.defender_position(),
            to: [12.0, 5.5],
            value,
            tick: a.tick,
            actor_id: None,
            target_id: Some(9),
            lane: Some(5),
            duration_ticks: 0,
        }
    }
    #[test]
    fn resolved_damage_merges_once_and_survives_accelerated_simulation_ticks_for_real_time() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
        a.events.clear();
        a.events.push_back(hit(a, 20, EventKind::Hit, 1));
        a.events.push_back(hit(a, 10, EventKind::WeakPoint, 2));
        let before = serde_json::to_value(&game).unwrap();
        let a = game.attempt.as_ref().unwrap();
        let mut stream = Stream::default();
        stream.observe(a, 10.0, true, false);
        assert_eq!(stream.active.len(), 1);
        assert_eq!(stream.active[0].value, 30);
        assert_eq!(stream.active[0].kind, Kind::Weak);
        stream.observe(a, 10.5, true, false);
        assert_eq!(stream.active[0].value, 30);
        stream.observe(a, 11.0, true, false);
        assert!(stream.active.is_empty());
        assert_eq!(serde_json::to_value(&game).unwrap(), before);
    }
    #[test]
    fn identical_results_are_distinct_by_sequence_and_each_presents_once() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
        a.events.clear();
        // Two pellets with identical tick, target, value and geometry.
        a.events.push_back(hit(a, 20, EventKind::Hit, 1));
        a.events.push_back(hit(a, 20, EventKind::Hit, 2));
        let a = game.attempt.as_ref().unwrap();
        let mut stream = Stream::default();
        stream.observe(a, 10.0, true, false);
        assert_eq!(stream.active.len(), 1);
        assert_eq!(stream.active[0].value, 40);
        stream.observe(a, 10.05, true, false);
        assert_eq!(stream.active[0].value, 40);
    }
    #[test]
    fn punch_is_large_then_settles_while_reduced_feedback_has_no_punch_or_drift() {
        assert!(motion(0.06, false).0 > 1.25);
        assert_eq!(motion(0.3, false).0, 1.0);
        assert!(motion(0.5, false).1 > motion(0.1, false).1);
        for age in [0.0, 0.05, 0.3, 0.8] {
            let (scale, rise, _) = motion(age, true);
            assert_eq!((scale, rise), (1.0, 0.0));
        }
    }
}
