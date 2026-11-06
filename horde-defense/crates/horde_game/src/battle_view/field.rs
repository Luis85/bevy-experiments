use super::interpolation::SceneBlend;
use crate::{
    animation::{ActorPose, Appearance, DefenderPose, Particle, SceneAnimation, StructurePose},
    visual_assets::enemy_visuals::{self, EnemySprite, EnemyVisualCatalog},
};
use horde_core::{
    content::{EnemyKind, StructureKind, content},
    enemy_catalog::EnemyVisual,
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, CorpseRecord, Enemy, EnemyCondition, EnemyPhase, EventKind},
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
/// The settled bodies a view draws: the newest `CORPSES_SETTLED_DRAWN` of the
/// settled prefix of `a.corpses`, `a.corpses[start..start + count]`. Their
/// views are not materialised per pass: the painter's cached mesh builds
/// only the bodies it lacks, through [`SettledCorpses::view`].
#[derive(Clone, Copy)]
pub struct SettledCorpses {
    /// Id of the first drawn body.
    pub first_id: Option<u64>,
    pub start: usize,
    pub count: usize,
    reduced: bool,
    catalog: &'static EnemyVisualCatalog,
}
impl SettledCorpses {
    fn new(
        a: &Attempt,
        settled: usize,
        reduced: bool,
        catalog: &'static EnemyVisualCatalog,
    ) -> Self {
        let settled = settled.min(a.corpses.len());
        let start = settled.saturating_sub(crate::perf_budget::CORPSES_SETTLED_DRAWN);
        Self {
            first_id: a.corpses[start..settled].first().map(|c| c.id),
            start,
            count: settled - start,
            reduced,
            catalog,
        }
    }
    /// The `n`th drawn settled body of `a`, the attempt this view was built from.
    pub fn view(&self, a: &Attempt, n: usize) -> Option<CorpseView> {
        if n >= self.count {
            return None;
        }
        let record = a.corpses.get(self.start + n)?;
        Some(body_view(
            self.catalog,
            content().levels[a.level].boss,
            BodyPose {
                at: record.at,
                appearance: Some(Appearance {
                    kind: record.kind,
                    boss: record.boss_kind.is_some(),
                    boss_kind: record.boss_kind,
                }),
                visual: record.visual.as_ref(),
                frame: 0,
                lean: if self.reduced { 0. } else { -0.85 },
                collapse: 1.,
            },
        ))
    }
}
impl PartialEq for SettledCorpses {
    fn eq(&self, other: &Self) -> bool {
        (self.first_id, self.start, self.count, self.reduced)
            == (other.first_id, other.start, other.count, other.reduced)
            && std::ptr::eq(self.catalog, other.catalog)
    }
}
impl std::fmt::Debug for SettledCorpses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettledCorpses")
            .field("first_id", &self.first_id)
            .field("start", &self.start)
            .field("count", &self.count)
            .field("reduced", &self.reduced)
            .finish_non_exhaustive()
    }
}
/// What a body view is built from: a sampled collapse or a settled record.
struct BodyPose<'a> {
    at: [f32; 2],
    appearance: Option<Appearance>,
    visual: Option<&'a EnemyVisual>,
    frame: u8,
    lean: f32,
    collapse: f32,
}
fn body_view(
    catalog: &'static EnemyVisualCatalog,
    level_boss: horde_core::content::BossKind,
    body: BodyPose,
) -> CorpseView {
    let boss = body.appearance.is_some_and(|v| v.boss);
    let sprite = body.visual.map_or_else(
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
        tint: body.visual.map(|v| v.tint),
        height: figure_height(catalog, boss, body.visual, art),
        frame: body.frame,
        lean: body.lean,
        collapse: body.collapse,
    }
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
    /// Settled bodies, drawn from the painter's cached mesh.
    pub settled: SettledCorpses,
    /// Collapsing bodies only, oldest first.
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
    /// The field view of `a`. `motion` is consumed: its particles move into
    /// the view rather than being cloned.
    pub fn build(a: &Attempt, motion: SceneAnimation, blend: SceneBlend, reduced: bool) -> Self {
        Self::build_with(a, motion, blend, reduced, enemy_visuals::active())
    }
    pub fn build_with(
        a: &Attempt,
        mut motion: SceneAnimation,
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
            .enumerate()
            .map(|(index, s)| StructureView {
                id: s.id,
                kind: s.kind,
                position: [s.x(), s.y()],
                footprint_width: s.footprint_width(),
                pose: aligned(&motion.structures, index, s.id).expect("a sampled structure"),
                hp: s.hp,
                max_hp: s.max_hp,
                charges: s.charges,
            })
            .collect();
        let settled = SettledCorpses::new(a, motion.settled_corpses, reduced, catalog);
        let corpses = motion
            .deaths
            .iter()
            .enumerate()
            .map(|(n, body)| {
                body_view(
                    catalog,
                    level_boss,
                    BodyPose {
                        at: body.at,
                        appearance: body.appearance,
                        visual: death_record(a, motion.settled_corpses + n, body.id)
                            .and_then(|record| record.visual.as_ref()),
                        frame: body.frame,
                        lean: body.lean,
                        collapse: body.collapse,
                    },
                )
            })
            .collect();
        // Indexed by attempt order, like the pose store.
        let positions = blend.positions(&a.enemies);
        let mut visible: Vec<(usize, &Enemy)> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| e.condition(a.mode) != EnemyCondition::Defeated && e.in_view())
            .collect();
        // Passing offsets are authoritative geometry. Painter order follows actual
        // feet rather than creation order when a faster actor passes behind another.
        visible.sort_by(|(_, a), (_, b)| a.y().total_cmp(&b.y()).then(a.id.cmp(&b.id)));
        let enemies = visible
            .into_iter()
            .map(|(index, e)| {
                let pose = aligned(&motion.enemies, index, e.id).expect("a sampled enemy");
                enemy_view(a, e, pose, positions[index], reduced, catalog)
            })
            .collect();
        Self {
            tick: a.tick,
            lanes: a.lane_count,
            width: a.width_yards,
            construction_bounds: a.construction_bounds(),
            construction_cell_width: a.construction_cell_width(),
            structures,
            settled,
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
            // Only events some painter can still draw (`art::fx_lifetime`).
            fx: a
                .events
                .iter()
                .filter(|e| {
                    a.tick.saturating_sub(e.tick) as f32 / 60.0 <= crate::art::fx_lifetime(e.kind)
                })
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
            particles: std::mem::take(&mut motion.particles),
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

/// The value `store` holds for `id`, the `index`th actor of the attempt it
/// was sampled from: read in place when the store lines up with the attempt,
/// else searched for by id.
fn aligned<T: Copy>(store: &[(u64, T)], index: usize, id: u64) -> Option<T> {
    match store.get(index) {
        Some(&(stored, value)) if stored == id => Some(value),
        _ => {
            #[cfg(test)]
            super::builds::record_id_lookup();
            store
                .iter()
                .find(|(stored, _)| *stored == id)
                .map(|&(_, value)| value)
        }
    }
}
/// The saved record of the collapsing body `id`, which sampling took from
/// `a.corpses[index]`; searched for by id if the attempt changed since.
fn death_record(a: &Attempt, index: usize, id: u64) -> Option<&CorpseRecord> {
    match a.corpses.get(index) {
        Some(record) if record.id == id => Some(record),
        _ => {
            #[cfg(test)]
            super::builds::record_id_lookup();
            a.corpses.iter().find(|record| record.id == id)
        }
    }
}

fn enemy_view(
    a: &Attempt,
    e: &Enemy,
    pose: ActorPose,
    position: [f32; 2],
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
        position,
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
