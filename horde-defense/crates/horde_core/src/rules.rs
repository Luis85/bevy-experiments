//! Pure, deterministic procedures for the supported Fourth Edition Lite subset.
//! Combat scheduling, aim commitment and equipment legality belong to `sim`.
use serde::{Deserialize, Serialize};

pub const RULESET_VERSION: &str = "lastlight-lite-2004-v1";
pub const RULESET_CORRECTION: u32 = 2;

/// SplitMix64 with explicit serializable state; never use wall-clock randomness.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rng {
    pub state: u64,
}
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, limit: u64) -> u64 {
        assert!(limit > 0);
        let threshold = limit.wrapping_neg() % limit;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return value % limit;
            }
        }
    }
    pub fn d6(&mut self) -> i32 {
        self.below(6) as i32 + 1
    }
    pub fn roll3d6(&mut self) -> i32 {
        self.d6() + self.d6() + self.d6()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stats {
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
    pub ht: i32,
}
impl Default for Stats {
    fn default() -> Self {
        Self {
            st: 10,
            dx: 10,
            iq: 10,
            ht: 10,
        }
    }
}
impl Stats {
    pub fn hp(self) -> i32 {
        self.st
    }
    pub fn fp(self) -> i32 {
        self.ht
    }
    pub fn will(self) -> i32 {
        self.iq
    }
    pub fn perception(self) -> i32 {
        self.iq
    }
    /// Basic Speed multiplied by four, preserving its quarter-point increments.
    pub fn speed_quarters(self) -> i32 {
        self.dx + self.ht
    }
    pub fn dodge(self) -> i32 {
        self.speed_quarters() / 4 + 3
    }
    pub fn point_cost(self) -> i32 {
        (self.st - 10) * 10 + (self.ht - 10) * 10 + (self.dx - 10) * 20 + (self.iq - 10) * 20
    }
    pub fn valid(self) -> bool {
        [self.st, self.dx, self.iq, self.ht]
            .iter()
            .all(|v| (1..=30).contains(v))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Average,
    Hard,
}
/// Total points invested to buy a skill at a level relative to its attribute.
/// A zero-point default is authored separately; it is not a purchased skill.
pub fn skill_point_cost(relative_level: i32, difficulty: Difficulty) -> Option<u32> {
    let start = match difficulty {
        Difficulty::Easy => 0,
        Difficulty::Average => -1,
        Difficulty::Hard => -2,
    };
    match relative_level - start {
        n if n < 0 => None,
        0 => Some(1),
        1 => Some(2),
        n => Some((n as u32 - 1) * 4),
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RollOutcome {
    CriticalSuccess,
    Success,
    Failure,
    CriticalFailure,
}
impl RollOutcome {
    pub fn succeeded(self) -> bool {
        matches!(self, Self::Success | Self::CriticalSuccess)
    }
    pub fn critical(self) -> bool {
        self == Self::CriticalSuccess
    }
}
/// Attack/attribute procedure. Non-defense skills below 3 cannot be attempted.
pub fn check_roll(skill: i32, roll: i32) -> RollOutcome {
    if skill < 3 {
        return RollOutcome::Failure;
    }
    if roll <= 4 || roll == 5 && skill >= 15 || roll == 6 && skill >= 16 {
        return RollOutcome::CriticalSuccess;
    }
    if roll == 18 || roll == 17 && skill <= 15 || roll >= skill.saturating_add(10) {
        return RollOutcome::CriticalFailure;
    }
    if roll <= skill && roll < 17 {
        RollOutcome::Success
    } else {
        RollOutcome::Failure
    }
}
pub fn defense_succeeds(defense: i32, roll: i32) -> bool {
    roll <= 4 || roll < 17 && roll <= defense
}
/// Attack-roll success only: excludes defenses, protection and injury.
pub fn attack_success_probability(skill: i32) -> f32 {
    let mut count = 0;
    for a in 1..=6 {
        for b in 1..=6 {
            for c in 1..=6 {
                if check_roll(skill, a + b + c).succeeded() {
                    count += 1;
                }
            }
        }
    }
    count as f32 / 216.0
}
/// Threshold enumeration shown in the GDD, intentionally distinct from automatic
/// success/failure rules at exceptional skill levels.
pub fn threshold_probability(skill: i32) -> f32 {
    let mut count = 0;
    for a in 1..=6 {
        for b in 1..=6 {
            for c in 1..=6 {
                if a + b + c <= skill {
                    count += 1;
                }
            }
        }
    }
    count as f32 / 216.0
}
pub fn range_penalty(yards: f32) -> i32 {
    if !yards.is_finite() {
        return -30;
    }
    let distance = yards.max(0.0);
    // Six steps per decade, rounded to the familiar preferred-number series.
    for step in 0..=30 {
        let base = match step % 6 {
            0 => 2.0,
            1 => 3.0,
            2 => 5.0,
            3 => 7.0,
            4 => 10.0,
            _ => 15.0,
        };
        if distance <= base * 10.0_f32.powi(step / 6) {
            return -step;
        }
    }
    -30
}
pub fn aim_bonus(accuracy: i32, seconds: u8) -> i32 {
    match seconds {
        0 => 0,
        1 => accuracy,
        2 => accuracy + 1,
        _ => accuracy + 2,
    }
}
/// The supported human ST interval in the pinned Lite reference. Arithmetic
/// expresses the progression; no rulebook table or text is shipped.
pub fn swing_damage(st: i32) -> Option<Damage> {
    if !(1..=20).contains(&st) {
        return None;
    }
    let mut adds = if st <= 8 { (st + 1) / 2 - 6 } else { st - 10 };
    let mut dice = 1;
    while adds > 2 {
        dice += 1;
        adds -= 4;
    }
    Some(Damage {
        dice,
        adds,
        kind: DamageKind::Crushing,
    })
}
/// The weapon catalogue authors its own modifiers at ST 10. This applies the
/// referenced swing progression while preserving those original modifiers.
pub fn strength_adjusted_swing(base: Damage, st: i32) -> Option<Damage> {
    rescale_swing(base, 10, st)
}
pub fn rescale_swing(damage: Damage, from_st: i32, to_st: i32) -> Option<Damage> {
    let from = swing_damage(from_st)?;
    let to = swing_damage(to_st)?;
    let dice = i32::from(damage.dice) + i32::from(to.dice) - i32::from(from.dice);
    let result = Damage {
        dice: u8::try_from(dice).ok()?,
        adds: damage.adds.checked_add(to.adds)?.checked_sub(from.adds)?,
        kind: damage.kind,
    };
    result.valid().then_some(result)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DamageKind {
    Crushing,
    Cutting,
    Piercing,
    Impaling,
    SmallPiercing,
    LargePiercing,
}
impl DamageKind {
    pub const ALL: [Self; 6] = [
        Self::Crushing,
        Self::Cutting,
        Self::Piercing,
        Self::Impaling,
        Self::SmallPiercing,
        Self::LargePiercing,
    ];
    /// Player-facing damage-kind name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Crushing => "Crushing",
            Self::Cutting => "Cutting",
            Self::Piercing => "Piercing",
            Self::Impaling => "Impaling",
            Self::SmallPiercing => "Small piercing",
            Self::LargePiercing => "Large piercing",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Damage {
    pub dice: u8,
    pub adds: i32,
    pub kind: DamageKind,
}
impl Damage {
    pub fn valid(self) -> bool {
        self.dice > 0 && self.dice <= 20 && (-30..=1_000_000).contains(&self.adds)
    }
    pub fn minimum(self) -> i32 {
        if self.kind == DamageKind::Crushing {
            0
        } else {
            1
        }
    }
    pub fn maximum(self) -> i32 {
        (i32::from(self.dice) * 6 + self.adds).max(self.minimum())
    }
    pub fn roll(self, rng: &mut Rng) -> i32 {
        ((0..self.dice).map(|_| rng.d6()).sum::<i32>() + self.adds).max(self.minimum())
    }
    pub fn injury(self, penetration: i32) -> i32 {
        if penetration <= 0 {
            return 0;
        }
        match self.kind {
            DamageKind::Cutting | DamageKind::LargePiercing => (penetration * 3 / 2).max(1),
            DamageKind::Impaling => penetration * 2,
            DamageKind::SmallPiercing => (penetration / 2).max(1),
            _ => penetration,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attack {
    pub skill: i32,
    pub damage: Damage,
    pub armor_divisor: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttackResult {
    pub effective_skill: i32,
    pub attack_roll: i32,
    pub outcome: RollOutcome,
    pub defense_roll: Option<i32>,
    pub defense: Option<i32>,
    pub defended: bool,
    pub hit: bool,
    pub basic_damage: i32,
    pub effective_dr: i32,
    pub penetration: i32,
    pub injury: i32,
    /// Original scenario consequence for attack critical failure: next action
    /// must Ready the weapon. The simulation applies it to the attacker.
    pub requires_ready: bool,
    pub conditions: Vec<String>,
}
impl AttackResult {
    pub fn explanation(&self) -> String {
        let outcome = match self.outcome {
            RollOutcome::Success => "success",
            RollOutcome::CriticalSuccess => "critical success",
            RollOutcome::Failure => "miss",
            RollOutcome::CriticalFailure => "critical failure",
        };
        let defense = match (self.defense_roll, self.defense) {
            (Some(roll), Some(target)) => format!(
                "roll {roll} vs {target} ({})",
                if self.defended { "avoided" } else { "failed" }
            ),
            (None, _) if self.outcome == RollOutcome::CriticalSuccess => {
                "bypassed by critical hit".into()
            }
            (None, Some(target)) => format!("not rolled (target {target})"),
            (None, None) => "unavailable".into(),
            (Some(roll), None) => format!("roll {roll} (target unavailable)"),
        };
        format!(
            "3d6 {} / skill {}: {}; defense {}; damage {} - DR {} = {}; injury {}{}",
            self.attack_roll,
            self.effective_skill,
            outcome,
            defense,
            self.basic_damage,
            self.effective_dr,
            self.penetration,
            self.injury,
            if self.conditions.is_empty() {
                String::new()
            } else {
                format!("; {}", self.conditions.join(", "))
            }
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InjuryState {
    pub max_hp: i32,
    pub hp: i32,
    pub max_fp: i32,
    pub fp: i32,
    pub shock: i32,
    pub stunned: bool,
    pub prone: bool,
    pub unconscious: bool,
    pub dead: bool,
    pub mortally_wounded: bool,
    pub weapon_dropped: bool,
    pub death_checks: i32,
}
impl InjuryState {
    /// Structural save validation only; legitimate below-zero HP is supported.
    pub fn valid(&self) -> bool {
        self.max_hp > 0
            && self.max_hp.checked_mul(5).is_some()
            && self.max_fp > 0
            && self.max_fp.checked_neg().is_some()
            && self.hp <= self.max_hp
            && self.hp.checked_mul(3).is_some()
            && self.hp.checked_neg().is_some()
            && self.fp <= self.max_fp
            && self.fp >= -self.max_fp
            && (0..=4).contains(&self.shock)
            && (0..=5).contains(&self.death_checks)
            && (!self.dead || self.hp <= -self.max_hp)
            && (!self.mortally_wounded || self.hp <= -self.max_hp)
    }
    pub fn new(max_hp: i32, max_fp: i32) -> Self {
        assert!(max_hp > 0 && max_fp > 0);
        Self {
            max_hp,
            hp: max_hp,
            max_fp,
            fp: max_fp,
            shock: 0,
            stunned: false,
            prone: false,
            unconscious: false,
            dead: false,
            mortally_wounded: false,
            weapon_dropped: false,
            death_checks: 0,
        }
    }
    pub fn terminal(&self) -> bool {
        self.unconscious || self.dead || self.mortally_wounded
    }
    pub fn clear_shock(&mut self) {
        self.shock = 0;
    }
    pub fn dodge(&self, base: i32) -> i32 {
        let mut score = base;
        if self.hp * 3 < self.max_hp {
            score = (score + 1) / 2;
        }
        if self.fp * 3 < self.max_fp {
            score = (score + 1) / 2;
        }
        score - if self.stunned { 4 } else { 0 } - if self.prone { 3 } else { 0 }
    }
    /// Call at the beginning of each legal action opportunity, before acting.
    pub fn start_turn(&mut self, rng: &mut Rng, ht: i32) -> bool {
        if self.terminal() {
            return false;
        }
        if self.hp <= 0 && !check_roll(ht - (-self.hp / self.max_hp), rng.roll3d6()).succeeded() {
            self.unconscious = true;
        }
        !self.terminal()
    }
    /// A stunned actor spends this entire turn Doing Nothing, even if recovery
    /// succeeds. Standing up and retrieving a dropped weapon are later actions.
    pub fn recover_stun(&mut self, rng: &mut Rng, ht: i32) -> bool {
        if self.stunned && check_roll(ht, rng.roll3d6()).succeeded() {
            self.stunned = false;
        }
        !self.stunned
    }
    pub fn fatigue_allows_action(&mut self, rng: &mut Rng, will: i32) -> bool {
        if self.fp > 0 {
            return true;
        }
        if self.fp <= -self.max_fp || !check_roll(will, rng.roll3d6()).succeeded() {
            self.unconscious = true;
            return false;
        }
        true
    }
    pub fn lose_fatigue(&mut self, amount: i32, rng: &mut Rng, ht: i32) {
        for _ in 0..amount.max(0) {
            if self.fp <= 0 {
                self.apply_injury(1, rng, ht);
            }
            self.fp = (self.fp - 1).max(-self.max_fp);
        }
        if self.fp <= -self.max_fp {
            self.unconscious = true;
        }
    }
    /// Applies penetrating, wound-multiplied injury, not raw weapon damage.
    /// Returns descriptive consequences for an explanatory combat log.
    pub fn apply_injury(&mut self, injury: i32, rng: &mut Rng, ht: i32) -> Vec<String> {
        let mut conditions = Vec::new();
        if injury <= 0 || self.dead {
            return conditions;
        }
        self.hp = self.hp.saturating_sub(injury);
        self.shock = (self.shock + injury).min(4);
        if self.hp <= -5 * self.max_hp {
            self.dead = true;
            conditions.push("dead: catastrophic injury".into());
            return conditions;
        }
        let crossed = (-self.hp / self.max_hp).max(0);
        while self.death_checks < crossed && !self.dead {
            self.death_checks += 1;
            let roll = rng.roll3d6();
            if !check_roll(ht, roll).succeeded() {
                if self.mortally_wounded || roll - ht > 2 {
                    self.dead = true;
                    conditions.push(format!("death check {roll}/{ht}: dead"));
                } else {
                    self.mortally_wounded = true;
                    conditions.push(format!(
                        "death check {roll}/{ht}: mortal wound, incapacitated"
                    ));
                }
            } else {
                conditions.push(format!("death check {roll}/{ht}: survived"));
            }
        }
        if injury * 2 > self.max_hp && !self.dead {
            let roll = rng.roll3d6();
            let outcome = check_roll(ht, roll);
            if !outcome.succeeded() {
                self.stunned = true;
                self.prone = true;
                self.weapon_dropped = true;
                conditions.push(format!(
                    "major wound {roll}/{ht}: stunned, knocked down, weapon dropped"
                ));
                if roll - ht >= 5 || outcome == RollOutcome::CriticalFailure {
                    self.unconscious = true;
                    conditions.push("unconscious from major wound".into());
                }
            } else {
                conditions.push(format!("major wound {roll}/{ht}: resisted knockdown"));
            }
        }
        conditions
    }
}

pub fn resolve_attack(
    rng: &mut Rng,
    attack: Attack,
    defense: Option<i32>,
    dr: i32,
    target: &mut InjuryState,
    ht: i32,
) -> AttackResult {
    let mut result = resolve_attack_roll(rng, attack, defense, dr, !target.terminal());
    result.conditions = target.apply_injury(result.injury, rng, ht);
    result
}
/// Scenario objects have protection and durability, but no biological shock,
/// consciousness, knockdown or mortality checks. Uses the same attack procedure.
pub fn resolve_object_attack(rng: &mut Rng, attack: Attack, dr: i32) -> AttackResult {
    resolve_attack_roll(rng, attack, None, dr, true)
}
fn resolve_attack_roll(
    rng: &mut Rng,
    attack: Attack,
    defense: Option<i32>,
    dr: i32,
    target_eligible: bool,
) -> AttackResult {
    let legal =
        attack.skill >= 3 && attack.damage.valid() && attack.armor_divisor >= 1 && target_eligible;
    // Invalid attacks consume no random draws and have no effects.
    let roll = if legal { rng.roll3d6() } else { 0 };
    let outcome = if legal {
        check_roll(attack.skill, roll)
    } else {
        RollOutcome::Failure
    };
    let defense = if target_eligible { defense } else { None };
    let defense_roll = if outcome == RollOutcome::Success {
        defense.map(|_| rng.roll3d6())
    } else {
        None
    };
    let defended = defense
        .zip(defense_roll)
        .is_some_and(|(score, die)| defense_succeeds(score, die));
    let hit = outcome.succeeded() && !defended;
    let basic_damage = if !hit {
        0
    } else if roll == 3 {
        attack.damage.maximum()
    } else {
        attack.damage.roll(rng)
    };
    let effective_dr = dr.max(0) / attack.armor_divisor.max(1);
    let penetration = if hit {
        (basic_damage - effective_dr).max(0)
    } else {
        0
    };
    let injury = attack.damage.injury(penetration);
    AttackResult {
        effective_skill: attack.skill,
        attack_roll: roll,
        outcome,
        defense_roll,
        defense,
        defended,
        hit,
        basic_damage,
        effective_dr,
        penetration,
        injury,
        requires_ready: outcome == RollOutcome::CriticalFailure,
        conditions: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerated_probability_labels_match_gdd_thresholds() {
        for (skill, count) in [(10, 108), (12, 160), (14, 196), (16, 212)] {
            assert!((threshold_probability(skill) * 216.0 - count as f32).abs() < 0.001);
            assert!((attack_success_probability(skill) * 216.0 - count as f32).abs() < 0.001);
        }
        assert_eq!(attack_success_probability(2), 0.0);
        // No skill can turn 17/18 into successful attacks.
        assert!((attack_success_probability(30) - 212.0 / 216.0).abs() < 0.0001);
    }
    #[test]
    fn critical_boundaries_and_automatic_defenses() {
        assert_eq!(check_roll(14, 5), RollOutcome::Success);
        assert_eq!(check_roll(15, 5), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(15, 6), RollOutcome::Success);
        assert_eq!(check_roll(16, 6), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(15, 17), RollOutcome::CriticalFailure);
        assert_eq!(check_roll(16, 17), RollOutcome::Failure);
        assert_eq!(check_roll(30, 18), RollOutcome::CriticalFailure);
        assert_eq!(check_roll(6, 16), RollOutcome::CriticalFailure);
        assert!(defense_succeeds(1, 4));
        assert!(!defense_succeeds(30, 17));
    }
    #[test]
    fn range_rounds_up_and_contact_is_one_yard() {
        for (range, penalty) in [
            (1.0, 0),
            (2.0, 0),
            (2.01, -1),
            (3.0, -1),
            (5.0, -2),
            (7.0, -3),
            (8.0, -4),
            (15.0, -5),
            (20.0, -6),
            (30.0, -7),
            (40.0, -8),
        ] {
            assert_eq!(range_penalty(range), penalty);
        }
        assert_eq!(aim_bonus(4, 0), 0);
        assert_eq!(aim_bonus(4, 1), 4);
        assert_eq!(aim_bonus(4, 2), 5);
        assert_eq!(aim_bonus(4, 20), 6);
    }
    #[test]
    fn attributes_and_skill_costs_use_separate_creation_accounting() {
        let stats = Stats {
            st: 12,
            dx: 11,
            iq: 10,
            ht: 12,
        };
        assert_eq!(stats.hp(), 12);
        assert_eq!(stats.fp(), 12);
        assert_eq!(stats.speed_quarters(), 23);
        assert_eq!(stats.dodge(), 8);
        assert_eq!(stats.point_cost(), 60);
        assert_eq!(skill_point_cost(0, Difficulty::Easy), Some(1));
        assert_eq!(skill_point_cost(1, Difficulty::Easy), Some(2));
        assert_eq!(skill_point_cost(2, Difficulty::Easy), Some(4));
        assert_eq!(skill_point_cost(3, Difficulty::Easy), Some(8));
        assert_eq!(skill_point_cost(0, Difficulty::Average), Some(2));
        assert_eq!(skill_point_cost(0, Difficulty::Hard), Some(4));
    }
    fn seed_with_roll(wanted: i32) -> u64 {
        (0..100_000)
            .find(|seed| Rng::new(*seed).roll3d6() == wanted)
            .expect("all 3d6 sums occur")
    }
    #[test]
    fn critical_three_bypasses_defense_and_maximizes_damage() {
        let mut rng = Rng::new(seed_with_roll(3));
        let damage = Damage {
            dice: 2,
            adds: 1,
            kind: DamageKind::Piercing,
        };
        let mut target = InjuryState::new(100, 10);
        let result = resolve_attack(
            &mut rng,
            Attack {
                skill: 12,
                damage,
                armor_divisor: 1,
            },
            Some(30),
            3,
            &mut target,
            10,
        );
        assert!(result.hit);
        assert_eq!(result.defense_roll, None);
        assert_eq!(result.basic_damage, 13);
        assert_eq!(result.penetration, 10);
        assert_eq!(result.injury, 10);
        assert_eq!(target.hp, 90);
    }
    #[test]
    fn protection_precedes_wounding_and_floors_injury() {
        let cut = Damage {
            dice: 1,
            adds: 0,
            kind: DamageKind::Cutting,
        };
        assert_eq!(cut.injury(3), 4);
        assert_eq!(cut.injury(0), 0);
        let small = Damage {
            kind: DamageKind::SmallPiercing,
            ..cut
        };
        assert_eq!(small.injury(1), 1);
        assert_eq!(small.injury(3), 1);
        let mut target = InjuryState::new(10, 10);
        let result = resolve_attack(
            &mut Rng::new(seed_with_roll(3)),
            Attack {
                skill: 12,
                damage: cut,
                armor_divisor: 1,
            },
            None,
            10,
            &mut target,
            10,
        );
        assert!(result.hit);
        assert_eq!(result.injury, 0);
        assert_eq!(target.hp, 10);
        assert_eq!(target.shock, 0);
    }
    #[test]
    fn zero_hp_is_not_automatically_death_or_unconsciousness() {
        let mut actor = InjuryState::new(10, 10);
        actor.hp = 1;
        actor.apply_injury(1, &mut Rng::new(1), 10);
        assert_eq!(actor.hp, 0);
        assert!(!actor.dead && !actor.unconscious);
        assert!(actor.start_turn(&mut Rng::new(seed_with_roll(10)), 10));
        assert!(!actor.start_turn(&mut Rng::new(seed_with_roll(11)), 10));
        assert!(actor.unconscious);
        assert!(!actor.dead);
    }
    #[test]
    fn major_wound_can_stun_without_ending_encounter() {
        let mut actor = InjuryState::new(20, 10);
        actor.apply_injury(11, &mut Rng::new(seed_with_roll(11)), 10);
        assert!(actor.stunned && actor.prone && actor.weapon_dropped);
        assert!(!actor.terminal());
        assert_eq!(actor.shock, 4);
        actor.recover_stun(&mut Rng::new(seed_with_roll(10)), 10);
        assert!(!actor.stunned);
        assert!(actor.prone && actor.weapon_dropped);
    }
    #[test]
    fn failed_death_check_distinguishes_mortal_wound_and_death() {
        let mut actor = InjuryState::new(10, 10);
        actor.hp = -9;
        actor.apply_injury(1, &mut Rng::new(seed_with_roll(11)), 10);
        assert!(actor.mortally_wounded);
        assert!(!actor.dead);
        assert!(actor.terminal());
        assert_eq!(actor.death_checks, 1);
        // Ordinary damage within this band does not create another threshold check.
        actor.apply_injury(1, &mut Rng::new(seed_with_roll(11)), 10);
        assert_eq!(actor.death_checks, 1);
        actor.hp = -19;
        actor.apply_injury(1, &mut Rng::new(seed_with_roll(11)), 10);
        assert!(actor.dead);
    }
    #[test]
    fn catastrophic_injury_cannot_be_survived_by_high_ht() {
        let mut actor = InjuryState::new(10, 10);
        actor.apply_injury(60, &mut Rng::new(1), 30);
        assert!(actor.dead);
        assert_eq!(actor.hp, -50);
    }
    #[test]
    fn invalid_attack_has_no_random_or_world_effect() {
        let mut rng = Rng::new(33);
        let before = rng;
        let mut target = InjuryState::new(10, 10);
        let result = resolve_attack(
            &mut rng,
            Attack {
                skill: 2,
                damage: Damage {
                    dice: 1,
                    adds: 0,
                    kind: DamageKind::Crushing,
                },
                armor_divisor: 1,
            },
            None,
            0,
            &mut target,
            10,
        );
        assert!(!result.hit);
        assert_eq!(rng, before);
        assert_eq!(target.hp, 10);
    }
    #[test]
    fn saved_random_state_continues_without_rerolling() {
        let mut rng = Rng::new(55);
        for _ in 0..123 {
            rng.roll3d6();
        }
        let encoded = serde_json::to_string(&rng).unwrap();
        let mut resumed: Rng = serde_json::from_str(&encoded).unwrap();
        for _ in 0..100 {
            assert_eq!(rng.next_u64(), resumed.next_u64());
        }
    }
    #[test]
    fn injury_and_fatigue_reduce_dodge_without_shock_penalty() {
        let mut actor = InjuryState::new(10, 10);
        actor.shock = 4;
        assert_eq!(actor.dodge(9), 9);
        actor.hp = 3;
        assert_eq!(actor.dodge(9), 5);
        actor.fp = 3;
        assert_eq!(actor.dodge(9), 3);
        actor.stunned = true;
        actor.prone = true;
        assert_eq!(actor.dodge(9), -4);
    }
    #[test]
    fn object_resolution_has_no_biological_injury_checks_or_hidden_random_draws() {
        let seed = seed_with_roll(3);
        let mut rng = Rng::new(seed);
        let mut expected = Rng::new(seed);
        expected.roll3d6();
        let result = resolve_object_attack(
            &mut rng,
            Attack {
                skill: 12,
                damage: Damage {
                    dice: 2,
                    adds: 1,
                    kind: DamageKind::Crushing,
                },
                armor_divisor: 1,
            },
            3,
        );
        assert_eq!(result.basic_damage, 13);
        assert_eq!(result.injury, 10);
        assert!(result.conditions.is_empty());
        assert_eq!(result.defense_roll, None);
        assert_eq!(
            rng, expected,
            "A wall cannot roll HT, fall unconscious or consume biological condition rolls"
        );
    }
    #[test]
    fn saved_physiology_accepts_negative_hp_but_rejects_contradictions_and_overflow() {
        let mut actor = InjuryState::new(10, 10);
        actor.hp = -9;
        assert!(actor.valid());
        actor.hp = 5;
        actor.dead = true;
        assert!(!actor.valid());
        actor.dead = false;
        actor.shock = 5;
        assert!(!actor.valid());
        actor.shock = 0;
        actor.fp = -11;
        assert!(!actor.valid());
        actor.fp = 10;
        actor.max_hp = i32::MAX;
        assert!(!actor.valid());
    }
    #[test]
    fn strength_changes_swung_weapon_damage_without_changing_firearm_profiles() {
        let crowbar = Damage {
            dice: 1,
            adds: 2,
            kind: DamageKind::Crushing,
        };
        assert_eq!(strength_adjusted_swing(crowbar, 10), Some(crowbar));
        assert_eq!(
            strength_adjusted_swing(crowbar, 11),
            Some(Damage {
                dice: 1,
                adds: 3,
                ..crowbar
            })
        );
        assert_eq!(
            strength_adjusted_swing(crowbar, 13),
            Some(Damage {
                dice: 2,
                adds: 1,
                ..crowbar
            })
        );
        assert_eq!(
            swing_damage(20),
            Some(Damage {
                dice: 3,
                adds: 2,
                kind: DamageKind::Crushing
            })
        );
        assert!(strength_adjusted_swing(crowbar, 21).is_none());
        assert_eq!(
            rescale_swing(strength_adjusted_swing(crowbar, 13).unwrap(), 13, 7),
            strength_adjusted_swing(crowbar, 7)
        );
    }
    #[test]
    fn attack_explanations_present_defense_values_without_option_syntax() {
        let mut result = AttackResult {
            effective_skill: 12,
            attack_roll: 9,
            outcome: RollOutcome::Success,
            defense_roll: Some(9),
            defense: Some(8),
            defended: false,
            hit: true,
            basic_damage: 7,
            effective_dr: 2,
            penetration: 5,
            injury: 5,
            requires_ready: false,
            conditions: vec![],
        };
        let before = serde_json::to_value(&result).unwrap();
        let text = result.explanation();
        assert!(
            text.contains("defense roll 9 vs 8 (failed)")
                && text.contains("damage 7 - DR 2 = 5; injury 5")
        );
        assert!(!text.contains("Some(") && !text.contains("None"));
        assert_eq!(serde_json::to_value(&result).unwrap(), before);
        result.defense_roll = None;
        result.defense = None;
        assert!(result.explanation().contains("defense unavailable"));
        result.outcome = RollOutcome::CriticalSuccess;
        assert!(result.explanation().contains("bypassed by critical hit"));
        result.outcome = RollOutcome::Failure;
        result.defense = Some(8);
        assert!(result.explanation().contains("not rolled (target 8)"));
    }
}
