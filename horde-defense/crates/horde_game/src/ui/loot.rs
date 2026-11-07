//! Bounded, real-time feedback for already-banked model receipts. No rewards
//! or authoritative random state are produced by this presentation cache.
use super::{UiCtx, armory, battle::keep_clear_shift, review};
use crate::art;
use bevy_egui::egui::{self, Color32, FontId, Rect, Stroke, pos2, vec2};
use horde_core::quality::{LootEvent, Quality};
use std::collections::{BTreeMap, BTreeSet};
const LIFETIME: f64 = 1.5;
pub(crate) const LIMIT: usize = 12;
#[derive(Clone)]
struct Flyup {
    receipt: LootEvent,
    started: f64,
}
#[derive(Default)]
struct Stream {
    attempt: Option<u64>,
    seen: BTreeSet<u64>,
    active: Vec<Flyup>,
    last_seen: f64,
}
impl Stream {
    fn observe(&mut self, attempt: u64, tick: u64, receipts: &[LootEvent], now: f64) -> bool {
        self.last_seen = now;
        if self.attempt != Some(attempt) {
            self.attempt = Some(attempt);
            self.seen.clear();
            self.active.clear();
        }
        self.active.retain(|e| now - e.started < LIFETIME);
        let current: BTreeSet<_> = receipts.iter().map(|r| r.item_id).collect();
        self.seen.retain(|id| current.contains(id));
        let mut rare = false;
        for receipt in receipts {
            if self.seen.insert(receipt.item_id) && tick.saturating_sub(receipt.tick) <= 120 {
                rare |= receipt.quality >= Quality::Rare;
                self.active.push(Flyup {
                    receipt: receipt.clone(),
                    started: now,
                });
            }
        }
        self.active.sort_by(|a, b| {
            b.receipt
                .quality
                .cmp(&a.receipt.quality)
                .then(b.receipt.item_id.cmp(&a.receipt.item_id))
        });
        self.active.truncate(LIMIT);
        rare
    }
}
#[derive(Default)]
pub(super) struct Feedback {
    streams: BTreeMap<u64, Stream>,
    review_start: Option<u64>,
}
impl Feedback {
    /// Evidence replay changes only this cache. The receipt and its already-
    /// owned item stay in the immutable attempt/campaign exactly as recorded.
    fn observe_for_capture(
        &mut self,
        attempt: &horde_core::sim::Attempt,
        battle: u64,
        now: f64,
        frame: u64,
        mode: review::LootCaptureMode,
        ready: bool,
    ) -> (bool, f64) {
        let eligible = battle == 0 && mode != review::LootCaptureMode::Disabled;
        let restart = eligible
            && self.review_start.is_none()
            && ready
            && (mode == review::LootCaptureMode::Still || frame >= 48);
        if restart {
            self.review_start = Some(frame);
        }
        let clock = if eligible {
            self.review_start.map(|start| {
                if mode == review::LootCaptureMode::Still {
                    0.5
                } else {
                    frame.saturating_sub(start) as f64 / 60.0
                }
            })
        } else {
            None
        };
        let clock = clock.unwrap_or(now);
        let rare = self.observe(
            battle,
            attempt.id,
            attempt.tick,
            &attempt.loot_events,
            clock,
        );
        if restart && let Some(receipt) = attempt.loot_events.last() {
            let stream = self.streams.get_mut(&battle).expect("observed stream");
            stream.active.clear();
            stream.active.push(Flyup {
                receipt: receipt.clone(),
                started: 0.0,
            });
            stream
                .seen
                .extend(attempt.loot_events.iter().map(|event| event.item_id));
        }
        (rare, clock)
    }
    fn observe(
        &mut self,
        battle: u64,
        attempt: u64,
        tick: u64,
        receipts: &[LootEvent],
        now: f64,
    ) -> bool {
        if !self.streams.contains_key(&battle)
            && self.streams.len() >= 9
            && let Some(key) = self
                .streams
                .iter()
                .min_by(|a, b| a.1.last_seen.total_cmp(&b.1.last_seen))
                .map(|(key, _)| *key)
        {
            self.streams.remove(&key);
        }
        self.streams
            .entry(battle)
            .or_default()
            .observe(attempt, tick, receipts, now)
    }
}
pub(super) fn draw(ui: &egui::Ui, arena: Rect, f: &mut UiCtx) {
    let Some(attempt) = f.game.attempt() else {
        return;
    };
    let projection = art::ArenaProjection::for_attempt(arena, attempt)
        .with_effects_clip(f.battle.effects_rect.unwrap_or(arena));
    let capture = f.launch.loot_capture_mode();
    if draw_for(
        ui,
        projection,
        attempt,
        0,
        f.textures,
        &mut f.battle.loot_feedback,
        capture,
        &f.battle.effects_keep_clear,
    ) {
        f.notify_rare_loot();
    }
}
/// The centre of a drop's icon `age` (0..=1) into its rise: 12 + 78 * age pt
/// above the feet at world `at`, drifting sideways by item id. The rise stops
/// where the quality label, 29 pt above the centre, meets the top of the
/// effects clip, so a top-lane drop is never cut off while it still shows.
pub(super) fn drop_icon_center(
    projection: art::ArenaProjection,
    at: [f32; 2],
    item_id: u64,
    age: f32,
) -> egui::Pos2 {
    let drift = ((item_id % 3) as f32 - 1.0) * age * 8.0;
    let center = projection.world(at[0], at[1]) + vec2(drift, -12.0 - age * 78.0);
    pos2(
        center.x,
        center
            .y
            .max(projection.effects_clip.top() + LABEL_RISE + 4.0),
    )
}
/// Where a drop's rise starts: the drawn body of the kill that dropped it,
/// the core kill point plus that hostile's [`crowd_stagger`], so the drop
/// rises beside its kill coin (W1 review). The receipt names no enemy, but
/// the body recorded the same tick at the same point is the one; with no
/// such body (a legacy receipt) the rise keeps the core point.
///
/// [`crowd_stagger`]: crate::battle_view::crowd_stagger
fn drop_origin(attempt: &horde_core::sim::Attempt, receipt: &LootEvent) -> [f32; 2] {
    let since = attempt
        .corpses
        .partition_point(|corpse| corpse.tick < receipt.tick);
    attempt.corpses[since..]
        .iter()
        .take_while(|corpse| corpse.tick == receipt.tick)
        .find(|corpse| corpse.at == receipt.at)
        .map_or(receipt.at, |corpse| {
            let [dx, dy] = crate::battle_view::crowd_stagger(corpse.id);
            [receipt.at[0] + dx, receipt.at[1] + dy]
        })
}
/// How far above the icon centre the quality label starts.
const LABEL_RISE: f32 = 29.0;
/// Draws `battle`'s fly-ups; none meets a rect of `keep_clear` (CF-1-09).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_for(
    ui: &egui::Ui,
    projection: art::ArenaProjection,
    attempt: &horde_core::sim::Attempt,
    battle: u64,
    textures: crate::visual_assets::ArtTextures,
    state: &mut Feedback,
    capture: review::LootCaptureMode,
    keep_clear: &[Rect],
) -> bool {
    let ready = textures.actors.is_some()
        && textures.props.is_some()
        && textures.keyart.is_some()
        && textures.ground.is_some()
        && textures.walkers.is_some()
        && textures.rig.is_some()
        && textures.menu_idle.is_some();
    let (rare, now) = state.observe_for_capture(
        attempt,
        battle,
        ui.input(|i| i.time),
        ui.ctx().cumulative_frame_nr(),
        capture,
        ready,
    );
    let painter = ui.painter_at(projection.effects_clip);
    for entry in &state.streams[&battle].active {
        let age = ((now - entry.started) / LIFETIME).clamp(0.0, 1.0) as f32;
        let fade = (1.0 - age).powf(0.65);
        let origin = drop_origin(attempt, &entry.receipt);
        let at = drop_icon_center(projection, origin, entry.receipt.item_id, age);
        let color = armory::quality_color(entry.receipt.quality).gamma_multiply(fade);
        let text = if entry.receipt.quality >= Quality::Rare {
            format!("{} · {}", entry.receipt.quality.name(), entry.receipt.name)
        } else {
            entry.receipt.quality.name().to_owned()
        };
        let mut job = egui::text::LayoutJob::simple(text, FontId::proportional(10.0), color, 180.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = painter.layout_job(job);
        let text_at = pos2(
            (at.x - galley.size().x * 0.5).clamp(
                projection.effects_clip.left() + 4.0,
                projection.effects_clip.right() - galley.size().x - 4.0,
            ),
            (at.y - LABEL_RISE).max(projection.effects_clip.top() + 4.0),
        );
        let icon = Rect::from_center_size(at, vec2(25.0, 25.0));
        // The headroom is the HUD band's too: a fly-up whose icon or label
        // would meet a HUD rect drops under it as one (CF-1-09).
        let drop = vec2(
            0.0,
            keep_clear_shift(
                icon.union(Rect::from_min_size(text_at, galley.size())),
                keep_clear,
            ),
        );
        let (icon, text_at) = (icon.translate(drop), text_at + drop);
        painter.rect_filled(icon, 0.0, Color32::from_black_alpha((180.0 * fade) as u8));
        armory::paint_item_icon(
            &painter,
            textures,
            entry
                .receipt
                .definition
                .index()
                .expect("validated loot receipt"),
            icon.shrink(3.0),
            Color32::WHITE.gamma_multiply(fade),
        );
        painter.rect_stroke(
            icon,
            0.0,
            Stroke::new(
                if entry.receipt.quality >= Quality::Rare {
                    2.0
                } else {
                    1.0
                },
                color,
            ),
            egui::StrokeKind::Inside,
        );
        painter.galley(text_at, galley, color);
    }
    rare
}
#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::content::Mode;
    use horde_core::sim::Game;
    use horde_core::survivors::SurvivorId;
    fn receipt(id: u64, quality: Quality) -> LootEvent {
        LootEvent {
            item_id: id,
            definition: horde_core::content::ItemDefId::at(4),
            level: 1,
            quality,
            name: "Rifle".into(),
            tick: 100,
            at: [20.0, 5.5],
        }
    }
    #[test]
    fn review_replays_one_banked_receipt_after_warmup_without_changing_game_items_coins_or_rng() {
        let mut game = Game::default();
        game.campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let id = game
            .campaign_mut()
            .add_item_with_quality(4, 1, Some(Quality::Epic))
            .unwrap();
        let attempt = game.attempt_mut().unwrap();
        let mut event = receipt(id, Quality::Epic);
        event.tick = attempt.tick;
        attempt.loot_events.push(event);
        let before = serde_json::to_value(&game).unwrap();
        let attempt = game.attempt().unwrap();
        let mut feedback = Feedback::default();
        feedback.observe_for_capture(attempt, 0, 10.0, 1, review::LootCaptureMode::Motion, false);
        feedback.observe_for_capture(attempt, 0, 20.0, 47, review::LootCaptureMode::Motion, true);
        assert!(feedback.streams[&0].active.is_empty());
        assert_eq!(feedback.review_start, None);
        let (_, clock) = feedback.observe_for_capture(
            attempt,
            0,
            25.0,
            48,
            review::LootCaptureMode::Motion,
            true,
        );
        assert_eq!(clock, 0.0);
        assert_eq!(feedback.streams[&0].active.len(), 1);
        let (_, clock) = feedback.observe_for_capture(
            attempt,
            0,
            30.0,
            60,
            review::LootCaptureMode::Motion,
            true,
        );
        assert_eq!(clock, 0.2);
        assert_eq!(feedback.streams[&0].active.len(), 1);
        assert_eq!(feedback.review_start, Some(48));
        feedback.observe_for_capture(
            attempt,
            0,
            100.0,
            140,
            review::LootCaptureMode::Motion,
            true,
        );
        assert!(feedback.streams[&0].active.is_empty());
        assert_eq!(serde_json::to_value(&game).unwrap(), before);
    }
    #[test]
    fn static_evidence_can_hold_mid_fade_while_normal_gameplay_keeps_real_time_expiry() {
        let mut game = Game::default();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let attempt = game.attempt_mut().unwrap();
        attempt.loot_events.push(receipt(6, Quality::Rare));
        let mut feedback = Feedback::default();
        let (_, time) = feedback.observe_for_capture(
            attempt,
            0,
            20.0,
            12,
            review::LootCaptureMode::Still,
            true,
        );
        assert_eq!(time, 0.5);
        let (_, time) = feedback.observe_for_capture(
            attempt,
            0,
            30.0,
            50,
            review::LootCaptureMode::Still,
            true,
        );
        assert_eq!(time, 0.5);
        assert_eq!(feedback.streams[&0].active.len(), 1);
        feedback.observe_for_capture(
            attempt,
            0,
            32.0,
            51,
            review::LootCaptureMode::Disabled,
            true,
        );
        assert!(feedback.streams[&0].active.is_empty());
    }
    /// W1 review: a drop rises from the drawn, crowd-staggered body of the
    /// kill that dropped it, beside its kill coin; a receipt whose body is
    /// gone (or never recorded) keeps the core point.
    #[test]
    fn a_drop_from_a_staggered_kill_starts_over_the_drawn_body() {
        use crate::battle_view::crowd_stagger;
        let mut game = Game::default();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let a = game.attempt_mut().unwrap();
        let dead = a.next_id + 10;
        let neighbour = a.next_id + 11;
        let event = receipt(6, Quality::Rare);
        let body = |id, at| horde_core::sim::CorpseRecord {
            id,
            kind: horde_core::content::EnemyKind::Runner,
            boss_kind: None,
            at,
            visual: None,
            tick: event.tick,
        };
        // A neighbour fell the same tick a yard off; only the body under the
        // receipt's point owns the drop.
        a.corpses = vec![body(neighbour, [19.0, 5.5]), body(dead, event.at)];
        let a = game.attempt().unwrap();
        let [dx, dy] = crowd_stagger(dead);
        let drawn = [event.at[0] + dx, event.at[1] + dy];
        assert_ne!(drawn, event.at, "the fixture body is staggered");
        let projection = art::ArenaProjection::for_attempt(
            Rect::from_min_size(pos2(0., 0.), vec2(800., 400.)),
            a,
        );
        assert_eq!(
            drop_icon_center(projection, drop_origin(a, &event), 6, 0.0),
            drop_icon_center(projection, drawn, 6, 0.0),
        );
        let stray = LootEvent {
            tick: event.tick + 1,
            ..event.clone()
        };
        assert_eq!(drop_origin(a, &stray), stray.at, "no body, core point");
    }
    #[test]
    fn independent_workers_with_equal_attempt_and_item_ids_do_not_share_feedback_identity() {
        let mut feedback = Feedback::default();
        let events = vec![receipt(1, Quality::Rare)];
        assert!(feedback.observe(8, 1, 100, &events, 0.0));
        assert!(feedback.observe(9, 1, 100, &events, 0.1));
        assert!(!feedback.observe(8, 1, 101, &events, 0.2));
        assert_eq!(feedback.streams.len(), 2);
    }
    #[test]
    fn receipts_show_once_for_real_time_duration_independent_of_observation_speed() {
        let mut state = Stream::default();
        let events = vec![receipt(1, Quality::Rare)];
        assert!(state.observe(7, 100, &events, 10.0));
        assert_eq!(state.active.len(), 1);
        assert!(!state.observe(7, 104, &events, 10.25));
        assert_eq!(state.active.len(), 1);
        assert!(!state.observe(7, 160, &events, 11.4));
        assert_eq!(state.active.len(), 1);
        state.observe(7, 200, &events, 11.6);
        assert!(state.active.is_empty());
        assert!(!state.observe(7, 201, &events, 11.7));
        assert!(state.active.is_empty());
    }
    #[test]
    fn visual_bursts_are_bounded_and_prioritize_high_quality_without_changing_receipts() {
        let mut events: Vec<_> = (1..=30).map(|id| receipt(id, Quality::Common)).collect();
        events.push(receipt(31, Quality::Legendary));
        let before = events.clone();
        let mut state = Stream::default();
        state.observe(1, 100, &events, 0.0);
        assert_eq!(state.active.len(), LIMIT);
        assert_eq!(state.active[0].receipt.quality, Quality::Legendary);
        assert_eq!(events, before);
        state.observe(2, 100, &[], 0.1);
        assert!(state.active.is_empty());
        assert!(state.seen.is_empty());
    }
}
