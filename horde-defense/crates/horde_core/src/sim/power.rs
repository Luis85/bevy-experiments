//! Defender power (CD11, design-campaign-defenders 1.11): one comparable
//! number for any defender in either combat model, read from the very
//! inputs a deploy starts from, so it cannot drift from what deploys. Pure:
//! no RNG and no mutation.
use super::*;
use crate::balance::BalanceSettings;
use crate::campaign::{Npc, mean_basic_damage};
use crate::command::CommandError;
use crate::defenders::{DefenderPower, PowerBand, recruit_identity};
use crate::progression::ProgressionSnapshot;
use crate::rules::{attack_success_probability, threshold_probability};
use std::sync::OnceLock;

/// The longest expected survival a power reading counts, in seconds: a
/// defender the reference enemy cannot hurt reads as this, not infinity.
const MAX_SURVIVAL_S: f32 = 3_600.0;

/// What a deploy of `defender` starts from at victory threat `threat`, before
/// the country's modifiers: the defender's profile with the balance preset's
/// defender scaling (a recruit's carries its own appearance), its maximum
/// health, the progression snapshot and the balance settings. A deploy at the
/// next threat ([`Game::next_threat`]) copies these into the attempt; on a
/// country whose modifiers leave the defender alone the attempt's profile is
/// exactly `profile`.
#[derive(Clone, Debug, PartialEq)]
pub struct DeploymentInputs {
    pub defender: DefenderId,
    pub threat: u32,
    pub profile: DefenderProfile,
    pub snapshot: ProgressionSnapshot,
    pub max_hp: i32,
    pub balance: BalanceSettings,
}

/// The inputs for `defender` (`character` wearing `loadout`) in `campaign`.
/// Shared by [`Game::deploy_as`] and [`Game::deployment_inputs`].
pub(super) fn deployment_inputs_for(
    campaign: &Campaign,
    defender: DefenderId,
    loadout: [u64; 4],
    character: &Npc,
    threat: u32,
) -> Result<DeploymentInputs, String> {
    let mode = defender.mode();
    let snapshot = campaign.progression_for(mode, threat, campaign.lane_count())?;
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
    Ok(DeploymentInputs {
        defender,
        threat,
        profile,
        snapshot,
        max_hp,
        balance,
    })
}

impl DeploymentInputs {
    /// The power these inputs give their defender.
    pub fn power(&self) -> DefenderPower {
        let (kill_rate, survival_s) = self.model();
        let (kill_ref, survival_ref) = reference(self.defender.mode());
        let ratio = (kill_rate / kill_ref) * (survival_s / survival_ref);
        let rating = (100.0 * ratio.sqrt()).round();
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
    /// Expected kills per second against the reference enemy (a Shambler)
    /// and expected seconds survived against its arrivals, in the
    /// defender's own combat model.
    fn model(&self) -> (f32, f32) {
        // `resolve_kind(Shambler)`, read in place rather than cloned.
        let enemy = crate::enemy_catalog::enemy_catalog().default_profile(EnemyKind::Shambler);
        let mode = self.defender.mode();
        let enemy_health = self.snapshot.enemy_health * self.balance.enemy_health;
        let enemy_damage = self.snapshot.enemy_damage * self.balance.enemy_damage;
        let arrival = self.snapshot.arrival_multiplier
            * self.balance.spawn_rate
            * if mode == Mode::Npc {
                self.balance.npc_spawn_rate
            } else {
                1.0
            };
        let survival = |incoming: f32| {
            if incoming > 0.0 {
                (self.max_hp as f32 / incoming).min(MAX_SURVIVAL_S)
            } else {
                MAX_SURVIVAL_S
            }
        };
        match mode {
            Mode::Personal => {
                let p = &self.profile.personal;
                let magazine = p.magazine.max(1) as f32;
                let dps =
                    (p.damage - enemy.armor).max(1) as f32 * (1.0 + 0.5 * p.weak_bonus) * magazine
                        / (magazine * p.cadence + p.reload).max(f32::MIN_POSITIVE);
                let kill_rate = dps / (enemy.health.max(1) as f32 * enemy_health);
                let hit = ((enemy.damage as f32 * enemy_damage).round() as i32 - p.armor).max(1);
                let incoming = hit as f32 / enemy.windup.max(0.5) * arrival;
                (kill_rate, survival(incoming))
            }
            Mode::Npc => {
                let n = &self.profile.npc;
                let hit_chance =
                    attack_success_probability(self.profile.ranged_skill + n.skill_bonus)
                        * (1.0 - threshold_probability(enemy.npc_dodge));
                let per_hit = (mean_basic_damage(Damage {
                    dice: n.damage_dice,
                    adds: n.damage_add,
                    kind: n.damage_kind,
                }) - enemy.npc_armor as f32)
                    .max(0.0);
                let magazine = n.magazine.max(1) as f32;
                let dps = hit_chance * per_hit * magazine / (magazine + n.reload_turns as f32);
                let kill_rate = dps / (enemy.npc_hp.max(1) as f32 * enemy_health);
                let enemy_hit = mean_basic_damage(Damage {
                    dice: enemy.npc_damage_dice,
                    adds: enemy.npc_damage_add,
                    kind: DamageKind::Crushing,
                });
                let incoming = attack_success_probability(enemy.npc_skill)
                    * (1.0 - threshold_probability(self.profile.stats.dodge()))
                    * (enemy_hit * enemy_damage - n.armor as f32).max(0.0)
                    * arrival;
                (kill_rate, survival(incoming))
            }
        }
    }
}

/// The kill rate and survival of a fresh starter at threat 0 in `mode`: the
/// default campaign's player (Personal) or Mara (Npc), so either rates 100.
fn reference(mode: Mode) -> (f32, f32) {
    static REFERENCE: OnceLock<[(f32, f32); 2]> = OnceLock::new();
    let reference = REFERENCE.get_or_init(|| {
        let campaign = Campaign::default();
        [
            (DefenderId::Player, campaign.player.loadout),
            (DefenderId::Mara, campaign.npc.loadout),
        ]
        .map(|(defender, loadout)| {
            let (kill_rate, survival_s) =
                deployment_inputs_for(&campaign, defender, loadout, &campaign.npc, 0)
                    .expect("threat 0 is always supported")
                    .model();
            (
                kill_rate.max(f32::MIN_POSITIVE),
                survival_s.max(f32::MIN_POSITIVE),
            )
        })
    });
    reference[usize::from(mode == Mode::Npc)]
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
        deployment_inputs_for(&self.campaign, defender, loadout, character, threat)
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
