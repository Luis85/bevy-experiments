//! Pins for the cue and bank tables (A6-01). Both are append-only: a cue's
//! discriminant indexes gate tables, and a bank's ordinal seeds its synthesis,
//! so reordering either changes shipped audio.

use horde_audio::{
    bank::{BankId, LEGACY_FAMILIES, variant_path},
    cue::{CUE_COUNT, Cue},
};

/// The v1 cue order, by discriminant. Later rows append below the last line.
const FROZEN_CUES: [(Cue, &str); 28] = [
    (Cue::Pistol, "Pistol"),
    (Cue::Smg, "Smg"),
    (Cue::Shotgun, "Shotgun"),
    (Cue::Rifle, "Rifle"),
    (Cue::Marksman, "Marksman"),
    (Cue::MachineGun, "MachineGun"),
    (Cue::NpcShot, "NpcShot"),
    (Cue::TurretShot, "TurretShot"),
    (Cue::Impact, "Impact"),
    (Cue::Armor, "Armor"),
    (Cue::Reload, "Reload"),
    (Cue::Throw, "Throw"),
    (Cue::Explosion, "Explosion"),
    (Cue::Molotov, "Molotov"),
    (Cue::Fire, "Fire"),
    (Cue::Retreat, "Retreat"),
    (Cue::Boss, "Boss"),
    (Cue::Wave, "Wave"),
    (Cue::Damage, "Damage"),
    (Cue::Melee, "Melee"),
    (Cue::Ready, "Ready"),
    (Cue::Structure, "Structure"),
    (Cue::Trap, "Trap"),
    (Cue::Incoming, "Incoming"),
    (Cue::Kill, "Kill"),
    (Cue::Victory, "Victory"),
    (Cue::Defeat, "Defeat"),
    (Cue::Confirm, "Confirm"),
];

/// The v1 synthesized families: file stem by bank ordinal.
const FROZEN_FAMILIES: [&str; 24] = [
    "pistol",
    "smg",
    "shotgun",
    "rifle",
    "marksman",
    "machinegun",
    "npc_shot",
    "turret_shot",
    "impact",
    "armor",
    "reload",
    "throw",
    "explosion",
    "molotov",
    "fire",
    "retreat",
    "boss",
    "wave",
    "damage",
    "melee",
    "ready",
    "structure",
    "trap",
    "incoming",
];

#[test]
fn cue_discriminants_are_append_only() {
    assert_eq!(CUE_COUNT, Cue::ALL.len());
    assert!(Cue::ALL.len() >= FROZEN_CUES.len());
    for (index, (cue, name)) in FROZEN_CUES.into_iter().enumerate() {
        assert_eq!(cue as usize, index, "{name} moved");
        assert_eq!(Cue::ALL[index], cue, "Cue::ALL[{index}]");
        assert_eq!(format!("{cue:?}"), name);
    }
    for (index, cue) in Cue::ALL.into_iter().enumerate() {
        assert_eq!(cue as usize, index, "Cue::ALL is not in discriminant order");
    }
}

#[test]
fn legacy_family_order_is_frozen() {
    let names: Vec<&str> = LEGACY_FAMILIES
        .into_iter()
        .map(|family| BankId::Legacy(family).name())
        .collect();
    assert_eq!(names, FROZEN_FAMILIES);
    for family in LEGACY_FAMILIES {
        assert_eq!(family.bank(), BankId::Legacy(family), "{family:?}");
        assert_eq!(family.legacy_family(), family);
    }
}

#[test]
fn bank_ordinals_are_append_only() {
    for (ordinal, family) in LEGACY_FAMILIES.into_iter().enumerate() {
        let bank = BankId::Legacy(family);
        assert_eq!(bank.ordinal(), ordinal as u64, "{family:?}");
        assert_eq!(bank.name(), FROZEN_FAMILIES[ordinal]);
        for variant in 0..3 {
            assert_eq!(
                bank.variant_path(variant),
                format!("audio/survival_{}_{variant}.wav", FROZEN_FAMILIES[ordinal])
            );
        }
    }
    // Every cue resolves to one bank, and no two banks share an ordinal.
    let mut seen = std::collections::BTreeMap::new();
    for cue in Cue::ALL {
        let bank = cue.bank();
        if let Some(previous) = seen.insert(bank.ordinal(), bank) {
            assert_eq!(previous, bank, "ordinal {} is shared", bank.ordinal());
        }
    }
    assert_eq!(seen.len(), LEGACY_FAMILIES.len());
}

#[test]
fn alias_cues_play_their_family_bank() {
    for (alias, family) in [
        (Cue::Kill, Cue::Impact),
        (Cue::Victory, Cue::Wave),
        (Cue::Defeat, Cue::Structure),
        (Cue::Confirm, Cue::Ready),
    ] {
        assert_eq!(alias.bank(), BankId::Legacy(family));
        assert!(!LEGACY_FAMILIES.contains(&alias), "{alias:?}");
        assert!(LEGACY_FAMILIES.contains(&family));
        for variant in 0..3 {
            assert_eq!(variant_path(alias, variant), variant_path(family, variant));
        }
    }
    assert_eq!(
        variant_path(Cue::Explosion, 7),
        "audio/survival_explosion_1.wav"
    );
    let retired: Vec<Cue> = Cue::ALL.into_iter().filter(|cue| cue.retired()).collect();
    assert_eq!(retired, vec![Cue::NpcShot]);
    for cue in Cue::ALL {
        // No live cue plays a retired bank.
        assert!(cue.retired() || !cue.legacy_family().retired(), "{cue:?}");
    }
}
