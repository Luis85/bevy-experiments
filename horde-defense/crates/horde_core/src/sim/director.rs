//! Spawn director: wave pacing, boss arrival and enemy materialisation.

use super::*;

impl Game {
    pub(super) fn spawn_director(&mut self, a: &mut Attempt) {
        let level = &content().levels[a.level];
        let boss_tick = u64::from(seconds(a.boss_time()));
        let warning = u64::from(seconds(level.boss_timing.warning));
        if !a.boss_spawned && !a.boss_warning_sent && a.tick >= boss_tick.saturating_sub(warning) {
            a.boss_warning_sent = true;
            a.event_with(
                EventKind::BossWarning,
                [a.width_yards, a.defender_position()[1]],
                [a.width_yards, a.defender_position()[1]],
                (boss_tick.saturating_sub(a.tick) / 60) as i32,
                EventMetadata {
                    lane: Some(a.lane_count / 2),
                    duration_ticks: warning as u32,
                    ..Default::default()
                },
            );
            a.note(format!(
                "{} approaches in {:.1}s",
                level.boss_name,
                a.boss_countdown()
            ));
        }
        if !a.boss_spawned && a.elapsed() >= a.boss_time() {
            a.boss_spawned = true;
            self.spawn_enemy(a, EnemyKind::Brute, a.lane_count / 2, true);
            a.caller_tick = a.tick + u64::from(seconds(level.boss_timing.initial_call));
            a.telemetry.boss_arrival_tick = Some(a.tick);
            a.note(format!(
                "{} has arrived. Defeat it to secure the sector.",
                level.boss_name
            ));
        }
        // Credits count groups, preserving the authored mean enemy rate while
        // higher ranks deliver a larger simultaneous, independently laned horde.
        a.spawn_credit += a.arrival_rate() * DT / a.progression.batch_size as f32;
        if a.spawn_credit >= 1.0 {
            a.spawn_credit -= 1.0;
            for _ in 0..a.progression.batch_size {
                if a.enemies.iter().filter(|e| !e.boss).count() < a.enemy_cap() {
                    let lane = choose_lane(a);
                    let pool = crate::enemy_catalog::enemy_catalog().wave_pool(
                        a.level.max(a.progression.wave_tier),
                        &self.campaign.unlocked_bosses(),
                    );
                    let total = pool.iter().map(|entry| u64::from(entry.weight)).sum();
                    let mut roll = a.rng.below(total);
                    for entry in &pool {
                        if roll < u64::from(entry.weight) {
                            self.spawn_archetype_as(a, &entry.archetype_id, lane, Arrival::Wave)
                                .expect("validated eligible wave archetype");
                            break;
                        }
                        roll -= u64::from(entry.weight);
                    }
                } else {
                    a.telemetry.skipped_spawns += 1;
                }
            }
        }
        if a.boss_spawned && level.boss == BossKind::Caller {
            if a.caller_lanes.is_none()
                && a.tick
                    >= a.caller_tick
                        .saturating_sub(u64::from(seconds(level.boss_timing.call_warning)))
            {
                let lane = a.rng.below(u64::from(a.lane_count)) as u8;
                let lanes = [lane, (lane + a.lane_count / 2) % a.lane_count];
                a.caller_lanes = Some(lanes);
                for lane in lanes {
                    a.event_with(
                        EventKind::Reinforcements,
                        [a.width_yards, lane as f32 + 0.5],
                        [a.width_yards, lane as f32 + 0.5],
                        1,
                        EventMetadata {
                            lane: Some(lane),
                            duration_ticks: seconds(level.boss_timing.call_warning),
                            ..Default::default()
                        },
                    );
                }
                a.note(format!(
                    "Caller signals lanes {} and {} · reinforcements in {:.1}s",
                    lanes[0] + 1,
                    lanes[1] + 1,
                    level.boss_timing.call_warning
                ));
            }
            if a.tick >= a.caller_tick {
                let m = a
                    .enemies
                    .iter()
                    .find(|e| e.boss)
                    .and_then(|e| e.boss_state.as_ref())
                    .map_or(1.0, BossState::call_interval_mult);
                a.caller_tick =
                    a.tick + u64::from(phase_ticks(seconds(level.boss_timing.call_interval), m));
                for l in a.caller_lanes.take().unwrap_or([0, a.lane_count / 2]) {
                    if a.enemies.iter().filter(|e| !e.boss).count() < a.enemy_cap() {
                        self.spawn_enemy(a, EnemyKind::Runner, l, false);
                    } else {
                        a.telemetry.skipped_spawns += 1;
                    }
                }
            }
        }
        if let Some(boss) = a.enemies.iter().find(|e| e.boss) {
            let exposed = boss.exposed_at(a.tick);
            if exposed != boss.exposed_at(a.tick.saturating_sub(1))
                || (exposed && boss.born_tick == a.tick)
            {
                let point = [boss.x, boss.y()];
                let id = boss.id;
                let lane = boss.lane;
                let duration = if exposed {
                    boss.boss_timing.exposure_duration
                } else {
                    boss.boss_timing.cycle - boss.boss_timing.exposure_duration
                };
                a.event_with(
                    EventKind::BossExposure,
                    point,
                    point,
                    i32::from(exposed),
                    EventMetadata {
                        actor_id: Some(id),
                        lane: Some(lane),
                        duration_ticks: seconds(duration),
                        ..Default::default()
                    },
                );
            }
        }
    }
    pub fn spawn_enemy(&mut self, a: &mut Attempt, kind: EnemyKind, lane: u8, boss: bool) {
        let resolved = crate::enemy_catalog::resolve_kind(kind);
        self.place_enemy(a, resolved, lane, boss, Arrival::Plain);
    }
    /// Spawns one pick of archetype `id` in `lane`, never as an elite (only
    /// the director's wave picks roll elite).
    pub fn spawn_archetype(&mut self, a: &mut Attempt, id: &str, lane: u8) -> Result<(), String> {
        self.spawn_archetype_as(a, id, lane, Arrival::Plain)
    }
    pub(super) fn spawn_archetype_as(
        &mut self,
        a: &mut Attempt,
        id: &str,
        lane: u8,
        arrival: Arrival,
    ) -> Result<(), String> {
        let resolved = crate::enemy_catalog::enemy_catalog().resolve(id)?;
        if resolved
            .unlocked_by
            .is_some_and(|boss| !self.campaign.unlocked_bosses().contains(&boss))
        {
            return Err("Defeat this boss before its ordinary variant joins the horde".into());
        }
        self.spawn_group(a, resolved, lane, arrival);
        Ok(())
    }
    /// A staged (never elite) pick of `resolved` in `lane`, for tests.
    #[cfg(test)]
    pub(super) fn spawn_pick(
        &mut self,
        a: &mut Attempt,
        resolved: crate::enemy_catalog::ResolvedEnemy,
        lane: u8,
    ) {
        self.spawn_group(a, resolved, lane, Arrival::Plain);
    }
    /// One weighted pick of `resolved` in `lane`: `group_size` members (a
    /// sprinter pack, W2-30), each behind the last at the lane's tail, so at
    /// `spawn_x + 0.65k`. The pick draws nothing more than one member would,
    /// and each member respects the cap; a member past it is a skipped spawn.
    /// Each member of a wave pick rolls elite on its own id.
    fn spawn_group(
        &mut self,
        a: &mut Attempt,
        resolved: crate::enemy_catalog::ResolvedEnemy,
        lane: u8,
        arrival: Arrival,
    ) {
        if resolved.group_size <= 1 {
            self.place_enemy(a, resolved, lane, false, arrival);
            return;
        }
        // The first member is the pick the caller made room for.
        for member in 0..resolved.group_size {
            if member > 0 && a.enemies.iter().filter(|e| !e.boss).count() >= a.enemy_cap() {
                a.telemetry.skipped_spawns += 1;
                continue;
            }
            self.place_enemy(a, resolved.clone(), lane, false, arrival);
        }
    }
    /// A director wave spawn of `resolved` as hostile `id`, made elite when
    /// the keyed ELITE roll lands under the snapshot's rate (design
    /// enemy-balance §5, ruling E7). The affix is the keyed AFFIX pick. No
    /// combat RNG is read, and an ineligible archetype or a zero rate
    /// returns the profile unchanged.
    fn roll_elite(
        &self,
        a: &Attempt,
        resolved: crate::enemy_catalog::ResolvedEnemy,
        id: u64,
    ) -> crate::enemy_catalog::ResolvedEnemy {
        let catalog = crate::enemy_catalog::enemy_catalog();
        let rate = a.progression.elite_rate_bp;
        if rate == 0
            || !catalog.elite_eligible(&resolved.archetype_id)
            || !rolls_elite(self.seed, a.id, id, rate)
        {
            return resolved;
        }
        let key = crate::hash::keyed(self.seed, a.id, id, crate::hash::tags::AFFIX);
        catalog.elite_profile(&resolved, key).unwrap_or(resolved)
    }
    /// Materialises one hostile in `lane` within the caps. A wave
    /// `arrival` may roll elite (bosses never do). A split arrival places a
    /// splitter's child at that x (W2-29): marked `split_child`, counted as
    /// a spawn, and announced by the parent's `Split` event instead of a
    /// `Spawn` at the field edge. No combat draw either way.
    pub(super) fn place_enemy(
        &mut self,
        a: &mut Attempt,
        resolved: crate::enemy_catalog::ResolvedEnemy,
        lane: u8,
        boss: bool,
        arrival: Arrival,
    ) {
        let level = &content().levels[a.level];
        if (boss && a.enemies.iter().any(|e| e.boss))
            || (!boss && a.enemies.iter().filter(|e| !e.boss).count() >= a.enemy_cap())
        {
            return;
        }
        let resolved = if arrival == Arrival::Wave && !boss {
            self.roll_elite(a, resolved, a.next_id)
        } else {
            resolved
        };
        let elite = resolved.elite.is_some();
        let kind = resolved.kind;
        let d = &resolved;
        let split_at = match arrival {
            Arrival::Split(x) => Some(x),
            Arrival::Wave | Arrival::Plain => None,
        };
        let lane = lane.min(a.lane_count - 1);
        let spawn_x = split_at.unwrap_or_else(|| {
            a.enemies
                .iter()
                .filter(|e| e.lane == lane)
                .fold(a.width_yards + 0.7, |tail, e| tail.max(e.x + 0.65))
        });
        let base_hp = if a.mode == Mode::Personal {
            if boss {
                level.boss_health.max(a.progression.boss_health_floor)
            } else {
                d.health
            }
        } else if boss {
            level.boss_npc_hp.max(a.progression.npc_boss_health_floor)
        } else {
            d.npc_hp
        };
        // The boss-rank health term (W4-01, ruling W-9), in the order of
        // `encounter_health_bound`; the literal 1.0 on a Sector boss.
        let rank_health = if boss {
            a.progression.boss_health_multiplier
        } else {
            1.0
        };
        let max_hp = (base_hp as f32 * rank_health * a.modifiers.enemy_health)
            .round()
            .max(1.0) as i32;
        let id = a.next_id;
        a.next_id += 1;
        a.enemies.push(Enemy {
            arena_width: a.width_yards,
            pass_offset: 0.0,
            id,
            kind,
            lane,
            x: spawn_x,
            hp: max_hp,
            max_hp,
            boss,
            attack_left: 0,
            armor: if a.mode == Mode::Npc {
                if boss {
                    level.boss_npc_armor + a.progression.npc_boss_armor_add
                } else {
                    d.npc_armor
                }
            } else if boss {
                boss_armor(level.boss_armor, a.progression.boss_armor_multiplier)
            } else {
                d.armor
            },
            injury: InjuryState::new(max_hp, 10),
            born_tick: a.tick,
            contact_since: None,
            target_structure: None,
            ready_turns: 0,
            boss_kind: if boss { Some(level.boss) } else { None },
            boss_timing: level.boss_timing.clone(),
            attack_phase: EnemyPhase::Advance,
            windup_total: 0,
            statuses: Statuses::default(),
            resolved: (!boss).then_some(resolved),
            split_child: split_at.is_some(),
            // Snapshotted at spawn like `boss_timing`: the frozen transition
            // count and the current catalogue's profile for the level's boss.
            boss_state: boss
                .then_some(a.progression.boss_phases)
                .filter(|&transitions| transitions > 0)
                .and_then(|transitions| {
                    crate::enemy_catalog::enemy_catalog()
                        .boss_phase(level.boss)
                        .map(|profile| BossState {
                            profile: profile.clone(),
                            transitions,
                            entered: 0,
                            hold_until: 0,
                        })
                }),
        });
        a.telemetry.spawned += 1;
        if elite {
            a.telemetry.elites_spawned = a.telemetry.elites_spawned.saturating_add(1);
        }
        // Bosses carry no resolved snapshot, so only ordinary spawns count.
        if let Some(r) = a.enemies.last().and_then(|e| e.resolved.as_ref()) {
            a.telemetry.count_archetype_spawn(&r.archetype_id);
        }
        a.telemetry.peak_enemies = a.telemetry.peak_enemies.max(a.enemies.len());
        if split_at.is_some() {
            return;
        }
        a.event_with(
            if boss {
                EventKind::Boss
            } else {
                EventKind::Spawn
            },
            [a.width_yards, lane as f32 + 0.5],
            [a.width_yards, lane as f32 + 0.5],
            0,
            EventMetadata {
                actor_id: Some(id),
                lane: Some(lane),
                duration_ticks: 72,
                ..Default::default()
            },
        );
    }
}
/// How a hostile enters the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Arrival {
    /// A director wave pick: it may roll elite.
    Wave,
    /// Any other spawn (bosses, Caller summons, staged spawns): never elite.
    Plain,
    /// A splitter's child at this x: never elite, never announced.
    Split(f32),
}

/// The keyed elite roll of hostile `enemy` in attempt `attempt` (ruling E7):
/// `keyed(seed, attempt, enemy, ELITE) % 10000 < rate_bp`. Stateless, so it
/// never draws the combat RNG.
pub(super) fn rolls_elite(seed: u64, attempt: u64, enemy: u64, rate_bp: u32) -> bool {
    crate::hash::keyed(seed, attempt, enemy, crate::hash::tags::ELITE) % 10_000 < u64::from(rate_bp)
}

/// The Personal boss armor scaled by the snapshot's rank multiplier,
/// rounded; the literal 1.0 keeps the level's armor exactly.
fn boss_armor(armor: i32, multiplier: f32) -> i32 {
    if multiplier == 1.0 {
        return armor;
    }
    (armor as f32 * multiplier).round() as i32
}

#[cfg(test)]
mod boss_warning_tests {
    use super::*;
    use crate::balance::BalanceSettings;

    /// W3-14: `Attempt::boss_warning_tick` is `None` until the director warns
    /// and then names the `BossWarning` event's tick, the boss's tick less
    /// the level's warning lead. The UI keys its wall-clock banner on it.
    #[test]
    fn boss_warning_tick_is_the_warning_events_tick() {
        let mut g = Game::default();
        g.campaign.legacy_features = true;
        g.campaign.balance_settings = BalanceSettings::legacy();
        g.deploy(0, Mode::Personal).unwrap();
        g.set_paused(false);
        let a = g.attempt.as_mut().unwrap();
        a.max_hp = 1_000_000;
        a.hp = a.max_hp;
        a.spawn_credit = -1.0e9;
        let lead = u64::from(seconds(content().levels[a.level].boss_timing.warning));
        let due = u64::from(seconds(a.boss_time())) - lead;
        assert!(lead > 0 && due > 3);
        a.tick = due - 3;
        assert_eq!(a.boss_warning_tick(), None);
        let mut ticks = 0;
        while g.attempt().unwrap().boss_warning_tick().is_none() {
            g.tick(Input::default());
            ticks += 1;
            assert!(ticks <= 10, "the director never warned");
        }
        let a = g.attempt().unwrap();
        let event = a
            .events
            .iter()
            .find(|e| e.kind == EventKind::BossWarning)
            .expect("the warning's event");
        assert_eq!(event.tick, due);
        assert_eq!(a.boss_warning_tick(), Some(due));
        // The answer stays once the boss has arrived.
        for _ in 0..=lead {
            g.tick(Input::default());
        }
        let a = g.attempt().unwrap();
        assert!(a.enemies.iter().any(|e| e.boss));
        assert_eq!(a.boss_warning_tick(), Some(due));
        assert_eq!(a.telemetry.boss_arrival_tick, Some(due + lead));
    }
}
