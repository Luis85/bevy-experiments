//! Multi-expedition orchestration and monotonic shared-reward settlement.
use super::*;

/// Single source for the unknown-expedition message shared with `command`.
pub(crate) const UNKNOWN_EXPEDITION: &str = "No active expedition with that ID";
/// The most battles, live and settled, a game keeps (save validation bound).
pub(crate) const MAX_BATTLES: usize = 32;

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
    /// The settled expeditions the player has not acknowledged, oldest first,
    /// and the count pruning folded (Ruling D10). Battles settled before save
    /// version 7 carry no summary and are never announced. Pure: no RNG, no
    /// mutation.
    pub fn unacknowledged_outcomes(&self) -> OutcomeNotices {
        let notices = self
            .battles
            .iter()
            .filter(|b| b.settled)
            .filter_map(|b| {
                let summary = b.summary.as_ref().filter(|s| !s.acknowledged)?;
                Some(ExpeditionNotice {
                    battle: b.id,
                    recruit_id: b.recruit_id,
                    recruit_name: b.game.campaign.npc.name.clone(),
                    country_id: b.country_id.clone(),
                    country_name: crate::world::world()
                        .country(&b.country_id)
                        .map_or_else(|| b.country_id.clone(), |c| c.name.clone()),
                    summary: summary.clone(),
                })
            })
            .collect();
        OutcomeNotices {
            notices,
            folded: self.strategy.folded_outcomes,
        }
    }
    /// `Command::AcknowledgeOutcomes`: marks the named settled expeditions'
    /// notices seen and takes `folded` off the folded count (saturating).
    /// Unknown, live, legacy and already acknowledged ids are ignored.
    pub(crate) fn acknowledge_outcomes(&mut self, battles: &[u64], folded: u32) {
        for b in self.battles.iter_mut().filter(|b| b.settled) {
            if let Some(summary) = b.summary.as_mut().filter(|_| battles.contains(&b.id)) {
                summary.acknowledged = true;
            }
        }
        self.strategy.folded_outcomes = self.strategy.folded_outcomes.saturating_sub(folded);
    }
    /// Keeps the battle list at its cap by dropping settled battles, oldest
    /// first: a legacy (summary-less) or acknowledged one when there is one,
    /// otherwise an unacknowledged one, whose notice is folded into
    /// `folded_outcomes` so no outcome goes uncounted. Live battles stay.
    fn prune_settled_battles(&mut self) {
        while self.battles.len() > MAX_BATTLES {
            let quiet = self
                .battles
                .iter()
                .position(|b| b.settled && b.summary.as_ref().is_none_or(|s| s.acknowledged));
            let Some(n) = quiet.or_else(|| self.battles.iter().position(|b| b.settled)) else {
                break;
            };
            if self
                .battles
                .remove(n)
                .summary
                .is_some_and(|s| !s.acknowledged)
            {
                self.strategy.folded_outcomes = self.strategy.folded_outcomes.saturating_add(1);
            }
        }
    }
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
        self.unqueue_reserved(npc.loadout);
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
        self.prune_settled_battles();
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
    /// Keeps schema v7 §3.6's "`pending_auto_sell` disjoint from
    /// `reserved_items`": a queued drop the player equips on a recruit and
    /// sends on duty (guard, expedition or main-run takeover) leaves every
    /// queue once that `loadout` is reserved. A sale guard would keep it
    /// anyway, and it is not queued again on release, like any rule change
    /// (never retroactive). Call after the reservation is committed.
    pub(crate) fn unqueue_reserved(&mut self, loadout: [u64; 4]) {
        for battle in self.battles.iter_mut().filter(|b| !b.settled) {
            battle.pending_auto_sell.retain(|id| !loadout.contains(id));
        }
    }
    pub fn recall_survivor(&mut self, battle_id: u64) -> Result<(), String> {
        let legacy = self.legacy_expeditions();
        let battle = self
            .active_battle_mut(battle_id)
            .ok_or_else(|| UNKNOWN_EXPEDITION.to_string())?;
        battle.game.withdraw();
        self.forward_expedition_events();
        self.settle_parallel(&legacy);
        Ok(())
    }
    pub(super) fn tick_parallel(&mut self) {
        let legacy = self.legacy_expeditions();
        for battle in &mut self.battles {
            if !battle.settled {
                battle.game.tick(Input::default());
            }
        }
        self.forward_expedition_events();
        self.settle_parallel(&legacy);
    }
    /// The live expeditions whose current attempt was deployed before v7
    /// (no drop tier, ruling E11): settlement never queues their drops for
    /// auto-sale. Read before the workers step, because an attempt that ends
    /// in that step is gone when settlement stores its last drops.
    fn legacy_expeditions(&self) -> Vec<u64> {
        self.battles
            .iter()
            .filter(|b| {
                !b.settled
                    && b.game
                        .attempt
                        .as_ref()
                        .is_some_and(|a| a.drop_tier.is_none())
            })
            .map(|b| b.id)
            .collect()
    }
    /// Commits each live expedition's new coins, loot and run records to the
    /// parent. Every stored drop is queued for auto-sale (hook W-a) unless the
    /// battle is in `legacy`; the queue is drained at the pass's last run
    /// record and by a final sweep before the battle settles.
    ///
    /// A pass that credits an expedition victory ends with the progression
    /// settlement (W2-C, ruling E2) when no main attempt is live; otherwise
    /// the main run's finish settles what the victory raised. Its receipt goes
    /// on the last credited battle's report and on the run record the pass
    /// appended for it.
    fn settle_parallel(&mut self, legacy: &[u64]) {
        // The last victory this pass credited: (battle, its record's attempt
        // id, or None on the record-less path).
        let mut credited: Option<(u64, Option<u64>)> = None;
        // The country operations this pass redeployed: their op-chain sync
        // ran before the settlement below, so it is refreshed after it.
        let mut redeployed = Vec::new();
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
            let mut stored = 0u32;
            let queue = !legacy.contains(&battle.id);
            for item in battle.game.campaign.items[battle.synced_item_index..]
                .iter()
                .filter(|item| item.id >= battle.synced_item_id)
            {
                match self.campaign.add_item_with_quality(
                    item.definition_index(),
                    item.level,
                    item.quality,
                ) {
                    Ok(id) => {
                        stored = stored.saturating_add(1);
                        if queue {
                            battle.pending_auto_sell.push(id);
                        }
                    }
                    Err(error) => self.settlement_issues.push(SettlementIssue::LootLost {
                        battle: battle.id,
                        definition: item.definition.clone(),
                        level: item.level,
                        error,
                    }),
                }
            }
            battle.synced_item_id = battle.game.campaign.next_item_id;
            battle.synced_item_index = battle.game.campaign.items.len();
            // The saved summary counts what this pass committed, so an
            // operation settled over several passes or a save/load is counted
            // once (Ruling D10). A v6 in-flight battle starts counting here.
            if coins > 0 || stored > 0 {
                let summary = battle.summary.get_or_insert_with(Default::default);
                summary.coins = summary.coins.saturating_add(coins);
                summary.loot_count = summary.loot_count.saturating_add(stored);
            }
            let had_pending = !battle.game.pending_runs.is_empty();
            let runs = std::mem::take(&mut battle.game.pending_runs);
            let last_run = runs.len().checked_sub(1);
            for (n, mut record) in runs.into_iter().enumerate() {
                if record.outcome == Outcome::Victory {
                    record.completion_xp = self.campaign.completion_xp_preview();
                    self.campaign.first_clear(record.template, Mode::Personal);
                    credited = Some((battle.id, Some(record.attempt_id)));
                    let summary = battle.summary.get_or_insert_with(Default::default);
                    summary.stages_cleared = summary.stages_cleared.saturating_add(1);
                    if let Some(country) = &record.country_id {
                        match self
                            .campaign
                            .record_country_stage_clear_in(country, record.stage)
                        {
                            Ok(freed) => summary.liberated |= freed,
                            Err(error) => {
                                self.settlement_issues
                                    .push(SettlementIssue::StageClearRejected {
                                        battle: battle.id,
                                        country: country.clone(),
                                        stage: record.stage,
                                        error,
                                    });
                            }
                        }
                        self.strategy.resolve_outbreak(country);
                    }
                }
                // Hook W-a: the pass's last record drains the queue, so
                // each stage's drops are sold once, on its own record.
                let auto_sold = if Some(n) == last_run {
                    auto_sell::drain_expedition_sale(&mut self.campaign, &self.strategy, battle)
                } else {
                    Default::default()
                };
                record.auto_sold_count = auto_sold.count;
                record.auto_sold_coins = auto_sold.coins;
                if let Some(report) = &mut battle.game.last_result
                    && report.attempt_id == record.attempt_id
                {
                    report.completion_xp = record.completion_xp;
                    report.auto_sold = auto_sold;
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
                // The record-less path (a worker report without a pending
                // run) credits only a victory whose record is not already in
                // the history: a second pass over the same report is a no-op.
                let committed = !had_pending
                    && self.campaign.run_history.iter().any(|r| {
                        r.worker_id == Some(battle.id) && r.attempt_id == report.attempt_id
                    });
                if !had_pending && !committed {
                    report.completion_xp = self.campaign.completion_xp_preview();
                    self.campaign.first_clear(report.level, Mode::Personal);
                    credited = Some((battle.id, None));
                    let summary = battle.summary.get_or_insert_with(Default::default);
                    summary.stages_cleared = summary.stages_cleared.saturating_add(1);
                    match self
                        .campaign
                        .record_country_stage_clear_in(&battle.country_id, battle.stage)
                    {
                        Ok(freed) => summary.liberated |= freed,
                        Err(error) => {
                            self.settlement_issues
                                .push(SettlementIssue::StageClearRejected {
                                    battle: battle.id,
                                    country: battle.country_id.clone(),
                                    stage: battle.stage,
                                    error,
                                });
                        }
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
                        redeployed.push(battle.id);
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
            // The final sweep (hook W-a): whatever no record drained, such as
            // the record-less legacy path's drops, sells under the same gate
            // and its receipt goes to the worker's report. A settled battle
            // holds no queue.
            let swept =
                auto_sell::drain_expedition_sale(&mut self.campaign, &self.strategy, battle);
            if !swept.is_empty()
                && let Some(report) = &mut battle.game.last_result
            {
                report.auto_sold.absorb(swept);
            }
            battle.settled = true;
            // The single settled transition finalises the notice: the final
            // outcome and, for a liberation, the fronts it opened.
            let summary = battle.summary.get_or_insert_with(Default::default);
            summary.outcome = battle.game.last_result.as_ref().map(|r| r.outcome);
            if summary.liberated {
                summary.fronts = self
                    .campaign
                    .world
                    .fronts_opened_by(&battle.country_id)
                    .into_iter()
                    .take(ExpeditionSummary::MAX_FRONTS)
                    .map(|c| c.id.clone())
                    .collect();
            }
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
        if self.attempt.is_none()
            && let Some((battle, attempt)) = credited
        {
            self.stamp_expedition_rewards(battle, attempt);
        }
        // Ruling E1: the op-chain sync copies both ledgers as this pass's
        // settlement left them, so a continuing worker never lags a claim or
        // a grant the pass just settled. The worker keeps the level and rank
        // it was synced and armed at; a later battle of the same pass may
        // have raised the parent's past them, so the copy stops at the
        // worker's own level and rank (a ledger above them is refused by
        // save validation). Its next stage victory re-syncs it in full.
        for battle in self
            .battles
            .iter_mut()
            .filter(|b| redeployed.contains(&b.id))
        {
            let worker = &mut battle.game.campaign;
            worker.player.level_grant_paid = self
                .campaign
                .player
                .level_grant_paid
                .min(worker.player.level);
            let rank = worker.progression_rank();
            worker.milestones_claimed = self
                .campaign
                .milestones_claimed
                .range(..=rank)
                .copied()
                .collect();
        }
    }
    /// Settles progression at the end of a pass that credited battle
    /// `battle`'s victory and stamps a non-empty receipt on that battle's
    /// report (when it is still the credited one) and on its run record
    /// `attempt` (None on the record-less path). The record-less path appends
    /// no record, so when that battle is a country operation that redeployed
    /// its next stage in the same pass, its report is gone too and the
    /// receipt is dropped: the coins, the item and the ledgers still land.
    fn stamp_expedition_rewards(&mut self, battle: u64, attempt: Option<u64>) {
        let rewards = self.settle_progression_rewards();
        if rewards.is_empty() {
            return;
        }
        if let Some(attempt) = attempt
            && let Some(record) = std::sync::Arc::make_mut(&mut self.campaign.run_history)
                .iter_mut()
                .rev()
                .find(|r| r.worker_id == Some(battle) && r.attempt_id == attempt)
        {
            record.level_grant_coins = record.level_grant_coins.saturating_add(rewards.level_coins);
            record.milestone_coins = record
                .milestone_coins
                .saturating_add(rewards.milestone_coins());
        }
        if let Some(report) = self
            .battles
            .iter_mut()
            .find(|b| b.id == battle)
            .and_then(|b| b.game.last_result.as_mut())
            .filter(|report| attempt.is_none_or(|attempt| report.attempt_id == attempt))
        {
            report.progression_rewards.absorb(rewards);
        }
    }
}
