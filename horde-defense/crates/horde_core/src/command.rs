//! Typed gameplay commands. `Game::apply` is the single mutation path for
//! frontends: every domain rule a player intent must satisfy lives here or in
//! the campaign/strategy/sim methods it calls, never in a frontend.
use crate::balance::BalanceSettings;
use crate::campaign::NpcTraining;
use crate::content::{Mode, Policy, StructureKind};
use crate::sim::{Game, JourneyContinuation};
use crate::strategy::RecruitStatus;
use crate::survivors::SurvivorId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Highest HP percentage at which an autonomous defender may be recalled.
pub const MAX_RECALL_PERCENT: u32 = 75;

/// Autonomous defender tactics proposed as one atomic policy change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcTactics {
    pub policy: Policy,
    pub prefer_aim: bool,
    pub early_reload: bool,
    pub allow_risk: bool,
    pub recall_percent: u32,
}

/// A player intent against the authoritative game. Pure data so frontends can
/// queue, deduplicate and record it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    ContinueJourney,
    LeaveJourney,
    SetBalance(BalanceSettings),
    ChooseSurvivor(SurvivorId),
    ChooseStart(String),
    EnterCountry(String),
    Deploy {
        stage: usize,
        mode: Mode,
    },
    Withdraw,
    SetPaused(bool),
    Place {
        kind: StructureKind,
        lane: u8,
        column: u8,
    },
    Salvage(u64),
    SaveStartingLayout,
    SaveExpeditionLayout(u64),
    ClearExpeditionLayout(u64),
    ClearLayout,
    RespecPlayer,
    BuySkillNode(String),
    BuyItem(usize),
    Equip {
        mode: Mode,
        id: u64,
    },
    Lock {
        id: u64,
        locked: bool,
    },
    Favorite {
        id: u64,
        favorite: bool,
    },
    SellItem(u64),
    SellUnprotected,
    SavePreset {
        name: String,
        mode: Mode,
    },
    ApplyPreset {
        index: usize,
        mode: Mode,
    },
    BuyNpcPoint,
    TrainNpc(NpcTraining),
    RespecNpc,
    SetNpcPolicy(NpcTactics),
    Recruit(String),
    EquipRecruit {
        id: u64,
        item: u64,
    },
    TrainRecruit {
        id: u64,
        training: NpcTraining,
    },
    BuyRecruitPoint(u64),
    SetRecruitPolicy {
        id: u64,
        policy: Policy,
    },
    AssignGuard {
        id: u64,
        country: String,
    },
    ReleaseGuard(u64),
    DeploySurvivor {
        id: u64,
        country: String,
        stage: usize,
    },
    CountryOperation {
        id: u64,
        country: String,
    },
    RecallSurvivor(u64),
    PlaceExpedition {
        battle: u64,
        kind: StructureKind,
        lane: u8,
        column: u8,
    },
    SalvageExpedition {
        battle: u64,
        structure: u64,
    },
}

/// What an applied command produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutcome {
    Applied,
    CountryLiberated,
    /// A newly created identity (item, recruit or expedition).
    Id(u64),
    /// Coins or supply returned by the command.
    Amount(u64),
}

/// Why a command was rejected. `Display` is the user-facing message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    SurvivorNotChosen,
    NoSelectedCountry,
    UnknownExpedition,
    ExpeditionEnded,
    RecallThresholdOutOfRange(u32),
    UnknownSurvivor,
    RecruitNotReady,
    /// A campaign, strategy or simulation rule rejected the command.
    Rejected(String),
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurvivorNotChosen => {
                f.write_str("Choose your survivor before selecting a starting country")
            }
            Self::NoSelectedCountry => f.write_str("No selected country"),
            Self::UnknownExpedition => f.write_str(crate::sim::UNKNOWN_EXPEDITION),
            Self::ExpeditionEnded => f.write_str("Expedition has ended"),
            Self::RecallThresholdOutOfRange(_) => write!(
                f,
                "Recall threshold must be between 0 and {MAX_RECALL_PERCENT} percent"
            ),
            Self::UnknownSurvivor => f.write_str("Unknown survivor"),
            Self::RecruitNotReady => {
                f.write_str("Recall the survivor before changing their preparation policy")
            }
            Self::Rejected(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for CommandError {}
impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::Rejected(message)
    }
}

impl Game {
    /// The mutation path for frontends: validates `command` against every
    /// domain rule and applies it atomically or not at all.
    pub fn apply(&mut self, command: &Command) -> Result<CommandOutcome, CommandError> {
        use CommandOutcome::{Amount, Applied, Id};
        Ok(match command {
            Command::ContinueJourney => match self.continue_journey()? {
                JourneyContinuation::Deployed => Applied,
                JourneyContinuation::CountryLiberated => CommandOutcome::CountryLiberated,
            },
            Command::LeaveJourney => {
                self.leave_journey();
                Applied
            }
            Command::SetBalance(settings) => {
                self.campaign.set_balance_settings(settings.clone())?;
                Applied
            }
            Command::ChooseSurvivor(id) => {
                self.campaign.choose_survivor(*id)?;
                Applied
            }
            Command::ChooseStart(id) => {
                if self.campaign.selected_survivor.is_none() {
                    return Err(CommandError::SurvivorNotChosen);
                }
                let mut selected = self.clone();
                selected.campaign.choose_start_country(id)?;
                selected.deploy_country_stage(0, Mode::Personal)?;
                selected.set_paused(false);
                *self = selected;
                Applied
            }
            Command::EnterCountry(id) => {
                let mut selected = self.clone();
                selected.campaign.enter_country(id)?;
                let country = selected
                    .campaign
                    .world
                    .active_country()
                    .ok_or(CommandError::NoSelectedCountry)?;
                if selected.campaign.world.completed(&country.id) >= country.level_count() {
                    *self = selected;
                    CommandOutcome::CountryLiberated
                } else {
                    let stage = selected.campaign.world.next_stage();
                    selected.deploy_country_stage(stage, Mode::Personal)?;
                    selected.set_paused(false);
                    *self = selected;
                    Applied
                }
            }
            Command::Deploy { stage, mode } => {
                self.deploy_country_stage(*stage, *mode)?;
                self.set_paused(false);
                Applied
            }
            Command::Withdraw => {
                self.withdraw();
                self.set_paused(true);
                Applied
            }
            Command::SetPaused(paused) => {
                self.set_paused(*paused);
                Applied
            }
            Command::Place { kind, lane, column } => {
                self.place(*kind, *lane, *column)?;
                Applied
            }
            Command::Salvage(id) => Amount(u64::from(self.salvage(*id)?)),
            Command::SaveStartingLayout => {
                self.save_country_layout()?;
                Applied
            }
            Command::ClearLayout => {
                self.clear_country_layout()?;
                Applied
            }
            Command::SaveExpeditionLayout(id) => {
                let battle = self
                    .active_battle(*id)
                    .ok_or(CommandError::UnknownExpedition)?;
                let attempt = battle
                    .game
                    .attempt
                    .as_ref()
                    .ok_or(CommandError::ExpeditionEnded)?;
                let layout = attempt.structure_layout();
                let country = battle.country_id.clone();
                self.campaign.save_country_layout(&country, layout)?;
                Applied
            }
            Command::ClearExpeditionLayout(id) => {
                let country = self
                    .active_battle(*id)
                    .ok_or(CommandError::UnknownExpedition)?
                    .country_id
                    .clone();
                self.campaign.clear_country_layout(&country)?;
                Applied
            }
            Command::RespecPlayer => {
                self.campaign.respec_player()?;
                Applied
            }
            Command::BuySkillNode(id) => {
                self.campaign.purchase_node(id)?;
                Applied
            }
            Command::BuyItem(index) => Id(self.campaign.buy_item(*index)?),
            Command::Equip { mode, id } => {
                self.campaign.equip(*mode, *id)?;
                Applied
            }
            Command::Lock { id, locked } => {
                self.campaign.set_lock(*id, *locked)?;
                Applied
            }
            Command::Favorite { id, favorite } => {
                self.campaign.set_favorite(*id, *favorite)?;
                Applied
            }
            Command::SellItem(id) => Amount(self.sell_item(*id)?),
            Command::SellUnprotected => Amount(self.sell_unprotected()?),
            Command::SavePreset { name, mode } => {
                self.campaign.save_preset(name.clone(), *mode)?;
                Applied
            }
            Command::ApplyPreset { index, mode } => {
                self.campaign.apply_preset(*index, *mode)?;
                Applied
            }
            Command::BuyNpcPoint => {
                self.campaign.buy_training_point()?;
                Applied
            }
            Command::TrainNpc(training) => {
                self.campaign.train_npc(*training)?;
                Applied
            }
            Command::RespecNpc => {
                self.campaign.respec_npc()?;
                Applied
            }
            Command::SetNpcPolicy(tactics) => {
                if tactics.recall_percent > MAX_RECALL_PERCENT {
                    return Err(CommandError::RecallThresholdOutOfRange(
                        tactics.recall_percent,
                    ));
                }
                let npc = &mut self.campaign.npc;
                npc.policy = tactics.policy;
                npc.prefer_aim = tactics.prefer_aim;
                npc.early_reload = tactics.early_reload;
                npc.allow_risk = tactics.allow_risk;
                npc.recall_percent = tactics.recall_percent;
                Applied
            }
            Command::Recruit(country) => Id(self.strategy.recruit(&mut self.campaign, country)?),
            Command::EquipRecruit { id, item } => {
                self.strategy.equip_recruit(&self.campaign, *id, *item)?;
                Applied
            }
            Command::TrainRecruit { id, training } => {
                self.strategy
                    .train_recruit(&mut self.campaign, *id, *training)?;
                Applied
            }
            Command::BuyRecruitPoint(id) => {
                self.strategy.buy_training_point(&mut self.campaign, *id)?;
                Applied
            }
            Command::SetRecruitPolicy { id, policy } => {
                let recruit = self
                    .strategy
                    .roster
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or(CommandError::UnknownSurvivor)?;
                if recruit.status != RecruitStatus::Ready {
                    return Err(CommandError::RecruitNotReady);
                }
                recruit.npc.policy = *policy;
                Applied
            }
            Command::AssignGuard { id, country } => {
                self.strategy
                    .assign_guard(&mut self.campaign, *id, country)?;
                Applied
            }
            Command::ReleaseGuard(id) => {
                self.strategy.release_guard(&mut self.campaign, *id)?;
                Applied
            }
            Command::DeploySurvivor { id, country, stage } => {
                Id(self.deploy_survivor(*id, country, *stage)?)
            }
            Command::CountryOperation { id, country } => {
                Id(self.deploy_country_operation(*id, country)?)
            }
            Command::RecallSurvivor(id) => {
                self.recall_survivor(*id)?;
                Applied
            }
            Command::PlaceExpedition {
                battle,
                kind,
                lane,
                column,
            } => {
                self.active_battle_mut(*battle)
                    .ok_or(CommandError::UnknownExpedition)?
                    .game
                    .place(*kind, *lane, *column)?;
                Applied
            }
            Command::SalvageExpedition { battle, structure } => {
                let worker = &mut self
                    .active_battle_mut(*battle)
                    .ok_or(CommandError::UnknownExpedition)?
                    .game;
                Amount(u64::from(worker.salvage(*structure)?))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(game: &Game) -> serde_json::Value {
        serde_json::to_value(game).unwrap()
    }
    /// A legacy-feature campaign with four liberated countries so recruits
    /// and parallel expeditions are unlocked.
    fn strategic_game() -> Game {
        let mut g = Game::default();
        g.campaign.legacy_features = true;
        g.campaign.balance_settings = BalanceSettings::legacy();
        let countries: Vec<_> = crate::world::world()
            .countries
            .iter()
            .take(4)
            .map(|c| (c.id.clone(), c.level_count()))
            .collect();
        g.campaign.choose_start_country(&countries[0].0).unwrap();
        for (id, count) in countries {
            g.campaign.world.country_levels.insert(id.clone(), count);
            g.campaign.world.cleared.push(id);
        }
        g.campaign.coins = 10_000;
        g.campaign.attempts = 1;
        g
    }
    fn tactics(recall_percent: u32) -> NpcTactics {
        NpcTactics {
            policy: Policy::Marksman,
            prefer_aim: true,
            early_reload: false,
            allow_risk: true,
            recall_percent,
        }
    }

    #[test]
    fn recall_threshold_above_limit_is_rejected_without_mutation() {
        let mut game = Game::default();
        let before = snapshot(&game);
        let error = game
            .apply(&Command::SetNpcPolicy(tactics(MAX_RECALL_PERCENT + 1)))
            .unwrap_err();
        assert_eq!(error, CommandError::RecallThresholdOutOfRange(76));
        assert_eq!(
            error.to_string(),
            "Recall threshold must be between 0 and 75 percent"
        );
        assert_eq!(snapshot(&game), before);
        assert_eq!(
            game.apply(&Command::SetNpcPolicy(tactics(MAX_RECALL_PERCENT))),
            Ok(CommandOutcome::Applied)
        );
        let npc = &game.campaign.npc;
        assert_eq!(
            (
                npc.policy,
                npc.prefer_aim,
                npc.allow_risk,
                npc.recall_percent
            ),
            (Policy::Marksman, true, true, 75)
        );
    }
    #[test]
    fn recruit_policy_requires_a_ready_recruit() {
        let mut game = strategic_game();
        let country = game.campaign.world.cleared[0].clone();
        let Ok(CommandOutcome::Id(recruit)) = game.apply(&Command::Recruit(country.clone())) else {
            panic!("recruit should succeed");
        };
        assert_eq!(
            game.apply(&Command::SetRecruitPolicy {
                id: recruit + 1000,
                policy: Policy::Marksman,
            }),
            Err(CommandError::UnknownSurvivor)
        );
        let Ok(CommandOutcome::Id(battle)) = game.apply(&Command::DeploySurvivor {
            id: recruit,
            country,
            stage: 0,
        }) else {
            panic!("deployment should succeed");
        };
        let before = snapshot(&game);
        let error = game
            .apply(&Command::SetRecruitPolicy {
                id: recruit,
                policy: Policy::Marksman,
            })
            .unwrap_err();
        assert_eq!(error, CommandError::RecruitNotReady);
        assert_eq!(
            error.to_string(),
            "Recall the survivor before changing their preparation policy"
        );
        assert_eq!(snapshot(&game), before);
        game.apply(&Command::RecallSurvivor(battle)).unwrap();
        game.apply(&Command::SetRecruitPolicy {
            id: recruit,
            policy: Policy::Marksman,
        })
        .unwrap();
        let recruit = game.strategy.roster.iter().find(|r| r.id == recruit);
        assert_eq!(recruit.unwrap().npc.policy, Policy::Marksman);
    }
    #[test]
    fn purchase_charges_and_returns_the_new_item_identity() {
        let mut game = Game::default();
        let index = crate::content::content()
            .items
            .iter()
            .position(|item| item.tier == 1)
            .unwrap();
        let price = game.campaign.buy_price(index).unwrap();
        game.campaign.coins = price + 7;
        let outcome = game.apply(&Command::BuyItem(index)).unwrap();
        let CommandOutcome::Id(id) = outcome else {
            panic!("purchase returns an item id, got {outcome:?}");
        };
        let item = game.campaign.items.iter().find(|item| item.id == id);
        assert_eq!(item.unwrap().definition_index(), index);
        assert_eq!(game.campaign.coins, 7);
        game.campaign.coins = 0;
        let before = snapshot(&game);
        assert!(matches!(
            game.apply(&Command::BuyItem(index)),
            Err(CommandError::Rejected(_))
        ));
        assert_eq!(snapshot(&game), before);
    }
    #[test]
    fn worker_commands_target_only_active_expeditions() {
        let mut game = strategic_game();
        let country = game.campaign.world.cleared[0].clone();
        let Ok(CommandOutcome::Id(recruit)) = game.apply(&Command::Recruit(country.clone())) else {
            panic!("recruit should succeed");
        };
        let kind = StructureKind::Wall;
        game.campaign.blueprints[kind] = true;
        let Ok(CommandOutcome::Id(battle)) = game.apply(&Command::DeploySurvivor {
            id: recruit,
            country: country.clone(),
            stage: 0,
        }) else {
            panic!("deployment should succeed");
        };
        game.apply(&Command::PlaceExpedition {
            battle,
            kind,
            lane: 0,
            column: 0,
        })
        .unwrap();
        let worker = game.active_battle(battle).unwrap();
        assert_eq!(worker.game.attempt.as_ref().unwrap().structures.len(), 1);
        game.apply(&Command::SaveExpeditionLayout(battle)).unwrap();
        let layout = &game.campaign.country_layouts[&country];
        assert_eq!(
            layout.as_slice(),
            &[crate::campaign::Placement {
                kind,
                lane: 0,
                column: 0
            }]
        );
        game.apply(&Command::RecallSurvivor(battle)).unwrap();
        assert!(game.active_battle(battle).is_none());
        for command in [
            Command::SaveExpeditionLayout(battle),
            Command::ClearExpeditionLayout(battle),
            Command::SalvageExpedition {
                battle,
                structure: 1,
            },
            Command::RecallSurvivor(battle),
        ] {
            let error = game.apply(&command).unwrap_err();
            assert_eq!(error.to_string(), "No active expedition with that ID");
        }
    }
    #[test]
    fn commands_round_trip_through_serde_for_replay() {
        let commands = [
            Command::SetNpcPolicy(tactics(30)),
            Command::PlaceExpedition {
                battle: 4,
                kind: StructureKind::Wall,
                lane: 2,
                column: 1,
            },
            Command::ChooseSurvivor(SurvivorId::June),
        ];
        for command in commands {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        }
    }
}
