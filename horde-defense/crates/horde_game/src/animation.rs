//! Bounded cosmetic sampling. Tick/IDs/events are read-only inputs; no clocks,
//! random draws, action decisions or hit geometry are written back to the core.
use horde_core::{
    content::{BossKind, EnemyKind, Mode, StructureKind, content},
    enemy_catalog::EnemyVisual,
    ordnance::OrdnanceKind,
    sim::{Attempt, Enemy, EnemyPhase, Event, EventKind, Structure},
    world::world,
};
use std::collections::{BTreeMap, VecDeque};

pub const MAX_SCENES: usize = 8;
pub const MAX_PARTICLES: usize = 128;
const EVENT_BUDGET: usize = 64;

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
#[derive(Clone, Debug, PartialEq)]
pub struct DeathPose {
    pub id: u64,
    pub at: [f32; 2],
    pub appearance: Option<Appearance>,
    pub visual: Option<EnemyVisual>,
    pub frame: u8,
    pub alpha: f32,
    pub collapse: f32,
    pub lean: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneAnimation {
    pub tick: u64,
    pub enemies: BTreeMap<u64, ActorPose>,
    pub defender: DefenderPose,
    pub structures: BTreeMap<u64, StructurePose>,
    pub deaths: Vec<DeathPose>,
    pub particles: Vec<Particle>,
    pub weather: WeatherKind,
}
impl SceneAnimation {
    pub fn sample(a: &Attempt, reduced: bool) -> Self {
        let mut frame = Self {
            tick: a.tick,
            enemies: BTreeMap::new(),
            defender: defender_pose(a, reduced),
            structures: BTreeMap::new(),
            deaths: vec![],
            particles: vec![],
            weather: scene_weather(a),
        };
        for e in &a.enemies {
            frame.enemies.insert(e.id, enemy_pose(a, e, reduced));
        }
        for s in &a.structures {
            frame.structures.insert(s.id, structure_pose(a, s, reduced));
        }
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

fn sample_deaths(a: &Attempt, reduced: bool, frame: &mut SceneAnimation) {
    frame.deaths.reserve(a.corpses.len());
    for corpse in &a.corpses {
        let age = a.tick.saturating_sub(corpse.tick) as f32;
        let collapse = if reduced {
            1.
        } else {
            smooth((age / 18.).min(1.))
        };
        frame.deaths.push(DeathPose {
            id: corpse.id,
            at: corpse.at,
            appearance: Some(Appearance {
                kind: corpse.kind,
                boss: corpse.boss_kind.is_some(),
                boss_kind: corpse.boss_kind,
            }),
            visual: corpse.visual.clone(),
            frame: 0,
            alpha: 1.,
            collapse,
            lean: if reduced { 0. } else { -0.85 * collapse },
        });
        if !reduced && age < 36. && frame.particles.len() < MAX_PARTICLES {
            let t = age / 36.;
            for n in 0..3 {
                push_particle(
                    &mut frame.particles,
                    Particle {
                        kind: ParticleKind::Dust,
                        at: [
                            corpse.at[0] + (noise(corpse.id.wrapping_add(n)) - 0.5) * t * 0.7,
                            corpse.at[1] + 0.22 - t * 0.25,
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

fn enemy_pose(a: &Attempt, e: &Enemy, reduced: bool) -> ActorPose {
    let moving = e.attack_phase == EnemyPhase::Advance && !e.injury.prone && !e.injury.stunned;
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
    let pose_tick = if e.hit_stop_left > 0 {
        a.tick.saturating_sub(u64::from(
            horde_core::sim::ENEMY_HIT_STOP_TICKS.saturating_sub(e.hit_stop_left),
        ))
    } else {
        a.tick
    };
    let clock = pose_tick.wrapping_add(e.id.wrapping_mul(7));
    let phase = (clock % cycle) as f32 / cycle as f32 * std::f32::consts::TAU;
    let hit = latest(a, |event| {
        event.target_id == Some(e.id)
            && event.value > 0
            && matches!(event.kind, EventKind::Hit | EventKind::WeakPoint)
    })
    .map_or(0., |event| pulse(a.tick, event.tick, 12));
    let lunge = latest(a, |event| {
        event.actor_id == Some(e.id)
            && matches!(
                event.kind,
                EventKind::Damage | EventKind::Miss | EventKind::Defended | EventKind::Armor
            )
    })
    .map_or(0., |event| pulse(a.tick, event.tick, 14));
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
    let nearest = a
        .enemies
        .iter()
        .filter(|e| {
            e.in_view() && (e.x - s.x()).hypot(e.y() - s.y()) <= content().structure(s.kind).range
        })
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
                            event.to[0] + theta.cos() * age * 0.85,
                            event.to[1] + theta.sin() * age * 0.5,
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
    fn fixture() -> Game {
        let mut game = Game::default();
        game.campaign
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        game.campaign.attempts = 1;
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt.take().unwrap();
        game.spawn_enemy(&mut a, EnemyKind::Runner, 4, false);
        a.enemies[0].x = 20.;
        game.attempt = Some(a);
        game
    }
    #[test]
    fn tick_based_walk_poses_vary_without_mutating_authority() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
        let before = serde_json::to_string(a).unwrap();
        let first = SceneAnimation::sample(a, false);
        assert_eq!(first, SceneAnimation::sample(a, false));
        assert_eq!(before, serde_json::to_string(a).unwrap());
        a.tick += 7;
        let next = SceneAnimation::sample(a, false);
        assert_ne!(
            first.enemies[&a.enemies[0].id].frame,
            next.enemies[&a.enemies[0].id].frame
        );
    }
    #[test]
    fn turret_holds_the_resolved_target_while_its_tracer_is_visible() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
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
        let first = cache.sample(game.attempt.as_ref().unwrap(), false);
        for _ in 0..60 {
            game.tick(Input::default());
            assert_eq!(first, cache.sample(game.attempt.as_ref().unwrap(), false));
        }
        assert_eq!(before, serde_json::to_string(&game).unwrap());
    }
    #[test]
    fn reduced_effects_suppress_motion_and_particles_but_keep_windup_and_reload_information() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
        a.enemies[0].attack_phase = EnemyPhase::Windup;
        a.enemies[0].windup_total = 60;
        a.enemies[0].attack_left = 15;
        a.reload_left = 30;
        let sample = SceneAnimation::sample(a, true);
        let pose = sample.enemies[&a.enemies[0].id];
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
        let a = game.attempt.as_mut().unwrap();
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
            let a = game.attempt.as_mut().unwrap();
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
    #[test]
    fn authoritative_remains_persist_without_events_and_after_save_resume() {
        let mut game = fixture();
        let mut cache = SceneCache::default();
        let a = game.attempt.as_mut().unwrap();
        let enemy = a.enemies.remove(0);
        a.corpses.push(horde_core::sim::CorpseRecord {
            id: enemy.id,
            kind: enemy.kind,
            boss_kind: enemy.boss_kind,
            at: [enemy.x, enemy.y()],
            visual: enemy.resolved.map(|v| v.visual),
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
        assert_eq!(later.deaths.len(), 1);
        assert_eq!(later.deaths[0].alpha, 1.);
        assert_eq!(later.deaths[0].collapse, 1.);
        assert_eq!(later.deaths[0].visual, first.deaths[0].visual);
    }
    #[test]
    fn resolved_attack_events_drive_lunges_and_recoil_without_moving_the_anchors() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
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
        assert_eq!(frame.enemies[&id].offset, [0.; 2]);
        assert!(frame.enemies[&id].lunge > 0.);
        assert!(frame.enemies[&id].lean < 0.);
        assert!(frame.defender.recoil > 0.);
        assert!(frame.defender.hit > 0.);
        assert_eq!(serde_json::to_string(a).unwrap(), before);
        a.tick += 15;
        let settled = SceneAnimation::sample(a, false);
        assert_eq!(settled.defender.recoil, 0.);
        assert_eq!(settled.enemies[&id].lunge, 0.);
    }
    #[test]
    fn a_full_defeat_burst_keeps_every_body_and_bounds_only_transient_effects() {
        let mut game = fixture();
        let mut a = game.attempt.take().unwrap();
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
        assert_eq!(cache.sample(&a, true).deaths.len(), 64);
    }
    #[test]
    fn loading_a_different_campaign_at_the_same_tick_does_not_leak_old_corpses() {
        let mut game = fixture();
        let mut cache = SceneCache::default();
        cache.sample(game.attempt.as_ref().unwrap(), false);
        game.set_paused(false);
        game.attempt.as_mut().unwrap().enemies[0].hp = 1;
        game.tick(Input {
            fire: true,
            aim: [20., 4.5],
            ..Input::default()
        });
        assert_eq!(
            cache
                .sample(game.attempt.as_ref().unwrap(), false)
                .deaths
                .len(),
            1
        );
        let mut fresh = fixture();
        fresh.attempt.as_mut().unwrap().tick = 1;
        assert!(
            cache
                .sample(fresh.attempt.as_ref().unwrap(), false)
                .deaths
                .is_empty()
        );
    }
    #[test]
    fn unrelated_new_events_do_not_teleport_an_existing_casing_trajectory() {
        let mut game = fixture();
        let a = game.attempt.as_mut().unwrap();
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
}
