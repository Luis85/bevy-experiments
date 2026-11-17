//! W2-30 (design enemy-balance §4 "Shield-bearer", "Juggernaut", "Sprinter"
//! and "Counter seams", ruling W-7): ranged armor, stagger immunity, groups
//! and the hazard multiplier on spit. Every test injects a test-only profile
//! so the mechanism is pinned apart from the shipped records: the composition
//! (W2-31) authored the shield-bearer's ranged armor, the juggernaut's
//! `stagger_immune` and the sprinter's `group_size` 4 in `enemy_catalog.json`.

use super::*;
use crate::enemy_catalog::{DeathEffect, ResolvedEnemy};
use crate::rules::{Damage, DamageKind, Rng};

/// A legacy game with an unpaused attempt deployed in `mode` that owns the
/// turret blueprint and grenades, and a Personal defender who cannot fall.
fn game(mode: Mode) -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.campaign.blueprints = KindMap::filled(true);
    for level in 0..3 {
        g.campaign.first_clear(level, Mode::Personal);
    }
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign.purchase_node("grenade_training").unwrap();
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    let a = g.attempt.as_mut().unwrap();
    assert!(a.enemies.is_empty(), "nothing has spawned at deploy");
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    // The director spawns nothing, so only the staged hostiles take part.
    a.spawn_credit = -1.0e9;
    g
}

/// A test-only archetype: `kind`'s default profile renamed to `id`.
fn profile(kind: EnemyKind, id: &str) -> ResolvedEnemy {
    let mut p = crate::enemy_catalog::resolve_kind(kind);
    p.archetype_id = id.into();
    p.name = format!("Test {id}");
    p
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

// ---------------------------------------------------------------------------
// Shield-bearer: ranged armor.

#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Shot,
    Turret,
    Melee,
    Ordnance,
    Trap,
    Fire,
}

/// Personal damage one durable shield-bearer (`armor`, `ranged` ranged
/// armor) takes from `act`, standing at the defense line when `contact`,
/// else at x 8, while the defender carries `penetration`.
fn shield_damage(armor: i32, ranged: i32, contact: bool, act: Act, penetration: i32) -> i32 {
    let mut g = game(Mode::Personal);
    match act {
        Act::Turret => g.place(StructureKind::Turret, 5, 0).unwrap(),
        Act::Trap => g.place(StructureKind::Trap, 5, 0).unwrap(),
        _ => {}
    }
    let mut a = g.attempt.take().unwrap();
    a.profile.modifiers.armor_penetration = penetration;
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let mut p = profile(EnemyKind::Shambler, "test-shield");
    p.ranged_armor = ranged;
    p.validate().unwrap();
    let x = if contact { DEFENSE_LINE } else { 8.0 };
    if act == Act::Trap {
        // The trap sits under the hostile, wherever it stands.
        a.structures[0].position_x = Some(x);
    }
    if act == Act::Fire {
        // A Molotov's fire zone, without the impact blast before it.
        a.ordnance.fire_zones.push(crate::ordnance::FireZone {
            id: 9_999,
            cell: [x as u8, 5],
            radius: 1.5,
            damage: 20,
            ticks_left: 40,
            tick_left: 1,
            tick_interval: 10,
        });
    }
    let e = &mut a.enemies[0];
    e.resolved = Some(p);
    e.x = x;
    e.armor = armor;
    e.hp = 1000;
    e.max_hp = 1000;
    e.injury = InjuryState::new(1000, 10);
    g.attempt = Some(a);
    match act {
        Act::Shot => g.tick(Input {
            aim: [x, 5.5],
            fire: true,
            ..Input::default()
        }),
        Act::Melee => g.tick(Input {
            aim: [x, 5.5],
            melee: true,
            ..Input::default()
        }),
        Act::Turret | Act::Trap | Act::Fire => {
            for _ in 0..40 {
                g.tick(Input::default());
            }
        }
        Act::Ordnance => {
            g.tick(Input {
                throw: Some(ThrowRequest {
                    kind: OrdnanceKind::Grenade,
                    cell: [8, 5],
                }),
                ..Input::default()
            });
            for _ in 0..40 {
                g.tick(Input::default());
            }
        }
    }
    let a = g.attempt.as_ref().unwrap();
    let e = &a.enemies[0];
    assert_eq!(e.x <= DEFENSE_LINE + 0.01, contact, "{act:?}: contact");
    e.max_hp - e.hp
}

/// FNV-1a of the per-tick NPC injury of one durable shield-bearer (DR
/// `armor`, `ranged` NPC ranged armor) under the NPC defender's fire and a
/// turret's, at the line when `contact`, else at x 8. Its own strikes never
/// wound, so the defender keeps fighting.
fn npc_shield_trace(armor: i32, ranged: i32, contact: bool) -> u64 {
    let mut g = game(Mode::Npc);
    g.place(StructureKind::Turret, 5, 0).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let mut p = profile(EnemyKind::Shambler, "test-shield");
    p.npc_ranged_armor = ranged;
    p.npc_damage_dice = 1;
    p.npc_damage_add = -30;
    p.speed = 0.01;
    p.validate().unwrap();
    let e = &mut a.enemies[0];
    e.resolved = Some(p);
    e.x = if contact { DEFENSE_LINE } else { 8.0 };
    e.armor = armor;
    e.max_hp = 1000;
    e.hp = 1000;
    e.injury = InjuryState::new(1000, 16);
    g.attempt = Some(a);
    let mut bytes = Vec::new();
    let mut wounded = false;
    for _ in 0..1800 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender holds");
        let e = &a.enemies[0];
        assert_eq!(e.contact(), contact);
        wounded |= e.injury.hp < 1000;
        bytes.extend(e.injury.hp.to_le_bytes());
        bytes.extend(format!("{:?}", a.rng).as_bytes());
    }
    assert!(wounded || ranged > 0, "the fire reaches it");
    crate::hash::fnv1a(&bytes)
}

#[test]
fn shield_ranged_armor_applies_to_shot_and_turret_until_contact() {
    let (armor, ranged) = (2, 3);
    // Personal: `ranged_armor` adds to the armor a shot or a turret meets
    // while the shield-bearer is out of contact, exactly as base armor would.
    for act in [Act::Shot, Act::Turret] {
        let shielded = shield_damage(armor, ranged, false, act, 0);
        assert!(shielded > 0, "{act:?} beats armor {armor} + {ranged}");
        assert_eq!(
            shielded,
            shield_damage(armor + ranged, 0, false, act, 0),
            "{act:?} out of contact"
        );
        assert!(shielded < shield_damage(armor, 0, false, act, 0), "{act:?}");
    }
    // Penetration reduces the ranged armor too (counter seam):
    // max(armor + ranged - penetration, 0).
    assert_eq!(
        shield_damage(armor, ranged, false, Act::Shot, ranged),
        shield_damage(armor, 0, false, Act::Shot, 0)
    );
    // In contact, and for melee, blasts, traps and fire, the shield does
    // nothing.
    for (act, contact) in [
        (Act::Shot, true),
        (Act::Melee, true),
        (Act::Ordnance, false),
        (Act::Trap, false),
        (Act::Fire, false),
    ] {
        let base = shield_damage(armor, 0, contact, act, 0);
        assert!(base > 0, "{act:?} reaches it");
        assert_eq!(
            shield_damage(armor, ranged, contact, act, 0),
            base,
            "{act:?}, contact {contact}"
        );
    }
    // NPC: `npc_ranged_armor` adds to the DR against the defender's fire and
    // the turret's out of contact, exactly as base armor would; in contact it
    // does nothing.
    assert_eq!(npc_shield_trace(0, 4, false), npc_shield_trace(4, 0, false));
    assert_ne!(npc_shield_trace(0, 4, false), npc_shield_trace(0, 0, false));
    assert_eq!(npc_shield_trace(0, 4, true), npc_shield_trace(0, 0, true));
}

// ---------------------------------------------------------------------------
// Juggernaut: stagger immunity.

/// A test-only exploder whose death burst hits 3d6.
fn exploder() -> ResolvedEnemy {
    let mut p = profile(EnemyKind::Shambler, "test-exploder");
    p.death_effect = Some(DeathEffect::Burst {
        radius: 1.6,
        damage: 30,
        structure_damage: 0,
        npc_dice: 3,
        npc_add: 0,
        fuse: false,
    });
    p.validate().unwrap();
    p
}

/// A test-only heavy: the Brute profile, `stagger_immune` when `immune`.
fn heavy(immune: bool) -> ResolvedEnemy {
    let mut p = profile(EnemyKind::Brute, "test-juggernaut");
    p.stagger_immune = immune;
    p.validate().unwrap();
    p
}

/// An NPC game where a defeated exploder's death burst, reaped with the
/// combat RNG `rng`, hits one 12-HP heavy (DR 0) beside it. Returns the RNG
/// after the reap and the heavy.
fn npc_burst_on_heavy(immune: bool, rng: Rng) -> (Rng, Enemy) {
    let (rng, e, _) = npc_burst_on_heavy_logged(immune, rng);
    (rng, e)
}

/// [`npc_burst_on_heavy`] plus the burst's rule-resolution log line.
fn npc_burst_on_heavy_logged(immune: bool, rng: Rng) -> (Rng, Enemy, String) {
    let mut g = game(Mode::Npc);
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    g.spawn_enemy(&mut a, EnemyKind::Brute, 5, false);
    let (source, target) = (a.enemies[0].id, a.enemies[1].id);
    a.enemies[0].resolved = Some(exploder());
    a.enemies[0].x = 6.0;
    a.enemies[0].injury.unconscious = true;
    let e = &mut a.enemies[1];
    e.resolved = Some(heavy(immune));
    e.x = 6.4;
    e.armor = 0;
    e.max_hp = 12;
    e.hp = 12;
    e.injury = InjuryState::new(12, 10);
    a.rng = rng;
    g.reap(&mut a);
    assert!(
        a.enemies.iter().all(|e| e.id != source),
        "the exploder is reaped"
    );
    let e = a.enemies.iter().find(|e| e.id == target).unwrap().clone();
    let line = a.latest_resolution.clone().expect("the burst is logged");
    (a.rng, e, line)
}

#[test]
fn npc_juggernaut_major_wound_clears_stagger_flags_and_matches_draw_count() {
    // The first combat RNG whose kill-drop roll misses and whose burst gives
    // the heavy a major wound that knocks it down without knocking it out.
    let (rng, staggered) = (1..)
        .map(Rng::new)
        .filter(|rng| {
            let mut probe = *rng;
            probe.below(100) >= 3
        })
        .map(|rng| (rng, npc_burst_on_heavy(false, rng).1))
        .find(|(_, e)| e.injury.stunned && !e.injury.terminal())
        .unwrap();
    assert!(staggered.injury.prone && staggered.injury.weapon_dropped);
    assert!(staggered.hit_stop_remaining() > 0);
    // Expected draws: the drop roll and one blast, exactly as before.
    let mut expected = rng;
    expected.below(100);
    let mut injury = InjuryState::new(12, 10);
    crate::rules::resolve_blast(
        &mut expected,
        Damage {
            dice: 3,
            adds: 0,
            kind: DamageKind::Crushing,
        },
        Some(injury.dodge(heavy(true).npc_dodge)),
        0,
        1.0,
        &mut injury,
        heavy(true).npc_ht,
    );
    assert_eq!(injury, staggered.injury, "the plain heavy takes the wound");
    let (after, immune) = npc_burst_on_heavy(true, rng);
    assert_eq!(after, expected, "the same draw count");
    assert_eq!(after, npc_burst_on_heavy(false, rng).0);
    assert_eq!(immune.injury.hp, staggered.injury.hp, "the same wound");
    assert_eq!(immune.hp, immune.injury.hp);
    assert_eq!(immune.injury.shock, staggered.injury.shock);
    assert!(
        !immune.injury.stunned && !immune.injury.prone && !immune.injury.weapon_dropped,
        "stagger flags cleared: {:?}",
        immune.injury
    );
    assert_eq!(immune.hit_stop_remaining(), 0, "no hit-stop");
}

/// W2 review: the log of a juggernaut's major wound matches its cleared
/// stagger flags; it never reports the hostile stunned, knocked down or
/// disarmed. The plain heavy's line is unchanged.
#[test]
fn npc_juggernaut_major_wound_log_reports_no_stagger() {
    let rng = (1..)
        .map(Rng::new)
        .filter(|rng| {
            let mut probe = *rng;
            probe.below(100) >= 3
        })
        .find(|rng| {
            let e = npc_burst_on_heavy(false, *rng).1;
            e.injury.stunned && !e.injury.terminal()
        })
        .unwrap();
    let (_, _, staggered) = npc_burst_on_heavy_logged(false, rng);
    assert!(
        staggered.contains("stunned, knocked down, weapon dropped"),
        "{staggered}"
    );
    let (_, immune, line) = npc_burst_on_heavy_logged(true, rng);
    assert!(!immune.injury.stunned, "{:?}", immune.injury);
    for word in ["stunned", "knocked down", "weapon dropped"] {
        assert!(!line.contains(word), "{word} in {line}");
    }
    assert!(
        line.contains("major wound"),
        "the wound is still logged: {line}"
    );
    assert_eq!(
        line.replace("stagger immune", "stunned, knocked down, weapon dropped"),
        staggered,
        "only the stagger consequence differs"
    );
}

#[test]
fn personal_stagger_immune_hit_never_holds_movement() {
    for immune in [false, true] {
        let mut g = game(Mode::Personal);
        let mut a = g.attempt.take().unwrap();
        g.spawn_enemy(&mut a, EnemyKind::Brute, 5, false);
        let e = &mut a.enemies[0];
        e.resolved = Some(heavy(immune));
        e.x = 8.0;
        e.armor = 0;
        e.hp = 1000;
        e.max_hp = 1000;
        g.attempt = Some(a);
        g.tick(Input {
            aim: [8.0, 5.5],
            fire: true,
            ..Input::default()
        });
        let e = &g.attempt.as_ref().unwrap().enemies[0];
        assert!(e.hp < 1000, "the shot lands");
        assert_eq!(e.hit_stop_remaining() == 0, immune, "immune {immune}");
    }
}

// ---------------------------------------------------------------------------
// Sprinter: groups.

#[test]
fn npc_sprinter_group_spawns_group_size_members_without_extra_rng() {
    let mut g = game(Mode::Npc);
    let mut a = g.attempt.take().unwrap();
    let mut p = profile(EnemyKind::Runner, "test-sprinter");
    p.group_size = 4;
    p.validate().unwrap();
    let rng = a.rng;
    let spawned = a.telemetry.spawned;
    let mut seen = a.event_seq;
    let x0 = a.width_yards + 0.7;
    g.spawn_pick(&mut a, p.clone(), 3);
    assert_eq!(a.rng, rng, "a group draws nothing");
    assert_eq!(a.enemies.len(), 4, "group_size members");
    let npc_hp = (p.npc_hp as f32 * a.modifiers.enemy_health).round() as i32;
    for (k, e) in a.enemies.iter().enumerate() {
        assert_eq!(e.lane, 3, "member {k}");
        let x = x0 + 0.65 * k as f32;
        assert!((e.x - x).abs() < 1e-4, "member {k} at {} not {x}", e.x);
        assert_eq!(e.resolved.as_ref(), Some(&p), "member {k}");
        assert_eq!(e.injury, InjuryState::new(npc_hp, 10), "member {k}");
        assert_eq!(e.armor, p.npc_armor);
        assert!(!e.split_child);
    }
    assert_eq!(a.telemetry.spawned, spawned + 4);
    let spawns: Vec<_> = new_events(&a, &mut seen)
        .into_iter()
        .filter(|e| e.kind == EventKind::Spawn)
        .filter_map(|e| e.actor_id)
        .collect();
    let ids: Vec<_> = a.enemies.iter().map(|e| e.id).collect();
    assert_eq!(spawns, ids, "one Spawn per member");
    // Each member respects the cap: two places left take two members.
    a.enemies.clear();
    a.progression.enemy_cap = 2;
    let skipped = a.telemetry.skipped_spawns;
    g.spawn_pick(&mut a, p, 7);
    assert_eq!(a.rng, rng);
    assert_eq!(a.enemies.len(), 2, "the cap holds");
    assert_eq!(a.telemetry.skipped_spawns, skipped + 2);
    // A shipped pick (group 1) still spawns one.
    a.enemies.clear();
    g.spawn_pick(
        &mut a,
        crate::enemy_catalog::resolve_kind(EnemyKind::Runner),
        7,
    );
    assert_eq!(a.enemies.len(), 1);
    assert_eq!(a.rng, rng);
}

/// W2 review: the director's own weighted wave pick of the shipped
/// sprinter spawns its whole group in the chosen lane, and the pass draws
/// only the lane and the one weighted roll.
#[test]
fn npc_director_wave_pick_of_a_sprinter_spawns_its_group() {
    let catalog = crate::enemy_catalog::enemy_catalog();
    let sprinter = catalog.resolve("sprinter").unwrap();
    assert!(sprinter.group_size > 1, "the shipped sprinter is a pack");
    let tier = 5;
    let mut g = game(Mode::Npc);
    let mut base = g.attempt.take().unwrap();
    base.progression.wave_tier = tier;
    base.progression.batch_size = 1;
    base.progression.elite_rate_bp = 0;
    base.progression.enemy_cap = 16;
    let pool = catalog.wave_pool(base.level.max(tier), &g.campaign.unlocked_bosses());
    let total: u64 = pool.iter().map(|entry| u64::from(entry.weight)).sum();
    // The first combat seed whose weighted roll lands on the sprinter,
    // with the lane and the RNG state that one lane draw and one roll give.
    let (mut a, lane, after) = (1..10_000)
        .find_map(|seed| {
            let mut a = base.clone();
            a.rng = Rng::new(seed);
            let mut probe = a.clone();
            let lane = choose_lane(&mut probe);
            let mut roll = probe.rng.below(total);
            let picked = pool.iter().find(|entry| {
                let weight = u64::from(entry.weight);
                if roll < weight {
                    return true;
                }
                roll -= weight;
                false
            })?;
            (picked.archetype_id == "sprinter").then_some((a, lane, probe.rng))
        })
        .expect("a seed picks the sprinter");
    a.spawn_credit = 0.999_999;
    g.spawn_director(&mut a);
    assert_eq!(a.rng, after, "one lane draw and one weighted roll");
    assert_eq!(
        a.enemies.len(),
        usize::from(sprinter.group_size),
        "group_size members"
    );
    for (k, e) in a.enemies.iter().enumerate() {
        let r = e.resolved.as_ref().expect("a wave spawn is resolved");
        assert_eq!(r.archetype_id, "sprinter", "member {k}");
        assert_eq!(e.lane, lane, "member {k}");
    }
}

// ---------------------------------------------------------------------------
// The hazard seam on spit.

/// A test-only spitter: the Runner profile with 4 yards of reach and a hit
/// that beats the starter armor.
fn spitter() -> ResolvedEnemy {
    let mut p = profile(EnemyKind::Runner, "test-spitter");
    p.reach = 4.0;
    p.damage = 40;
    p.npc_skill = 16;
    p.validate().unwrap();
    p
}

/// The damage of each spit on the defender within `ticks`, and the attempt's
/// `hazard_damage_taken`, on a 12-yard field in `mode` with the survivor's
/// hazard multiplier at `multiplier`.
fn spits(mode: Mode, multiplier: f32, ticks: u32) -> (Vec<i32>, u64, i32) {
    let mut g = game(mode);
    let mut a = g.attempt.take().unwrap();
    a.width_yards = 12.0;
    a.profile.modifiers.hazard_damage_multiplier = multiplier;
    g.spawn_enemy(&mut a, EnemyKind::Runner, 5, false);
    let e = a.enemies.last_mut().unwrap();
    e.resolved = Some(spitter());
    e.x = DEFENSE_LINE + 5.0;
    e.max_hp = 1000;
    e.hp = 1000;
    e.injury = InjuryState::new(1000, 10);
    a.injury = InjuryState::new(1000, 10);
    let armor = a.profile.personal.armor;
    g.attempt = Some(a);
    let (mut seen, mut values) = (0, Vec::new());
    for _ in 0..ticks {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender holds");
        for ev in new_events(a, &mut seen) {
            if ev.kind == EventKind::Spit {
                assert_eq!(ev.target_id, None, "at the defender");
                values.push(ev.value);
            }
        }
    }
    let a = g.attempt.as_ref().unwrap();
    (values, a.telemetry.hazard_damage_taken, armor)
}

#[test]
fn spit_on_the_defender_runs_through_the_hazard_seam() {
    // Personal: raw × multiplier before armor (and Brace), and the damage
    // counts as hazard damage taken.
    let (neutral, hazard, armor) = spits(Mode::Personal, 1.0, 1200);
    assert!(!neutral.is_empty() && neutral[0] > 0, "the spit wounds");
    assert_eq!(hazard, neutral.iter().map(|v| *v as u64).sum::<u64>());
    let raw = neutral[0] + armor;
    let (scaled, hazard, _) = spits(Mode::Personal, 0.5, 1200);
    assert_eq!(scaled.len(), neutral.len());
    let expected = ((raw as f32 * 0.5).round() as i32 - armor).max(0);
    assert!(
        scaled.iter().all(|v| *v == expected),
        "{scaled:?} vs {expected}"
    );
    assert_eq!(hazard, scaled.iter().map(|v| *v as u64).sum::<u64>());
    // NPC: Mara and recruits take raw spit (the multiplier is Personal-only),
    // and it still counts as hazard damage.
    let (npc, hazard, _) = spits(Mode::Npc, 0.5, 3600);
    assert!(npc.iter().any(|v| *v > 0), "an NPC spit wounds: {npc:?}");
    assert_eq!(hazard, npc.iter().map(|v| *v as u64).sum::<u64>());
    assert_eq!(spits(Mode::Npc, 1.0, 3600).0, npc);
}
