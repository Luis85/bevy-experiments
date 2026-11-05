//! The recording vocabulary is frozen: every command and input a FORMAT 1
//! recording may hold must keep parsing and reserializing to the same bytes.
//!
//! `fixtures/recording_vocabulary_v1.json` holds one sample per v1 `Command`
//! variant (41) and two `Input` samples, each as the exact text a recording
//! line embeds, written on the v6 build (parent commit 3d492cd) by the
//! `#[ignore]`d writer below:
//! `LASTLIGHT_WRITE_RECORDING_VOCABULARY=<path> cargo test --locked -p
//! horde_core --test recording_vocabulary -- --ignored`. New variants are
//! appended to `Command`; these 41 never change.
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::campaign::NpcTraining;
use horde_core::command::{Command, NpcTactics};
use horde_core::content::{Mode, Policy, StructureKind};
use horde_core::ordnance::{OrdnanceKind, ThrowRequest};
use horde_core::sim::Input;
use horde_core::survivors::SurvivorId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const VOCABULARY: &str = include_str!("fixtures/recording_vocabulary_v1.json");

/// The 41 `Command` variants of recording FORMAT 1, in declaration order.
const V1_COMMANDS: [&str; 41] = [
    "ContinueJourney",
    "LeaveJourney",
    "SetBalance",
    "ChooseSurvivor",
    "ChooseStart",
    "EnterCountry",
    "Deploy",
    "Withdraw",
    "SetPaused",
    "Place",
    "Salvage",
    "SaveStartingLayout",
    "SaveExpeditionLayout",
    "ClearExpeditionLayout",
    "ClearLayout",
    "RespecPlayer",
    "BuySkillNode",
    "BuyItem",
    "Equip",
    "Lock",
    "Favorite",
    "SellItem",
    "SellUnprotected",
    "SavePreset",
    "ApplyPreset",
    "BuyNpcPoint",
    "TrainNpc",
    "RespecNpc",
    "SetNpcPolicy",
    "Recruit",
    "EquipRecruit",
    "TrainRecruit",
    "BuyRecruitPoint",
    "SetRecruitPolicy",
    "AssignGuard",
    "ReleaseGuard",
    "DeploySurvivor",
    "CountryOperation",
    "RecallSurvivor",
    "PlaceExpedition",
    "SalvageExpedition",
];

#[derive(Serialize, Deserialize)]
struct Vocabulary {
    format: u32,
    commands: Vec<String>,
    inputs: Vec<String>,
}

/// The externally tagged variant name of a serialized command.
fn variant(json: &serde_json::Value) -> String {
    match json {
        serde_json::Value::String(name) => name.clone(),
        serde_json::Value::Object(map) if map.len() == 1 => map.keys().next().unwrap().clone(),
        other => panic!("not an externally tagged command: {other}"),
    }
}

#[test]
fn every_v1_recorded_command_still_parses_and_reserializes_identically() {
    let vocabulary: Vocabulary = serde_json::from_str(VOCABULARY).unwrap();
    assert_eq!(vocabulary.format, 1);
    let mut names = Vec::new();
    for text in &vocabulary.commands {
        let command: Command = serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(&serde_json::to_string(&command).unwrap(), text);
        names.push(variant(&serde_json::from_str(text).unwrap()));
    }
    assert_eq!(names, V1_COMMANDS, "one sample per v1 variant, in order");
    assert_eq!(vocabulary.inputs.len(), 2);
    for text in &vocabulary.inputs {
        let input: Input = serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(&serde_json::to_string(&input).unwrap(), text);
    }
    let unique: BTreeSet<_> = V1_COMMANDS.iter().collect();
    assert_eq!(unique.len(), 41);
}

/// One sample per v1 variant, every payload away from its default.
fn samples() -> Vec<Command> {
    let mut balance = BalanceSettings::for_preset(BalancePreset::Hard);
    balance.preset = BalancePreset::Custom;
    balance.enemy_health = 1.35;
    vec![
        Command::ContinueJourney,
        Command::LeaveJourney,
        Command::SetBalance(balance),
        Command::ChooseSurvivor(SurvivorId::June),
        Command::ChooseStart("DEU".into()),
        Command::EnterCountry("POL".into()),
        Command::Deploy {
            stage: 2,
            mode: Mode::Npc,
        },
        Command::Withdraw,
        Command::SetPaused(true),
        Command::Place {
            kind: StructureKind::Turret,
            lane: 4,
            column: 1,
        },
        Command::Salvage(17),
        Command::SaveStartingLayout,
        Command::SaveExpeditionLayout(9),
        Command::ClearExpeditionLayout(9),
        Command::ClearLayout,
        Command::RespecPlayer,
        Command::BuySkillNode("companion_contact".into()),
        Command::BuyItem("checkpoint_rifle".into()),
        Command::Equip {
            mode: Mode::Personal,
            id: 42,
        },
        Command::Lock {
            id: 42,
            locked: true,
        },
        Command::Favorite {
            id: 43,
            favorite: true,
        },
        Command::SellItem(44),
        Command::SellUnprotected,
        Command::SavePreset {
            name: "Night line".into(),
            mode: Mode::Npc,
        },
        Command::ApplyPreset {
            index: 1,
            mode: Mode::Personal,
        },
        Command::BuyNpcPoint,
        Command::TrainNpc(NpcTraining::Guns),
        Command::RespecNpc,
        Command::SetNpcPolicy(NpcTactics {
            policy: Policy::Guardian,
            prefer_aim: false,
            early_reload: true,
            allow_risk: true,
            recall_percent: 40,
        }),
        Command::Recruit("MDA".into()),
        Command::EquipRecruit { id: 3, item: 45 },
        Command::TrainRecruit {
            id: 3,
            training: NpcTraining::Dexterity,
        },
        Command::BuyRecruitPoint(3),
        Command::SetRecruitPolicy {
            id: 3,
            policy: Policy::BossFocus,
        },
        Command::AssignGuard {
            id: 1,
            country: "MDA".into(),
        },
        Command::ReleaseGuard(1),
        Command::DeploySurvivor {
            id: 2,
            country: "HUN".into(),
            stage: 1,
        },
        Command::CountryOperation {
            id: 2,
            country: "SVK".into(),
        },
        Command::RecallSurvivor(13),
        Command::PlaceExpedition {
            battle: 13,
            kind: StructureKind::Wall,
            lane: 2,
            column: 0,
        },
        Command::SalvageExpedition {
            battle: 13,
            structure: 6,
        },
    ]
}

#[test]
#[ignore = "writes fixtures/recording_vocabulary_v1.json; set LASTLIGHT_WRITE_RECORDING_VOCABULARY"]
fn write_recording_vocabulary() {
    let Some(path) = std::env::var_os("LASTLIGHT_WRITE_RECORDING_VOCABULARY") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    assert!(!path.exists(), "refusing to overwrite {}", path.display());
    let inputs = [
        Input::default(),
        Input {
            aim: [12.5, 3.5],
            fire: true,
            melee: true,
            reload: true,
            heal: true,
            brace: true,
            auto_reload: false,
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: [3, 2],
            }),
        },
    ];
    let vocabulary = Vocabulary {
        format: 1,
        commands: samples()
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect(),
        inputs: inputs
            .iter()
            .map(|i| serde_json::to_string(i).unwrap())
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&vocabulary).unwrap();
    text.push('\n');
    std::fs::write(&path, text).unwrap();
}
