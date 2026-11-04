//! Legacy save compatibility against a real checked-in generation.
//!
//! `fixtures/save_v1.json` was written by `Game::save` at save format v1
//! (commit cb9f34d): a legacy campaign with three clears, all blueprints,
//! Wall/Turret upgrades, legacy body points, NPC training, a bought SMG, looted
//! Rare/Epic gear, one finished personal run and a live NPC attempt at tick 2400.
use horde_core::campaign::{BodyAttribute, NpcTraining};
use horde_core::content::{EnemyKind, ItemDefId, StructureKind};
use horde_core::quality::Quality;
use horde_core::sim::{Game, Input, NpcManeuver, SAVE_VERSION};

const FIXTURE: &str = include_str!("fixtures/save_v1.json");

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

#[test]
fn migrated_v1_fixture_resaves_as_v2_and_resumes_byte_identically() {
    let mut direct = fixture();
    let path =
        std::env::temp_dir().join(format!("lastlight-v1-resave-{}.json", std::process::id()));
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
