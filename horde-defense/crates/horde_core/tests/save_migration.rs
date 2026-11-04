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
mod support;

use horde_core::campaign::{BodyAttribute, NpcTraining};
use horde_core::content::{EnemyKind, ItemDefId, StructureKind};
use horde_core::ordnance::OrdnanceKind;
use horde_core::quality::Quality;
use horde_core::sim::{Game, Input, NpcManeuver, SAVE_VERSION};
use horde_core::skill_catalog::unlocks;
use horde_core::status::StatusId;

const FIXTURE: &str = include_str!("fixtures/save_v1.json");
const FIXTURE_V2: &str = include_str!("fixtures/save_v2.json");
const FIXTURE_V3: &str = include_str!("fixtures/save_v3.json");
const FIXTURE_V4: &str = include_str!("fixtures/save_v4.json");
const FIXTURE_V5: &str = include_str!("fixtures/save_v5.json");

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
        .campaign
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
        game.campaign.item(6).unwrap().definition().name,
        "Compact SMG"
    );
    let gear: Vec<_> = game.campaign.run_history[0]
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
    let visual = &game.attempt.as_ref().unwrap().profile.equipment_visual;
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
    let c = &game.campaign;
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
    let a = game.attempt.as_ref().unwrap();
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
            let a = g.attempt.as_ref().unwrap();
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
    let a = game.attempt.as_ref().unwrap();
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
    let a = game.attempt.as_ref().unwrap();
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
        let aim = g.attempt.as_ref().and_then(|a| {
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
            let a = g.attempt.as_ref().unwrap();
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
        counts(&game.attempt.as_ref().unwrap().loot_quality_counts),
        [0, 2, 1, 0, 1, 1]
    );
    assert_eq!(
        counts(&game.campaign.run_history[0].loot_by_quality),
        [1, 1, 0, 2, 0, 0]
    );
    assert_eq!(game.attempt.as_ref().unwrap().loot, 6);
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
            let a = g.attempt.as_ref().unwrap();
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
    let o = &game.attempt.as_ref().unwrap().ordnance;
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
            let a = g.attempt.as_ref().unwrap();
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
        game.attempt
            .as_ref()
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
            let a = g.attempt.as_ref().unwrap();
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
