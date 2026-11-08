//! Structure behaviour: turrets and traps acting each tick.

use super::*;

impl Game {
    pub(super) fn structures_act(&mut self, a: &mut Attempt) {
        for n in 0..a.structures.len() {
            a.structures[n].cooldown = a.structures[n].cooldown.saturating_sub(1);
            if a.mode == Mode::Npc && a.tick.is_multiple_of(60) && a.structures[n].ready_turns > 0 {
                a.structures[n].ready_turns -= 1;
                continue;
            }
            if a.structures[n].kind == StructureKind::Wall || a.structures[n].cooldown > 0 {
                continue;
            }
            if a.mode == Mode::Npc && !a.tick.is_multiple_of(60) {
                continue;
            }
            let s = &a.structures[n];
            let d = content().structure(s.kind);
            let origin = [s.x(), s.y()];
            let mut candidates: Vec<usize> = a
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if (a.mode == Mode::Personal && e.hp <= 0)
                        || (a.mode == Mode::Npc && e.injury.terminal())
                    {
                        return false;
                    }
                    if s.kind == StructureKind::Trap {
                        s.charges > 0 && e.lane == s.lane && (e.x - s.x()).abs() <= 0.65
                    } else {
                        (e.x - s.x()).hypot(e.y() - s.y()) <= d.range
                    }
                })
                .map(|(n, _)| n)
                .collect();
            candidates.sort_by(|&n, &m| {
                a.enemies[n]
                    .x
                    .total_cmp(&a.enemies[m].x)
                    .then(a.enemies[n].id.cmp(&a.enemies[m].id))
            });
            let Some(&target) = candidates.first() else {
                continue;
            };
            a.structures[n].cooldown =
                seconds(d.cooldown).max(if a.mode == Mode::Npc { 60 } else { 1 });
            if a.structures[n].kind == StructureKind::Trap {
                a.structures[n].charges -= 1;
            }
            let point = [a.enemies[target].x, a.enemies[target].y()];
            let meta = EventMetadata {
                actor_id: Some(a.structures[n].id),
                target_id: Some(a.enemies[target].id),
                lane: Some(a.enemies[target].lane),
                ..Default::default()
            };
            a.event_with(EventKind::Shot, origin, point, 0, meta);
            let bonus = structure_bonus(a, self.campaign.structure_upgrades[d.kind]);
            let hit = if d.kind == StructureKind::Trap {
                HitSource::Trap
            } else {
                HitSource::Turret
            };
            let damage = if a.mode == Mode::Personal {
                let damage = ((d.damage as f32 * bonus).round() as i32
                    - enemy_armor_vs(a, &a.enemies[target], hit))
                .max(0);
                if damage == 0 {
                    a.telemetry.absorbed += 1;
                }
                a.event_with(
                    if damage == 0 {
                        EventKind::Armor
                    } else {
                        EventKind::Hit
                    },
                    origin,
                    point,
                    damage,
                    meta,
                );
                damage
            } else {
                let e = &a.enemies[target];
                let distance = (point[0] - origin[0]).hypot(point[1] - origin[1]);
                let attack = Attack {
                    skill: d.npc_skill + rules::range_penalty(distance),
                    damage: Damage {
                        dice: d.npc.damage_dice,
                        adds: d.npc.damage_add + ((bonus - 1.0) * 4.0).round() as i32,
                        kind: d.npc.damage_kind,
                    },
                    armor_divisor: 1,
                };
                let dodge = e.injury.dodge(enemy_dodge(a, e));
                let armor = enemy_armor_vs(a, e, hit);
                let ht = enemy_ht(a, e);
                let result = rules::resolve_attack(
                    &mut a.rng,
                    attack,
                    Some(dodge),
                    armor,
                    &mut a.enemies[target].injury,
                    ht,
                );
                if result.requires_ready {
                    a.structures[n].ready_turns = 1;
                }
                a.resolution_event(&result, origin, point, meta, true);
                a.resolved(
                    meta.actor_id,
                    meta.target_id,
                    &result,
                    format!(
                        "{} → lane {}: {}",
                        d.name,
                        a.enemies[target].lane + 1,
                        result.explanation()
                    ),
                );
                result.injury
            };
            a.enemies[target].apply_damage(damage, a.mode);
            a.structure_damage += damage.max(0) as u64;
            a.dps.record(
                a.tick,
                crate::stats::DamageSource::Structures,
                damage.max(0) as u64,
            );
            if a.structures[n].kind == StructureKind::Trap {
                a.telemetry.trap_damage += damage.max(0) as u64;
            } else {
                a.telemetry.turret_damage += damage.max(0) as u64;
            }
            if a.enemies[target].boss && enemy_defeated(a, &a.enemies[target]) {
                return;
            }
        }
    }
}
