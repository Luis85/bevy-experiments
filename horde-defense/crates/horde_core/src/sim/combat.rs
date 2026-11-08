//! Enemy attack resolution, damage application and corpse reaping.

use super::*;

/// The catalogue prefix a pre-v7 attempt's drop pool reads: the 29 v1
/// definitions. The loot-only items appended at 29-67 (W2-08) and any later
/// appends only reach attempts deployed from v7 on (`drop_tier` Some).
const LEGACY_DROP_DEFINITIONS: usize = 29;

/// Hazard (blast and spit) damage `raw` after the survivor's
/// `hazard_damage_multiplier`, applied before armor and Brace. Personal
/// defenders only, like the class-perk and penetration folds: Mara and
/// recruits (NPC mode) take `raw`. The neutral 1.0 returns `raw` untouched
/// (no float round trip).
// The burst and reach-attack paths (enemy-balance, W2-29/W2-30) are the
// consumers; until they land only the tests read it.
#[cfg_attr(not(test), expect(dead_code, reason = "consumed from W2-29"))]
pub(crate) fn hazard_scaled(a: &Attempt, raw: i32) -> i32 {
    let m = a.profile.modifiers.hazard_damage_multiplier;
    if a.mode != Mode::Personal || m == 1.0 {
        raw
    } else {
        (raw as f32 * m).round() as i32
    }
}

impl Game {
    pub(super) fn enemy_attacks(&mut self, a: &mut Attempt) {
        for n in 0..a.enemies.len() {
            let e = &a.enemies[n];
            if (a.mode == Mode::Personal && e.hp <= 0)
                || (a.mode == Mode::Npc && e.injury.terminal())
            {
                continue;
            }
            if a.enemies[n].contact_since.is_some_and(|tick| a.tick > tick) {
                a.enemies[n].attack_left = a.enemies[n].attack_left.saturating_sub(1);
                if a.enemies[n].attack_phase == EnemyPhase::Recovery
                    && a.enemies[n].attack_left == 0
                {
                    let duration = seconds(enemy_profile(&a.enemies[n]).windup);
                    a.enemies[n].attack_phase = EnemyPhase::Windup;
                    a.enemies[n].attack_left = duration;
                    a.enemies[n].windup_total = duration;
                    let e = &a.enemies[n];
                    let point = [e.x, e.y()];
                    let meta = EventMetadata {
                        actor_id: Some(e.id),
                        target_id: e.target_structure,
                        lane: Some(e.lane),
                        duration_ticks: duration,
                        ..Default::default()
                    };
                    a.event_with(
                        EventKind::Windup,
                        point,
                        if meta.target_id.is_none() {
                            a.defender_position()
                        } else {
                            point
                        },
                        0,
                        meta,
                    );
                }
            }
            let mut turn_shock = 0;
            if a.mode == Mode::Npc && a.tick.is_multiple_of(60) {
                turn_shock = a.enemies[n].injury.shock;
                a.enemies[n].injury.clear_shock();
                let ht = enemy_ht(a, &a.enemies[n]);
                if !a.enemies[n].injury.start_turn(&mut a.rng, ht) {
                    if a.enemies[n].boss {
                        break;
                    }
                    continue;
                }
                if a.enemies[n].injury.stunned {
                    a.enemies[n].injury.recover_stun(&mut a.rng, ht);
                    continue;
                }
                if a.enemies[n].injury.prone {
                    a.enemies[n].injury.prone = false;
                    continue;
                }
                // Hostiles use natural strikes; knockdown cannot drop a claw.
                a.enemies[n].injury.weapon_dropped = false;
                if a.enemies[n].ready_turns > 0 {
                    a.enemies[n].ready_turns -= 1;
                    continue;
                }
            }
            let e = &a.enemies[n];
            let Some(contact_tick) = e.contact_since else {
                if a.mode == Mode::Npc && a.tick.is_multiple_of(60) {
                    a.enemies[n].injury.clear_shock();
                }
                continue;
            };
            if a.tick <= contact_tick {
                continue;
            }
            if a.enemies[n].attack_left > 0 {
                continue;
            }
            if a.mode == Mode::Npc && !a.tick.is_multiple_of(60) {
                continue;
            }
            let e = &a.enemies[n];
            let d = enemy_profile(e).clone();
            let origin = [e.x, e.y()];
            let lane = e.lane;
            let structure = e
                .target_structure
                .and_then(|id| a.structures.iter().position(|s| s.id == id));
            let actor_id = e.id;
            let target_id = e.target_structure;
            let meta = EventMetadata {
                actor_id: Some(actor_id),
                target_id,
                lane: Some(lane),
                ..Default::default()
            };
            let mut damage = if e.boss {
                content().levels[a.level].boss_damage
            } else {
                d.damage
            };
            damage = (damage as f32 * a.progression.enemy_damage * a.balance.enemy_damage).round()
                as i32;
            if structure.is_some() {
                damage *= if e.boss && content().levels[a.level].boss == BossKind::SiegeBrute {
                    2
                } else {
                    i32::from(d.structure_damage_multiplier)
                };
            }
            let siege = e.boss && content().levels[a.level].boss == BossKind::SiegeBrute;
            a.enemies[n].attack_phase = if siege {
                EnemyPhase::Recovery
            } else {
                EnemyPhase::Windup
            };
            a.enemies[n].attack_left = if siege {
                seconds(content().levels[a.level].boss_timing.siege_recovery)
            } else if a.mode == Mode::Personal {
                seconds(d.windup)
            } else {
                60
            };
            a.enemies[n].windup_total = a.enemies[n].attack_left;
            if !siege {
                a.event_with(
                    EventKind::Windup,
                    origin,
                    if target_id.is_none() {
                        a.defender_position()
                    } else {
                        origin
                    },
                    0,
                    EventMetadata {
                        duration_ticks: a.enemies[n].attack_left,
                        ..meta
                    },
                );
            }
            if structure.is_none() {
                a.telemetry.contact_attacks += 1;
            }
            if a.mode == Mode::Npc {
                let l = &content().levels[a.level];
                let boss = a.enemies[n].boss;
                let demolition = if structure.is_some() {
                    if boss && l.boss == BossKind::SiegeBrute {
                        2
                    } else {
                        d.structure_damage_multiplier
                    }
                } else {
                    1
                };
                let mut attack = Attack {
                    skill: if boss { l.boss_npc_skill } else { d.npc_skill } - turn_shock,
                    damage: Damage {
                        dice: (if boss {
                            l.boss_npc_damage_dice
                        } else {
                            d.npc_damage_dice
                        })
                        .saturating_mul(demolition),
                        adds: (if boss {
                            l.boss_npc_damage_add
                        } else {
                            d.npc_damage_add
                        }) * i32::from(demolition)
                            + ((a.progression.enemy_damage - 1.0) * 6.0).floor() as i32,
                        kind: DamageKind::Crushing,
                    },
                    armor_divisor: 1,
                };
                let expected = crate::campaign::mean_basic_damage(attack.damage);
                attack.damage.adds = (attack.damage.adds as f32
                    + expected * (a.balance.enemy_damage - 1.0))
                    .round()
                    .clamp(-30.0, 1_000_000.0) as i32;
                if let Some(s) = structure {
                    let protection = content().structure(a.structures[s].kind).npc.armor;
                    let result = rules::resolve_object_attack(&mut a.rng, attack, protection);
                    if result.requires_ready {
                        a.enemies[n].ready_turns = 1;
                    }
                    damage = result.injury * 5;
                    a.structures[s].hp -= damage;
                    let mut feedback = result.clone();
                    feedback.injury = damage;
                    a.resolution_event(
                        &feedback,
                        origin,
                        [a.structures[s].x(), a.structures[s].y()],
                        meta,
                        false,
                    );
                    a.resolved(
                        Some(actor_id),
                        target_id,
                        &result,
                        format!(
                            "{} → structure in lane {}: {}",
                            d.name,
                            lane + 1,
                            result.explanation()
                        ),
                    );
                } else {
                    let dodge = a
                        .injury
                        .dodge((a.profile.stats.dx + a.profile.stats.ht) / 4 + 3)
                        + a.defense_bonus;
                    let result = rules::resolve_attack(
                        &mut a.rng,
                        attack,
                        Some(dodge),
                        a.profile.npc.armor,
                        &mut a.injury,
                        a.profile.stats.ht,
                    );
                    if result.requires_ready {
                        a.enemies[n].ready_turns = 1;
                    }
                    if result.defense_roll.is_some() || result.injury > 0 {
                        a.aim_target = None;
                        a.aim_seconds = 0;
                    }
                    damage = result.injury;
                    a.hp = a.injury.hp;
                    a.resolution_event(&result, origin, a.defender_position(), meta, false);
                    a.resolved(
                        Some(actor_id),
                        None,
                        &result,
                        format!(
                            "{} → survivor from lane {}: {}",
                            d.name,
                            lane + 1,
                            result.explanation()
                        ),
                    );
                }
                a.enemies[n].injury.clear_shock();
            } else if let Some(s) = structure {
                a.structures[s].hp -= damage;
            } else {
                damage = (damage - a.profile.personal.armor).max(0);
                if a.statuses.active(StatusId::Brace) {
                    damage /= 2;
                }
                a.hp -= damage;
            }
            let point = structure.map_or(a.defender_position(), |s| {
                [a.structures[s].x(), a.structures[s].y()]
            });
            if a.mode == Mode::Personal {
                a.event_with(
                    if damage > 0 {
                        EventKind::Damage
                    } else {
                        EventKind::Armor
                    },
                    origin,
                    point,
                    damage,
                    meta,
                );
            }
            if structure.is_none() && damage > 0 {
                a.last_damage_lane = Some(lane);
                a.telemetry.damage_by_lane[lane as usize] += damage as u64;
                // Personal hit points and the NPC injury resolver both land
                // here; bosses and unresolved legacy enemies are not counted.
                let e = &a.enemies[n];
                if let Some(r) = e.resolved.as_ref().filter(|_| !e.boss) {
                    a.telemetry
                        .count_archetype_damage_taken(&r.archetype_id, damage as u64);
                }
            }
            if structure.is_some() {
                a.telemetry.structure_damage_taken += damage.max(0) as u64;
            }
            if a.mode == Mode::Npc && a.injury.terminal() {
                break;
            }
        }
        let destroyed: Vec<_> = a
            .structures
            .iter()
            .filter(|s| s.hp <= 0)
            .map(|s| [s.x(), s.y()])
            .collect();
        for point in destroyed {
            a.telemetry.structure_losses += 1;
            a.event(EventKind::StructureLost, point, point, 0);
        }
        a.structures.retain(|s| s.hp > 0);
        for e in &mut a.enemies {
            if e.target_structure
                .is_some_and(|id| !a.structures.iter().any(|s| s.id == id))
            {
                e.target_structure = None;
                e.contact_since = None;
                e.attack_left = 0;
                e.attack_phase = EnemyPhase::Advance;
                e.windup_total = 0;
            }
        }
    }
    /// The uniform drop pick over `available` catalogue indices: exactly one
    /// `a.rng` draw. None when the pool is empty, which draws nothing.
    fn uniform_drop_pick(a: &mut Attempt, available: &[usize]) -> Option<usize> {
        if available.is_empty() {
            return None;
        }
        Some(available[a.rng.below(available.len() as u64) as usize])
    }
    /// The v7 drop pick (`drop_tier` Some): uniform over
    /// [`crate::content::Content::drop_pool`] at the encounter template's
    /// level + 1, the pool the content pins count.
    fn uniform_drop_definition(a: &mut Attempt) -> Option<usize> {
        let available = content().drop_pool(a.level + 1);
        Self::uniform_drop_pick(a, &available)
    }
    /// The pre-v7 drop pool: only the [`LEGACY_DROP_DEFINITIONS`] v1
    /// definitions (index 4 on, tiers 1 to the template's level + 1), so an
    /// attempt deployed before v7 never draws an appended item and keeps its
    /// picks byte-identical (ruling E11). The filter is verbatim, with no
    /// NPC-issue term.
    fn legacy_drop_definition(a: &mut Attempt) -> Option<usize> {
        let available: Vec<usize> = content()
            .items
            .iter()
            .take(LEGACY_DROP_DEFINITIONS)
            .enumerate()
            .filter(|(n, i)| *n >= 4 && i.tier > 0 && i.tier <= a.level + 1)
            .map(|(n, _)| n)
            .collect();
        Self::uniform_drop_pick(a, &available)
    }
    /// The legacy drop: [`Self::legacy_drop_definition`] and the absolute
    /// quality roll (keyed, no `a.rng`).
    fn legacy_drop_pick(
        &self,
        a: &mut Attempt,
        e: &Enemy,
    ) -> Option<(usize, crate::quality::Quality)> {
        let definition = Self::legacy_drop_definition(a)?;
        let quality = crate::quality::catalog().roll(self.seed, a.id, e.id, a.progression.rank);
        Some((definition, quality))
    }
    /// Banks one drop in the shared armory and counts it on the attempt (the
    /// loot total, the quality counts, the capped receipts and the log).
    /// Returns the new item id, or None when the armory refused the item.
    fn bank_drop(
        &mut self,
        a: &mut Attempt,
        e: &Enemy,
        definition: usize,
        quality: crate::quality::Quality,
    ) -> Option<u64> {
        let item_id = self
            .campaign
            .add_item_with_quality(definition, a.progression.item_level, Some(quality))
            .ok()?;
        a.loot += 1;
        a.loot_quality_counts[quality] += 1;
        if a.loot_events.len() == crate::quality::MAX_LOOT_EVENTS {
            a.loot_events.remove(0);
        }
        a.loot_events.push(crate::quality::LootEvent {
            at: [e.x, e.y()],
            item_id,
            definition: ItemDefId::at(definition),
            level: a.progression.item_level,
            quality,
            name: content().items[definition].name.clone(),
            tick: a.tick,
        });
        a.note(format!(
            "Recovered {} {} · level {} · banked in the shared armory",
            quality.name(),
            content().items[definition].name,
            a.progression.item_level
        ));
        Some(item_id)
    }
    /// Records a drop for settlement (ruling E5): only a root attempt, and
    /// at most [`MAX_RUN_DROPS`] per run (the save bound). Item ids rise
    /// monotonically, so the list stays ascending.
    fn record_drop(&self, a: &mut Attempt, item_id: u64) {
        if !self.worker && a.drop_ids.len() < MAX_RUN_DROPS {
            a.drop_ids.push(item_id);
        }
    }
    pub(super) fn reap(&mut self, a: &mut Attempt) {
        let defeated: Vec<usize> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if a.mode == Mode::Personal {
                    e.hp <= 0
                } else {
                    e.injury.terminal()
                }
            })
            .map(|(n, _)| n)
            .collect();
        for n in defeated.into_iter().rev() {
            let e = a.enemies.remove(n);
            if !a.reward_ledger.insert(e.id) {
                continue;
            }
            a.corpses.push(CorpseRecord {
                id: e.id,
                kind: e.kind,
                boss_kind: e.boss_kind,
                at: [e.x, e.y()],
                visual: e.resolved.as_ref().map(|profile| profile.visual.clone()),
                tick: a.tick,
            });
            let d = enemy_profile(&e);
            let level = &content().levels[a.level];
            let base_coins = if e.boss {
                level.boss_reward + level.completion_bonus
            } else {
                d.reward
            };
            let coins = (base_coins as f32 * a.modifiers.reward).round().max(1.0) as u64;
            let xp = if e.boss {
                40 + a.level as u64 * 15
            } else {
                d.xp
            };
            a.kill_xp = a
                .kill_xp
                .saturating_add(self.campaign.reward(coins, xp, a.mode));
            a.telemetry.kills_by_kind[e.kind] += 1;
            if let Some(r) = e.resolved.as_ref().filter(|_| !e.boss) {
                a.telemetry.count_archetype_kill(&r.archetype_id);
            }
            if e.boss {
                a.telemetry.boss_defeat_tick = Some(a.tick);
            }
            a.coins += coins;
            a.kills += 1;
            a.event_with(
                EventKind::Kill,
                [e.x, e.y()],
                [e.x, e.y()],
                coins as i32,
                EventMetadata {
                    actor_id: Some(e.id),
                    lane: Some(e.lane),
                    ..Default::default()
                },
            );
            let drop = e.boss || a.rng.below(100) < 3 || a.kills.is_multiple_of(20);
            if drop {
                if a.drop_tier.is_none() {
                    // An attempt deployed before v7 (ruling E11): today's
                    // drop, verbatim, pinned by V6_DROP_TRACE_FNV. It records
                    // no drop id, so settlement never auto-sells it.
                    if let Some((definition, quality)) = self.legacy_drop_pick(a, &e) {
                        self.bank_drop(a, &e, definition, quality);
                    }
                } else if let Some(definition) = Self::uniform_drop_definition(a) {
                    // The Some path draws from `Content::drop_pool`, the
                    // loot-only items included (W2-08), with today's uniform
                    // pick until the weighted DropTable (W2-09) replaces it,
                    // and rolls its quality relative to the item's base
                    // (decision 7). The elite floor arrives with the elite
                    // drop split (W2-35).
                    let quality = crate::loot::drop_quality(
                        content().items[definition].base_quality,
                        self.seed,
                        a.id,
                        e.id,
                        a.progression.rank,
                        false,
                    );
                    if let Some(item_id) = self.bank_drop(a, &e, definition, quality) {
                        self.record_drop(a, item_id);
                    }
                }
            }
            if a.mode == Mode::Npc {
                a.note(format!(
                    "{} in lane {} {} · +{} coins",
                    enemy_name(a, &e),
                    e.lane + 1,
                    if e.injury.dead {
                        "dead"
                    } else if e.injury.unconscious {
                        "unconscious; removed from the encounter"
                    } else {
                        "incapacitated"
                    },
                    coins
                ));
            }
        }
    }
}
