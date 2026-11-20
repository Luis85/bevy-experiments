//! Building definitions of the regional economy data (`economy.buildings`).
//!
//! A definition says what a building costs, how long it takes to raise, which
//! cells it covers, what it becomes when finished and where it may stand.
//! Validation and hashing are driven from [`super::economy_spec::EconomySpec`].
use super::economy_spec::{EconomySpec, GoodTag, Stack, role_key};
use crate::village::Role;
use crate::village::catalog::{COOP, HERDER};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// Ground a building may stand on. Water is never buildable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildSurface {
    Grass,
    Soil,
    Stone,
}

/// What a finished building is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildingKind {
    /// A household with this many beds.
    Home { beds: u32 },
    /// A workplace of one profession. `station` must be that profession's
    /// workstation kind; `capacity` is the number of workers it seats.
    Workplace {
        profession: Role,
        station: String,
        #[serde(default = "two")]
        capacity: u32,
    },
    /// A stockpile or barn holding this many units, selling to residents.
    Storage { capacity: u32 },
    /// A market stall holding this many units, selling to anyone.
    Stall { capacity: u32 },
}
fn two() -> u32 {
    2
}

impl BuildingKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Home { .. } => "home",
            Self::Workplace { .. } => "workplace",
            Self::Storage { .. } => "storage",
            Self::Stall { .. } => "stall",
        }
    }
}

/// Cells of the building's body: `width` across the entrance, `depth` behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintCells {
    pub width: u32,
    pub depth: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlacementRules {
    /// Ground every body cell must have.
    pub surfaces: Vec<BuildSurface>,
    /// Largest height difference across the body and its entrance.
    pub max_slope_mm: u32,
    /// Free cells kept between the body and any other building.
    pub clearance: u32,
    /// A waterfront building (the trading post): it may stand on the coast,
    /// within `COAST_REACH_CELLS` of the sea, however far that is from its
    /// settlement's centre.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub coast: bool,
}
impl Default for PlacementRules {
    fn default() -> Self {
        Self {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Stone],
            max_slope_mm: 800,
            clearance: 1,
            coast: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingDef {
    /// Stable key. Renaming it is a new building.
    pub id: String,
    /// Name shown to players, for example `Cottage`.
    pub display: String,
    /// Goods that must be delivered to the site before work starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost: Vec<Stack>,
    /// Builder ticks after the materials are in.
    pub work_ticks: u32,
    pub footprint: FootprintCells,
    pub kind: BuildingKind,
    /// A solid body blocks walking and sight; an open yard does not.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub solid: bool,
    /// Whether the civic planner may commission it.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub civic: bool,
    #[serde(default)]
    pub placement: PlacementRules,
    /// Crop plots around the building. Only an open storage building may have
    /// them: the store is the granary of the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<FieldDef>,
    /// Animals kept as counted stock. Only a workplace may keep them: its
    /// trade's recipes feed them (input) and collect what they yield (output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub livestock: Option<LivestockDef>,
    /// How the finished building wears and whether it can catch fire
    /// (`civic::upkeep`). Absent: it never wears or burns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upkeep: Option<UpkeepDef>,
}

/// The wear and fire risk of one building kind (`civic::upkeep`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpkeepDef {
    /// Condition lost a day, permille of a sound building (0 to 1000).
    pub wear_per_day: u32,
    /// Wear on a day of storm or snow, percent of an ordinary day's.
    #[serde(default = "hundred")]
    pub bad_weather_percent: u32,
    /// Share of the building's cost and work a repair from nothing (a
    /// condition of 0) takes, percent; a lighter repair takes that share of
    /// the damage. A repair restores the building to sound.
    pub repair_percent: u32,
    /// A fire risk: a hearth, an oven or a forge. Absent: it never burns
    /// on its own (a fire next door may still spread to it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire: Option<FireDef>,
}
fn hundred() -> u32 {
    100
}

/// The daily fire risk of a building kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FireDef {
    /// Chance of a fire on one day, permille (at most 100).
    pub daily_permille: u32,
    /// The seasons with that risk (a home's hearth burns in winter); empty:
    /// every season.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<crate::village::Season>,
}

/// The animals of a pen (`village::livestock`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivestockDef {
    /// The animals as players read them, plural (`hens`).
    pub display: String,
    /// Animals a new pen starts with, 1 to `max_head`.
    pub head: u32,
    /// The most the pen holds; a well-fed herd breeds up to it.
    pub max_head: u32,
    /// Animals a hungry herd never falls below (they forage for themselves).
    #[serde(default)]
    pub min_head: u32,
    /// Animals one finished batch of the trade's recipes at the pen feeds.
    pub fed_per_batch: u32,
    /// Hungry days in a row (some animal unfed at dawn) before one dies.
    pub starve_days: u32,
    /// Days in a row of a fully fed herd before one more is born, outside winter.
    pub breed_days: u32,
    /// Every settlement is founded with one such building, beside its farm.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub founding: bool,
}

/// The crop plots of a field building.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    /// Number of plots, each sown, tended and harvested on its own.
    pub plots: u32,
    /// Key of the crop in `economy.crops`.
    pub crop: String,
    /// Every settlement is founded with one such building, beside its farm,
    /// with seed for every plot. Every founding field of the list is laid
    /// out, in list order (the first beside the farm).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub founding: bool,
}
fn yes() -> bool {
    true
}
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

/// A good that can lie in a store or be built in: no wear, not worn.
pub fn storable(economy: &EconomySpec, kind: &str) -> bool {
    economy.good(kind).is_some_and(|good| {
        good.durability.is_none()
            && !good.tags.contains(&GoodTag::Tool)
            && !good.tags.contains(&GoodTag::Clothing)
    })
}

fn building(id: &str, display: &str, cost: &[(&str, u32)], work_ticks: u32) -> BuildingDef {
    BuildingDef {
        id: id.into(),
        display: display.into(),
        cost: cost
            .iter()
            .map(|(good, count)| Stack {
                good: (*good).into(),
                count: *count,
            })
            .collect(),
        work_ticks,
        footprint: FootprintCells { width: 5, depth: 5 },
        kind: BuildingKind::Home { beds: 2 },
        solid: true,
        civic: true,
        placement: PlacementRules::default(),
        field: None,
        livestock: None,
        upkeep: None,
    }
}

/// Wear and fire risk of a shipped building: condition lost a day, and the
/// fire chance a day with its seasons. Repairs take 40% of the cost from
/// nothing and bad weather wears at 150%. Reasons in
/// `docs/island-economy-authoring.md` (Upkeep and fire).
fn upkeep(wear: u32, fire: Option<(u32, &[crate::village::Season])>) -> Option<UpkeepDef> {
    Some(UpkeepDef {
        wear_per_day: wear,
        bad_weather_percent: 150,
        repair_percent: 40,
        fire: fire.map(|(daily_permille, seasons)| FireDef {
            daily_permille,
            seasons: seasons.to_vec(),
        }),
    })
}

/// The built-in buildings: the cottage and the thirteen workplaces of the
/// shipped settlements, plus the stores and the stall residents can add.
pub fn default_buildings(economy: &EconomySpec) -> Vec<BuildingDef> {
    use crate::village::Season::Winter;
    let mut out = vec![BuildingDef {
        footprint: FootprintCells { width: 4, depth: 5 },
        upkeep: upkeep(15, Some((3, &[Winter]))),
        ..building("cottage", "Cottage", &[("plank", 8), ("log", 4)], 600)
    }];
    for (role, id, display) in [
        (Role::Farmer, "farmstead", "Farmstead"),
        (Role::Miller, "mill", "Mill"),
        (Role::Baker, "bakery", "Bakery"),
        (Role::WaterKeeper, "well", "Well"),
        (Role::Woodcutter, "timber_yard", "Timber yard"),
        (
            Role::Carpenter,
            "carpenter_workshop",
            "Carpenter's workshop",
        ),
        (Role::Miner, "mine_head", "Mine head"),
        (Role::Smith, "smithy", "Smithy"),
        (Role::Innkeeper, "inn", "Inn"),
        (Role::Hunter, "hunting_lodge", "Hunting lodge"),
        (Role::Tanner, "tannery", "Tannery"),
        (Role::Tailor, "tailor_shop", "Tailor's shop"),
        (Role::Fisher, "fishing_hut", "Fishing hut"),
    ] {
        let open = matches!(role, Role::WaterKeeper | Role::Fisher);
        out.push(BuildingDef {
            kind: BuildingKind::Workplace {
                profession: role,
                station: economy
                    .profession(role)
                    .map(|entry| entry.station.clone())
                    .unwrap_or_default(),
                capacity: 2,
            },
            footprint: if open {
                FootprintCells { width: 3, depth: 3 }
            } else {
                FootprintCells { width: 5, depth: 5 }
            },
            solid: !open,
            // An oven and a forge burn all year; an open yard wears slower.
            upkeep: match role {
                Role::Baker | Role::Smith => upkeep(20, Some((5, &[]))),
                _ if open => upkeep(10, None),
                _ => upkeep(15, None),
            },
            ..building(id, display, &[("plank", 10), ("log", 6)], 800)
        });
    }
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 120 },
        footprint: FootprintCells { width: 4, depth: 4 },
        solid: false,
        upkeep: upkeep(10, None),
        ..building("stockpile", "Stockpile", &[("log", 4)], 200)
    });
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 240 },
        footprint: FootprintCells { width: 5, depth: 4 },
        civic: false,
        upkeep: upkeep(12, None),
        ..building("barn", "Barn", &[("plank", 12), ("log", 6)], 600)
    });
    out.push(BuildingDef {
        kind: BuildingKind::Home { beds: 4 },
        footprint: FootprintCells { width: 5, depth: 6 },
        upkeep: upkeep(15, Some((3, &[Winter]))),
        ..building("house", "Family house", &[("plank", 12), ("log", 6)], 800)
    });
    out.push(BuildingDef {
        // A field keeps its harvest in an open granary, so it is a store too.
        kind: BuildingKind::Storage { capacity: 80 },
        field: Some(FieldDef {
            plots: 8,
            crop: "grain".into(),
            founding: true,
        }),
        footprint: FootprintCells { width: 8, depth: 6 },
        solid: false,
        placement: PlacementRules {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Soil, BuildSurface::Stone],
            max_slope_mm: 2000,
            ..PlacementRules::default()
        },
        ..building("field", "Field", &[("log", 4)], 300)
    });
    out.push(BuildingDef {
        kind: BuildingKind::Stall { capacity: 240 },
        footprint: FootprintCells { width: 4, depth: 3 },
        solid: false,
        civic: false,
        // It stands at the water, where the boat ties up.
        placement: PlacementRules {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Soil, BuildSurface::Stone],
            max_slope_mm: 1200,
            coast: true,
            ..PlacementRules::default()
        },
        upkeep: upkeep(12, None),
        ..building(
            "trading_post",
            "Trading post",
            &[("plank", 10), ("log", 4)],
            400,
        )
    });
    out.push(BuildingDef {
        kind: BuildingKind::Stall { capacity: 60 },
        footprint: FootprintCells { width: 3, depth: 2 },
        solid: false,
        civic: false,
        upkeep: upkeep(12, None),
        ..building(
            "market_stall",
            "Market stall",
            &[("plank", 6), ("log", 2)],
            300,
        )
    });
    // The second field crop's founding field, beside the grain field.
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 60 },
        field: Some(FieldDef {
            plots: 6,
            crop: "turnip".into(),
            founding: true,
        }),
        footprint: FootprintCells { width: 6, depth: 6 },
        solid: false,
        placement: PlacementRules {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Soil, BuildSurface::Stone],
            max_slope_mm: 2000,
            ..PlacementRules::default()
        },
        ..building("turnip_field", "Turnip field", &[("log", 4)], 300)
    });
    // Hens kept by the hen keeper: an open, fenced yard.
    out.push(BuildingDef {
        kind: BuildingKind::Workplace {
            profession: Role::from_key(HERDER),
            station: COOP.into(),
            capacity: 1,
        },
        livestock: Some(LivestockDef {
            display: "hens".into(),
            head: 4,
            max_head: 8,
            min_head: 2,
            fed_per_batch: 2,
            starve_days: 3,
            breed_days: 4,
            founding: true,
        }),
        footprint: FootprintCells { width: 4, depth: 3 },
        solid: false,
        upkeep: upkeep(12, None),
        ..building("hen_house", "Hen house", &[("plank", 6), ("log", 4)], 400)
    });
    // A small plot the overseer lays out with the Shape tools; the planner
    // never commissions it.
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 30 },
        field: Some(FieldDef {
            plots: 4,
            crop: "grain".into(),
            founding: false,
        }),
        footprint: FootprintCells { width: 5, depth: 4 },
        solid: false,
        civic: false,
        placement: PlacementRules {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Soil, BuildSurface::Stone],
            max_slope_mm: 2000,
            ..PlacementRules::default()
        },
        ..building("garden", "Garden plot", &[("log", 2)], 200)
    });
    out
}

/// Every error names the offending building.
pub fn validate(economy: &EconomySpec) -> Result<(), String> {
    if economy.buildings.len() > 64 {
        return Err("economy: buildings needs at most 64 entries".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for def in &economy.buildings {
        let name = &def.id;
        if name.trim().is_empty() {
            return Err("economy: a building has an empty id".into());
        }
        if !seen.insert(name.as_str()) {
            return Err(format!("economy: duplicate building `{name}`"));
        }
        if def.display.trim().is_empty() {
            return Err(format!("economy: building `{name}` needs a display name"));
        }
        if !(1..=100_000).contains(&def.work_ticks) {
            return Err(format!(
                "economy: building `{name}` work_ticks must be 1..=100000"
            ));
        }
        if !(1..=12).contains(&def.footprint.width) || !(1..=12).contains(&def.footprint.depth) {
            return Err(format!(
                "economy: building `{name}` footprint must be 1..=12 cells wide and deep"
            ));
        }
        let mut goods = std::collections::BTreeSet::new();
        for stack in &def.cost {
            if economy.good(&stack.good).is_none() {
                return Err(format!(
                    "economy: building `{name}` cost references unknown good `{}`",
                    stack.good
                ));
            }
            if !goods.insert(stack.good.as_str()) {
                return Err(format!(
                    "economy: building `{name}` lists cost `{}` twice",
                    stack.good
                ));
            }
            if !(1..=1000).contains(&stack.count) {
                return Err(format!(
                    "economy: building `{name}` cost of `{}` must be 1..=1000",
                    stack.good
                ));
            }
            if !storable(economy, &stack.good) {
                return Err(format!(
                    "economy: building `{name}` cost `{}` is a tool or clothing and cannot be a building material",
                    stack.good
                ));
            }
        }
        match &def.kind {
            BuildingKind::Home { beds } => {
                if !(1..=8).contains(beds) {
                    return Err(format!(
                        "economy: building `{name}` is a home and needs 1..=8 beds"
                    ));
                }
            }
            BuildingKind::Workplace {
                profession,
                station,
                capacity,
            } => {
                let Some(entry) = economy
                    .profession(*profession)
                    .filter(|_| *profession != Role::Visitor)
                else {
                    return Err(format!(
                        "economy: building `{name}` is a workplace for unknown profession `{}`",
                        role_key(*profession)
                    ));
                };
                if &entry.station != station {
                    return Err(format!(
                        "economy: building `{name}` is a workplace for `{}`, whose station is `{}`, not `{station}`",
                        role_key(*profession),
                        entry.station
                    ));
                }
                if !(1..=8).contains(capacity) {
                    return Err(format!(
                        "economy: building `{name}` workplace capacity must be 1..=8"
                    ));
                }
            }
            BuildingKind::Storage { capacity } | BuildingKind::Stall { capacity } => {
                if !(1..=10_000).contains(capacity) {
                    return Err(format!(
                        "economy: building `{name}` capacity must be 1..=10000 units"
                    ));
                }
            }
        }
        if let Some(field) = &def.field {
            if !matches!(def.kind, BuildingKind::Storage { .. }) || def.solid {
                return Err(format!(
                    "economy: building `{name}` has a field, so it must be an open (not solid) \
                     building of type storage: the granary of the field"
                ));
            }
            if !(1..=64).contains(&field.plots) {
                return Err(format!(
                    "economy: building `{name}` field needs 1..=64 plots"
                ));
            }
            if economy.crop(&field.crop).is_none() {
                return Err(format!(
                    "economy: building `{name}` field grows unknown crop `{}`",
                    field.crop
                ));
            }
        }
        if let Some(herd) = &def.livestock {
            if !matches!(def.kind, BuildingKind::Workplace { .. }) {
                return Err(format!(
                    "economy: building `{name}` keeps livestock, so it must be a workplace: \
                     its trade feeds the animals and collects what they yield"
                ));
            }
            if herd.display.trim().is_empty() {
                return Err(format!(
                    "economy: building `{name}` livestock needs a display name"
                ));
            }
            if !(1..=64).contains(&herd.max_head)
                || !(1..=herd.max_head).contains(&herd.head)
                || herd.min_head > herd.head
            {
                return Err(format!(
                    "economy: building `{name}` livestock needs 1 <= head <= max_head <= 64 \
                     and min_head <= head"
                ));
            }
            if !(1..=herd.max_head).contains(&herd.fed_per_batch) {
                return Err(format!(
                    "economy: building `{name}` livestock fed_per_batch must be 1..=max_head"
                ));
            }
            if !(1..=100).contains(&herd.starve_days) || !(1..=100).contains(&herd.breed_days) {
                return Err(format!(
                    "economy: building `{name}` livestock starve_days and breed_days must be 1..=100"
                ));
            }
        }
        if let Some(upkeep) = &def.upkeep {
            if upkeep.wear_per_day > 1000
                || !(1..=1000).contains(&upkeep.bad_weather_percent)
                || !(1..=100).contains(&upkeep.repair_percent)
            {
                return Err(format!(
                    "economy: building `{name}` upkeep needs wear_per_day up to 1000, \
                     bad_weather_percent 1..=1000 and repair_percent 1..=100"
                ));
            }
            if def.cost.is_empty() {
                return Err(format!(
                    "economy: building `{name}` has upkeep but no cost: a repair takes a \
                     share of the cost, so a building that wears must cost something"
                ));
            }
            if upkeep
                .fire
                .as_ref()
                .is_some_and(|fire| fire.daily_permille > 100)
            {
                return Err(format!(
                    "economy: building `{name}` fire daily_permille must be at most 100"
                ));
            }
        }
        let rules = &def.placement;
        if rules.surfaces.is_empty() {
            return Err(format!(
                "economy: building `{name}` placement needs at least one buildable surface"
            ));
        }
        if rules.max_slope_mm > 3000 || rules.clearance > 8 {
            return Err(format!(
                "economy: building `{name}` placement needs max_slope_mm up to 3000 and clearance up to 8 cells"
            ));
        }
    }
    Ok(())
}

pub fn sim_hash(buildings: &[BuildingDef], h: &mut Fnv64) {
    h.u32(buildings.len() as u32);
    for def in buildings {
        h.str(&def.id);
        h.str(&def.display);
        h.u32(def.cost.len() as u32);
        for stack in &def.cost {
            h.str(&stack.good);
            h.u32(stack.count);
        }
        h.u32(def.work_ticks);
        h.u32(def.footprint.width);
        h.u32(def.footprint.depth);
        match &def.kind {
            BuildingKind::Home { beds } => {
                h.u8(0);
                h.u32(*beds);
            }
            BuildingKind::Workplace {
                profession,
                station,
                capacity,
            } => {
                h.u8(1);
                // The key of a data-only trade too (every one has tag 14).
                profession.sim_hash(h);
                h.str(station);
                h.u32(*capacity);
            }
            BuildingKind::Storage { capacity } => {
                h.u8(2);
                h.u32(*capacity);
            }
            BuildingKind::Stall { capacity } => {
                h.u8(3);
                h.u32(*capacity);
            }
        }
        h.opt(def.field.as_ref(), |h, field| {
            h.u32(field.plots);
            h.str(&field.crop);
            h.u8(u8::from(field.founding));
        });
        h.opt(def.livestock.as_ref(), |h, herd| {
            h.str(&herd.display);
            h.u32(herd.head);
            h.u32(herd.max_head);
            h.u32(herd.min_head);
            h.u32(herd.fed_per_batch);
            h.u32(herd.starve_days);
            h.u32(herd.breed_days);
            h.u8(u8::from(herd.founding));
        });
        h.u8(u8::from(def.solid));
        h.u8(u8::from(def.civic));
        h.u32(def.placement.surfaces.len() as u32);
        for surface in &def.placement.surfaces {
            h.u8(*surface as u8);
        }
        h.u32(def.placement.max_slope_mm);
        h.u32(def.placement.clearance);
        h.opt(def.upkeep.as_ref(), |h, upkeep| {
            h.u32(upkeep.wear_per_day);
            h.u32(upkeep.bad_weather_percent);
            h.u32(upkeep.repair_percent);
            h.opt(upkeep.fire.as_ref(), |h, fire| {
                h.u32(fire.daily_permille);
                h.u32(fire.seasons.len() as u32);
                for season in &fire.seasons {
                    h.u8(*season as u8);
                }
            });
        });
    }
}
