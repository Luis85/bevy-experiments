//! Enemy movement, blocking and structure contact.
//!
//! Enemies never block each other (user decision, 2026-11-06, W1-69): there
//! is no lane queue and no overtaking, bodies overlap. Each enemy walks until
//! it reaches `DEFENSE_LINE` or the face of a live non-trap structure in its
//! lane (structures are barricades, not traffic) and attacks from there.
//! An enemy with `reach` (the spitter, W2-27) stops instead at
//! `ranged_stop`, short of the line and never in the far half, unless a
//! structure's face lies ahead of that stop; followers walk through it.
//! `EnemyPhase::Queued` is never assigned; a saved one resolves to Advance or
//! contact on the next step.

use super::*;

impl Game {
    pub(super) fn move_enemies(&mut self, a: &mut Attempt) {
        for lane in 0..a.lane_count {
            let mut order: Vec<usize> = a
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| e.lane == lane)
                .map(|(n, _)| n)
                .collect();
            order.sort_by(|&n, &m| {
                a.enemies[n]
                    .x
                    .total_cmp(&a.enemies[m].x)
                    .then(a.enemies[n].id.cmp(&a.enemies[m].id))
            });
            for n in order {
                let e = &a.enemies[n];
                if e.condition(a.mode) == EnemyCondition::Defeated {
                    continue;
                }
                let previous_target = e.target_structure;
                // A ranged enemy stations at its clamped stop; a live
                // structure whose face lies ahead of it is met in melee.
                let station = ranged_stop(a.width_yards, enemy_reach(e));
                let mut stop = station;
                let mut blocker = None;
                for s in &a.structures {
                    if s.lane == lane
                        && s.kind != StructureKind::Trap
                        && s.hp > 0
                        && s.x() <= e.x + s.blocking_radius()
                        && s.x() + s.blocking_radius() > stop
                    {
                        stop = s.x() + s.blocking_radius();
                        blocker = Some(s.id);
                    }
                }
                let speed = if e.boss {
                    boss_speed(e)
                } else {
                    enemy_profile(e).speed
                };
                let injury = &a.enemies[n].injury;
                // A boss holding after a phase transition stands still.
                let slow = if a.enemies[n].statuses.active(StatusId::HitStop)
                    || a.enemies[n].boss_held(a.tick)
                    || (a.mode == Mode::Npc && (injury.prone || injury.stunned))
                {
                    0.0
                } else if a.mode == Mode::Npc {
                    let hp = if injury.hp * 3 < injury.max_hp {
                        0.5
                    } else {
                        1.0
                    };
                    let fp = if injury.fp * 3 < injury.max_fp {
                        0.5
                    } else {
                        1.0
                    };
                    hp * fp
                } else {
                    1.0
                };
                a.enemies[n].x = (a.enemies[n].x - speed * DT * slow * a.modifiers.enemy_speed)
                    .max(stop.min(a.enemies[n].x));
                a.enemies[n].statuses.tick();
                let blocked = a.enemies[n].x <= stop + 0.01;
                // `stop` is the line, a structure face or a spitter's station.
                // A stationed spitter spits at the nearest wall between it
                // and the line, else at the defender; reach 0 never stations.
                let target = if !blocked {
                    None
                } else if blocker.is_none() && station > DEFENSE_LINE {
                    spit_wall(a, lane, a.enemies[n].x)
                } else {
                    blocker
                };
                a.enemies[n].target_structure = target;
                if blocked {
                    if a.enemies[n].contact_since.is_none() || previous_target != target {
                        a.enemies[n].contact_since = Some(a.tick);
                        let duration = seconds(enemy_profile(&a.enemies[n]).windup);
                        a.enemies[n].attack_left = duration;
                        a.enemies[n].windup_total = duration;
                        a.enemies[n].attack_phase = EnemyPhase::Windup;
                        let enemy = &a.enemies[n];
                        let point = [enemy.x, enemy.y()];
                        let meta = EventMetadata {
                            actor_id: Some(enemy.id),
                            target_id: target,
                            lane: Some(lane),
                            duration_ticks: duration,
                            ..Default::default()
                        };
                        a.event_with(EventKind::Contact, point, point, 0, meta);
                        a.event_with(
                            EventKind::Windup,
                            point,
                            if target.is_none() {
                                a.defender_position()
                            } else {
                                point
                            },
                            0,
                            meta,
                        );
                        if stop <= DEFENSE_LINE + 0.01 && a.first_contact.is_none() {
                            a.first_contact = Some(a.elapsed());
                        }
                    }
                } else {
                    a.enemies[n].contact_since = None;
                    a.enemies[n].attack_left = 0;
                    a.enemies[n].windup_total = 0;
                    a.enemies[n].attack_phase = EnemyPhase::Advance;
                }
                // Overtaking is gone: a saved passing offset eases back to the
                // lane centre at the old passing rate.
                let difference = -a.enemies[n].pass_offset;
                a.enemies[n].pass_offset += difference.clamp(-2.0 * DT * slow, 2.0 * DT * slow);
            }
        }
    }
}
