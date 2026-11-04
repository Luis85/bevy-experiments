//! Presentation seam between the simulation and the egui battlefield painter.
//! `BattleViewModel` is a pure per-frame snapshot of one attempt plus its
//! cosmetic caches; it is built headlessly and never written back to the core.
//! Actor positions blend between the last two fixed ticks by the fixed clock's
//! overstep, so motion stays smooth at any display rate.
//!
//! `BattleScreen` is the same seam for the battle screen's controls and
//! readouts around the painter: built from `&Game`, it carries every attempt
//! value the screen lays out, so egui code never reads the attempt itself.
use crate::{
    animation::{ActorPose, DefenderPose, Particle, SceneAnimation, StructurePose},
    launch::LaunchConfig,
    runner::{GameSession, RuntimeSet},
    ui::BattleUi,
    visual_assets::enemy_visuals::{self, EnemySprite, EnemyVisualCatalog},
};
use bevy::prelude::*;
use horde_core::{
    content::{EnemyKind, Mode, StructureKind, content},
    enemy_catalog::EnemyVisual,
    kind_map::KindMap,
    ordnance::{self, OrdnanceKind},
    quality::LootEvent,
    sim::{Attempt, Enemy, EnemyCondition, EnemyPhase, EventKind, Game},
    skill_catalog::unlocks,
    world::{StageTheme, world},
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
        game.attempt().into_iter().map(|a| (None, a)).chain(
            game.battles()
                .iter()
                .filter_map(|b| b.game().attempt().map(|a| (Some(b.id), a))),
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

/// The arena the battle screen projects onto: the attempt's, else the
/// campaign's next deployment (lanes, width in yards).
pub fn arena_geometry(game: &Game) -> (u8, f32) {
    game.attempt().map_or_else(
        || {
            (
                game.campaign().lane_count(),
                game.campaign().arena_geometry().1,
            )
        },
        |a| (a.lane_count, a.width_yards),
    )
}

/// The defender's magazine and reload clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponView {
    pub ammo: u32,
    pub magazine: u32,
    pub reload_left: u32,
    pub reload_total: u32,
}
impl WeaponView {
    pub fn of(a: &Attempt) -> Self {
        Self {
            ammo: a.ammo,
            magazine: a.magazine,
            reload_left: a.reload_left,
            reload_total: a.reload_duration_ticks(),
        }
    }
    /// Reload completion in 0..=1 while reloading.
    pub fn reload_progress(&self) -> Option<f32> {
        (self.reload_left > 0).then(|| {
            (1.0 - self.reload_left as f32 / self.reload_total.max(1) as f32).clamp(0.0, 1.0)
        })
    }
}

/// One throwable's unlock, stock and cooldown for the attack selector, the
/// target cursor and the reticle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrdnanceSlotView {
    pub unlocked: bool,
    pub charges: u32,
    pub issued: u32,
    pub cooldown: u32,
    /// Full cooldown in ticks after the profile multiplier, at least 1.
    pub cooldown_total: f32,
    pub ready: bool,
    /// Blast radius in yards after the profile multiplier.
    pub radius: f32,
}
impl OrdnanceSlotView {
    pub fn of(a: &Attempt, kind: OrdnanceKind) -> Self {
        let definition = ordnance::catalog().definition(kind);
        let modifiers = &a.profile.modifiers;
        Self {
            unlocked: modifiers.unlocked(match kind {
                OrdnanceKind::Grenade => unlocks::GRENADE,
                OrdnanceKind::Molotov => unlocks::MOLOTOV,
            }),
            charges: a.ordnance.charges(kind),
            issued: a.ordnance.issued(kind),
            cooldown: a.ordnance.cooldown(kind),
            cooldown_total: (definition.cooldown_ticks as f32
                * modifiers.ordnance_cooldown_multiplier)
                .round()
                .clamp(1.0, 3600.0),
            ready: a.ordnance.ready(kind),
            radius: definition.radius * modifiers.ordnance_radius_multiplier,
        }
    }
}

/// The cursor badge: rounds or charges left, a reload or cooldown arc and a
/// short label.
#[derive(Clone, Debug, PartialEq)]
pub struct ReticleReadout {
    pub remaining: u32,
    pub capacity: u32,
    pub progress: Option<f32>,
    pub label: String,
    /// Observed (NPC) battles draw the neutral reticle colour.
    pub observed: bool,
}
impl ReticleReadout {
    /// The readout for `a`; a selected throwable applies to personal battles only.
    pub fn build(a: &Attempt, selected: Option<OrdnanceKind>) -> Self {
        let slot = selected.map(|kind| OrdnanceSlotView::of(a, kind));
        Self::from_parts(a.mode, &WeaponView::of(a), slot.as_ref())
    }
    fn from_parts(mode: Mode, weapon: &WeaponView, slot: Option<&OrdnanceSlotView>) -> Self {
        let reload = weapon.reload_progress();
        let observed = mode == Mode::Npc;
        match slot.filter(|_| mode == Mode::Personal) {
            Some(slot) => Self {
                remaining: slot.charges,
                capacity: slot.issued.max(1),
                progress: reload.or((slot.cooldown > 0)
                    .then(|| (1.0 - slot.cooldown as f32 / slot.cooldown_total).clamp(0.0, 1.0))),
                label: if reload.is_some() {
                    "RELOAD".into()
                } else {
                    format!("{} left", slot.charges)
                },
                observed,
            },
            None => Self {
                remaining: weapon.ammo,
                capacity: weapon.magazine.max(1),
                progress: reload,
                label: if reload.is_some() {
                    format!("R {:.1}s", weapon.reload_left as f32 / 60.0)
                } else {
                    format!("{}/{}", weapon.ammo, weapon.magazine)
                },
                observed,
            },
        }
    }
}

/// Which first-deployment field hint applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldHint {
    /// Nothing has happened yet.
    Opening,
    /// No shot fired.
    Shoot,
    /// Never reloaded and the magazine is not full.
    Reload,
    /// No enemy has reached the line.
    Contact,
    Banked,
}

/// Deployment facts for the field-details menu.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldReadout {
    pub threat_rank: u32,
    pub hostiles: usize,
    pub enemy_cap: usize,
    pub arrival_rate: f32,
    pub difficulty: &'static str,
    pub batch_size: u32,
    pub gear_level: u32,
    pub level_name: &'static str,
    pub hint: FieldHint,
    /// The country stage's local setting, when deployed to a country.
    pub setting: Option<StageTheme>,
}

/// Lines of the resolution log the field-details menu shows.
pub const LOG_TAIL: usize = 18;

/// Every attempt value the battle screen lays out around the painter.
#[derive(Clone, Debug, PartialEq)]
pub struct BattleScreen {
    pub mode: Mode,
    pub tick: u64,
    pub paused: bool,
    pub lanes: u8,
    pub width_yards: f32,
    /// Backdrop and terrain theme index.
    pub theme: usize,
    pub last_loot: Option<LootEvent>,
    pub weapon_class: &'static str,
    pub weapon: WeaponView,
    pub ordnance: KindMap<OrdnanceKind, OrdnanceSlotView>,
    pub heal: bool,
    pub brace: bool,
    pub melee_cooldown: u32,
    pub supply: u32,
    pub field: FieldReadout,
    /// Newest first, at most `LOG_TAIL` lines.
    pub log_tail: Vec<String>,
    /// Every enemy's position, in attempt order, for aim assist.
    pub targets: Vec<[f32; 2]>,
    pub blueprints: KindMap<StructureKind, bool>,
    pub global_paused: bool,
    /// The campaign has not finished a deployment yet.
    pub first_deployment: bool,
}
impl BattleScreen {
    /// The active battle's screen, or `None` outside a battle.
    pub fn build(game: &Game) -> Option<Self> {
        let a = game.attempt()?;
        let country = world().country(a.country_id.as_deref().unwrap_or(""));
        let theme = country
            .map(|country| match country.biome.to_lowercase().as_str() {
                "desert" | "arid" => 2,
                "tundra" | "polar" | "boreal" => 3,
                "tropical" | "rainforest" => 4,
                "mediterranean" => 1,
                _ => 0,
            })
            .unwrap_or(a.level);
        let hint = if a.tick == 0 {
            FieldHint::Opening
        } else if a.shots == 0 {
            FieldHint::Shoot
        } else if a.reload_ticks == 0 && a.ammo < a.magazine {
            FieldHint::Reload
        } else if a.first_contact.is_none() {
            FieldHint::Contact
        } else {
            FieldHint::Banked
        };
        Some(Self {
            mode: a.mode,
            tick: a.tick,
            paused: a.paused,
            lanes: a.lane_count,
            width_yards: a.width_yards,
            theme,
            last_loot: a.loot_events.last().cloned(),
            weapon_class: a.profile.weapon.class.name(),
            weapon: WeaponView::of(a),
            ordnance: KindMap::from_fn(|kind| OrdnanceSlotView::of(a, kind)),
            heal: a.profile.heal,
            brace: a.profile.brace,
            melee_cooldown: a.melee_cooldown,
            supply: a.supply,
            field: FieldReadout {
                threat_rank: a.progression.rank,
                hostiles: a.enemies.len(),
                enemy_cap: a.enemy_cap(),
                arrival_rate: a.arrival_rate(),
                difficulty: a.balance.preset.name(),
                batch_size: a.progression.batch_size,
                gear_level: a.progression.item_level,
                level_name: &content().levels[a.level].name,
                hint,
                setting: country.map(|country| country.stage_theme(a.stage)),
            },
            log_tail: a.log.iter().rev().take(LOG_TAIL).cloned().collect(),
            targets: a.enemies.iter().map(|e| [e.x, e.y()]).collect(),
            blueprints: game.campaign().blueprints.clone(),
            global_paused: game.global_paused(),
            first_deployment: game.campaign().attempts == 0,
        })
    }
    pub fn personal(&self) -> bool {
        self.mode == Mode::Personal
    }
    pub fn any_blueprint(&self) -> bool {
        self.blueprints.iter().any(|owned| *owned)
    }
    pub fn slot(&self, kind: OrdnanceKind) -> &OrdnanceSlotView {
        &self.ordnance[kind]
    }
    /// The reticle readout with `selected` throwable, if any.
    pub fn reticle(&self, selected: Option<OrdnanceKind>) -> ReticleReadout {
        ReticleReadout::from_parts(
            self.mode,
            &self.weapon,
            selected.map(|kind| self.slot(kind)),
        )
    }
    /// The 1-based lane the aim selects, clamped onto the arena.
    pub fn selected_lane(&self, aim: [f32; 2]) -> u8 {
        aim[1]
            .floor()
            .clamp(0.0, f32::from(self.lanes.saturating_sub(1))) as u8
            + 1
    }
    /// The nearest enemy within `radius` yards of `aim`, else `aim` itself.
    pub fn assist(&self, aim: [f32; 2], radius: f32) -> [f32; 2] {
        if radius <= 0.0 {
            return aim;
        }
        let distance = |p: &[f32; 2]| ((p[0] - aim[0]).powi(2) + (p[1] - aim[1]).powi(2)).sqrt();
        self.targets
            .iter()
            .filter(|p| distance(p) < radius)
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .copied()
            .unwrap_or(aim)
    }
}

/// The construction cell under the aim and whether a structure may go there.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementPreview {
    pub lane: u8,
    pub column: u8,
    pub center: [f32; 2],
    pub half_width: f32,
    pub valid: Result<(), String>,
    /// The structure already standing in the cell, which salvage removes.
    pub occupant: Option<u64>,
}
/// `None` outside a battle or when the aim is off the construction grid.
pub fn placement_preview(
    game: &Game,
    kind: StructureKind,
    aim: [f32; 2],
) -> Option<PlacementPreview> {
    let a = game.attempt()?;
    let [lane, column] = a.build_cell_at(aim)?;
    Some(PlacementPreview {
        lane,
        column,
        center: a.build_cell_center(lane, column).expect("validated cell"),
        half_width: a.construction_cell_width() * 0.5,
        valid: game.preview_placement(kind, lane, column),
        occupant: a
            .structures
            .iter()
            .find(|s| s.lane == lane && s.column == column)
            .map(|s| s.id),
    })
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
        game.campaign_mut()
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        game.campaign_mut().attempts = 1;
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut a, EnemyKind::Runner, 4, false);
        game.spawn_enemy(&mut a, EnemyKind::Brute, 1, false);
        game.spawn_enemy(&mut a, EnemyKind::Shambler, 2, true);
        for (n, e) in a.enemies.iter_mut().enumerate() {
            e.x = 18. + n as f32;
        }
        a.paused = false;
        *game.attempt_slot_mut() = Some(a);
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
        let a = game.attempt().unwrap();
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
        let a = game.attempt_mut().unwrap();
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
        cache.record([(None, game.attempt().unwrap())]);
        let before: BTreeMap<u64, [f32; 2]> = game
            .attempt()
            .unwrap()
            .enemies
            .iter()
            .map(|e| (e.id, [e.x, e.y()]))
            .collect();
        for _ in 0..2 {
            game.tick(Input::default());
        }
        let a = game.attempt().unwrap();
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
        let a = game.attempt().unwrap();
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
        cache.record([(None, game.attempt().unwrap())]);
        let before: BTreeMap<u64, [f32; 2]> = game
            .attempt()
            .unwrap()
            .enemies
            .iter()
            .map(|e| (e.id, [e.x, e.y()]))
            .collect();
        for _ in 0..MAX_TICKS_PER_STEP {
            game.tick(Input::default());
        }
        let a = game.attempt().unwrap();
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
    fn battle_screen_is_absent_outside_battle_and_geometry_falls_back_to_the_campaign() {
        let game = Game::default();
        assert_eq!(BattleScreen::build(&game), None);
        assert_eq!(
            placement_preview(&game, StructureKind::ALL[0], [3.0, 0.5]),
            None
        );
        assert_eq!(
            arena_geometry(&game),
            (
                game.campaign().lane_count(),
                game.campaign().arena_geometry().1
            )
        );
        let game = fixture();
        let a = game.attempt().unwrap();
        assert_eq!(arena_geometry(&game), (a.lane_count, a.width_yards));
    }

    #[test]
    fn battle_screen_mirrors_the_attempt_headlessly_without_mutating_it() {
        let mut game = fixture();
        for _ in 0..30 {
            game.tick(Input::default());
        }
        let a = game.attempt_mut().unwrap();
        a.log = (0..25).map(|n| format!("line {n}")).collect();
        let a = game.attempt().unwrap();
        let before = serde_json::to_string(a).unwrap();
        let view = BattleScreen::build(&game).unwrap();
        assert_eq!(before, serde_json::to_string(a).unwrap());
        assert_eq!(
            (
                view.mode,
                view.tick,
                view.paused,
                view.lanes,
                view.width_yards
            ),
            (a.mode, a.tick, a.paused, a.lane_count, a.width_yards)
        );
        assert!(view.personal());
        assert_eq!(view.last_loot.as_ref(), a.loot_events.last());
        assert_eq!(view.weapon_class, a.profile.weapon.class.name());
        assert_eq!(
            view.weapon,
            WeaponView {
                ammo: a.ammo,
                magazine: a.magazine,
                reload_left: a.reload_left,
                reload_total: a.reload_duration_ticks(),
            }
        );
        for kind in [OrdnanceKind::Grenade, OrdnanceKind::Molotov] {
            let slot = view.slot(kind);
            assert_eq!(slot.charges, a.ordnance.charges(kind));
            assert_eq!(slot.issued, a.ordnance.issued(kind));
            assert_eq!(slot.cooldown, a.ordnance.cooldown(kind));
            assert_eq!(slot.ready, a.ordnance.ready(kind));
            assert_eq!(
                slot.radius,
                ordnance::catalog().definition(kind).radius
                    * a.profile.modifiers.ordnance_radius_multiplier
            );
            assert!(slot.cooldown_total >= 1.0);
        }
        assert_eq!((view.heal, view.brace), (a.profile.heal, a.profile.brace));
        assert_eq!(
            (view.melee_cooldown, view.supply),
            (a.melee_cooldown, a.supply)
        );
        assert_eq!(view.field.threat_rank, a.progression.rank);
        assert_eq!(view.field.hostiles, a.enemies.len());
        assert_eq!(view.field.enemy_cap, a.enemy_cap());
        assert_eq!(view.field.arrival_rate, a.arrival_rate());
        assert_eq!(view.field.difficulty, a.balance.preset.name());
        assert_eq!(view.field.batch_size, a.progression.batch_size);
        assert_eq!(view.field.gear_level, a.progression.item_level);
        assert_eq!(view.field.level_name, content().levels[a.level].name);
        assert_eq!(view.log_tail.len(), LOG_TAIL);
        assert_eq!(view.log_tail[0], "line 24");
        assert_eq!(view.log_tail[LOG_TAIL - 1], "line 7");
        assert_eq!(view.targets.len(), a.enemies.len());
        assert_eq!(view.blueprints, game.campaign().blueprints);
        assert_eq!(
            view.any_blueprint(),
            game.campaign().blueprints.iter().any(|b| *b)
        );
        assert_eq!(view.global_paused, game.global_paused());
        assert!(!view.first_deployment);
    }

    #[test]
    fn country_stages_carry_their_biome_theme_and_local_setting() {
        let mut game = Game::default();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let a = game.attempt().unwrap();
        let country = world().country(a.country_id.as_deref().unwrap()).unwrap();
        let view = BattleScreen::build(&game).unwrap();
        assert_eq!(view.field.setting, Some(country.stage_theme(a.stage)));
        assert!(view.theme <= 4);
        // Without a country the backdrop follows the level and no setting shows.
        let mut legacy = fixture();
        legacy.attempt_mut().unwrap().country_id = None;
        let view = BattleScreen::build(&legacy).unwrap();
        assert_eq!(view.theme, legacy.attempt().unwrap().level);
        assert_eq!(view.field.setting, None);
    }

    #[test]
    fn reticle_uses_exact_ammo_and_frozen_reload_ticks_without_a_presentation_clock() {
        let mut game = Game::default();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let a = game.attempt_mut().unwrap();
        a.ammo = 3;
        a.reload_left = 63;
        a.profile.personal.reload = 2.1;
        let state = ReticleReadout::build(a, None);
        assert_eq!((state.remaining, state.capacity), (3, a.magazine));
        assert_eq!(state.progress, Some(0.5));
        assert_eq!(state.label, "R 1.0s");
        assert!(!state.observed);
        assert_eq!(ReticleReadout::build(a, None), state);
        assert_eq!(BattleScreen::build(&game).unwrap().reticle(None), state);
        let a = game.attempt_mut().unwrap();
        a.mode = Mode::Npc;
        a.profile.npc.reload_turns = 3;
        a.reload_left = 90;
        let observed = ReticleReadout::build(a, Some(OrdnanceKind::Grenade));
        assert_eq!(observed.progress, Some(0.5));
        // Observed battles never show a throwable readout.
        assert_eq!(observed.label, "R 1.5s");
        assert!(observed.observed);
    }

    #[test]
    fn selected_throwables_report_charges_and_cooldown_on_the_reticle() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.reload_left = 0;
        let kind = OrdnanceKind::Grenade;
        let total = OrdnanceSlotView::of(a, kind).cooldown_total;
        let charges = a.ordnance.charges(kind);
        let issued = a.ordnance.issued(kind);
        let view = BattleScreen::build(&game).unwrap();
        let readout = view.reticle(Some(kind));
        assert_eq!(
            readout,
            ReticleReadout::build(game.attempt().unwrap(), Some(kind))
        );
        assert_eq!(
            (readout.remaining, readout.capacity),
            (charges, issued.max(1))
        );
        assert_eq!(readout.label, format!("{charges} left"));
        assert_eq!(readout.progress, None);
        let mut cooling = view.clone();
        cooling.ordnance = KindMap::from_fn(|k| OrdnanceSlotView {
            cooldown: (total / 2.0) as u32,
            ..*view.slot(k)
        });
        let progress = cooling.reticle(Some(kind)).progress.unwrap();
        assert!((progress - (1.0 - (total / 2.0).floor() / total)).abs() < 1e-6);
        let mut reloading = view;
        reloading.weapon.reload_left = 1;
        assert_eq!(reloading.reticle(Some(kind)).label, "RELOAD");
    }

    #[test]
    fn aim_assist_snaps_to_the_nearest_enemy_inside_the_radius_only() {
        let mut view = BattleScreen::build(&fixture()).unwrap();
        view.targets = vec![[5.0, 1.5], [4.0, 1.5], [4.0, 1.5], [20.0, 0.5]];
        assert_eq!(view.assist([3.5, 1.5], 0.0), [3.5, 1.5]);
        assert_eq!(view.assist([3.5, 1.5], 2.0), [4.0, 1.5]);
        assert_eq!(view.assist([3.5, 1.5], 0.4), [3.5, 1.5]);
        assert_eq!(view.assist([19.0, 0.5], 1.5), [20.0, 0.5]);
        view.lanes = 3;
        assert_eq!(view.selected_lane([0.0, -2.0]), 1);
        assert_eq!(view.selected_lane([0.0, 1.7]), 2);
        assert_eq!(view.selected_lane([0.0, 9.0]), 3);
    }

    #[test]
    fn first_deployment_hints_follow_the_attempt_milestones() {
        let mut game = Game::default();
        game.deploy(0, Mode::Personal).unwrap();
        let hint = |game: &Game| BattleScreen::build(game).unwrap().field.hint;
        assert!(BattleScreen::build(&game).unwrap().first_deployment);
        assert_eq!(hint(&game), FieldHint::Opening);
        let a = game.attempt_mut().unwrap();
        a.tick = 10;
        a.shots = 0;
        assert_eq!(hint(&game), FieldHint::Shoot);
        let a = game.attempt_mut().unwrap();
        a.shots = 2;
        a.reload_ticks = 0;
        a.ammo = a.magazine.saturating_sub(1);
        assert_eq!(hint(&game), FieldHint::Reload);
        let a = game.attempt_mut().unwrap();
        a.ammo = a.magazine;
        a.first_contact = None;
        assert_eq!(hint(&game), FieldHint::Contact);
        game.attempt_mut().unwrap().first_contact = Some(3.0);
        assert_eq!(hint(&game), FieldHint::Banked);
    }

    #[test]
    fn placement_preview_reports_the_aimed_cell_validity_and_occupant() {
        let game = fixture();
        let a = game.attempt().unwrap();
        let kind = StructureKind::ALL[0];
        assert_eq!(placement_preview(&game, kind, [-1.0, 0.5]), None);
        let center = a.build_cell_center(1, 2).unwrap();
        let preview = placement_preview(&game, kind, center).unwrap();
        assert_eq!((preview.lane, preview.column), (1, 2));
        assert_eq!(preview.center, center);
        assert_eq!(preview.half_width, a.construction_cell_width() * 0.5);
        assert_eq!(preview.valid, game.preview_placement(kind, 1, 2));
        assert_eq!(
            preview.occupant,
            a.structures
                .iter()
                .find(|s| s.lane == 1 && s.column == 2)
                .map(|s| s.id)
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
