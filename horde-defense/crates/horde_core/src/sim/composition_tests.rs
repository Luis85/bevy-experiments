//! W2-31 (design enemy-balance §4 "Records", ruling W-7): the shipped
//! archetype records resolve in an NPC (3d6) battle. The mechanics are
//! pinned on test-only profiles by W2-27..W2-30; this pins the shipped
//! records' NPC numbers and that each one, spawned through the catalogue,
//! is fought to a resolution.

use super::*;

/// The design table's NPC stats: hp, ht, skill, dodge, armor, damage dice,
/// damage add.
const NPC_TABLE: [(&str, [i32; 7]); 7] = [
    ("exploder", [10, 10, 9, 6, 0, 1, 0]),
    ("splitter", [18, 12, 11, 6, 2, 2, 0]),
    ("splitter-brood", [5, 9, 9, 8, 0, 1, -2]),
    ("spitter", [8, 10, 11, 8, 0, 1, 1]),
    ("shield-bearer", [12, 11, 11, 7, 3, 1, 2]),
    ("sprinter", [6, 9, 10, 9, 0, 1, -1]),
    ("juggernaut", [30, 14, 12, 5, 5, 3, 2]),
];

/// A legacy NPC game with an unpaused attempt whose director spawns nothing
/// and whose defender cannot fall, so only the staged hostiles take part.
fn npc_game() -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.deploy(0, Mode::Npc).unwrap();
    g.set_paused(false);
    let a = g.attempt.as_mut().unwrap();
    assert!(a.enemies.is_empty(), "nothing has spawned at deploy");
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    a.spawn_credit = -1.0e9;
    g
}

#[test]
fn npc_resolution_for_each_new_archetype() {
    let catalog = crate::enemy_catalog::enemy_catalog();
    for (id, stats) in NPC_TABLE {
        let record = catalog.resolve(id).unwrap();
        assert_eq!(
            [
                record.npc_hp,
                record.npc_ht,
                record.npc_skill,
                record.npc_dodge,
                record.npc_armor,
                i32::from(record.npc_damage_dice),
                record.npc_damage_add,
            ],
            stats,
            "{id}: the authored NPC stats"
        );
        let mut g = npc_game();
        let mut a = g.attempt.take().unwrap();
        g.spawn_archetype(&mut a, id, 5).unwrap();
        let members: Vec<u64> = a.enemies.iter().map(|e| e.id).collect();
        assert_eq!(members.len(), usize::from(record.group_size), "{id}");
        let hp = (record.npc_hp as f32 * a.modifiers.enemy_health)
            .round()
            .max(1.0) as i32;
        for e in &a.enemies {
            assert_eq!(e.resolved.as_ref(), Some(&record), "{id}: own profile");
            assert_eq!((e.max_hp, e.hp), (hp, hp), "{id}: own NPC hit points");
            // Every hostile spawns with 10 fatigue points (director.rs).
            assert_eq!(e.injury, InjuryState::new(hp, 10), "{id}");
            assert_eq!(e.armor, record.npc_armor, "{id}: own NPC armor");
        }
        g.attempt = Some(a);
        let mut seen = 0;
        let mut detonations = 0;
        let mut split = None;
        for _ in 0..60 * 120 {
            g.tick(Input::default());
            let a = g.attempt.as_ref().expect("the defender holds");
            for ev in a.events.iter().filter(|ev| ev.seq >= seen) {
                match ev.kind {
                    EventKind::Detonation if members.contains(&ev.actor_id.unwrap_or(0)) => {
                        detonations += 1
                    }
                    EventKind::Split => split = Some(ev.value),
                    _ => {}
                }
            }
            seen = a.events.back().map_or(seen, |ev| ev.seq + 1);
            for e in &a.enemies {
                if e.resolved.as_ref().is_some_and(|r| r.stagger_immune) {
                    assert!(
                        !(e.injury.prone || e.injury.stunned || e.injury.weapon_dropped),
                        "{id}: a stagger-immune hostile never stays down"
                    );
                }
            }
            if a.enemies.is_empty() {
                break;
            }
        }
        let a = g.attempt.as_ref().unwrap();
        assert!(a.enemies.is_empty(), "{id}: every hostile was resolved");
        let kills = |key: &str| {
            a.telemetry
                .kills_by_archetype
                .get(key)
                .copied()
                .unwrap_or(0)
        };
        match id {
            // Killed (a death burst) or fused on the defender: one blast.
            "exploder" => assert_eq!(detonations, 1, "{id}: one blast"),
            "splitter" => {
                assert_eq!(kills(id), 1, "{id}");
                assert_eq!(split, Some(3), "{id}: three children");
                assert_eq!(kills("splitter-brood"), 3, "{id}: its children");
            }
            _ => assert_eq!(kills(id), u32::from(record.group_size), "{id}"),
        }
    }
}
