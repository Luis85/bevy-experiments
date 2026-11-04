//! Enemy movement, blocking and structure contact.

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
            let mut offsets = std::collections::BTreeMap::<usize, f32>::new();
            for pair in order.windows(2) {
                let (front, back) = (pair[0], pair[1]);
                let ahead = &a.enemies[front];
                let behind = &a.enemies[back];
                if ahead.condition(a.mode) == EnemyCondition::Defeated
                    || behind.condition(a.mode) == EnemyCondition::Defeated
                {
                    continue;
                }
                let gap = behind.x - ahead.x;
                let front_speed = enemy_step_speed(a, ahead);
                let back_speed = enemy_step_speed(a, behind);
                let passing =
                    (back_speed > front_speed + 0.01 && gap < 1.4 && ahead.contact_since.is_none())
                        || (front_speed > back_speed + 0.01
                            && gap < 0.9
                            && (ahead.pass_offset.abs() > 0.01 || behind.pass_offset.abs() > 0.01));
                if passing {
                    let (fast, slow) = if back_speed > front_speed {
                        (back, front)
                    } else {
                        (front, back)
                    };
                    let sign = if lane + 1 == a.lane_count { -1.0 } else { 1.0 };
                    offsets.insert(fast, sign * MAX_PASS_OFFSET);
                    offsets.insert(
                        slow,
                        if lane == 0 || lane + 1 == a.lane_count {
                            0.0
                        } else {
                            -MAX_PASS_OFFSET
                        },
                    );
                } else if gap < 0.65
                    && (ahead.pass_offset.abs() > 0.01 || behind.pass_offset.abs() > 0.01)
                {
                    offsets.entry(front).or_insert(ahead.pass_offset);
                    offsets.entry(back).or_insert(behind.pass_offset);
                }
            }
            let mut queue_front: Option<f32> = None;
            for n in order {
                let e = &a.enemies[n];
                if e.condition(a.mode) == EnemyCondition::Defeated {
                    continue;
                }
                let previous_target = e.target_structure;
                let mut stop = DEFENSE_LINE;
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
                if let Some(front) = queue_front
                    && front + 0.65 > stop
                {
                    stop = front + 0.65;
                    blocker = None;
                }
                let speed = if e.boss { 0.6 } else { enemy_profile(e).speed };
                let injury = &a.enemies[n].injury;
                let slow = if a.enemies[n].statuses.active(StatusId::HitStop)
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
                a.enemies[n].target_structure = if blocked { blocker } else { None };
                if blocked && (stop <= DEFENSE_LINE + 0.01 || blocker.is_some()) {
                    if a.enemies[n].contact_since.is_none() || previous_target != blocker {
                        a.enemies[n].contact_since = Some(a.tick);
                        let duration = seconds(enemy_profile(&a.enemies[n]).windup);
                        a.enemies[n].attack_left = duration;
                        a.enemies[n].windup_total = duration;
                        a.enemies[n].attack_phase = EnemyPhase::Windup;
                        let enemy = &a.enemies[n];
                        let point = [enemy.x, enemy.y()];
                        let meta = EventMetadata {
                            actor_id: Some(enemy.id),
                            target_id: blocker,
                            lane: Some(lane),
                            duration_ticks: duration,
                            ..Default::default()
                        };
                        a.event_with(EventKind::Contact, point, point, 0, meta);
                        a.event_with(
                            EventKind::Windup,
                            point,
                            if blocker.is_none() {
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
                    a.enemies[n].attack_phase = if blocked {
                        EnemyPhase::Queued
                    } else {
                        EnemyPhase::Advance
                    };
                }
                queue_front = blocked.then_some(a.enemies[n].x);
                let target = offsets.get(&n).copied().unwrap_or(0.0);
                let difference = target - a.enemies[n].pass_offset;
                a.enemies[n].pass_offset += difference.clamp(-2.0 * DT * slow, 2.0 * DT * slow);
            }
        }
    }
}
