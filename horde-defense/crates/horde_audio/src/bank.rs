//! Banks: the sets of WAV variants a cue plays, and their file paths.
//!
//! A bank's ordinal seeds its synthesis, so ordinals are append-only: the 24
//! v1 families hold 0..=23 in [`LEGACY_FAMILIES`] order, the wave-6 gun
//! stages 24..=28, the enemy voices 29..=52, the boss kinds 53..=61 and the
//! UI families 62..=63. Later families take the next free numbers.

use crate::cue::Cue;
use horde_core::content::BossKind;

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

/// A stage of handling a gun that is not the shot itself. The reload's
/// finishing stage is the v1 `Reload` family. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GunStage {
    DryFire,
    ReloadOut,
    ReloadIn,
    /// One shotgun shell pushed in.
    ShellInsert,
    /// The marksman rifle's bolt.
    BoltCycle,
}
impl GunStage {
    pub const ALL: [Self; 5] = [
        Self::DryFire,
        Self::ReloadOut,
        Self::ReloadIn,
        Self::ShellInsert,
        Self::BoltCycle,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::DryFire => "dryfire",
            Self::ReloadOut => "reload_out",
            Self::ReloadIn => "reload_in",
            Self::ShellInsert => "shell_insert",
            Self::BoltCycle => "bolt_cycle",
        }
    }
}

/// The voice an enemy archetype sounds with. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyVoice {
    Shambler,
    Runner,
    Brute,
    Armored,
    Demolisher,
    Swarm,
    Spitter,
    Exploder,
}
impl EnemyVoice {
    pub const ALL: [Self; 8] = [
        Self::Shambler,
        Self::Runner,
        Self::Brute,
        Self::Armored,
        Self::Demolisher,
        Self::Swarm,
        Self::Spitter,
        Self::Exploder,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shambler => "shambler",
            Self::Runner => "runner",
            Self::Brute => "brute",
            Self::Armored => "armored",
            Self::Demolisher => "demolisher",
            Self::Swarm => "swarm",
            Self::Spitter => "spitter",
            Self::Exploder => "exploder",
        }
    }
}

/// What an enemy is doing when it sounds. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyStage {
    Windup,
    Contact,
    Death,
}
impl EnemyStage {
    pub const ALL: [Self; 3] = [Self::Windup, Self::Contact, Self::Death];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windup => "windup",
            Self::Contact => "contact",
            Self::Death => "death",
        }
    }
}

/// A boss moment. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossStage {
    Arrival,
    Phase,
    Death,
}
impl BossStage {
    pub const ALL: [Self; 3] = [Self::Arrival, Self::Phase, Self::Death];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Arrival => "arrival",
            Self::Phase => "phase",
            Self::Death => "death",
        }
    }
}

/// The boss kinds in bank order. Append-only, like `BossKind` itself.
pub const BOSS_KINDS: [BossKind; 3] = [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute];

const fn boss_index(kind: BossKind) -> u64 {
    match kind {
        BossKind::Breacher => 0,
        BossKind::Caller => 1,
        BossKind::SiegeBrute => 2,
    }
}

/// The file-name part of a boss kind.
pub const fn boss_name(kind: BossKind) -> &'static str {
    match kind {
        BossKind::Breacher => "breacher",
        BossKind::Caller => "caller",
        BossKind::SiegeBrute => "siege_brute",
    }
}

/// An interface sound family. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiFamily {
    LootReveal,
    CoinBatch,
}
impl UiFamily {
    pub const ALL: [Self; 2] = [Self::LootReveal, Self::CoinBatch];
    pub const fn name(self) -> &'static str {
        match self {
            Self::LootReveal => "loot_reveal",
            Self::CoinBatch => "coin_batch",
        }
    }
}

/// One bank of WAV variants. Append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankId {
    /// A v1 family, named by its family cue. Build it with [`Cue::bank`]; an
    /// alias cue here resolves to its family.
    Legacy(Cue),
    Gun(GunStage),
    Enemy(EnemyVoice, EnemyStage),
    Boss(BossKind, BossStage),
    Ui(UiFamily),
}

/// A seed takes `1 + ordinal * 17 + variant`, so a bank has at most this many
/// variants before two banks would share a seed.
pub const MAX_VARIANTS: u64 = 17;

/// The synthesis seed of one variant of one bank. For the v1 families'
/// variants 0-2 this is the v1 seed.
pub const fn seed(bank: BankId, variant: u64) -> u64 {
    assert!(variant < MAX_VARIANTS);
    0x85eb_ca6b_u64.wrapping_mul(1 + bank.ordinal() * 17 + variant)
}

impl BankId {
    /// The append-only number that seeds this bank's synthesis.
    pub const fn ordinal(self) -> u64 {
        match self {
            // A family's ordinal is its cue discriminant: the families are the
            // first 24 cues (`bank_ordinals_are_append_only`).
            Self::Legacy(cue) => cue.legacy_family() as u64,
            Self::Gun(stage) => 24 + stage as u64,
            Self::Enemy(voice, stage) => 29 + voice as u64 * 3 + stage as u64,
            Self::Boss(kind, stage) => 53 + boss_index(kind) * 3 + stage as u64,
            Self::Ui(family) => 62 + family as u64,
        }
    }

    /// How many WAV variants the bank ships. A v1 family whose cue (or an
    /// alias of it) can restart within 250 ms ships five so that a repeat is
    /// not heard as one (`frequent_cues_have_at_least_five_variants`).
    pub const fn variants(self) -> u64 {
        match self {
            Self::Legacy(cue) => match cue.legacy_family() {
                Cue::Fire
                | Cue::Retreat
                | Cue::Boss
                | Cue::Wave
                | Cue::Structure
                | Cue::Incoming
                | Cue::NpcShot => 3,
                _ => 5,
            },
            Self::Gun(GunStage::DryFire | GunStage::ShellInsert) => 5,
            Self::Gun(_) => 3,
            Self::Enemy(..) => 5,
            Self::Boss(..) => 3,
            Self::Ui(_) => 3,
        }
    }

    /// The file stem between `survival_` and the variant number.
    pub fn name(self) -> String {
        match self {
            Self::Legacy(cue) => legacy_name(cue.legacy_family()).to_owned(),
            Self::Gun(stage) => format!("gun_{}", stage.name()),
            Self::Enemy(voice, stage) => format!("enemy_{}_{}", voice.name(), stage.name()),
            Self::Boss(kind, stage) => format!("boss_{}_{}", boss_name(kind), stage.name()),
            Self::Ui(family) => format!("ui_{}", family.name()),
        }
    }

    /// The variant's path relative to `assets/`. Variants wrap.
    pub fn variant_path(self, variant: u64) -> String {
        format!(
            "audio/survival_{}_{}.wav",
            self.name(),
            variant % self.variants()
        )
    }

    /// A retired bank is archived: it is never exported or loaded.
    pub const fn retired(self) -> bool {
        match self {
            Self::Legacy(cue) => cue.legacy_family().retired(),
            _ => false,
        }
    }

    /// Every bank that ships, in ordinal order.
    pub fn shipped() -> Vec<Self> {
        let mut banks: Vec<Self> = LEGACY_FAMILIES.into_iter().map(Self::Legacy).collect();
        banks.extend(GunStage::ALL.into_iter().map(Self::Gun));
        for voice in EnemyVoice::ALL {
            banks.extend(EnemyStage::ALL.map(|stage| Self::Enemy(voice, stage)));
        }
        for kind in BOSS_KINDS {
            banks.extend(BossStage::ALL.map(|stage| Self::Boss(kind, stage)));
        }
        banks.extend(UiFamily::ALL.into_iter().map(Self::Ui));
        banks.retain(|bank| !bank.retired());
        banks
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
