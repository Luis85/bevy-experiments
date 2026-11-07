//! Legacy save compatibility against a real checked-in generation.
//!
//! `fixtures/save_v1.json` was written by `Game::save` at save format v1
//! (commit cb9f34d): a legacy campaign with three clears, all blueprints,
//! Wall/Turret upgrades, legacy body points, NPC training, a bought SMG, looted
//! Rare/Epic gear, one finished personal run and a live NPC attempt at tick 2400.
//!
//! `fixtures/save_v2.json` was written by `Game::save` at save format v2
//! (commit 5c91e05): a personal attempt at tick 42 with every unlock skill
//! (piercing, second wind, brace, sweep, grenade, Molotov), brace active
//! (259 ticks) and two hit-stopped Runner snapshots (5 ticks) beside a Shambler.
//!
//! `fixtures/save_v3.json` was written by `Game::save` at save format v3
//! (commit 804b2bb): a live personal attempt with positional quality-indexed
//! loot counters `[0,2,1,0,1,1]` and a finished run with `[1,1,0,2,0,0]`.
//!
//! `fixtures/save_v4.json` was written by `Game::save` at save format v4
//! (commit c6f518d): a live personal attempt at tick 481 with both throws
//! unlocked, one grenade and one Molotov thrown (positional ordnance counters:
//! charges `[1,1]`, cooldowns `[10,61]`) and the Molotov's fire zone burning.
//!
//! `fixtures/save_v5.json` was written by `Game::save` at save format v5
//! (commit b9d1fd4): a modern campaign whose level progress is positional —
//! campaign first clears `[true,true,false,false,false]`, `unlocked: 3`, NPC
//! first clears `[true,false,..]` with `first_clear_claimed: [2,0,0,0,0]`, one
//! withdrawn run on level 1 (`template: 1`) and a live personal attempt on
//! level 2 (`level: 2`) at tick 420.
//!
//! `fixtures/save_v6.json` was written by `Game::save` at save format v6 on
//! untouched v6 code (parent commit 3d492cd), by the `#[ignore]`d writer in
//! `save_fixtures.rs` (seed 0x5AFE_0006). Its state was reached through
//! `Game::apply` with the `--smoke` bot inputs; the staging seam set only the
//! game and strategy seeds and coins, and restored the root report that the
//! final deployment clears. It holds a Hard campaign with one earlier Custom
//! round, 14 victories, every catalogue level cleared (Mara's allowance of 52
//! fully awarded), four liberated countries, three recruits (Guarding in
//! Moldova; Ready after a won Hungarian expedition, in bought gear; Deployed
//! on a live Polish expedition with one synced drop), a stash with a locked
//! Epic, a favourite Rare and a second preset, a settled Mara victory as the
//! root report, and a live Personal attempt in Belarus at tick 423 (rank 14)
//! with enemies, a corpse and a drop. Rook's natural level is 33 (Moldova
//! alone reaches 14), and the file is 120,842 bytes: the 14 run records, two
//! 80-line Npc logs and two worker games are most of it.
//!
//! `fixtures/save_v7.json` is the save format 7 freeze (W1-63, schema-v7.md
//! section 11), written by `Game::save` through the `#[ignore]`d
//! `write_save_v7_fixture` in `save_fixtures.rs` (seed 0x5AFE_0007; see
//! `stage_v7_scenario` there). Played through `Game::apply`: Rook liberates
//! Moldova and Romania, Mara replays Moldova stage 1 (the restored root
//! report, staged as recruit 2's main-run takeover), recruit 2 wins Ukraine stage 1 on an expedition, recruit 3 is out
//! in Hungary with a synced drop, the preset is Custom on Hard's extras,
//! auto-sell is on, and Mara holds Romania stage 1 at tick 1740 (rank 8).
//! Staged through the test-support seams: recruit 1 takes that defense over
//! (`stage_takeover`), and every v7 field without a wave-1 writer is set to a
//! valid non-default value. It is 95,266 bytes (budget 96 KB), its checksum
//! and length are pinned, and it is never re-cut: a later saved field must
//! load it unchanged.
mod support;

use horde_core::campaign::{BodyAttribute, NpcTraining};
use horde_core::content::{EnemyKind, ItemDefId, Mode, StructureKind};
use horde_core::defenders::DefenderId;
use horde_core::ordnance::OrdnanceKind;
use horde_core::quality::Quality;
use horde_core::sim::{Game, Input, NpcManeuver, Outcome, SAVE_VERSION, SaveError, smoke_input};
use horde_core::skill_catalog::unlocks;
use horde_core::status::StatusId;
use horde_core::strategy::RecruitStatus;
use horde_core::survivors::Appearance;

const FIXTURE: &str = include_str!("fixtures/save_v1.json");
const FIXTURE_V2: &str = include_str!("fixtures/save_v2.json");
const FIXTURE_V3: &str = include_str!("fixtures/save_v3.json");
const FIXTURE_V4: &str = include_str!("fixtures/save_v4.json");
const FIXTURE_V5: &str = include_str!("fixtures/save_v5.json");
const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

fn fixture() -> Game {
    Game::decode_save_generation(FIXTURE.as_bytes()).expect("v1 fixture migrates and validates")
}

#[test]
fn v1_fixture_is_a_real_v1_generation() {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(envelope["version"], 1);
    assert!(envelope.get("content_fingerprint").is_none());
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["campaign"]["items"][5]["definition"], 22);
    assert_eq!(
        payload["campaign"]["blueprints"],
        serde_json::json!([true, true, true])
    );
}

#[test]
fn v1_fixture_preserves_item_identities() {
    let game = fixture();
    let items: Vec<_> = game
        .campaign()
        .items
        .iter()
        .map(|i| (i.id, i.definition.as_str(), i.level, i.quality))
        .collect();
    assert_eq!(
        items,
        [
            (1, "service_carbine", 1, None),
            (2, "crowbar", 1, None),
            (3, "work_jacket", 1, None),
            (4, "field_webbing", 1, None),
            (5, "worn_service_pistol", 1, None),
            (6, "compact_smg", 3, None),
            (7, "scavenged_ballistic_vest", 3, Some(Quality::Rare)),
            (8, "longwatch_rifle", 2, Some(Quality::Epic)),
        ]
    );
    assert_eq!(
        game.campaign().item(6).unwrap().definition().name,
        "Compact SMG"
    );
    let gear: Vec<_> = game.campaign().run_history[0]
        .gear
        .iter()
        .map(|g| g.definition.as_str())
        .collect();
    assert_eq!(
        gear,
        [
            "compact_smg",
            "crowbar",
            "scavenged_ballistic_vest",
            "field_webbing"
        ]
    );
    let visual = &game.attempt().unwrap().profile.equipment_visual;
    assert_eq!(
        visual.ranged.as_ref().unwrap().definition,
        ItemDefId::from("service_carbine")
    );
    assert_eq!(
        visual.outfit.as_ref().unwrap().definition,
        ItemDefId::from("work_jacket")
    );
}

#[test]
fn v1_fixture_preserves_kind_indexed_counters() {
    let game = fixture();
    let c = game.campaign();
    assert!(StructureKind::ALL.iter().all(|kind| c.blueprints[*kind]));
    assert_eq!(c.structure_upgrades[StructureKind::Trap], 0);
    assert_eq!(c.structure_upgrades[StructureKind::Wall], 1);
    assert_eq!(c.structure_upgrades[StructureKind::Turret], 1);
    assert_eq!(c.player.body[BodyAttribute::Strength], 1);
    assert_eq!(c.player.body[BodyAttribute::Perception], 1);
    assert_eq!(c.player.body.values().sum::<u32>(), 2);
    assert_eq!(c.npc.investments[NpcTraining::Guns], 1);
    assert_eq!(c.npc.investments[NpcTraining::Melee], 1);
    assert_eq!(c.npc.investments.values().sum::<u32>(), 2);
    let a = game.attempt().unwrap();
    assert_eq!(a.tick, 2400);
    assert_eq!(a.telemetry.kills_by_kind[EnemyKind::Shambler], 2);
    assert_eq!(a.telemetry.kills_by_kind[EnemyKind::Runner], 6);
    assert_eq!(a.telemetry.kills_by_kind.values().sum::<u32>(), 8);
    let decisions = &a.telemetry.npc_decisions;
    assert_eq!(
        [
            decisions[NpcManeuver::DoNothing],
            decisions[NpcManeuver::Aim],
            decisions[NpcManeuver::Attack],
            decisions[NpcManeuver::Reload],
        ],
        [5, 17, 16, 2]
    );
    assert_eq!(decisions.values().sum::<u32>(), 40);
    let modifiers = &a.profile.modifiers;
    assert_eq!(modifiers.body_development[BodyAttribute::Strength], 1);
    assert_eq!(modifiers.structure_ranks[StructureKind::Turret], 1);
    assert!(modifiers.blueprints[StructureKind::Trap]);
}

/// FNV-1a of the v1 fixture's NPC attempt continued for 1800 idle ticks
/// (sampled every 150), recorded on the v2 build at commit 5c91e05. Covers the
/// migrated Runner-category snapshot's NPC targeting priority.
const V1_CONTINUATION_FNV: u64 = 0xcceb_2db8_51a0_7913;

#[test]
fn v1_fixture_continues_exactly_as_the_v2_build_did() {
    let mut g = fixture();
    let mut trace = String::new();
    for t in 0..1800u32 {
        g.tick(Input::default());
        if t % 150 == 149 {
            let a = g.attempt().unwrap();
            trace.push_str(&format!(
                "t{} hp{} kills{} target{:?} enemies{:?}\n",
                a.tick,
                a.hp,
                a.telemetry.kills_by_kind.values().sum::<u32>(),
                a.decision_target,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    let fnv = support::fnv1a(trace.as_bytes());
    assert_eq!(trace.lines().count(), 12);
    assert_eq!(fnv, V1_CONTINUATION_FNV, "{trace}");
}

#[test]
fn migrated_v1_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture(), "v1");
}

fn resave_and_resume_byte_identically(mut direct: Game, label: &str) {
    let path = std::env::temp_dir().join(format!(
        "lastlight-{label}-resave-{}.json",
        std::process::id()
    ));
    direct.save(&path).unwrap();
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(envelope["version"], SAVE_VERSION);
    let mut resumed = Game::load(&path).unwrap();
    for extension in ["json", "bak"] {
        let _ = std::fs::remove_file(path.with_extension(extension));
    }
    for _ in 0..600 {
        direct.tick(Input::default());
        resumed.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&direct).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
}

fn fixture_v2() -> Game {
    Game::decode_save_generation(FIXTURE_V2.as_bytes()).expect("v2 fixture migrates and validates")
}

#[test]
fn v2_fixture_is_a_real_v2_generation() {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V2).unwrap();
    assert_eq!(envelope["version"], 2);
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    let attempt = &payload["attempt"];
    assert_eq!(attempt["brace_left"], 259);
    assert_eq!(attempt["enemies"][0]["hit_stop_left"], 5);
    assert_eq!(attempt["profile"]["modifiers"]["grenade_unlocked"], true);
    assert!(attempt["enemies"][0]["resolved"]["target_urgency"].is_null());
}

#[test]
fn v2_fixture_counters_become_statuses_with_the_same_clocks() {
    let game = fixture_v2();
    let a = game.attempt().unwrap();
    assert_eq!(a.tick, 42);
    assert_eq!(a.brace_remaining_ticks(), 259);
    assert_eq!(a.statuses().iter().count(), 1);
    let enemies: Vec<_> = a
        .enemies
        .iter()
        .map(|e| (e.id, e.kind, e.hit_stop_remaining()))
        .collect();
    assert_eq!(
        enemies,
        [
            (1, EnemyKind::Runner, 5),
            (2, EnemyKind::Shambler, 0),
            (3, EnemyKind::Runner, 5)
        ]
    );
    let stop = a.enemies[0].statuses.get(StatusId::HitStop).unwrap();
    assert_eq!((stop.stacks, stop.magnitude), (1, 0));
    assert!(a.enemies[1].statuses.is_empty());
}

#[test]
fn v2_fixture_unlock_flags_and_target_urgency_become_data() {
    let game = fixture_v2();
    let a = game.attempt().unwrap();
    for id in unlocks::ALL {
        assert!(a.profile.modifiers.unlocked(id), "{id}");
    }
    assert_eq!(a.profile.modifiers.unlocks.len(), unlocks::ALL.len());
    let urgency: Vec<_> = a
        .enemies
        .iter()
        .map(|e| e.resolved.as_ref().unwrap().target_urgency)
        .collect();
    assert_eq!(urgency, [-3.0, 0.0, -3.0]);
    // Migrated snapshots equal what the current catalogue resolves.
    assert_eq!(
        a.enemies[0].resolved.as_ref().unwrap(),
        &horde_core::enemy_catalog::resolve_kind(EnemyKind::Runner)
    );
}

/// Trace of the v2 fixture continued for 900 ticks (aim at the nearest enemy,
/// fire whenever one exists, brace at step 400), recorded on the v2 build at
/// commit 5c91e05. The status/unlock/targeting refactor must reproduce it.
const V2_CONTINUATION: [&str; 6] = [
    "t46 hp125 brace255 kills0 enemies[(1, 8, 1090582386, 1), (2, 32, 1091095334, 0), (3, 11, 1090582386, 1)]",
    "t47 hp125 brace254 kills0 enemies[(1, 8, 1090582386, 0), (2, 32, 1091085067, 0), (3, 11, 1090582386, 0)]",
    "t48 hp125 brace253 kills0 enemies[(1, 8, 1090558356, 0), (2, 32, 1091074800, 0), (3, 11, 1090558356, 0)]",
    "t343 hp125 brace0 kills4 enemies[]",
    "t543 hp125 brace200 kills4 enemies[(5, 2, 1093660889, 0), (6, 32, 1095252274, 0)]",
    "t942 hp125 brace0 kills7 enemies[(8, 17, 1094451448, 0)]",
];

#[test]
fn v2_fixture_continues_exactly_as_the_v2_build_did() {
    let mut g = fixture_v2();
    let mut trace = Vec::new();
    for t in 0..900u32 {
        let aim = g.attempt().and_then(|a| {
            a.enemies
                .iter()
                .min_by(|x, y| x.x.total_cmp(&y.x))
                .map(|e| [e.x, e.y()])
        });
        g.tick(Input {
            fire: aim.is_some(),
            aim: aim.unwrap_or([15.0, 5.5]),
            brace: t == 400,
            ..Input::default()
        });
        if [3, 4, 5, 300, 500, 899].contains(&t) {
            let a = g.attempt().unwrap();
            trace.push(format!(
                "t{} hp{} brace{} kills{} enemies{:?}",
                a.tick,
                a.hp,
                a.brace_remaining_ticks(),
                a.telemetry.kills_by_kind.values().sum::<u32>(),
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits(), e.hit_stop_remaining()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert_eq!(trace, V2_CONTINUATION);
}

#[test]
fn migrated_v2_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture_v2(), "v2");
}

fn fixture_v3() -> Game {
    Game::decode_save_generation(FIXTURE_V3.as_bytes()).expect("v3 fixture migrates and validates")
}

#[test]
fn v3_fixture_quality_arrays_become_kind_maps_with_the_same_counts() {
    let game = fixture_v3();
    let counts = |map: &horde_core::kind_map::KindMap<Quality, u32>| {
        Quality::ALL.map(|quality| map[quality])
    };
    assert_eq!(
        counts(&game.attempt().unwrap().loot_quality_counts),
        [0, 2, 1, 0, 1, 1]
    );
    assert_eq!(
        counts(&game.campaign().run_history[0].loot_by_quality),
        [1, 1, 0, 2, 0, 0]
    );
    assert_eq!(game.attempt().unwrap().loot, 6);
}

#[test]
fn migrated_v3_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture_v3(), "v3");
}

#[test]
fn v3_fixture_is_a_real_v3_generation() {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V3).unwrap();
    assert_eq!(envelope["version"], 3);
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_eq!(
        payload["attempt"]["loot_quality_counts"],
        serde_json::json!([0, 2, 1, 0, 1, 1])
    );
    assert_eq!(
        payload["campaign"]["run_history"][0]["loot_by_quality"],
        serde_json::json!([1, 1, 0, 2, 0, 0])
    );
}

/// Trace of the v3 fixture continued for 600 idle ticks, recorded on the v3
/// build (commit 804b2bb) before the quality arrays became kind maps.
const V3_CONTINUATION_FNV: u64 = 0x83c6_c668_77ca_ad9f;

#[test]
fn v3_fixture_continues_exactly_as_the_v3_build_did() {
    let mut g = fixture_v3();
    let mut trace = String::new();
    for t in 0..600u32 {
        g.tick(Input::default());
        if t % 50 == 49 {
            let a = g.attempt().unwrap();
            trace.push_str(&format!(
                "t{} hp{} loot{} coins{} rng{} enemies{:?}
",
                a.tick,
                a.hp,
                a.loot,
                a.coins,
                a.rng.state,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    let fnv = support::fnv1a(trace.as_bytes());
    assert_eq!(trace.lines().count(), 12);
    assert_eq!(fnv, V3_CONTINUATION_FNV, "{trace}");
}

fn fixture_v4() -> Game {
    Game::decode_save_generation(FIXTURE_V4.as_bytes()).expect("v4 fixture migrates and validates")
}

#[test]
fn v4_fixture_is_a_real_v4_generation() {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V4).unwrap();
    assert_eq!(envelope["version"], 4);
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    let ordnance = &payload["attempt"]["ordnance"];
    assert_eq!(ordnance["cooldowns"], serde_json::json!([10, 61]));
    assert_eq!(ordnance["charges"], serde_json::json!([1, 1]));
    assert_eq!(ordnance["issued"], serde_json::json!([2, 2]));
    assert_eq!(ordnance["spent"], serde_json::json!([1, 1]));
}

#[test]
fn v4_fixture_ordnance_arrays_become_kind_maps_with_the_same_counts() {
    let game = fixture_v4();
    let o = &game.attempt().unwrap().ordnance;
    let counts = |map: &horde_core::kind_map::KindMap<OrdnanceKind, u32>| {
        [map[OrdnanceKind::Grenade], map[OrdnanceKind::Molotov]]
    };
    assert_eq!(counts(&o.cooldowns), [10, 61]);
    assert_eq!(counts(&o.charges), [1, 1]);
    assert_eq!(counts(&o.issued), [2, 2]);
    assert_eq!(counts(&o.spent), [1, 1]);
    assert_eq!(o.fire_zones.len(), 1);
}

#[test]
fn migrated_v4_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture_v4(), "v4");
}

/// Trace of the v4 fixture continued for 600 idle ticks, recorded on the v4
/// build (commit c6f518d) before the ordnance arrays became kind maps.
const V4_CONTINUATION_FNV: u64 = 0x6e44_5102_4b8b_cc56;

#[test]
fn v4_fixture_continues_exactly_as_the_v4_build_did() {
    let mut g = fixture_v4();
    let mut trace = String::new();
    for t in 0..600u32 {
        g.tick(Input::default());
        if t % 50 == 49 {
            let a = g.attempt().unwrap();
            trace.push_str(&format!(
                "t{} hp{} loot{} coins{} rng{} enemies{:?} ord{:?}\n",
                a.tick,
                a.hp,
                a.loot,
                a.coins,
                a.rng.state,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits()))
                    .collect::<Vec<_>>(),
                (a.ordnance.projectiles.len(), a.ordnance.fire_zones.len())
            ));
        }
    }
    let fnv = support::fnv1a(trace.as_bytes());
    assert_eq!(trace.lines().count(), 12);
    assert_eq!(fnv, V4_CONTINUATION_FNV, "{trace}");
}

#[test]
fn migrated_attempt_gear_snapshots_carry_stable_item_ids() {
    let ids = |game: &Game| -> Vec<String> {
        game.attempt()
            .unwrap()
            .gear_snapshot
            .iter()
            .map(|gear| gear.definition.as_str().to_owned())
            .collect()
    };
    assert_eq!(
        ids(&fixture_v2()),
        [
            "worn_service_pistol",
            "crowbar",
            "work_jacket",
            "field_webbing"
        ]
    );
    assert_eq!(ids(&fixture_v3()), ids(&fixture_v2()));
    let v1 = ids(&fixture());
    assert!(!v1.is_empty());
    assert!(v1.iter().all(|id| {
        horde_core::content::content()
            .item_index(&id.as_str().into())
            .is_some()
    }));
}

fn fixture_v5() -> Game {
    Game::decode_save_generation(FIXTURE_V5.as_bytes()).expect("v5 fixture migrates and validates")
}

#[test]
fn v5_fixture_is_a_real_v5_generation() {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V5).unwrap();
    assert_eq!(envelope["version"], 5);
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    let campaign = &payload["campaign"];
    assert_eq!(campaign["unlocked"], 3);
    assert_eq!(
        campaign["first_clears"],
        serde_json::json!([true, true, false, false, false])
    );
    assert_eq!(
        campaign["npc"]["first_clears"],
        serde_json::json!([true, false, false, false, false])
    );
    assert_eq!(
        campaign["npc"]["first_clear_claimed"],
        serde_json::json!([2, 0, 0, 0, 0])
    );
    assert_eq!(campaign["run_history"][0]["template"], 1);
    assert_eq!(payload["attempt"]["level"], 2);
}

#[test]
fn migrated_v5_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture_v5(), "v5");
}

/// Trace of the v5 fixture continued for 600 idle ticks, recorded on the v5
/// build (commit b9d1fd4) before level progress was keyed by level id.
const V5_CONTINUATION_FNV: u64 = 0xbc2a_f76c_324f_6287;

#[test]
fn v5_fixture_continues_exactly_as_the_v5_build_did() {
    let mut g = fixture_v5();
    let mut trace = String::new();
    for t in 0..600u32 {
        g.tick(Input::default());
        if t % 50 == 49 {
            let a = g.attempt().unwrap();
            trace.push_str(&format!(
                "t{} hp{} loot{} coins{} rng{} level{} enemies{:?}
",
                a.tick,
                a.hp,
                a.loot,
                a.coins,
                a.rng.state,
                a.level,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    let fnv = support::fnv1a(trace.as_bytes());
    assert_eq!(trace.lines().count(), 12);
    assert_eq!(fnv, V5_CONTINUATION_FNV, "{trace}");
}

/// Level progress of a decoded game: campaign clears, frontier, NPC clears and
/// claimed first-clear points, keyed by stable level id.
type LevelProgress<'a> = (Vec<&'a str>, usize, Vec<&'a str>, Vec<(&'a str, u32)>);

fn level_progress(game: &Game) -> LevelProgress<'_> {
    let c = game.campaign();
    (
        c.first_clears.iter().map(String::as_str).collect(),
        c.unlocked(),
        c.npc.first_clears.iter().map(String::as_str).collect(),
        c.npc
            .first_clear_claimed
            .iter()
            .map(|(id, n)| (id.as_str(), *n))
            .collect(),
    )
}

#[test]
fn v5_fixture_level_progress_becomes_level_ids_with_the_same_frontier() {
    let game = fixture_v5();
    assert_eq!(
        level_progress(&game),
        (
            vec!["ashwood_crossing", "the_old_road"],
            3,
            vec!["the_old_road"],
            vec![("the_old_road", 2)]
        )
    );
    // In-memory level positions are unchanged; on disk they are level ids.
    assert_eq!(game.attempt().unwrap().level, 2);
    assert_eq!(game.campaign().run_history[0].template, 1);
    let saved = serde_json::to_value(&game).unwrap();
    assert_eq!(saved["attempt"]["level"], "depot_nine");
    assert_eq!(
        saved["campaign"]["run_history"][0]["template"],
        "ashwood_crossing"
    );
    assert!(saved["campaign"].get("unlocked").is_none());
    assert_eq!(
        saved["campaign"]["first_clears"],
        serde_json::json!(["ashwood_crossing", "the_old_road"])
    );
    assert_eq!(
        saved["campaign"]["npc"]["first_clear_claimed"],
        serde_json::json!({"the_old_road": 2})
    );
}

#[test]
fn every_legacy_fixture_keeps_its_clears_frontier_and_claims() {
    let none = || (vec![], 1, vec![], vec![]);
    assert_eq!(
        level_progress(&fixture()),
        (
            vec!["ashwood_crossing", "depot_nine", "the_old_road"],
            4,
            vec!["the_old_road"],
            vec![("the_old_road", 2)]
        )
    );
    assert_eq!(level_progress(&fixture_v2()), none());
    assert_eq!(level_progress(&fixture_v3()), none());
    assert_eq!(level_progress(&fixture_v4()), none());
    assert_eq!(
        level_progress(&fixture_v6()),
        (
            vec![
                "ashwood_crossing",
                "depot_nine",
                "lastlight_checkpoint",
                "the_iron_yard",
                "the_old_road"
            ],
            5,
            vec!["depot_nine"],
            vec![]
        )
    );
    for game in [
        fixture(),
        fixture_v2(),
        fixture_v3(),
        fixture_v4(),
        fixture_v6(),
    ] {
        assert_eq!(game.attempt().unwrap().level, 0);
    }
    assert_eq!(fixture().campaign().run_history[0].template, 0);
    assert_eq!(fixture_v3().campaign().run_history[0].template, 0);
    let templates: Vec<_> = fixture_v6()
        .campaign()
        .run_history
        .iter()
        .map(|r| r.template)
        .collect();
    assert_eq!(templates, [0, 1, 2, 0, 1, 2, 0, 1, 2, 3, 4, 0, 1, 2]);
}

fn fixture_v6() -> Game {
    Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture validates")
}

/// The saved JSON of the v6 fixture's game.
fn payload_v6() -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_str(FIXTURE_V6).unwrap();
    assert_eq!(envelope["version"], 6);
    serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap()
}

/// Every object key anywhere in `value`.
fn all_keys(value: &serde_json::Value, keys: &mut std::collections::BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                keys.insert(key.clone());
                all_keys(child, keys);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|item| all_keys(item, keys)),
        _ => {}
    }
}

#[test]
fn v6_fixture_is_a_real_v6_generation() {
    let payload = payload_v6();
    let campaign = &payload["campaign"];
    assert_eq!(campaign["balance_settings"]["preset"], "Hard");
    let history = campaign["run_history"].as_array().unwrap();
    assert_eq!(history[0]["balance"]["preset"], "Custom");
    assert!(history.iter().any(|r| r["worker_id"].is_null()));
    assert!(history.iter().any(|r| r["worker_id"].is_u64()));
    assert_eq!(campaign["victories"], 14);
    assert_eq!(campaign["npc"]["awarded"], 52);
    assert_eq!(campaign["first_clears"].as_array().unwrap().len(), 5);
    let statuses: Vec<_> = payload["strategy"]["roster"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["Guarding", "Ready", "Deployed"]);
    let battles = payload["battles"].as_array().unwrap();
    let settled: Vec<_> = battles.iter().map(|b| b["settled"] == true).collect();
    assert_eq!(
        settled,
        [true, false],
        "one settled and one live expedition"
    );
    assert_eq!(battles[0]["game"]["last_result"]["outcome"], "Victory");
    assert_eq!(payload["attempt"]["mode"], "Personal");
    assert_eq!(payload["last_result"]["mode"], "Npc");
    // No v7 key exists yet: no defender seat on any attempt or report, and
    // no auto-sell rule or tracked drop anywhere.
    let mut sites = vec![&payload["attempt"], &payload["last_result"]];
    for battle in battles {
        sites.extend([&battle["game"]["attempt"], &battle["game"]["last_result"]]);
    }
    for site in sites.into_iter().filter(|site| !site.is_null()) {
        assert!(site.get("defender").is_none(), "{site}");
    }
    let mut keys = std::collections::BTreeSet::new();
    all_keys(&payload, &mut keys);
    assert!(!keys.contains("auto_sell") && !keys.contains("drop_ids"));
}

#[test]
fn v6_fixture_decodes_with_the_recorded_campaign_state() {
    let game = fixture_v6();
    let c = game.campaign();
    assert_eq!(c.progression_rank(), 14);
    assert_eq!((c.player.level, c.allowance()), (33, 52));
    assert_eq!(c.world.cleared, ["MDA", "ROU", "UKR", "HUN"]);
    let roster: Vec<_> = game
        .strategy()
        .roster
        .iter()
        .map(|r| (r.id, r.status, r.npc.awarded))
        .collect();
    assert_eq!(
        roster,
        [
            (1, RecruitStatus::Guarding, 0),
            (2, RecruitStatus::Ready, 52),
            (3, RecruitStatus::Deployed, 52)
        ]
    );
    // The Ready veteran still carries bought gear from the shared stash.
    let veteran = &game.strategy().roster[1];
    assert!(veteran.npc.loadout.iter().all(|id| *id > 79));
    let quality = |id: u64| c.item(id).and_then(|i| i.quality);
    let locked: Vec<_> = c
        .items
        .iter()
        .filter(|i| i.locked && !i.protected)
        .collect();
    let favourites: Vec<_> = c.items.iter().filter(|i| i.favorite).collect();
    assert_eq!(locked.len(), 1);
    assert_eq!(quality(locked[0].id), Some(Quality::Epic));
    assert_eq!(favourites.len(), 1);
    assert_eq!(quality(favourites[0].id), Some(Quality::Rare));
    assert_eq!(c.presets.len(), 2);
    let live = &game.battles()[1];
    assert!(!live.settled && live.synced_item_index > 8);
    assert_eq!(live.game().attempt().unwrap().loot, 1);
    let a = game.attempt().unwrap();
    assert_eq!(
        (a.tick, a.progression.rank, a.mode),
        (423, 14, Mode::Personal)
    );
    assert!(!a.enemies.is_empty() && !a.corpses.is_empty() && a.loot_events.len() == 1);
    let mara = game.last_result().unwrap();
    assert_eq!((mara.mode, mara.outcome), (Mode::Npc, Outcome::Victory));
    assert!(!mara.loot_events.is_empty());
}

#[test]
fn migrated_v6_fixture_resaves_as_current_version_and_resumes_byte_identically() {
    resave_and_resume_byte_identically(fixture_v6(), "v6");
}

/// FNV-1a of the v6 fixture continued for 600 idle ticks (sampled every 50),
/// recorded on the v6 build at parent commit 3d492cd. Each line is the v5
/// line for the root attempt plus, per expedition, its settled flag, tick,
/// hp, RNG state and enemy count. Campaign coins, items and the strategy RNG
/// are left out: reward and strategy policy may move in waves 2 and 4.
const V6_CONTINUATION_FNV: u64 = 0x90a0_13c2_877b_6b59;

#[test]
fn v6_fixture_continues_exactly_as_the_v6_build_did() {
    let mut g = fixture_v6();
    let mut trace = String::new();
    for t in 0..600u32 {
        g.tick(Input::default());
        if t % 50 == 49 {
            let a = g.attempt().unwrap();
            trace.push_str(&format!(
                "t{} hp{} loot{} coins{} rng{} level{} enemies{:?}",
                a.tick,
                a.hp,
                a.loot,
                a.coins,
                a.rng.state,
                a.level,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits()))
                    .collect::<Vec<_>>()
            ));
            for battle in g.battles() {
                let worker = battle.game().attempt();
                trace.push_str(&format!(
                    " | b{} settled{} {:?}",
                    battle.id,
                    battle.settled,
                    worker.map(|a| (a.tick, a.hp, a.rng.state, a.enemies.len()))
                ));
            }
            trace.push('\n');
        }
    }
    assert!(
        !g.battles()[1].settled,
        "the live expedition outlasts 600 ticks"
    );
    let fnv = support::fnv1a(trace.as_bytes());
    assert_eq!(trace.lines().count(), 12);
    assert_eq!(fnv, V6_CONTINUATION_FNV, "{trace}");
}

/// FNV-1a of the first drops after the v6 fixture resumes under the smoke bot
/// (definition, quality, level and the attempt RNG state after each), recorded
/// on the v6 build at parent commit 3d492cd. Attempts deployed before v7 keep
/// this legacy drop path byte-identical (ruling E11); asserted from W2-06.
const V6_DROP_TRACE_FNV: u64 = 0x2016_774b_b9f6_3c67;

/// The v6 fixture's root attempt and live expedition, resumed under the smoke
/// bot until the root attempt has dropped three more items.
fn v6_drop_trace() -> String {
    let mut g = fixture_v6();
    let mut seen = [
        g.attempt().unwrap().loot_events.last().unwrap().item_id,
        g.battles()[1]
            .game()
            .attempt()
            .unwrap()
            .loot_events
            .last()
            .unwrap()
            .item_id,
    ];
    let mut trace = String::new();
    let mut root_drops = 0;
    // Bounded like the fixture writer's drop search: an attempt that stays
    // alive without dropping fails here instead of hanging the suite.
    let resumed = g.attempt().unwrap().tick;
    while root_drops < 3 {
        let input = smoke_input(&g);
        g.tick(input);
        let attempts = [g.attempt(), g.battles()[1].game().attempt()];
        assert!(
            attempts[0].is_some(),
            "the root attempt ended first:\n{trace}"
        );
        assert!(
            attempts[0].unwrap().tick < resumed + 3600,
            "no third root drop within a minute of the resume:\n{trace}"
        );
        for (source, attempt) in attempts.into_iter().enumerate() {
            let Some(a) = attempt else { continue };
            for event in a.loot_events.iter().filter(|e| e.item_id > seen[source]) {
                trace.push_str(&format!(
                    "{source} t{} {} {:?} L{} rng{}\n",
                    a.tick, event.definition, event.quality, event.level, a.rng.state
                ));
                root_drops += usize::from(source == 0);
            }
            if let Some(last) = a.loot_events.last() {
                seen[source] = seen[source].max(last.item_id);
            }
        }
    }
    trace
}

#[test]
fn v6_drop_trace_is_pinned() {
    let trace = v6_drop_trace();
    assert!(trace.lines().count() >= 3);
    assert_eq!(
        support::fnv1a(trace.as_bytes()),
        V6_DROP_TRACE_FNV,
        "{trace}"
    );
}

/// FNV-1a of 10,000 quality rolls over seeded (seed, attempt, enemy, rank)
/// tuples, recorded on the v6 build at parent commit 3d492cd. Pins the keyed
/// quality roll before its mixer moves to `crate::hash` (W1-09).
const QUALITY_ROLL_TABLE_FNV: u64 = 0x1f6b_c282_6f97_e583;

#[test]
fn quality_roll_table_is_pinned() {
    let catalog = horde_core::quality::catalog();
    let mut tuples = horde_core::rules::Rng::new(0x5155_414C_5441_424C);
    let mut counts = [0u32; Quality::ALL.len()];
    let mut table = String::new();
    for n in 0..10_000u32 {
        let seed = tuples.next_u64();
        let attempt = tuples.next_u64() % 4096;
        let enemy = tuples.next_u64() % 4096;
        let rank = if n % 100 == 99 { 1000 + n % 7 } else { n % 41 };
        let quality = catalog.roll(seed, attempt, enemy, rank);
        counts[Quality::ALL.iter().position(|q| *q == quality).unwrap()] += 1;
        table.push_str(quality.name());
        table.push('\n');
    }
    assert_eq!(
        support::fnv1a(table.as_bytes()),
        QUALITY_ROLL_TABLE_FNV,
        "counts {counts:?}"
    );
}

/// FNV-1a of the v6 fixture stash: per item, the effective stats the quality
/// and level scaling produce and the sale value, recorded on the v6 build at
/// parent commit 3d492cd.
const V6_STASH_TABLE_FNV: u64 = 0x7868_6fdb_47d0_2c09;

#[test]
fn v6_stash_item_stats_and_values_are_pinned() {
    let game = fixture_v6();
    let table = v6_stash_table(&game);
    assert_eq!(table.lines().count(), game.campaign().items.len());
    assert_eq!(
        support::fnv1a(table.as_bytes()),
        V6_STASH_TABLE_FNV,
        "{table}"
    );
}

/// The v7 loot schema (W1-21) leaves every v6 stash item's stats and value
/// as the v6 build computed them: `loot_only` defaults to false, so each
/// definition stays purchasable exactly when it has a shop tier.
#[test]
fn v6_items_keep_stats_and_value() {
    let game = fixture_v6();
    let table = v6_stash_table(&game);
    assert_eq!(table.lines().count(), game.campaign().items.len());
    assert_eq!(
        support::fnv1a(table.as_bytes()),
        V6_STASH_TABLE_FNV,
        "{table}"
    );
    for item in &game.campaign().items {
        let definition = item.definition();
        assert!(!definition.loot_only, "{}", item.definition);
        assert_eq!(definition.purchasable(), definition.tier > 0);
    }
}

/// The bundled enemy catalogue, as the build ships it (no `LASTLIGHT_ENEMIES`
/// override).
const ENEMY_CATALOG_JSON: &str = include_str!("../enemy_catalog.json");
/// The bundled content catalogue (no `LASTLIGHT_CONTENT` override).
const CONTENT_JSON: &str = include_str!("../content.json");

/// The 11 archetypes the V1-V6 encounters can spawn, in catalogue order.
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
/// Wave tiers 0..=4 of `enemy_catalog.json`; new content enters at tier 5.
const FROZEN_WAVE_TIERS: std::ops::RangeInclusive<usize> = 0..=4;

/// FNV-1a of [`frozen_catalogue_table`] on the bundled catalogues, recorded
/// at the wave-1 close (W2-53, parent af530f0), re-recorded when the legacy
/// kind defaults joined the table (W2-53 review). Design-save-determinism
/// invariant 3: these are the catalogue records a saved V1-V6 encounter
/// ticks, so they are append-only (ISO-3). A move means a frozen record was
/// edited: fix the edit. A content change meant to reach legacy attempts
/// (Ruling R4) re-blesses the continuation pins it moves in that wave's
/// GOLDEN re-bless with a written reason; there is no `rules_revision`.
const FROZEN_CATALOGUE_FNV: u64 = 0x58a8_acb7_5443_1bad;

/// One line per frozen record, as the catalogues resolve it: each frozen
/// archetype's `ResolvedEnemy`, the archetype each legacy `EnemyKind` defaults
/// to (boss and event spawns go through `resolve_kind`, and a saved enemy
/// without a snapshot falls back to `default_profile`), each frozen wave
/// tier's entries (sorted by archetype id, the order `EnemyCatalog` keeps) and
/// each of the 5 content levels. Records are serialized through their types, so a new field at its
/// skipped neutral default writes nothing. Wave entries come from the authored
/// JSON rather than `wave_pool`, so an entry locked behind any boss still
/// counts. Panics when either catalogue fails to load.
fn frozen_catalogue_table(enemy_json: &str, content_json: &str) -> String {
    use horde_core::enemy_catalog::{EnemyCatalog, WeightedEnemy};
    let catalog = EnemyCatalog::from_json_str(enemy_json).expect("the enemy catalogue loads");
    let content =
        horde_core::content::Content::from_json_str(content_json).expect("the content loads");
    let mut table = String::new();
    for id in FROZEN_ARCHETYPES {
        let enemy = catalog.resolve(id).unwrap();
        table.push_str(&format!(
            "archetype {id} {}\n",
            serde_json::to_string(&enemy).unwrap()
        ));
    }
    for kind in EnemyKind::ALL {
        let id = catalog.default_for_kind(kind);
        table.push_str(&format!("default {kind:?} {id}\n"));
    }
    let authored: serde_json::Value = serde_json::from_str(enemy_json).unwrap();
    let waves = authored["waves"].as_array().expect("a waves array");
    for tier in FROZEN_WAVE_TIERS {
        let wave = waves
            .iter()
            .find(|wave| wave["level"].as_u64() == Some(tier as u64))
            .unwrap_or_else(|| panic!("wave tier {tier} exists"));
        let mut entries: Vec<WeightedEnemy> =
            serde_json::from_value(wave["entries"].clone()).unwrap();
        entries.sort_by(|a, b| a.archetype_id.cmp(&b.archetype_id));
        table.push_str(&format!(
            "wave {tier} {}\n",
            serde_json::to_string(&entries).unwrap()
        ));
    }
    assert_eq!(content.levels.len(), 5, "the 5 content levels");
    for (index, level) in content.levels.iter().enumerate() {
        table.push_str(&format!(
            "level {index} {}\n",
            serde_json::to_string(level).unwrap()
        ));
    }
    table
}

/// The bundled enemy catalogue with `edit` applied to its authored JSON.
fn edited_enemy_catalog(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut authored: serde_json::Value = serde_json::from_str(ENEMY_CATALOG_JSON).unwrap();
    edit(&mut authored);
    serde_json::to_string(&authored).unwrap()
}

#[test]
fn legacy_catalogue_records_that_tick_saved_encounters_are_frozen() {
    let table = frozen_catalogue_table(ENEMY_CATALOG_JSON, CONTENT_JSON);
    assert_eq!(
        table.lines().count(),
        FROZEN_ARCHETYPES.len() + EnemyKind::ALL.len() + FROZEN_WAVE_TIERS.count() + 5
    );
    assert_eq!(
        support::fnv1a(table.as_bytes()),
        FROZEN_CATALOGUE_FNV,
        "a frozen catalogue record moved (append-only, ISO-3):\n{table}"
    );

    // Appending an archetype and the next wave tier leaves the table as is.
    let appended = edited_enemy_catalog(|authored| {
        let mut archetype = authored["archetypes"][0].clone();
        archetype["id"] = "frozen-guard-probe".into();
        archetype["name"] = "Frozen Guard Probe".into();
        authored["archetypes"]
            .as_array_mut()
            .unwrap()
            .push(archetype);
        let waves = authored["waves"].as_array_mut().unwrap();
        let mut wave = waves[0].clone();
        wave["level"] = waves.len().into();
        wave["entries"] = serde_json::json!([{"archetype_id": "frozen-guard-probe", "weight": 9}]);
        waves.push(wave);
    });
    assert_eq!(
        frozen_catalogue_table(&appended, CONTENT_JSON),
        table,
        "appended records are not frozen"
    );

    // Editing a frozen wave tier or a frozen archetype's component moves it.
    let reweighted = edited_enemy_catalog(|authored| {
        let entry = &mut authored["waves"][4]["entries"][0]["weight"];
        *entry = (entry.as_u64().unwrap() + 1).into();
    });
    assert_ne!(frozen_catalogue_table(&reweighted, CONTENT_JSON), table);
    let tougher = edited_enemy_catalog(|authored| {
        let health = &mut authored["bodies"][0]["health"];
        *health = (health.as_i64().unwrap() + 1).into();
    });
    assert_ne!(frozen_catalogue_table(&tougher, CONTENT_JSON), table);

    // Re-pointing a legacy kind default (boss and event spawns, snapshot-less
    // enemies) at another unrestricted archetype of that kind moves it.
    let repointed = edited_enemy_catalog(|authored| {
        let defaults = authored["defaults"].as_array_mut().unwrap();
        let runner = defaults
            .iter_mut()
            .find(|default| default["kind"] == "Runner")
            .unwrap();
        runner["archetype_id"] = "skitter".into();
    });
    assert_ne!(
        frozen_catalogue_table(&repointed, CONTENT_JSON),
        table,
        "a re-pointed kind default moves the table"
    );
}

/// The v6 fixture through the SAVE_VERSION 7 migration: the auto-sell rule
/// is off (the serde default), no attempt tracks drops or has a drop tier
/// (a pre-v7 legacy loot path), no expedition has a pending sale, nothing
/// was auto-sold, and each report's drop ids and quality counts come from
/// its loot receipts (sub-step (b)); the root report's counts come from its
/// own run record.
#[test]
fn v6_save_loads_with_auto_sell_off_and_empty_drop_tracking() {
    let game = fixture_v6();
    let c = game.campaign();
    assert!(c.auto_sell.is_default() && !c.auto_sell.enabled);
    assert!(
        c.run_history
            .iter()
            .all(|r| (r.auto_sold_count, r.auto_sold_coins) == (0, 0))
    );
    let mut attempts = vec![game.attempt().unwrap()];
    let mut reports = vec![(game.last_result().unwrap(), true)];
    for battle in game.battles() {
        assert!(battle.pending_auto_sell.is_empty(), "{}", battle.id);
        attempts.extend(battle.game().attempt());
        reports.extend(battle.game().last_result().map(|r| (r, false)));
    }
    assert!(attempts.len() >= 2 && reports.len() >= 2);
    for a in attempts {
        assert!(a.drop_ids.is_empty() && a.drop_tier.is_none(), "{}", a.id);
    }
    for (r, root) in reports {
        assert!(r.auto_sold.is_empty());
        let mut ids: Vec<u64> = r.loot_events.iter().map(|e| e.item_id).collect();
        ids.sort_unstable();
        assert!(!ids.is_empty(), "every v6 report here has drops");
        assert_eq!(r.drop_ids, ids, "report {}", r.attempt_id);
        let expected = if root {
            c.run_history
                .iter()
                .find(|record| record.attempt_id == r.attempt_id && record.worker_id.is_none())
                .expect("the root report has its run record")
                .loot_by_quality
                .clone()
        } else {
            let mut counted = horde_core::kind_map::KindMap::default();
            for event in &r.loot_events {
                counted[event.quality] += 1;
            }
            counted
        };
        assert_eq!(r.loot_by_quality, expected, "report {}", r.attempt_id);
    }
}

fn v6_stash_table(game: &Game) -> String {
    let c = game.campaign();
    let mut table = String::new();
    for item in &c.items {
        let e = c.effective_item(item.id).unwrap();
        table.push_str(&format!(
            "{} {} L{} {:?} dmg{} melee{} armor{} hp{} stamina{} npc_armor{} npc_add{} value{}\n",
            item.id,
            item.definition,
            item.level,
            item.quality,
            e.personal.damage,
            e.personal.melee_damage,
            e.personal.armor,
            e.personal.health,
            e.personal.stamina,
            e.npc.armor,
            e.npc.damage_add,
            c.item_sale_value(item.id).unwrap()
        ));
    }
    table
}

/// The v6 fixture through the SAVE_VERSION 7 migration: the root report is
/// Mara's (Npc, keyless), the root attempt is the player's, and every worker
/// attempt and report sits in its expedition's recruit seat. No next
/// defender is chosen, so the migrated campaign continues with the report's
/// defender, Mara (the active-defender leg is W2-12's
/// `migrated_v6_continues_with_mara`).
#[test]
fn v6_fixture_defenders_resolve() {
    let game = fixture_v6();
    assert_eq!(game.attempt().unwrap().defender(), DefenderId::Player);
    assert_eq!(game.last_result().unwrap().defender(), DefenderId::Mara);
    assert_eq!(
        game.campaign().next_defender,
        None,
        "no next defender chosen"
    );
    let (mut attempts, mut reports) = (0, 0);
    for battle in game.battles() {
        let seat = DefenderId::Recruit(battle.recruit_id);
        if let Some(a) = battle.game().attempt() {
            assert_eq!(a.defender(), seat, "expedition {}", battle.id);
            attempts += 1;
        }
        if let Some(r) = battle.game().last_result() {
            assert_eq!(r.defender(), seat, "expedition {}", battle.id);
            reports += 1;
        }
    }
    assert!(attempts >= 1 && reports >= 1, "{attempts} {reports}");
    // The seat keys are written for workers only, and load-save-load-save is
    // byte-identical.
    let dir = std::env::temp_dir();
    let path = |n: u8| {
        dir.join(format!(
            "lastlight-v6-seats-{n}-{}.json",
            std::process::id()
        ))
    };
    game.save(&path(1)).unwrap();
    let first = std::fs::read(path(1)).unwrap();
    Game::load(&path(1)).unwrap().save(&path(2)).unwrap();
    let second = std::fs::read(path(2)).unwrap();
    for n in [1, 2] {
        for extension in ["json", "bak"] {
            let _ = std::fs::remove_file(path(n).with_extension(extension));
        }
    }
    assert!(first == second, "the second resave is byte-identical");
    let envelope: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert!(payload["attempt"].get("defender").is_none());
    assert!(payload["last_result"].get("defender").is_none());
    for battle in payload["battles"].as_array().unwrap() {
        let seat = serde_json::json!({"Recruit": battle["recruit_id"]});
        for key in ["attempt", "last_result"] {
            if battle["game"][key].is_object() {
                assert_eq!(battle["game"][key]["defender"], seat, "{key}");
            }
        }
    }
}

/// A v6 Npc attempt saved without an appearance loads as Female through
/// `Game::from_payload`; a current-version payload keeps the serde default.
#[test]
fn v6_npc_attempt_without_appearance_loads_female_through_from_payload() {
    let mut value = serde_json::to_value(support::game(Mode::Npc)).unwrap();
    value["attempt"]["profile"]
        .as_object_mut()
        .unwrap()
        .remove("appearance");
    let migrated = Game::from_payload(value.clone(), 6).unwrap();
    assert_eq!(
        migrated.attempt().unwrap().profile.appearance,
        Appearance::Female
    );
    let current = Game::from_payload(value, SAVE_VERSION).unwrap();
    assert_eq!(
        current.attempt().unwrap().profile.appearance,
        Appearance::Male
    );
}

#[test]
fn decode_rejects_version_8() {
    assert_eq!(SAVE_VERSION, 7);
    let payload = serde_json::to_string(&Game::default()).unwrap();
    let envelope = |version: u32| {
        serde_json::to_vec(&serde_json::json!({
            "version": version,
            "checksum": support::fnv1a(payload.as_bytes()),
            "payload": payload,
        }))
        .unwrap()
    };
    assert!(Game::decode_save_generation(&envelope(7)).is_ok());
    assert!(matches!(
        Game::decode_save_generation(&envelope(8)),
        Err(SaveError::UnsupportedVersion(8))
    ));
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    for version in [0, 8] {
        assert!(
            matches!(
                Game::from_payload(value.clone(), version),
                Err(SaveError::UnsupportedVersion(v)) if v == version
            ),
            "{version}"
        );
    }
    // Every supported version decodes through the same seam.
    for version in 1..=SAVE_VERSION {
        Game::from_payload(value.clone(), version).unwrap();
    }
}

/// Sub-steps (c) and (d) (W1-24) on the v6 fixture: every player, the root's
/// and each expedition's, has its level-up grant marked paid up to its level,
/// and every campaign claims the milestones its rank 14 had reached (5 and
/// 10, not 15), so a v6 save is owed nothing retroactively. The same payload
/// at level 12 is marked paid to 12. Loading and 600 idle ticks pay no coins.
#[test]
fn v6_save_migrates_level_grant_paid_to_level_and_claims_reached_milestones() {
    let reached = std::collections::BTreeSet::from([5, 10]);
    let mut game = fixture_v6();
    let c = game.campaign();
    assert_eq!((c.progression_rank(), c.player.level), (14, 33));
    assert_eq!(c.player.level_grant_paid, 33);
    assert_eq!(c.milestones_claimed, reached);
    assert_eq!(game.battles().len(), 2);
    for battle in game.battles() {
        let worker = battle.game().campaign();
        assert!(worker.player.level >= 2, "expedition {}", battle.id);
        assert_eq!(
            worker.player.level_grant_paid, worker.player.level,
            "expedition {}",
            battle.id
        );
        assert_eq!(
            worker.milestones_claimed, reached,
            "expedition {}",
            battle.id
        );
    }
    // The live expedition earns coins that settlement syncs to the parent;
    // apart from them the parent's coins are unchanged.
    let coins = |g: &Game| {
        let workers: u64 = g.battles().iter().map(|b| b.game().campaign().coins).sum();
        (g.campaign().coins, workers)
    };
    let (parent, workers) = coins(&game);
    for _ in 0..600 {
        game.tick(Input::default());
    }
    let (parent_after, workers_after) = coins(&game);
    assert_eq!(
        parent_after - parent,
        workers_after - workers,
        "no retroactive grant"
    );

    let mut payload = payload_v6();
    payload["campaign"]["player"]["level"] = 12.into();
    payload["campaign"]["player"]["xp"] = 0.into();
    let game = Game::from_payload(payload, 6).expect("a level 12 v6 save loads");
    let c = game.campaign();
    assert_eq!((c.player.level, c.player.level_grant_paid), (12, 12));
    assert_eq!(c.milestones_claimed, reached);
}

/// A level 1 player writes no `level_grant_paid` key and a campaign below
/// rank 5 no `milestones_claimed` key, through the migration and a resave.
#[test]
fn level_one_saves_gain_no_level_grant_key() {
    let synthetic = serde_json::to_value(support::game(Mode::Personal)).unwrap();
    let v2 = Game::decode_save_generation(FIXTURE_V2.as_bytes()).unwrap();
    let synthetic = Game::from_payload(synthetic, 6).unwrap();
    for (label, game) in [
        ("v2", v2),
        ("v3", fixture_v3()),
        ("v4", fixture_v4()),
        ("v6 synthetic", synthetic),
    ] {
        let c = game.campaign();
        assert_eq!(c.player.level, 1, "{label}");
        assert_eq!(c.player.level_grant_paid, 0, "{label}");
        assert!(c.milestones_claimed.is_empty(), "{label}");
        let mut keys = std::collections::BTreeSet::new();
        all_keys(&serde_json::to_value(&game).unwrap(), &mut keys);
        assert!(!keys.contains("level_grant_paid"), "{label}");
        assert!(!keys.contains("milestones_claimed"), "{label}");
    }
    // Below rank 5 a levelled player is marked paid, but nothing is claimed.
    for (label, game, level) in [("v1", fixture(), 18), ("v5", fixture_v5(), 5)] {
        let c = game.campaign();
        assert!(c.progression_rank() < 5, "{label}");
        assert_eq!(c.player.level_grant_paid, level, "{label}");
        assert!(c.milestones_claimed.is_empty(), "{label}");
    }
}

/// The current-format v6 fixture with `edit` applied to its saved JSON.
fn edited_v6(edit: impl FnOnce(&mut serde_json::Value)) -> Result<Game, SaveError> {
    let mut value = serde_json::to_value(fixture_v6()).unwrap();
    edit(&mut value);
    Game::from_payload(value, SAVE_VERSION)
}

fn assert_invalid(result: Result<Game, SaveError>, label: &str) {
    match result {
        Err(SaveError::Invalid(_)) => {}
        other => panic!("{label} loaded: {:?}", other.err()),
    }
}

#[test]
fn rejects_level_grant_paid_above_level() {
    edited_v6(|v| v["campaign"]["player"]["level_grant_paid"] = 33.into()).unwrap();
    assert_invalid(
        edited_v6(|v| v["campaign"]["player"]["level_grant_paid"] = 34.into()),
        "root paid above level",
    );
    edited_v6(|v| v["battles"][0]["game"]["campaign"]["player"]["level_grant_paid"] = 31.into())
        .unwrap();
    assert_invalid(
        edited_v6(|v| {
            v["battles"][0]["game"]["campaign"]["player"]["level_grant_paid"] = 32.into();
        }),
        "worker paid above level",
    );
}

#[test]
fn rejects_milestone_claimed_above_rank() {
    let claim = |set: &[u32]| serde_json::json!(set);
    edited_v6(|v| v["campaign"]["milestones_claimed"] = claim(&[5, 10, 14])).unwrap();
    assert_invalid(
        edited_v6(|v| v["campaign"]["milestones_claimed"] = claim(&[5, 10, 15])),
        "root claim above rank 14",
    );
    // Expedition 13 was cloned at rank 12.
    edited_v6(|v| v["battles"][0]["game"]["campaign"]["milestones_claimed"] = claim(&[12]))
        .unwrap();
    assert_invalid(
        edited_v6(|v| v["battles"][0]["game"]["campaign"]["milestones_claimed"] = claim(&[13])),
        "worker claim above its rank 12",
    );
    // At most 64 claims, whatever the rank.
    let many: Vec<u32> = (1..=65).collect();
    edited_v6(|v| {
        v["campaign"]["victories"] = 100.into();
        v["campaign"]["milestones_claimed"] = claim(&many[..64]);
    })
    .unwrap();
    assert_invalid(
        edited_v6(|v| {
            v["campaign"]["victories"] = 100.into();
            v["campaign"]["milestones_claimed"] = claim(&many);
        }),
        "65 claims",
    );
}

/// The load-path leg of the save repair (W1 review): a v6 save whose Ready
/// recruit has a weapon slot pointing at an item the stash no longer holds
/// (a damaged or hand-edited file) still loads. `reconcile_saved_game` puts
/// the slot back on that recruit's own protected starter weapon, the bundle
/// issued at recruitment (recruit 2 is the second roster ordinal), before
/// validation runs, and leaves the other slots and recruits as saved.
#[test]
fn v6_save_with_an_unowned_recruit_item_loads_on_that_recruits_starter() {
    let mut payload = payload_v6();
    let roster = payload["strategy"]["roster"].as_array_mut().unwrap();
    assert_eq!(roster[1]["id"], 2);
    assert_eq!(roster[1]["status"], "Ready");
    let saved: Vec<serde_json::Value> =
        roster.iter().map(|r| r["npc"]["loadout"].clone()).collect();
    let missing = 9_999_999u64;
    roster[1]["npc"]["loadout"][0] = missing.into();
    let game = Game::from_payload(payload, 6).expect("the repair runs before validation");
    let c = game.campaign();
    assert!(c.item(missing).is_none());
    let recruits = &game.strategy().roster;
    let repaired = recruits[1].npc.loadout[0];
    // Recruit 2's starter weapon in the fixture, not Mara's (item 1).
    assert_eq!(repaired, 72);
    let item = c.item(repaired).unwrap();
    assert!(item.protected && item.definition_index() == 0);
    let mut expected = saved[1].clone();
    expected[0] = repaired.into();
    assert_eq!(serde_json::json!(recruits[1].npc.loadout), expected);
    for n in [0, 2] {
        assert_eq!(
            serde_json::json!(recruits[n].npc.loadout),
            saved[n],
            "recruit {}",
            n + 1
        );
    }
}

// ---------------------------------------------------------------------------
// save_v7.json: the schema-freeze file (W1-63; schema-v7.md section 11).
// ---------------------------------------------------------------------------

const FIXTURE_V7: &str = include_str!("fixtures/save_v7.json");
/// The frozen file's size budget (schema-v7.md section 11).
const V7_FIXTURE_MAX_BYTES: usize = 96 * 1024;
/// The frozen file's identity: its envelope checksum (FNV-1a of the payload)
/// and its byte length. A re-cut fails here and goes through the
/// stop-and-ask path the freeze declares.
const V7_FIXTURE_CHECKSUM: u64 = 8_527_615_155_064_533_385;
const V7_FIXTURE_BYTES: usize = 95_266;

fn fixture_v7() -> Game {
    Game::decode_save_generation(FIXTURE_V7.as_bytes()).expect("v7 fixture loads and validates")
}

/// The v7 fixture's envelope, as `Game::save` wrote it.
fn envelope_v7() -> serde_json::Value {
    serde_json::from_str(FIXTURE_V7).unwrap()
}

/// The saved JSON of the v7 fixture's game.
fn payload_v7() -> serde_json::Value {
    serde_json::from_str(envelope_v7()["payload"].as_str().unwrap()).unwrap()
}

/// Every value at the registry key path `path` (see
/// `support::values_at_key_path`), in the root game and in every worker game.
fn v7_values<'a>(payload: &'a serde_json::Value, path: &str) -> Vec<&'a serde_json::Value> {
    let mut values = support::values_at_key_path(payload, path);
    values.extend(support::values_at_key_path(
        payload,
        &format!("{}{path}", support::WORKER_GAME_PREFIX),
    ));
    values
}

/// Whether `value` is what the v7 field `key` (the last path segment) holds
/// at its default, the value convention C1 skips: null, false, zero, empty,
/// an object of such values (an all-zero quality map, an empty receipt), or
/// the field's named default.
fn is_v7_default(key: &str, value: &serde_json::Value) -> bool {
    use serde_json::Value;
    fn empty(value: &Value) -> bool {
        match value {
            Value::Null => true,
            Value::Bool(b) => !b,
            Value::Number(n) => n.as_f64() == Some(0.0),
            Value::String(s) => s.is_empty(),
            Value::Array(items) => items.is_empty(),
            Value::Object(map) => map.values().all(empty),
        }
    }
    let named = match key {
        "auto_sell" => serde_json::to_value(horde_core::campaign::AutoSellRule::default()).unwrap(),
        "defender" => serde_json::json!("ModeOwner"),
        "boss_rank" => serde_json::json!("Sector"),
        "balance_extras_basis" => serde_json::json!("Normal"),
        "group_size" => serde_json::json!(1),
        "front_ramp" | "hazard_damage_multiplier" => serde_json::json!(1.0),
        key if key.starts_with("boss_") && key.ends_with("_multiplier") => serde_json::json!(1.0),
        _ => Value::Null,
    };
    empty(value) || *value == named
}

/// The frozen file is a real save format 7 generation, written by
/// `Game::save`, within its size budget, and every field save format 7 added
/// (`support::V7_SAVED_KEYS`) occurs in it at a non-default value.
#[test]
fn v7_fixture_is_a_real_v7_generation() {
    assert!(
        FIXTURE_V7.len() <= V7_FIXTURE_MAX_BYTES,
        "save_v7.json is {} bytes",
        FIXTURE_V7.len()
    );
    let envelope = envelope_v7();
    assert_eq!(
        (envelope["checksum"].as_u64(), FIXTURE_V7.len()),
        (Some(V7_FIXTURE_CHECKSUM), V7_FIXTURE_BYTES),
        "save_v7.json is frozen and never re-cut"
    );
    assert_eq!(envelope["version"], 7);
    assert!(envelope["ruleset_version"].is_u64());
    assert!(envelope["content_fingerprint"].is_u64());
    let payload = payload_v7();
    let mut missing = Vec::new();
    for (section, path) in support::V7_SAVED_KEYS {
        let key = path.rsplit('/').next().unwrap();
        if v7_values(&payload, path)
            .into_iter()
            .all(|value| is_v7_default(key, value))
        {
            missing.push(format!("{section} {path}"));
        }
    }
    assert!(
        missing.is_empty(),
        "never at a non-default value: {missing:?}"
    );
    // The section 11 content beyond the defender keys.
    let campaign = &payload["campaign"];
    assert_eq!(campaign["balance_settings"]["preset"], "Custom");
    assert_eq!(campaign["balance_extras_basis"], "Hard");
    assert_eq!(campaign["auto_sell"]["enabled"], true);
    let snapshot = &payload["attempt"]["progression"];
    assert_eq!(snapshot["wave_tier"], 11);
    assert_eq!(snapshot["boss_rank"], "Capstone");
    assert_eq!(snapshot["boss_phases"], 2);
    assert!(snapshot["elite_rate_bp"].as_u64().unwrap() > 0);
    for key in [
        "front_ramp",
        "boss_armor_multiplier",
        "boss_health_multiplier",
        "boss_reward_multiplier",
        "boss_damage_multiplier",
    ] {
        assert!(snapshot[key].as_f64().unwrap() > 1.0, "{key}");
    }
    let enemies = payload["attempt"]["enemies"].as_array().unwrap();
    let resolved = |e: &serde_json::Value, key: &str| e["resolved"][key].clone();
    assert!(enemies.iter().any(|e| resolved(e, "elite").is_object()));
    assert!(enemies.iter().any(|e| {
        resolved(e, "archetype_id") == "spitter"
            && resolved(e, "reach").as_f64().unwrap_or(0.0) > 0.0
            && e["attack_phase"] == "Windup"
            && e["contact_since"].is_null()
    }));
    assert!(enemies.iter().any(|e| e["split_child"] == true));
    assert!(
        enemies
            .iter()
            .any(|e| e["boss"] == true && e["boss_state"]["entered"] == 1)
    );
    let modifiers = &payload["attempt"]["profile"]["modifiers"];
    assert_eq!(modifiers["class_modifiers"].as_object().unwrap().len(), 1);
    assert!(modifiers["hazard_damage_multiplier"].as_f64().unwrap() < 1.0);
}

/// The defender state of section 11, by JSON pointer: a live depth-0 Npc
/// defense held by recruit 1 (Defending, its loadout reserved, its identity
/// appearance), recruit 2 named as the next defender, one settled
/// unacknowledged expedition and one live one in their recruits' seats, a
/// folded outcome, a containment and a main-run takeover record.
#[test]
fn v7_fixture_holds_every_defender_key_at_non_default() {
    use serde_json::json;
    let payload = payload_v7();
    let (defending, next, settled, live) = (1u64, 2u64, 2u64, 3u64);
    let appearance = horde_core::defenders::recruit_identity(defending).1;
    for (pointer, expected) in [
        ("/attempt/mode", json!("Npc")),
        ("/attempt/defender", json!({"Recruit": defending})),
        ("/attempt/profile/appearance", json!(appearance)),
        ("/campaign/reserved", json!("Npc")),
        ("/campaign/reserved_recruit", json!(defending)),
        ("/campaign/next_defender", json!({"Recruit": next})),
        ("/strategy/roster/0/id", json!(defending)),
        ("/strategy/roster/0/status", json!("Defending")),
        ("/strategy/roster/1/id", json!(next)),
        ("/strategy/roster/1/status", json!("Ready")),
        ("/strategy/roster/2/id", json!(live)),
        ("/strategy/roster/2/status", json!("Deployed")),
        ("/battles/0/recruit_id", json!(settled)),
        ("/battles/0/settled", json!(true)),
        (
            "/battles/0/game/last_result/defender",
            json!({"Recruit": settled}),
        ),
        ("/battles/0/summary/acknowledged", json!(false)),
        ("/battles/1/recruit_id", json!(live)),
        ("/battles/1/settled", json!(false)),
        ("/battles/1/game/attempt/defender", json!({"Recruit": live})),
    ] {
        assert_eq!(payload.pointer(pointer), Some(&expected), "{pointer}");
    }
    let reserved = payload["campaign"]["reserved_items"].as_array().unwrap();
    for item in payload["strategy"]["roster"][0]["npc"]["loadout"]
        .as_array()
        .unwrap()
    {
        assert!(reserved.contains(item), "loadout item {item} is reserved");
    }
    assert!(payload["strategy"]["folded_outcomes"].as_u64().unwrap() > 0);
    assert!(
        payload["strategy"]["roster"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["containments"].as_u64().unwrap_or(0) > 0)
    );
    let history = payload["campaign"]["run_history"].as_array().unwrap();
    // The main-run takeover record carries its receipts, and the restored
    // root report of the same attempt names the same recruit.
    let takeover = history
        .iter()
        .find(|r| r["worker_id"].is_null() && r["recruit_id"].is_u64() && r["mode"] == "Npc")
        .expect("a main-run takeover record");
    for key in [
        "auto_sold_count",
        "level_grant_coins",
        "milestone_coins",
        "elite_kills",
        "boss_ttk_ticks",
        "boss_rank",
    ] {
        assert!(
            !is_v7_default(key, &takeover[key]),
            "the takeover record's {key} is {}",
            takeover[key]
        );
    }
    let report = &payload["last_result"];
    assert_eq!(report["attempt_id"], takeover["attempt_id"]);
    assert_eq!(
        report["defender"],
        json!({"Recruit": takeover["recruit_id"]}),
        "the takeover record and its report name one defender"
    );
    // A worker's pending record is that expedition's own run: its country,
    // and an attempt the parent has not recorded yet.
    let mut pending = 0;
    for battle in payload["battles"].as_array().unwrap() {
        for record in battle["game"]["pending_runs"].as_array().unwrap() {
            assert_eq!(record["country_id"], battle["country_id"]);
            assert!(
                history
                    .iter()
                    .all(|r| r["attempt_id"] != record["attempt_id"]),
                "pending attempt {} is already in run_history",
                record["attempt_id"]
            );
            pending += 1;
        }
    }
    assert!(pending > 0, "a worker holds a pending run record");
    // The typed reads agree.
    let game = fixture_v7();
    assert_eq!(
        game.attempt().unwrap().defender(),
        DefenderId::Recruit(defending)
    );
    assert_eq!(
        game.campaign().next_defender,
        Some(DefenderId::Recruit(next))
    );
    assert_eq!(game.strategy().roster[0].status, RecruitStatus::Defending);
    assert_eq!(
        game.battles()[0].game().last_result().unwrap().defender(),
        DefenderId::Recruit(settled)
    );
    assert_eq!(
        game.battles()[1].game().attempt().unwrap().defender(),
        DefenderId::Recruit(live)
    );
}

/// The frozen file loads, validates and saves back to the very same payload,
/// and a second resave is byte-identical to the first. One flipped payload
/// byte is refused.
#[test]
fn v7_fixture_loads_validates_and_resaves_byte_identically() {
    let envelope = envelope_v7();
    let saved = envelope["payload"].as_str().unwrap();
    let game = fixture_v7();
    assert_eq!(game.to_save_json().unwrap(), saved);
    let dir = std::env::temp_dir();
    let path = |n: u8| {
        dir.join(format!(
            "lastlight-v7-resave-{n}-{}.json",
            std::process::id()
        ))
    };
    game.save(&path(1)).unwrap();
    let first = std::fs::read(path(1)).unwrap();
    Game::load(&path(1)).unwrap().save(&path(2)).unwrap();
    let second = std::fs::read(path(2)).unwrap();
    for n in [1, 2] {
        for extension in ["json", "bak"] {
            let _ = std::fs::remove_file(path(n).with_extension(extension));
        }
    }
    assert!(first == second, "the second resave is byte-identical");
    let resaved: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(resaved["version"], SAVE_VERSION);
    assert_eq!(resaved["payload"].as_str().unwrap(), saved);
    // The checksum guards every payload byte: the last digit of the seed,
    // flipped to another digit, still parses as a valid, typed payload and is
    // refused on the checksum alone.
    let key = "\\\"seed\\\":";
    let at = FIXTURE_V7.find(key).expect("the payload holds a seed") + key.len();
    let digits = FIXTURE_V7[at..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    assert!(digits > 1, "the seed is a multi-digit number");
    let mut flipped = FIXTURE_V7.as_bytes().to_vec();
    flipped[at + digits - 1] ^= 0x01;
    assert!(flipped[at + digits - 1].is_ascii_digit());
    assert!(matches!(
        Game::decode_save_generation(&flipped),
        Err(SaveError::ChecksumMismatch)
    ));
}

/// Every enemy and balance field save format 7 added (sections 3.1
/// `balance_extras_basis` and 3.8-3.12) survives load and resave with the
/// same saved text, and the loaded game digests to the saved checksum.
#[test]
fn v7_fixture_round_trips_every_enemy_balance_field_to_equal_digest() {
    let envelope = envelope_v7();
    let payload = payload_v7();
    let game = fixture_v7();
    assert_eq!(
        serde_json::json!(game.state_digest()),
        envelope["checksum"],
        "the loaded game digests to the saved checksum"
    );
    let resaved: serde_json::Value = serde_json::from_str(&game.to_save_json().unwrap()).unwrap();
    let reloaded: Game = serde_json::from_value(resaved.clone()).unwrap();
    assert_eq!(reloaded.state_digest(), game.state_digest());
    let sections = ["3.8 ", "3.9 ", "3.10 ", "3.11 ", "3.12 "];
    let mut checked = 0;
    for (section, path) in support::V7_SAVED_KEYS {
        if !(sections.iter().any(|s| section.starts_with(s))
            || path.ends_with("/balance_extras_basis"))
        {
            continue;
        }
        let before = v7_values(&payload, path);
        assert!(!before.is_empty(), "{section} {path} is saved");
        assert_eq!(before, v7_values(&resaved, path), "{section} {path}");
        checked += 1;
    }
    assert_eq!(checked, 37);
    // The typed values behind the saved text.
    let a = game.attempt().unwrap();
    let snapshot = &a.progression;
    assert_eq!(snapshot.wave_tier, 11);
    assert_eq!(
        snapshot.boss_rank,
        horde_core::progression::BossRank::Capstone
    );
    assert_eq!(snapshot.boss_phases, 2);
    let boss = a.enemies.iter().find(|e| e.boss).unwrap();
    let state = boss.boss_state.as_ref().unwrap();
    assert_eq!((state.transitions, state.entered), (2, 1));
    assert_eq!(a.telemetry.boss_phase_ticks.len(), 1);
    assert_eq!(
        game.campaign().balance_extras_basis,
        horde_core::balance::BalancePreset::Hard
    );
}
