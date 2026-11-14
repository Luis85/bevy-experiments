//! Defender power (CD11, design-campaign-defenders 1.11): one comparable
//! number for any defender in either combat model, read from the very
//! inputs a deploy starts from, so it cannot drift from what deploys. Pure:
//! no RNG and no mutation.
//!
//! The Npc model (W4-16, ruling R62) plays the stage the inputs describe as
//! a deterministic fluid of hostiles, one one-second turn at a time: the
//! arrivals of the stage's schedule walk in from the far edge, the defender
//! spends one action a turn on the nearest, at the range-penalised hit
//! chance after the Aim turns that pay, the backlog it cannot kill reaches
//! the line and hurts it, and the stage is won once the boss falls. The
//! rating is the heaviest arrival load the defender still wins at, against
//! a fresh Mara's.
use super::npc::aim_turns_that_pay;
use super::*;
use crate::balance::BalanceSettings;
use crate::campaign::{Npc, mean_basic_damage};
use crate::command::CommandError;
use crate::defenders::{DefenderPower, PowerBand, recruit_identity};
use crate::enemy_catalog::enemy_catalog;
use crate::progression::ProgressionSnapshot;
use crate::rules::{attack_success_probability, threshold_probability};
use std::sync::OnceLock;

/// The longest expected survival a power reading counts, in seconds: a
/// defender the reference enemy cannot hurt reads as this, not infinity.
const MAX_SURVIVAL_S: f32 = 3_600.0;

/// What a deploy of `defender` starts from at victory threat `threat`, before
/// the country's modifiers: the defender's profile with the balance preset's
/// defender scaling (a recruit's carries its own appearance), its maximum
/// health, the progression snapshot, the balance settings, the catalogue
/// level whose schedule the stage plays and the arena it is fought in. A
/// deploy at the next threat ([`Game::next_threat`]) copies these into the
/// attempt; on a country whose modifiers leave the defender alone the
/// attempt's profile is exactly `profile`. The snapshot is the rank's,
/// before the stage's front position: a deploy on stage 0 of a multi-stage
/// front freezes exactly it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeploymentInputs {
    pub defender: DefenderId,
    pub threat: u32,
    pub profile: DefenderProfile,
    pub snapshot: ProgressionSnapshot,
    pub max_hp: i32,
    pub balance: BalanceSettings,
    /// The catalogue level (`content().levels`) the stage plays: its
    /// arrival schedule, boss time and boss. 0 for the rank-only reading.
    pub level: usize,
    /// The arena (`Campaign::arena_geometry`): lanes and field depth.
    pub lanes: u8,
    pub width_yards: f32,
}

/// The inputs for `defender` (`character` wearing `loadout`) in `campaign`,
/// on catalogue `level` at `front` (None: no country, or the rank-only
/// reading). Shared by [`Game::deploy_as`] and [`Game::deployment_inputs`].
pub(super) fn deployment_inputs_for(
    campaign: &Campaign,
    defender: DefenderId,
    loadout: [u64; 4],
    character: &Npc,
    threat: u32,
    level: usize,
    front: Option<crate::progression::FrontPosition>,
) -> Result<DeploymentInputs, String> {
    let mode = defender.mode();
    let snapshot = campaign.progression_for(mode, threat, campaign.lane_count(), front)?;
    let mut profile = campaign.profile_for_loadout(mode, loadout, character);
    if let Some(id) = defender.recruit() {
        profile.appearance = recruit_identity(id).1;
    }
    let balance = campaign.balance_settings.clone();
    profile.personal.damage = (profile.personal.damage as f32 * balance.player_damage)
        .round()
        .max(1.0) as i32;
    profile.personal.melee_damage = (profile.personal.melee_damage as f32 * balance.player_damage)
        .round()
        .max(1.0) as i32;
    profile.personal.health = (profile.personal.health as f32 * balance.defender_health)
        .round()
        .max(1.0) as i32;
    profile.max_hp = (profile.max_hp as f32 * balance.defender_health)
        .round()
        .max(1.0) as i32;
    let expected = mean_basic_damage(Damage {
        dice: profile.npc.damage_dice,
        adds: profile.npc.damage_add,
        kind: profile.npc.damage_kind,
    });
    profile.npc.damage_add = (profile.npc.damage_add as f32
        + expected * (balance.player_damage - 1.0))
        .round()
        .clamp(-30.0, 1_000_000.0) as i32;
    let expected = mean_basic_damage(Damage {
        dice: profile.npc_melee_damage_dice,
        adds: profile.npc_melee_damage_add,
        kind: profile.npc_melee_kind,
    });
    profile.npc_melee_damage_add = (profile.npc_melee_damage_add as f32
        + expected * (balance.player_damage - 1.0))
        .round()
        .clamp(-30.0, 1_000_000.0) as i32;
    let max_hp = if mode == Mode::Personal {
        profile.personal.health
    } else {
        profile.max_hp
    };
    let (lanes, width_yards) = campaign.arena_geometry();
    Ok(DeploymentInputs {
        defender,
        threat,
        profile,
        snapshot,
        max_hp,
        balance,
        level,
        lanes,
        width_yards,
    })
}

impl DeploymentInputs {
    /// The power these inputs give their defender.
    pub fn power(&self) -> DefenderPower {
        let (kill_rate, survival_s, ratio) = match self.defender.mode() {
            Mode::Personal => {
                let (kill_rate, survival_s) = self.personal_model();
                let (kill_ref, survival_ref) = reference().personal;
                let ratio = ((kill_rate / kill_ref) * (survival_s / survival_ref)).sqrt();
                (kill_rate, survival_s, ratio)
            }
            Mode::Npc => {
                let model = NpcModel::new(self);
                (
                    model.kill_rate(0.5 * model.width),
                    model.survival_s(),
                    model.load_held() / reference().npc_load,
                )
            }
        };
        let rating = (100.0 * ratio).round();
        let rating = if rating.is_nan() {
            1
        } else {
            rating.clamp(1.0, 999.0) as u16
        };
        DefenderPower {
            defender: self.defender,
            threat: self.threat,
            kills_per_min: kill_rate * 60.0,
            survival_s,
            rating,
            band: PowerBand::for_rating(rating),
        }
    }
    /// Personal: expected kills per second against the reference enemy (a
    /// Shambler) and expected seconds survived against its arrivals.
    fn personal_model(&self) -> (f32, f32) {
        // `resolve_kind(Shambler)`, read in place rather than cloned.
        let enemy = enemy_catalog().default_profile(EnemyKind::Shambler);
        let enemy_health = self.snapshot.enemy_health * self.balance.enemy_health;
        let enemy_damage = self.snapshot.enemy_damage * self.balance.enemy_damage;
        let arrival = self.snapshot.arrival_multiplier * self.balance.spawn_rate;
        let p = &self.profile.personal;
        let magazine = p.magazine.max(1) as f32;
        let dps = (p.damage - enemy.armor).max(1) as f32 * (1.0 + 0.5 * p.weak_bonus) * magazine
            / (magazine * p.cadence + p.reload).max(f32::MIN_POSITIVE);
        let kill_rate = dps / (enemy.health.max(1) as f32 * enemy_health);
        let hit = ((enemy.damage as f32 * enemy_damage).round() as i32 - p.armor).max(1);
        let incoming = hit as f32 / enemy.windup.max(0.5) * arrival;
        let survival_s = if incoming > 0.0 {
            (self.max_hp as f32 / incoming).min(MAX_SURVIVAL_S)
        } else {
            MAX_SURVIVAL_S
        };
        (kill_rate, survival_s)
    }
}

/// The Personal model's kill rate and survival for a fresh starter at
/// threat 0, and the load a fresh Mara holds, both from the default
/// campaign, so a fresh starter rates exactly 100 in either model.
struct Reference {
    personal: (f32, f32),
    npc_load: f32,
}

fn reference() -> &'static Reference {
    static REFERENCE: OnceLock<Reference> = OnceLock::new();
    REFERENCE.get_or_init(|| {
        let campaign = Campaign::default();
        let inputs = |defender, loadout| {
            deployment_inputs_for(&campaign, defender, loadout, &campaign.npc, 0, 0, None)
                .expect("threat 0 is always supported")
        };
        let (kill_rate, survival_s) =
            inputs(DefenderId::Player, campaign.player.loadout).personal_model();
        let npc_load = NpcModel::new(&inputs(DefenderId::Mara, campaign.npc.loadout)).load_held();
        Reference {
            personal: (
                kill_rate.max(f32::MIN_POSITIVE),
                survival_s.max(f32::MIN_POSITIVE),
            ),
            npc_load: npc_load.max(f32::MIN_POSITIVE),
        }
    })
}

/// Where the Npc defender stands (`Attempt::defender_position`), yards
/// from the near edge.
const DEFENDER_X: f32 = 0.5;
/// How much nearer the boss counts as a target (`select_npc_target`).
const BOSS_URGENCY_YD: f32 = 12.0;
/// The most Aim turns any policy takes before a shot (the Marksman's).
const MAX_AIM_TURNS: u8 = 2;
/// The range penalties the turn table covers (0 to -12, out to 150 yards);
/// a longer shot reads the last.
const MAX_PENALTY: usize = 12;
/// The heaviest arrival load (times the stage's own) the hold search reads,
/// and its bisection steps.
const MAX_LOAD: f32 = 4.0;
const LOAD_STEPS: u32 = 10;
/// How long past the boss's arrival a modelled stage may run before the
/// model calls it lost, in seconds.
const BOSS_FIGHT_S: u32 = 600;

/// One wave tier's hostiles as the Npc model reads them, members-weighted.
#[derive(Clone, Copy, Default)]
struct Mix {
    /// Landed attack rolls per hostile killed (the dodge folded in).
    shots_per_kill: f32,
    /// Seconds a hostile takes to walk a yard.
    pace: f32,
    /// Expected injury a turn one hostile in contact deals the defender.
    contact_dps: f32,
    /// Hostiles per pick.
    members: f32,
}
impl Mix {
    /// The harder of two mixes in every term, so a deeper tier never reads
    /// easier (and power never rises with threat).
    fn worst(self, other: Self) -> Self {
        Self {
            shots_per_kill: self.shots_per_kill.max(other.shots_per_kill),
            pace: self.pace.min(other.pace),
            contact_dps: self.contact_dps.max(other.contact_dps),
            members: self.members.max(other.members),
        }
    }
}

/// The Npc stage the inputs describe, ready to play turn by turn.
struct NpcModel {
    /// Hits landed a turn at each range penalty, one action a turn: the
    /// Aim turns that pay and the reload turns are spent from it.
    hits_per_turn: [f32; MAX_PENALTY + 1],
    /// Hostiles killed a turn at each range step (`hits_per_turn` over the
    /// mix's landed shots per kill).
    kills_per_turn: [f32; MAX_PENALTY + 1],
    /// The farthest a hostile may stand, in yards from the near edge, for
    /// each range step: a shot at `x` reads the first step with `x` at most
    /// its entry (the lane offset folded in).
    x_steps: [f32; MAX_PENALTY],
    mix: Mix,
    /// The schedule's start rate, growth, rate cap (picks per second) and
    /// boss time (seconds), with the arrival multiplier on top.
    start_rate: f32,
    growth: f32,
    rate_cap: f32,
    boss_time: f32,
    arrival: f32,
    enemy_cap: f32,
    width: f32,
    max_hp: f32,
    boss_shots: f32,
    boss_pace: f32,
    boss_dps: f32,
}

enum StageEnd {
    Won,
    Lost(u32),
    Held,
}

impl NpcModel {
    fn new(inputs: &DeploymentInputs) -> Self {
        let p = &inputs.profile;
        let n = &p.npc;
        let b = &inputs.balance;
        let s = &inputs.snapshot;
        let levels = &content().levels;
        let level = &levels[inputs.level.min(levels.len() - 1)];
        let magazine = n.magazine.max(1) as f32;
        let firing = magazine / (magazine + n.reload_turns as f32);
        let skill = p.ranged_skill + n.skill_bonus;
        let hits_per_turn = std::array::from_fn(|penalty| {
            let skill = skill - penalty as i32;
            let aim = aim_turns_that_pay(skill, n.accuracy, 0, MAX_AIM_TURNS);
            attack_success_probability(skill + rules::aim_bonus(n.accuracy, aim))
                / f32::from(aim + 1)
                * firing
        });
        let weapon = Injury::new(Damage {
            dice: n.damage_dice,
            adds: n.damage_add,
            kind: n.damage_kind,
        });
        let enemy_health = s.enemy_health * b.enemy_health;
        let speed = s.enemy_speed * b.enemy_speed;
        let defender_dodge = 1.0 - threshold_probability(p.stats.dodge());
        // What one hostile's (or the boss's) blow does to this defender a
        // turn, with the battle's damage scaling (`sim/combat.rs`).
        let blow = |injury: &Injury, skill: i32, rank_adds: i32| {
            let adds =
                injury.damage.adds + ((s.enemy_damage - 1.0) * 6.0).floor() as i32 + rank_adds;
            let adds = (adds as f32 + injury.basic(adds) * (b.enemy_damage - 1.0))
                .round()
                .clamp(-30.0, 1_000_000.0) as i32;
            attack_success_probability(skill) * defender_dodge * injury.mean(adds, n.armor)
        };
        let shots = |hp: f32, armor: i32, dodge: i32| {
            let injury = weapon.mean(weapon.damage.adds, armor);
            let hits = if injury > 1e-3 {
                (0.5 + hp / injury).max(1.0)
            } else {
                1e4
            };
            hits / (1.0 - threshold_probability(dodge)).max(1e-3)
        };
        let pool = pool();
        // Each hostile kind once, then every tier up to the stage's as a
        // members-weighted mix of them.
        let kinds: Vec<Mix> = pool
            .kinds
            .iter()
            .map(|k| Mix {
                shots_per_kill: shots(k.hp * enemy_health, k.armor, k.dodge),
                pace: 1.0 / (k.speed * speed).max(1e-3),
                contact_dps: blow(&k.blow, k.skill, 0),
                members: 1.0,
            })
            .collect();
        let tier = inputs.level.max(s.wave_tier).min(pool.tiers.len() - 1);
        let mix = pool.tiers[..=tier]
            .iter()
            .map(|tier| {
                let mean = |f: fn(&Mix) -> f32| {
                    tier.members
                        .iter()
                        .map(|(kind, weight)| weight * f(&kinds[*kind]))
                        .sum::<f32>()
                        / tier.members_weight
                };
                Mix {
                    shots_per_kill: mean(|k| k.shots_per_kill),
                    pace: mean(|k| k.pace),
                    contact_dps: mean(|k| k.contact_dps),
                    members: tier.members_weight / tier.picks,
                }
            })
            .reduce(Mix::worst)
            .unwrap_or_default();
        let boss_hp = level.boss_npc_hp.max(s.npc_boss_health_floor).max(1) as f32
            * s.boss_health_multiplier
            * enemy_health;
        let brute = enemy_catalog().default_profile(EnemyKind::Brute);
        let kills_per_turn = hits_per_turn.map(|hits| hits / mix.shots_per_kill.max(1.0));
        Self {
            hits_per_turn,
            kills_per_turn,
            mix,
            start_rate: level.start_rate.max(s.start_rate_floor),
            growth: level.growth,
            rate_cap: level.rate_cap.max(s.rate_cap_floor),
            boss_time: (level.boss_time.max(s.boss_time_floor) * b.boss_time).max(1.0),
            arrival: s.arrival_multiplier * b.spawn_rate * b.npc_spawn_rate,
            enemy_cap: if s.enemy_cap == 0 {
                level.enemy_cap
            } else {
                s.enemy_cap
            } as f32,
            width: inputs.width_yards.max(DEFENSE_LINE + 1.0),
            // A hostile's lane lies a mean quarter of the lanes to the
            // side of the defender's.
            x_steps: {
                let lateral = f32::from(inputs.lanes) * 0.25;
                RANGE_STEPS.map(|step| {
                    let along = step * step - lateral * lateral;
                    if along < 0.0 {
                        f32::NEG_INFINITY
                    } else {
                        DEFENDER_X + along.sqrt()
                    }
                })
            },
            max_hp: inputs.max_hp.max(1) as f32,
            boss_shots: shots(
                boss_hp,
                level.boss_npc_armor + s.npc_boss_armor_add,
                level.boss_npc_dodge,
            ),
            boss_pace: 1.0 / (brute.speed * speed).max(1e-3),
            boss_dps: blow(
                &Injury::new(Damage {
                    dice: level.boss_npc_damage_dice,
                    adds: level.boss_npc_damage_add,
                    kind: DamageKind::Crushing,
                }),
                level.boss_npc_skill,
                ((s.boss_damage_multiplier - 1.0) * 6.0).floor() as i32,
            ),
        }
    }
    /// The range step of a shot at a hostile `x` yards out.
    fn step(&self, x: f32) -> usize {
        if x <= DEFENSE_LINE + 0.01 {
            return 0;
        }
        self.x_steps
            .iter()
            .position(|limit| x <= *limit)
            .unwrap_or(MAX_PENALTY)
    }
    /// Hits landed a turn on a hostile `x` yards out.
    fn hits_at(&self, x: f32) -> f32 {
        self.hits_per_turn[self.step(x)]
    }
    /// Hostiles killed a turn at `x` yards: one action a turn.
    fn kill_rate(&self, x: f32) -> f32 {
        self.kills_per_turn[self.step(x)]
    }
    /// Hostiles arriving in the second after `t` (`Attempt::arrival_rate`).
    fn arrivals(&self, t: f32) -> f32 {
        ((self.start_rate * (1.0 + self.growth * t / self.boss_time)).min(self.rate_cap)
            * self.arrival)
            .min(30.0)
            * self.mix.members
    }
    /// Plays the stage at `load` times its arrivals for at most `horizon`
    /// turns; with `boss` the boss walks in at the boss time and the stage
    /// ends when it falls.
    fn play(&self, load: f32, boss: bool, horizon: u32) -> StageEnd {
        let boss_turn = self.boss_time.ceil() as u32;
        let mut field = 0.0f32;
        let mut damage = 0.0f32;
        let mut boss_x: Option<f32> = None;
        let mut boss_left = self.boss_shots;
        for turn in 0..horizon {
            let arrivals = self.arrivals(turn as f32) * load;
            field = (field + arrivals).min(self.enemy_cap);
            if boss && turn >= boss_turn {
                let x = boss_x.map_or(self.width, |x| x - 1.0 / self.boss_pace);
                boss_x = Some(x.max(DEFENSE_LINE));
            }
            // The hostiles walk in single file at the arrivals' spacing: the
            // approach holds `transit` of them, and the backlog past that
            // stands at the line.
            let per_yard = arrivals * self.mix.pace;
            let transit = per_yard * (self.width - DEFENSE_LINE);
            let contact = (field - transit).max(0.0);
            let nearest = if contact > 0.0 || per_yard <= 0.0 {
                DEFENSE_LINE
            } else {
                (self.width - field / per_yard).max(DEFENSE_LINE)
            };
            // One action: a hostile in contact first, then the boss once it
            // is the nearest target counting its urgency, else the nearest.
            match boss_x {
                Some(x) if contact <= 0.0 && (x - BOSS_URGENCY_YD <= nearest || field < 1.0) => {
                    boss_left -= self.hits_at(x);
                }
                _ => field -= self.kill_rate(nearest).min(field),
            }
            damage += contact * self.mix.contact_dps;
            if boss_x.is_some_and(|x| x <= DEFENSE_LINE) {
                damage += self.boss_dps;
            }
            if damage >= self.max_hp {
                return StageEnd::Lost(turn + 1);
            }
            if boss && boss_left <= 0.0 {
                return StageEnd::Won;
            }
            // Past the boss time the arrivals are steady: with nobody at the
            // line and a kill a turn at the line outpacing them, the line
            // holds for good.
            if !boss && contact <= 0.0 && turn > boss_turn && self.kills_per_turn[0] >= arrivals {
                return StageEnd::Held;
            }
        }
        if boss {
            StageEnd::Lost(horizon)
        } else {
            StageEnd::Held
        }
    }
    fn wins(&self, load: f32) -> bool {
        let horizon = self.boss_time.ceil() as u32 + BOSS_FIGHT_S;
        matches!(self.play(load, true, horizon), StageEnd::Won)
    }
    /// The heaviest load, in multiples of the stage's arrivals, at which the
    /// defender still wins the stage (bisected; 0 when it cannot win even
    /// alone with the boss, [`MAX_LOAD`] at most).
    fn load_held(&self) -> f32 {
        let half = 0.5 * MAX_LOAD;
        let (mut low, mut high) = if self.wins(half) {
            if self.wins(MAX_LOAD) {
                return MAX_LOAD;
            }
            (half, MAX_LOAD)
        } else {
            (0.0, half)
        };
        for _ in 0..LOAD_STEPS {
            let mid = 0.5 * (low + high);
            if self.wins(mid) {
                low = mid;
            } else {
                high = mid;
            }
        }
        0.5 * (low + high)
    }
    /// Seconds the defender lasts against the stage's own arrivals, the
    /// boss left out, with the rate held at its cap past the boss time.
    fn survival_s(&self) -> f32 {
        match self.play(1.0, false, MAX_SURVIVAL_S as u32) {
            StageEnd::Lost(turns) => turns as f32,
            StageEnd::Won | StageEnd::Held => MAX_SURVIVAL_S,
        }
    }
}

/// The longest range of each step of `rules::range_penalty`, 0 to -11
/// (the preferred-number series: 2, 3, 5, 7, 10, 15 yards and on by
/// decades); a longer shot takes -12.
const RANGE_STEPS: [f32; MAX_PENALTY] = [
    2.0, 3.0, 5.0, 7.0, 10.0, 15.0, 20.0, 30.0, 50.0, 70.0, 100.0, 150.0,
];

/// One hostile kind of the wave pools as the Npc model reads it, before
/// rank scaling.
struct PoolKind {
    hp: f32,
    /// Damage resistance against shots (ranged armor included).
    armor: i32,
    dodge: i32,
    speed: f32,
    skill: i32,
    blow: Injury,
}
/// One wave tier: (kind, members per unit of pick weight) rows
/// (`EnemyCatalog::pool_members`).
struct PoolTier {
    members: Vec<(usize, f32)>,
    members_weight: f32,
    picks: f32,
}
struct Pool {
    kinds: Vec<PoolKind>,
    tiers: Vec<PoolTier>,
}

/// Every wave tier's hostiles, read once from the catalogue (no boss
/// unlocks: the pool of a fresh campaign).
fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let catalog = enemy_catalog();
        let mut ids: Vec<&str> = Vec::new();
        let mut kinds = Vec::new();
        let tiers = (0..catalog.wave_count().max(1))
            .map(|tier| {
                let (members, picks) = catalog.pool_members(tier, &[]);
                let members: Vec<(usize, f32)> = members
                    .into_iter()
                    .map(|(weight, e)| {
                        let kind = ids
                            .iter()
                            .position(|id| *id == e.archetype_id)
                            .unwrap_or_else(|| {
                                ids.push(&e.archetype_id);
                                kinds.push(PoolKind {
                                    hp: e.npc_hp.max(1) as f32,
                                    armor: e.npc_armor + e.npc_ranged_armor,
                                    dodge: e.npc_dodge,
                                    speed: e.speed,
                                    skill: e.npc_skill,
                                    blow: Injury::new(Damage {
                                        dice: e.npc_damage_dice,
                                        adds: e.npc_damage_add,
                                        kind: DamageKind::Crushing,
                                    }),
                                });
                                kinds.len() - 1
                            });
                        (kind, weight)
                    })
                    .collect();
                PoolTier {
                    members_weight: members.iter().map(|(_, w)| w).sum::<f32>().max(1e-6),
                    members,
                    picks: picks.max(1e-6),
                }
            })
            .collect();
        Pool { kinds, tiers }
    })
}

/// The injury a weapon's hits deal, by the target's damage resistance: the
/// dice distribution, the minimum damage and the wound multiplier.
struct Injury {
    damage: Damage,
    /// Ways to roll each dice total (index = total).
    counts: Vec<u32>,
    total: f32,
}
impl Injury {
    /// Up to 8 dice are rolled out exactly; past that the spread hardly
    /// matters and each extra die adds its mean.
    fn new(damage: Damage) -> Self {
        let mut counts = vec![1u32];
        for _ in 0..damage.dice.min(8) {
            let mut next = vec![0u32; counts.len() + 6];
            for (sum, count) in counts.iter().enumerate() {
                for face in 1..=6 {
                    next[sum + face] += count;
                }
            }
            counts = next;
        }
        let extra = 3.5 * f32::from(damage.dice.saturating_sub(8));
        let total = counts.iter().sum::<u32>() as f32;
        Self {
            damage: Damage {
                adds: damage.adds.saturating_add(extra.round() as i32),
                ..damage
            },
            counts,
            total,
        }
    }
    /// The mean basic damage of one roll with `adds` (the minimum applied).
    fn basic(&self, adds: i32) -> f32 {
        let minimum = self.damage.minimum();
        self.weighted(|sum| sum.saturating_add(adds).max(minimum) as f32)
    }
    /// The mean injury of one hit with `adds` through `dr`.
    fn mean(&self, adds: i32, dr: i32) -> f32 {
        let minimum = self.damage.minimum();
        self.weighted(|sum| {
            self.damage.injury(
                sum.saturating_add(adds)
                    .max(minimum)
                    .saturating_sub(dr.max(0)),
            ) as f32
        })
    }
    fn weighted(&self, f: impl Fn(i32) -> f32) -> f32 {
        self.counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(sum, count)| *count as f32 * f(sum as i32))
            .sum::<f32>()
            / self.total
    }
}

impl Game {
    /// The victory threat the next defense faces: the campaign's
    /// progression rank, the rank every deploy reads.
    pub fn next_threat(&self) -> u32 {
        self.campaign.progression_rank()
    }
    /// What a deploy of `defender` at victory threat `threat` starts from
    /// (see [`DeploymentInputs`]). `UnknownSurvivor` for a recruit not on
    /// the roster; `Rejected` past the supported threat. Pure.
    pub fn deployment_inputs(
        &self,
        defender: DefenderId,
        threat: u32,
    ) -> Result<DeploymentInputs, CommandError> {
        let (loadout, character) = match defender {
            DefenderId::Player => (self.campaign.player.loadout, &self.campaign.npc),
            DefenderId::Mara => (self.campaign.npc.loadout, &self.campaign.npc),
            DefenderId::Recruit(id) => {
                let recruit = self
                    .strategy
                    .roster
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(CommandError::UnknownSurvivor)?;
                (recruit.npc.loadout, &recruit.npc)
            }
        };
        deployment_inputs_for(
            &self.campaign,
            defender,
            loadout,
            character,
            threat,
            0,
            None,
        )
        .map_err(CommandError::Rejected)
    }
    /// `defender`'s power against victory threat `next_threat` (normally
    /// [`Self::next_threat`]), whether or not it could deploy now. Pure: no
    /// RNG, no mutation. Errors as [`Self::deployment_inputs`].
    pub fn defender_power(
        &self,
        defender: DefenderId,
        next_threat: u32,
    ) -> Result<DefenderPower, CommandError> {
        Ok(self.deployment_inputs(defender, next_threat)?.power())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_steps_are_the_range_table() {
        // A shot at a hostile in contact is fired at 1 yard (`npc_turn`):
        // the first step, which `NpcModel::step` gives every hostile at the
        // line.
        assert_eq!(rules::range_penalty(1.0), 0);
        for step in 0..=1_000 {
            let range = step as f32 * 0.25;
            let expected = (-rules::range_penalty(range)).min(MAX_PENALTY as i32) as usize;
            let index = RANGE_STEPS
                .iter()
                .position(|limit| range <= *limit)
                .unwrap_or(MAX_PENALTY);
            assert_eq!(index, expected, "{range}");
        }
    }

    #[test]
    fn injury_means_match_the_dice() {
        // 3d+1 piercing through DR 0 is the plain mean, through DR 4 the
        // mean less 4 (no roll falls under it); 1d through DR 4 counts
        // only the 5 and the 6.
        let injury = Injury::new(Damage {
            dice: 3,
            adds: 1,
            kind: DamageKind::Piercing,
        });
        assert!((injury.mean(1, 0) - 11.5).abs() < 1e-4);
        assert!((injury.basic(1) - 11.5).abs() < 1e-4);
        assert!((injury.mean(1, 4) - 7.5).abs() < 1e-4);
        let one = Injury::new(Damage {
            dice: 1,
            adds: 0,
            kind: DamageKind::Crushing,
        });
        assert!((one.mean(0, 4) - 3.0 / 6.0).abs() < 1e-5);
        // 1d-1 crushing can roll 0: its mean counts the floor.
        let weak = Injury::new(Damage {
            dice: 1,
            adds: -1,
            kind: DamageKind::Crushing,
        });
        assert!((weak.basic(-1) - mean_basic_damage(weak.damage)).abs() < 1e-5);
    }
}
