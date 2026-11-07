//! Read-only, real-time presentation of already-resolved combat results.
use super::{
    UiCtx,
    battle::{KEEP_CLEAR_GAP, keep_clear_shift},
    review,
};
use crate::{
    art::{self, AMBER, GREEN, MUTED, RED, TEXT},
    battle_view::SceneBlend,
    settings::Settings,
};
use bevy_egui::egui::{self, Color32, FontId, Rect, pos2, vec2};
use horde_core::sim::{Attempt, Event, EventKind};
use std::collections::{BTreeMap, BTreeSet};
const LIFE: f64 = 0.9;
pub(crate) const LIMIT: usize = 24;
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
    /// The pellets of one shot that struck nothing: one tick, one muzzle.
    /// They share a single MISS (CF-1-12).
    Volley(u64, [u32; 2]),
}
#[derive(Clone, Debug)]
struct Bubble {
    kind: Kind,
    value: i32,
    at: [f32; 2],
    target: Target,
    started: f64,
    /// The side of the struck hostile's health bar the popup went to, once
    /// it met the bar; it then follows the bar there for the rest of its life.
    beside: Option<Side>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Right,
    Left,
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
    } else if kind == Kind::Miss {
        Target::Volley(event.tick, event.from.map(f32::to_bits))
    } else if event.to == a.defender_position() {
        Target::Defender
    } else {
        Target::Point(event.to.map(f32::to_bits))
    };
    // Over a struck hostile the number rises from its drawn body.
    let [dx, dy] = crate::battle_view::struck_stagger(a, event);
    let mut at = [event.to[0] + dx, event.to[1] + dy];
    if matches!(target, Target::Point(_) | Target::Volley(..)) {
        // A shot into open ground ends 40 yards out, far off a zoomed field:
        // its MISS sits where the tracer leaves the field (W1 review).
        at = on_field(event.from, at, [a.width_yards, f32::from(a.lane_count)]);
    }
    Some(Bubble {
        kind,
        value: event.value,
        at,
        target,
        started: 0.0,
        beside: None,
    })
}
/// `to` when it lies in the world box `[0, size[0]] x [0, size[1]]`;
/// otherwise where the segment from `from` leaves the box (`to` clamped into
/// it when `from` lies outside too).
fn on_field(from: [f32; 2], to: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    let inside = |p: [f32; 2]| (0..2).all(|i| (0.0..=size[i]).contains(&p[i]));
    if inside(to) {
        return to;
    }
    let t = if inside(from) {
        (0..2).fold(1.0_f32, |t, i| {
            let d = to[i] - from[i];
            let edge = if d > 0.0 { size[i] } else { 0.0 };
            if d == 0.0 {
                t
            } else {
                t.min((edge - from[i]) / d)
            }
        })
    } else {
        1.0
    };
    [0, 1].map(|i| (from[i] + (to[i] - from[i]) * t).clamp(0.0, size[i]))
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
            // A later pellet of a shot already showing its MISS adds none.
            if matches!(next.target, Target::Volley(..))
                && self.active.iter().any(|old| old.target == next.target)
            {
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
            // A steady aim misses through the same point every shot (core
            // shots are deterministic): its MISS refreshes in place instead
            // of stacking one per shot (W1 review).
            if matches!(next.target, Target::Volley(..)) {
                self.active
                    .retain(|old| !(old.kind == Kind::Miss && old.at == next.at));
            }
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
    warmed: Option<Warmed>,
}
/// The only scales the punch takes (W1-64). Every popup font size is a base
/// size times one of these, so the font atlas holds a fixed glyph set that
/// `warm_glyphs` rasterises once; a continuous scale rasterised new glyph
/// sizes on every frame and re-uploaded the whole atlas.
const SCALE_STEPS: [f32; 4] = [0.85, 1.0, 1.15, 1.3];
/// Every glyph a popup can draw, plus egui's overflow ellipsis.
const GLYPHS: &str = "0123456789-!ARMODGEFNIS\u{2026}";
const KINDS: [Kind; 7] = [
    Kind::Hit,
    Kind::Weak,
    Kind::Incoming,
    Kind::Armor,
    Kind::Dodge,
    Kind::Defend,
    Kind::Miss,
];
fn display_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("display".into()))
}
/// Unscaled point size of one popup kind.
fn base_size(kind: Kind, reduced: bool) -> f32 {
    match kind {
        Kind::Hit if reduced => 23.0,
        Kind::Hit => 27.0,
        Kind::Weak if reduced => 27.0,
        Kind::Weak => 33.0,
        Kind::Incoming => 26.0,
        Kind::Dodge => 19.0,
        Kind::Armor | Kind::Defend | Kind::Miss => 18.0,
    }
}
/// Every font size `draw_for` can ask for, each once.
fn popup_sizes() -> Vec<f32> {
    let mut sizes: Vec<f32> = KINDS
        .iter()
        .flat_map(|&kind| {
            SCALE_STEPS
                .iter()
                .map(move |&scale| base_size(kind, false) * scale)
                .chain([base_size(kind, true)])
        })
        .collect();
    sizes.sort_by(f32::total_cmp);
    sizes.dedup_by(|a, b| a.to_bits() == b.to_bits());
    sizes
}
/// The fonts generation `warm_glyphs` rasterised for: its pixels-per-point
/// and the atlas fill seen on the latest pass. Within one generation the fill
/// only grows; egui rebuilds an atlas that nears full, so a lower fill means
/// the warmed glyphs are gone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Warmed {
    pixels_per_point: f32,
    fill: f32,
}
/// The fonts of battle text whose characters change while it plays: enemy
/// labels (8, 10), HUD and overlay readouts (10-14, DPS 12), coin pop-ups
/// (15, 16), the body and button styles (16), the field labels (18) and the
/// monospace trap charges and terrain notes (10).
fn battle_text_fonts() -> impl Iterator<Item = FontId> {
    [8.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 18.0]
        .map(FontId::proportional)
        .into_iter()
        .chain([FontId::monospace(10.0)])
}
/// Printable ASCII plus the separators the battle readouts use.
fn battle_text_glyphs() -> String {
    (' '..='~')
        .chain(['\u{b7}', '\u{d7}', '\u{2026}'])
        .collect()
}
/// Rasterises the fixed popup glyph set at every popup size, and every
/// battle-text glyph in the battle text fonts, once per fonts generation, so
/// no later battle frame adds a glyph to the atlas (W1-64).
fn warm_glyphs(ctx: &egui::Context, warmed: &mut Option<Warmed>) {
    let pixels_per_point = ctx.pixels_per_point();
    let fill = || ctx.fonts(|fonts| fonts.font_atlas_fill_ratio());
    let seen = fill();
    let fill = if warmed.is_some_and(|w| w.pixels_per_point == pixels_per_point && seen >= w.fill) {
        seen
    } else {
        ctx.fonts_mut(|fonts| {
            for size in popup_sizes() {
                fonts.layout_no_wrap(GLYPHS.into(), display_font(size), Color32::WHITE);
            }
            let text = battle_text_glyphs();
            for font in battle_text_fonts() {
                fonts.layout_no_wrap(text.clone(), font, Color32::WHITE);
            }
        });
        fill()
    };
    *warmed = Some(Warmed {
        pixels_per_point,
        fill,
    });
}
/// The nearest of `SCALE_STEPS`.
fn snap(scale: f32) -> f32 {
    SCALE_STEPS
        .into_iter()
        .min_by(|a, b| (a - scale).abs().total_cmp(&(b - scale).abs()))
        .unwrap_or(1.0)
}
fn motion(age: f64, reduced: bool) -> (f32, f32, f32) {
    let t = (age / LIFE).clamp(0.0, 1.0) as f32;
    let scale = if reduced {
        1.0
    } else if age < 0.06 {
        snap(0.85 + 0.45 * (age as f32 / 0.06))
    } else if age < 0.18 {
        snap(1.0 + 0.30 * (1.0 - (age as f32 - 0.06) / 0.12).powi(2))
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
/// How far over its event point a popup starts at zoom 1.
const NUMBER_LIFT: f32 = 62.0;
/// How far a popup's centre stays over a struck hostile's health bar: half a
/// fresh popup with its outline (16 pt) plus a 5-pt gap.
const BAR_CLEARANCE: f32 = 21.0;
/// How far over its event point (a struck hostile's feet) a popup starts: the
/// fixed lift, or clear of the health bar where the zoom raises the bar past
/// it (W1 review). Every field up to 7x16 at both target sizes keeps the
/// fixed lift; only the 2.1-2.5x fields rise further.
fn number_lift(arena: art::ArenaProjection) -> f32 {
    NUMBER_LIFT.max(arena.lane_height() * art::HEALTH_BAR_LIFT + BAR_CLEARANCE)
}
/// The health bar `art::battlefield` paints this frame over the live hostile
/// a popup reports on: over its body where `BattleViewModel` draws it, at the
/// blended position plus its crowd stagger. A hostile walks on after the
/// hit, so this is not the bar over `bubble.at`, the body at the hit (W1
/// review). `drawn` caches `blend`'s positions of `a.enemies` for the frame.
fn struck_bar(
    a: &Attempt,
    arena: art::ArenaProjection,
    blend: SceneBlend,
    drawn: &mut Option<Vec<[f32; 2]>>,
    bubble: &Bubble,
) -> Option<Rect> {
    let Target::Actor(id) = bubble.target else {
        return None;
    };
    let index = a.enemies.iter().position(|e| e.id == id)?;
    let [x, y] = drawn.get_or_insert_with(|| blend.positions(&a.enemies))[index];
    let [dx, dy] = crate::battle_view::crowd_stagger(id);
    let sy = arena.lane_height();
    Some(Rect::from_center_size(
        arena.world(x + dx, y + dy) + vec2(0.0, -sy * art::HEALTH_BAR_LIFT),
        vec2(
            if a.enemies[index].boss {
                sy * 1.1
            } else {
                sy * 0.65
            },
            3.0,
        ),
    ))
}
pub(super) fn draw(ui: &egui::Ui, arena: Rect, f: &mut UiCtx) {
    if let Some(a) = f.game.attempt() {
        draw_for(
            ui,
            a,
            art::ArenaProjection::for_attempt(arena, a)
                .with_effects_clip(f.battle.effects_rect.unwrap_or(arena)),
            f.battle.interpolation.scene(None, a),
            0,
            &mut f.battle.damage_feedback,
            f.settings,
            f.launch.damage_capture_mode(),
            &f.battle.effects_keep_clear,
        );
    }
}
/// Draws `battle`'s popups; none meets a rect of `keep_clear` (CF-1-09).
/// `blend` is the one the field painted `a`'s hostiles with this frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_for(
    ui: &egui::Ui,
    a: &Attempt,
    arena: art::ArenaProjection,
    blend: SceneBlend,
    battle: u64,
    state: &mut Feedback,
    settings: &Settings,
    mode: review::LootCaptureMode,
    keep_clear: &[Rect],
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
    warm_glyphs(ui.ctx(), &mut state.warmed);
    let p = ui.painter_at(arena.effects_clip);
    let mut positions = None;
    for bubble in &mut stream.active {
        let age = (now - bubble.started).max(0.0);
        let (scale, rise, fade) = motion(age, reduced);
        let (text, color) = match bubble.kind {
            Kind::Hit => (bubble.value.to_string(), TEXT),
            Kind::Weak => (format!("{}!", bubble.value), AMBER),
            Kind::Incoming => (format!("-{}", bubble.value), RED),
            Kind::Armor => ("ARMOR".into(), MUTED),
            Kind::Dodge => ("DODGE".into(), GREEN),
            Kind::Defend => ("DEFENDED".into(), GREEN),
            Kind::Miss => ("MISS".into(), MUTED),
        };
        let color = color.gamma_multiply(fade);
        let mut job = egui::text::LayoutJob::simple(
            text,
            display_font(base_size(bubble.kind, reduced) * scale),
            color,
            (arena.rect.width() - 12.0).clamp(1.0, 220.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = p.layout_job(job);
        let world = arena.world(bubble.at[0], bubble.at[1]);
        // A popup over open ground stays on the field; one over an actor may
        // rise into the forest and the headroom above the arena.
        let bounds = if matches!(bubble.target, Target::Point(_) | Target::Volley(..)) {
            arena.rect
        } else {
            arena.effects_clip
        };
        let center = pos2(
            world.x.clamp(
                bounds.left() + galley.size().x * 0.5 + 4.0,
                bounds.right() - galley.size().x * 0.5 - 4.0,
            ),
            (world.y - number_lift(arena) - rise).max(bounds.top() + galley.size().y * 0.5 + 4.0),
        );
        let border = if reduced { 1.0 } else { 2.0 };
        // The headroom is the HUD band's too: a popup that would meet a HUD
        // rect drops under it (CF-1-09), the same in reduced effects.
        let drawn = Rect::from_center_size(center, galley.size()).expand(border);
        let mut center = center + vec2(0.0, keep_clear_shift(drawn, keep_clear));
        // Dropped under the HUD, a top-lane number would sit on the struck
        // hostile's own health bar for its whole life: it goes beside the
        // bar instead, right or else left, still clear of the HUD (W1 review).
        // The bar walks on with the hostile, so the number follows the bar
        // painted this frame for the rest of its life, on the side it first
        // took while that side stays on the field.
        if let Some(bar) = struck_bar(a, arena, blend, &mut positions, bubble)
            && (bubble.beside.is_some()
                || Rect::from_center_size(center, galley.size())
                    .expand(border)
                    .intersects(bar))
        {
            let half = galley.size().x * 0.5 + border;
            let right = bar.right() + KEEP_CLEAR_GAP + half;
            let left = bar.left() - KEEP_CLEAR_GAP - half;
            // A side fits the number at its punch peak, so the punch never
            // pushes it across the bar.
            let peak = if reduced {
                1.0
            } else {
                SCALE_STEPS[SCALE_STEPS.len() - 1]
            };
            let width = galley.size().x * peak / scale + 2.0 * border;
            let fits_right = bar.right() + KEEP_CLEAR_GAP + width <= arena.rect.right();
            let fits_left = bar.left() - KEEP_CLEAR_GAP - width >= arena.rect.left();
            let side = match bubble.beside {
                Some(Side::Left) if fits_left || !fits_right => Side::Left,
                _ if fits_right => Side::Right,
                _ => Side::Left,
            };
            bubble.beside = Some(side);
            let x = if side == Side::Right { right } else { left };
            center = pos2(x, bar.center().y - rise);
            let beside = Rect::from_center_size(center, galley.size()).expand(border);
            center += vec2(0.0, keep_clear_shift(beside, keep_clear));
        }
        let origin = center - galley.size() * 0.5;
        let outline = Color32::from_black_alpha((240.0 * fade) as u8);
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
        game.campaign_mut().choose_start_country("DEU").unwrap();
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
        let a = game.attempt_mut().unwrap();
        a.events.clear();
        a.events.push_back(hit(a, 20, EventKind::Hit, 1));
        a.events.push_back(hit(a, 10, EventKind::WeakPoint, 2));
        let before = serde_json::to_value(&game).unwrap();
        let a = game.attempt().unwrap();
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
        let a = game.attempt_mut().unwrap();
        a.events.clear();
        // Two pellets with identical tick, target, value and geometry.
        a.events.push_back(hit(a, 20, EventKind::Hit, 1));
        a.events.push_back(hit(a, 20, EventKind::Hit, 2));
        let a = game.attempt().unwrap();
        let mut stream = Stream::default();
        stream.observe(a, 10.0, true, false);
        assert_eq!(stream.active.len(), 1);
        assert_eq!(stream.active[0].value, 40);
        stream.observe(a, 10.05, true, false);
        assert_eq!(stream.active[0].value, 40);
    }
    /// W1-69 review: a number over a hostile rises from its drawn,
    /// crowd-staggered body, alive or fallen this tick; a structure keeps the
    /// event point.
    #[test]
    fn numbers_over_a_hostile_anchor_on_its_drawn_body() {
        use crate::battle_view::crowd_stagger;
        let mut game = fixture();
        let mut a = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut a, horde_core::content::EnemyKind::Runner, 5, false);
        *game.attempt_slot_mut() = Some(a);
        let a = game.attempt_mut().unwrap();
        let live = a.enemies.last().unwrap().id;
        let at = [a.enemies.last().unwrap().x, a.enemies.last().unwrap().y()];
        let dead = a.next_id + 10;
        let structure = a.next_id + 20;
        a.corpses = vec![horde_core::sim::CorpseRecord {
            id: dead,
            kind: horde_core::content::EnemyKind::Runner,
            boss_kind: None,
            at: [12.0, 5.5],
            visual: None,
            tick: a.tick,
        }];
        let a = game.attempt().unwrap();
        let staggered = |at: [f32; 2], id| {
            let [dx, dy] = crowd_stagger(id);
            [at[0] + dx, at[1] + dy]
        };
        let struck = |to, target| Event {
            to,
            target_id: Some(target),
            ..hit(a, 20, EventKind::Hit, 1)
        };
        assert_ne!(staggered(at, live), at);
        assert_eq!(
            classify(a, &struck(at, live)).unwrap().at,
            staggered(at, live)
        );
        assert_eq!(
            classify(a, &struck([12.0, 5.5], dead)).unwrap().at,
            staggered([12.0, 5.5], dead)
        );
        assert_eq!(
            classify(a, &struck([9.0, 4.5], structure)).unwrap().at,
            [9.0, 4.5]
        );
        // A friendly miss (MISS) and a hostile's dodge (DODGE) rise from the
        // drawn body too.
        let missed = Event {
            kind: EventKind::Miss,
            value: 0,
            ..struck(at, live)
        };
        let dodged = Event {
            kind: EventKind::Defended,
            value: 0,
            ..struck(at, live)
        };
        let miss = classify(a, &missed).unwrap();
        assert_eq!(miss.kind, Kind::Miss);
        assert_eq!(miss.at, staggered(at, live));
        let dodge = classify(a, &dodged).unwrap();
        assert_eq!(dodge.kind, Kind::Dodge);
        assert_eq!(dodge.at, staggered(at, live));
    }
    /// W1-64: popups at staggered ages, every kind, over 120 egui passes at
    /// both budget sizes (UI scale 1.0 and 1.35), each at windows scale 1 and
    /// 2. After a short warmup no pass may
    /// add a glyph to the font atlas: bevy_egui writes every partial atlas
    /// update into `Assets<Image>`, so the render world re-extracts and
    /// re-uploads the whole atlas.
    #[test]
    fn damage_popups_stop_uploading_font_texture_after_warmup() {
        const WARMUP_PASSES: usize = 10;
        let kinds = [
            EventKind::Hit,
            EventKind::WeakPoint,
            EventKind::Damage,
            EventKind::Armor,
            EventKind::Miss,
            EventKind::Defended,
            EventKind::Melee,
        ];
        // The budget's two sizes, 1440x900 at UI scale 1.0 and 1050x700 at
        // 1.35, each at windows scale 1 and 2: pixels per point 1, 2, 1.35
        // and 2.7, the last two putting every popup size on a fractional
        // pixel size.
        for (native, zoom, window) in [
            (1.0, 1.0, vec2(1440.0, 900.0)),
            (2.0, 1.0, vec2(1440.0, 900.0)),
            (1.0, 1.35, vec2(1050.0, 700.0)),
            (2.0, 1.35, vec2(1050.0, 700.0)),
        ] {
            let pixels_per_point = native * zoom;
            let ctx = egui::Context::default();
            crate::settings::install_font(&ctx);
            crate::settings::configure(&ctx, zoom);
            let screen = window / zoom;
            let mut game = fixture();
            game.attempt_mut().unwrap().events.clear();
            let settings = Settings::default();
            let mut state = Feedback::default();
            let mut uploads = Vec::new();
            for pass in 0..120u64 {
                if pass % 3 == 0 {
                    let a = game.attempt_mut().unwrap();
                    let n = pass / 3;
                    let value = 1 + ((n * 7919) % 1499) as i32;
                    let mut event = hit(a, value, kinds[n as usize % kinds.len()], pass + 1);
                    event.target_id = Some(100 + n % 11);
                    a.events.push_back(event);
                }
                let a = game.attempt().unwrap();
                let mut input = egui::RawInput {
                    time: Some(pass as f64 / 60.0),
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), screen)),
                    ..Default::default()
                };
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .native_pixels_per_point = Some(native);
                let mut output = ctx.run_ui(input, |ui| {
                    // The canvas inset 24/124/24/84: 1392x692 and 729.8x310.5.
                    let arena = Rect::from_min_size(pos2(24.0, 124.0), screen - vec2(48.0, 208.0));
                    draw_for(
                        ui,
                        a,
                        art::ArenaProjection::for_attempt(arena, a),
                        SceneBlend::NONE,
                        0,
                        &mut state,
                        &settings,
                        review::LootCaptureMode::Disabled,
                        &[],
                    );
                });
                uploads.push(output.textures_delta.set.len());
                output.textures_delta.clear();
            }
            assert!(
                (ctx.pixels_per_point() - pixels_per_point).abs() < 1e-6,
                "the context ran at {} pixels per point, not {pixels_per_point}",
                ctx.pixels_per_point()
            );
            let late: Vec<_> = uploads
                .iter()
                .enumerate()
                .skip(WARMUP_PASSES)
                .filter(|(_, n)| **n > 0)
                .map(|(pass, _)| pass)
                .collect();
            let allowed = crate::perf_budget::EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX;
            assert_eq!(
                late.len().saturating_sub(allowed),
                0,
                "pixels_per_point {pixels_per_point}: font texture uploads after pass \
                 {WARMUP_PASSES} in passes {late:?}"
            );
        }
    }
    /// `game`'s attempt resized to a 3-lane, 12-yard field, the 2.5x zoom of
    /// the fit table, with no events.
    fn zoomed_fixture() -> Game {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.lane_count = 3;
        a.width_yards = 12.0;
        a.events.clear();
        game
    }
    /// One egui pass of `draw_for` over `a` in the 1440x900 battle layout of
    /// its geometry (first deployment, so with the hint band): the layout and
    /// the screen rect of every popup text shape drawn, outline included.
    fn drawn_popup(a: &Attempt) -> (crate::ui::battle::BattleLayout, Option<Rect>) {
        let ctx = egui::Context::default();
        crate::settings::install_font(&ctx);
        crate::settings::configure(&ctx, 1.0);
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
        let layout =
            crate::ui::battle::BattleLayout::new(canvas, a.lane_count, a.width_yards, true);
        let input = egui::RawInput {
            time: Some(0.0),
            screen_rect: Some(canvas),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            draw_for(
                ui,
                a,
                art::ArenaProjection::for_attempt(layout.field.rect, a)
                    .with_effects_clip(layout.effects_clip),
                SceneBlend::NONE,
                0,
                &mut Feedback::default(),
                &Settings::default(),
                review::LootCaptureMode::Disabled,
                &[],
            );
        });
        let drawn = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.rect.translate(text.pos.to_vec2())),
                _ => None,
            })
            .reduce(|a, b| a.union(b));
        output.textures_delta.clear();
        (layout, drawn)
    }
    /// W1 review: a manual shot that strikes nothing ends 40 yards out, far
    /// off a zoomed field. Its MISS sits where the tracer leaves the field,
    /// inside the field, never in the HUD band or the forest strips.
    #[test]
    fn a_missed_shot_popup_sits_on_the_field_where_its_tracer_leaves_it() {
        for angle in [0.3_f32, 0.0, -0.3] {
            let mut game = zoomed_fixture();
            let a = game.attempt_mut().unwrap();
            let from = a.defender_position();
            let to = [from[0] + 40.0 * angle.cos(), from[1] + 40.0 * angle.sin()];
            a.events.push_back(Event {
                kind: EventKind::Miss,
                from,
                to,
                value: 0,
                target_id: None,
                lane: None,
                ..hit(a, 0, EventKind::Miss, 1)
            });
            let a = game.attempt().unwrap();
            let (layout, drawn) = drawn_popup(a);
            let drawn = drawn.expect("a MISS popup is drawn");
            assert!(
                layout.field.rect.contains_rect(drawn) && layout.arena.contains_rect(drawn),
                "{angle}: MISS at {drawn:?} leaves the field {:?}",
                layout.field.rect
            );
            // Anchored where the from-to segment crosses the field's edge.
            let at = classify(a, a.events.back().unwrap()).unwrap().at;
            let off_tracer =
                ((at[0] - from[0]) * angle.sin() - (at[1] - from[1]) * angle.cos()).abs();
            assert!(off_tracer < 1e-4, "{angle}: {at:?} is off the tracer");
            assert!(
                at[0] == 12.0 || at[1] == 0.0 || at[1] == 3.0,
                "{angle}: {at:?} is not on the field's edge"
            );
        }
    }
    /// Fires one shot per entry of `aims` (a y offset from the defender at
    /// the field's right edge) from a `pellets`-pellet weapon into open
    /// ground, observing the stream 0.2 s apart: the MISS bubbles alive after
    /// each shot.
    fn misses_per_shot(pellets: u32, aims: &[f32]) -> Vec<usize> {
        let mut game = fixture();
        {
            let a = game.attempt_mut().unwrap();
            a.enemies.clear();
            a.events.clear();
            a.profile.weapon.pellets = pellets;
            a.profile.weapon.spread_radians = if pellets > 1 { 0.4 } else { 0.0 };
            a.ammo = a.magazine.max(3);
        }
        game.set_paused(false);
        let mut stream = Stream::default();
        let mut misses_per_shot = Vec::new();
        for (shot, &dy) in aims.iter().enumerate() {
            let before = game.attempt().unwrap().events.len();
            let mut ticks = 0;
            while game.attempt().unwrap().events.len() == before {
                let aim = {
                    let a = game.attempt().unwrap();
                    [a.width_yards, a.defender_position()[1] + dy]
                };
                game.tick(horde_core::sim::Input {
                    aim,
                    fire: true,
                    ..Default::default()
                });
                ticks += 1;
                assert!(ticks < 120, "shot {shot} never fired");
            }
            let a = game.attempt().unwrap();
            let missed = a
                .events
                .iter()
                .skip(before)
                .filter(|e| e.kind == EventKind::Miss)
                .count();
            assert_eq!(missed, pellets as usize, "shot {shot}: every pellet misses");
            stream.observe(a, 10.0 + shot as f64 * 0.2, true, false);
            misses_per_shot.push(
                stream
                    .active
                    .iter()
                    .filter(|bubble| bubble.kind == Kind::Miss)
                    .count(),
            );
        }
        misses_per_shot
    }
    /// CF-1-12: every pellet of a shotgun shot that strikes nothing is its
    /// own core Miss, and each made its own MISS at the field edge, a column
    /// of six per shot. One shot now shows one MISS (at its first pellet's
    /// edge point), and the next shot at another aim its own; reduced
    /// effects share the stream, so they show the same.
    #[test]
    fn a_shotgun_volley_that_misses_with_every_pellet_shows_one_miss() {
        assert_eq!(
            misses_per_shot(6, &[0.3, -0.3]),
            [1, 2],
            "one MISS per shot"
        );
    }
    /// W1 review: core shots are deterministic, so a steady aim misses
    /// through the same edge point every shot. Its MISS refreshes in place
    /// (one label), not a rising column of one per shot.
    #[test]
    fn steady_aim_misses_refresh_one_miss_instead_of_stacking() {
        assert_eq!(misses_per_shot(1, &[0.3, 0.3]), [1, 1], "single projectile");
        assert_eq!(misses_per_shot(6, &[0.3, 0.3]), [1, 1], "shotgun");
    }
    /// W1 review: the zoom raises the enemy health bar (0.41 lane heights
    /// over the feet) past a fixed lift, so on a 2.5x field a fresh number
    /// covered the readout it reports on.
    #[test]
    fn a_fresh_damage_number_clears_the_struck_enemy_health_bar_on_a_zoomed_field() {
        let mut game = zoomed_fixture();
        let mut a = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut a, horde_core::content::EnemyKind::Runner, 1, false);
        *game.attempt_slot_mut() = Some(a);
        let a = game.attempt_mut().unwrap();
        let enemy = a.enemies.last_mut().unwrap();
        enemy.x = 6.0;
        let (id, feet) = (enemy.id, [enemy.x, enemy.y()]);
        a.events.push_back(Event {
            to: feet,
            target_id: Some(id),
            lane: Some(1),
            ..hit(a, 20, EventKind::Hit, 1)
        });
        let a = game.attempt().unwrap();
        let (layout, drawn) = drawn_popup(a);
        let drawn = drawn.expect("a damage number is drawn");
        let arena = art::ArenaProjection::for_attempt(layout.field.rect, a);
        let sy = arena.lane_height();
        let body = classify(a, a.events.back().unwrap()).unwrap().at;
        // The bar `art::battlefield` paints over the drawn body.
        let bar = Rect::from_center_size(
            arena.world(body[0], body[1]) + vec2(0.0, -sy * art::HEALTH_BAR_LIFT),
            vec2(sy * 0.65, 3.0),
        );
        assert!(
            !drawn.intersects(bar),
            "the number at {drawn:?} covers the health bar at {bar:?}"
        );
    }
    /// W1 review: on the zoomed 7x16 field the field top is the arena top, so
    /// a top-lane number always meets a HUD top-band rect, and the keep-clear
    /// shift dropped it onto the struck enemy's own health bar for its whole
    /// life. It now meets neither, fresh, at the punch peak and late, at both
    /// target sizes, in normal and reduced effects, across the lane.
    ///
    /// The struck Runner walks toward the line between frames and is painted
    /// blended between the previous fixed step (four ticks back, the most one
    /// step advances) and now, so the bar checked is the one painted where
    /// the body is drawn on that frame, not where it was struck. A number
    /// beside the bar stays beside it, on one side, for the rest of its life.
    #[test]
    fn a_top_lane_number_kept_clear_of_the_hud_clears_the_struck_enemy_health_bar() {
        /// The Runner's speed in content.json, yards per second.
        const RUNNER_SPEED: f32 = 1.1;
        /// Ticks between the painted previous position and now.
        const STEP_TICKS: u64 = 4;
        /// The frame's blend between the previous position and now.
        const FRACTION: f32 = 0.5;
        for (size, zoom) in [
            (vec2(1440.0, 900.0), 1.0),
            (vec2(1050.0, 700.0) / 1.35, 1.35),
        ] {
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), size);
            for reduced in [false, true] {
                for x in [0.4_f32, 3.2, 8.0, 12.8, 15.6] {
                    for kind in [EventKind::Hit, EventKind::WeakPoint] {
                        let mut game = fixture();
                        let mut a = game.attempt_slot_mut().take().unwrap();
                        a.lane_count = 7;
                        a.width_yards = 16.0;
                        a.events.clear();
                        a.enemies.clear();
                        a.tick = 100;
                        game.spawn_enemy(&mut a, horde_core::content::EnemyKind::Runner, 0, false);
                        *game.attempt_slot_mut() = Some(a);
                        let a = game.attempt_mut().unwrap();
                        let enemy = a.enemies.last_mut().unwrap();
                        enemy.x = x;
                        let (id, feet) = (enemy.id, [enemy.x, enemy.y()]);
                        a.events.push_back(Event {
                            to: feet,
                            target_id: Some(id),
                            lane: Some(0),
                            ..hit(a, 4632, kind, 1)
                        });
                        let hit_tick = a.tick;
                        // Where the Runner's feet are `t` seconds after the
                        // hit: it walks left until it meets the line.
                        let walked = |t: f32| x - (RUNNER_SPEED * t).min(x - 0.4);
                        let layout = crate::ui::battle::BattleLayout::new(
                            canvas,
                            a.lane_count,
                            a.width_yards,
                            false,
                        );
                        let keep_clear = layout.effects_keep_clear();
                        let arena = art::ArenaProjection::for_attempt(layout.field.rect, a)
                            .with_effects_clip(layout.effects_clip);
                        let sy = arena.lane_height();
                        let settings = Settings {
                            reduced_effects: reduced,
                            ..Settings::default()
                        };
                        let ctx = egui::Context::default();
                        crate::settings::install_font(&ctx);
                        crate::settings::configure(&ctx, zoom);
                        let mut state = Feedback::default();
                        let mut beside: Option<(f64, bool)> = None;
                        for age in [0.0, 0.06, 0.5, 0.85] {
                            let ticks = (age * 60.0_f64).round() as u64;
                            let a = game.attempt_mut().unwrap();
                            a.tick = hit_tick + ticks;
                            a.enemies[0].x = walked(age as f32);
                            let mut previous = a.clone();
                            previous.tick -= STEP_TICKS;
                            previous.enemies[0].x =
                                walked((ticks as f32 - STEP_TICKS as f32) / 60.0);
                            let mut interpolation =
                                crate::battle_view::ActorInterpolation::default();
                            interpolation.record([(None, &previous)]);
                            interpolation.fraction = FRACTION;
                            let a = game.attempt().unwrap();
                            // The bar `art::battlefield` paints over the body
                            // drawn on this frame.
                            let (before, now) = (previous.enemies[0].x, a.enemies[0].x);
                            let [dx, dy] = crate::battle_view::crowd_stagger(id);
                            let body = [
                                before + (now - before) * FRACTION + dx,
                                a.enemies[0].y() + dy,
                            ];
                            let bar = Rect::from_center_size(
                                arena.world(body[0], body[1])
                                    + vec2(0.0, -sy * art::HEALTH_BAR_LIFT),
                                vec2(sy * 0.65, 3.0),
                            );
                            let input = egui::RawInput {
                                time: Some(age),
                                screen_rect: Some(canvas),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| {
                                draw_for(
                                    ui,
                                    a,
                                    arena,
                                    interpolation.scene(None, a),
                                    0,
                                    &mut state,
                                    &settings,
                                    review::LootCaptureMode::Disabled,
                                    &keep_clear,
                                );
                            });
                            output.textures_delta.clear();
                            let label = format!(
                                "{kind:?} at x {x} in {size:?}, reduced {reduced}, age {age}"
                            );
                            let drawn = output
                                .shapes
                                .iter()
                                .filter_map(|clipped| match &clipped.shape {
                                    egui::Shape::Text(text) => {
                                        Some(text.galley.rect.translate(text.pos.to_vec2()))
                                    }
                                    _ => None,
                                })
                                .reduce(|a, b| a.union(b))
                                .unwrap_or_else(|| panic!("{label}: a number is drawn"));
                            assert!(
                                !drawn.intersects(bar),
                                "{label}: the number at {drawn:?} covers the health bar at {bar:?}"
                            );
                            for hud in keep_clear {
                                assert!(
                                    !drawn.intersects(hud),
                                    "{label}: the number at {drawn:?} meets HUD {hud:?}"
                                );
                            }
                            assert!(
                                layout.effects_clip.contains_rect(drawn),
                                "{label}: the number at {drawn:?} leaves the effects clip {:?}",
                                layout.effects_clip
                            );
                            // Beside the bar: level with it, a gap's width off
                            // one end.
                            let level = drawn.top() < bar.bottom() && bar.top() < drawn.bottom();
                            let near = |gap: f32| (0.0..=KEEP_CLEAR_GAP + 2.0).contains(&gap);
                            let on_right = near(drawn.left() - bar.right());
                            let on_left = near(bar.left() - drawn.right());
                            let now_beside = (level && (on_right || on_left)).then_some(on_right);
                            if let Some((since, was_right)) = beside {
                                assert_eq!(
                                    now_beside,
                                    Some(was_right),
                                    "{label}: beside the bar on the {} since age {since}, the \
                                     number at {drawn:?} left the bar at {bar:?}",
                                    if was_right { "right" } else { "left" }
                                );
                            } else {
                                beside = now_beside.map(|on_right| (age, on_right));
                            }
                        }
                    }
                }
            }
        }
    }
    /// The lift moves only on the 2.1-2.5x fields: 17x30, 11x20 and 7x16
    /// keep the fixed lift at both target sizes, with or without the hint
    /// band.
    #[test]
    fn number_lift_moves_only_on_fields_zoomed_past_seven_by_sixteen() {
        let canvases = [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0)),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1050.0, 700.0) / 1.35),
        ];
        for canvas in canvases {
            for band in [false, true] {
                for (lanes, width, raised) in [
                    (17, 30.0, false),
                    (11, 20.0, false),
                    (7, 16.0, false),
                    (5, 14.0, canvas.width() == 1440.0),
                    (3, 12.0, canvas.width() == 1440.0),
                ] {
                    let layout = crate::ui::battle::BattleLayout::new(canvas, lanes, width, band);
                    let arena =
                        art::ArenaProjection::with_geometry(layout.field.rect, lanes, width);
                    let lift = number_lift(arena);
                    let label = format!("{lanes}x{width} in {:?}, band {band}", canvas.size());
                    if raised {
                        assert!(lift > NUMBER_LIFT, "{label}: {lift}");
                    } else {
                        assert_eq!(lift, NUMBER_LIFT, "{label}");
                    }
                }
            }
        }
    }
    #[test]
    fn punch_is_large_then_settles_while_reduced_feedback_has_no_punch_or_drift() {
        assert!(motion(0.06, false).0 > 1.25);
        assert_eq!(motion(0.3, false).0, 1.0);
        assert!(motion(0.5, false).1 > motion(0.1, false).1);
        // W1-64: the punch steps through the fixed scales only, so every
        // drawn size is one `popup_sizes` warms.
        let mut steps = Vec::new();
        for frame in 0..=60 {
            let scale = motion(frame as f64 / 60.0, false).0;
            assert!(SCALE_STEPS.contains(&scale), "{scale} at frame {frame}");
            if steps.last() != Some(&scale) {
                steps.push(scale);
            }
            for kind in KINDS {
                for (reduced, scale) in [(false, scale), (true, 1.0)] {
                    let size = base_size(kind, reduced) * scale;
                    assert!(popup_sizes().iter().any(|s| s.to_bits() == size.to_bits()));
                }
            }
        }
        assert_eq!(steps, [0.85, 1.0, 1.15, 1.3, 1.15, 1.0]);
        for age in [0.0, 0.05, 0.3, 0.8] {
            let (scale, rise, _) = motion(age, true);
            assert_eq!((scale, rise), (1.0, 0.0));
        }
    }
}
