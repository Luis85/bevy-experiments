//! W2-48 (design-save-determinism §4-§5 and §11 "Wave 2: elites and RNG
//! isolation; fixture behaviour tests"; plan EB14 and SD6): the legacy
//! guards that keep the wave-2 systems away from encounters saved before
//! them, and behaviour tests that play the checked-in v6 and v7 fixtures
//! forward through the wave-2 rules.
//!
//! - A rank-0 deploy rolls no elite and fields only the archetypes the V1-V6
//!   encounters could spawn (the new archetypes enter at wave tier 5).
//! - Every attempt a v1-v6 save holds keeps a snapshot with no elite rate and
//!   no boss phases (R4: live legacy attempts keep their frozen snapshot
//!   fields), so the v6 fixture's live defense spawns no elite for a minute
//!   of play and its boss never phases.
//! - The v6 fixture continues through the wave-2 defender rules: a keyless
//!   Companion report redeploys Mara, and a v6 recruit takes over the next
//!   defense.
//! - The v6 ledgers migrated by step (c) and (d) pay nothing twice: a worker
//!   level-up pays its grant once, in the parent, and a pre-threat save's
//!   rank jump at load pays no milestone it had already reached.
mod support;

use horde_core::command::Command;
use horde_core::content::{EnemyKind, Mode};
use horde_core::defenders::{DefenderId, DefenderRole};
use horde_core::progression::level_grant;
use horde_core::sim::{Attempt, EventKind, Game, Input, Outcome, SAVE_VERSION, smoke_input};
use horde_core::strategy::RecruitStatus;
use horde_core::survivors::SurvivorId;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::advance;

/// The checked-in v1-v6 generations, in version order.
const LEGACY_FIXTURES: [&str; 6] = [
    include_str!("fixtures/save_v1.json"),
    include_str!("fixtures/save_v2.json"),
    include_str!("fixtures/save_v3.json"),
    include_str!("fixtures/save_v4.json"),
    include_str!("fixtures/save_v5.json"),
    include_str!("fixtures/save_v6.json"),
];
const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");
const FIXTURE_V7: &str = include_str!("fixtures/save_v7.json");

/// The 11 archetypes the V1-V6 encounters can spawn (`save_migration.rs`
/// `FROZEN_ARCHETYPES`, the W2-53 frozen records).
const FROZEN_ARCHETYPES: [&str; 11] = [
    "shambler",
    "runner",
    "brute",
    "armored",
    "demolisher",
    "breacher-pack",
    "caller-pack",
    "siege-pack",
    "skitter",
    "plated-runner",
    "hungry-shambler",
];

/// The v6 fixture's recruit that came home from the won Hungarian
/// expedition (Ready), and the live Polish expedition's recruit.
const V6_READY_RECRUIT: u64 = 2;
const V6_LIVE_EXPEDITION: u64 = 16;

fn decode(text: &str) -> Game {
    Game::decode_save_generation(text.as_bytes()).expect("the fixture validates")
}

/// The saved JSON of the v6 fixture's game, as the v6 build wrote it.
fn payload_v6() -> Value {
    let envelope: Value = serde_json::from_str(FIXTURE_V6).unwrap();
    assert_eq!(envelope["version"], 6);
    serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap()
}

/// The root attempt and every expedition's worker attempt, labelled.
fn attempts(g: &Game) -> Vec<(String, &Attempt)> {
    let mut out: Vec<(String, &Attempt)> = g
        .attempt()
        .map(|a| ("root".to_string(), a))
        .into_iter()
        .collect();
    for battle in g.battles() {
        if let Some(a) = battle.game().attempt() {
            out.push((format!("expedition {}", battle.id), a));
        }
    }
    out
}

/// What makes an attempt a wave-2 encounter: the elite rate and boss
/// transitions its snapshot froze, the elites and phased bosses on its
/// field, and the elites and phase entries its telemetry counted.
fn wave_2_marks(a: &Attempt) -> (u32, u8, usize, usize, u32, usize) {
    let elites = a
        .enemies
        .iter()
        .filter(|e| e.resolved.as_ref().is_some_and(|r| r.elite.is_some()))
        .count();
    let phased = a.enemies.iter().filter(|e| e.boss_state.is_some()).count();
    (
        a.progression.elite_rate_bp,
        a.progression.boss_phases,
        elites,
        phased,
        a.telemetry.elites_spawned,
        a.telemetry.boss_phase_ticks.len(),
    )
}

/// Saves through validation and loads.
fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e:?}"))
}

fn apply_ok(g: &mut Game, command: Command) {
    g.apply(&command)
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
}

fn status(g: &Game, id: u64) -> RecruitStatus {
    g.strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("recruit {id}"))
        .status
}

/// Ends the live main run in victory on the next tick.
fn win_main(g: &mut Game) {
    support::enemy(g, 0, 20.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    let boss = a.enemies.last_mut().unwrap();
    boss.hp = 0;
    boss.injury.unconscious = true;
    advance(g, 1);
    assert!(g.attempt().is_none(), "the run settled");
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

/// Spawns the level's boss on the root attempt through the director's
/// placement (the same `place_enemy` a natural boss spawn takes) and drops
/// it to a fifth of its hit points, under both phase thresholds.
fn wounded_boss(g: &mut Game) {
    let mut a = g.attempt_slot_mut().take().unwrap();
    assert!(!a.enemies.iter().any(|e| e.boss), "no boss yet");
    let lane = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
    a.boss_spawned = true;
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    let boss = a.enemies.iter_mut().find(|e| e.boss).expect("the boss");
    boss.hp = boss.max_hp / 5;
    boss.injury.hp = boss.hp;
    *g.attempt_slot_mut() = Some(a);
}

/// A fresh campaign's first deploy (threat rank 0) on every preset, in both
/// modes and on three seeds, played by the smoke bot for three minutes:
/// the snapshot freezes no elite rate, no hostile is ever elite, and every
/// hostile is one of the 11 archetypes the V1-V6 encounters could spawn.
#[test]
fn rank_zero_deploy_spawns_no_elites_and_has_no_new_archetypes() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let frozen: BTreeSet<&str> = FROZEN_ARCHETYPES.into();
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        for mode in [Mode::Personal, Mode::Npc] {
            for seed in [0x1, 0x1a57_1167, 0xdead_beef_2026] {
                let label = format!("{preset:?} {mode:?} {seed:#x}");
                let mut g = Game::default();
                g.set_seed(seed);
                apply_ok(
                    &mut g,
                    Command::SetBalance(BalanceSettings::for_preset(preset)),
                );
                apply_ok(&mut g, Command::ChooseSurvivor(SurvivorId::Rook));
                // The journey's first defense is a Personal deploy; an Npc
                // run replaces it once Mara is met.
                apply_ok(&mut g, Command::ChooseStart("DEU".into()));
                if mode == Mode::Npc {
                    apply_ok(&mut g, Command::Withdraw);
                    g.campaign_mut().legacy_features = true;
                    apply_ok(&mut g, Command::Deploy { stage: 0, mode });
                }
                assert_eq!(g.campaign().progression_rank(), 0, "{label}");
                g.set_paused(false);
                let a = g.attempt().unwrap();
                assert_eq!(a.progression.rank, 0, "{label}");
                assert_eq!(a.progression.elite_rate_bp, 0, "{label}");
                let mut seen = BTreeSet::new();
                for _ in 0..10_800 {
                    let input = smoke_input(&g);
                    g.tick(input);
                    let Some(a) = g.attempt() else { break };
                    for e in &a.enemies {
                        if let Some(r) = &e.resolved {
                            assert!(r.elite.is_none(), "{label}: elite enemy {}", e.id);
                            seen.insert(r.archetype_id.clone());
                        }
                    }
                    assert_eq!(a.telemetry.elites_spawned, 0, "{label}");
                    for archetype in a.telemetry.spawned_by_archetype.keys() {
                        assert!(
                            frozen.contains(archetype.as_str()),
                            "{label}: new archetype {archetype}"
                        );
                    }
                }
                assert!(!seen.is_empty(), "{label}: nothing spawned");
                assert!(
                    seen.iter().all(|id| frozen.contains(id.as_str())),
                    "{label}: {seen:?}"
                );
            }
        }
    }
}

/// R4: every attempt a v1-v6 save holds, the root's and each expedition's,
/// loads with a snapshot that rolls no elite and phases no boss, with no
/// elite or phased boss on its field and none counted. The v7 freeze holds
/// both (schema-v7 §11), so the check is not vacuous.
#[test]
fn legacy_attempt_snapshots_have_zero_elite_rate_and_no_boss_phases() {
    let none = (0, 0, 0, 0, 0, 0);
    let mut checked = 0;
    for (n, text) in LEGACY_FIXTURES.into_iter().enumerate() {
        let version = n + 1;
        let g = decode(text);
        let list = attempts(&g);
        assert!(!list.is_empty(), "v{version} holds a live attempt");
        for (label, a) in list {
            assert!(
                a.enemies.iter().all(|e| !e.split_child),
                "v{version} {label}"
            );
            assert_eq!(wave_2_marks(a), none, "v{version} {label}");
            checked += 1;
        }
        // The resave keeps them so (the snapshot fields are skipped at 0).
        let g = reload(&g);
        for (label, a) in attempts(&g) {
            assert_eq!(wave_2_marks(a), none, "v{version} {label} resaved");
        }
    }
    assert_eq!(checked, 7, "six roots and the v6 live expedition");
    let v7 = decode(FIXTURE_V7);
    let marks: Vec<_> = attempts(&v7)
        .into_iter()
        .map(|(_, a)| wave_2_marks(a))
        .collect();
    assert!(marks.iter().any(|m| m.0 > 0 && m.2 > 0), "{marks:?}");
    assert!(marks.iter().any(|m| m.1 > 0 && m.3 > 0), "{marks:?}");
}

/// The v6 fixture's live Personal defense (Hard, rank 14, frozen before
/// elites existed) and its live Polish expedition, resumed under the smoke
/// bot for 3600 ticks: hostiles keep arriving, and none is elite.
#[test]
fn v6_active_attempt_spawns_no_elites_for_3600_ticks() {
    let mut g = decode(FIXTURE_V6);
    let root = g.attempt().unwrap();
    assert_eq!((root.mode, root.progression.rank), (Mode::Personal, 14));
    let spawned = root.telemetry.spawned;
    let resumed = root.tick;
    let mut elites = Vec::new();
    for _ in 0..3600 {
        let input = smoke_input(&g);
        g.tick(input);
        for (label, a) in attempts(&g) {
            for e in &a.enemies {
                if e.resolved.as_ref().is_some_and(|r| r.elite.is_some()) {
                    elites.push(format!("{label} t{} enemy {}", a.tick, e.id));
                }
            }
            assert_eq!(a.progression.elite_rate_bp, 0, "{label}");
            assert_eq!(a.telemetry.elites_spawned, 0, "{label}");
        }
    }
    assert!(elites.is_empty(), "{elites:?}");
    let a = g.attempt().expect("the v6 defense is still live");
    assert_eq!(a.tick, resumed + 3600);
    assert!(
        a.telemetry.spawned >= spawned + 30,
        "only {} spawns after the resume",
        a.telemetry.spawned - spawned
    );
}

/// The v6 fixture's live defense froze no boss transitions, so its boss
/// (Hard at rank 14: two transitions on a deploy made today) spawns without
/// phases and, held under both thresholds, never enters one: no
/// `BossPhase` event, no phase tick, no hold. The same boss on a snapshot
/// that does freeze transitions phases on the next tick.
#[test]
fn v6_legacy_boss_has_no_phases() {
    let mut g = decode(FIXTURE_V6);
    // The boss under test spawns on the root attempt, so its snapshot is
    // the precondition (the expedition's is pinned by
    // `legacy_attempt_snapshots_have_zero_elite_rate_and_no_boss_phases`).
    let root = g.attempt().unwrap();
    assert_eq!(
        (
            root.mode,
            root.progression.rank,
            root.progression.boss_phases
        ),
        (Mode::Personal, 14, 0),
        "the v6 root snapshot froze no boss transitions"
    );
    let mut phased = g.clone();
    phased.attempt_mut().unwrap().progression.boss_phases = 2;
    for (label, game, phases) in [("v6", &mut g, false), ("phased", &mut phased, true)] {
        wounded_boss(game);
        let a = game.attempt().unwrap();
        let boss = a.enemies.iter().find(|e| e.boss).unwrap();
        assert_eq!(boss.boss_state.is_some(), phases, "{label}");
        let seq = a.events.back().map_or(0, |e| e.seq);
        for _ in 0..3 {
            game.tick(Input::default());
        }
        let a = game.attempt().expect("the defense is still live");
        let boss = a.enemies.iter().find(|e| e.boss).expect("the boss lives");
        let events = a
            .events
            .iter()
            .filter(|e| e.seq > seq && e.kind == EventKind::BossPhase)
            .count();
        assert_eq!(events > 0, phases, "{label}: {events} BossPhase events");
        assert_eq!(a.telemetry.boss_phase_ticks.is_empty(), !phases, "{label}");
        assert_eq!(boss.boss_held(a.tick), phases, "{label}");
    }
}

/// The design §10 root Npc report case, as inline migration JSON (a deploy
/// clears `last_result`, so the fixture's settled Mara report sits beside
/// a live attempt): the v6 fixture with its live defense cleared and its
/// keyless Companion report turned into a Belarus stage 1 loss. After the
/// migration the report reads as Mara's, and `ContinueJourney` redeploys
/// Mara on the reported stage with her own loadout reserved, frozen with
/// the current Hard threat (elites roll on the new deployment).
#[test]
fn continue_journey_after_migrated_companion_report_redeploys_companion() {
    let mut payload = payload_v6();
    payload["attempt"] = Value::Null;
    let campaign = payload["campaign"].as_object_mut().unwrap();
    // Only the root reservation goes; `reserved_items` holds the guard's
    // and the live expedition's loadouts.
    campaign.remove("reserved");
    let report = payload["last_result"].as_object_mut().unwrap();
    assert_eq!(report["mode"], "Npc");
    assert!(!report.contains_key("defender"), "a v6 report is keyless");
    for (key, value) in [
        ("outcome", json!("Defeat")),
        ("country_id", json!("BLR")),
        ("country_name", json!("Belarus")),
        ("stage", json!(0)),
        ("level", json!("the_old_road")),
        ("country_freed", json!(false)),
        ("first_clear", json!(false)),
        ("completion_xp", json!(0)),
    ] {
        report.insert(key.into(), value);
    }
    let mut g = Game::from_payload(payload, 6).expect("the edited v6 save loads");
    let saved = serde_json::to_value(&g).unwrap();
    assert!(saved["last_result"].get("defender").is_none());
    assert_eq!(g.last_result().unwrap().defender(), DefenderId::Mara);
    assert_eq!(g.campaign().next_defender, None);
    assert_eq!(g.next_defender(), DefenderId::Mara);
    assert_eq!(g.continue_blocker(), None);

    apply_ok(&mut g, Command::ContinueJourney);
    let a = g.attempt().expect("Continue deployed");
    assert_eq!((a.mode, a.defender()), (Mode::Npc, DefenderId::Mara));
    assert_eq!((a.country_id.as_deref(), a.stage), (Some("BLR"), 0));
    let card = g.active_defender().unwrap();
    assert_eq!(
        (card.id, card.role),
        (DefenderId::Mara, DefenderRole::Companion)
    );
    assert_eq!(card.name, g.campaign().npc.name);
    let c = g.campaign();
    assert_eq!(c.reserved, Some(Mode::Npc));
    assert_eq!(c.reserved_recruit, None);
    assert!(c.npc.loadout.iter().all(|id| c.item_reserved(*id)));
    assert!(
        g.strategy()
            .roster
            .iter()
            .all(|r| r.status != RecruitStatus::Defending)
    );
    // A deployment made today freezes today's Hard threat.
    let expected = c
        .progression_for(Mode::Npc, 14, a.lane_count, None)
        .unwrap()
        .elite_rate_bp;
    assert!(expected > 0);
    assert_eq!(a.progression.elite_rate_bp, expected);
    assert!(a.drop_tier.is_some(), "a v7 deployment tracks its drops");
    let g = reload(&g);
    assert_eq!(g.attempt().unwrap().defender(), DefenderId::Mara);
}

/// The v6 fixture's Ready recruit (home from the won Hungarian expedition)
/// takes over the next main defense: once the live Personal defense is
/// withdrawn, `SetNextDefender` and `ContinueJourney` deploy it on the
/// reported stage in its own seat and loadout, Defending, and the save
/// validates. Its victory is credited to the recruit, not Mara, and frees it.
#[test]
fn v6_fixture_recruit_can_take_over_the_next_defense() {
    let id = V6_READY_RECRUIT;
    let mut g = decode(FIXTURE_V6);
    assert_eq!(status(&g, id), RecruitStatus::Ready);
    apply_ok(&mut g, Command::Withdraw);
    let report = g.last_result().unwrap();
    assert_eq!(report.defender(), DefenderId::Player);
    let (country, stage) = (report.country_id.clone(), report.stage);
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
    assert_eq!(g.next_defender(), DefenderId::Recruit(id));
    let mara = g.campaign().npc.clone();
    let recruit = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .npc
        .clone();

    apply_ok(&mut g, Command::ContinueJourney);
    let a = g.attempt().expect("the takeover deployed");
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!((a.country_id.clone(), a.stage), (country, stage));
    assert_eq!(status(&g, id), RecruitStatus::Defending);
    let c = g.campaign();
    assert_eq!(
        (c.reserved, c.reserved_recruit),
        (Some(Mode::Npc), Some(id))
    );
    assert_eq!(c.next_defender, None, "Continue clears the choice");
    assert!(recruit.loadout.iter().all(|i| c.reserved_items.contains(i)));
    assert!(
        !mara.loadout.iter().any(|i| c.item_reserved(*i)),
        "Mara's idle loadout stays free"
    );
    let card = g.active_defender().unwrap();
    assert_eq!(
        (card.id, card.role),
        (DefenderId::Recruit(id), DefenderRole::Recruit)
    );
    let mut g = reload(&g);
    assert_eq!(status(&g, id), RecruitStatus::Defending);

    g.set_paused(false);
    win_main(&mut g);
    assert_eq!(g.last_result().unwrap().defender(), DefenderId::Recruit(id));
    assert_eq!(status(&g, id), RecruitStatus::Ready);
    let c = g.campaign();
    assert_eq!((c.reserved, c.reserved_recruit), (None, None));
    assert!(!recruit.loadout.iter().any(|i| c.item_reserved(*i)));
    let after = &g.strategy().roster.iter().find(|r| r.id == id).unwrap().npc;
    assert!(
        after.victories > recruit.victories,
        "the recruit is credited"
    );
    assert_eq!(
        (c.npc.attempts, c.npc.victories),
        (mara.attempts, mara.victories),
        "Mara is not"
    );
    reload(&g);
}

/// Ruling E2 on the migrated v6 ledgers (step (c) marked every player paid
/// to its level): the live Polish expedition's victory levels the worker's
/// own copy of the player and, at settlement, the parent's, from 33 to 34.
/// The worker pays nothing and its wallet syncs only the run's coins; the
/// parent pays `level_grant(34)` once, on the worker's report and run
/// record. Later passes and a reload pay nothing more.
#[test]
fn worker_level_up_does_not_pay_the_level_grant_twice() {
    let b = V6_LIVE_EXPEDITION;
    let mut g = decode(FIXTURE_V6);
    // The parent settles an expedition only while no main attempt is live.
    apply_ok(&mut g, Command::Withdraw);
    g.set_paused(false);
    let player = &g.campaign().player;
    assert_eq!((player.level, player.level_grant_paid), (33, 33));
    let next = player.next_level_xp();
    g.campaign_mut().player.xp = next - 1;
    let worker = worker(&mut g, b);
    let player = &mut worker.campaign_mut().player;
    assert_eq!((player.level, player.level_grant_paid), (33, 33));
    player.xp = player.next_level_xp() - 1;
    let worker_coins = worker.campaign().coins;
    let coins = g.campaign().coins;

    win_stage(&mut g, b);
    let w = g.battles().iter().find(|x| x.id == b).unwrap().game();
    let report = w.last_result().unwrap();
    assert_eq!(report.outcome, Outcome::Victory);
    let wp = &w.campaign().player;
    assert_eq!(
        (wp.level, wp.level_grant_paid),
        (34, 33),
        "a worker never settles"
    );
    assert_eq!(
        w.campaign().coins,
        report.coins,
        "the worker's wallet holds only its run's coins"
    );
    let paid = report.progression_rewards.clone();
    let p = &g.campaign().player;
    assert_eq!((p.level, p.level_grant_paid), (34, 34));
    assert_eq!((paid.level_from, paid.level_to), (33, 34));
    assert!(level_grant(34) > 0);
    assert_eq!(paid.level_coins, level_grant(34), "one grant, not two");
    let milestones: u64 = paid.milestones.iter().map(|m| m.coins).sum();
    assert_eq!(
        g.campaign().coins,
        coins + (report.coins - worker_coins) + paid.level_coins + milestones,
        "the synced run coins and one receipt"
    );
    let records: Vec<_> = g
        .campaign()
        .run_history
        .iter()
        .filter(|r| r.worker_id == Some(b))
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].level_grant_coins, level_grant(34));

    let coins = g.campaign().coins;
    let synced = |g: &Game| {
        g.battles()
            .iter()
            .find(|x| x.id == b)
            .map_or(0, |x| x.game().campaign().coins)
    };
    let worker_coins = synced(&g);
    advance(&mut g, 3);
    let mut g = reload(&g);
    advance(&mut g, 3);
    assert_eq!(
        g.campaign().coins - coins,
        synced(&g) - worker_coins,
        "paid exactly once"
    );
    let p = &g.campaign().player;
    assert_eq!((p.level, p.level_grant_paid), (34, 34));
}

fn worker(g: &mut Game, id: u64) -> &mut Game {
    g.battles_mut()
        .iter_mut()
        .find(|b| b.id == id)
        .unwrap()
        .game_mut()
}

/// Ends the expedition's current stage in victory on the next tick.
fn win_stage(g: &mut Game, b: u64) {
    let w = worker(g, b);
    support::enemy(w, 5, 20.0, true);
    let a = w.attempt_mut().unwrap();
    a.boss_spawned = true;
    let boss = a.enemies.last_mut().unwrap();
    boss.hp = 0;
    boss.injury.unconscious = true;
    advance(g, 1);
}

/// Design §4 "Grants": `before` is measured after
/// `migrate_threat_history()`. The v6 fixture as a pre-threat save (no
/// threat revision, no victories) jumps to its legacy clear rank 14 at load;
/// step (d) claimed the milestones that rank had reached (5 and 10), so the
/// jump pays none of them at the next settlement. The same game with the
/// claims cleared would pay both, so the guard is not vacuous.
#[test]
fn migrated_threat_jump_pays_no_milestone() {
    let mut payload = payload_v6();
    let pre_threat = |campaign: &mut Value| {
        let campaign = campaign.as_object_mut().unwrap();
        campaign.remove("threat_revision");
        campaign.insert("victories".into(), json!(0));
    };
    pre_threat(&mut payload["campaign"]);
    for battle in payload["battles"].as_array_mut().unwrap() {
        pre_threat(&mut battle["game"]["campaign"]);
    }
    let g = Game::from_payload(payload, 6).expect("the pre-threat v6 save loads");
    let c = g.campaign();
    assert_eq!((c.victories, c.progression_rank()), (14, 14), "the jump");
    assert_eq!(c.milestones_claimed, BTreeSet::from([5, 10]));
    let authored: Vec<u32> = horde_core::progression::rewards()
        .milestones
        .iter()
        .map(|m| m.rank)
        .filter(|rank| *rank <= 14)
        .collect();
    assert_eq!(authored, [5, 10], "the shipped milestones it reached");

    let mut unclaimed = g.clone();
    unclaimed.campaign_mut().milestones_claimed.clear();
    let mut settled = Vec::new();
    for (label, mut game) in [("migrated", g), ("unclaimed", unclaimed)] {
        apply_ok(&mut game, Command::Withdraw);
        let paid = game.last_result().unwrap().progression_rewards.clone();
        assert_eq!(
            paid.level_coins, 0,
            "{label}: the migrated ledger already paid the level grant, so the withdrawal pays none"
        );
        assert_eq!(
            game.campaign().milestones_claimed,
            BTreeSet::from([5, 10]),
            "{label}"
        );
        let record = game.campaign().run_history.last().unwrap();
        let ranks: Vec<u32> = paid.milestones.iter().map(|m| m.rank).collect();
        let coins: u64 = paid.milestones.iter().map(|m| m.coins).sum();
        assert_eq!(record.milestone_coins, coins, "{label}");
        settled.push((ranks, coins, game.campaign().coins));
    }
    let [(migrated, none, base), (unclaimed, owed, paid)] = settled.as_slice() else {
        unreachable!()
    };
    assert!(migrated.is_empty(), "the jump paid {migrated:?}");
    assert_eq!(*none, 0);
    assert_eq!(*unclaimed, [5, 10], "unclaimed, the same withdrawal pays");
    assert_eq!(*paid, base + owed);
}
