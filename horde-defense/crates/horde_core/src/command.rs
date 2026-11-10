//! Typed gameplay commands. `Game::apply` is the single mutation path for
//! frontends: every domain rule a player intent must satisfy lives here or in
//! the campaign/strategy/sim methods it calls, never in a frontend.
use crate::balance::BalanceSettings;
use crate::campaign::{AutoSellRule, Npc, NpcTraining, SaleBlock};
use crate::content::{ItemDefId, Mode, Policy, StructureKind};
use crate::defenders::{DefenderBlocker, DefenderId, RecruitBlocker};
use crate::quality::Quality;
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
impl NpcTactics {
    /// Writes the five tactics fields onto `npc`; nothing else changes.
    fn write_to(&self, npc: &mut Npc) {
        npc.policy = self.policy;
        npc.prefer_aim = self.prefer_aim;
        npc.early_reload = self.early_reload;
        npc.allow_risk = self.allow_risk;
        npc.recall_percent = self.recall_percent;
    }
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
    /// Buy one of the catalogue item named by its stable definition ID.
    BuyItem(ItemDefId),
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
    /// Removes the saved preset at `index`; its items stop blocking sales.
    DeletePreset {
        index: usize,
    },
    /// Stores the auto-sell rule; allowed in or out of battle, it acts at
    /// the next settlement.
    SetAutoSell(AutoSellRule),
    /// Sells the stash items the stored rule matches now (ignoring
    /// `enabled`), but only if that is still `expect_count` items for
    /// `expect_coins`: the quote the player confirmed.
    ApplyAutoSellToStash {
        expect_count: u32,
        expect_coins: u64,
    },
    /// Chooses who defends next (only between defenses). `BeginDefense` and
    /// `ContinueJourney` deploy the choice; `Deploy` never reads it.
    SetNextDefender(DefenderId),
    /// Deploys [`Game::next_defender`] on the active country's `stage`, with
    /// `Deploy`'s unpause, and clears the choice on success.
    BeginDefense {
        stage: usize,
    },
    /// Sets a recruit's tactics: a Ready recruit in camp, or the recruit
    /// Defending the main run, whose next npc turn uses them. `SetNpcPolicy`
    /// always edits Mara.
    SetRecruitTactics {
        id: u64,
        tactics: NpcTactics,
    },
    /// Marks the named settled expeditions' outcome notices seen and takes
    /// `folded` off the folded-outcome count. Idempotent: unknown, live and
    /// already acknowledged ids are ignored and the count saturates at 0.
    AcknowledgeOutcomes {
        battles: Vec<u64>,
        folded: u32,
    },
    /// Makes the country the active one between defenses, never deploying:
    /// `BeginDefense` starts its defense. A liberated country (an active
    /// outbreak's included) answers `CountryLiberated`. `EnterCountry` keeps
    /// its recorded meaning, select and deploy at once.
    SelectCountry(String),
    /// Recruits from a liberated country as one of the two archetypes
    /// `Game::recruit_offer` names, by its `Archetype::id`. `Recruit` keeps
    /// taking the first offer.
    RecruitAs {
        country: String,
        archetype: String,
    },
}

impl Command {
    /// The roster recruit this command sends away from camp. Dispatching the
    /// next defender hands the next defense back to the player.
    fn dispatched_recruit(&self) -> Option<u64> {
        match self {
            Self::AssignGuard { id, .. }
            | Self::DeploySurvivor { id, .. }
            | Self::CountryOperation { id, .. } => Some(*id),
            _ => None,
        }
    }
    /// The roster recruit this command acts on, for the commands a recruit
    /// defending the main run must refuse (see [`Game::apply`]).
    fn defender_gated_recruit(&self) -> Option<u64> {
        match self {
            Self::AssignGuard { id, .. }
            | Self::DeploySurvivor { id, .. }
            | Self::CountryOperation { id, .. }
            | Self::TrainRecruit { id, .. }
            | Self::EquipRecruit { id, .. }
            | Self::BuyRecruitPoint(id)
            | Self::SetRecruitPolicy { id, .. } => Some(*id),
            _ => None,
        }
    }
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
    /// One batch sale: how many items sold and what they paid together.
    Sold {
        count: u32,
        coins: u64,
    },
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
    /// The command is only allowed between defenses, with no live attempt.
    DefenseInProgress,
    /// The named defender cannot take this command right now.
    DefenderUnavailable {
        defender: DefenderId,
        blocker: DefenderBlocker,
    },
    /// The item cannot be sold; the block says why (strict `SellItem`).
    SaleBlocked(SaleBlock),
    /// The sale would overflow the wallet; nothing was sold.
    WalletLimit,
    /// An auto-sell rule must sell below a quality above Poor.
    AutoSellRuleInvalid(Quality),
    /// The auto-sell rule matches no sellable stash item.
    NothingToSell,
    /// The stash changed since the quote; this is the current quote and
    /// nothing was sold.
    SaleQuoteChanged {
        count: u32,
        coins: u64,
    },
    /// The campaign has no starting country yet; `ChooseStart` chooses it.
    StartNotChosen,
    /// The country cannot be entered yet (or is not on the map).
    CountryLocked(String),
    /// `RecruitAs` named an archetype the country does not offer (this
    /// text, unknown names included); nothing changed.
    ArchetypeNotOffered(String),
    /// The country offers no recruit now; the blocker says why.
    RecruitUnavailable(RecruitBlocker),
    /// The row is not trainable (IQ, HP or FP; see `NpcTraining::TRAINABLE`);
    /// nothing changed.
    TrainingUnsupported(NpcTraining),
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
            Self::DefenseInProgress => f.write_str("Only possible between defenses"),
            Self::DefenderUnavailable { blocker, .. } => write!(f, "{blocker}"),
            Self::SaleBlocked(block) => f.write_str(&block.label()),
            Self::WalletLimit => f.write_str("Sale would exceed the wallet limit"),
            Self::AutoSellRuleInvalid(_) => f.write_str("Auto-sell threshold must be above Poor"),
            Self::NothingToSell => f.write_str("Nothing in the stash matches the auto-sell rule"),
            Self::SaleQuoteChanged { count, coins } => {
                let items = if *count == 1 { "item" } else { "items" };
                let unit = if *coins == 1 { "coin" } else { "coins" };
                write!(
                    f,
                    "The stash changed: auto-sell would now sell {count} {items} for {coins} {unit}"
                )
            }
            Self::StartNotChosen => f.write_str("Choose a starting country first"),
            Self::CountryLocked(_) => {
                f.write_str("Liberate a neighboring country to reach this one")
            }
            Self::ArchetypeNotOffered(_) => {
                f.write_str("This country's survivors do not offer that archetype")
            }
            Self::RecruitUnavailable(blocker) => write!(f, "{blocker}"),
            Self::TrainingUnsupported(training) => f.write_str(match training {
                NpcTraining::Intelligence => "IQ cannot be trained",
                NpcTraining::HitPoints => "HP cannot be trained on its own; train ST instead",
                NpcTraining::Fatigue => "FP cannot be trained on its own; train HT instead",
                _ => "This attribute cannot be trained",
            }),
        }
    }
}
impl std::error::Error for CommandError {}

/// The typed training verdict `TrainNpc` and `TrainRecruit` check before the
/// campaign is reached.
fn trainable(training: NpcTraining) -> Result<(), CommandError> {
    if training.is_trainable() {
        Ok(())
    } else {
        Err(CommandError::TrainingUnsupported(training))
    }
}
impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::Rejected(message)
    }
}

impl Game {
    /// The single frontend mutation path: validates `command` against every
    /// domain rule before changing state. Frontend widgets only see `&Game`, so
    /// this is enforced by types there; `Game` fields stay public in core
    /// (narrowing them is a tracked follow-up). Only the staged arms
    /// (`ChooseStart`, `EnterCountry`) apply to a clone and commit on success;
    /// every other arm relies on the atomicity of the domain method it calls.
    pub fn apply(&mut self, command: &Command) -> Result<CommandOutcome, CommandError> {
        use CommandOutcome::{Amount, Applied, Id};
        if let Some(id) = command.defender_gated_recruit() {
            self.reject_defending_recruit(id)?;
        }
        let leaving = command
            .dispatched_recruit()
            .filter(|id| self.next_defender() == DefenderId::Recruit(*id));
        let outcome = match command {
            Command::ContinueJourney => match self.continue_next_defense()? {
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
                // The journey's first defense waits for "Begin defense", so
                // the controls can be read before anything moves.
                selected.set_paused(true);
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
            // Frozen meaning: Personal is the player and Npc is Mara; the
            // chosen next defender is neither read nor cleared.
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
            Command::BuyItem(definition) => {
                let index = definition
                    .index()
                    .ok_or_else(|| CommandError::Rejected("Unknown equipment".into()))?;
                Id(self.campaign.buy_item(index)?)
            }
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
            Command::DeletePreset { index } => {
                self.campaign.delete_preset(*index)?;
                Applied
            }
            Command::SetAutoSell(rule) => {
                self.set_auto_sell(*rule)?;
                Applied
            }
            Command::ApplyAutoSellToStash {
                expect_count,
                expect_coins,
            } => {
                let (count, coins) = self.apply_auto_sell_to_stash(*expect_count, *expect_coins)?;
                CommandOutcome::Sold { count, coins }
            }
            Command::BuyNpcPoint => {
                self.campaign.buy_training_point()?;
                Applied
            }
            Command::TrainNpc(training) => {
                trainable(*training)?;
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
                tactics.write_to(&mut self.campaign.npc);
                Applied
            }
            Command::Recruit(country) => Id(self.recruit_first_offer(country)?),
            Command::EquipRecruit { id, item } => {
                self.strategy.equip_recruit(&self.campaign, *id, *item)?;
                Applied
            }
            Command::TrainRecruit { id, training } => {
                trainable(*training)?;
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
                if let Some(r) = self.strategy.roster.iter().find(|r| r.id == *id) {
                    let loadout = r.npc.loadout;
                    self.unqueue_reserved(loadout);
                }
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
            Command::SetNextDefender(defender) => {
                self.set_next_defender(*defender)?;
                Applied
            }
            Command::BeginDefense { stage } => {
                self.begin_defense(*stage)?;
                Applied
            }
            // Ruling D12: a Defending recruit's roster npc is the main run's
            // npc (D6), so the change acts at its next npc turn.
            Command::SetRecruitTactics { id, tactics } => {
                if tactics.recall_percent > MAX_RECALL_PERCENT {
                    return Err(CommandError::RecallThresholdOutOfRange(
                        tactics.recall_percent,
                    ));
                }
                let recruit = self
                    .strategy
                    .roster
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or(CommandError::UnknownSurvivor)?;
                if !matches!(
                    recruit.status,
                    RecruitStatus::Ready | RecruitStatus::Defending
                ) {
                    return Err(CommandError::RecruitNotReady);
                }
                tactics.write_to(&mut recruit.npc);
                Applied
            }
            Command::AcknowledgeOutcomes { battles, folded } => {
                self.acknowledge_outcomes(battles, *folded);
                Applied
            }
            Command::SelectCountry(id) => self.select_country(id)?,
            Command::RecruitAs { country, archetype } => Id(self.recruit_as(country, archetype)?),
        };
        // Only a dispatch that succeeded takes the next defender away.
        if let Some(id) = leaving {
            self.next_defender_dispatched(id);
        }
        Ok(outcome)
    }
}

impl Game {
    /// The typed pre-check for recruit commands: a recruit defending the main
    /// run cannot be guarded, dispatched, trained, re-equipped or re-tasked.
    /// Unknown ids fall through to each command's own `UnknownSurvivor` path.
    fn reject_defending_recruit(&self, id: u64) -> Result<(), CommandError> {
        let defending = self
            .strategy
            .roster
            .iter()
            .any(|r| r.id == id && r.status == RecruitStatus::Defending);
        if defending {
            return Err(CommandError::DefenderUnavailable {
                defender: DefenderId::Recruit(id),
                blocker: DefenderBlocker::Defending,
            });
        }
        Ok(())
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
    fn the_first_defense_waits_for_the_player_to_begin() {
        let mut game = Game::default();
        game.apply(&Command::ChooseSurvivor(SurvivorId::Rook))
            .unwrap();
        game.apply(&Command::ChooseStart("DEU".into())).unwrap();
        let a = game.attempt.as_ref().unwrap();
        assert!(a.paused && game.global_paused, "first defense starts live");
        assert_eq!(a.tick, 0);
        game.apply(&Command::SetPaused(false)).unwrap();
        assert!(!game.attempt.as_ref().unwrap().paused);
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
        let definition = crate::content::ItemDefId::at(index);
        let outcome = game.apply(&Command::BuyItem(definition.clone())).unwrap();
        let CommandOutcome::Id(id) = outcome else {
            panic!("purchase returns an item id, got {outcome:?}");
        };
        let item = game.campaign.items.iter().find(|item| item.id == id);
        assert_eq!(item.unwrap().definition, definition);
        assert_eq!(game.campaign.coins, 7);
        game.campaign.coins = 0;
        let before = snapshot(&game);
        assert!(matches!(
            game.apply(&Command::BuyItem(definition)),
            Err(CommandError::Rejected(_))
        ));
        assert_eq!(snapshot(&game), before);
        game.campaign.coins = price + 7;
        assert!(matches!(
            game.apply(&Command::BuyItem("no_such_item".into())),
            Err(CommandError::Rejected(_))
        ));
        assert_eq!(game.campaign.coins, price + 7);
    }
    #[test]
    fn buy_item_command_refuses_every_loot_only_definition() {
        // The command path horde_game's armory and the debug recordings use:
        // a loot-only definition (29-67, W2-08) is Rejected with the loot
        // reason before the tier and coin checks, and nothing changes.
        let mut game = Game::default();
        game.campaign.coins = 1_000_000;
        let before = snapshot(&game);
        let items = &crate::content::content().items;
        let loot_only: Vec<usize> = (0..items.len()).filter(|&n| items[n].loot_only).collect();
        assert_eq!(loot_only, (29..68).collect::<Vec<_>>());
        for n in loot_only {
            assert_eq!(
                game.apply(&Command::BuyItem(crate::content::ItemDefId::at(n))),
                Err(CommandError::Rejected(
                    crate::content::LOOT_ONLY_REASON.into()
                )),
                "{}",
                items[n].id
            );
            assert_eq!(snapshot(&game), before, "{}", items[n].id);
        }
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
    /// The `Command` variants appended after recording FORMAT 1's 41
    /// (`tests/recording_vocabulary.rs`), in declaration order.
    const APPENDED_COMMANDS: [&str; 9] = [
        "DeletePreset",
        "SetAutoSell",
        "ApplyAutoSellToStash",
        "SetNextDefender",
        "BeginDefense",
        "SetRecruitTactics",
        "AcknowledgeOutcomes",
        "SelectCountry",
        "RecruitAs",
    ];
    /// The index in [`APPENDED_COMMANDS`] of `command`'s variant, or None for
    /// one of the 41 FORMAT 1 variants. Exhaustive: an appended variant does
    /// not compile until it is listed here, and the round-trip test below
    /// then wants a sample of it.
    fn appended_index(command: &Command) -> Option<usize> {
        match command {
            Command::ContinueJourney
            | Command::LeaveJourney
            | Command::SetBalance(_)
            | Command::ChooseSurvivor(_)
            | Command::ChooseStart(_)
            | Command::EnterCountry(_)
            | Command::Deploy { .. }
            | Command::Withdraw
            | Command::SetPaused(_)
            | Command::Place { .. }
            | Command::Salvage(_)
            | Command::SaveStartingLayout
            | Command::SaveExpeditionLayout(_)
            | Command::ClearExpeditionLayout(_)
            | Command::ClearLayout
            | Command::RespecPlayer
            | Command::BuySkillNode(_)
            | Command::BuyItem(_)
            | Command::Equip { .. }
            | Command::Lock { .. }
            | Command::Favorite { .. }
            | Command::SellItem(_)
            | Command::SellUnprotected
            | Command::SavePreset { .. }
            | Command::ApplyPreset { .. }
            | Command::BuyNpcPoint
            | Command::TrainNpc(_)
            | Command::RespecNpc
            | Command::SetNpcPolicy(_)
            | Command::Recruit(_)
            | Command::EquipRecruit { .. }
            | Command::TrainRecruit { .. }
            | Command::BuyRecruitPoint(_)
            | Command::SetRecruitPolicy { .. }
            | Command::AssignGuard { .. }
            | Command::ReleaseGuard(_)
            | Command::DeploySurvivor { .. }
            | Command::CountryOperation { .. }
            | Command::RecallSurvivor(_)
            | Command::PlaceExpedition { .. }
            | Command::SalvageExpedition { .. } => None,
            Command::DeletePreset { .. } => Some(0),
            Command::SetAutoSell(_) => Some(1),
            Command::ApplyAutoSellToStash { .. } => Some(2),
            Command::SetNextDefender(_) => Some(3),
            Command::BeginDefense { .. } => Some(4),
            Command::SetRecruitTactics { .. } => Some(5),
            Command::AcknowledgeOutcomes { .. } => Some(6),
            Command::SelectCountry(_) => Some(7),
            Command::RecruitAs { .. } => Some(8),
        }
    }
    /// W2-48 (design-save-determinism §6): a debug recording replays every
    /// command it holds, so each appended variant, at non-default values,
    /// parses back to itself and reserializes to the very line it was read
    /// from, under its own externally tagged name. One sample per appended
    /// variant, in declaration order, beside three FORMAT 1 variants.
    #[test]
    fn commands_round_trip_through_serde_for_replay() {
        let v1 = [
            Command::SetNpcPolicy(tactics(30)),
            Command::PlaceExpedition {
                battle: 4,
                kind: StructureKind::Wall,
                lane: 2,
                column: 1,
            },
            Command::ChooseSurvivor(SurvivorId::June),
        ];
        let rule = AutoSellRule {
            enabled: true,
            sell_below: Quality::Epic,
            slots: [false, true, true, false],
            keep_upgrades: false,
            apply_to_npc_runs: false,
        };
        let appended = [
            Command::DeletePreset { index: 2 },
            Command::SetAutoSell(rule),
            Command::ApplyAutoSellToStash {
                expect_count: 7,
                expect_coins: 1_234,
            },
            Command::SetNextDefender(DefenderId::Recruit(3)),
            Command::BeginDefense { stage: 4 },
            Command::SetRecruitTactics {
                id: 5,
                tactics: tactics(40),
            },
            Command::AcknowledgeOutcomes {
                battles: vec![9, 12],
                folded: 3,
            },
            Command::SelectCountry("POL".into()),
            Command::RecruitAs {
                country: "HUN".into(),
                archetype: "medic".into(),
            },
        ];
        assert!(v1.iter().all(|c| appended_index(c).is_none()));
        assert_eq!(
            appended.iter().map(appended_index).collect::<Vec<_>>(),
            (0..APPENDED_COMMANDS.len()).map(Some).collect::<Vec<_>>(),
            "one sample per appended variant, in declaration order"
        );
        let defenders = [DefenderId::Player, DefenderId::Mara].map(Command::SetNextDefender);
        for command in v1.into_iter().chain(appended).chain(defenders) {
            let line = serde_json::to_string(&command).unwrap();
            let replayed: Command = serde_json::from_str(&line).unwrap();
            assert_eq!(replayed, command, "{line}");
            assert_eq!(serde_json::to_string(&replayed).unwrap(), line);
            if let Some(index) = appended_index(&command) {
                let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                let name = value.as_object().and_then(|map| map.keys().next());
                assert_eq!(
                    name.map(String::as_str),
                    Some(APPENDED_COMMANDS[index]),
                    "{line}"
                );
            }
        }
    }
    #[test]
    fn command_serde_round_trip_covers_appended_economy_variants() {
        let rule = crate::campaign::AutoSellRule {
            enabled: true,
            sell_below: crate::quality::Quality::Rare,
            slots: [true, false, true, false],
            keep_upgrades: false,
            apply_to_npc_runs: false,
        };
        let commands = [
            Command::DeletePreset { index: 2 },
            Command::SetAutoSell(rule),
            Command::ApplyAutoSellToStash {
                expect_count: 50,
                expect_coins: 1_234,
            },
        ];
        for command in commands {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        }
        let sold = CommandOutcome::Sold {
            count: 50,
            coins: 1_234,
        };
        let json = serde_json::to_string(&sold).unwrap();
        assert_eq!(serde_json::from_str::<CommandOutcome>(&json).unwrap(), sold);
        // The appended errors read as plain sentences.
        for error in [
            CommandError::AutoSellRuleInvalid(crate::quality::Quality::Poor),
            CommandError::NothingToSell,
            CommandError::SaleQuoteChanged {
                count: 3,
                coins: 40,
            },
        ] {
            let text = error.to_string();
            assert!(!text.is_empty() && !text.contains("::"), "{text}");
        }
    }

    #[test]
    fn sale_quote_changed_picks_nouns_by_count() {
        let single = CommandError::SaleQuoteChanged { count: 1, coins: 1 };
        assert_eq!(
            single.to_string(),
            "The stash changed: auto-sell would now sell 1 item for 1 coin"
        );
        let several = CommandError::SaleQuoteChanged { count: 2, coins: 5 };
        assert_eq!(
            several.to_string(),
            "The stash changed: auto-sell would now sell 2 items for 5 coins"
        );
    }
}
