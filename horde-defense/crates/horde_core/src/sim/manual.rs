//! Personal-mode manual actions: heal, brace, melee, shooting and ordnance.

use super::*;

impl Game {
    pub(super) fn manual_actions(&mut self, a: &mut Attempt, input: &Input) {
        if input.heal && !a.heal_used && a.profile.heal && a.hp < a.max_hp {
            a.heal_used = true;
            let healed = (a.max_hp - a.hp).min(35);
            a.hp = (a.hp + 35).min(a.max_hp);
            a.event(
                EventKind::Ability,
                a.defender_position(),
                a.defender_position(),
                healed,
            );
        }
        if input.aim.iter().any(|value| !value.is_finite()) {
            return;
        }
        if input.brace
            && a.profile.brace
            && !a.statuses.active(StatusId::Brace)
            && a.stamina >= 30.0
        {
            a.stamina -= 30.0;
            a.statuses.refresh(StatusId::Brace, BRACE_DURATION_TICKS);
            a.event(
                EventKind::Ability,
                a.defender_position(),
                a.defender_position(),
                0,
            );
        }
        let lane = input.aim[1].floor().clamp(0.0, f32::from(a.lane_count - 1)) as u8;
        // A hostile a death burst defeated waits on the field for its reap
        // (W2-29); a strike never lands on it.
        let target = a
            .enemies
            .iter()
            .position(|e| e.lane == lane && e.contact() && !enemy_defeated(a, e));
        if input.melee
            && a.melee_cooldown == 0
            && a.melee_commit == 0
            && a.stamina >= 25.0
            && let Some(n) = target
        {
            a.telemetry.canceled_reload_ticks += u64::from(a.reload_left);
            a.reload_left = 0;
            a.stamina -= 25.0;
            a.melee_cooldown = 72;
            a.melee_commit = 30;
            let damage = (a.profile.personal.melee_damage
                - enemy_armor_vs(a, &a.enemies[n], HitSource::Melee))
            .max(0);
            let point = [a.enemies[n].x, a.enemies[n].y()];
            a.enemies[n].apply_damage(damage, a.mode);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Defender, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            let meta = EventMetadata {
                target_id: Some(a.enemies[n].id),
                lane: Some(lane),
                ..Default::default()
            };
            a.event_with(EventKind::Melee, a.defender_position(), point, damage, meta);
            if damage == 0 {
                a.telemetry.absorbed += 1;
                a.event_with(EventKind::Armor, a.defender_position(), point, 0, meta);
            }
            if a.enemies[n].boss && enemy_defeated(a, &a.enemies[n]) {
                return;
            }
            if a.profile.sweep {
                let mut total = 0;
                let targets: Vec<_> = a
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        e.contact() && e.lane.abs_diff(lane) == 1 && !enemy_defeated(a, e)
                    })
                    .map(|(n, _)| n)
                    .collect();
                for n in targets {
                    let secondary = (a.profile.personal.melee_damage / 2
                        - enemy_armor_vs(a, &a.enemies[n], HitSource::Melee))
                    .max(0);
                    let e = &mut a.enemies[n];
                    if e.contact() && e.lane.abs_diff(lane) == 1 {
                        e.apply_damage(secondary, a.mode);
                        total += secondary;
                    }
                }
                a.active_damage += total as u64;
                a.dps
                    .record(a.tick, crate::stats::DamageSource::Defender, total as u64);
                a.telemetry.defender_damage += total as u64;
            }
            return;
        }
        if a.melee_commit > 0 {
            return;
        }
        if let Some(request) = input.throw {
            self.throw_ordnance(a, request);
            return;
        }
        if (input.reload || (input.auto_reload && a.ammo == 0))
            && a.reload_left == 0
            && a.ammo < a.magazine
        {
            a.reload_left = seconds(a.profile.personal.reload.max(0.5));
            return;
        }
        if !input.fire || a.shot_cooldown > 0 || a.reload_left > 0 || a.ammo == 0 {
            return;
        }
        let dx = input.aim[0] - a.defender_position()[0];
        let dy = input.aim[1] - a.defender_position()[1];
        let length = dx.hypot(dy);
        if length < 0.01 {
            return;
        }
        let base_angle = dy.atan2(dx);
        let pellets = a.profile.weapon.pellets;
        let count = if a.profile.pierce { 2 } else { 1 };
        // A deterministic fan consumes one shell. Damage is aggregated by stable
        // enemy ID before applying the per-trigger pistol cap and injury feedback.
        let mut impacts = std::collections::BTreeMap::<u64, (usize, i32, bool)>::new();
        a.ammo -= 1;
        a.shots += 1;
        a.shot_cooldown = seconds(a.profile.personal.cadence.max(0.08));
        for pellet in 0..pellets {
            let offset = if pellets == 1 {
                0.0
            } else {
                pellet as f32 / (pellets - 1) as f32 - 0.5
            } * a.profile.weapon.spread_radians;
            let angle = base_angle + offset;
            let direction = [angle.cos(), angle.sin()];
            let mut candidates = Vec::new();
            for (n, e) in a.enemies.iter().enumerate() {
                if !e.in_view() || e.hp <= 0 {
                    continue;
                }
                let v = [
                    e.x - a.defender_position()[0],
                    e.y() - a.defender_position()[1],
                ];
                let along = v[0] * direction[0] + v[1] * direction[1];
                let perpendicular = (v[0] * direction[1] - v[1] * direction[0]).abs();
                let radius = if e.boss { 0.55 } else { 0.34 };
                if along > 0.0 && along < 45.0 && perpendicular <= radius {
                    candidates.push((n, along, perpendicular));
                }
            }
            candidates.sort_by(|x, y| {
                x.1.total_cmp(&y.1)
                    .then(a.enemies[x.0].id.cmp(&a.enemies[y.0].id))
            });
            let endpoint = candidates.iter().take(count).next_back().map_or(
                [
                    a.defender_position()[0] + direction[0] * 40.0,
                    a.defender_position()[1] + direction[1] * 40.0,
                ],
                |&(n, _, _)| [a.enemies[n].x, a.enemies[n].y()],
            );
            a.event(EventKind::Shot, a.defender_position(), endpoint, 0);
            if candidates.is_empty() {
                a.event(EventKind::Miss, a.defender_position(), endpoint, 0);
            }
            for &(n, _, perpendicular) in candidates.iter().take(count) {
                let weak = perpendicular < 0.12;
                let raw = (a.profile.personal.damage as f32
                    * if weak {
                        1.0 + a.profile.personal.weak_bonus
                    } else {
                        1.0
                    })
                .round() as i32;
                // Penetration lowers the armor of personal shots only, ranged
                // armor included, never below zero (melee, ordnance and
                // turrets ignore it).
                let armor = (enemy_armor_vs(a, &a.enemies[n], HitSource::Shot)
                    - a.profile.modifiers.armor_penetration)
                    .max(0);
                let damage = (raw - armor).max(0);
                let impact = impacts.entry(a.enemies[n].id).or_insert((n, 0, false));
                impact.1 = impact.1.saturating_add(damage);
                impact.2 |= weak;
            }
        }
        if impacts.is_empty() {
            a.telemetry.misses += 1;
        }
        let mut impacts: Vec<_> = impacts.into_iter().collect();
        impacts.sort_by(|(left_id, (left, _, _)), (right_id, (right, _, _))| {
            let distance = |index: usize| {
                (a.enemies[index].x - a.defender_position()[0])
                    .hypot(a.enemies[index].y() - a.defender_position()[1])
            };
            distance(*left)
                .total_cmp(&distance(*right))
                .then(left_id.cmp(right_id))
        });
        for (_, (n, damage, weak)) in impacts {
            let damage = damage.min(a.profile.weapon.hit_cap(a.enemies[n].max_hp));
            a.enemies[n].apply_damage(damage, a.mode);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Defender, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            if damage == 0 {
                a.telemetry.absorbed += 1;
            } else if weak {
                a.telemetry.weak_hits += 1;
            }
            a.hits += 1;
            let pos = [a.enemies[n].x, a.enemies[n].y()];
            let meta = EventMetadata {
                target_id: Some(a.enemies[n].id),
                lane: Some(a.enemies[n].lane),
                ..Default::default()
            };
            a.event_with(
                if damage == 0 {
                    EventKind::Armor
                } else if weak {
                    EventKind::WeakPoint
                } else {
                    EventKind::Hit
                },
                a.defender_position(),
                pos,
                damage,
                meta,
            );
            if a.enemies[n].boss && enemy_defeated(a, &a.enemies[n]) {
                break;
            }
        }
    }

    fn throw_ordnance(&mut self, a: &mut Attempt, request: ThrowRequest) {
        let kind = request.kind;
        if a.mode != Mode::Personal
            || !ordnance::valid_cell_for_arena(request.cell, a.lane_count, a.width_yards)
            || !a.ordnance.ready(kind)
            || a.reload_left > 0
            || a.melee_commit > 0
            || a.ordnance.projectiles.len() + a.ordnance.fire_zones.len()
                >= ordnance::MAX_ACTIVE_ORDNANCE
            || !a.profile.modifiers.unlocked(request.kind.unlock())
        {
            return;
        }
        let definition = ordnance::catalog().definition(request.kind);
        let modifiers = &a.profile.modifiers;
        let damage_multiplier = modifiers.ordnance_damage_multiplier
            * a.modifiers.personal_damage
            * a.progression.fortification_multiplier;
        let projectile = ThrownOrdnance {
            id: a.next_id,
            kind: request.kind,
            cell: request.cell,
            flight_left: definition.flight_ticks,
            flight_ticks: definition.flight_ticks,
            damage: (definition.damage as f32 * damage_multiplier)
                .round()
                .clamp(1.0, 100_000.0) as i32,
            radius: (definition.radius * modifiers.ordnance_radius_multiplier).clamp(0.1, 15.0),
            edge_damage_fraction: definition.edge_damage_fraction,
            burn_damage: if definition.burn_damage > 0 {
                (definition.burn_damage as f32 * damage_multiplier)
                    .round()
                    .clamp(1.0, 100_000.0) as i32
            } else {
                0
            },
            burn_ticks: if definition.burn_ticks > 0 {
                (definition.burn_ticks as f32 * modifiers.burn_duration_multiplier)
                    .round()
                    .clamp(1.0, 3600.0) as u32
            } else {
                0
            },
            burn_interval: definition.burn_interval,
            release_cues: Some(ordnance::ReleaseCues {
                tick: a.tick,
                brace: a.statuses.active(StatusId::Brace),
                last_shot_tick: a
                    .events
                    .iter()
                    .rev()
                    .find(|event| {
                        event.kind == EventKind::Shot
                            && event.actor_id.is_none()
                            && event.tick <= a.tick
                    })
                    .map(|event| event.tick),
                last_hit_tick: a
                    .events
                    .iter()
                    .rev()
                    .find(|event| {
                        event.kind == EventKind::Damage
                            && event.target_id.is_none()
                            && event.value > 0
                            && event.tick <= a.tick
                    })
                    .map(|event| event.tick),
            }),
        };
        a.next_id += 1;
        a.ordnance.next_id += 1;
        a.ordnance.charges[kind] -= 1;
        a.ordnance.spent[kind] += 1;
        a.ordnance.cooldowns[kind] = (definition.cooldown_ticks as f32
            * modifiers.ordnance_cooldown_multiplier)
            .round()
            .clamp(1.0, 3600.0) as u32;
        a.event_with(
            EventKind::Throw,
            a.defender_position(),
            ordnance::cell_center(request.cell),
            kind.event_value(),
            EventMetadata {
                actor_id: Some(projectile.id),
                lane: Some(request.cell[1]),
                duration_ticks: projectile.flight_ticks,
                ordnance: Some(projectile.kind),
                ..Default::default()
            },
        );
        a.ordnance.projectiles.push(projectile);
    }
    pub(super) fn advance_ordnance(&mut self, a: &mut Attempt) {
        for cooldown in a.ordnance.cooldowns.values_mut() {
            *cooldown = cooldown.saturating_sub(1);
        }
        let mut pulses = Vec::new();
        for zone in &mut a.ordnance.fire_zones {
            zone.ticks_left = zone.ticks_left.saturating_sub(1);
            zone.tick_left = zone.tick_left.saturating_sub(1);
            if zone.tick_left == 0 {
                pulses.push((zone.id, zone.cell, zone.radius, zone.damage));
                zone.tick_left = zone.tick_interval;
            }
        }
        a.ordnance.fire_zones.retain(|z| z.ticks_left > 0);
        for (id, cell, radius, damage) in pulses {
            self.ordnance_area_damage(
                a,
                (id, OrdnanceKind::Molotov, HitSource::Fire),
                cell,
                radius,
                damage,
                1.0,
            );
        }
        let mut impacts = Vec::new();
        for projectile in &mut a.ordnance.projectiles {
            projectile.flight_left = projectile.flight_left.saturating_sub(1);
            if projectile.flight_left == 0 {
                impacts.push(projectile.clone());
            }
        }
        a.ordnance.projectiles.retain(|p| p.flight_left > 0);
        for projectile in impacts {
            let point = ordnance::cell_center(projectile.cell);
            // Area-event value is radius in hundredths of a yard. Individual
            // injury events below carry damage and the resolved hostile ID.
            a.event_with(
                EventKind::Explosion,
                a.defender_position(),
                point,
                (projectile.radius * 100.0).round() as i32,
                EventMetadata {
                    actor_id: Some(projectile.id),
                    lane: Some(projectile.cell[1]),
                    duration_ticks: 30,
                    ordnance: Some(projectile.kind),
                    ..Default::default()
                },
            );
            self.ordnance_area_damage(
                a,
                (projectile.id, projectile.kind, HitSource::Area),
                projectile.cell,
                projectile.radius,
                projectile.damage,
                projectile.edge_damage_fraction,
            );
            if projectile.kind == OrdnanceKind::Molotov {
                a.event_with(
                    EventKind::Burn,
                    a.defender_position(),
                    point,
                    (projectile.radius * 100.0).round() as i32,
                    EventMetadata {
                        actor_id: Some(projectile.id),
                        lane: Some(projectile.cell[1]),
                        duration_ticks: projectile.burn_ticks,
                        ordnance: Some(OrdnanceKind::Molotov),
                        ..Default::default()
                    },
                );
                a.ordnance.fire_zones.push(FireZone {
                    id: projectile.id,
                    cell: projectile.cell,
                    radius: projectile.radius,
                    damage: projectile.burn_damage,
                    ticks_left: projectile.burn_ticks,
                    tick_left: projectile.burn_interval,
                    tick_interval: projectile.burn_interval,
                });
            }
        }
    }
    fn ordnance_area_damage(
        &mut self,
        a: &mut Attempt,
        (source, ordnance, hit): (u64, OrdnanceKind, HitSource),
        cell: [u8; 2],
        radius: f32,
        raw_damage: i32,
        edge_fraction: f32,
    ) {
        let center = ordnance::cell_center(cell);
        let mut targets: Vec<_> = a
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| {
                enemy.hp > 0
                    && enemy.in_view()
                    && (enemy.x - center[0]).hypot(enemy.y() - center[1]) <= radius
            })
            .map(|(index, enemy)| (enemy.id, index))
            .collect();
        targets.sort_unstable_by_key(|(id, _)| *id);
        for (_, index) in targets {
            let enemy = &a.enemies[index];
            let point = [enemy.x, enemy.y()];
            let distance = (point[0] - center[0]).hypot(point[1] - center[1]);
            let raw = (raw_damage as f32 * (1.0 - (1.0 - edge_fraction) * (distance / radius)))
                .round() as i32;
            let damage = (raw - enemy_armor_vs(a, enemy, hit)).max(0);
            let meta = EventMetadata {
                actor_id: Some(source),
                target_id: Some(enemy.id),
                lane: Some(enemy.lane),
                ordnance: Some(ordnance),
                ..Default::default()
            };
            a.enemies[index].apply_damage(damage, Mode::Personal);
            a.active_damage += damage as u64;
            a.dps
                .record(a.tick, crate::stats::DamageSource::Ordnance, damage as u64);
            a.telemetry.defender_damage += damage as u64;
            if damage == 0 {
                a.telemetry.absorbed += 1;
            }
            a.event_with(
                if damage == 0 {
                    EventKind::Armor
                } else {
                    EventKind::Hit
                },
                center,
                point,
                damage,
                meta,
            );
        }
    }
}
