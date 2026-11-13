//! The cue list. Append-only: a cue's discriminant indexes the gate tables,
//! and the synthesized families' discriminants seed their synthesis.

use crate::bank::BankId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Cue {
    Pistol,
    Smg,
    Shotgun,
    Rifle,
    Marksman,
    MachineGun,
    /// Retired (see [`Cue::retired`]); the slot keeps later discriminants.
    NpcShot,
    TurretShot,
    Impact,
    Armor,
    Reload,
    Throw,
    Explosion,
    Molotov,
    Fire,
    Retreat,
    Boss,
    Wave,
    Damage,
    Melee,
    Ready,
    Structure,
    Trap,
    Incoming,
    // Presentation cues that reuse a synthesized family's bank (see `bank`).
    // They come last so the synthesized families keep their seeds.
    /// An enemy died.
    Kill,
    /// Attempt-finished stingers. A withdrawal plays `Retreat`.
    Victory,
    Defeat,
    /// A purchase, training or equip command succeeded.
    Confirm,
}

/// Number of cues; sizes every per-cue table.
pub const CUE_COUNT: usize = Cue::ALL.len();

impl Cue {
    /// Every cue, in discriminant order.
    pub const ALL: [Self; 28] = [
        Self::Pistol,
        Self::Smg,
        Self::Shotgun,
        Self::Rifle,
        Self::Marksman,
        Self::MachineGun,
        Self::NpcShot,
        Self::TurretShot,
        Self::Impact,
        Self::Armor,
        Self::Reload,
        Self::Throw,
        Self::Explosion,
        Self::Molotov,
        Self::Fire,
        Self::Retreat,
        Self::Boss,
        Self::Wave,
        Self::Damage,
        Self::Melee,
        Self::Ready,
        Self::Structure,
        Self::Trap,
        Self::Incoming,
        Self::Kill,
        Self::Victory,
        Self::Defeat,
        Self::Confirm,
    ];

    /// The synthesized family whose WAV variants this cue plays. Alias cues
    /// differ from their family only in priority, gain and repeat gating.
    pub const fn legacy_family(self) -> Self {
        match self {
            Self::Kill => Self::Impact,
            Self::Victory => Self::Wave,
            Self::Defeat => Self::Structure,
            Self::Confirm => Self::Ready,
            cue => cue,
        }
    }

    /// The bank this cue plays.
    pub const fn bank(self) -> BankId {
        BankId::Legacy(self.legacy_family())
    }

    /// A retired cue keeps its slot (and its synthesis, the provenance of its
    /// archived files) but is never emitted, loaded or shipped. NpcShot retired
    /// on 2026-11-04 (user): NPC defenders sound like their weapon class.
    pub const fn retired(self) -> bool {
        matches!(self, Self::NpcShot)
    }
}
