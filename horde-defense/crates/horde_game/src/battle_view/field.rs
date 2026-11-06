use super::interpolation::SceneBlend;
use crate::{
    animation::{ActorPose, DefenderPose, Particle, SceneAnimation, StructurePose},
    visual_assets::enemy_visuals::{self, EnemySprite, EnemyVisualCatalog},
};
use horde_core::{
    content::{EnemyKind, StructureKind, content},
    enemy_catalog::EnemyVisual,
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, Enemy, EnemyCondition, EnemyPhase, EventKind},
};

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
        #[cfg(test)]
        super::builds::record(|b| &mut b.field);
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
