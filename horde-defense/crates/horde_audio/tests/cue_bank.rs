//! Pins for the cue and bank tables (A6-01). Both are append-only: a cue's
//! discriminant indexes gate tables, and a bank's ordinal seeds its synthesis,
//! so reordering either changes shipped audio.

use horde_audio::{
    bank::{BankId, LEGACY_FAMILIES, variant_path},
    cue::{CUE_COUNT, Cue},
};

/// The v1 cue order, by discriminant. Later rows append below the last line.
const FROZEN_CUES: [(Cue, &str); 37] = [
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
    // A6-12.
    (Cue::EnemyWindup, "EnemyWindup"),
    (Cue::EnemyContact, "EnemyContact"),
    (Cue::EnemyDeath, "EnemyDeath"),
    (Cue::BossArrival, "BossArrival"),
    (Cue::BossPhase, "BossPhase"),
    (Cue::BossDeath, "BossDeath"),
    (Cue::BossExposed, "BossExposed"),
    (Cue::Block, "Block"),
    (Cue::Ability, "Ability"),
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
    let names: Vec<String> = LEGACY_FAMILIES
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
    // The 24 families, and the default banks of the six enemy and boss
    // voice cues (their requests name the voice's or the kind's bank).
    assert_eq!(seen.len(), LEGACY_FAMILIES.len() + 6);
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
    // Variants wrap at the bank's own count: five for a frequent family,
    // three for a rare one.
    assert_eq!(
        variant_path(Cue::Explosion, 7),
        "audio/survival_explosion_2.wav"
    );
    assert_eq!(variant_path(Cue::Boss, 7), "audio/survival_boss_1.wav");
    let retired: Vec<Cue> = Cue::ALL.into_iter().filter(|cue| cue.retired()).collect();
    assert_eq!(retired, vec![Cue::NpcShot]);
    for cue in Cue::ALL {
        // No live cue plays a retired bank.
        assert!(cue.retired() || !cue.legacy_family().retired(), "{cue:?}");
    }
}

/// The wave-6 banks take the ordinals after the v1 families. An ordinal seeds
/// a bank's synthesis, so a moved one would change every file of the bank.
#[test]
fn wave6_bank_ordinals_and_names_are_frozen() {
    use horde_audio::bank::{BossStage, EnemyStage, EnemyVoice, GunStage, UiFamily, seed};
    use horde_core::content::BossKind;
    let pinned = [
        (BankId::Gun(GunStage::DryFire), 24, "gun_dryfire"),
        (BankId::Gun(GunStage::BoltCycle), 28, "gun_bolt_cycle"),
        (
            BankId::Enemy(EnemyVoice::Shambler, EnemyStage::Windup),
            29,
            "enemy_shambler_windup",
        ),
        (
            BankId::Enemy(EnemyVoice::Exploder, EnemyStage::Death),
            52,
            "enemy_exploder_death",
        ),
        (
            BankId::Boss(BossKind::Breacher, BossStage::Arrival),
            53,
            "boss_breacher_arrival",
        ),
        (
            BankId::Boss(BossKind::SiegeBrute, BossStage::Death),
            61,
            "boss_siege_brute_death",
        ),
        (BankId::Ui(UiFamily::LootReveal), 62, "ui_loot_reveal"),
        (BankId::Ui(UiFamily::CoinBatch), 63, "ui_coin_batch"),
    ];
    for (bank, ordinal, name) in pinned {
        assert_eq!(bank.ordinal(), ordinal, "{bank:?}");
        assert_eq!(bank.name(), name);
    }
    // The shipped banks are ordinals 0..=63 without the retired NpcShot (6),
    // each with its own name, and every variant has its own seed.
    let shipped = BankId::shipped();
    let ordinals: Vec<u64> = shipped.iter().map(|bank| bank.ordinal()).collect();
    let expected: Vec<u64> = (0..=63).filter(|ordinal| *ordinal != 6).collect();
    assert_eq!(ordinals, expected);
    let names: std::collections::BTreeSet<String> =
        shipped.iter().map(|bank| bank.name()).collect();
    assert_eq!(names.len(), shipped.len());
    let files: usize = shipped.iter().map(|bank| bank.variants() as usize).sum();
    assert_eq!(files, 275);
    let seeds: std::collections::BTreeSet<u64> = shipped
        .iter()
        .flat_map(|bank| (0..bank.variants()).map(|variant| seed(*bank, variant)))
        .collect();
    assert_eq!(seeds.len(), files);
    // The v1 seed of the v1 variants.
    assert_eq!(
        seed(BankId::Legacy(Cue::Smg), 2),
        0x85eb_ca6b_u64 * (1 + 17 + 2)
    );
}
