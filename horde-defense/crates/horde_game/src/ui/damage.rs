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
    // Over a struck hostile the number rises from its drawn body.
    let [dx, dy] = crate::battle_view::struck_stagger(a, event);
    Some(Bubble {
        kind,
        value: event.value,
        at: [event.to[0] + dx, event.to[1] + dy],
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
pub(super) fn draw(ui: &egui::Ui, arena: Rect, f: &mut UiCtx) {
    if let Some(a) = f.game.attempt() {
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
    warm_glyphs(ui.ctx(), &mut state.warmed);
    let p = ui.painter_at(arena.effects_clip);
    for bubble in &stream.active {
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
                        0,
                        &mut state,
                        &settings,
                        review::LootCaptureMode::Disabled,
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
