//! W2-29 (design enemy-balance §4 "Exploder" and "Splitter", ruling W-7):
//! the exploder's fuse and death burst, the splitter's children and their NPC
//! resolution. No shipped archetype authors a `death_effect` yet (the
//! exploder and splitter records land with the composition, W2-31), so every
//! test injects a test-only profile.

use super::*;
use crate::enemy_catalog::{DeathEffect, ResolvedEnemy};
use crate::rules::{Damage, DamageKind};
use std::collections::BTreeMap;

/// A legacy game with an unpaused attempt deployed in `mode` and a defender
/// who cannot fall.
fn game(mode: Mode) -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    let a = g.attempt.as_mut().unwrap();
    assert!(a.enemies.is_empty(), "nothing has spawned at deploy");
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    g
}

fn burst(fuse: bool, damage: i32) -> DeathEffect {
    DeathEffect::Burst {
        radius: 1.6,
        damage,
        structure_damage: 40,
        npc_dice: 3,
        npc_add: 0,
        fuse,
    }
}

/// A test-only archetype: `kind`'s default profile with `effect`.
fn profile(kind: EnemyKind, id: &str, effect: DeathEffect) -> ResolvedEnemy {
    let mut p = crate::enemy_catalog::resolve_kind(kind);
    p.archetype_id = id.into();
    p.name = format!("Test {id}");
    p.death_effect = Some(effect);
    p.validate().unwrap();
    p
}

fn exploder(effect: DeathEffect) -> ResolvedEnemy {
    profile(EnemyKind::Shambler, "test-exploder", effect)
}

fn splitter(child: &str, count: u8) -> ResolvedEnemy {
    profile(
        EnemyKind::Brute,
        "test-splitter",
        DeathEffect::Split {
            archetype_id: child.into(),
            count,
        },
    )
}

/// Spawns a `kind` hostile in `lane` at `x`, with `resolved` in place of its
/// default profile when Some; returns its id.
fn place(
    g: &mut Game,
    a: &mut Attempt,
    kind: EnemyKind,
    resolved: Option<ResolvedEnemy>,
    lane: u8,
    x: f32,
) -> u64 {
    g.spawn_enemy(a, kind, lane, false);
    let e = a.enemies.last_mut().unwrap();
    if let Some(r) = resolved {
        assert_eq!(r.kind, kind);
        e.resolved = Some(r);
    }
    e.x = x;
    e.id
}

fn find(a: &Attempt, id: u64) -> Option<&Enemy> {
    a.enemies.iter().find(|e| e.id == id)
}

fn find_mut(a: &mut Attempt, id: u64) -> &mut Enemy {
    a.enemies
        .iter_mut()
        .find(|e| e.id == id)
        .expect("the enemy lives")
}

/// Marks `id` defeated in `a`'s mode.
fn defeat(a: &mut Attempt, id: u64) {
    let mode = a.mode;
    let e = find_mut(a, id);
    if mode == Mode::Personal {
        e.hp = 0;
    } else {
        e.injury.unconscious = true;
    }
}

/// A combat RNG whose next `below(100)` is at least 3, so a kill's 3% drop
/// roll misses and the reap draws nothing more.
fn no_drop_rng() -> crate::rules::Rng {
    (1..)
        .map(crate::rules::Rng::new)
        .find(|rng| rng.clone().below(100) >= 3)
        .unwrap()
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

#[test]
fn npc_exploder_fuse_detonates_on_turn_without_reward_or_corpse() {
    let mut g = game(Mode::Npc);
    let mut a = g.attempt.take().unwrap();
    // The director spawns nothing, so only the exploder takes part.
    a.spawn_credit = -1.0e9;
    let id = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(exploder(burst(true, 30))),
        5,
        DEFENSE_LINE + 0.3,
    );
    // Durable, so the defender cannot shoot it down before it reaches her.
    let e = find_mut(&mut a, id);
    e.max_hp = 1000;
    e.hp = 1000;
    e.injury = InjuryState::new(1000, 10);
    g.attempt = Some(a);
    let mut seen = 0;
    let mut detonated = None;
    for _ in 0..900 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().expect("the defender holds");
        for ev in new_events(a, &mut seen) {
            if ev.actor_id != Some(id) {
                continue;
            }
            assert!(
                !matches!(
                    ev.kind,
                    EventKind::Miss | EventKind::Spit | EventKind::Kill | EventKind::Hit
                ),
                "a fused exploder never attacks or dies: {:?}",
                ev.kind
            );
            if ev.kind == EventKind::Detonation {
                assert!(detonated.is_none(), "it detonates once");
                assert_eq!(ev.target_id, None, "the fuse struck the defender");
                assert_eq!(ev.value, 160, "radius in hundredths");
                detonated = Some(ev.tick);
            }
        }
        if detonated.is_some() {
            break;
        }
    }
    let tick = detonated.expect("the fuse detonated in contact");
    assert!(tick.is_multiple_of(60), "at a normal NPC turn, tick {tick}");
    let a = g.attempt.as_ref().unwrap();
    assert!(find(a, id).is_none(), "removed in the same pass");
    assert!(a.reward_ledger.contains(&id), "its id is in the ledger");
    assert!(a.corpses.iter().all(|c| c.id != id), "no corpse");
    assert_eq!((a.kills, a.coins), (0, 0), "no reward");
    let fuse = g
        .drain_events()
        .find_map(|r| match r.event {
            SimEvent::Detonation {
                enemy_id, trigger, ..
            } if enemy_id == id => Some((r.tick, trigger)),
            _ => None,
        })
        .expect("a typed detonation");
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(
        fuse,
        (
            tick,
            DetonationTrigger::Fuse {
                target_id: None,
                to: a.defender_position(),
            }
        )
    );
    assert!(
        a.log.iter().any(|line| line.contains("Test test-exploder")),
        "the blast resolution is logged: {:?}",
        a.log
    );
}

#[test]
fn npc_death_burst_hits_enemies_in_radius_in_id_order_never_defender() {
    let mut g = game(Mode::Npc);
    let mut a = g.attempt.take().unwrap();
    let blast = exploder(burst(false, 30));
    let x = place(&mut g, &mut a, EnemyKind::Shambler, Some(blast), 5, 6.0);
    // Two hostiles inside the 1.6-yard radius, one outside it; the later id
    // stands earlier in the list, so list order is not id order.
    let near_early = place(&mut g, &mut a, EnemyKind::Runner, None, 6, 6.3);
    let near_late = place(&mut g, &mut a, EnemyKind::Brute, None, 5, 6.8);
    let far = place(&mut g, &mut a, EnemyKind::Runner, None, 5, 8.0);
    assert!(near_early < near_late);
    a.enemies.swap(1, 2);
    assert_eq!(a.enemies[1].id, near_late);
    let far_before = find(&a, far).unwrap().injury.clone();
    defeat(&mut a, x);
    a.rng = no_drop_rng();
    let defender = (a.injury.clone(), a.hp);
    // Expected: the kill's drop roll, then one blast per hostile in radius in
    // ascending id, each with its own dodge, DR and HT.
    let mut expected = a.rng;
    expected.below(100);
    let mut injuries = BTreeMap::new();
    for id in [near_early, near_late] {
        let e = find(&a, id).unwrap();
        let mut injury = e.injury.clone();
        crate::rules::resolve_blast(
            &mut expected,
            Damage {
                dice: 3,
                adds: 0,
                kind: DamageKind::Crushing,
            },
            Some(e.injury.dodge(enemy_dodge(&a, e))),
            enemy_armor(&a, e),
            1.0,
            &mut injury,
            enemy_ht(&a, e),
        );
        injuries.insert(id, injury);
    }
    g.reap(&mut a);
    assert!(find(&a, x).is_none(), "the exploder is reaped");
    assert_eq!(a.kills, 1, "its kill pays normally");
    assert!(a.corpses.iter().any(|c| c.id == x));
    for (id, injury) in &injuries {
        let e = find(&a, *id).unwrap();
        assert_eq!(&e.injury, injury, "hostile {id}");
        assert_eq!(e.hp, e.injury.hp);
    }
    assert_eq!(
        find(&a, far).unwrap().injury,
        far_before,
        "outside the radius"
    );
    assert_eq!((a.injury.clone(), a.hp), defender, "never the defender");
    assert_eq!(a.rng, expected, "no other combat draws");
    let detonation = a
        .events
        .iter()
        .find(|e| e.kind == EventKind::Detonation)
        .expect("the burst is published");
    assert_eq!(detonation.actor_id, Some(x));
    assert_eq!(detonation.target_id, None);
    assert_eq!(detonation.from, [6.0, 5.5]);
    g.attempt = Some(a);
    let records: Vec<SimEvent> = g.drain_events().map(|r| r.event).collect();
    assert!(records.contains(&SimEvent::Detonation {
        enemy_id: x,
        trigger: DetonationTrigger::Death,
        lane: Some(5),
        at: [6.0, 5.5],
        radius: 1.6,
    }));
    let struck: Vec<u64> = records
        .iter()
        .filter_map(|event| match event {
            // The exploder has left the field when its burst resolves.
            SimEvent::RuleResolved {
                attacker: Actor::Unknown { id },
                target_id: Some(target),
                ..
            } if *id == x => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(struck, [near_early, near_late], "ascending id");
}

/// A game in `mode` with `parent` defeated in lane 4 at x 7, and the combat
/// RNG set so the parent's drop roll misses.
fn split_field(mode: Mode, parent: ResolvedEnemy) -> (Game, Attempt, u64) {
    let mut g = game(mode);
    let mut a = g.attempt.take().unwrap();
    let id = place(&mut g, &mut a, EnemyKind::Brute, Some(parent), 4, 7.0);
    defeat(&mut a, id);
    a.rng = no_drop_rng();
    (g, a, id)
}

#[test]
fn npc_split_children_use_own_npc_stats_and_draw_no_rng() {
    let (mut g, mut a, parent) = split_field(Mode::Npc, splitter("skitter", 3));
    let mut plain = crate::enemy_catalog::resolve_kind(EnemyKind::Brute);
    plain.archetype_id = "test-splitter".into();
    let (mut g2, mut b, _) = split_field(Mode::Npc, plain);
    g.reap(&mut a);
    g2.reap(&mut b);
    assert_eq!(a.rng, b.rng, "the split draws nothing");
    assert_eq!(a.kills, 1);
    let skitter = crate::enemy_catalog::enemy_catalog()
        .resolve("skitter")
        .unwrap();
    let hp = (skitter.npc_hp as f32 * a.modifiers.enemy_health)
        .round()
        .max(1.0) as i32;
    let children: Vec<&Enemy> = a.enemies.iter().collect();
    assert_eq!(children.len(), 3);
    for (k, child) in children.iter().enumerate() {
        assert!(child.split_child);
        assert!(child.id > parent);
        assert_eq!(child.lane, 4);
        assert_eq!(child.x, 7.0 + 0.35 * k as f32);
        assert_eq!(child.kind, skitter.kind);
        assert_eq!(child.resolved.as_ref(), Some(&skitter), "own profile");
        assert_eq!((child.max_hp, child.hp), (hp, hp), "own NPC hit points");
        assert_eq!(child.injury, InjuryState::new(hp, 10));
        assert_eq!(child.armor, skitter.npc_armor, "own NPC armor");
    }
    let split = a
        .events
        .iter()
        .find(|e| e.kind == EventKind::Split)
        .expect("the split is published");
    assert_eq!((split.actor_id, split.value), (Some(parent), 3));
    assert_eq!(split.from, [7.0, 4.5]);
    // A child takes and discards its drop roll and never drops, even on
    // the every-20th kill.
    let child = a.enemies[0].id;
    defeat(&mut a, child);
    a.kills = 19;
    let mut expected = a.rng;
    expected.below(100);
    let loot = a.loot;
    g.reap(&mut a);
    assert_eq!(a.kills, 20);
    assert_eq!(a.loot, loot, "a split child never drops");
    assert_eq!(a.rng, expected, "exactly the discarded drop roll");
    assert!(
        a.enemies.iter().all(|e| e.split_child),
        "a child never splits"
    );
    // A target the catalogue no longer has falls back to the Runner default.
    let (mut g, mut a, _) = split_field(Mode::Npc, splitter("no-such-archetype", 2));
    g.reap(&mut a);
    let runner = crate::enemy_catalog::resolve_kind(EnemyKind::Runner);
    assert_eq!(a.enemies.len(), 2);
    assert!(
        a.enemies
            .iter()
            .all(|e| e.split_child && e.resolved.as_ref() == Some(&runner))
    );
}

/// A chain exploder: a burst no NPC-mode victim dodges (dodge 0) and that
/// always defeats it, so both modes chain the same way.
fn chain_exploder() -> ResolvedEnemy {
    let mut p = exploder(DeathEffect::Burst {
        radius: 1.6,
        damage: 10_000,
        structure_damage: 40,
        npc_dice: 20,
        npc_add: 100,
        fuse: false,
    });
    p.npc_dodge = 0;
    p.validate().unwrap();
    p
}

/// A game in `mode` with three chain exploders in lane 5 at x 8, 9 and 10
/// (each inside the next one's radius, the first and last 2 yards apart),
/// the first defeated, and an untouched bystander far away in lane 1.
fn chain_field(mode: Mode) -> (Game, Vec<u64>, u64) {
    let mut g = game(mode);
    let mut a = g.attempt.take().unwrap();
    let ids: Vec<u64> = [8.0, 9.0, 10.0]
        .into_iter()
        .map(|x| {
            place(
                &mut g,
                &mut a,
                EnemyKind::Shambler,
                Some(chain_exploder()),
                5,
                x,
            )
        })
        .collect();
    let bystander = place(&mut g, &mut a, EnemyKind::Brute, None, 1, 12.0);
    defeat(&mut a, ids[0]);
    g.attempt = Some(a);
    (g, ids, bystander)
}

fn save_round_trip(g: &Game) -> Game {
    let json = g.to_save_json().expect("the save is valid");
    Game::from_payload(serde_json::from_str(&json).unwrap(), SAVE_VERSION).unwrap()
}

#[test]
fn chain_bursts_resolve_one_tick_later_and_terminate() {
    for mode in [Mode::Personal, Mode::Npc] {
        let (mut g, ids, bystander) = chain_field(mode);
        let mut seen = 0;
        let mut detonations = Vec::new();
        let start = g.attempt.as_ref().unwrap().tick;
        // Short of the first NPC turn, so the NPC never shoots the bystander.
        for step in 1..=30u64 {
            g.tick(Input::default());
            let a = g.attempt.as_ref().expect("the defender holds");
            for ev in new_events(a, &mut seen) {
                if ev.kind == EventKind::Detonation {
                    detonations.push((ev.actor_id.unwrap(), ev.tick - start));
                }
            }
            if step > 3 {
                continue;
            }
            // Link `step` burst this tick; its victim waits for the next.
            let k = step as usize - 1;
            assert_eq!(a.kills, step as u32, "{mode:?} tick {step}");
            assert!(find(a, ids[k]).is_none(), "{mode:?} tick {step}: reaped");
            let Some(next) = ids.get(k + 1).copied() else {
                continue;
            };
            let victim = find(a, next).expect("reaped next tick, not now");
            assert!(
                enemy_defeated(a, victim),
                "{mode:?} tick {step}: defeated by the burst"
            );
            // The save between the burst and the reap is valid.
            let loaded = save_round_trip(&g);
            assert!(find(loaded.attempt.as_ref().unwrap(), next).is_some());
            // A defeated hostile outside the burst's radius on its tick is not.
            let held = g.attempt.clone();
            defeat(g.attempt.as_mut().unwrap(), bystander);
            assert!(
                g.to_save_json().is_err(),
                "{mode:?} tick {step}: a defeated hostile outside the radius"
            );
            g.attempt = held;
        }
        assert_eq!(
            detonations,
            [(ids[0], 1), (ids[1], 2), (ids[2], 3)],
            "{mode:?}: one link per tick; each exploder bursts once and the chain ends"
        );
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(a.kills, 3);
        assert!(find(a, bystander).is_some_and(|e| e.hp == e.max_hp));
    }
}

#[test]
fn burst_victim_save_survives_a_full_event_window() {
    // The presentation window is a 512-event ring: a crowded tick can evict
    // the burst's Detonation before the save. Its held victim still saves.
    for mode in [Mode::Personal, Mode::Npc] {
        let (mut g, ids, _) = chain_field(mode);
        g.tick(Input::default());
        let a = g.attempt.as_mut().unwrap();
        assert!(find(a, ids[1]).is_some_and(|e| enemy_defeated(a, e)));
        for _ in 0..MAX_EVENTS {
            a.event(EventKind::Kill, [1.0, 0.5], [1.0, 0.5], 0);
        }
        assert!(
            a.events.iter().all(|ev| ev.kind != EventKind::Detonation),
            "{mode:?}: the burst's Detonation left the window"
        );
        let loaded = save_round_trip(&g);
        assert!(find(loaded.attempt.as_ref().unwrap(), ids[1]).is_some());
    }
}

#[test]
fn save_rejects_a_defeated_hostile_beside_a_defender_fuse() {
    // A fuse on the defender publishes a Detonation with no target, like a
    // death burst, but it never hits a hostile: a defeated one beside it is
    // invalid.
    let mut g = game(Mode::Personal);
    let mut a = g.attempt.take().unwrap();
    a.spawn_credit = -1.0e9;
    let fuse = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(exploder(burst(true, 30))),
        5,
        DEFENSE_LINE,
    );
    let bystander = place(
        &mut g,
        &mut a,
        EnemyKind::Brute,
        None,
        5,
        DEFENSE_LINE + 0.5,
    );
    g.attempt = Some(a);
    let mut seen = 0;
    let mut fused = false;
    for _ in 0..900 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().unwrap();
        fused = new_events(a, &mut seen).iter().any(|ev| {
            ev.kind == EventKind::Detonation && ev.actor_id == Some(fuse) && ev.target_id.is_none()
        });
        if fused {
            break;
        }
    }
    assert!(fused, "the fuse detonated on the defender");
    // A saved spawn credit lies in [0, 1).
    g.attempt.as_mut().unwrap().spawn_credit = 0.0;
    assert!(g.to_save_json().is_ok(), "the fuse tick itself saves");
    defeat(g.attempt.as_mut().unwrap(), bystander);
    assert!(
        g.to_save_json().is_err(),
        "a defeated hostile beside a defender fuse"
    );
}

#[test]
fn start_of_tick_reap_frees_a_burst_victims_slot_before_the_director() {
    // NPC mode, the field at cap with a burst victim held: the next tick's
    // first reap pays the victim before the director runs, so the director
    // fills the freed slot that tick. Mutation proof: without the
    // start-of-tick reap (sim.rs, before advance_ordnance) the first NPC-mode
    // reap follows the director, which sees the field full and spawns nothing.
    let mut g = game(Mode::Npc);
    let mut a = g.attempt.take().unwrap();
    let cap = a.enemy_cap();
    let source = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(chain_exploder()),
        5,
        9.0,
    );
    let victim = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(chain_exploder()),
        5,
        10.0,
    );
    while a.enemies.len() < cap {
        let lane = (a.enemies.len() % 3) as u8;
        place(&mut g, &mut a, EnemyKind::Runner, None, lane, 13.0);
    }
    defeat(&mut a, source);
    g.reap(&mut a);
    assert!(
        find(&a, victim).is_some_and(|e| enemy_defeated(&a, e)),
        "held"
    );
    place(&mut g, &mut a, EnemyKind::Runner, None, 0, 13.0);
    assert_eq!(a.enemies.iter().filter(|e| !e.boss).count(), cap);
    let mut seen = a.events.back().map_or(0, |ev| ev.seq + 1);
    g.attempt = Some(a);
    // The held victim saves in NPC mode too (hp tracks the injury state).
    let loaded = save_round_trip(&g);
    assert!(find(loaded.attempt.as_ref().unwrap(), victim).is_some());
    // The director spawns a batch next tick.
    g.attempt.as_mut().unwrap().spawn_credit = 1.0;
    g.tick(Input::default());
    let a = g.attempt.as_ref().unwrap();
    let fresh = new_events(a, &mut seen);
    let kill = fresh
        .iter()
        .find(|ev| ev.kind == EventKind::Kill && ev.actor_id == Some(victim))
        .expect("the victim is reaped this tick");
    let spawn = fresh
        .iter()
        .find(|ev| ev.kind == EventKind::Spawn)
        .expect("the director fills the freed slot");
    assert!(kill.seq < spawn.seq, "the reap precedes the director");
}

#[test]
fn splitter_at_cap_minus_one_spawns_exactly_one_child_and_saves() {
    let mut g = game(Mode::Personal);
    let mut a = g.attempt.take().unwrap();
    let cap = a.enemy_cap();
    let parent = place(
        &mut g,
        &mut a,
        EnemyKind::Brute,
        Some(splitter("skitter", 3)),
        2,
        9.0,
    );
    for n in 1..cap {
        let lane = (n % usize::from(a.lane_count)) as u8;
        place(&mut g, &mut a, EnemyKind::Runner, None, lane, 11.0);
    }
    assert_eq!(a.enemies.iter().filter(|e| !e.boss).count(), cap);
    defeat(&mut a, parent);
    g.reap(&mut a);
    // The parent's reap leaves the field at cap - 1: room for one child.
    let children: Vec<&Enemy> = a.enemies.iter().filter(|e| e.split_child).collect();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].x, 9.0);
    assert_eq!(a.enemies.len(), cap);
    let split = a
        .events
        .iter()
        .find(|e| e.kind == EventKind::Split)
        .expect("the split is published");
    assert_eq!(split.value, 1, "the count spawned");
    g.attempt = Some(a);
    let json = g.to_save_json().expect("the field at cap saves");
    let loaded = Game::from_payload(
        serde_json::from_str(&json).unwrap(),
        crate::sim::SAVE_VERSION,
    )
    .unwrap();
    let a = loaded.attempt.as_ref().unwrap();
    assert_eq!(a.enemies.iter().filter(|e| e.split_child).count(), 1);
    // Only a burst victim may be saved defeated: any other one is invalid.
    let other = g.attempt.as_ref().unwrap().enemies[0].id;
    defeat(g.attempt.as_mut().unwrap(), other);
    assert!(
        g.to_save_json().is_err(),
        "a defeated hostile outside a burst"
    );
}

#[test]
fn personal_exploder_fuse_strikes_its_wall_and_splashes_structures() {
    // The Personal fuse (design §4): the threat-scaled blast hits the wall
    // it targets, flat structure_damage the other structures within the
    // radius, and the exploder leaves without reward; on the defender, armor
    // and Brace apply as to any strike.
    let mut g = game(Mode::Personal);
    g.campaign.blueprints = KindMap::filled(true);
    g.set_paused(true);
    g.place(StructureKind::Wall, 5, 0).unwrap();
    g.place(StructureKind::Wall, 4, 0).unwrap();
    g.place(StructureKind::Wall, 1, 4).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    a.spawn_credit = -1.0e9;
    let wall = a.structures.iter().find(|s| s.lane == 5).unwrap().clone();
    let face = wall.x() + wall.blocking_radius();
    let id = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(exploder(burst(true, 30))),
        5,
        face + 0.05,
    );
    let hp: Vec<(u64, i32)> = a.structures.iter().map(|s| (s.id, s.hp)).collect();
    g.attempt = Some(a);
    let mut seen = 0;
    let mut fused = false;
    for _ in 0..900 {
        g.tick(Input::default());
        let a = g.attempt.as_ref().unwrap();
        fused |= new_events(a, &mut seen)
            .iter()
            .any(|ev| ev.kind == EventKind::Detonation && ev.actor_id == Some(id));
        if fused {
            break;
        }
    }
    assert!(fused, "the fuse detonated at the wall");
    let a = g.attempt.as_ref().unwrap();
    assert!(find(a, id).is_none() && a.reward_ledger.contains(&id));
    assert_eq!(a.kills, 0);
    let raw = (30.0 * a.progression.enemy_damage * a.balance.enemy_damage).round() as i32;
    let (mut splashed, mut spared) = (false, false);
    for (sid, before) in hp {
        let s = a.structures.iter().find(|s| s.id == sid).unwrap();
        let near = (s.x() - (face + 0.05)).hypot(s.y() - 5.5) <= 1.6;
        splashed |= near && sid != wall.id;
        spared |= !near;
        let expected = if sid == wall.id {
            before - raw
        } else if near {
            before - 40
        } else {
            before
        };
        assert_eq!(s.hp, expected, "structure {sid} in lane {}", s.lane);
    }
    assert!(
        splashed && spared,
        "one wall inside the radius, one outside"
    );
    // On the defender: (damage x threat) - armor, halved by Brace.
    let mut g = game(Mode::Personal);
    let mut a = g.attempt.take().unwrap();
    let id = place(
        &mut g,
        &mut a,
        EnemyKind::Shambler,
        Some(exploder(burst(true, 30))),
        5,
        DEFENSE_LINE,
    );
    a.spawn_credit = -1.0e9;
    a.statuses
        .refresh(StatusId::Brace, BRACE_DURATION_TICKS * 4);
    let before = a.hp;
    g.attempt = Some(a);
    for _ in 0..900 {
        g.tick(Input::default());
        if find(g.attempt.as_ref().unwrap(), id).is_none() {
            break;
        }
    }
    let a = g.attempt.as_ref().unwrap();
    assert!(find(a, id).is_none(), "the fuse detonated on the defender");
    let raw = (30.0 * a.progression.enemy_damage * a.balance.enemy_damage).round() as i32;
    let dealt = (raw - a.profile.personal.armor).max(0) / 2;
    assert_eq!(before - a.hp, dealt);
    assert_eq!(a.telemetry.hazard_damage_taken, dealt as u64);
}
