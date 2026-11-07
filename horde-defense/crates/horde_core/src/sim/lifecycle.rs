//! Attempt lifecycle: deployment, retry, withdrawal, settlement and repeat runs.

use super::*;

impl Game {
    pub(super) fn deploy_internal(
        &mut self,
        level: usize,
        mode: Mode,
        country_stage: Option<usize>,
    ) -> Result<(), String> {
        if self.attempt.is_some() {
            return Err("Finish or recall the active deployment first".into());
        }
        if level >= content().levels.len()
            || (country_stage.is_none() && level >= self.campaign.unlocked())
        {
            return Err("This stage is not unlocked".into());
        }
        self.campaign.validate_loadout(if mode == Mode::Personal {
            self.campaign.player.loadout
        } else {
            self.campaign.npc.loadout
        })?;
        let mut profile = self.campaign.profile(mode);
        let country = if country_stage.is_some() {
            self.campaign.world.active_country()
        } else {
            None
        };
        let (lane_count, width_yards) = self.campaign.arena_geometry();
        let balance = self.campaign.balance_settings.clone();
        let progression = self.campaign.deployment_progression(mode)?;
        let mut modifiers = country.map_or_else(Modifiers::default, |c| {
            let mut m = c.modifiers;
            m.enemy_health *= c.difficulty;
            m.enemy_speed *= c.difficulty.sqrt();
            m.reward *= c.difficulty;
            m
        });
        modifiers.enemy_health *= progression.enemy_health * balance.enemy_health;
        modifiers.enemy_speed *= progression.enemy_speed * balance.enemy_speed;
        modifiers.reward *= progression.reward_multiplier * balance.reward;
        profile.personal.damage = (profile.personal.damage as f32 * balance.player_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.melee_damage = (profile.personal.melee_damage as f32
            * balance.player_damage)
            .round()
            .max(1.0) as i32;
        profile.personal.health = (profile.personal.health as f32 * balance.defender_health)
            .round()
            .max(1.0) as i32;
        profile.max_hp = (profile.max_hp as f32 * balance.defender_health)
            .round()
            .max(1.0) as i32;
        let expected = crate::campaign::mean_basic_damage(Damage {
            dice: profile.npc.damage_dice,
            adds: profile.npc.damage_add,
            kind: profile.npc.damage_kind,
        });
        profile.npc.damage_add = (profile.npc.damage_add as f32
            + expected * (balance.player_damage - 1.0))
            .round()
            .clamp(-30.0, 1_000_000.0) as i32;
        let expected = crate::campaign::mean_basic_damage(Damage {
            dice: profile.npc_melee_damage_dice,
            adds: profile.npc_melee_damage_add,
            kind: profile.npc_melee_kind,
        });
        profile.npc_melee_damage_add = (profile.npc_melee_damage_add as f32
            + expected * (balance.player_damage - 1.0))
            .round()
            .clamp(-30.0, 1_000_000.0) as i32;
        let boss_base = if mode == Mode::Npc {
            content().levels[level]
                .boss_npc_hp
                .max(progression.npc_boss_health_floor)
        } else {
            content().levels[level]
                .boss_health
                .max(progression.boss_health_floor)
        };
        let maximum_base =
            boss_base.max(crate::enemy_catalog::enemy_catalog().maximum_health(mode));
        if maximum_base as f32 * modifiers.enemy_health > 10_000_000.0 {
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
        self.campaign.reserve(mode)?;
        let id = self.next_attempt_id;
        self.next_attempt_id += 1;
        let loadout = if mode == Mode::Personal {
            self.campaign.player.loadout
        } else {
            self.campaign.npc.loadout
        };
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
        let a = Attempt {
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
        };
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
    fn deploy_report(&mut self, r: &Report) -> Result<(), String> {
        if let Some(country) = &r.country_id {
            self.campaign.enter_country(country)?;
            self.deploy_country_stage(r.stage, r.mode)
        } else {
            self.deploy(r.level, r.mode)
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
        let first_clear = outcome == Outcome::Victory && self.campaign.first_clear(a.level, a.mode);
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
        self.campaign.finish_attempt();
        let record = crate::stats::RunRecord {
            id: 0,
            attempt_id: a.id,
            worker_id: None,
            recruit_id: None,
            outcome,
            mode: a.mode,
            country_id: a.country_id.clone(),
            country_name: a.country_name.clone(),
            defender_name: if a.mode == Mode::Npc {
                self.campaign.npc.name.clone()
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
            auto_sold_count: 0,
            auto_sold_coins: 0,
            level_grant_coins: 0,
            milestone_coins: 0,
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
            // Every drop the run recorded (root attempts on the drop_tier
            // Some path only; empty for a legacy or expedition attempt) and
            // the run's quality counts.
            drop_ids: std::mem::take(&mut a.drop_ids),
            loot_by_quality: a.loot_quality_counts.clone(),
            auto_sold: Default::default(),
            progression_rewards: Default::default(),
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
