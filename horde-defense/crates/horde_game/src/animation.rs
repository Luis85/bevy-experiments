//! Bounded cosmetic sampling. Tick/IDs/events are read-only inputs; no clocks,
//! random draws, action decisions or hit geometry are written back to the core.
use crate::battle_view::{crowd_stagger, struck_stagger};
use horde_core::{
    content::{BossKind, EnemyKind, Mode, StructureKind, content},
    ordnance::OrdnanceKind,
    sim::{Attempt, Enemy, EnemyPhase, Event, EventKind, Structure},
    world::world,
};
use std::collections::VecDeque;

pub const MAX_SCENES: usize = 8;
pub const MAX_PARTICLES: usize = 128;
pub(crate) const EVENT_BUDGET: usize = 64;
/// Ticks a body takes to collapse; older bodies are settled.
pub const COLLAPSE_TICKS: u64 = 18;
/// Ticks a fresh body raises dust.
const DUST_TICKS: u64 = 36;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorPose {
    pub frame: u8,
    pub offset: [f32; 2],
    pub lean: f32,
    pub scale: [f32; 2],
    pub hit: f32,
    pub windup: f32,
    pub lunge: f32,
    pub moving: bool,
}
impl Default for ActorPose {
    fn default() -> Self {
        Self {
            frame: 0,
            offset: [0.; 2],
            lean: 0.,
            scale: [1.; 2],
            hit: 0.,
            windup: 0.,
            lunge: 0.,
            moving: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DefenderPose {
    pub recoil: f32,
    pub lean: f32,
    pub melee: f32,
    pub hit: f32,
    pub reload_progress: Option<f32>,
    pub throwing: f32,
    pub throw_kind: Option<OrdnanceKind>,
    pub throw_target: Option<[f32; 2]>,
    pub shot_target: Option<[f32; 2]>,
    pub target: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructurePose {
    pub target: [f32; 2],
    pub recoil: f32,
    pub trigger: f32,
    pub hit: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Calm,
    Mist,
    Rain,
    Snow,
    Dust,
    Ash,
}
impl WeatherKind {
    pub fn from_scene(weather: &str, biome: &str) -> Self {
        let text = weather.to_ascii_lowercase();
        let biome = biome.to_ascii_lowercase();
        if ["clear", "calm", "none"]
            .iter()
            .any(|word| text.contains(word))
        {
            Self::Calm
        } else if ["rain", "drizzle", "storm"]
            .iter()
            .any(|word| text.contains(word))
        {
            Self::Rain
        } else if ["snow", "frost", "cold"]
            .iter()
            .any(|word| text.contains(word))
        {
            Self::Snow
        } else if text.contains("ash") {
            Self::Ash
        } else if ["dust", "sand", "dry"]
            .iter()
            .any(|word| text.contains(word))
        {
            Self::Dust
        } else if ["mist", "fog", "haze"]
            .iter()
            .any(|word| text.contains(word))
        {
            Self::Mist
        } else if biome.contains("arid") || biome.contains("desert") {
            Self::Dust
        } else if text.contains("wind") && (biome.contains("alpine") || biome.contains("boreal")) {
            Self::Snow
        } else {
            Self::Mist
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Casing,
    Spark,
    Dust,
    Rain,
    Snow,
    Mist,
    Ash,
    Debris,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub kind: ParticleKind,
    pub at: [f32; 2],
    pub size: f32,
    pub alpha: f32,
    pub angle: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub kind: EnemyKind,
    pub boss: bool,
    pub boss_kind: Option<BossKind>,
}
/// A collapsing body. Its archetype visual stays on the saved record
/// (`a.corpses[settled_corpses + n]` for the `n`th death), so sampling does
/// not copy it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeathPose {
    pub id: u64,
    pub at: [f32; 2],
    pub appearance: Option<Appearance>,
    pub frame: u8,
    pub alpha: f32,
    pub collapse: f32,
    pub lean: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneAnimation {
    pub tick: u64,
    /// `(id, pose)` of every enemy, in `a.enemies` order.
    pub enemies: Vec<(u64, ActorPose)>,
    pub defender: DefenderPose,
    /// `(id, pose)` of every structure, in `a.structures` order.
    pub structures: Vec<(u64, StructurePose)>,
    /// Poses of the collapsing bodies only: the newest
    /// `CORPSES_COLLAPSING_DRAWN` of `a.corpses` younger than
    /// `COLLAPSE_TICKS`, none in reduced mode.
    pub deaths: Vec<DeathPose>,
    /// Length of the settled prefix of `a.corpses`, which is not sampled:
    /// saved bodies are append-only at ascending ticks, so every body before
    /// the collapsing ones has settled.
    pub settled_corpses: usize,
    pub particles: Vec<Particle>,
    pub weather: WeatherKind,
}
impl SceneAnimation {
    pub fn sample(a: &Attempt, reduced: bool) -> Self {
        let pulses = EnemyPulses::of(a);
        let mut frame = Self {
            tick: a.tick,
            enemies: a
                .enemies
                .iter()
                .map(|e| (e.id, enemy_pose(a, e, reduced, &pulses)))
                .collect(),
            defender: defender_pose(a, reduced),
            structures: a
                .structures
                .iter()
                .map(|s| (s.id, structure_pose(a, s, reduced)))
                .collect(),
            deaths: vec![],
            settled_corpses: 0,
            particles: vec![],
            weather: scene_weather(a),
        };
        sample_deaths(a, reduced, &mut frame);
        if !reduced {
            for event in a.events.iter().rev().take(EVENT_BUDGET) {
                sample_event_particles(a, event, &mut frame.particles);
            }
            sample_weather(a, frame.weather, &mut frame.particles);
        }
        frame
    }
}

// Scene bookkeeping is bounded; persistent remains belong exclusively to the
// attempt. Sampling a resumed/replaced save cannot retain another run's bodies.
#[derive(Clone, Debug)]
struct EncounterHistory {
    id: u64,
    scope: u64,
    level: usize,
    mode: Mode,
    country: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct SceneCache {
    scenes: VecDeque<EncounterHistory>,
}
impl SceneCache {
    pub fn sample(&mut self, a: &Attempt, reduced: bool) -> SceneAnimation {
        self.sample_scoped(a, reduced, 0)
    }
    pub fn sample_scoped(&mut self, a: &Attempt, reduced: bool, scope: u64) -> SceneAnimation {
        if let Some(index) = self.scenes.iter().position(|scene| {
            scene.id == a.id
                && scene.country == a.country_id
                && scene.scope == scope
                && scene.level == a.level
                && scene.mode == a.mode
        }) {
            let scene = self.scenes.remove(index).expect("known presentation scene");
            self.scenes.push_back(scene);
        } else {
            if self.scenes.len() == MAX_SCENES {
                self.scenes.pop_front();
            }
            self.scenes.push_back(EncounterHistory {
                id: a.id,
                scope,
                level: a.level,
                mode: a.mode,
                country: a.country_id.clone(),
            });
        }
        SceneAnimation::sample(a, reduced)
    }
    #[cfg(test)]
    fn scene_count(&self) -> usize {
        self.scenes.len()
    }
}

/// Bodies before this index of `a.corpses` have settled: all of them in
/// reduced mode, otherwise those at least `COLLAPSE_TICKS` old and every body
/// beyond the newest `CORPSES_COLLAPSING_DRAWN`.
pub fn settled_corpse_count(a: &Attempt, reduced: bool) -> usize {
    if reduced {
        return a.corpses.len();
    }
    let collapsed = older_than(a, COLLAPSE_TICKS);
    collapsed.max(
        a.corpses
            .len()
            .saturating_sub(crate::perf_budget::CORPSES_COLLAPSING_DRAWN),
    )
}
/// Length of the prefix of `a.corpses` at least `ticks` old (ascending ticks).
fn older_than(a: &Attempt, ticks: u64) -> usize {
    a.corpses
        .partition_point(|corpse| a.tick.saturating_sub(corpse.tick) >= ticks)
}

fn sample_deaths(a: &Attempt, reduced: bool, frame: &mut SceneAnimation) {
    let settled = settled_corpse_count(a, reduced);
    frame.settled_corpses = settled;
    frame.deaths.reserve(a.corpses.len() - settled);
    for corpse in &a.corpses[settled..] {
        let age = a.tick.saturating_sub(corpse.tick) as f32;
        let collapse = smooth((age / COLLAPSE_TICKS as f32).min(1.));
        frame.deaths.push(DeathPose {
            id: corpse.id,
            at: corpse.at,
            appearance: Some(Appearance {
                kind: corpse.kind,
                boss: corpse.boss_kind.is_some(),
                boss_kind: corpse.boss_kind,
            }),
            frame: 0,
            alpha: 1.,
            collapse,
            lean: -0.85 * collapse,
        });
    }
    if reduced {
        return;
    }
    // Fresh bodies raise dust a little longer than they collapse, oldest first.
    for corpse in &a.corpses[older_than(a, DUST_TICKS)..] {
        let age = a.tick.saturating_sub(corpse.tick) as f32;
        if age < DUST_TICKS as f32 && frame.particles.len() < MAX_PARTICLES {
            let t = age / DUST_TICKS as f32;
            // The dust rises from the drawn, crowd-staggered body (W1-69 review).
            let [dx, dy] = crowd_stagger(corpse.id);
            let at = [corpse.at[0] + dx, corpse.at[1] + dy];
            for n in 0..3 {
                push_particle(
                    &mut frame.particles,
                    Particle {
                        kind: ParticleKind::Dust,
                        at: [
                            at[0] + (noise(corpse.id.wrapping_add(n)) - 0.5) * t * 0.7,
                            at[1] + 0.22 - t * 0.25,
                        ],
                        size: 0.04 + t * 0.12,
                        alpha: (1. - t) * 0.35,
                        angle: 0.,
                    },
                );
            }
        }
    }
}

/// The birth tick of each enemy's latest hit and latest attack event, from
/// one pass over the events rather than two event scans per enemy.
struct EnemyPulses {
    /// `(target id, tick)` of the latest damaging Hit or WeakPoint, by id.
    hits: Vec<(u64, u64)>,
    /// `(actor id, tick)` of the latest Damage, Miss, Defended or Armor, by id.
    lunges: Vec<(u64, u64)>,
}
impl EnemyPulses {
    fn of(a: &Attempt) -> Self {
        Self {
            hits: latest_ticks_by_id(a, |event| {
                (event.value > 0 && matches!(event.kind, EventKind::Hit | EventKind::WeakPoint))
                    .then_some(event.target_id)
                    .flatten()
            }),
            lunges: latest_ticks_by_id(a, |event| {
                matches!(
                    event.kind,
                    EventKind::Damage | EventKind::Miss | EventKind::Defended | EventKind::Armor
                )
                .then_some(event.actor_id)
                .flatten()
            }),
        }
    }
}
/// `(id, tick)` of the latest event at or before `a.tick` that `actor`
/// assigns to each id, sorted by id: what `latest` finds for that id.
fn latest_ticks_by_id(a: &Attempt, actor: impl Fn(&Event) -> Option<u64>) -> Vec<(u64, u64)> {
    let mut latest: Vec<(u64, u64)> = a
        .events
        .iter()
        .rev()
        .filter(|event| event.tick <= a.tick)
        .filter_map(|event| actor(event).map(|id| (id, event.tick)))
        .collect();
    // Newest first; the stable sort keeps each id's newest event first.
    latest.sort_by_key(|&(id, _)| id);
    latest.dedup_by_key(|&mut (id, _)| id);
    latest
}
fn latest_tick_of(latest: &[(u64, u64)], id: u64) -> Option<u64> {
    latest
        .binary_search_by_key(&id, |&(id, _)| id)
        .ok()
        .map(|n| latest[n].1)
}

fn enemy_pose(a: &Attempt, e: &Enemy, reduced: bool, pulses: &EnemyPulses) -> ActorPose {
    // A boss holding after a phase transition stands still in the core.
    let moving = e.attack_phase == EnemyPhase::Advance
        && !e.injury.prone
        && !e.injury.stunned
        && !e.boss_held(a.tick);
    let base_frame_ticks = match e.kind {
        EnemyKind::Runner => 7,
        EnemyKind::Brute => 22,
        EnemyKind::Armored => 16,
        _ => 18,
    };
    let injury_speed = if a.mode == Mode::Npc {
        (if e.injury.hp * 3 < e.injury.max_hp {
            0.5
        } else {
            1.
        }) * (if e.injury.fp * 3 < e.injury.max_fp {
            0.5
        } else {
            1.
        })
    } else {
        1.
    };
    let frame_ticks = ((base_frame_ticks as f32
        / (a.modifiers.enemy_speed * injury_speed).clamp(0.15, 3.))
    .round() as u64)
        .max(3);
    let cycle = frame_ticks * 4;
    let pose_tick = if e.hit_stop_remaining() > 0 {
        a.tick.saturating_sub(u64::from(
            horde_core::sim::ENEMY_HIT_STOP_TICKS.saturating_sub(e.hit_stop_remaining()),
        ))
    } else {
        a.tick
    };
    let clock = pose_tick.wrapping_add(e.id.wrapping_mul(7));
    let phase = (clock % cycle) as f32 / cycle as f32 * std::f32::consts::TAU;
    let hit = latest_tick_of(&pulses.hits, e.id).map_or(0., |born| pulse(a.tick, born, 12));
    let lunge = latest_tick_of(&pulses.lunges, e.id).map_or(0., |born| pulse(a.tick, born, 14));
    let windup = if e.attack_phase == EnemyPhase::Windup {
        e.windup_progress().clamp(0., 1.)
    } else {
        0.
    };
    if reduced {
        return ActorPose {
            hit,
            windup,
            moving,
            ..Default::default()
        };
    }
    let stride = if moving { phase.sin() } else { 0. };
    let recovery = if e.attack_phase == EnemyPhase::Recovery {
        e.attack_left as f32 / e.windup_total.max(1) as f32
    } else {
        0.
    };
    ActorPose {
        frame: if moving {
            ((clock / frame_ticks) % 4) as u8
        } else {
            0
        },
        // Attack/reaction motion bends around planted feet rather than moving
        // the gameplay anchor or making the targeting geometry chase a sprite.
        offset: [stride * 0.012, -stride.abs() * 0.02],
        lean: stride * 0.045 + windup.powi(2) * 0.16 - lunge * 0.27 + hit * 0.12 + recovery * 0.06,
        scale: [1. + hit * 0.045, 1. - hit * 0.075],
        hit,
        windup,
        lunge,
        moving,
    }
}
fn defender_pose(a: &Attempt, reduced: bool) -> DefenderPose {
    let shot = latest(a, |event| {
        event.actor_id.is_none() && event.kind == EventKind::Shot
    });
    let melee = latest(a, |event| {
        event.actor_id.is_none() && event.kind == EventKind::Melee
    })
    .map_or(0., |event| pulse(a.tick, event.tick, 24));
    let (recoil, hit) = defender_pulses_at(a, a.tick);
    let throw = latest(a, |event| event.kind == EventKind::Throw)
        .filter(|event| a.tick.saturating_sub(event.tick) < 24);
    let throwing = throw.map_or(0., |event| pulse(a.tick, event.tick, 24));
    let shot_target = shot
        .filter(|event| a.tick.saturating_sub(event.tick) < 8)
        .map(|event| event.to);
    let reload_progress = (a.reload_left > 0).then(|| {
        let total = if a.mode == Mode::Npc {
            a.profile.npc.reload_turns * 60
        } else {
            (a.profile.personal.reload * 60.).ceil() as u32
        };
        (1. - a.reload_left as f32 / total.max(1) as f32).clamp(0., 1.)
    });
    let target = a
        .decision_target
        .and_then(|id| a.enemies.iter().find(|e| e.id == id))
        .map(|e| [e.x, e.y()])
        .or_else(|| shot.map(|shot| shot.to))
        .unwrap_or([a.width_yards * 0.5, a.defender_position()[1]]);
    DefenderPose {
        recoil: if reduced { 0. } else { recoil },
        lean: if reduced {
            0.
        } else {
            recoil * 0.075
                + hit * 0.06
                + reload_progress.map_or(0., |p| (p * std::f32::consts::TAU).sin() * 0.04)
        },
        melee: if reduced { 0. } else { melee },
        hit,
        reload_progress,
        throwing: if reduced { 0. } else { throwing },
        throw_kind: throw.map(|event| {
            if event.value == 0 {
                OrdnanceKind::Grenade
            } else {
                OrdnanceKind::Molotov
            }
        }),
        throw_target: throw.map(|event| event.to),
        shot_target,
        target,
    }
}
/// The same event pulses drive the visible body and historical projectile
/// release poses. Sampling an earlier tick ignores later events.
pub fn defender_pulses_at(a: &Attempt, tick: u64) -> (f32, f32) {
    let recoil = latest_at(a, tick, |event| {
        event.actor_id.is_none() && event.kind == EventKind::Shot
    })
    .map(|event| event.tick);
    let hit = latest_at(a, tick, |event| {
        event.target_id.is_none() && event.value > 0 && event.kind == EventKind::Damage
    })
    .map(|event| event.tick);
    defender_pulses_from_ticks(tick, recoil, hit)
}
/// Event-backed live poses and saved release facts share the same pulse curve.
pub fn defender_pulses_from_ticks(
    tick: u64,
    last_shot_tick: Option<u64>,
    last_hit_tick: Option<u64>,
) -> (f32, f32) {
    (
        last_shot_tick.map_or(0., |born| pulse(tick, born, 10)),
        last_hit_tick.map_or(0., |born| pulse(tick, born, 15)),
    )
}
fn structure_pose(a: &Attempt, s: &Structure, reduced: bool) -> StructurePose {
    let shot = latest(a, |event| {
        event.actor_id == Some(s.id) && event.kind == EventKind::Shot
    });
    let range = content().structure(s.kind).range;
    let nearest = a
        .enemies
        .iter()
        .filter(|e| e.in_view() && (e.x - s.x()).hypot(e.y() - s.y()) <= range)
        .min_by(|e, f| e.x.total_cmp(&f.x).then(e.id.cmp(&f.id)))
        .map(|e| [e.x, e.y()])
        .unwrap_or([s.x() + 6., s.y()]);
    // Hold the resolved direction for the short visible tracer. Turning toward
    // another actor before it fades would detach the ray from the barrel.
    let target = if let Some(event) = shot.filter(|event| a.tick.saturating_sub(event.tick) < 8) {
        event.to
    } else if reduced {
        nearest
    } else {
        shot.map_or(nearest, |event| {
            let t = smooth((a.tick.saturating_sub(event.tick) as f32 / 30.).clamp(0., 1.));
            [
                lerp(event.to[0], nearest[0], t),
                lerp(event.to[1], nearest[1], t),
            ]
        })
    };
    let recoil = shot.map_or(0., |event| pulse(a.tick, event.tick, 12));
    let hit = latest(a, |event| {
        event.target_id == Some(s.id) && event.kind == EventKind::Damage && event.value > 0
    })
    .map_or(0., |event| pulse(a.tick, event.tick, 14));
    StructurePose {
        target,
        recoil: if reduced { 0. } else { recoil },
        trigger: if !reduced && s.kind == StructureKind::Trap {
            recoil
        } else {
            0.
        },
        hit,
    }
}
fn scene_weather(a: &Attempt) -> WeatherKind {
    if let Some(country) = a.country_id.as_deref().and_then(|id| world().country(id)) {
        let scene = country.stage_theme(a.stage);
        WeatherKind::from_scene(&scene.weather, &scene.biome)
    } else {
        match a.level {
            2 => WeatherKind::Dust,
            3 => WeatherKind::Mist,
            4 => WeatherKind::Rain,
            _ => WeatherKind::Mist,
        }
    }
}
fn latest(a: &Attempt, predicate: impl Fn(&Event) -> bool) -> Option<&Event> {
    latest_at(a, a.tick, predicate)
}
fn latest_at(a: &Attempt, tick: u64, predicate: impl Fn(&Event) -> bool) -> Option<&Event> {
    a.events
        .iter()
        .rev()
        .find(|event| event.tick <= tick && predicate(event))
}
fn pulse(tick: u64, born: u64, duration: u64) -> f32 {
    if born > tick {
        0.
    } else {
        (1. - tick.saturating_sub(born) as f32 / duration as f32)
            .clamp(0., 1.)
            .powi(2)
    }
}
fn smooth(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn noise(id: u64) -> f32 {
    let n = id.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17);
    (n >> 40) as f32 / 0x00ff_ffff_u32 as f32
}
fn push_particle(out: &mut Vec<Particle>, particle: Particle) {
    if out.len() < MAX_PARTICLES {
        out.push(particle);
    }
}
fn sample_event_particles(a: &Attempt, event: &Event, out: &mut Vec<Particle>) {
    if event.tick > a.tick {
        return;
    }
    let age = (a.tick - event.tick) as f32 / 60.;
    let seed = event.tick.wrapping_mul(31)
        ^ event.actor_id.unwrap_or(0).rotate_left(3)
        ^ event.target_id.unwrap_or(0).rotate_left(11)
        ^ (event.kind as u64).wrapping_mul(97)
        ^ u64::from(event.to[0].to_bits()).rotate_left(17)
        ^ u64::from(event.to[1].to_bits());
    match event.kind {
        EventKind::Shot if age < 0.45 => {
            // Traps create a spring/dust impulse, not a firearm muzzle flash.
            if event.actor_id.is_some_and(|id| {
                a.structures
                    .iter()
                    .any(|s| s.id == id && s.kind == StructureKind::Trap)
            }) {
                for n in 0..4 {
                    push_particle(
                        out,
                        Particle {
                            kind: ParticleKind::Dust,
                            at: [
                                event.from[0] + (noise(seed + n) - 0.5) * age * 1.2,
                                event.from[1] + 0.2 - age * 0.5,
                            ],
                            size: 0.04 + age * 0.15,
                            alpha: (1. - age / 0.45) * 0.5,
                            angle: 0.,
                        },
                    );
                }
                return;
            }
            // Muzzle light is painted from the calibrated displayed gun transform,
            // rather than a guessed offset from the authoritative aim origin.
            for n in 0..2 {
                let random = noise(seed + n);
                push_particle(
                    out,
                    Particle {
                        kind: ParticleKind::Casing,
                        at: [
                            event.from[0] + age * (0.35 + random * 0.45),
                            event.from[1] + age * (-0.8 + random * 0.25) + age * age * 2.2,
                        ],
                        size: 0.035,
                        alpha: (1. - age / 0.45) * 0.85,
                        angle: age * (12. + random * 8.),
                    },
                );
            }
        }
        EventKind::Hit | EventKind::WeakPoint | EventKind::Armor if age < 0.3 => {
            let [dx, dy] = struck_stagger(a, event);
            let to = [event.to[0] + dx, event.to[1] + dy];
            for n in 0..3 {
                let theta = noise(seed + n) * std::f32::consts::TAU;
                push_particle(
                    out,
                    Particle {
                        kind: if event.kind == EventKind::Armor {
                            ParticleKind::Spark
                        } else {
                            ParticleKind::Dust
                        },
                        at: [
                            to[0] + theta.cos() * age * 0.85,
                            to[1] + theta.sin() * age * 0.5,
                        ],
                        size: 0.025 + age * 0.025,
                        alpha: 1. - age / 0.3,
                        angle: theta,
                    },
                );
            }
        }
        EventKind::StructureLost if age < 0.5 => {
            for n in 0..8 {
                let random = noise(seed + n);
                let theta = random * std::f32::consts::TAU;
                push_particle(
                    out,
                    Particle {
                        kind: ParticleKind::Debris,
                        at: [
                            event.to[0] + theta.cos() * age * 1.2,
                            event.to[1] + theta.sin() * age * 0.5 + age * age * 0.6,
                        ],
                        size: 0.04 + noise(seed + n + 51) * 0.05,
                        alpha: 1. - age / 0.5,
                        angle: theta + age * 4.,
                    },
                );
            }
        }
        _ => {}
    }
}
fn sample_weather(a: &Attempt, weather: WeatherKind, out: &mut Vec<Particle>) {
    if weather == WeatherKind::Calm {
        return;
    }
    let left = a.width_yards * 0.1;
    let span = a.width_yards - left;
    let lanes = f32::from(a.lane_count);
    let seconds = a.tick as f32 / 60.;
    let count = match weather {
        WeatherKind::Mist => 5,
        WeatherKind::Rain => 36,
        _ => 24,
    };
    for n in 0..count {
        let seed = a.id.wrapping_mul(97).wrapping_add(n as u64 * 13);
        let random = noise(seed);
        let drift = noise(seed + 3);
        let (kind, x, y, size, alpha, angle) = match weather {
            WeatherKind::Rain => (
                ParticleKind::Rain,
                left + ((random * span - seconds * 1.4).rem_euclid(span)),
                ((drift * lanes + seconds * 7.).rem_euclid(lanes)),
                0.25,
                0.18,
                1.77,
            ),
            WeatherKind::Snow => (
                ParticleKind::Snow,
                left + ((random * span - seconds * 0.25).rem_euclid(span)),
                ((drift * lanes + seconds * 0.35).rem_euclid(lanes)),
                0.025 + random * 0.018,
                0.22,
                0.,
            ),
            WeatherKind::Dust | WeatherKind::Ash => (
                if weather == WeatherKind::Ash {
                    ParticleKind::Ash
                } else {
                    ParticleKind::Dust
                },
                left + ((random * span - seconds * (0.35 + drift * 0.3)).rem_euclid(span)),
                drift * lanes + (seconds * 0.3 + random * 6.).sin() * 0.08,
                0.035 + drift * 0.035,
                0.12,
                0.,
            ),
            WeatherKind::Mist => (
                ParticleKind::Mist,
                a.width_yards * 0.23
                    + ((random * (a.width_yards * 0.77) - seconds * 0.12)
                        .rem_euclid(a.width_yards * 0.77)),
                1. + drift * (lanes - 2.0),
                0.8 + random * 0.5,
                0.035,
                0.,
            ),
            WeatherKind::Calm => unreachable!(),
        };
        push_particle(
            out,
            Particle {
                kind,
                at: [x, y],
                size,
                alpha,
                angle,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::sim::{Game, Input};
    /// The sampled pose of enemy `id`.
    fn pose(frame: &SceneAnimation, id: u64) -> ActorPose {
        frame
            .enemies
            .iter()
            .find(|(enemy, _)| *enemy == id)
            .expect("a sampled enemy")
            .1
    }
    fn fixture() -> Game {
        let mut game = Game::default();
        game.campaign_mut()
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        game.campaign_mut().attempts = 1;
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut a, EnemyKind::Runner, 4, false);
        a.enemies[0].x = 20.;
        *game.attempt_slot_mut() = Some(a);
        game
    }
    #[test]
    fn tick_based_walk_poses_vary_without_mutating_authority() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        let before = serde_json::to_string(a).unwrap();
        let first = SceneAnimation::sample(a, false);
        assert_eq!(first, SceneAnimation::sample(a, false));
        assert_eq!(before, serde_json::to_string(a).unwrap());
        a.tick += 7;
        let next = SceneAnimation::sample(a, false);
        assert_ne!(
            pose(&first, a.enemies[0].id).frame,
            pose(&next, a.enemies[0].id).frame
        );
    }
    #[test]
    fn turret_holds_the_resolved_target_while_its_tracer_is_visible() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.enemies[0].x = 12.;
        let turret = Structure {
            id: 900,
            kind: StructureKind::Turret,
            lane: 5,
            column: 0,
            hp: 100,
            max_hp: 100,
            cooldown: 10,
            charges: 0,
            cost: 12,
            ready_turns: 0,
            position_x: None,
            build_width: None,
        };
        a.events = vec![Event {
            kind: EventKind::Shot,
            actor_id: Some(turret.id),
            target_id: None,
            tick: 100,
            from: [turret.x(), turret.y()],
            to: [20., 0.5],
            lane: Some(0),
            value: 0,
            duration_ticks: 0,
            seq: 0,
        }]
        .into();
        for tick in 100..108 {
            a.tick = tick;
            for reduced in [false, true] {
                let before = serde_json::to_string(a).unwrap();
                assert_eq!(structure_pose(a, &turret, reduced).target, [20., 0.5]);
                assert_eq!(serde_json::to_string(a).unwrap(), before);
            }
        }
        a.tick = 108;
        assert_ne!(structure_pose(a, &turret, false).target, [20., 0.5]);
        assert_eq!(structure_pose(a, &turret, true).target, [12., 4.5]);
    }
    #[test]
    fn paused_ticks_freeze_motion_weather_and_cached_deaths() {
        let mut game = fixture();
        game.set_paused(true);
        let mut cache = SceneCache::default();
        let before = serde_json::to_string(&game).unwrap();
        let first = cache.sample(game.attempt().unwrap(), false);
        for _ in 0..60 {
            game.tick(Input::default());
            assert_eq!(first, cache.sample(game.attempt().unwrap(), false));
        }
        assert_eq!(before, serde_json::to_string(&game).unwrap());
    }
    #[test]
    fn reduced_effects_suppress_motion_and_particles_but_keep_windup_and_reload_information() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.enemies[0].attack_phase = EnemyPhase::Windup;
        a.enemies[0].windup_total = 60;
        a.enemies[0].attack_left = 15;
        a.reload_left = 30;
        let sample = SceneAnimation::sample(a, true);
        let pose = pose(&sample, a.enemies[0].id);
        assert_eq!(pose.offset, [0.; 2]);
        assert_eq!(pose.lean, 0.);
        assert_eq!(pose.windup, 0.75);
        assert!(sample.defender.reload_progress.is_some());
        assert!(sample.particles.is_empty());
    }
    #[test]
    fn effects_and_encounter_history_remain_bounded_under_repeated_sampling() {
        let mut game = fixture();
        let mut cache = SceneCache::default();
        let a = game.attempt_mut().unwrap();
        a.events = (0..512)
            .map(|_| Event {
                kind: EventKind::Shot,
                from: [0.5, 5.5],
                to: [20., 4.5],
                value: 0,
                tick: a.tick,
                actor_id: None,
                target_id: Some(a.enemies[0].id),
                lane: Some(4),
                duration_ticks: 0,
                seq: 0,
            })
            .collect();
        let before = serde_json::to_string(a).unwrap();
        assert_eq!(cache.sample(a, false).particles.len(), MAX_PARTICLES);
        assert_eq!(serde_json::to_string(a).unwrap(), before);
        for id in 0..100 {
            let a = game.attempt_mut().unwrap();
            a.id = id;
            let sampled = cache.sample(a, false);
            assert!(sampled.particles.len() <= MAX_PARTICLES);
            assert!(cache.scene_count() <= MAX_SCENES);
        }
        assert_eq!(
            WeatherKind::from_scene("clear", "Tropical"),
            WeatherKind::Calm
        );
        assert_eq!(
            WeatherKind::from_scene("light_rain", "Arid"),
            WeatherKind::Rain
        );
    }
    /// Pin (W1-67): the one-pass pulse tables give every enemy, on a live
    /// progressed peak, the hit and attack event its own two scans of the
    /// events found, including events stamped after the attempt's tick.
    #[test]
    fn enemy_pulses_match_the_latest_event_scans() {
        let mut game = horde_core::sim::maximum_progression_render_fixture().expect("fixture");
        game.set_paused(false);
        game.attempt_mut().unwrap().paused = false;
        let mut checked = 0;
        for tick in 0..300 {
            game.tick(Input::default());
            let a = game.attempt_mut().unwrap();
            if tick % 10 != 0 {
                continue;
            }
            // The first enemy was hit and attacked twice, the newest at this
            // tick; a hit stamped after it (as a sampled earlier tick sees)
            // is skipped.
            let template = a.events.back().cloned().expect("live events");
            let first = a.enemies.first().map(|e| e.id);
            let staged = |kind, tick, actor_id, target_id| Event {
                kind,
                tick,
                value: 1,
                actor_id,
                target_id,
                ..template.clone()
            };
            let now = a.tick;
            a.events
                .push_front(staged(EventKind::Hit, now.saturating_sub(5), None, first));
            a.events.push_front(staged(
                EventKind::Damage,
                now.saturating_sub(6),
                first,
                None,
            ));
            a.events.push_back(staged(EventKind::Hit, now, None, first));
            a.events
                .push_back(staged(EventKind::Miss, now, first, None));
            a.events
                .push_back(staged(EventKind::Hit, now + 1, None, first));
            let a = game.attempt().unwrap();
            let pulses = EnemyPulses::of(a);
            for e in &a.enemies {
                let hit = latest(a, |event| {
                    event.target_id == Some(e.id)
                        && event.value > 0
                        && matches!(event.kind, EventKind::Hit | EventKind::WeakPoint)
                })
                .map(|event| event.tick);
                let lunge = latest(a, |event| {
                    event.actor_id == Some(e.id)
                        && matches!(
                            event.kind,
                            EventKind::Damage
                                | EventKind::Miss
                                | EventKind::Defended
                                | EventKind::Armor
                        )
                })
                .map(|event| event.tick);
                assert_eq!(latest_tick_of(&pulses.hits, e.id), hit, "enemy {}", e.id);
                assert_eq!(
                    latest_tick_of(&pulses.lunges, e.id),
                    lunge,
                    "enemy {}",
                    e.id
                );
                checked += usize::from(hit.is_some()) + usize::from(lunge.is_some());
            }
            assert_eq!(latest_tick_of(&pulses.hits, first.unwrap()), Some(now));
            assert_eq!(latest_tick_of(&pulses.lunges, first.unwrap()), Some(now));
            let events = &mut game.attempt_mut().unwrap().events;
            events.drain(..2);
            events.truncate(events.len() - 3);
        }
        assert!(checked > 100, "the peak hits and attacks: {checked}");
    }
    #[test]
    fn authoritative_remains_persist_without_events_and_after_save_resume() {
        let mut game = fixture();
        let mut cache = SceneCache::default();
        let a = game.attempt_mut().unwrap();
        let enemy = a.enemies.remove(0);
        // A tint and scale away from the white 1.0 defaults, so a save that
        // reset either would show.
        let visual = enemy
            .resolved
            .as_ref()
            .map(|v| horde_core::enemy_catalog::EnemyVisual {
                tint: [0.8, 0.6, 0.4, 1.0],
                scale: 1.15,
                ..v.visual.clone()
            });
        assert!(visual.is_some(), "the fixture enemy has a resolved visual");
        a.corpses.push(horde_core::sim::CorpseRecord {
            id: enemy.id,
            kind: enemy.kind,
            boss_kind: enemy.boss_kind,
            at: [enemy.x, enemy.y()],
            visual: visual.clone(),
            tick: a.tick,
        });
        let first = cache.sample(a, false);
        assert_eq!(first.deaths.len(), 1);
        assert_eq!(first.deaths[0].appearance.unwrap().kind, EnemyKind::Runner);
        assert_eq!(first, cache.sample(a, false));
        a.tick += 100_000;
        a.events.clear();
        let resumed: Attempt = serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap();
        let later = SceneCache::default().sample(&resumed, false);
        // The settled body is drawn from the saved record, not re-sampled.
        assert!(later.deaths.is_empty());
        assert_eq!(later.settled_corpses, 1);
        assert_eq!(first.deaths[0].id, resumed.corpses[0].id);
        // Corpses read their sprite, tint and scale only from that record, so
        // the save keeps the whole visual, not just its presence.
        assert_eq!(resumed.corpses[0].visual, visual);
        assert_eq!(resumed.corpses, a.corpses);
    }
    /// `count` saved bodies of the fixture's kind: the newest `fresh` died
    /// within the collapse window, the rest long ago, at ascending ticks.
    fn stage_bodies(a: &mut Attempt, count: usize, fresh: usize) {
        a.tick = 100_000;
        let template = a.enemies[0].clone();
        a.corpses = (0..count)
            .map(|n| horde_core::sim::CorpseRecord {
                id: 10_000 + n as u64,
                kind: template.kind,
                boss_kind: None,
                at: [5.0 + (n % 20) as f32, 0.5 + (n % 10) as f32],
                visual: template.resolved.as_ref().map(|v| v.visual.clone()),
                tick: if n + fresh < count {
                    n as u64
                } else {
                    a.tick - COLLAPSE_TICKS + 1 + ((n + fresh - count) / 3) as u64
                },
            })
            .collect();
    }
    #[test]
    fn scene_animation_samples_only_collapsing_corpses() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        stage_bodies(a, 1_500, 50);
        let before = serde_json::to_string(a).unwrap();
        let sample = SceneAnimation::sample(a, false);
        assert_eq!(sample.deaths.len(), 50);
        let newest: Vec<u64> = a.corpses[1_450..].iter().map(|c| c.id).collect();
        let sampled: Vec<u64> = sample.deaths.iter().map(|d| d.id).collect();
        assert_eq!(sampled, newest, "the collapsing bodies are the newest");
        assert!(sample.deaths.iter().all(|d| d.collapse < 1.));
        assert_eq!(sample.settled_corpses, 1_450);
        // Reduced effects show every body settled: nothing collapses.
        let reduced = SceneAnimation::sample(a, true);
        assert!(reduced.deaths.is_empty());
        assert_eq!(reduced.settled_corpses, 1_500);
        // A burst larger than the cap collapses only its newest bodies.
        stage_bodies(a, 1_500, 200);
        let burst = SceneAnimation::sample(a, false);
        assert_eq!(
            burst.deaths.len(),
            crate::perf_budget::CORPSES_COLLAPSING_DRAWN
        );
        assert_eq!(
            burst.settled_corpses,
            1_500 - crate::perf_budget::CORPSES_COLLAPSING_DRAWN
        );
        assert_eq!(burst.deaths.last().unwrap().id, a.corpses[1_499].id);
        // Saved records are only read.
        stage_bodies(a, 1_500, 50);
        assert_eq!(serde_json::to_string(a).unwrap(), before);
    }
    #[test]
    fn resolved_attack_events_drive_lunges_and_recoil_without_moving_the_anchors() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        let id = a.enemies[0].id;
        a.enemies[0].attack_phase = EnemyPhase::Windup;
        a.enemies[0].windup_total = 60;
        a.enemies[0].attack_left = 5;
        a.events = vec![
            Event {
                kind: EventKind::Shot,
                from: [0.5, 5.5],
                to: [20., 4.5],
                value: 0,
                tick: a.tick,
                actor_id: None,
                target_id: Some(id),
                lane: Some(4),
                duration_ticks: 0,
                seq: 0,
            },
            Event {
                kind: EventKind::Damage,
                from: [20., 4.5],
                to: [0.5, 5.5],
                value: 4,
                tick: a.tick,
                actor_id: Some(id),
                target_id: None,
                lane: Some(4),
                duration_ticks: 0,
                seq: 0,
            },
        ]
        .into();
        let before = serde_json::to_string(a).unwrap();
        let frame = SceneAnimation::sample(a, false);
        assert_eq!(pose(&frame, id).offset, [0.; 2]);
        assert!(pose(&frame, id).lunge > 0.);
        assert!(pose(&frame, id).lean < 0.);
        assert!(frame.defender.recoil > 0.);
        assert!(frame.defender.hit > 0.);
        assert_eq!(serde_json::to_string(a).unwrap(), before);
        a.tick += 15;
        let settled = SceneAnimation::sample(a, false);
        assert_eq!(settled.defender.recoil, 0.);
        assert_eq!(pose(&settled, id).lunge, 0.);
    }
    #[test]
    fn a_full_defeat_burst_keeps_every_body_and_bounds_only_transient_effects() {
        let mut game = fixture();
        let mut a = game.attempt_slot_mut().take().unwrap();
        for n in 1..64 {
            game.spawn_enemy(&mut a, EnemyKind::Shambler, (n % 11) as u8, false);
            a.enemies.last_mut().unwrap().x = 20.;
        }
        a.corpses = a
            .enemies
            .drain(..)
            .map(|e| horde_core::sim::CorpseRecord {
                id: e.id,
                kind: e.kind,
                boss_kind: e.boss_kind,
                at: [e.x, e.y()],
                visual: e.resolved.map(|v| v.visual),
                tick: a.tick,
            })
            .collect();
        let mut cache = SceneCache::default();
        let before = serde_json::to_string(&a).unwrap();
        let first = cache.sample(&a, false);
        assert_eq!(first.deaths.len(), 64);
        assert!(first.particles.len() <= MAX_PARTICLES);
        assert_eq!(first, cache.sample(&a, false));
        assert_eq!(before, serde_json::to_string(&a).unwrap());
        a.tick += 500_000;
        let settled = cache.sample(&a, true);
        assert!(settled.deaths.is_empty());
        assert_eq!(settled.settled_corpses, 64);
        assert_eq!(a.corpses.len(), 64);
    }
    #[test]
    fn loading_a_different_campaign_at_the_same_tick_does_not_leak_old_corpses() {
        let mut game = fixture();
        let mut cache = SceneCache::default();
        cache.sample(game.attempt().unwrap(), false);
        game.set_paused(false);
        game.attempt_mut().unwrap().enemies[0].hp = 1;
        game.tick(Input {
            fire: true,
            aim: [20., 4.5],
            ..Input::default()
        });
        assert_eq!(cache.sample(game.attempt().unwrap(), false).deaths.len(), 1);
        let mut fresh = fixture();
        fresh.attempt_mut().unwrap().tick = 1;
        assert!(
            cache
                .sample(fresh.attempt().unwrap(), false)
                .deaths
                .is_empty()
        );
    }
    #[test]
    fn unrelated_new_events_do_not_teleport_an_existing_casing_trajectory() {
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.tick = 10;
        a.events = vec![Event {
            kind: EventKind::Shot,
            from: [0.5, 5.5],
            to: [20., 4.5],
            value: 0,
            tick: 4,
            actor_id: None,
            target_id: Some(a.enemies[0].id),
            lane: Some(4),
            duration_ticks: 0,
            seq: 0,
        }]
        .into();
        let before: Vec<_> = SceneAnimation::sample(a, false)
            .particles
            .into_iter()
            .filter(|p| p.kind == ParticleKind::Casing)
            .collect();
        assert_eq!(before.len(), 2);
        a.events.push_back(Event {
            kind: EventKind::Damage,
            from: [1.5, 0.5],
            to: [0.5, 5.5],
            value: 1,
            tick: 10,
            actor_id: Some(99),
            target_id: None,
            lane: Some(0),
            duration_ticks: 0,
            seq: 0,
        });
        let after: Vec<_> = SceneAnimation::sample(a, false)
            .particles
            .into_iter()
            .filter(|p| p.kind == ParticleKind::Casing)
            .collect();
        assert_eq!(before, after);
    }
    /// W1-69 review: impact sparks and fresh-body dust start on the drawn,
    /// crowd-staggered hostile, as its pose and body do; an impact on any
    /// other target (a structure) keeps the event point.
    #[test]
    fn impact_and_death_particles_follow_the_crowd_stagger() {
        use crate::battle_view::crowd_stagger;
        let mut game = fixture();
        let a = game.attempt_mut().unwrap();
        a.tick = 30;
        let live = a.enemies[0].id;
        let dead = live + 1000;
        let structure = live + 2000;
        a.corpses = vec![horde_core::sim::CorpseRecord {
            id: dead,
            kind: EnemyKind::Runner,
            boss_kind: None,
            at: [14., 2.5],
            visual: None,
            tick: 30,
        }];
        let armor = |to: [f32; 2], target: u64| Event {
            kind: EventKind::Armor,
            from: [0.5, 5.5],
            to,
            value: 0,
            tick: 30,
            actor_id: None,
            target_id: Some(target),
            lane: Some(4),
            duration_ticks: 0,
            seq: 0,
        };
        a.events = vec![
            armor([20., 4.5], live),
            armor([14., 2.5], dead),
            armor([9., 6.5], structure),
        ]
        .into();
        let frame = SceneAnimation::sample(a, false);
        let sparks_at = |to: [f32; 2]| {
            frame
                .particles
                .iter()
                .filter(|p| p.kind == ParticleKind::Spark)
                .filter(|p| (p.at[0] - to[0]).abs() < 0.5 && (p.at[1] - to[1]).abs() < 0.5)
                .map(|p| p.at)
                .collect::<Vec<_>>()
        };
        let staggered = |at: [f32; 2], id: u64| {
            let [dx, dy] = crowd_stagger(id);
            [at[0] + dx, at[1] + dy]
        };
        assert_eq!(sparks_at([20., 4.5]), vec![staggered([20., 4.5], live); 3]);
        assert_eq!(sparks_at([14., 2.5]), vec![staggered([14., 2.5], dead); 3]);
        assert_eq!(sparks_at([9., 6.5]), vec![[9., 6.5]; 3]);
        let body = staggered([14., 2.5 + 0.22], dead);
        let near = |at: [f32; 2]| (at[0] - body[0]).abs() < 1e-4 && (at[1] - body[1]).abs() < 1e-4;
        assert_eq!(
            frame
                .particles
                .iter()
                .filter(|p| p.kind == ParticleKind::Dust && near(p.at))
                .count(),
            3
        );
    }
    /// W2-36 review (design enemy-balance §6): a boss holding after a phase
    /// transition stands still in the core, so it does not walk in place:
    /// its Advance pose is the standing pose for the whole hold, normal and
    /// reduced, and its walk resumes when the hold ends.
    #[test]
    fn held_boss_does_not_walk_in_place() {
        let mut game = fixture();
        let mut a = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut a, EnemyKind::Brute, 4, true);
        let boss = a.enemies.iter_mut().find(|e| e.boss).unwrap();
        let id = boss.id;
        boss.x = 20.;
        boss.attack_phase = EnemyPhase::Advance;
        boss.boss_state = Some(horde_core::sim::BossState {
            profile: horde_core::sim::BossPhaseProfile {
                name: "Breach".into(),
                speed_mult: 1.4,
                armor_mult: 0.6,
                call_interval_mult: 1.0,
                siege_recovery_mult: 1.0,
                hold_ticks: 90,
            },
            transitions: 1,
            entered: 1,
            hold_until: 190,
        });
        *game.attempt_slot_mut() = Some(a);
        let a = game.attempt_mut().unwrap();
        for reduced in [false, true] {
            a.tick = 100;
            let standing = pose(&SceneAnimation::sample(a, reduced), id);
            assert!(!standing.moving, "reduced {reduced}: held at 100");
            for tick in 101..190 {
                a.tick = tick;
                assert_eq!(
                    pose(&SceneAnimation::sample(a, reduced), id),
                    standing,
                    "reduced {reduced}: the held boss keeps its pose at {tick}"
                );
            }
            a.tick = 190;
            assert!(pose(&SceneAnimation::sample(a, reduced), id).moving);
        }
        // Normal effects: the walk cycle runs again after the hold.
        let frames: std::collections::BTreeSet<u8> = (190..290)
            .map(|tick| {
                a.tick = tick;
                pose(&SceneAnimation::sample(a, false), id).frame
            })
            .collect();
        assert!(frames.len() > 1, "the boss walks after its hold");
    }
}
