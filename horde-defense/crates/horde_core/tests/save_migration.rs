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
    while root_drops < 3 {
        let input = smoke_input(&g);
        g.tick(input);
        let attempts = [g.attempt(), g.battles()[1].game().attempt()];
        assert!(
            attempts[0].is_some(),
            "the root attempt ended first:\n{trace}"
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
    assert_eq!(table.lines().count(), c.items.len());
    assert_eq!(
        support::fnv1a(table.as_bytes()),
        V6_STASH_TABLE_FNV,
        "{table}"
    );
}

/// The v6 fixture through the SAVE_VERSION 7 migration: the root report is
/// Mara's (Npc, keyless), the root attempt is the player's, and every worker
/// attempt and report sits in its expedition's recruit seat.
#[test]
fn v6_fixture_defenders_resolve() {
    let game = fixture_v6();
    assert_eq!(game.attempt().unwrap().defender(), DefenderId::Player);
    assert_eq!(game.last_result().unwrap().defender(), DefenderId::Mara);
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
