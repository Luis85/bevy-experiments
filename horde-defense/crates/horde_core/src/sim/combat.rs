//! Enemy attack resolution, damage application and corpse reaping.

use super::*;
use crate::enemy_catalog::DeathEffect;

/// The catalogue prefix a pre-v7 attempt's drop pool reads: the 29 v1
/// definitions. The loot-only items appended at 29-67 (W2-08) and any later
/// appends only reach attempts deployed from v7 on (`drop_tier` Some).
const LEGACY_DROP_DEFINITIONS: usize = 29;

/// Hazard (blast and spit) damage `raw` after the survivor's
/// `hazard_damage_multiplier`, applied before armor and Brace. Personal
/// defenders only, like the class-perk and penetration folds: Mara and
/// recruits (NPC mode) take `raw`. The neutral 1.0 returns `raw` untouched
/// (no float round trip).
// Consumers: the exploder's fuse on a Personal defender (W2-29) and a
// spitter's spit on it (the hazard seam, W2-30).
pub(crate) fn hazard_scaled(a: &Attempt, raw: i32) -> i32 {
    let m = a.profile.modifiers.hazard_damage_multiplier;
    if a.mode != Mode::Personal || m == 1.0 {
        raw
    } else {
        (raw as f32 * m).round() as i32
    }
}

/// An exploder's blast (design enemy-balance §4 "Exploder", ruling W-7),
/// copied out of its [`DeathEffect::Burst`].
#[derive(Clone, Copy, Debug)]
struct Blast {
    radius: f32,
    damage: i32,
    structure_damage: i32,
    npc_dice: u8,
    npc_add: i32,
}

/// The blast `e` carries and whether it also detonates on contact (`fuse`).
/// Bosses fight with their level's stats and never burst.
fn blast_of(e: &Enemy) -> Option<(Blast, bool)> {
    if e.boss {
        return None;
    }
    match e.resolved.as_ref()?.death_effect.as_ref()? {
        DeathEffect::Burst {
            radius,
            damage,
            structure_damage,
            npc_dice,
            npc_add,
            fuse,
        } => Some((
            Blast {
                radius: *radius,
                damage: *damage,
                structure_damage: *structure_damage,
                npc_dice: *npc_dice,
                npc_add: *npc_add,
            },
            *fuse,
        )),
        DeathEffect::Split { .. } => None,
    }
}

/// A `Detonation` event's value: the radius in hundredths of a yard.
fn blast_value(radius: f32) -> i32 {
    (radius * 100.0).round() as i32
}

/// A death effect collected while reaping, applied after the reap loop.
enum DeathAct {
    Burst {
        name: String,
        lane: u8,
        at: [f32; 2],
        blast: Blast,
    },
    Split {
        lane: u8,
        x: f32,
        archetype_id: String,
        count: u8,
    },
}

/// The profile a split child of `archetype_id` spawns with: the current
/// catalogue's record at split time, else the Runner default. A record that
/// could not be a child (a death effect or group of its own, an unlock or
/// an elite mark) also falls back, so a child never splits.
fn split_child_profile(archetype_id: &str) -> crate::enemy_catalog::ResolvedEnemy {
    crate::enemy_catalog::enemy_catalog()
        .resolve(archetype_id)
        .ok()
        .filter(|r| {
            r.death_effect.is_none()
                && r.group_size == 1
                && r.unlocked_by.is_none()
                && r.elite.is_none()
        })
        .unwrap_or_else(|| crate::enemy_catalog::resolve_kind(EnemyKind::Runner))
}

/// Enters the living boss's next phase when its hit points are at or below
/// that transition's threshold (design enemy-balance §6): at most one
/// transition per tick and never past the transitions frozen at spawn. It
/// starts the hold, publishes `BossPhase` (value = the transition number,
/// lasting the hold) and records the tick in the telemetry. Draws no RNG.
fn enter_boss_phase(a: &mut Attempt) {
    let mode = a.mode;
    let tick = a.tick;
    let Some(e) = a
        .enemies
        .iter_mut()
        .find(|e| e.boss && e.condition(mode) != EnemyCondition::Defeated)
    else {
        return;
    };
    let Some(state) = e.boss_state.as_mut() else {
        return;
    };
    // `hold_until` is only written on entry, so this is this tick's entry.
    let entered_this_tick =
        state.entered > 0 && state.hold_until == tick + u64::from(state.profile.hold_ticks);
    if state.entered >= state.transitions || entered_this_tick {
        return;
    }
    let threshold = BOSS_PHASE_THRESHOLDS[usize::from(state.entered).min(1)];
    if i64::from(e.hp) * 100 > i64::from(e.max_hp) * i64::from(threshold) {
        return;
    }
    state.entered += 1;
    state.hold_until = tick + u64::from(state.profile.hold_ticks);
    let (phase, hold, name) = (
        state.entered,
        state.profile.hold_ticks,
        state.profile.name.clone(),
    );
    let (id, lane, point) = (e.id, e.lane, [e.x, e.y()]);
    if a.telemetry.boss_phase_ticks.len() < telemetry::MAX_BOSS_PHASE_TICKS {
        a.telemetry.boss_phase_ticks.push(tick);
    }
    a.event_with(
        EventKind::BossPhase,
        point,
        point,
        i32::from(phase),
        EventMetadata {
            actor_id: Some(id),
            lane: Some(lane),
            duration_ticks: hold,
            ..Default::default()
        },
    );
    let boss_name = content().levels[a.level].boss_name.clone();
    a.note(format!("{boss_name} enters phase {phase}: {name}"));
}

/// Removes every structure a strike or blast destroyed and releases the
/// hostiles that targeted it.
fn remove_destroyed_structures(a: &mut Attempt) {
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

impl Game {
    pub(super) fn enemy_attacks(&mut self, a: &mut Attempt) {
        // Exploders that detonated on contact this pass, removed after it.
        let mut fused = Vec::new();
        for n in 0..a.enemies.len() {
            let e = &a.enemies[n];
            if (a.mode == Mode::Personal && e.hp <= 0)
                || (a.mode == Mode::Npc && e.injury.terminal())
            {
                continue;
            }
            // A boss holding after a phase transition neither counts down
            // nor strikes; its NPC turn bookkeeping still runs.
            let held = a.enemies[n].boss_held(a.tick);
            if !held && a.enemies[n].contact_since.is_some_and(|tick| a.tick > tick) {
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
                    let to = a.windup_to(e, point);
                    a.event_with(EventKind::Windup, point, to, 0, meta);
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
            if held {
                continue;
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
            // A spitter's strike from its station (W2-27) takes the existing
            // attack path; Some carries the yards to its target.
            let spit = spit_range(a, e, structure.map(|s| &a.structures[s]));
            if spit.is_some() && spit_from_far_half(a, e) {
                continue;
            }
            if let Some((blast, true)) = blast_of(e) {
                // Its windup completed in contact: the exploder detonates
                // instead of attacking and leaves the field without reward.
                fused.push(e.id);
                self.detonate_fuse(a, n, structure, blast);
                if a.mode == Mode::Npc && a.injury.terminal() {
                    break;
                }
                continue;
            }
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
            let mut scaled = damage as f32 * a.progression.enemy_damage * a.balance.enemy_damage;
            if e.boss {
                // The boss rank term (ruling W-6); the literal 1.0 through
                // rank 4, and regular enemies never read it.
                scaled *= a.progression.boss_damage_multiplier;
            }
            damage = scaled.round() as i32;
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
                phase_ticks(
                    seconds(content().levels[a.level].boss_timing.siege_recovery),
                    a.enemies[n]
                        .boss_state
                        .as_ref()
                        .map_or(1.0, BossState::siege_recovery_mult),
                )
            } else if a.mode == Mode::Personal {
                seconds(d.windup)
            } else {
                60
            };
            a.enemies[n].windup_total = a.enemies[n].attack_left;
            if !siege {
                let to = a.windup_to(&a.enemies[n], origin);
                a.event_with(
                    EventKind::Windup,
                    origin,
                    to,
                    0,
                    EventMetadata {
                        duration_ticks: a.enemies[n].attack_left,
                        ..meta
                    },
                );
            }
            if structure.is_none() && spit.is_none() {
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
                    // A spit is a ranged attack: the range penalty applies
                    // and the defender may still dodge it.
                    skill: if boss { l.boss_npc_skill } else { d.npc_skill } - turn_shock
                        + spit.map_or(0, rules::range_penalty),
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
                            + ((a.progression.enemy_damage - 1.0) * 6.0).floor() as i32
                            + if boss {
                                // The boss rank term (ruling W-6).
                                ((a.progression.boss_damage_multiplier - 1.0) * 6.0).floor() as i32
                            } else {
                                0
                            },
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
                // A spit is hazard damage (the counter seam, W2-30); a melee
                // strike is not.
                if spit.is_some() {
                    damage = hazard_scaled(a, damage);
                }
                damage = (damage - a.profile.personal.armor).max(0);
                if a.statuses.active(StatusId::Brace) {
                    damage /= 2;
                }
                a.hp -= damage;
            }
            let point = structure.map_or(a.defender_position(), |s| {
                [a.structures[s].x(), a.structures[s].y()]
            });
            if spit.is_some() {
                a.event_with(EventKind::Spit, origin, point, damage.max(0), meta);
            }
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
                if spit.is_some() {
                    a.telemetry.hazard_damage_taken += damage as u64;
                }
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
        if !fused.is_empty() {
            // The ledger keeps a fused id from ever paying; it leaves no
            // corpse and no Kill event.
            a.reward_ledger.extend(fused.iter().copied());
            a.enemies.retain(|e| !fused.contains(&e.id));
        }
        remove_destroyed_structures(a);
    }
    /// The NPC damage of `blast` with the threat terms an enemy attack's
    /// damage carries (the rank's enemy damage and the balance setting).
    fn npc_blast_damage(a: &Attempt, blast: Blast) -> Damage {
        let mut damage = Damage {
            dice: blast.npc_dice,
            adds: blast.npc_add + ((a.progression.enemy_damage - 1.0) * 6.0).floor() as i32,
            kind: DamageKind::Crushing,
        };
        let expected = crate::campaign::mean_basic_damage(damage);
        damage.adds = (damage.adds as f32 + expected * (a.balance.enemy_damage - 1.0))
            .round()
            .clamp(-30.0, 1_000_000.0) as i32;
        damage
    }
    /// The exploder at `n` detonates on its target: the defender, or the
    /// structure at index `structure`. Flat `structure_damage` also hits the
    /// other structures within the radius; other hostiles are not hit.
    fn detonate_fuse(&mut self, a: &mut Attempt, n: usize, structure: Option<usize>, blast: Blast) {
        let e = &a.enemies[n];
        let (id, lane, origin, name) = (e.id, e.lane, [e.x, e.y()], enemy_name(a, e));
        let archetype = e.resolved.as_ref().map(|r| r.archetype_id.clone());
        let target_id = structure.map(|s| a.structures[s].id);
        let to = structure.map_or(a.defender_position(), |s| {
            [a.structures[s].x(), a.structures[s].y()]
        });
        let meta = EventMetadata {
            actor_id: Some(id),
            target_id,
            lane: Some(lane),
            ..Default::default()
        };
        a.event_with(
            EventKind::Detonation,
            origin,
            to,
            blast_value(blast.radius),
            EventMetadata {
                duration_ticks: 30,
                detonation: Some(DetonationTrigger::Fuse { target_id, to }),
                ..meta
            },
        );
        let damage = if a.mode == Mode::Npc {
            let hit = Self::npc_blast_damage(a, blast);
            if let Some(s) = structure {
                let protection = content().structure(a.structures[s].kind).npc.armor;
                let result = rules::resolve_object_blast(&mut a.rng, hit, protection);
                let damage = result.injury * 5;
                a.structures[s].hp -= damage;
                let mut feedback = result.clone();
                feedback.injury = damage;
                a.resolution_event(&feedback, origin, to, meta, false);
                a.resolved(
                    Some(id),
                    target_id,
                    &result,
                    format!(
                        "{name} detonates at a structure in lane {}: {}",
                        lane + 1,
                        result.blast_explanation()
                    ),
                );
                damage
            } else {
                let dodge = a
                    .injury
                    .dodge((a.profile.stats.dx + a.profile.stats.ht) / 4 + 3)
                    + a.defense_bonus;
                // Scale 1.0: the hazard multiplier is the Personal
                // survivor's perk (`hazard_scaled`), never Mara's or a
                // recruit's (W2-24 review).
                let result = rules::resolve_blast(
                    &mut a.rng,
                    hit,
                    Some(dodge),
                    a.profile.npc.armor,
                    1.0,
                    &mut a.injury,
                    a.profile.stats.ht,
                );
                if result.defense_roll.is_some() || result.injury > 0 {
                    a.aim_target = None;
                    a.aim_seconds = 0;
                }
                a.hp = a.injury.hp;
                a.resolution_event(&result, origin, to, meta, false);
                a.resolved(
                    Some(id),
                    None,
                    &result,
                    format!(
                        "{name} detonates against the survivor from lane {}: {}",
                        lane + 1,
                        result.blast_explanation()
                    ),
                );
                result.injury
            }
        } else {
            let raw = (blast.damage as f32 * a.progression.enemy_damage * a.balance.enemy_damage)
                .round() as i32;
            let damage = if let Some(s) = structure {
                a.structures[s].hp -= raw;
                raw
            } else {
                let mut damage = (hazard_scaled(a, raw) - a.profile.personal.armor).max(0);
                if a.statuses.active(StatusId::Brace) {
                    damage /= 2;
                }
                a.hp -= damage;
                damage
            };
            a.event_with(
                if damage > 0 {
                    EventKind::Damage
                } else {
                    EventKind::Armor
                },
                origin,
                to,
                damage,
                meta,
            );
            damage
        };
        if structure.is_some() {
            a.telemetry.structure_damage_taken += damage.max(0) as u64;
        } else if damage > 0 {
            a.last_damage_lane = Some(lane);
            a.telemetry.damage_by_lane[lane as usize] += damage as u64;
            a.telemetry.hazard_damage_taken += damage as u64;
            if let Some(archetype) = archetype {
                a.telemetry
                    .count_archetype_damage_taken(&archetype, damage as u64);
            }
        }
        Self::blast_structures(a, origin, blast, target_id);
    }
    /// Flat `structure_damage` on every live structure within the radius of
    /// `at`, except `spared` (a fuse's target, which took the blast itself).
    fn blast_structures(a: &mut Attempt, at: [f32; 2], blast: Blast, spared: Option<u64>) {
        if blast.structure_damage == 0 {
            return;
        }
        for s in &mut a.structures {
            if s.hp > 0
                && Some(s.id) != spared
                && (s.x() - at[0]).hypot(s.y() - at[1]) <= blast.radius
            {
                s.hp -= blast.structure_damage;
                a.telemetry.structure_damage_taken += blast.structure_damage as u64;
            }
        }
    }
    /// A dying exploder's burst (ruling W-7): every other live hostile within
    /// the radius in ascending id, and flat `structure_damage` on the
    /// structures there; never the defender. A hostile the burst defeats
    /// stays on the field until the next tick's reap, so a chain advances
    /// one link per tick and ends (each exploder bursts once).
    fn death_burst(
        &mut self,
        a: &mut Attempt,
        source: u64,
        name: &str,
        (lane, at): (u8, [f32; 2]),
        blast: Blast,
    ) {
        a.event_with(
            EventKind::Detonation,
            at,
            at,
            blast_value(blast.radius),
            EventMetadata {
                actor_id: Some(source),
                lane: Some(lane),
                duration_ticks: 30,
                detonation: Some(DetonationTrigger::Death),
                ..Default::default()
            },
        );
        let mut targets: Vec<(u64, usize)> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !enemy_defeated(a, e) && (e.x - at[0]).hypot(e.y() - at[1]) <= blast.radius
            })
            .map(|(n, e)| (e.id, n))
            .collect();
        targets.sort_unstable();
        for (id, n) in targets {
            if a.mode == Mode::Personal {
                let damage =
                    (blast.damage - enemy_armor_vs(a, &a.enemies[n], HitSource::Area)).max(0);
                a.enemies[n].apply_damage(damage, Mode::Personal);
            } else {
                let e = &a.enemies[n];
                let dodge = e.injury.dodge(enemy_dodge(a, e));
                let armor = enemy_armor_vs(a, e, HitSource::Area);
                let (ht, target) = (enemy_ht(a, e), enemy_name(a, e));
                let mut result = rules::resolve_blast(
                    &mut a.rng,
                    Damage {
                        dice: blast.npc_dice,
                        adds: blast.npc_add,
                        kind: DamageKind::Crushing,
                    },
                    Some(dodge),
                    armor,
                    1.0,
                    &mut a.enemies[n].injury,
                    ht,
                );
                a.enemies[n].log_hit(&mut result);
                a.enemies[n].apply_damage(result.injury, Mode::Npc);
                a.resolved(
                    Some(source),
                    Some(id),
                    &result,
                    format!(
                        "{name} bursts on {target} in lane {}: {}",
                        a.enemies[n].lane + 1,
                        result.blast_explanation()
                    ),
                );
            }
            if enemy_defeated(a, &a.enemies[n]) {
                if a.burst_victims.0 != a.tick {
                    a.burst_victims = (a.tick, Vec::new());
                }
                a.burst_victims.1.push(id);
            }
        }
        Self::blast_structures(a, at, blast, None);
    }
    /// A splitter's children: `min(count, enemy_cap - regular hostiles)` of
    /// [`split_child_profile`] at `max(x, DEFENSE_LINE) + 0.35k` in its lane,
    /// with their own stats and no combat draw.
    fn split(
        &mut self,
        a: &mut Attempt,
        parent: u64,
        (lane, x): (u8, f32),
        target: &str,
        count: u8,
    ) {
        let regular = a.enemies.iter().filter(|e| !e.boss).count();
        let spawned = usize::from(count).min(a.enemy_cap().saturating_sub(regular));
        if spawned == 0 {
            return;
        }
        let profile = split_child_profile(target);
        for k in 0..spawned {
            let at = x.max(DEFENSE_LINE) + 0.35 * k as f32;
            self.place_enemy(
                a,
                profile.clone(),
                lane,
                false,
                director::Arrival::Split(at),
            );
        }
        let at = [x, lane as f32 + 0.5];
        a.event_with(
            EventKind::Split,
            at,
            at,
            spawned as i32,
            EventMetadata {
                actor_id: Some(parent),
                lane: Some(lane),
                duration_ticks: 30,
                ..Default::default()
            },
        );
    }
    /// The uniform drop pick over `available` catalogue indices: exactly one
    /// `a.rng` draw. None when the pool is empty, which draws nothing.
    fn uniform_drop_pick(a: &mut Attempt, available: &[usize]) -> Option<usize> {
        if available.is_empty() {
            return None;
        }
        Some(available[a.rng.below(available.len() as u64) as usize])
    }
    /// The v7 drop pick (`drop_tier` Some): the weighted
    /// [`crate::loot::DropTable`] at the attempt's frozen drop tier, the
    /// loot-only items included (W2-08). A `normal` drop takes exactly one
    /// `a.rng.below(total)` draw, as the uniform pick took one; an
    /// elite-only drop (ruling E8) picks with the keyed ELITE_PICK hash and
    /// draws nothing, so elite status never changes the combat RNG. An
    /// empty table draws nothing.
    fn drop_definition(&self, a: &mut Attempt, e: &Enemy, normal: bool) -> Option<usize> {
        let table = crate::loot::DropTable::for_attempt(a);
        if table.total() == 0 {
            return None;
        }
        let roll = if normal {
            a.rng.below(table.total())
        } else {
            crate::hash::keyed(self.seed, a.id, e.id, crate::hash::tags::ELITE_PICK) % table.total()
        };
        Some(table.pick(roll))
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
        // Every damage step reaps, so a boss enters its phase right after
        // the hit that crossed the threshold.
        enter_boss_phase(a);
        // A hostile a death burst defeated this tick waits for the next.
        let (burst_tick, held) = &a.burst_victims;
        let defeated: Vec<usize> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                (if a.mode == Mode::Personal {
                    e.hp <= 0
                } else {
                    e.injury.terminal()
                }) && !(*burst_tick == a.tick && held.contains(&e.id))
            })
            .map(|(n, _)| n)
            .collect();
        let mut effects = Vec::new();
        for n in defeated.into_iter().rev() {
            let e = a.enemies.remove(n);
            if !a.reward_ledger.insert(e.id) {
                continue;
            }
            if let Some((blast, _)) = blast_of(&e) {
                effects.push((
                    e.id,
                    DeathAct::Burst {
                        name: enemy_name(a, &e),
                        lane: e.lane,
                        at: [e.x, e.y()],
                        blast,
                    },
                ));
            } else if let Some(DeathEffect::Split {
                archetype_id,
                count,
            }) = e
                .resolved
                .as_ref()
                .filter(|_| !e.boss && !e.split_child)
                .and_then(|r| r.death_effect.as_ref())
            {
                effects.push((
                    e.id,
                    DeathAct::Split {
                        lane: e.lane,
                        x: e.x,
                        archetype_id: archetype_id.clone(),
                        count: *count,
                    },
                ));
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
                // Like the archetype kills, only while below the spawns.
                if r.elite.is_some() && a.telemetry.elites_killed < a.telemetry.elites_spawned {
                    a.telemetry.elites_killed += 1;
                }
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
            // A split child takes and discards the 3% roll and never drops
            // (ruling E8), so it draws what any other kill draws. Elite
            // status never changes the draws either: an elite always drops,
            // but only the normal term reads the combat RNG for its pick.
            let rolled = !e.boss && a.rng.below(100) < 3;
            let normal = e.boss || (!e.split_child && (rolled || a.kills.is_multiple_of(20)));
            let elite =
                !e.boss && !e.split_child && e.resolved.as_ref().is_some_and(|r| r.elite.is_some());
            if a.drop_tier.is_none() {
                // An attempt deployed before v7 (ruling E11): today's drop,
                // verbatim, pinned by V6_DROP_TRACE_FNV, with no elite term.
                // It records no drop id, so settlement never auto-sells it.
                if normal && let Some((definition, quality)) = self.legacy_drop_pick(a, &e) {
                    self.bank_drop(a, &e, definition, quality);
                }
            } else if (normal || elite)
                && let Some(definition) = self.drop_definition(a, &e, normal)
            {
                // The Some path rolls its quality relative to the item's
                // base (decision 7); an elite's drop is never below
                // `ELITE_QUALITY_FLOOR`, a boss's has no floor.
                let quality = crate::loot::drop_quality(
                    content().items[definition].base_quality,
                    self.seed,
                    a.id,
                    e.id,
                    a.progression.rank,
                    elite,
                );
                if let Some(item_id) = self.bank_drop(a, &e, definition, quality) {
                    self.record_drop(a, item_id);
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
        if effects.is_empty() {
            return;
        }
        // After the loop, in ascending id of the hostile that died.
        effects.sort_unstable_by_key(|(id, _)| *id);
        for (id, act) in effects {
            match act {
                DeathAct::Burst {
                    name,
                    lane,
                    at,
                    blast,
                } => self.death_burst(a, id, &name, (lane, at), blast),
                DeathAct::Split {
                    lane,
                    x,
                    archetype_id,
                    count,
                } => self.split(a, id, (lane, x), &archetype_id, count),
            }
        }
        remove_destroyed_structures(a);
    }
}
