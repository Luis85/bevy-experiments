//! Choosing the next defender (ruling D9) and the defender cards the
//! frontend names and draws. `Campaign.next_defender` is the only saved
//! state; everything else here is a pure read.
use super::*;
use crate::command::CommandError;
use crate::defenders::{
    DefenderBlocker, DefenderCard, DefenderOption, DefenderRole, GuardDuty, OriginInfo,
    OriginTrait, ServiceRecord,
};
use crate::skill_catalog::{CampaignFeature, SkillEffect, skills};

/// The upgrade node that unlocks `feature` (the first in catalogue order).
pub(super) fn feature_node(feature: CampaignFeature) -> String {
    skills()
        .nodes
        .iter()
        .find(|node| {
            node.effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::FeatureUnlock(f) if *f == feature))
        })
        .map_or_else(String::new, |node| node.id.clone())
}

impl Game {
    /// Who defends next: the chosen defender, or else the defender of the
    /// last report (today's continue behaviour), or else the player.
    pub fn next_defender(&self) -> DefenderId {
        self.campaign.next_defender.unwrap_or_else(|| {
            self.last_result
                .as_ref()
                .map_or(DefenderId::Player, Report::defender)
        })
    }
    /// The typed reason `defender` cannot be chosen or sent into the next
    /// defense, ignoring any defense in progress: Mara needs Companion, a
    /// recruit needs Recruitment, must be in camp and must carry a loadout
    /// it can deploy with (in that order). No mutation.
    pub(crate) fn defender_selectable(&self, defender: DefenderId) -> Result<(), CommandError> {
        let blocker = match defender {
            DefenderId::Player => None,
            DefenderId::Mara => (!self.campaign.npc_available()).then(|| DefenderBlocker::Locked {
                node_id: feature_node(CampaignFeature::Companion),
            }),
            DefenderId::Recruit(id) => {
                let recruit = self
                    .strategy
                    .roster
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(CommandError::UnknownSurvivor)?;
                if !self.strategy.recruitment_unlocked(&self.campaign) {
                    Some(DefenderBlocker::Locked {
                        node_id: feature_node(CampaignFeature::Recruitment),
                    })
                } else if let Some(blocker) = self.duty_blocker(id, recruit.status) {
                    Some(blocker)
                } else {
                    self.recruit_loadout_check(&recruit.npc)
                        .err()
                        .map(|reason| DefenderBlocker::InvalidLoadout { reason })
                }
            }
        };
        match blocker {
            None => Ok(()),
            Some(blocker) => Err(CommandError::DefenderUnavailable { defender, blocker }),
        }
    }
    /// `Command::SetNextDefender`: between defenses only; stores the choice.
    pub(crate) fn set_next_defender(&mut self, defender: DefenderId) -> Result<(), CommandError> {
        if self.worker {
            return Err(CommandError::Rejected(
                "Use the parent expedition controller".into(),
            ));
        }
        if self.attempt.is_some() {
            return Err(CommandError::DefenseInProgress);
        }
        self.defender_selectable(defender)?;
        self.campaign.next_defender = Some(defender);
        Ok(())
    }
    /// `Command::BeginDefense`: deploys [`Self::next_defender`] on the active
    /// country's `stage` and unpauses like `Deploy`; the choice is cleared on
    /// success. With a defense in progress the error is `Deploy`'s.
    pub(crate) fn begin_defense(&mut self, stage: usize) -> Result<(), CommandError> {
        let defender = self.next_defender();
        if self.attempt.is_none() {
            self.defender_selectable(defender)?;
        }
        self.deploy_country_stage_as(stage, defender)?;
        self.campaign.next_defender = None;
        self.set_paused(false);
        Ok(())
    }
    /// A dispatch sent recruit `id`, the next defender, away from camp: the
    /// player defends next, and the strategy log says so.
    pub(crate) fn next_defender_dispatched(&mut self, id: u64) {
        self.campaign.next_defender = Some(DefenderId::Player);
        if let Some(r) = self.strategy.roster.iter().find(|r| r.id == id) {
            let note = format!(
                "{} left the next defense; you will defend next.",
                r.npc.name
            );
            self.strategy.note(note);
        }
    }
    /// Every defender the player can choose from, in order: the player,
    /// Mara (listed even while locked), then the roster by id. Each row
    /// carries `SetNextDefender`'s verdict between defenses (a live defense
    /// rejects every choice with `DefenseInProgress`), marks the next
    /// defender, and gives its power at [`Self::next_threat`] (held at the
    /// highest supported threat, where a deploy would be rejected).
    pub fn available_defenders(&self) -> Vec<DefenderOption> {
        let mut ids = vec![DefenderId::Player, DefenderId::Mara];
        let mut recruits: Vec<_> = self.strategy.roster.iter().map(|r| r.id).collect();
        recruits.sort_unstable();
        ids.extend(recruits.into_iter().map(DefenderId::Recruit));
        let next = self.next_defender();
        let threat = self
            .next_threat()
            .min(crate::progression::catalog().threat_guard_rank);
        ids.into_iter()
            .filter_map(|id| {
                let card = self.defender_card(id)?;
                let selectable = match self.defender_selectable(id) {
                    Ok(()) => Ok(()),
                    Err(CommandError::DefenderUnavailable { blocker, .. }) => Err(blocker),
                    Err(_) => return None,
                };
                Some(DefenderOption {
                    card,
                    selectable,
                    selected: id == next,
                    power: self.defender_power(id, threat).ok()?,
                })
            })
            .collect()
    }
    /// The selectable defender with the highest power at the next threat;
    /// on a tie the earlier in [`Self::available_defenders`] order, so the
    /// player (always selectable) is the fallback.
    pub(crate) fn recommended_defender(&self) -> DefenderId {
        let mut best: Option<(DefenderId, u16)> = None;
        for option in self.available_defenders() {
            if option.selectable.is_ok() && best.is_none_or(|(_, top)| option.power.rating > top) {
                best = Some((option.card.id, option.power.rating));
            }
        }
        best.map_or(DefenderId::Player, |(id, _)| id)
    }
    /// Recruit `recruit_id`'s service history (design 1.10), derived on
    /// every read from the run history, the recruit's own counters, its
    /// live guard assignment and the saved containments. Pure.
    pub fn service_record(&self, recruit_id: u64) -> Result<ServiceRecord, CommandError> {
        let recruit = self
            .strategy
            .roster
            .iter()
            .find(|r| r.id == recruit_id)
            .ok_or(CommandError::UnknownSurvivor)?;
        let mut record = ServiceRecord {
            recruit_id,
            deployments: 0,
            takeovers: 0,
            expeditions: 0,
            victories: 0,
            defeats: 0,
            withdrawals: 0,
            kills: 0,
            coins: 0,
            stages_cleared: 0,
            liberations: 0,
            countries: Vec::new(),
            last_outcome: None,
            npc_attempts: recruit.npc.attempts,
            npc_victories: recruit.npc.victories,
            guard: self
                .strategy
                .guards
                .iter()
                .find(|g| g.recruit_id == recruit_id)
                .map(|g| GuardDuty {
                    country_id: g.country_id.clone(),
                    duty_ticks: g.duty_ticks,
                }),
            containments: recruit.containments,
        };
        let mut expeditions = BTreeSet::new();
        let mut stages = BTreeSet::new();
        let bump = |n: &mut u32| *n = n.saturating_add(1);
        for run in self
            .campaign
            .run_history
            .iter()
            .filter(|r| r.recruit_id == Some(recruit_id))
        {
            bump(&mut record.deployments);
            match run.worker_id {
                None => bump(&mut record.takeovers),
                Some(battle) => {
                    expeditions.insert(battle);
                }
            }
            match run.outcome {
                Outcome::Victory => {
                    bump(&mut record.victories);
                    stages.insert((run.country_id.as_deref(), run.stage));
                    let liberated = run
                        .country_id
                        .as_deref()
                        .and_then(|id| crate::world::world().country(id))
                        .is_some_and(|c| run.stage + 1 == c.level_count());
                    if liberated {
                        bump(&mut record.liberations);
                    }
                }
                Outcome::Defeat => bump(&mut record.defeats),
                Outcome::Withdrawal => bump(&mut record.withdrawals),
            }
            record.kills = record.kills.saturating_add(u64::from(run.kills));
            record.coins = record.coins.saturating_add(run.coins);
            if let Some(country) = &run.country_id
                && !record.countries.contains(country)
            {
                record.countries.push(country.clone());
            }
            record.last_outcome = Some(run.outcome);
        }
        record.expeditions = u32::try_from(expeditions.len()).unwrap_or(u32::MAX);
        record.stages_cleared = u32::try_from(stages.len()).unwrap_or(u32::MAX);
        Ok(record)
    }
    /// The items `defender` carries into a defense: the player's or Mara's
    /// loadout, or a recruit's own; None for a recruit not on the roster.
    pub fn defender_loadout(&self, defender: DefenderId) -> Option<[u64; 4]> {
        match defender {
            DefenderId::Player => Some(self.campaign.player.loadout),
            DefenderId::Mara => Some(self.campaign.npc.loadout),
            DefenderId::Recruit(id) => self
                .strategy
                .roster
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.npc.loadout),
        }
    }
    /// The card for `defender`, or None for a recruit not on the roster.
    pub fn defender_card(&self, defender: DefenderId) -> Option<DefenderCard> {
        let campaign = &self.campaign;
        let npc_card = |npc: &crate::campaign::Npc, appearance, role, origin| DefenderCard {
            id: defender,
            name: npc.name.clone(),
            hud_name: crate::defenders::hud_name(&npc.name),
            appearance,
            role,
            archetype: crate::defenders::Archetype::from_creation(&npc.creation),
            origin,
            mode: Mode::Npc,
            level: u32::from(npc.advancement_level()),
            level_cap: u32::from(campaign.advancement_cap_level()),
        };
        match defender {
            DefenderId::Player => {
                let survivor = campaign
                    .selected_survivor
                    .unwrap_or(crate::survivors::SurvivorId::Rook);
                Some(DefenderCard {
                    id: defender,
                    name: survivor.name().into(),
                    hud_name: crate::defenders::hud_name(survivor.name()),
                    appearance: campaign
                        .selected_survivor
                        .map_or(crate::survivors::Appearance::Male, |s| s.appearance()),
                    role: DefenderRole::Survivor,
                    archetype: None,
                    origin: None,
                    mode: Mode::Personal,
                    level: campaign.player.level,
                    level_cap: campaign.player_level_cap(),
                })
            }
            DefenderId::Mara => Some(npc_card(
                &campaign.npc,
                crate::survivors::Appearance::Female,
                DefenderRole::Companion,
                None,
            )),
            DefenderId::Recruit(id) => {
                let r = self.strategy.roster.iter().find(|r| r.id == id)?;
                let origin = crate::world::world()
                    .country(&r.origin)
                    .map(|country| OriginInfo {
                        country_id: country.id.clone(),
                        country_name: country.name.clone(),
                        origin_trait: OriginTrait::for_biome(&country.biome),
                    });
                Some(npc_card(
                    &r.npc,
                    crate::defenders::recruit_identity(id).1,
                    DefenderRole::Recruit,
                    origin,
                ))
            }
        }
    }
    /// The card for whoever holds the live main-run defense.
    pub fn active_defender(&self) -> Option<DefenderCard> {
        if self.worker {
            return None;
        }
        self.defender_card(self.attempt.as_ref()?.defender())
    }
    /// The card for the recruit fighting expedition `battle_id`, read from
    /// this game's roster (inside the worker the seat would read as Mara's).
    pub fn battle_defender_card(&self, battle_id: u64) -> Option<DefenderCard> {
        let battle = self.battles.iter().find(|b| b.id == battle_id)?;
        self.defender_card(DefenderId::Recruit(battle.recruit_id))
    }
}
