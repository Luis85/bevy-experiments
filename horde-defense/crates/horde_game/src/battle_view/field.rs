use super::interpolation::SceneBlend;
use crate::{
    animation::{ActorPose, Appearance, DefenderPose, Particle, SceneAnimation, StructurePose},
    art::{FxSource, shooter_class},
    visual_assets::enemy_visuals::{self, EnemySprite, EnemyVisualCatalog},
};
use horde_core::{
    content::{EnemyKind, Mode, StructureKind, content},
    enemy_catalog::EnemyVisual,
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, CorpseRecord, Enemy, EnemyCondition, EnemyPhase, Event, EventKind},
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
/// How tall the field draws live hostile `e`, from the active visual
/// catalog: the height `BattleViewModel` gives its `EnemyView`.
pub fn enemy_height(a: &Attempt, e: &Enemy) -> FigureHeight {
    let catalog = enemy_visuals::active();
    let (visual, sprite) = enemy_art(a, e, catalog);
    figure_height(catalog, e.boss, visual, sprite)
}
/// `e`'s resolved visual, if it has one, and its sprite in `catalog`.
fn enemy_art<'a>(
    a: &Attempt,
    e: &'a Enemy,
    catalog: &'static EnemyVisualCatalog,
) -> (Option<&'a EnemyVisual>, Option<&'static EnemySprite>) {
    let visual = e.resolved.as_ref().map(|profile| &profile.visual);
    let sprite = visual.map_or_else(
        || {
            let level_boss = content().levels[a.level].boss;
            let boss = e.boss.then(|| e.boss_kind.unwrap_or(level_boss));
            enemy_visuals::legacy_sprite(e.kind, boss)
        },
        |visual| visual.sprite,
    );
    (visual, catalog.get(sprite))
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
/// What the battlefield paints for `e`'s health in `mode`: the critical
/// callout, a health bar on a wounded hostile or a boss, or nothing on a
/// hostile at full HP. The damage popups move beside the bar only where
/// this paints one (W1 review).
pub fn health_view(e: &Enemy, mode: Mode) -> HealthView {
    if e.critically_wounded(mode) {
        HealthView::Critical
    } else if e.hp < e.max_hp || e.boss {
        HealthView::Bar {
            fraction: (e.hp.max(0) as f32 / e.max_hp as f32).clamp(0.0, 1.0),
            boss: e.boss,
        }
    } else {
        HealthView::Hidden
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyView {
    pub id: u64,
    pub kind: EnemyKind,
    pub boss: bool,
    pub lane: u8,
    /// Interpolated world position (yards, lanes) of the feet.
    pub position: [f32; 2],
    /// Presentation-only crowd offset ([`crowd_stagger`]); painters draw the
    /// hostile, its flash, rings and bars at [`EnemyView::drawn`].
    pub stagger: [f32; 2],
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
    /// Where a ranged windup's spit lands, in world yards and lanes
    /// (`Attempt::spit_target`): the painters draw a dashed aim line to it
    /// from the drawn feet (W2-28). None for a melee windup and out of
    /// windup.
    pub aim_target: Option<[f32; 2]>,
    pub hit_flash: Option<f32>,
    pub stunned: bool,
    pub health: HealthView,
    pub label: Option<&'static str>,
}
impl EnemyView {
    /// Where the painters draw the feet: `position` plus the crowd stagger.
    pub fn drawn(&self) -> [f32; 2] {
        [
            self.position[0] + self.stagger[0],
            self.position[1] + self.stagger[1],
        ]
    }
}

/// Largest crowd stagger behind the feet, in yards (W1-69).
pub const CROWD_STAGGER_YARDS: f32 = 0.15;
/// Largest crowd stagger across the lane, either way, in lanes (W1-69).
pub const CROWD_STAGGER_LANES: f32 = 0.12;

/// The static, presentation-only draw offset `[yards, lanes]` of hostile `id`.
///
/// Enemies overlap in the simulation since W1-69 (user decision, 2026-11-06),
/// so a crowd at the line or at a wall shares one spot. Each body is drawn a
/// little apart so the crowd reads as a crowd: up to
/// [`CROWD_STAGGER_YARDS`] behind its feet (never past the line or into a
/// wall face) and up to [`CROWD_STAGGER_LANES`] across its lane. The offset
/// is a function of the id alone, never an RNG draw, so it is the same every
/// frame and never touches the simulation.
pub fn crowd_stagger(id: u64) -> [f32; 2] {
    // The R2 low-discrepancy sequence (fractional parts of id / plastic
    // number and id / its square) in 64-bit fixed point: bodies spawned close
    // together, as a crowd is, land well apart, and exactly for any id.
    let unit = |step: u64| (id.wrapping_mul(step) >> 48) as f32 / 65535.0;
    [
        unit(0xC13F_A9A9_02A6_3000) * CROWD_STAGGER_YARDS,
        (unit(0x91E1_0DA5_C79E_7800) * 2.0 - 1.0) * CROWD_STAGGER_LANES,
    ]
}

/// The [`crowd_stagger`] of the hostile `event` struck or killed, so every
/// mark painted for it (sparks, hit cross, kill blood and coin, tracer end,
/// damage number) lands on the drawn body (W1-69 review); zero for any other
/// target, such as a structure an enemy struck, and for a shot into open
/// ground. The hostile may have died of the hit since.
pub fn struck_stagger(a: &Attempt, event: &Event) -> [f32; 2] {
    let id = match event.kind {
        EventKind::Kill => event.actor_id,
        // The survivor's own shot names no target: it ends on the farthest
        // hostile it struck, whose impact this tick shares its end points.
        EventKind::Shot if event.target_id.is_none() => a
            .events
            .iter()
            .filter(|other| {
                matches!(
                    other.kind,
                    EventKind::Hit | EventKind::WeakPoint | EventKind::Armor
                ) && other.tick == event.tick
                    && other.from == event.from
                    && other.to == event.to
            })
            .find_map(|other| other.target_id),
        EventKind::Shot
        | EventKind::Hit
        | EventKind::WeakPoint
        | EventKind::Armor
        | EventKind::Melee
        | EventKind::Miss
        | EventKind::Defended => event.target_id,
        _ => None,
    };
    let Some(id) = id else {
        return [0.; 2];
    };
    let since = a.corpses.partition_point(|corpse| corpse.tick < event.tick);
    let hostile = a.enemies.iter().any(|e| e.id == id)
        || a.corpses[since..].iter().any(|corpse| corpse.id == id);
    if hostile { crowd_stagger(id) } else { [0.; 2] }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CorpseView {
    /// Where the painters draw the feet: the core position at death plus the
    /// dead hostile's [`crowd_stagger`], as its [`EnemyView::drawn`] was.
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
                id: record.id,
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
    /// The dead hostile's id, for its crowd stagger.
    id: u64,
    /// Core position of the feet at death.
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
    // The body falls where the hostile was drawn (W1-69 review): the same
    // static crowd stagger, so a body in a stack does not jump as it dies.
    let [dx, dy] = crowd_stagger(body.id);
    CorpseView {
        at: [body.at[0] + dx, body.at[1] + dy],
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
    /// The event point, on the drawn body when a hostile was struck
    /// ([`struck_stagger`]).
    pub to: [f32; 2],
    pub value: i32,
    pub lane: Option<u8>,
    pub actor_id: Option<u64>,
    pub target_id: Option<u64>,
    /// The event's tick and sequence number.
    pub tick: u64,
    pub seq: u64,
    /// A Shot event's place in its trigger pull ([`Volley`]).
    pub volley: Option<Volley>,
    /// What fired a Shot event; `None` for a trap and for every other kind.
    pub source: Option<FxSource>,
    /// What burst in an Explosion event ([`explosion_ordnance`]); `None`
    /// for every other kind.
    pub ordnance: Option<OrdnanceKind>,
    /// A Kill event of a boss: its saved body has a boss kind (W5-06).
    pub boss: bool,
}
/// The ground mark a burst leaves (W5-05): derived from an Explosion event
/// still in the event log, aged in seconds up to [`SCORCH_SECONDS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScorchView {
    pub center: [f32; 2],
    /// The blast radius in yards.
    pub radius: f32,
    pub age: f32,
}
/// How long a scorch mark stays, in seconds.
pub const SCORCH_SECONDS: f32 = 3.0;
/// The most scorch marks one frame draws: the newest ones.
pub const SCORCH_MAX: usize = 8;
/// What burst in `event`, an Explosion event of `a`: a Molotov when the log
/// has a Burn event of the same projectile and tick, or burning ground with
/// that id exists; otherwise a grenade. Presentation only: the event log
/// carries no ordnance kind, and no saved field is added for it.
fn explosion_ordnance(a: &Attempt, event: &Event) -> OrdnanceKind {
    let burn = a.events.iter().any(|other| {
        other.kind == EventKind::Burn
            && other.tick == event.tick
            && other.actor_id == event.actor_id
    });
    let zone = event
        .actor_id
        .is_some_and(|id| a.ordnance.fire_zones.iter().any(|zone| zone.id == id));
    if burn || zone {
        OrdnanceKind::Molotov
    } else {
        OrdnanceKind::Grenade
    }
}
/// The scorch marks of `a`, oldest first: its newest [`SCORCH_MAX`]
/// Explosion events no older than [`SCORCH_SECONDS`].
fn scorch_views(a: &Attempt) -> Vec<ScorchView> {
    let mut out: Vec<_> = a
        .events
        .iter()
        .rev()
        .filter(|e| e.kind == EventKind::Explosion)
        .map(|e| ScorchView {
            center: e.to,
            radius: (e.value as f32 / 100.0).max(0.1),
            age: a.tick.saturating_sub(e.tick) as f32 / 60.0,
        })
        .take_while(|scorch| scorch.age <= SCORCH_SECONDS)
        .take(SCORCH_MAX)
        .collect();
    out.reverse();
    out
}
/// The trigger pull a Shot event belongs to (W5-03). The survivor's Shot
/// events of one tick are one volley (a shell's pellets); a turret's are
/// grouped by tick and structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volley {
    /// The survivor's shot count at this trigger pull, 1-based:
    /// `Attempt::shots` less the newer volleys in the log, so it stays the
    /// same as the event log rolls. Always 0 for a turret.
    pub ordinal: u32,
    /// The volley's first event: the one that draws the flash, the light
    /// and the casings.
    pub first: bool,
}
impl Volley {
    /// A lone shot outside any counted volley.
    pub const SINGLE: Self = Self {
        ordinal: 0,
        first: true,
    };
}
/// Whether `a.events[index]`, a Shot event, opens its volley: no earlier
/// Shot of the same tick and actor is in the log.
pub fn first_of_volley(a: &Attempt, index: usize) -> bool {
    let Some(event) = a.events.get(index) else {
        return true;
    };
    !a.events
        .range(..index)
        .rev()
        .take_while(|other| other.tick == event.tick)
        .any(|other| other.kind == EventKind::Shot && other.actor_id == event.actor_id)
}
/// What fired the Shot `event`: the survivor's deployed weapon class, a
/// turret, or `None` for a trap's spring.
fn shot_source(a: &Attempt, event: &Event) -> Option<FxSource> {
    let Some(id) = event.actor_id else {
        return Some(FxSource::Weapon(shooter_class(a)));
    };
    let trap = a
        .structures
        .iter()
        .any(|s| s.id == id && s.kind == StructureKind::Trap);
    (!trap).then_some(FxSource::Turret)
}
/// Whether the Kill `event` felled a boss: the body it left (the kill names
/// the dead hostile as its actor) has a boss kind.
fn boss_kill(a: &Attempt, event: &Event) -> bool {
    let since = a.corpses.partition_point(|corpse| corpse.tick < event.tick);
    a.corpses[since..]
        .iter()
        .any(|corpse| Some(corpse.id) == event.actor_id && corpse.boss_kind.is_some())
}
/// The events of `a` some painter can still draw (`art::fx_lifetime`), oldest
/// first, each Shot with its volley and source.
fn fx_views(a: &Attempt) -> Vec<FxView> {
    let mut out = Vec::new();
    // Survivor volleys newer than the event being read.
    let mut newer = 0u32;
    for (index, e) in a.events.iter().enumerate().rev() {
        let age = a.tick.saturating_sub(e.tick) as f32 / 60.0;
        if age > crate::art::fx_lifetime(e.kind) {
            continue;
        }
        let shot = e.kind == EventKind::Shot;
        let volley = shot.then(|| {
            let first = first_of_volley(a, index);
            let survivor = e.actor_id.is_none();
            let ordinal = if survivor {
                a.shots.saturating_sub(newer)
            } else {
                0
            };
            if survivor && first {
                newer += 1;
            }
            Volley { ordinal, first }
        });
        out.push(FxView {
            kind: e.kind,
            age,
            from: e.from,
            to: {
                let [dx, dy] = struck_stagger(a, e);
                [e.to[0] + dx, e.to[1] + dy]
            },
            value: e.value,
            lane: e.lane,
            actor_id: e.actor_id,
            target_id: e.target_id,
            tick: e.tick,
            seq: e.seq,
            volley,
            source: shot.then(|| shot_source(a, e)).flatten(),
            ordnance: (e.kind == EventKind::Explosion).then(|| explosion_ordnance(a, e)),
            boss: e.kind == EventKind::Kill && boss_kill(a, e),
        });
    }
    out.reverse();
    out
}
/// A phased boss holding after a transition (W2-37, ruling W-13): the
/// visual twin of the `BossPhase` event. The painters draw the
/// [`BossPhaseView::label`] over the boss `boss_id` and a ring at its feet
/// on every frame of the hold, the ring static in reduced effects.
#[derive(Clone, Debug, PartialEq)]
pub struct BossPhaseView {
    /// The holding boss, so the painters find its drawn body.
    pub boss_id: u64,
    /// The transition entered, 1 or 2 (the event's `value`).
    pub transition: u8,
    /// The phase profile's name (`BossPhaseProfile::name`).
    pub name: String,
    /// Ticks of the hold still to run, `hold_until - tick`: from the
    /// profile's `hold_ticks` (90) on the entry tick down to 1.
    pub hold_left_ticks: u64,
}
impl BossPhaseView {
    /// "PHASE n - NAME", the name in capitals like the boss callouts.
    pub fn label(&self) -> String {
        format!("PHASE {} - {}", self.transition, self.name.to_uppercase())
    }
    /// The phase of `a`'s living boss while it holds at `a.tick`
    /// ([`Enemy::boss_held`]); None outside a hold and for a boss without
    /// phases.
    fn of(a: &Attempt) -> Option<Self> {
        a.enemies
            .iter()
            .filter(|e| e.boss && e.condition(a.mode) != EnemyCondition::Defeated)
            .find_map(|e| {
                let state = e.boss_state.as_ref().filter(|s| s.held(a.tick))?;
                Some(Self {
                    boss_id: e.id,
                    transition: state.entered,
                    name: state.profile.name.clone(),
                    hold_left_ticks: state.hold_until - a.tick,
                })
            })
    }
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
    /// Visible enemies in painter order: lane feet, then x descending (the
    /// body nearest the line on top), then drawn feet, then id.
    pub enemies: Vec<EnemyView>,
    pub fire_zones: Vec<FireZoneView>,
    pub projectiles: Vec<ProjectileView>,
    pub fx: Vec<FxView>,
    /// Scorch marks under the newest bursts, oldest first (W5-05).
    pub scorch: Vec<ScorchView>,
    pub particles: Vec<Particle>,
    pub defender: DefenderView,
    /// The boss holding in a phase transition this tick, if any (W2-37).
    pub boss_phase: Option<BossPhaseView>,
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
                        id: body.id,
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
        // Lanes paint top to bottom by their feet. Within a lane bodies overlap
        // (W1-69), so they paint far to near, the nearest to the line on top.
        // A stack on one spot paints by its drawn feet (the crowd stagger
        // moves them across the lane), farther first, then by id.
        let drawn_feet = |e: &Enemy| e.y() + crowd_stagger(e.id)[1];
        visible.sort_by(|(_, a), (_, b)| {
            a.y()
                .total_cmp(&b.y())
                .then(b.x.total_cmp(&a.x))
                .then(drawn_feet(a).total_cmp(&drawn_feet(b)))
                .then(a.id.cmp(&b.id))
        });
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
            fx: fx_views(a),
            scorch: scorch_views(a),
            particles: std::mem::take(&mut motion.particles),
            defender: DefenderView {
                position: a.defender_position(),
                pose: motion.defender,
            },
            boss_phase: BossPhaseView::of(a),
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
    let (visual, sprite) = enemy_art(a, e, catalog);
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
    let health = health_view(e, a.mode);
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
        stagger: crowd_stagger(e.id),
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
        aim_target: (e.attack_phase == EnemyPhase::Windup)
            .then(|| a.spit_target(e))
            .flatten(),
        hit_flash: (e.hit_stop_remaining() > 0).then(|| {
            (e.hit_stop_remaining() as f32 / horde_core::sim::ENEMY_HIT_STOP_TICKS as f32).sqrt()
        }),
        stunned: e.injury.stunned,
        health,
        label,
    }
}
