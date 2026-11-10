//! Attempt lifecycle: deployment, retry, withdrawal, settlement and repeat runs.

use super::*;
use crate::campaign::Npc;
use crate::defenders::DefenderBlocker;
use crate::strategy::RecruitStatus;
use tracing::warn;

impl Game {
    /// The recruit holding `seat` on this game's main run, or None for the
    /// mode owner. Inside a worker the seat names the expedition's recruit,
    /// whose npc is the worker's own `campaign.npc` copy, so this is None.
    fn seat_recruit(&self, seat: DefenderSeat) -> Option<u64> {
        match seat {
            DefenderSeat::Recruit(id) if !self.worker => Some(id),
            _ => None,
        }
    }
    /// The npc that fights from `seat` (ruling D6): Mara (or a worker's
    /// recruit copy) for the mode owner, and the roster entry for a recruit
    /// defending the main run. A seat naming no roster recruit (invalid
    /// state that save validation rejects) reads Mara's tactics.
    pub fn defender_npc(&self, seat: DefenderSeat) -> &Npc {
        self.seat_recruit(seat)
            .and_then(|id| self.strategy.roster.iter().find(|r| r.id == id))
            .map_or(&self.campaign.npc, |r| &r.npc)
    }
    /// The npc [`Self::defender_npc`] names, for crediting; None for a seat
    /// naming no roster recruit, which is never credited to Mara instead.
    pub(crate) fn defender_npc_mut(&mut self, seat: DefenderSeat) -> Option<&mut Npc> {
        match self.seat_recruit(seat) {
            None => Some(&mut self.campaign.npc),
            Some(id) => self
                .strategy
                .roster
                .iter_mut()
                .find(|r| r.id == id)
                .map(|r| &mut r.npc),
        }
    }
    /// Combat progress for the npc in `seat`: the mode owner's path is
    /// `Campaign::record_npc_progress` unchanged; a defending recruit's
    /// roster entry accrues the same and claims against the same allowance.
    pub(super) fn record_defender_progress(&mut self, seat: DefenderSeat, hits: u64, ticks: u64) {
        if self.seat_recruit(seat).is_none() {
            self.campaign.record_npc_progress(hits, ticks);
            return;
        }
        let allowance = self.campaign.allowance();
        if let Some(npc) = self.defender_npc_mut(seat) {
            npc.successful_hits = npc.successful_hits.saturating_add(hits);
            npc.combat_ticks = npc.combat_ticks.saturating_add(ticks);
            npc.claim_objectives(allowance);
        }
    }
    /// Why recruit `id` with `status` is away from camp, or None when Ready.
    pub(super) fn duty_blocker(&self, id: u64, status: RecruitStatus) -> Option<DefenderBlocker> {
        match status {
            RecruitStatus::Ready => None,
            RecruitStatus::Guarding => Some(DefenderBlocker::Guarding {
                country_id: self
                    .strategy
                    .guards
                    .iter()
                    .find(|g| g.recruit_id == id)
                    .map_or_else(String::new, |g| g.country_id.clone()),
            }),
            RecruitStatus::Deployed => Some(DefenderBlocker::OnExpedition {
                battle: self
                    .battles
                    .iter()
                    .find(|b| !b.settled && b.recruit_id == id)
                    .map_or(0, |b| b.id),
            }),
            RecruitStatus::Defending => Some(DefenderBlocker::Defending),
        }
    }
    /// The takeover preflight (ruling D8): recruit `id` can defend the main
    /// run now. The unknown, lock, duty and loadout checks are the selection
    /// verdict ([`Self::defender_selectable`]), so every entry point gives a
    /// recruit the same first reason. No mutation; returns the recruit's npc.
    fn takeover_preflight(&self, id: u64) -> Result<Npc, String> {
        if self.worker {
            return Err("An expedition cannot hand its defense to another survivor".into());
        }
        self.defender_selectable(DefenderId::Recruit(id))
            .map_err(|error| error.to_string())?;
        let recruit = self
            .strategy
            .roster
            .iter()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        self.campaign.preparation()?;
        Ok(recruit.npc.clone())
    }
    /// The loadout part of a recruit's verdict: `npc`'s items exist, fit
    /// their slots, are usable by a survivor defender and are not held by
    /// another deployment (the takeover commit's `check_external`). The main
    /// run's own hold is left out, so a defense in progress keeps the
    /// between-defenses verdict. No mutation.
    pub(super) fn recruit_loadout_check(&self, npc: &Npc) -> Result<(), String> {
        self.campaign.validate_loadout(npc.loadout)?;
        for item in npc.loadout {
            let item = self.campaign.item(item).ok_or("Missing equipment")?;
            self.campaign
                .weapon_access(Mode::Npc, item.definition_index())
                .map_err(str::to_owned)?;
        }
        let takeover = self.campaign.reserved_recruit.and_then(|id| {
            self.strategy
                .roster
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.npc.loadout)
        });
        if self.campaign.held_elsewhere(npc.loadout, takeover) {
            return Err(crate::campaign::ITEM_HELD_ELSEWHERE.into());
        }
        Ok(())
    }
    /// The takeover commit: the recruit's loadout and the main run are
    /// reserved and the recruit is Defending, or nothing changed.
    fn reserve_takeover(&mut self, id: u64, loadout: [u64; 4]) -> Result<(), String> {
        self.campaign.reserve_takeover(id, loadout)?;
        if let Err(error) = self.strategy.reserve_defender(id) {
            self.campaign.release_takeover(loadout);
            self.campaign.reserved = None;
            return Err(error);
        }
        self.unqueue_reserved(loadout);
        Ok(())
    }
    /// Returns the defending recruit `id` to the roster and frees its items.
    /// The roster entry is the battle's npc, so nothing is copied back.
    fn release_takeover(&mut self, id: u64) {
        let loadout = self
            .strategy
            .roster
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.npc.loadout);
        match loadout {
            Some(loadout) => self.campaign.release_takeover(loadout),
            None => self.campaign.reserved_recruit = None,
        }
        if let Err(error) = self.strategy.release_defender(id) {
            warn!(recruit = id, %error, "takeover release found no defending recruit");
        }
    }
    /// Deploys `defender` on catalogue `level` (`country_stage` for a country
    /// front). The mode owners deploy as they always have; a recruit passes
    /// the takeover preflight first and is committed in place of the mode
    /// owner's reservation (ruling D8).
    pub(super) fn deploy_as(
        &mut self,
        level: usize,
        defender: DefenderId,
        country_stage: Option<usize>,
    ) -> Result<(), String> {
        let mode = defender.mode();
        if self.attempt.is_some() {
            return Err("Finish or recall the active deployment first".into());
        }
        if level >= content().levels.len()
            || (country_stage.is_none() && level >= self.campaign.unlocked())
        {
            return Err("This stage is not unlocked".into());
        }
        let takeover = match defender.recruit() {
            Some(id) => Some((id, self.takeover_preflight(id)?)),
            None => None,
        };
        let loadout = match &takeover {
            Some((_, npc)) => npc.loadout,
            None if mode == Mode::Personal => self.campaign.player.loadout,
            None => self.campaign.npc.loadout,
        };
        self.campaign.validate_loadout(loadout)?;
        // The inputs defender_power reads (one shared helper): the mode
        // owners fight with Mara's npc record, as `Campaign::profile` always
        // has. The power number is the rank-only reading (no front
        // position), so it equals what deploys on stage 0 or without a
        // country; deploying passes the stage's position, so a deeper stage
        // also carries the front ramp.
        let character = match &takeover {
            Some((_, npc)) => npc,
            None => &self.campaign.npc,
        };
        let country = if country_stage.is_some() {
            self.campaign.world.active_country()
        } else {
            None
        };
        let super::power::DeploymentInputs {
            mut profile,
            snapshot: progression,
            balance,
            ..
        } = super::power::deployment_inputs_for(
            &self.campaign,
            defender,
            loadout,
            character,
            self.campaign.progression_rank(),
            // Axis 2: the stage's place on the country's front.
            country
                .zip(country_stage)
                .map(|(c, stage)| c.front_position(stage)),
        )?;
        let (lane_count, width_yards) = self.campaign.arena_geometry();
        let mut modifiers = country.map_or_else(Modifiers::default, |c| {
            // The start clamp (design-enemy-balance §8).
            let difficulty = c.effective_difficulty(&self.campaign.world);
            let mut m = c.modifiers;
            m.enemy_health *= difficulty;
            m.enemy_speed *= difficulty.sqrt();
            m.reward *= difficulty;
            m
        });
        modifiers.enemy_health *= progression.enemy_health * balance.enemy_health;
        modifiers.enemy_speed *= progression.enemy_speed * balance.enemy_speed;
        modifiers.reward *= progression.reward_multiplier * balance.reward;
        if encounter_health_bound(&content().levels[level], mode, &progression, &modifiers)
            > MAX_ENCOUNTER_HEALTH
        {
            return Err("Encounter health exceeds supported numerical safety bounds".into());
        }
        profile.personal.damage = (profile.personal.damage as f32 * modifiers.personal_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.melee_damage = (profile.personal.melee_damage as f32
            * modifiers.personal_damage)
            .round()
            .max(1.0) as i32;
        profile.ranged_skill += modifiers.npc_skill;
        profile.melee_skill += modifiers.npc_skill;
        let max_hp = if mode == Mode::Personal {
            profile.personal.health
        } else {
            profile.max_hp
        };
        let stamina = if mode == Mode::Personal {
            profile.personal.stamina as f32
        } else {
            profile.max_fp as f32
        };
        let magazine = if mode == Mode::Personal {
            profile.personal.magazine
        } else {
            profile.npc.magazine
        };
        let supply = (content().levels[level].supply as i32
            + modifiers.supply
            + progression.supply_bonus as i32
            + balance.supply)
            .max(0) as u32;
        let layout = country
            .map(|c| self.campaign.layout_for(&c.id).to_vec())
            .unwrap_or_default();
        match &takeover {
            Some((recruit, _)) => self.reserve_takeover(*recruit, loadout)?,
            None => self.campaign.reserve(mode)?,
        }
        let id = self.next_attempt_id;
        self.next_attempt_id += 1;
        let gear_snapshot = crate::content::Slot::ALL
            .into_iter()
            .map(|slot| {
                let item = self
                    .campaign
                    .item(loadout[slot.index()])
                    .expect("validated loadout");
                let definition = item.definition();
                crate::stats::GearRecord {
                    slot,
                    definition: item.definition.clone(),
                    name: definition.name.clone(),
                    level: item.level,
                    quality: self.campaign.item_quality(item.id).expect("owned"),
                    weapon_class: (slot == crate::content::Slot::Ranged)
                        .then_some(profile.weapon.class),
                }
            })
            .collect();
        let mut a = Attempt {
            width_yards,
            balance,
            dps: crate::stats::DpsTracker::new(),
            gear_snapshot,
            kill_xp: 0,
            loot_quality_counts: Default::default(),
            lane_count,
            loot_events: Vec::new(),
            progression,
            id,
            level,
            mode,
            seat: crate::defenders::DefenderSeat::ModeOwner,
            tick: 0,
            paused: false,
            country_id: country.map(|c| c.id.clone()),
            country_name: country.map_or_else(|| "Local defense".into(), |c| c.name.clone()),
            stage: country_stage.unwrap_or(level),
            modifiers,
            enemies: vec![],
            corpses: vec![],
            structures: vec![],
            layout_omissions: 0,
            hp: max_hp,
            max_hp,
            stamina,
            max_stamina: stamina,
            ammo: magazine,
            magazine,
            reload_left: 0,
            shot_cooldown: 0,
            melee_cooldown: 0,
            melee_commit: 0,
            supply,
            boss_spawned: false,
            kills: 0,
            coins: 0,
            loot: 0,
            drop_ids: Vec::new(),
            // Always Some from wave 2 (ruling E11): None marks an attempt
            // deployed before v7, which keeps the legacy drop path.
            drop_tier: Some(crate::loot::deploy_drop_tier(
                level,
                self.campaign.unlocked(),
            )),
            decision: "Ready to deploy".into(),
            events: VecDeque::new(),
            log: vec![],
            latest_resolution: None,
            last_damage_lane: None,
            ordnance: OrdnanceState::for_deployment(mode, &profile.modifiers),
            profile,
            rng: Rng::new(self.seed ^ id.wrapping_mul(0x9E3779B97F4A7C15)),
            injury: InjuryState::new(max_hp, stamina as i32),
            reward_ledger: BTreeSet::new(),
            first_contact: None,
            shots: 0,
            hits: 0,
            active_damage: 0,
            structure_damage: 0,
            reload_ticks: 0,
            aim_ticks: 0,
            next_id: 1,
            telemetry: CombatTelemetry {
                damage_by_lane: vec![0; usize::from(lane_count)],
                ..Default::default()
            },
            maneuver: NpcManeuver::DoNothing,
            decision_target: None,
            defense_bonus: 0,
            spawn_credit: 0.0,
            lane_history: vec![],
            aim_target: None,
            aim_seconds: 0,
            ready_turns: 0,
            npc_holding_melee: false,
            heal_used: false,
            statuses: Statuses::default(),
            caller_tick: 0,
            boss_warning_sent: false,
            caller_lanes: None,
            event_seq: self.next_event_seq,
            emitted: Vec::new(),
            burst_victims: Default::default(),
        };
        if let Some((recruit, _)) = takeover {
            super::parallel::stamp_recruit_defender(&mut a, recruit);
        }
        info_span!("battle", attempt = id, mode = ?mode, seed = self.seed).in_scope(|| {
            info!(
                worker = self.worker,
                country = ?a.country_id,
                stage = a.stage,
                level,
                "attempt started"
            );
        });
        self.last_result = None;
        self.attempt = Some(a);
        if !self.worker {
            self.campaign.journey_active = true;
            self.campaign.journey_revision = 1;
        }
        self.global_paused = false;
        for (index, p) in layout.iter().enumerate() {
            if let Err(error) = self.place(p.kind, p.lane as u8, p.column as u8) {
                let a = self.attempt.as_mut().expect("new attempt");
                a.layout_omissions = layout.len() - index;
                a.note(format!(
                    "Saved country plan: {} placements omitted ({error}); full plan preserved.",
                    a.layout_omissions
                ));
                break;
            }
        }
        Ok(())
    }
    pub fn retry(&mut self) -> Result<(), String> {
        let r = self.last_result.clone().ok_or("No previous deployment")?;
        self.deploy_report(&r)
    }
    /// Deploys the reported run again with the defender who fought it
    /// (retry and repeats; the chosen next defender is not read).
    fn deploy_report(&mut self, r: &Report) -> Result<(), String> {
        if let Some(country) = &r.country_id {
            self.campaign.enter_country(country)?;
            self.deploy_country_stage_as(r.stage, r.defender())
        } else {
            self.deploy_as(r.level, r.defender(), None)
        }
    }
    pub fn withdraw(&mut self) {
        if let Some(a) = self.attempt.take() {
            self.finish(
                a,
                Outcome::Withdrawal,
                "Recalled to camp with all banked rewards".into(),
            );
        }
    }
    pub(super) fn finish(&mut self, mut a: Attempt, outcome: Outcome, cause: String) {
        info_span!("battle", attempt = a.id, mode = ?a.mode, seed = self.seed).in_scope(|| {
            info!(
                outcome = ?outcome,
                ticks = a.tick,
                kills = a.kills,
                %cause,
                "attempt finished"
            );
        });
        self.finish_events(&mut a, outcome, &cause);
        if !self.worker {
            self.repeat_remaining = 0;
        }
        let completion_xp = if outcome == Outcome::Victory {
            self.campaign.completion_xp_preview()
        } else {
            0
        };
        // Finish order for a takeover (ruling D8): victory credit, the npc
        // attempts count, the takeover release, the main-run auto-sell (H1,
        // ruling E2), the progression settlement (grants and milestones,
        // ruling E2), then the run record and the report, both naming the
        // recruit.
        let takeover = if a.mode == Mode::Npc {
            self.seat_recruit(a.seat)
        } else {
            None
        };
        let first_clear = outcome == Outcome::Victory
            && match takeover {
                None => self.campaign.first_clear(a.level, a.mode),
                Some(id) => {
                    let first = self.campaign.commit_victory(a.level);
                    if let Some(r) = self.strategy.roster.iter_mut().find(|r| r.id == id) {
                        self.campaign.credit_npc_victory(&mut r.npc, a.level);
                    }
                    first
                }
            };
        let country_freed = if outcome == Outcome::Victory && a.country_id.is_some() {
            self.campaign
                .record_country_stage_clear(a.stage)
                .unwrap_or(false)
        } else {
            false
        };
        if outcome == Outcome::Victory
            && let Some(country) = &a.country_id
        {
            self.strategy.resolve_outbreak(country);
        }
        // An npc run (the main run reserved for Npc) counts one attempt on
        // the npc that fought it: Mara, a worker's recruit copy or the
        // defending recruit's roster entry.
        let npc_run = self.campaign.reserved == Some(Mode::Npc);
        self.campaign.finish_attempt();
        if npc_run && let Some(npc) = self.defender_npc_mut(a.seat) {
            npc.attempts += 1;
        }
        if let Some(id) = takeover {
            self.release_takeover(id);
        }
        // Every drop the run recorded (root attempts on the drop_tier Some
        // path only; empty for a legacy or expedition attempt). H1 sells
        // from them, judged for the defender who ran the battle.
        let drops = std::mem::take(&mut a.drop_ids);
        let auto_sold = self.settle_auto_sell(&drops, a.defender());
        // Progression settlement (W2-C, ruling E2): after H1, so auto-sell
        // never sees a milestone item; nothing in a worker.
        let progression_rewards = self.settle_progression_rewards();
        let record = crate::stats::RunRecord {
            id: 0,
            attempt_id: a.id,
            worker_id: None,
            recruit_id: takeover,
            outcome,
            mode: a.mode,
            country_id: a.country_id.clone(),
            country_name: a.country_name.clone(),
            defender_name: if a.mode == Mode::Npc {
                self.defender_npc(a.seat).name.clone()
            } else {
                self.campaign
                    .selected_survivor
                    .unwrap_or(crate::survivors::SurvivorId::Rook)
                    .name()
                    .into()
            },
            cause: cause.clone(),
            stage: a.stage,
            template: a.level,
            survivor: if a.mode == Mode::Personal {
                Some(
                    self.campaign
                        .selected_survivor
                        .unwrap_or(crate::survivors::SurvivorId::Rook),
                )
            } else {
                None
            },
            lane_count: a.lane_count,
            width_yards: a.width_yards,
            threat_rank: a.progression.rank,
            balance: a.balance.clone(),
            gear: a.gear_snapshot.clone(),
            duration_ticks: a.tick,
            kills: a.kills,
            shots: a.shots,
            hits: a.hits,
            misses: a.telemetry.misses,
            loot: a.loot,
            damage_by_source: a.dps.totals,
            average_dps: a.average_dps(),
            peak_dps: a.peak_dps(),
            coins: a.coins,
            kill_xp: a.kill_xp,
            completion_xp,
            loot_by_quality: a.loot_quality_counts.clone(),
            end_hp: a.hp,
            max_hp: a.max_hp,
            legacy: a.gear_snapshot.is_empty(),
            auto_sold_count: auto_sold.count,
            auto_sold_coins: auto_sold.coins,
            level_grant_coins: progression_rewards.level_coins,
            milestone_coins: progression_rewards.milestone_coins(),
            // The record's bounds: elite kills at most `kills`, the boss
            // time-to-kill at most `duration_ticks`.
            elite_kills: a.telemetry.elites_killed.min(a.kills),
            boss_ttk_ticks: a
                .telemetry
                .boss_arrival_tick
                .zip(a.telemetry.boss_defeat_tick)
                .map(|(arrived, fell)| fell.saturating_sub(arrived).min(a.tick)),
            boss_rank: a.progression.boss_rank,
        };
        if self.worker {
            self.pending_runs.push(record);
        } else {
            self.campaign.append_run(record);
        }
        self.last_result = Some(Report {
            attempt_id: a.id,
            width_yards: a.width_yards,
            lane_count: a.lane_count,
            completion_xp,
            loot_events: a.loot_events.clone(),
            outcome,
            level: a.level,
            mode: a.mode,
            seat: a.seat,
            elapsed: a.elapsed(),
            kills: a.kills,
            country_id: a.country_id.clone(),
            country_name: a.country_name.clone(),
            stage: a.stage,
            country_freed,
            coins: a.coins,
            loot: a.loot,
            // Every drop, sold ones included, and the run's quality counts.
            drop_ids: drops,
            loot_by_quality: a.loot_quality_counts.clone(),
            auto_sold,
            progression_rewards,
            boss_rank: a.progression.boss_rank,
            cause,
            tip: if outcome == Outcome::Victory {
                "Spend retained rewards, review your loadout, then secure the next sector.".into()
            } else if a.mode == Mode::Npc {
                "Train ranged skill or buy a reliable rifle; review attack rolls and protection in the combat log.".into()
            } else if a.reload_ticks > a.tick / 4 {
                "Coordination and a larger magazine reduce dangerous reload windows.".into()
            } else {
                "Aim at the nearest runner first. Melee answers enemies at any contact lane. Vitality and armor buy recovery time.".into()
            },
            first_clear,
            shots: a.shots,
            hits: a.hits,
            first_contact: a.first_contact,
            active_damage: a.active_damage,
            structure_damage: a.structure_damage,
            log: a.log,
            telemetry: a.telemetry,
        });
        if outcome != Outcome::Victory || first_clear || country_freed {
            self.repeat_remaining = 0;
        }
    }
    pub fn start_repeat(&mut self, count: u32) -> Result<(), String> {
        if count == 0 || count > 10 {
            return Err("Repeat count must be between one and ten".into());
        }
        let report = self
            .last_result
            .clone()
            .ok_or("Complete a survivor deployment first")?;
        if report.mode != Mode::Npc || report.outcome != Outcome::Victory {
            return Err("Repeats require a cleared survivor deployment".into());
        }
        self.deploy_report(&report)?;
        self.repeat_remaining = count;
        self.set_paused(false);
        Ok(())
    }
    pub fn continue_repeat(&mut self) -> Result<bool, String> {
        if self.attempt.is_some() || self.repeat_remaining == 0 {
            return Ok(false);
        }
        let r = self
            .last_result
            .clone()
            .ok_or("Missing deployment result")?;
        if r.outcome != Outcome::Victory || r.first_clear {
            self.repeat_remaining = 0;
            return Ok(false);
        }
        self.repeat_remaining -= 1;
        if self.repeat_remaining == 0 {
            return Ok(false);
        }
        if let Err(error) = self.deploy_report(&r) {
            self.repeat_remaining = 0;
            return Err(error);
        }
        self.set_paused(false);
        Ok(true)
    }
}
