//! NPC survivor decisions and target selection.

use super::*;

impl Game {
    pub(super) fn npc_turn(&mut self, a: &mut Attempt) {
        a.defense_bonus = 0;
        a.maneuver = NpcManeuver::DoNothing;
        a.decision_target = None;
        if !a.injury.start_turn(&mut a.rng, a.profile.stats.ht) {
            a.hp = a.injury.hp;
            a.decision = "Incapacitated · unable to begin a maneuver".into();
            return;
        }
        if a.injury.stunned {
            a.maneuver = NpcManeuver::Recover;
            a.injury.recover_stun(&mut a.rng, a.profile.stats.ht);
            a.decision = "Do Nothing · recover from stun".into();
            a.injury.clear_shock();
            return;
        }
        if a.injury.prone {
            a.maneuver = NpcManeuver::Posture;
            a.injury.prone = false;
            a.decision = "Change Posture · stand in place".into();
            a.injury.clear_shock();
            return;
        }
        if a.injury.weapon_dropped {
            a.maneuver = NpcManeuver::Ready;
            a.injury.weapon_dropped = false;
            a.decision = "Ready · retrieve dropped weapon".into();
            a.injury.clear_shock();
            return;
        }
        if !a
            .injury
            .fatigue_allows_action(&mut a.rng, a.profile.stats.iq)
        {
            a.maneuver = NpcManeuver::Recover;
            a.decision = "Do Nothing · exhausted".into();
            a.injury.clear_shock();
            return;
        }
        if a.ready_turns > 0 {
            a.maneuver = NpcManeuver::Ready;
            a.ready_turns -= 1;
            a.decision = "Ready · recover weapon".into();
            a.injury.clear_shock();
            return;
        }
        if a.reload_left > 0 {
            a.maneuver = NpcManeuver::Reload;
            a.reload_ticks += 60;
            a.reload_left = a.reload_left.saturating_sub(60);
            a.decision = format!(
                "Ready · reload, {} turns remaining",
                a.reload_left.div_ceil(60)
            );
            if a.reload_left == 0 {
                a.ammo = a.magazine;
                a.event(
                    EventKind::Reload,
                    a.defender_position(),
                    a.defender_position(),
                    0,
                );
            }
            a.injury.clear_shock();
            return;
        }
        let policy = self.defender_npc(a.seat).policy;
        let target = select_npc_target(a, policy);
        let Some(target) = target else {
            a.decision = "Do Nothing · watching the approach".into();
            a.aim_target = None;
            a.aim_seconds = 0;
            a.injury.clear_shock();
            return;
        };
        let e = &a.enemies[target];
        a.decision_target = Some(e.id);
        let contact = e.contact();
        let id = e.id;
        let lane = e.lane;
        let imminent = a.enemies.iter().any(|e| e.x < 5.0);
        let melee = contact && (a.ammo == 0 || policy == Policy::Guardian);
        if melee != a.npc_holding_melee {
            a.maneuver = NpcManeuver::Ready;
            a.npc_holding_melee = melee;
            a.aim_target = None;
            a.aim_seconds = 0;
            a.decision = if melee {
                "Ready · sling rifle and draw melee weapon".into()
            } else {
                "Ready · ready rifle".into()
            };
            a.injury.clear_shock();
            return;
        }
        if (a.ammo == 0
            || (self.defender_npc(a.seat).early_reload && a.ammo <= a.magazine / 4 && !imminent))
            && !melee
        {
            a.maneuver = NpcManeuver::Reload;
            a.reload_left = a.profile.npc.reload_turns.saturating_sub(1) * 60;
            a.reload_ticks += 60;
            a.decision = format!("Ready · reload for {} turns", a.profile.npc.reload_turns);
            if a.reload_left == 0 {
                a.ammo = a.magazine;
            }
            a.aim_target = None;
            a.aim_seconds = 0;
            a.injury.clear_shock();
            return;
        }
        let current_range = if contact {
            1.0
        } else {
            (e.x - a.defender_position()[0]).hypot(e.y() - a.defender_position()[1])
        };
        if !melee {
            // This preference limits action selection, never the dice. Even a
            // risk-taking policy cannot make an illegal skill-below-3 attack.
            let minimum_skill = if self.defender_npc(a.seat).allow_risk {
                3
            } else {
                8
            };
            let aimed_seconds = if a.aim_target == Some(id) {
                a.aim_seconds
            } else {
                0
            };
            let base_skill = a.profile.ranged_skill
                + a.profile.npc.skill_bonus
                + rules::range_penalty(current_range)
                - a.injury.shock;
            let effective_skill =
                base_skill + rules::aim_bonus(a.profile.npc.accuracy, aimed_seconds);
            if effective_skill < minimum_skill {
                let fully_aimed_skill = base_skill + rules::aim_bonus(a.profile.npc.accuracy, 3);
                if aimed_seconds < 3 && fully_aimed_skill >= minimum_skill {
                    a.maneuver = NpcManeuver::Aim;
                    a.aim_target = Some(id);
                    a.aim_seconds = aimed_seconds + 1;
                    a.aim_ticks += 60;
                    a.decision = format!(
                        "Aim at {} in lane {} · cautious policy: attack skill {} below {}",
                        enemy_name(a, e),
                        lane + 1,
                        effective_skill,
                        minimum_skill
                    );
                } else {
                    a.aim_target = None;
                    a.aim_seconds = 0;
                    a.decision = format!(
                        "Do Nothing · wait for a safer shot in lane {}: attack skill {} below {}",
                        lane + 1,
                        effective_skill,
                        minimum_skill
                    );
                    if a.enemies
                        .iter()
                        .any(|e| e.contact() && !e.injury.terminal())
                    {
                        a.maneuver = NpcManeuver::AllOutDefense;
                        a.defense_bonus = 2;
                        a.decision = format!(
                            "All-Out Defense · Dodge +2 until next turn; wait for a safer shot in lane {} (skill {})",
                            lane + 1,
                            effective_skill
                        );
                    }
                }
                a.injury.clear_shock();
                return;
            }
        }
        let desired_aim = if policy == Policy::Marksman && !imminent {
            2
        } else {
            1
        };
        let aimed = if a.aim_target == Some(id) {
            a.aim_seconds
        } else {
            0
        };
        let wants_aim = self.defender_npc(a.seat).prefer_aim
            && (policy == Policy::Marksman || (!imminent && !contact));
        let aim = if a.drop_tier.is_none() {
            // An attempt deployed before save format 7 keeps the old test
            // (ruling R4's byte-identical legacy path): aim below effective
            // skill 14, the Marksman always, up to the desired seconds.
            let useful_aim = a.profile.ranged_skill
                + a.profile.npc.skill_bonus
                + rules::range_penalty(current_range)
                < 14;
            wants_aim
                && (policy == Policy::Marksman || useful_aim)
                && (a.aim_target != Some(id) || a.aim_seconds < desired_aim)
        } else {
            // An aim is useful only when it raises the expected hits per
            // turn (W4-14): every policy, the Marksman too, weighs the turn
            // it costs.
            wants_aim
                && aim_turns_that_pay(
                    a.profile.ranged_skill
                        + a.profile.npc.skill_bonus
                        + rules::range_penalty(current_range)
                        - a.injury.shock,
                    a.profile.npc.accuracy,
                    aimed,
                    desired_aim,
                ) > 0
        };
        if !melee && aim {
            a.maneuver = NpcManeuver::Aim;
            if a.aim_target != Some(id) {
                a.aim_seconds = 0;
            }
            a.aim_target = Some(id);
            a.aim_seconds += 1;
            a.aim_ticks += 60;
            a.decision = format!(
                "Aim at {} in lane {} · {}s",
                enemy_name(a, e),
                lane + 1,
                a.aim_seconds
            );
            a.injury.clear_shock();
            return;
        }
        let range = if contact {
            1.0
        } else {
            (e.x - a.defender_position()[0]).hypot(e.y() - a.defender_position()[1])
        };
        let skill = if melee {
            a.profile.melee_skill
        } else {
            a.profile.ranged_skill
                + a.profile.npc.skill_bonus
                + rules::range_penalty(range)
                + rules::aim_bonus(
                    a.profile.npc.accuracy,
                    if a.aim_target == Some(id) {
                        a.aim_seconds
                    } else {
                        0
                    },
                )
        };
        let skill = skill - a.injury.shock;
        let damage = if melee {
            let full = Damage {
                dice: a.profile.npc_melee_damage_dice,
                adds: a.profile.npc_melee_damage_add,
                kind: a.profile.npc_melee_kind,
            };
            if a.injury.fp * 3 < a.injury.max_fp {
                rules::rescale_swing(full, a.profile.stats.st, (a.profile.stats.st + 1) / 2)
                    .expect("validated human ST and melee profile")
            } else {
                full
            }
        } else {
            Damage {
                dice: a.profile.npc.damage_dice,
                adds: a.profile.npc.damage_add,
                kind: a.profile.npc.damage_kind,
            }
        };
        let attack = Attack {
            skill,
            damage,
            armor_divisor: 1,
        };
        let armor = enemy_armor_vs(
            a,
            e,
            if melee {
                HitSource::Melee
            } else {
                HitSource::Shot
            },
        );
        let dodge = e.injury.dodge(enemy_dodge(a, e));
        let ht = enemy_ht(a, e);
        let point = [e.x, e.y()];
        let target_name = enemy_name(a, e);
        a.maneuver = NpcManeuver::Attack;
        let mut result = rules::resolve_attack(
            &mut a.rng,
            attack,
            Some(dodge),
            armor,
            &mut a.enemies[target].injury,
            ht,
        );
        a.enemies[target].log_hit(&mut result);
        if !melee {
            a.ammo -= 1;
            a.shots += 1;
        }
        if result.hit {
            a.hits += 1;
            if !melee {
                self.record_defender_progress(a.seat, 1, 0);
            }
        }
        if result.requires_ready {
            a.ready_turns = 1;
        }
        a.enemies[target].apply_damage(result.injury, a.mode);
        a.active_damage += result.injury.max(0) as u64;
        a.dps.record(
            a.tick,
            crate::stats::DamageSource::Defender,
            result.injury.max(0) as u64,
        );
        a.telemetry.defender_damage += result.injury.max(0) as u64;
        a.resolved(
            None,
            Some(id),
            &result,
            format!(
                "{} {} in lane {}: {}",
                if melee { "Strike" } else { "Shoot" },
                target_name,
                lane + 1,
                result.explanation()
            ),
        );
        a.decision = format!(
            "{} {} in lane {} · {}",
            if melee {
                "Attack: strike"
            } else {
                "Attack: shoot"
            },
            target_name,
            lane + 1,
            if contact {
                "close engagement"
            } else {
                "nearest visible threat"
            }
        );
        let meta = EventMetadata {
            target_id: Some(id),
            lane: Some(lane),
            ..Default::default()
        };
        a.event_with(
            if melee {
                EventKind::Melee
            } else {
                EventKind::Shot
            },
            a.defender_position(),
            point,
            0,
            meta,
        );
        a.resolution_event(&result, a.defender_position(), point, meta, true);
        a.aim_target = None;
        a.aim_seconds = 0;
        a.injury.clear_shock();
        a.hp = a.injury.hp;
        a.stamina = a.injury.fp as f32;
    }
}

/// The further Aim turns, at most `most - aimed`, after which one shot
/// lands the most expected hits per turn spent, for a shooter at effective
/// `skill` (before any aim bonus) with weapon `accuracy` who has already
/// aimed `aimed` seconds at the target. 0 means shoot now; ties shoot
/// sooner. The target's dodge scales every option alike, so it cancels.
/// Pure: reads no RNG. The Npc `defender_power` model reads it too (W4-16).
pub(super) fn aim_turns_that_pay(skill: i32, accuracy: i32, aimed: u8, most: u8) -> u8 {
    let rate = |more: u8| {
        rules::attack_success_probability(skill + rules::aim_bonus(accuracy, aimed + more))
            / f32::from(more + 1)
    };
    (1..=most.saturating_sub(aimed)).fold(
        0,
        |best, more| {
            if rate(more) > rate(best) { more } else { best }
        },
    )
}

fn select_npc_target(a: &Attempt, policy: Policy) -> Option<usize> {
    if !a
        .enemies
        .iter()
        .any(|e| e.contact() && !e.injury.terminal())
        && let Some(n) = a
            .enemies
            .iter()
            .position(|e| Some(e.id) == a.aim_target && !e.injury.terminal() && e.in_view())
    {
        return Some(n);
    }
    let mut targets: Vec<usize> = a
        .enemies
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.injury.terminal() && e.in_view())
        .map(|(n, _)| n)
        .collect();
    targets.sort_by(|&n, &m| {
        let score = |e: &Enemy| {
            let mut urgency = e.x;
            if e.contact() {
                urgency -= 50.0;
            }
            if e.injury.stunned || e.injury.prone {
                urgency += 15.0;
            }
            if e.injury.hp <= 0 {
                urgency += 8.0;
            }
            // Skip a zero bias so the score keeps its exact float value.
            let bias = enemy_profile(e).target_urgency;
            if bias != 0.0 {
                urgency += bias;
            }
            if e.boss {
                urgency -= if policy == Policy::BossFocus && a.hp * 2 > a.max_hp {
                    65.0
                } else if policy == Policy::BossFocus {
                    30.0
                } else {
                    12.0
                };
            }
            if policy == Policy::Guardian && e.target_structure.is_some() {
                urgency -= 10.0;
            }
            urgency
        };
        score(&a.enemies[n])
            .total_cmp(&score(&a.enemies[m]))
            .then(a.enemies[n].id.cmp(&a.enemies[m].id))
    });
    targets.first().copied()
}

pub(super) fn structure_bonus(a: &Attempt, rank: u32) -> f32 {
    (1.0 + rank as f32 * 0.2)
        * a.progression.fortification_multiplier
        * if a.profile.engineering && !a.profile.modifiers.engineering {
            1.25
        } else {
            a.profile.modifiers.engineering_multiplier
        }
}

#[cfg(test)]
mod tests {
    use super::aim_turns_that_pay;

    /// W4-14: the further Aim turns that land the most expected hits per
    /// turn, from `rules::attack_success_probability` (3d6 with criticals).
    #[test]
    fn aim_pays_only_when_it_raises_expected_hits_per_turn() {
        // Acc 5. Skill 9: one aimed shot (14, 90.7%) beats two unaimed ones
        // (2 x 37.5%). Skill 10: two unaimed (2 x 50.0%) beat one aimed
        // (15, 95.4%), so the old threshold (aim below 14) aimed at 10-13
        // and lost hits.
        assert_eq!(aim_turns_that_pay(9, 5, 0, 1), 1);
        assert_eq!(aim_turns_that_pay(10, 5, 0, 1), 0);
        assert_eq!(aim_turns_that_pay(13, 5, 0, 1), 0);
        // The Marksman's second second (skill 15 over three turns, 31.8%
        // per turn) loses to the first (45.4%), and once aimed, shooting
        // now (90.7%) beats aiming on (47.7% per turn).
        assert_eq!(aim_turns_that_pay(9, 5, 0, 2), 1);
        assert_eq!(aim_turns_that_pay(9, 5, 1, 2), 0);
        // Only a desperate shot takes both seconds: Acc 1 at skill 4
        // (1.9%) aims to 6 (9.3% over three turns, 3.1% per turn).
        assert_eq!(aim_turns_that_pay(4, 1, 0, 2), 2);
        // Nothing left to aim: shoot.
        assert_eq!(aim_turns_that_pay(4, 1, 2, 2), 0);
    }
}
