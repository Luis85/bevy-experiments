//! Banks: the sets of WAV variants a cue plays, and their file paths.
//!
//! A bank's ordinal seeds its synthesis, so ordinals are append-only: the 24
//! v1 families hold 0..=23 in [`LEGACY_FAMILIES`] order, and later families
//! take the next free numbers.

use crate::cue::Cue;

/// The synthesized v1 families in bank-ordinal order. Alias cues (Kill,
/// Victory, Defeat, Confirm) are not families: they play a family's bank.
/// Retired families stay listed so their archived files stay reproducible.
pub const LEGACY_FAMILIES: [Cue; 24] = [
    Cue::Pistol,
    Cue::Smg,
    Cue::Shotgun,
    Cue::Rifle,
    Cue::Marksman,
    Cue::MachineGun,
    Cue::NpcShot,
    Cue::TurretShot,
    Cue::Impact,
    Cue::Armor,
    Cue::Reload,
    Cue::Throw,
    Cue::Explosion,
    Cue::Molotov,
    Cue::Fire,
    Cue::Retreat,
    Cue::Boss,
    Cue::Wave,
    Cue::Damage,
    Cue::Melee,
    Cue::Ready,
    Cue::Structure,
    Cue::Trap,
    Cue::Incoming,
];

/// Variants the runtime rotates through per bank.
const RUNTIME_VARIANTS: u64 = 3;

/// One bank of WAV variants. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankId {
    /// A v1 family, named by its family cue. Build it with [`Cue::bank`]; an
    /// alias cue here resolves to its family.
    Legacy(Cue),
}

impl BankId {
    /// The append-only number that seeds this bank's synthesis.
    pub const fn ordinal(self) -> u64 {
        match self {
            // A family's ordinal is its cue discriminant: the families are the
            // first 24 cues (`bank_ordinals_are_append_only`).
            Self::Legacy(cue) => cue.legacy_family() as u64,
        }
    }

    /// The file stem between `survival_` and the variant number.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Legacy(cue) => legacy_name(cue.legacy_family()),
        }
    }

    /// The variant's path relative to `assets/`. Variants wrap.
    pub fn variant_path(self, variant: u64) -> String {
        format!(
            "audio/survival_{}_{}.wav",
            self.name(),
            variant % RUNTIME_VARIANTS
        )
    }
}

const fn legacy_name(family: Cue) -> &'static str {
    match family {
        Cue::Kill | Cue::Victory | Cue::Defeat | Cue::Confirm => {
            legacy_name(family.legacy_family())
        }
        Cue::Pistol => "pistol",
        Cue::Smg => "smg",
        Cue::Shotgun => "shotgun",
        Cue::Rifle => "rifle",
        Cue::Marksman => "marksman",
        Cue::MachineGun => "machinegun",
        Cue::NpcShot => "npc_shot",
        Cue::TurretShot => "turret_shot",
        Cue::Impact => "impact",
        Cue::Armor => "armor",
        Cue::Reload => "reload",
        Cue::Throw => "throw",
        Cue::Explosion => "explosion",
        Cue::Molotov => "molotov",
        Cue::Fire => "fire",
        Cue::Retreat => "retreat",
        Cue::Boss => "boss",
        Cue::Wave => "wave",
        Cue::Damage => "damage",
        Cue::Melee => "melee",
        Cue::Ready => "ready",
        Cue::Structure => "structure",
        Cue::Trap => "trap",
        Cue::Incoming => "incoming",
    }
}

/// The path of the variant `cue` plays, relative to `assets/`.
pub fn variant_path(cue: Cue, variant: u64) -> String {
    cue.bank().variant_path(variant)
}

/// The ambience loops relative to `assets/`, in the runtime's channel order:
/// base, wind, urban, nature, rain, cold.
pub const AMBIENCE_FILES: [&str; 6] = [
    "audio/ambience.wav",
    "audio/ambient_wind.wav",
    "audio/ambient_urban.wav",
    "audio/ambient_nature.wav",
    "audio/ambient_rain.wav",
    "audio/ambient_cold.wav",
];
