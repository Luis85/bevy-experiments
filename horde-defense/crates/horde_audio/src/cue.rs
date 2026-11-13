//! The cue list. Append-only: a cue's discriminant indexes the gate tables,
//! and the synthesized families' discriminants seed their synthesis.

use crate::bank::{BankId, BossStage, EnemyStage, EnemyVoice};
use horde_core::content::BossKind;

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
    // Wave 6 (A6-12): the enemy and boss voices. Each plays the bank its
    // request's `Voicing` names (the archetype's voice, the boss's kind);
    // `Cue::bank` is only the default.
    /// An enemy winds up against the defender. A warning.
    EnemyWindup,
    /// An enemy's attack lands, on the defender or on a structure.
    EnemyContact,
    /// An enemy died, in its own voice. `Kill` is the fallback.
    EnemyDeath,
    /// A boss of a known kind arrived. `Boss` is the fallback.
    BossArrival,
    /// A boss entered a phase. A warning.
    BossPhase,
    BossDeath,
    /// A boss's weak point opened (the Ready bank, lower and louder).
    BossExposed,
    /// The defender defended a strike (the Armor bank, lower and quieter).
    Block,
    /// A heal or a brace (the Ready bank, lower).
    Ability,
}

/// Number of cues; sizes every per-cue table.
pub const CUE_COUNT: usize = Cue::ALL.len();

impl Cue {
    /// Every cue, in discriminant order.
    pub const ALL: [Self; 37] = [
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
        Self::EnemyWindup,
        Self::EnemyContact,
        Self::EnemyDeath,
        Self::BossArrival,
        Self::BossPhase,
        Self::BossDeath,
        Self::BossExposed,
        Self::Block,
        Self::Ability,
    ];

    /// The synthesized family whose WAV variants this cue plays. Alias cues
    /// differ from their family only in priority, gain and repeat gating.
    /// For an enemy or boss voice cue it is the v1 family of its fallback
    /// cue (`Kill` for a death, `Boss` for a boss moment); those cues play
    /// their own banks ([`Cue::bank`]).
    pub const fn legacy_family(self) -> Self {
        match self {
            Self::Kill | Self::EnemyDeath | Self::BossDeath => Self::Impact,
            Self::Victory => Self::Wave,
            Self::Defeat => Self::Structure,
            Self::Confirm | Self::BossExposed | Self::Ability => Self::Ready,
            Self::Block => Self::Armor,
            Self::EnemyWindup => Self::Incoming,
            Self::EnemyContact => Self::Melee,
            Self::BossArrival | Self::BossPhase => Self::Boss,
            cue => cue,
        }
    }

    /// The bank this cue plays when its request names no other: an enemy
    /// voice cue defaults to the shambler's bank, a boss cue to the
    /// breacher's (`events` names the real one per request).
    pub const fn bank(self) -> BankId {
        match self {
            Self::EnemyWindup => BankId::Enemy(EnemyVoice::Shambler, EnemyStage::Windup),
            Self::EnemyContact => BankId::Enemy(EnemyVoice::Shambler, EnemyStage::Contact),
            Self::EnemyDeath => BankId::Enemy(EnemyVoice::Shambler, EnemyStage::Death),
            Self::BossArrival => BankId::Boss(BossKind::Breacher, BossStage::Arrival),
            Self::BossPhase => BankId::Boss(BossKind::Breacher, BossStage::Phase),
            Self::BossDeath => BankId::Boss(BossKind::Breacher, BossStage::Death),
            cue => BankId::Legacy(cue.legacy_family()),
        }
    }

    /// The cue whose repeat gate this cue shares: every enemy death shares
    /// the kill's 160 ms, so a horde's deaths are one rate-limited stream.
    pub const fn gate_key(self) -> Self {
        match self {
            Self::EnemyDeath => Self::Kill,
            cue => cue,
        }
    }

    /// A retired cue keeps its slot (and its synthesis, the provenance of its
    /// archived files) but is never emitted, loaded or shipped. NpcShot retired
    /// on 2026-11-04 (user): NPC defenders sound like their weapon class.
    pub const fn retired(self) -> bool {
        matches!(self, Self::NpcShot)
    }
}
