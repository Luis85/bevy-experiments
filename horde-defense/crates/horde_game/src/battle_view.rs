//! Presentation seam between the simulation and the egui battlefield painter.
//! `BattleViewModel` is a pure per-frame snapshot of one attempt plus its
//! cosmetic caches; it is built headlessly and never written back to the core.
//! Actor positions blend between the last two fixed ticks by the fixed clock's
//! overstep, so motion stays smooth at any display rate.
use crate::{
    animation::{ActorPose, DefenderPose, Particle, SceneAnimation, StructurePose},
    launch::LaunchConfig,
    runner::{GameSession, RuntimeSet},
    ui::BattleUi,
    visual_assets::enemy_visuals::{self, EnemySprite, EnemyVisualCatalog},
};
use bevy::prelude::*;
use horde_core::{
    content::{EnemyKind, StructureKind, content},
    enemy_catalog::EnemyVisual,
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, Enemy, EnemyCondition, EnemyPhase, EventKind},
};
use std::collections::BTreeMap;

/// Observation speed advances at most this many whole ticks per fixed step.
const MAX_TICKS_PER_STEP: u64 = 4;

/// Records pre-tick positions and the frame's blend fraction for `BattleUi`.
pub struct BattleViewPlugin;
impl Plugin for BattleViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            record_tick_snapshots.before(RuntimeSet::Advance),
        )
        .add_systems(Update, update_blend_fraction);
    }
}

/// Runs before each fixed step's ticks: what is drawn as "previous".
fn record_tick_snapshots(session: Res<GameSession>, mut battle: ResMut<BattleUi>) {
    let game = &session.game;
    battle.interpolation.record(
        game.attempt.iter().map(|a| (None, a)).chain(
            game.battles
                .iter()
                .filter_map(|b| b.game.attempt.as_ref().map(|a| (Some(b.id), a))),
        ),
    );
}

fn update_blend_fraction(
    time: Res<Time<Fixed>>,
    config: Res<LaunchConfig>,
    mut battle: ResMut<BattleUi>,
) {
    battle.interpolation.fraction = blend_fraction(&config, time.overstep_fraction());
}

/// Fixed-step exports and automated screenshots draw the authoritative tick
/// exactly (fraction 1), so their frames do not depend on wall-clock overstep.
pub fn blend_fraction(config: &LaunchConfig, overstep: f32) -> f32 {
    if config.sequence_path().is_some() || config.capture_path().is_some() {
        1.0
    } else if overstep.is_finite() {
        overstep.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[derive(Clone, Debug)]
struct TickSnapshot {
    /// `None` for the active attempt, else the parallel battle id.
    scope: Option<u64>,
    attempt: u64,
    tick: u64,
    positions: BTreeMap<u64, [f32; 2]>,
}

/// Presentation-only cache of the positions each actor had before the latest
/// fixed step, keyed by scene and actor id.
#[derive(Clone, Debug)]
pub struct ActorInterpolation {
    pub fraction: f32,
    snapshots: Vec<TickSnapshot>,
}
impl Default for ActorInterpolation {
    fn default() -> Self {
        Self {
            fraction: 1.0,
            snapshots: Vec::new(),
        }
    }
}
impl ActorInterpolation {
    pub fn record<'a>(&mut self, attempts: impl IntoIterator<Item = (Option<u64>, &'a Attempt)>) {
        self.snapshots.clear();
        for (scope, a) in attempts {
            self.snapshots.push(TickSnapshot {
                scope,
                attempt: a.id,
                tick: a.tick,
                positions: a.enemies.iter().map(|e| (e.id, [e.x, e.y()])).collect(),
            });
        }
    }
    /// The blend for one drawn attempt; identity unless a snapshot of this
    /// scene was taken within the latest fixed step.
    pub fn scene(&self, scope: Option<u64>, a: &Attempt) -> SceneBlend<'_> {
        let snapshot = self.snapshots.iter().find(|s| {
            s.scope == scope
                && s.attempt == a.id
                && s.tick < a.tick
                && a.tick - s.tick <= MAX_TICKS_PER_STEP
        });
        SceneBlend {
            fraction: self.fraction,
            previous: snapshot.map(|s| &s.positions),
        }
    }
}

/// Copyable blend handle for one attempt's frame.
#[derive(Clone, Copy, Debug)]
pub struct SceneBlend<'a> {
    fraction: f32,
    previous: Option<&'a BTreeMap<u64, [f32; 2]>>,
}
impl SceneBlend<'_> {
    /// Draw authoritative positions exactly.
    #[cfg(test)]
    pub const NONE: SceneBlend<'static> = SceneBlend {
        fraction: 1.0,
        previous: None,
    };
    pub fn position(&self, id: u64, current: [f32; 2]) -> [f32; 2] {
        match self.previous.and_then(|previous| previous.get(&id)) {
            Some(before) if self.fraction < 1.0 => [
                before[0] + (current[0] - before[0]) * self.fraction,
                before[1] + (current[1] - before[1]) * self.fraction,
            ],
            _ => current,
        }
    }
}

/// Figure height in lanes: `factor`, times the archetype scale when present.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FigureHeight {
    pub factor: f32,
    pub scale: Option<f32>,
}
impl FigureHeight {
    /// Multiplication order is part of the pixel contract.
    pub fn pixels(self, lane_height: f32) -> f32 {
        match self.scale {
            Some(scale) => lane_height * self.factor * scale,
            None => lane_height * self.factor,
        }
    }
}
fn figure_height(
    catalog: &EnemyVisualCatalog,
    boss: bool,
    visual: Option<&EnemyVisual>,
    sprite: Option<&EnemySprite>,
) -> FigureHeight {
    if boss {
        FigureHeight {
            factor: catalog.boss_height,
            scale: None,
        }
    } else {
        FigureHeight {
            factor: sprite.map_or(catalog.default_height, |s| s.height),
            scale: visual.map(|v| v.scale),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactView {
    /// Attack readiness in 0..=1 for the contact gauge.
    pub ready: f32,
    pub recovering: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HealthView {
    Hidden,
    Critical,
    Bar { fraction: f32, boss: bool },
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyView {
    pub id: u64,
    pub kind: EnemyKind,
    pub boss: bool,
    pub lane: u8,
    /// Interpolated world position (yards, lanes) of the feet.
    pub position: [f32; 2],
    pub pose: ActorPose,
    pub sprite: Option<&'static EnemySprite>,
    /// RGBA in 0..=1; `None` draws untinted.
    pub tint: Option<[f32; 4]>,
    pub height: FigureHeight,
    /// Procedural fallback figure: lane-relative size and gait phase.
    pub fallback_scale: f32,
    pub phase: f32,
    pub contact: Option<ContactView>,
    pub windup: Option<f32>,
    pub hit_flash: Option<f32>,
    pub stunned: bool,
    pub health: HealthView,
    pub label: Option<&'static str>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CorpseView {
    pub at: [f32; 2],
    /// Legacy bodies without an appearance draw only a ground shadow.
    pub has_figure: bool,
    pub sprite: Option<&'static EnemySprite>,
    pub tint: Option<[f32; 4]>,
    pub height: FigureHeight,
    pub frame: u8,
    pub lean: f32,
    pub collapse: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructureView {
    pub id: u64,
    pub kind: StructureKind,
    pub position: [f32; 2],
    pub footprint_width: f32,
    pub pose: StructurePose,
    pub hp: i32,
    pub max_hp: i32,
    pub charges: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireZoneView {
    pub id: u64,
    pub center: [f32; 2],
    pub radius: f32,
    pub ticks_left: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileView {
    pub id: u64,
    pub kind: OrdnanceKind,
    pub target: [f32; 2],
    /// Flight progress in 0..=1.
    pub flight: f32,
}
/// One recent event to draw, aged in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxView {
    pub kind: EventKind,
    pub age: f32,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub value: i32,
    pub lane: Option<u8>,
    pub actor_id: Option<u64>,
    pub target_id: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefenderView {
    pub position: [f32; 2],
    pub pose: DefenderPose,
}

/// Everything the battlefield painter draws for one attempt and frame.
#[derive(Clone, Debug, PartialEq)]
pub struct BattleViewModel {
    pub tick: u64,
    pub lanes: u8,
    pub width: f32,
    pub construction_bounds: [f32; 2],
    pub construction_cell_width: f32,
    pub structures: Vec<StructureView>,
    pub corpses: Vec<CorpseView>,
    /// Visible enemies in painter order (feet, then id).
    pub enemies: Vec<EnemyView>,
    pub fire_zones: Vec<FireZoneView>,
    pub projectiles: Vec<ProjectileView>,
    pub fx: Vec<FxView>,
    pub particles: Vec<Particle>,
    pub defender: DefenderView,
}
impl BattleViewModel {
    pub fn build(a: &Attempt, motion: &SceneAnimation, blend: SceneBlend, reduced: bool) -> Self {
        Self::build_with(a, motion, blend, reduced, enemy_visuals::active())
    }
    pub fn build_with(
        a: &Attempt,
        motion: &SceneAnimation,
        blend: SceneBlend,
        reduced: bool,
        catalog: &'static EnemyVisualCatalog,
    ) -> Self {
        let level_boss = content().levels[a.level].boss;
        let structures = a
            .structures
            .iter()
            .map(|s| StructureView {
                id: s.id,
                kind: s.kind,
                position: [s.x(), s.y()],
                footprint_width: s.footprint_width(),
                pose: motion.structures[&s.id],
                hp: s.hp,
                max_hp: s.max_hp,
                charges: s.charges,
            })
            .collect();
        let corpses = motion
            .deaths
            .iter()
            .map(|body| {
                let boss = body.appearance.is_some_and(|v| v.boss);
                let sprite = body.visual.as_ref().map_or_else(
                    || {
                        body.appearance.map(|v| {
                            let boss = v.boss.then(|| v.boss_kind.unwrap_or(level_boss));
                            enemy_visuals::legacy_sprite(v.kind, boss)
                        })
                    },
                    |visual| Some(visual.sprite),
                );
                let art = sprite.and_then(|sprite| catalog.get(sprite));
                CorpseView {
                    at: body.at,
                    has_figure: body.appearance.is_some(),
                    sprite: art,
                    tint: body.visual.as_ref().map(|v| v.tint),
                    height: figure_height(catalog, boss, body.visual.as_ref(), art),
                    frame: body.frame,
                    lean: body.lean,
                    collapse: body.collapse,
                }
            })
            .collect();
        let mut visible: Vec<&Enemy> = a
            .enemies
            .iter()
            .filter(|e| e.condition(a.mode) != EnemyCondition::Defeated && e.in_view())
            .collect();
        // Passing offsets are authoritative geometry. Painter order follows actual
        // feet rather than creation order when a faster actor passes behind another.
        visible.sort_by(|a, b| a.y().total_cmp(&b.y()).then(a.id.cmp(&b.id)));
        let enemies = visible
            .into_iter()
            .map(|e| enemy_view(a, e, motion.enemies[&e.id], blend, reduced, catalog))
            .collect();
        Self {
            tick: a.tick,
            lanes: a.lane_count,
            width: a.width_yards,
            construction_bounds: a.construction_bounds(),
            construction_cell_width: a.construction_cell_width(),
            structures,
            corpses,
            enemies,
            fire_zones: a
                .ordnance
                .fire_zones
                .iter()
                .map(|zone| FireZoneView {
                    id: zone.id,
                    center: ordnance::cell_center(zone.cell),
                    radius: zone.radius,
                    ticks_left: zone.ticks_left,
                })
                .collect(),
            projectiles: a
                .ordnance
                .projectiles
                .iter()
                .map(|projectile| ProjectileView {
                    id: projectile.id,
                    kind: projectile.kind,
                    target: ordnance::cell_center(projectile.cell),
                    flight: (1.
                        - projectile.flight_left as f32 / projectile.flight_ticks.max(1) as f32)
                        .clamp(0., 1.),
                })
                .collect(),
            fx: a
                .events
                .iter()
                .map(|e| FxView {
                    kind: e.kind,
                    age: a.tick.saturating_sub(e.tick) as f32 / 60.0,
                    from: e.from,
                    to: e.to,
                    value: e.value,
                    lane: e.lane,
                    actor_id: e.actor_id,
                    target_id: e.target_id,
                })
                .collect(),
            particles: motion.particles.clone(),
            defender: DefenderView {
                position: a.defender_position(),
                pose: motion.defender,
            },
        }
    }
    pub fn structure(&self, id: u64) -> Option<&StructureView> {
        self.structures.iter().find(|s| s.id == id)
    }
}

fn enemy_view(
    a: &Attempt,
    e: &Enemy,
    pose: ActorPose,
    blend: SceneBlend,
    reduced: bool,
    catalog: &'static EnemyVisualCatalog,
) -> EnemyView {
    let level_boss = content().levels[a.level].boss;
    let visual = e.resolved.as_ref().map(|profile| &profile.visual);
    let sprite = visual.map_or_else(
        || {
            let boss = e.boss.then(|| e.boss_kind.unwrap_or(level_boss));
            enemy_visuals::legacy_sprite(e.kind, boss)
        },
        |visual| visual.sprite,
    );
    let sprite = catalog.get(sprite);
    let phase = if reduced || !pose.moving {
        0.0
    } else {
        a.elapsed()
            * if e.kind == EnemyKind::Runner {
                12.0
            } else {
                5.0
            }
            + e.id as f32
    };
    let contact = e.contact().then(|| {
        let recovering = e.attack_phase == EnemyPhase::Recovery;
        ContactView {
            ready: if recovering {
                (1. - e.attack_left as f32 / e.windup_total.max(1) as f32).clamp(0., 1.)
            } else {
                pose.windup
            },
            recovering,
        }
    });
    let health = if e.critically_wounded(a.mode) {
        HealthView::Critical
    } else if e.hp < e.max_hp || e.boss {
        HealthView::Bar {
            fraction: (e.hp.max(0) as f32 / e.max_hp as f32).clamp(0.0, 1.0),
            boss: e.boss,
        }
    } else {
        HealthView::Hidden
    };
    let label = e.boss.then(|| {
        if e.attack_phase == EnemyPhase::Recovery {
            "RECOVERING · WINDOW OPEN"
        } else if e.exposed_at(a.tick) {
            "EXPOSED · FIRE NOW"
        } else {
            match level_boss {
                horde_core::content::BossKind::Breacher => "BREACHER",
                horde_core::content::BossKind::Caller => "CALLER",
                horde_core::content::BossKind::SiegeBrute => "SIEGE BRUTE",
            }
        }
    });
    EnemyView {
        id: e.id,
        kind: e.kind,
        boss: e.boss,
        lane: e.lane,
        position: blend.position(e.id, [e.x, e.y()]),
        pose,
        sprite,
        tint: visual.map(|v| v.tint),
        height: figure_height(catalog, e.boss, visual, sprite),
        fallback_scale: if e.boss {
            0.57
        } else if e.kind == EnemyKind::Brute {
            0.44
        } else {
            0.34
        },
        phase,
        contact,
        windup: (e.attack_phase == EnemyPhase::Windup).then_some(pose.windup),
        hit_flash: (e.hit_stop_remaining() > 0).then(|| {
            (e.hit_stop_remaining() as f32 / horde_core::sim::ENEMY_HIT_STOP_TICKS as f32).sqrt()
        }),
        stunned: e.injury.stunned,
        health,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::ReviewOptions;
    use horde_core::{
        content::{BossKind, Mode},
        enemy_catalog::VisualSprite,
        sim::{Game, Input},
    };

    fn fixture() -> Game {
        let mut game = Game::default();
        game.campaign
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        game.campaign.attempts = 1;
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt.take().unwrap();
        game.spawn_enemy(&mut a, EnemyKind::Runner, 4, false);
        game.spawn_enemy(&mut a, EnemyKind::Brute, 1, false);
        game.spawn_enemy(&mut a, EnemyKind::Shambler, 2, true);
        for (n, e) in a.enemies.iter_mut().enumerate() {
            e.x = 18. + n as f32;
        }
        a.paused = false;
        game.attempt = Some(a);
        game
    }
    fn view(a: &Attempt, blend: SceneBlend) -> BattleViewModel {
        BattleViewModel::build(a, &SceneAnimation::sample(a, false), blend, false)
    }
    fn drawn_at(a: &Attempt, blend: SceneBlend, id: u64) -> [f32; 2] {
        view(a, blend)
            .enemies
            .into_iter()
            .find(|e| e.id == id)
            .unwrap()
            .position
    }

    #[test]
    fn view_model_mirrors_the_attempt_headlessly_without_mutating_it() {
        let mut game = fixture();
        for _ in 0..30 {
            game.tick(Input::default());
        }
        let a = game.attempt.as_ref().unwrap();
        let before = serde_json::to_string(a).unwrap();
        let view = view(a, SceneBlend::NONE);
        assert_eq!(before, serde_json::to_string(a).unwrap());
        assert_eq!(view.tick, a.tick);
        assert_eq!(view.lanes, a.lane_count);
        let visible = a
            .enemies
            .iter()
            .filter(|e| e.condition(a.mode) != EnemyCondition::Defeated && e.in_view())
            .count();
        assert_eq!(view.enemies.len(), visible);
        assert!(!view.enemies.is_empty());
        for pair in view.enemies.windows(2) {
            assert!(pair[0].position[1] <= pair[1].position[1]);
        }
        for actor in &view.enemies {
            let e = a.enemies.iter().find(|e| e.id == actor.id).unwrap();
            assert_eq!(actor.position, [e.x, e.y()]);
            assert_eq!(actor.kind, e.kind);
            let sprite = actor.sprite.expect("every catalogue sprite has art");
            let expected = e.resolved.as_ref().map_or_else(
                || {
                    let boss = e
                        .boss
                        .then(|| e.boss_kind.unwrap_or(content().levels[a.level].boss));
                    enemy_visuals::legacy_sprite(e.kind, boss)
                },
                |profile| profile.visual.sprite,
            );
            assert_eq!(sprite.sprite, expected);
            if e.boss {
                assert_eq!(actor.height.factor, enemy_visuals::active().boss_height);
                assert!(actor.label.is_some());
                assert!(matches!(actor.health, HealthView::Bar { boss: true, .. }));
            } else {
                assert_eq!(actor.height.factor, sprite.height);
                assert!(actor.label.is_none());
            }
        }
        assert_eq!(view.structures.len(), a.structures.len());
        assert_eq!(view.fx.len(), a.events.len());
        assert_eq!(view.defender.position, a.defender_position());
        assert_eq!(view.fire_zones.len(), a.ordnance.fire_zones.len());
        assert_eq!(view.projectiles.len(), a.ordnance.projectiles.len());
    }

    #[test]
    fn legacy_enemies_without_a_profile_draw_their_kind_or_level_boss() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
        for e in &mut a.enemies {
            e.resolved = None;
            e.boss_kind = None;
        }
        let view = view(a, SceneBlend::NONE);
        let level_boss = content().levels[a.level].boss;
        assert_eq!(view.enemies.len(), 3);
        for actor in &view.enemies {
            let boss = actor.boss.then_some(level_boss);
            assert_eq!(
                actor.sprite.unwrap().sprite,
                enemy_visuals::legacy_sprite(actor.kind, boss)
            );
            assert_eq!(actor.tint, None);
            assert_eq!(actor.height.scale, None);
        }
        let brute = view
            .enemies
            .iter()
            .find(|e| e.kind == EnemyKind::Brute)
            .unwrap();
        assert_eq!(brute.height.pixels(10.), 10. * 0.9);
        assert_eq!(
            enemy_visuals::legacy_sprite(EnemyKind::Runner, Some(BossKind::Caller)),
            VisualSprite::Caller
        );
    }

    #[test]
    fn actor_positions_blend_between_fixed_ticks_by_the_overstep() {
        let mut game = fixture();
        let mut cache = ActorInterpolation::default();
        cache.record([(None, game.attempt.as_ref().unwrap())]);
        let before: BTreeMap<u64, [f32; 2]> = game
            .attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .map(|e| (e.id, [e.x, e.y()]))
            .collect();
        for _ in 0..2 {
            game.tick(Input::default());
        }
        let a = game.attempt.as_ref().unwrap();
        let moved = a
            .enemies
            .iter()
            .find(|e| before[&e.id] != [e.x, e.y()])
            .expect("an enemy advanced");
        let (id, from, to) = (moved.id, before[&moved.id], [moved.x, moved.y()]);
        cache.fraction = 0.0;
        assert_eq!(drawn_at(a, cache.scene(None, a), id), from);
        cache.fraction = 0.5;
        let half = drawn_at(a, cache.scene(None, a), id);
        for axis in 0..2 {
            assert!((half[axis] - (from[axis] + to[axis]) * 0.5).abs() < 1e-4);
        }
        // Fraction 1 is the authoritative position exactly, as is a scene the
        // snapshot does not describe.
        cache.fraction = 1.0;
        assert_eq!(drawn_at(a, cache.scene(None, a), id), to);
        cache.fraction = 0.5;
        assert_eq!(drawn_at(a, cache.scene(Some(7), a), id), to);
        // A stale snapshot (more than one fixed step of ticks) is not blended.
        for _ in 0..MAX_TICKS_PER_STEP {
            game.tick(Input::default());
        }
        let a = game.attempt.as_ref().unwrap();
        let current = a.enemies.iter().find(|e| e.id == id).unwrap();
        assert_eq!(
            drawn_at(a, cache.scene(None, a), id),
            [current.x, current.y()]
        );
    }

    #[test]
    fn a_full_four_tick_fixed_step_still_blends_from_its_snapshot() {
        let mut game = fixture();
        let mut cache = ActorInterpolation::default();
        cache.record([(None, game.attempt.as_ref().unwrap())]);
        let before: BTreeMap<u64, [f32; 2]> = game
            .attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .map(|e| (e.id, [e.x, e.y()]))
            .collect();
        for _ in 0..MAX_TICKS_PER_STEP {
            game.tick(Input::default());
        }
        let a = game.attempt.as_ref().unwrap();
        let moved = a
            .enemies
            .iter()
            .find(|e| before[&e.id] != [e.x, e.y()])
            .expect("an enemy advanced");
        cache.fraction = 0.0;
        assert_eq!(
            drawn_at(a, cache.scene(None, a), moved.id),
            before[&moved.id]
        );
    }

    #[test]
    fn capture_modes_draw_the_authoritative_tick_exactly() {
        let play = LaunchConfig::default();
        assert_eq!(blend_fraction(&play, 0.25), 0.25);
        assert_eq!(blend_fraction(&play, 1.7), 1.0);
        assert_eq!(blend_fraction(&play, f32::NAN), 1.0);
        for review in [
            ReviewOptions {
                sequence: Some("target/sequence".into()),
                ..Default::default()
            },
            ReviewOptions {
                capture: Some("target/shot.png".into()),
                ..Default::default()
            },
        ] {
            let config = LaunchConfig {
                review: Some(review),
                ..Default::default()
            };
            for overstep in [0.0, 0.3, 0.99] {
                assert_eq!(blend_fraction(&config, overstep), 1.0);
            }
        }
    }
}
