//! W2-27 (design enemy-balance §4 "Spitter", acceptance G6.6): the ranged
//! stop, walls that intercept spit, the NPC range penalty and the far-half
//! guard. Every test injects a test-only profile so the mechanism is pinned
//! apart from the shipped records: the composition (W2-31) authored the
//! spitter's `reach` 4.0 in `enemy_catalog.json`.

use super::*;

/// A legacy game with an unpaused attempt deployed in `mode`.
fn game(mode: Mode) -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.campaign.blueprints = KindMap::filled(true);
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    g
}

/// FNV-1a of the per-tick enemy, structure and defender state of one
/// fixed-seed battle on a 12-yard field with walls, until it ends or `ticks`
/// pass. Hostiles fight both at the line and at a wall face.
fn battle_trace(mode: Mode, ticks: u32) -> u64 {
    let mut g = game(mode);
    g.set_paused(true);
    g.attempt.as_mut().unwrap().width_yards = 12.0;
    for (lane, column) in [(3, 0), (5, 2), (7, 4)] {
        g.place(StructureKind::Wall, lane, column).unwrap();
    }
    // Durable brutes reach the walls in lanes 3 and 5 and the line in 1 and 9
    // whatever the defender does.
    let mut a = g.attempt.take().unwrap();
    for lane in [1, 3, 5, 9] {
        g.spawn_enemy(&mut a, EnemyKind::Brute, lane, false);
        let e = a.enemies.last_mut().unwrap();
        e.x = 9.0;
        e.max_hp = 1000;
        e.hp = 1000;
        e.injury = InjuryState::new(1000, 10);
    }
    g.attempt = Some(a);
    g.set_paused(false);
    let mut bytes = Vec::new();
    let (mut at_line, mut at_wall) = (false, false);
    for tick in 0..ticks {
        g.tick(Input {
            aim: [6.0, 5.5],
            fire: tick % 90 < 30,
            ..Input::default()
        });
        let Some(a) = g.attempt.as_ref() else {
            break;
        };
        bytes.extend(a.tick.to_le_bytes());
        bytes.extend(a.hp.to_le_bytes());
        bytes.extend(a.injury.hp.to_le_bytes());
        for e in &a.enemies {
            bytes.extend(e.id.to_le_bytes());
            bytes.push(e.lane);
            bytes.extend(e.x.to_bits().to_le_bytes());
            bytes.extend(e.hp.to_le_bytes());
            bytes.push(e.attack_phase as u8);
            bytes.extend(e.attack_left.to_le_bytes());
            bytes.extend(e.contact_since.unwrap_or(u64::MAX).to_le_bytes());
            bytes.extend(e.target_structure.unwrap_or(u64::MAX).to_le_bytes());
        }
        for s in &a.structures {
            bytes.extend(s.id.to_le_bytes());
            bytes.extend(s.hp.to_le_bytes());
        }
        bytes.extend((a.events.len() as u64).to_le_bytes());
        for e in a.enemies.iter().filter(|e| e.contact_since.is_some()) {
            at_line |= e.target_structure.is_none();
            at_wall |= e.target_structure.is_some();
        }
    }
    assert!(
        at_line && at_wall,
        "{mode:?}: contact at the line and at a wall"
    );
    crate::hash::fnv1a(&bytes)
}

#[test]
fn reach_zero_profile_identical() {
    // Characterisation pin: every shipped archetype has reach 0, so the
    // ranged stop is the defense line and every enemy fights as before W2-27.
    // The digests were recorded on the parent commit (before the spitter
    // mechanics) and must not move for reach. The Npc digest was re-recorded
    // at W4-14 (gameplay): the defender aims only when the aim raises its
    // expected hits per turn; reach plays no part in that change. Again at
    // W4-14: `npc_arrival_factor` 0.9 -> 0.5 halves the Npc arrivals.
    for width in [8.0, 12.0, 20.0, 30.0] {
        for reach in [0.0, -0.0] {
            assert_eq!(ranged_stop(width, reach).to_bits(), DEFENSE_LINE.to_bits());
        }
    }
    for (mode, digest) in [
        (Mode::Personal, 0xc2bf_fafb_4c4d_9cc3),
        (Mode::Npc, 0x6e5b_1a81_1778_d533),
    ] {
        assert_eq!(
            battle_trace(mode, 3600),
            digest,
            "{mode:?} reach-0 battle trace moved"
        );
    }
}

/// A test-only ranged archetype: the Runner profile with `reach` yards of
/// reach, a hit that beats the starter armor and a sure NPC strike.
fn spitter_profile(reach: f32) -> crate::enemy_catalog::ResolvedEnemy {
    let mut p = crate::enemy_catalog::resolve_kind(EnemyKind::Runner);
    p.archetype_id = "test-spitter".into();
    p.name = "Test spitter".into();
    p.reach = reach;
    p.damage = 40;
    p.npc_skill = 16;
    p.validate().unwrap();
    p
}

/// Spawns a spitter with `reach` in `lane` at `x`; returns its id.
fn spawn_spitter(g: &mut Game, a: &mut Attempt, reach: f32, lane: u8, x: f32) -> u64 {
    g.spawn_enemy(a, EnemyKind::Runner, lane, false);
    let e = a.enemies.last_mut().unwrap();
    e.resolved = Some(spitter_profile(reach));
    e.x = x;
    e.id
}

/// A Personal game on a `width`-yard field whose defender cannot fall.
fn field(width: f32) -> Game {
    let mut g = game(Mode::Personal);
    let a = g.attempt.as_mut().unwrap();
    assert!(a.enemies.is_empty(), "nothing has spawned at deploy");
    a.width_yards = width;
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    g
}

/// The presentation events published from sequence number `seen` on;
/// advances `seen` past them.
fn new_events(a: &Attempt, seen: &mut u64) -> Vec<Event> {
    let fresh: Vec<Event> = a
        .events
        .iter()
        .filter(|e| e.seq >= *seen)
        .cloned()
        .collect();
    if let Some(last) = fresh.last() {
        *seen = last.seq + 1;
    }
    fresh
}

fn find(a: &Attempt, id: u64) -> &Enemy {
    a.enemies
        .iter()
        .find(|e| e.id == id)
        .expect("the enemy lives")
}

#[test]
fn spitter_stations_at_clamped_stop() {
    for width in [8.0, 12.0, 20.0, 30.0] {
        for reach in [4.0, 12.0] {
            // clamp(DEFENSE_LINE + reach, DEFENSE_LINE, width / 2).
            let stop: f32 = (DEFENSE_LINE + reach).min(width * 0.5);
            let case = format!("width {width}, reach {reach}");
            let mut g = field(width);
            let mut a = g.attempt.take().unwrap();
            let spitter = spawn_spitter(&mut g, &mut a, reach, 5, stop + 1.0);
            g.spawn_enemy(&mut a, EnemyKind::Runner, 5, false);
            let f = a.enemies.last_mut().unwrap();
            f.x = stop + 1.4;
            let follower = f.id;
            g.attempt = Some(a);
            let (mut seen, mut spat, mut walked_through) = (0, false, false);
            for _ in 0..3600 {
                g.tick(Input::default());
                let a = g.attempt.as_ref().expect("the defender cannot fall");
                let s = find(a, spitter);
                assert!(
                    s.x >= stop,
                    "{case}: the spitter walked past {stop} to {}",
                    s.x
                );
                for ev in new_events(a, &mut seen) {
                    if ev.kind == EventKind::Spit && ev.actor_id == Some(spitter) {
                        assert_eq!(ev.target_id, None, "{case}");
                        assert_eq!(ev.from, [stop, 5.5], "{case}");
                        assert_eq!(ev.to, a.defender_position(), "{case}");
                        assert!(ev.value > 0, "{case}: the spit wounds the defender");
                        spat = true;
                    }
                }
                // Followers walk through a stationed spitter (W1-69).
                walked_through |= find(a, follower).contact();
                if spat && walked_through {
                    break;
                }
            }
            let a = g.attempt.as_ref().unwrap();
            let s = find(a, spitter);
            assert_eq!(s.x, stop, "{case}: stationed at the clamped stop");
            assert!(
                !s.contact(),
                "{case}: a stationed spitter is not in contact"
            );
            assert!(s.contact_since.is_some(), "{case}: it attacks from there");
            assert_eq!(s.target_structure, None, "{case}: at the defender");
            assert!(spat, "{case}: the spitter never spat");
            assert!(
                walked_through,
                "{case}: the follower never reached the line"
            );
        }
    }
}

#[test]
fn wall_in_lane_intercepts_spit() {
    let reach = 4.0;
    let stop = DEFENSE_LINE + reach;
    let mut g = field(12.0);
    g.set_paused(true);
    // Column 0 sits between the station and the line; column 4's face lies
    // ahead of the station, so a spitter meets it in melee.
    g.place(StructureKind::Wall, 5, 0).unwrap();
    g.place(StructureKind::Wall, 7, 4).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    let (near, far) = (&a.structures[0], &a.structures[1]);
    assert!(near.x() + near.blocking_radius() < stop);
    let (near_wall, near_at) = (near.id, [near.x(), near.y()]);
    let (far_wall, far_face) = (far.id, far.x() + far.blocking_radius());
    assert!(far_face > stop);
    a.structures[0].hp = 100;
    a.structures[0].max_hp = 100;
    a.structures[1].hp = 1_000_000;
    a.structures[1].max_hp = 1_000_000;
    let spitter = spawn_spitter(&mut g, &mut a, reach, 5, stop + 1.0);
    let melee = spawn_spitter(&mut g, &mut a, reach, 7, far_face + 1.0);
    g.attempt = Some(a);
    let (mut seen, mut wall_spits, mut defender_spits, mut lost) = (0, 0, 0, false);
    let mut melee_hits = 0;
    for _ in 0..3600 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender cannot fall");
        assert!(find(a, spitter).x >= stop, "the spitter left its station");
        for ev in new_events(a, &mut seen) {
            if ev.kind == EventKind::StructureLost && ev.to == near_at {
                lost = true;
            }
            if ev.actor_id == Some(spitter) {
                match ev.kind {
                    EventKind::Spit if !lost => {
                        assert_eq!(ev.target_id, Some(near_wall), "the wall intercepts");
                        assert_eq!(ev.to, near_at);
                        wall_spits += 1;
                    }
                    EventKind::Spit => {
                        assert_eq!(ev.target_id, None, "the wall is gone");
                        assert_eq!(ev.to, a.defender_position());
                        defender_spits += 1;
                    }
                    EventKind::Damage | EventKind::Armor if !lost => {
                        assert_eq!(ev.target_id, Some(near_wall), "nothing passes the wall");
                    }
                    _ => {}
                }
            }
            if ev.actor_id == Some(melee) {
                assert_ne!(ev.kind, EventKind::Spit, "a wall ahead is met in melee");
                if ev.kind == EventKind::Damage {
                    assert_eq!(ev.target_id, Some(far_wall));
                    melee_hits += 1;
                }
            }
        }
        if defender_spits > 0 && melee_hits > 0 {
            break;
        }
    }
    assert_eq!(
        wall_spits, 3,
        "three 40-point spits break the 100-point wall"
    );
    assert!(
        lost && defender_spits > 0,
        "past the wall the spit reaches the defender"
    );
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(find(a, spitter).x, stop);
    let m = find(a, melee);
    assert!(
        (m.x - far_face).abs() <= 0.01,
        "melee at the wall face, at {}",
        m.x
    );
    assert_eq!(m.target_structure, Some(far_wall));
    assert!(melee_hits > 0);
}

#[test]
fn spitter_never_winds_up_or_damages_from_the_far_half() {
    for width in [8.0, 12.0, 20.0, 30.0] {
        let half = width * 0.5;
        // The longest authored reach walks in from the edge of the field.
        let mut g = field(width);
        let mut a = g.attempt.take().unwrap();
        let spitter = spawn_spitter(&mut g, &mut a, 12.0, 5, width - 0.5);
        g.attempt = Some(a);
        let (mut seen, mut spits) = (0, 0);
        for _ in 0..3600 {
            g.tick(Input::default());
            let a = g.attempt.as_ref().unwrap();
            for ev in new_events(a, &mut seen) {
                if ev.actor_id == Some(spitter)
                    && matches!(
                        ev.kind,
                        EventKind::Contact
                            | EventKind::Windup
                            | EventKind::Spit
                            | EventKind::Damage
                            | EventKind::Armor
                    )
                {
                    assert!(
                        ev.from[0] <= half + 0.01,
                        "width {width}: {:?} from {}",
                        ev.kind,
                        ev.from[0]
                    );
                    spits += usize::from(ev.kind == EventKind::Spit);
                }
            }
            if spits >= 3 {
                break;
            }
        }
        assert!(
            spits >= 3,
            "width {width}: the spitter attacks from its station"
        );
        // The resolution guard: a spitter in contact from the far half (no
        // valid play stations one there) neither winds up nor wounds. A
        // finished windup stages the strike; a recovery about to end stages
        // the next windup.
        for mode in [Mode::Personal, Mode::Npc] {
            for (phase, left) in [(EnemyPhase::Windup, 0), (EnemyPhase::Recovery, 1)] {
                let mut g = game(mode);
                let mut a = g.attempt.take().unwrap();
                a.width_yards = width;
                a.tick = 120;
                let spitter = spawn_spitter(&mut g, &mut a, 12.0, 5, half + 0.5);
                let e = a.enemies.last_mut().unwrap();
                e.contact_since = Some(60);
                e.attack_left = left;
                e.attack_phase = phase;
                let (hp, injury) = (a.hp, a.injury.clone());
                let seq = a.event_seq;
                g.enemy_attacks(&mut a);
                assert_eq!(
                    (a.hp, &a.injury),
                    (hp, &injury),
                    "width {width} {mode:?} {phase:?}"
                );
                assert!(
                    a.events
                        .iter()
                        .filter(|e| e.seq >= seq)
                        .all(|e| e.actor_id != Some(spitter)),
                    "width {width} {mode:?} {phase:?}: no windup or strike from the far half"
                );
            }
        }
    }
}

#[test]
fn npc_spitter_uses_range_penalty_and_dodge() {
    let mut dodge_rolled = false;
    for draws in 0..16 {
        let mut g = game(Mode::Npc);
        let mut a = g.attempt.take().unwrap();
        assert_eq!(a.progression.enemy_damage, 1.0);
        assert_eq!(a.balance.enemy_damage, 1.0);
        for _ in 0..draws {
            a.rng.roll3d6();
        }
        let reach = 4.0;
        let stop = (DEFENSE_LINE + reach).min(a.width_yards * 0.5);
        let lane = 2;
        a.tick = 120;
        let spitter = spawn_spitter(&mut g, &mut a, reach, lane, stop);
        let e = a.enemies.last_mut().unwrap();
        e.contact_since = Some(60);
        e.attack_left = 0;
        e.attack_phase = EnemyPhase::Windup;
        let d = spitter_profile(reach);
        let at = a.defender_position();
        let distance = (stop - at[0]).hypot(f32::from(lane) + 0.5 - at[1]);
        let penalty = rules::range_penalty(distance);
        assert!(penalty < 0, "the station is out of contact range");
        let dodge = a
            .injury
            .dodge((a.profile.stats.dx + a.profile.stats.ht) / 4 + 3)
            + a.defense_bonus;
        let (mut rng, mut injury) = (a.rng, a.injury.clone());
        let expected = rules::resolve_attack(
            &mut rng,
            Attack {
                skill: d.npc_skill + penalty,
                damage: Damage {
                    dice: d.npc_damage_dice,
                    adds: d.npc_damage_add,
                    kind: DamageKind::Crushing,
                },
                armor_divisor: 1,
            },
            Some(dodge),
            a.profile.npc.armor,
            &mut injury,
            a.profile.stats.ht,
        );
        assert_eq!(expected.defense, Some(dodge));
        dodge_rolled |= expected.defense_roll.is_some();
        g.enemy_attacks(&mut a);
        let summary = a
            .emitted
            .iter()
            .find_map(|r| match &r.event {
                SimEvent::RuleResolved {
                    attacker: Actor::Enemy { id },
                    summary,
                    ..
                } if *id == spitter => Some(summary.clone()),
                _ => None,
            })
            .expect("the spit resolves on the enemy turn");
        assert_eq!(
            summary,
            format!(
                "{} → survivor from lane {}: {}",
                d.name,
                lane + 1,
                expected.explanation()
            ),
            "draws {draws}"
        );
        assert_eq!(a.injury, injury, "draws {draws}");
        assert_eq!(a.rng, rng, "draws {draws}: one resolution's draws");
        let spit = a
            .events
            .iter()
            .find(|e| e.kind == EventKind::Spit && e.actor_id == Some(spitter))
            .expect("the spit is published");
        assert_eq!(spit.value, expected.injury);
        assert_eq!(spit.to, at);
    }
    assert!(
        dodge_rolled,
        "some spit hit and the defender rolled a dodge"
    );
}

#[test]
fn far_half_guard_is_one_predicate_for_resolution_and_aim() {
    // W2 review: the resolution guard (sim/combat.rs) and the aim accessor
    // (`Attempt::spit_target`) share `spit_from_far_half`; the far half
    // starts strictly past the middle of the field plus 0.01 yd.
    for width in [8.0, 12.0, 20.0, 30.0] {
        let mut g = field(width);
        let mut a = g.attempt.take().unwrap();
        let edge = width * 0.5 + 0.01;
        spawn_spitter(&mut g, &mut a, 12.0, 5, edge);
        a.enemies.last_mut().unwrap().contact_since = Some(0);
        let e = a.enemies.last().unwrap().clone();
        assert!(
            !spit_from_far_half(&a, &e),
            "width {width}: the edge is near"
        );
        assert!(a.spit_target(&e).is_some(), "width {width}: it aims");
        let mut far = e.clone();
        far.x = edge + 0.01;
        assert!(spit_from_far_half(&a, &far), "width {width}: past the edge");
        assert_eq!(a.spit_target(&far), None, "width {width}: it never aims");
    }
}

#[test]
fn spit_target_is_where_the_winding_spit_lands() {
    // W2-28: the accessor behind the ranged windup's aim line names the point
    // the next Spit event carries: the intercepting wall, then the defender
    // once the wall falls. A melee windup at the line or at a wall face, and
    // an advancing spitter, aim at nothing.
    let reach = 4.0;
    let stop = DEFENSE_LINE + reach;
    let mut g = field(12.0);
    g.set_paused(true);
    g.place(StructureKind::Wall, 5, 0).unwrap();
    g.place(StructureKind::Wall, 7, 4).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    a.structures[0].hp = 100;
    a.structures[0].max_hp = 100;
    a.structures[1].hp = 1_000_000;
    a.structures[1].max_hp = 1_000_000;
    let near_wall = a.structures[0].id;
    let far_face = a.structures[1].x() + a.structures[1].blocking_radius();
    let spitter = spawn_spitter(&mut g, &mut a, reach, 5, stop + 1.0);
    let melee = spawn_spitter(&mut g, &mut a, reach, 7, far_face + 1.0);
    g.spawn_enemy(&mut a, EnemyKind::Runner, 2, false);
    a.enemies.last_mut().unwrap().x = DEFENSE_LINE + 0.5;
    let runner = a.enemies.last().unwrap().id;
    g.attempt = Some(a);
    let (mut seen, mut aimed, mut spits) = (0, None, Vec::new());
    let (mut advancing, mut melee_windups, mut runner_windups) = (0, 0, 0);
    for _ in 0..3600 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender cannot fall");
        for ev in new_events(a, &mut seen) {
            if ev.kind == EventKind::Spit && ev.actor_id == Some(spitter) {
                assert_eq!(Some(ev.to), aimed, "the spit lands where it aimed");
                spits.push(ev.target_id);
            }
        }
        let s = find(a, spitter);
        if s.attack_phase == EnemyPhase::Windup {
            aimed = a.spit_target(s);
            assert!(aimed.is_some(), "a stationed spitter's windup aims");
        } else if s.contact_since.is_none() {
            assert_eq!(a.spit_target(s), None, "an advancing spitter aims");
            advancing += 1;
        }
        for (id, windups) in [(melee, &mut melee_windups), (runner, &mut runner_windups)] {
            if let Some(e) = a.enemies.iter().find(|e| e.id == id)
                && e.attack_phase == EnemyPhase::Windup
            {
                assert_eq!(a.spit_target(e), None, "a melee windup aims");
                *windups += 1;
            }
        }
        if spits.contains(&None) && melee_windups > 0 && runner_windups > 0 {
            break;
        }
    }
    assert!(
        spits.contains(&Some(near_wall)),
        "the wall drew spit: {spits:?}"
    );
    assert!(spits.contains(&None), "then the defender: {spits:?}");
    assert!(advancing > 0 && melee_windups > 0 && runner_windups > 0);
}

#[test]
fn ranged_windup_points_where_its_spit_lands() {
    // W2 review: a stationed spitter's Windup event carries the point its
    // Spit lands on (the intercepting wall's centre, then the defender), not
    // its own position; a melee windup at a wall face still points at
    // itself and one at the line at the defender.
    let reach = 4.0;
    let stop = DEFENSE_LINE + reach;
    let mut g = field(12.0);
    g.set_paused(true);
    g.place(StructureKind::Wall, 5, 0).unwrap();
    g.place(StructureKind::Wall, 7, 4).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    a.structures[0].hp = 100;
    a.structures[0].max_hp = 100;
    a.structures[1].hp = 1_000_000;
    a.structures[1].max_hp = 1_000_000;
    let near_wall = a.structures[0].id;
    let wall_centre = [a.structures[0].x(), a.structures[0].y()];
    let far_face = a.structures[1].x() + a.structures[1].blocking_radius();
    let spitter = spawn_spitter(&mut g, &mut a, reach, 5, stop + 1.0);
    let melee = spawn_spitter(&mut g, &mut a, reach, 7, far_face + 1.0);
    g.spawn_enemy(&mut a, EnemyKind::Runner, 2, false);
    a.enemies.last_mut().unwrap().x = DEFENSE_LINE + 0.5;
    let runner = a.enemies.last().unwrap().id;
    g.attempt = Some(a);
    let (mut seen, mut aimed, mut spits) = (0, None, Vec::new());
    let (mut wall_windups, mut melee_windups, mut runner_windups) = (0, 0, 0);
    for _ in 0..3600 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender cannot fall");
        let defender = a.defender_position();
        for ev in new_events(a, &mut seen) {
            if ev.kind == EventKind::Windup && ev.actor_id == Some(spitter) {
                if ev.target_id == Some(near_wall) {
                    assert_eq!(ev.to, wall_centre, "the windup aims at the wall");
                    wall_windups += 1;
                } else {
                    assert_eq!(ev.to, defender, "past the wall it aims at the defender");
                }
                aimed = Some(ev.to);
            } else if ev.kind == EventKind::Spit && ev.actor_id == Some(spitter) {
                assert_eq!(Some(ev.to), aimed, "the spit lands where its windup aimed");
                spits.push(ev.target_id);
            } else if ev.kind == EventKind::Windup && ev.actor_id == Some(melee) {
                assert_eq!(ev.to, ev.from, "a melee windup at a wall face");
                melee_windups += 1;
            } else if ev.kind == EventKind::Windup && ev.actor_id == Some(runner) {
                assert_eq!(ev.to, defender, "a melee windup at the line");
                runner_windups += 1;
            }
        }
        if spits.contains(&None) && melee_windups > 0 && runner_windups > 0 {
            break;
        }
    }
    assert!(
        spits.contains(&Some(near_wall)) && spits.contains(&None),
        "the wall, then the defender, drew spit: {spits:?}"
    );
    assert!(wall_windups > 0 && melee_windups > 0 && runner_windups > 0);
}
