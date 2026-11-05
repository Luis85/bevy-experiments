//! Multi-expedition orchestration and monotonic shared-reward settlement.
use super::*;

/// Single source for the unknown-expedition message shared with `command`.
pub(crate) const UNKNOWN_EXPEDITION: &str = "No active expedition with that ID";

/// Seats recruit `recruit_id` on a freshly deployed attempt: the saved seat
/// names the recruit and the profile takes the recruit's identity-table
/// appearance instead of the Npc deploy's Female default. Presentation only
/// reads the appearance; no rule or RNG draw depends on either field.
pub(crate) fn stamp_recruit_defender(attempt: &mut Attempt, recruit_id: u64) {
    attempt.seat = DefenderSeat::Recruit(recruit_id);
    attempt.profile.appearance = crate::defenders::recruit_identity(recruit_id).1;
}

/// A shared-campaign transfer that settlement could not complete. Settlement
/// still advances its cursors, so each issue is reported exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettlementIssue {
    LootLost {
        battle: u64,
        definition: ItemDefId,
        level: u32,
        error: String,
    },
    StageClearRejected {
        battle: u64,
        country: String,
        stage: usize,
        error: String,
    },
    RecruitReleaseFailed {
        battle: u64,
        recruit: u64,
        error: String,
    },
    /// A rank milestone's item could not be stored; its coins and the claim
    /// still apply.
    MilestoneItemLost {
        rank: u32,
        definition: ItemDefId,
        error: String,
    },
}
impl std::fmt::Display for SettlementIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LootLost {
                battle,
                definition,
                level,
                error,
            } => write!(
                f,
                "Expedition {battle} loot (item {definition}, level {level}) was not stored: {error}"
            ),
            Self::StageClearRejected {
                battle,
                country,
                stage,
                error,
            } => write!(
                f,
                "Expedition {battle} clear of {country} stage {} was not recorded: {error}",
                stage + 1
            ),
            Self::RecruitReleaseFailed {
                battle,
                recruit,
                error,
            } => write!(
                f,
                "Expedition {battle} survivor {recruit} could not return to the roster: {error}"
            ),
            Self::MilestoneItemLost {
                rank,
                definition,
                error,
            } => write!(
                f,
                "Rank {rank} milestone item {definition} was not stored: {error}"
            ),
        }
    }
}

impl Game {
    /// Each expedition owns its actual serialized battle; shared earnings are
    /// transferred by monotonic settlement cursors, never by rerunning events.
    pub fn deploy_survivor(
        &mut self,
        recruit_id: u64,
        country_id: &str,
        stage: usize,
    ) -> Result<u64, String> {
        if self.worker {
            return Err("An expedition cannot dispatch further expeditions".into());
        }
        if !self.strategy.parallel_unlocked(&self.campaign) {
            return Err("Liberate more countries to unlock parallel expeditions".into());
        }
        if self.battles.iter().filter(|b| !b.settled).count()
            >= crate::world::world().strategy_config.max_parallel
        {
            return Err("All parallel expedition slots are occupied".into());
        }
        if !self.campaign.world.can_enter(country_id) {
            return Err("Free a neighboring country before deploying here".into());
        }
        let npc = self
            .strategy
            .roster
            .iter()
            .find(|r| r.id == recruit_id)
            .ok_or("Unknown survivor")?
            .npc
            .clone();
        self.campaign.reserve_external(npc.loadout)?;
        let mut worker = Game {
            campaign: self.campaign.clone_for_worker(),
            seed: self.seed ^ recruit_id.wrapping_mul(0xD1342543DE82EF95),
            next_attempt_id: self.next_attempt_id,
            worker: true,
            ..Game::default()
        };
        worker.campaign.reserved = None;
        worker.campaign.reserved_items.clear();
        worker.campaign.npc = npc.clone();
        worker.campaign.coins = 0;
        worker.campaign.player.loadout = [1, 2, 3, 4];
        worker.campaign.presets.clear();
        worker
            .campaign
            .items
            .retain(|item| item.id <= 4 || npc.loadout.contains(&item.id));
        worker.campaign.items.shrink_to_fit();
        let operation = worker
            .campaign
            .world
            .enter(country_id)
            .and_then(|()| worker.deploy_country_stage(stage, Mode::Npc));
        if let Err(error) = operation {
            self.campaign.release_external(npc.loadout);
            return Err(error);
        }
        if let Some(attempt) = worker.attempt.as_mut() {
            stamp_recruit_defender(attempt, recruit_id);
        }
        if let Err(error) = self.strategy.reserve_recruit(recruit_id) {
            self.campaign.release_external(npc.loadout);
            return Err(error);
        }
        self.next_attempt_id += 1;
        let id = self.next_attempt_id;
        let first_item = worker.campaign.next_item_id;
        let first_item_index = worker.campaign.items.len();
        worker.set_paused(false);
        self.battles.push(ParallelBattle {
            id,
            recruit_id,
            country_id: country_id.into(),
            stage,
            game: Box::new(worker),
            synced_coins: 0,
            synced_item_id: first_item,
            synced_item_index: first_item_index,
            settled: false,
            loadout: npc.loadout,
            country_operation: false,
            summary: None,
            pending_auto_sell: Vec::new(),
        });
        while self.battles.len() > 32 {
            if let Some(n) = self.battles.iter().position(|b| b.settled) {
                self.battles.remove(n);
            } else {
                break;
            }
        }
        Ok(id)
    }
    pub fn deploy_country_operation(
        &mut self,
        recruit_id: u64,
        country_id: &str,
    ) -> Result<u64, String> {
        if !self.strategy.grand_strategy_unlocked(&self.campaign) {
            return Err("Liberate more countries to unlock country operations".into());
        }
        let country = crate::world::world()
            .country(country_id)
            .ok_or("Unknown operation country")?;
        let stage = self.campaign.world.completed(country_id);
        if stage >= country.level_count() {
            return Err("This country is already liberated; respond to an outbreak with a single deployment".into());
        }
        if self
            .battles
            .iter()
            .any(|b| !b.settled && b.country_id == country_id)
        {
            return Err("A survivor is already securing this country".into());
        }
        let id = self.deploy_survivor(recruit_id, country_id, stage)?;
        self.battles
            .iter_mut()
            .find(|b| b.id == id)
            .expect("new operation exists")
            .country_operation = true;
        Ok(id)
    }
    pub fn recall_survivor(&mut self, battle_id: u64) -> Result<(), String> {
        let battle = self
            .active_battle_mut(battle_id)
            .ok_or_else(|| UNKNOWN_EXPEDITION.to_string())?;
        battle.game.withdraw();
        self.forward_expedition_events();
        self.settle_parallel();
        Ok(())
    }
    pub(super) fn tick_parallel(&mut self) {
        for battle in &mut self.battles {
            if !battle.settled {
                battle.game.tick(Input::default());
            }
        }
        self.forward_expedition_events();
        self.settle_parallel();
    }
    fn settle_parallel(&mut self) {
        for battle in &mut self.battles {
            if battle.settled {
                continue;
            }
            let coins = battle
                .game
                .campaign
                .coins
                .saturating_sub(battle.synced_coins);
            self.campaign.reward(coins, 0, Mode::Npc);
            battle.synced_coins = battle.game.campaign.coins;
            for item in battle.game.campaign.items[battle.synced_item_index..]
                .iter()
                .filter(|item| item.id >= battle.synced_item_id)
            {
                if let Err(error) = self.campaign.add_item_with_quality(
                    item.definition_index(),
                    item.level,
                    item.quality,
                ) {
                    self.settlement_issues.push(SettlementIssue::LootLost {
                        battle: battle.id,
                        definition: item.definition.clone(),
                        level: item.level,
                        error,
                    });
                }
            }
            battle.synced_item_id = battle.game.campaign.next_item_id;
            battle.synced_item_index = battle.game.campaign.items.len();
            let had_pending = !battle.game.pending_runs.is_empty();
            for mut record in std::mem::take(&mut battle.game.pending_runs) {
                if record.outcome == Outcome::Victory {
                    record.completion_xp = self.campaign.completion_xp_preview();
                    self.campaign.first_clear(record.template, Mode::Personal);
                    if let Some(country) = &record.country_id {
                        if let Err(error) = self
                            .campaign
                            .record_country_stage_clear_in(country, record.stage)
                        {
                            self.settlement_issues
                                .push(SettlementIssue::StageClearRejected {
                                    battle: battle.id,
                                    country: country.clone(),
                                    stage: record.stage,
                                    error,
                                });
                        }
                        self.strategy.resolve_outbreak(country);
                    }
                }
                if let Some(report) = &mut battle.game.last_result
                    && report.attempt_id == record.attempt_id
                {
                    report.completion_xp = record.completion_xp;
                }
                record.worker_id = Some(battle.id);
                record.recruit_id = Some(battle.recruit_id);
                self.campaign.append_run(record);
            }
            if battle.game.attempt.is_some() {
                continue;
            }
            if let Some(report) = &mut battle.game.last_result
                && report.outcome == Outcome::Victory
            {
                if !had_pending {
                    report.completion_xp = self.campaign.completion_xp_preview();
                    self.campaign.first_clear(report.level, Mode::Personal);
                    if let Err(error) = self
                        .campaign
                        .record_country_stage_clear_in(&battle.country_id, battle.stage)
                    {
                        self.settlement_issues
                            .push(SettlementIssue::StageClearRejected {
                                battle: battle.id,
                                country: battle.country_id.clone(),
                                stage: battle.stage,
                                error,
                            });
                    }
                    self.strategy.resolve_outbreak(&battle.country_id);
                }
                if battle.country_operation && !report.country_freed {
                    battle.game.campaign.balance_settings = self.campaign.balance_settings.clone();
                    battle.game.campaign.balance_extras_basis = self.campaign.balance_extras_basis;
                    battle.game.campaign.victories = self.campaign.victories;
                    battle.game.campaign.threat_revision = 1;
                    battle.game.campaign.player.level = self.campaign.player.level;
                    battle.game.campaign.player.xp = self.campaign.player.xp;
                    battle.game.campaign.player.points = self.campaign.player.points;
                    battle.game.campaign.player.body = self.campaign.player.body.clone();
                    battle.game.campaign.player.legacy_graph_points =
                        self.campaign.player.legacy_graph_points;
                    battle.game.campaign.player.level_grant_paid =
                        self.campaign.player.level_grant_paid;
                    battle.game.campaign.milestones_claimed =
                        self.campaign.milestones_claimed.clone();
                    battle.game.campaign.skill_nodes = self.campaign.skill_nodes.clone();
                    battle.game.campaign.skills = self.campaign.skills.clone();
                    battle.game.campaign.blueprints = self.campaign.blueprints.clone();
                    battle.game.campaign.structure_upgrades =
                        self.campaign.structure_upgrades.clone();
                    for (id, count) in self
                        .campaign
                        .world
                        .country_levels
                        .iter()
                        .chain(self.campaign.world.clear_high_water.iter())
                    {
                        battle
                            .game
                            .campaign
                            .world
                            .clear_high_water
                            .entry(id.clone())
                            .and_modify(|n| *n = (*n).max(*count))
                            .or_insert(*count);
                    }
                    let next = battle.game.campaign.world.completed(&battle.country_id);
                    if battle.game.deploy_country_stage(next, Mode::Npc).is_ok() {
                        // The worker game has no roster and does not know who
                        // it carries; the parent re-seats the recruit.
                        if let Some(attempt) = battle.game.attempt.as_mut() {
                            stamp_recruit_defender(attempt, battle.recruit_id);
                        }
                        battle.stage = next;
                        battle.game.set_paused(false);
                        continue;
                    }
                }
            }
            let npc = battle.game.campaign.npc.clone();
            self.campaign.release_external(battle.loadout);
            if let Err(error) = self.strategy.release_recruit(battle.recruit_id, npc) {
                self.settlement_issues
                    .push(SettlementIssue::RecruitReleaseFailed {
                        battle: battle.id,
                        recruit: battle.recruit_id,
                        error,
                    });
            }
            battle.settled = true;
            // Completed reports retain the deployed loadout, not a second copy
            // of the entire shared stash or its presets for every past battle.
            battle.game.campaign.player.loadout = [1, 2, 3, 4];
            battle.game.campaign.presets.clear();
            battle.game.campaign.layout.clear();
            let keep = battle.game.campaign.npc.loadout;
            battle
                .game
                .campaign
                .items
                .retain(|item| item.id <= 4 || keep.contains(&item.id));
            battle.game.campaign.items.shrink_to_fit();
            battle.synced_item_index = battle.game.campaign.items.len();
        }
    }
}
