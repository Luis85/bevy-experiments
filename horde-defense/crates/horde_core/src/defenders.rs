//! Defender identity: who holds the line in a run. Types and pure functions
//! only; nothing here mutates a game.
//!
//! [`DefenderId`] is the only public identity. [`DefenderSeat`] is its saved
//! form on an attempt or report: absent means the mode owner (the player for
//! `Mode::Personal`, Mara for `Mode::Npc`), so saves without the key keep
//! their meaning. [`resolve`] joins the two. Display names come from the live
//! records (`npc.name`, the survivor name), never from this module, except the
//! recruit identity table used when a recruit joins.
use crate::campaign::NpcCreation;
use crate::content::{Mode, Policy};
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

/// Threat ranks each advancement level above the first adds to an
/// expedition (W4-12, ruling R57).
pub const EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL: u32 = 1;
/// Threat ranks each item level above 1 adds to an expedition, averaged over
/// the four loadout slots (W4-12, ruling R57).
pub const EXPEDITION_RANKS_PER_ITEM_LEVEL: u32 = 1;

/// The threat rank a recruit's expedition plays at (W4-12, ruling R57): it
/// follows the recruit, its training and level (`advancement_level`, 1..=10)
/// and its gear (the item levels of its four loadout slots), never the
/// campaign rank. A starter recruit (level 1, four level-1 items) plays at
/// rank 0. Pure: no RNG and no campaign state.
pub fn expedition_threat_rank(advancement_level: u8, item_levels: [u32; 4]) -> u32 {
    let training = u32::from(advancement_level.max(1) - 1)
        .saturating_mul(EXPEDITION_RANKS_PER_ADVANCEMENT_LEVEL);
    let gear_levels = item_levels.iter().fold(0u32, |sum, level| {
        sum.saturating_add(level.saturating_sub(1))
    });
    let gear = gear_levels.saturating_mul(EXPEDITION_RANKS_PER_ITEM_LEVEL) / 4;
    training.saturating_add(gear)
}

/// The table entry for recruit `id` (ids start at 1).
pub fn recruit_identity(id: u64) -> (&'static str, Appearance) {
    let len = RECRUIT_IDENTITIES.len() as u64;
    RECRUIT_IDENTITIES[(id.saturating_sub(1) % len) as usize]
}

/// The name recruit `id` joins with: the table name, with the id added once
/// the table has been used up.
pub fn recruit_name(id: u64) -> String {
    let (name, _) = recruit_identity(id);
    if id <= RECRUIT_IDENTITIES.len() as u64 {
        name.to_string()
    } else {
        format!("{name} {id}")
    }
}

/// A survivor defender's creation template. Its [`Self::id`] is the
/// `NpcCreation.archetype` name and the `Command::RecruitAs` archetype text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Archetype {
    Marksman,
    Guardian,
    Scout,
    Steadfast,
}
impl Archetype {
    /// In rotation order: recruit `id` is offered `ALL[(id - 1) % 4]` first.
    pub const ALL: [Self; 4] = [Self::Marksman, Self::Guardian, Self::Scout, Self::Steadfast];
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Marksman => 0,
            Self::Guardian => 1,
            Self::Scout => 2,
            Self::Steadfast => 3,
        }
    }
    /// The rotation archetype of recruit `id`: what every recruit joined as
    /// before the choice existed.
    pub fn rotation(id: u64) -> Self {
        Self::ALL[(id.saturating_sub(1) % Self::ALL.len() as u64) as usize]
    }
    /// The stable name: `NpcCreation.archetype` and the `RecruitAs` text.
    pub fn id(self) -> &'static str {
        match self {
            Self::Marksman => "Marksman",
            Self::Guardian => "Guardian",
            Self::Scout => "Scout",
            Self::Steadfast => "Steadfast",
        }
    }
    /// Player-facing name (the same words as [`Self::id`]).
    pub fn label(self) -> &'static str {
        self.id()
    }
    /// The archetype named exactly `id`, or None.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
    /// The archetype a creation was made from, or None for a custom one.
    pub fn from_creation(creation: &NpcCreation) -> Option<Self> {
        Self::from_id(&creation.archetype)
    }
    /// The starting attributes and skills.
    pub fn creation(self) -> NpcCreation {
        NpcCreation::archetype(self.index())
    }
    /// The tactics policy a recruit of this archetype starts with.
    pub fn default_policy(self) -> Policy {
        match self {
            Self::Marksman => Policy::Marksman,
            Self::Guardian => Policy::Guardian,
            Self::Scout => Policy::Balanced,
            Self::Steadfast => Policy::BossFocus,
        }
    }
}

/// One archetype a recruit offer lets the player pick, with the policy and
/// stats the recruit would start with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchetypeChoice {
    pub archetype: Archetype,
    pub policy: Policy,
    pub st: i32,
    pub dx: i32,
    pub ht: i32,
    pub guns: i32,
    pub melee: i32,
}
impl ArchetypeChoice {
    pub fn of(archetype: Archetype) -> Self {
        let c = archetype.creation();
        Self {
            archetype,
            policy: archetype.default_policy(),
            st: c.st,
            dx: c.dx,
            ht: c.ht,
            guns: c.guns,
            melee: c.melee,
        }
    }
}

/// The next recruit a liberated country offers (`Game::recruit_offer`).
/// Derived on every read; never saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecruitOffer {
    pub country_id: String,
    pub country_name: String,
    /// What recruiting costs now (`Campaign::recruit_price`).
    pub price: u64,
    pub name: String,
    pub appearance: Appearance,
    pub origin_trait: OriginTrait,
    /// Two distinct archetypes; `Command::Recruit` takes the first.
    pub archetypes: [ArchetypeChoice; 2],
}

/// Why a country offers no recruit right now, in the order it is checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecruitBlocker {
    /// The upgrade node that unlocks recruitment is not owned.
    NeedsNode {
        id: String,
        name: String,
        price: u64,
    },
    /// Recruits join only from a fully liberated country.
    NotLiberated,
    RosterFull,
    /// A defender is out on the main defense.
    DefenseInProgress,
    /// Recruiting costs this many coins, more than the camp holds.
    NeedCoins(u64),
}
impl RecruitBlocker {
    /// Plain player-facing text.
    pub fn label(&self) -> String {
        match self {
            Self::NeedsNode { name, price, .. } => {
                format!(
                    "Unlock {name} ({} coins) to recruit survivors",
                    crate::coins::grouped(*price)
                )
            }
            Self::NotLiberated => "Recruit in a fully liberated country".into(),
            Self::RosterFull => "Survivor roster is full".into(),
            Self::DefenseInProgress => {
                "Return the active defender to camp before recruiting".into()
            }
            Self::NeedCoins(price) => recruitment_requires(*price),
        }
    }
}
/// "Recruitment requires 1,500 coins": the recruit price the camp cannot
/// pay, read by [`RecruitBlocker::NeedCoins`] and the recruit preflight.
pub(crate) fn recruitment_requires(price: u64) -> String {
    format!(
        "Recruitment requires {} coins",
        crate::coins::grouped(price)
    )
}
impl std::fmt::Display for RecruitBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
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
    /// A survivor defender's creation archetype; None for the player and
    /// for a custom creation.
    pub archetype: Option<Archetype>,
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
/// `SetNextDefender` would accept it between defenses (or the typed reason
/// it would not), whether it is the next defender, and its power at the
/// next threat. A live defense rejects every choice with
/// `CommandError::DefenseInProgress` and leaves `selectable` at its
/// between-defenses verdict, with one exception: during a takeover the
/// defending recruit's row reads `Err(DefenderBlocker::Defending)`.
#[derive(Clone, Debug, PartialEq)]
pub struct DefenderOption {
    pub card: DefenderCard,
    pub selectable: Result<(), DefenderBlocker>,
    pub selected: bool,
    pub power: DefenderPower,
}

/// How a defender's [`DefenderPower::rating`] compares with a fresh starter
/// (100): Weak below 85, Even from 85 to 115, Strong above 115.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PowerBand {
    Weak,
    Even,
    Strong,
}
impl PowerBand {
    pub fn for_rating(rating: u16) -> Self {
        match rating {
            0..=84 => Self::Weak,
            85..=115 => Self::Even,
            _ => Self::Strong,
        }
    }
    /// Plain player-facing text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Weak => "Weak",
            Self::Even => "Even",
            Self::Strong => "Strong",
        }
    }
}

/// A comparable strength for any defender against a given victory threat
/// (`Game::defender_power`, CD11). Analytic, from the inputs a deploy uses:
/// the expected kill rate against the reference enemy and the expected time
/// the defender survives its arrivals, in the defender's own combat model.
/// `rating` is the geometric mean of both against a fresh starter's at
/// threat 0 (exactly 100 in either model), clamped to 1..=999. Derived on
/// every read; never saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefenderPower {
    pub defender: DefenderId,
    pub threat: u32,
    pub kills_per_min: f32,
    pub survival_s: f32,
    pub rating: u16,
    pub band: PowerBand,
}

/// A recruit's live guard duty, as its service record shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardDuty {
    pub country_id: String,
    pub duty_ticks: u64,
}

/// A recruit's service history (`Game::service_record`, design 1.10).
/// Derived from the run history (each record's `recruit_id`), the recruit's
/// own counters, its live guard assignment and the saved
/// `Recruit.containments`; never saved itself. Runs recorded before the
/// history named their recruit are not counted ("since records began").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRecord {
    pub recruit_id: u64,
    /// Every recorded run the recruit defended.
    pub deployments: u32,
    /// Runs defending the main run (no expedition).
    pub takeovers: u32,
    /// Distinct expeditions the recruit's runs came from.
    pub expeditions: u32,
    pub victories: u32,
    pub defeats: u32,
    pub withdrawals: u32,
    pub kills: u64,
    pub coins: u64,
    /// Distinct country stages won.
    pub stages_cleared: u32,
    /// Victories on a country's final stage.
    pub liberations: u32,
    /// The countries fought in, in the order first fought.
    pub countries: Vec<String>,
    pub last_outcome: Option<crate::sim::Outcome>,
    /// The recruit's own attempt and victory counters (guard wins included).
    pub npc_attempts: u64,
    pub npc_victories: u64,
    /// The live guard assignment, if the recruit is guarding.
    pub guard: Option<GuardDuty>,
    /// Outbreaks the recruit contained while guarding.
    pub containments: u32,
}
impl ServiceRecord {
    /// Whether the record starts mid-career and is labelled "since records
    /// began" (design 1.10): the recruit's own attempt counter runs ahead of
    /// the runs the history names it on. Every v7 run, a takeover or an
    /// expedition stage, appends a record carrying `recruit_id` as it counts
    /// the attempt, so only runs from before the history named its recruit
    /// can make the difference. A pure read; nothing is saved for it.
    pub fn since_records_began(&self) -> bool {
        self.npc_attempts > u64::from(self.deployments)
    }
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
