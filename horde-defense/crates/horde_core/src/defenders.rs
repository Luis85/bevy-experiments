//! Defender identity: who holds the line in a run. Types and pure functions
//! only; nothing here mutates a game.
//!
//! [`DefenderId`] is the only public identity. [`DefenderSeat`] is its saved
//! form on an attempt or report: absent means the mode owner (the player for
//! `Mode::Personal`, Mara for `Mode::Npc`), so saves without the key keep
//! their meaning. [`resolve`] joins the two. Display names come from the live
//! records (`npc.name`, the survivor name), never from this module, except the
//! recruit identity table used when a recruit joins.
use crate::content::Mode;
use crate::survivors::Appearance;
use serde::{Deserialize, Serialize};

/// Who defends. Serialised externally tagged:
/// `"Player"`, `"Mara"` or `{"Recruit": id}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DefenderId {
    Player,
    Mara,
    Recruit(u64),
}
impl DefenderId {
    /// The rule set the defender fights under: the player is Personal, Mara
    /// and every recruit are Npc.
    pub fn mode(self) -> Mode {
        match self {
            Self::Player => Mode::Personal,
            Self::Mara | Self::Recruit(_) => Mode::Npc,
        }
    }
    pub fn recruit(self) -> Option<u64> {
        match self {
            Self::Recruit(id) => Some(id),
            Self::Player | Self::Mara => None,
        }
    }
    /// The mode owner: Personal is the player, Npc is Mara.
    pub fn from_mode(mode: Mode) -> Self {
        resolve(mode, DefenderSeat::ModeOwner)
    }
    /// The seat that, with [`Self::mode`], resolves back to this defender.
    pub fn seat(self) -> DefenderSeat {
        match self {
            Self::Recruit(id) => DefenderSeat::Recruit(id),
            Self::Player | Self::Mara => DefenderSeat::ModeOwner,
        }
    }
}

/// The saved defender on an attempt or report. `ModeOwner` is the default
/// and is skipped when saving; a recruit seat serialises exactly like
/// [`DefenderId::Recruit`] (`{"Recruit": id}`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum DefenderSeat {
    #[default]
    ModeOwner,
    Recruit(u64),
}
impl DefenderSeat {
    /// Serde skip predicate: the mode owner is never written.
    pub fn is_mode_owner(&self) -> bool {
        *self == Self::ModeOwner
    }
}

/// The defender a mode and seat name. Personal is always the player (a
/// Personal attempt with a recruit seat is invalid saved state and is
/// rejected by validation); Npc is Mara unless a recruit holds the seat.
pub fn resolve(mode: Mode, seat: DefenderSeat) -> DefenderId {
    match (mode, seat) {
        (Mode::Personal, _) => DefenderId::Player,
        (Mode::Npc, DefenderSeat::ModeOwner) => DefenderId::Mara,
        (Mode::Npc, DefenderSeat::Recruit(id)) => DefenderId::Recruit(id),
    }
}

/// Recruit names and appearances in join order. Recruit `id` takes entry
/// `(id - 1) % 12`; ids past the table add the id to the name. The order and
/// names are the ones recruits have always received.
pub const RECRUIT_IDENTITIES: [(&str, Appearance); 12] = [
    ("Alex Rowan", Appearance::Male),
    ("Sam Okoro", Appearance::Male),
    ("Noor Aziz", Appearance::Female),
    ("Ren Mori", Appearance::Male),
    ("Lena Fischer", Appearance::Female),
    ("Ari Silva", Appearance::Female),
    ("Mika Chen", Appearance::Female),
    ("Imani Cole", Appearance::Female),
    ("Sasha Volk", Appearance::Male),
    ("Kai Morgan", Appearance::Male),
    ("Dara Nouri", Appearance::Female),
    ("Morgan Reyes", Appearance::Male),
];

/// The highest advancement level a survivor defender can reach (CD5): the
/// campaign allowance stops growing here.
pub const MAX_ADVANCEMENT_LEVEL: u8 = 10;

/// The table entry for recruit `id` (ids start at 1).
pub fn recruit_identity(id: u64) -> (&'static str, Appearance) {
    let len = RECRUIT_IDENTITIES.len() as u64;
    RECRUIT_IDENTITIES[(id.saturating_sub(1) % len) as usize]
}

/// Why a defender cannot be chosen or deployed right now.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DefenderBlocker {
    /// The upgrade node that unlocks this defender is not owned.
    Locked {
        node_id: String,
    },
    Guarding {
        country_id: String,
    },
    OnExpedition {
        battle: u64,
    },
    Defending,
    InvalidLoadout {
        reason: String,
    },
}
impl DefenderBlocker {
    /// Plain player-facing text.
    pub fn label(&self) -> String {
        match self {
            Self::Locked { node_id } => {
                let name = crate::skill_catalog::skills()
                    .node(node_id)
                    .map_or(node_id.as_str(), |n| n.name.as_str());
                format!("Locked until you unlock {name}")
            }
            Self::Guarding { country_id } => {
                let name = crate::world::world()
                    .country(country_id)
                    .map_or(country_id.as_str(), |c| c.name.as_str());
                format!("Guarding {name}")
            }
            Self::OnExpedition { .. } => "Away on an expedition".into(),
            Self::Defending => "Already defending the front".into(),
            Self::InvalidLoadout { reason } => format!("Cannot deploy: {reason}"),
        }
    }
}
impl std::fmt::Display for DefenderBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// What a defender is to the campaign: the player's own survivor, the
/// companion Mara, or a roster recruit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DefenderRole {
    Survivor,
    Companion,
    Recruit,
}

/// Where a recruit comes from: the country it joined in and the flavour
/// trait of that country's biome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginInfo {
    pub country_id: String,
    pub country_name: String,
    pub origin_trait: OriginTrait,
}

/// A defender as the frontend names and draws it (`Game::defender_card`).
/// Derived from the live records on every read; never saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenderCard {
    pub id: DefenderId,
    pub name: String,
    /// The first word of the name, upper case (ROOK, MARA, ALEX).
    pub hud_name: String,
    pub appearance: Appearance,
    pub role: DefenderRole,
    /// Some for a recruit only.
    pub origin: Option<OriginInfo>,
    /// The rule set the defender fights under.
    pub mode: Mode,
    /// The player's level, or a survivor defender's advancement level.
    pub level: u32,
    /// The highest `level` the campaign allows right now.
    pub level_cap: u32,
}

/// One row of `Game::available_defenders`: the card, whether
/// `SetNextDefender` would accept it now (or the typed reason it would not),
/// and whether it is the next defender.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenderOption {
    pub card: DefenderCard,
    pub selectable: Result<(), DefenderBlocker>,
    pub selected: bool,
}

/// The HUD form of a defender name: its first word in upper case.
pub fn hud_name(name: &str) -> String {
    name.split_whitespace()
        .next()
        .unwrap_or(name)
        .to_uppercase()
}

/// Flavour from a recruit's home biome. Text only, with no stat effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OriginTrait {
    MonsoonHardened,
    HedgerowForager,
    DustWalker,
    FrostSteady,
    HighCountryClimber,
}
impl OriginTrait {
    pub const ALL: [Self; 5] = [
        Self::MonsoonHardened,
        Self::HedgerowForager,
        Self::DustWalker,
        Self::FrostSteady,
        Self::HighCountryClimber,
    ];
    /// The trait for a world biome, or None for a biome this table does not
    /// know.
    pub fn from_biome(biome: &str) -> Option<Self> {
        match biome {
            "Tropical" => Some(Self::MonsoonHardened),
            "Temperate" => Some(Self::HedgerowForager),
            "Arid" => Some(Self::DustWalker),
            "Boreal" => Some(Self::FrostSteady),
            "Alpine" => Some(Self::HighCountryClimber),
            _ => None,
        }
    }
    /// The trait for a world biome; an unknown biome reads as Temperate.
    pub fn for_biome(biome: &str) -> Self {
        Self::from_biome(biome).unwrap_or(Self::HedgerowForager)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::MonsoonHardened => "Monsoon-hardened",
            Self::HedgerowForager => "Hedgerow forager",
            Self::DustWalker => "Dust-walker",
            Self::FrostSteady => "Frost-steady",
            Self::HighCountryClimber => "High-country climber",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Self::MonsoonHardened => "Kept moving through flood seasons and endless rain.",
            Self::HedgerowForager => "Lived off fields and hedgerows between the towns.",
            Self::DustWalker => "Crossed open dry country with little water.",
            Self::FrostSteady => "Held out through long dark winters in the north.",
            Self::HighCountryClimber => "Knows the mountain passes and thin air.",
        }
    }
}
