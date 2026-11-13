//! Building definitions of the regional economy data (`economy.buildings`).
//!
//! A definition says what a building costs, how long it takes to raise, which
//! cells it covers, what it becomes when finished and where it may stand.
//! Validation and hashing are driven from [`super::economy_spec::EconomySpec`].
use super::economy_spec::{EconomySpec, GoodTag, Stack, role_key};
use crate::village::Role;
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
    /// with seed for every plot. Only the first founding field in the list is
    /// used.
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
    }
}

/// The built-in buildings: the cottage and the thirteen workplaces of the
/// shipped settlements, plus the stores and the stall residents can add.
pub fn default_buildings(economy: &EconomySpec) -> Vec<BuildingDef> {
    let mut out = vec![BuildingDef {
        footprint: FootprintCells { width: 4, depth: 5 },
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
            ..building(id, display, &[("plank", 10), ("log", 6)], 800)
        });
    }
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 120 },
        footprint: FootprintCells { width: 4, depth: 4 },
        solid: false,
        ..building("stockpile", "Stockpile", &[("log", 4)], 200)
    });
    out.push(BuildingDef {
        kind: BuildingKind::Storage { capacity: 240 },
        footprint: FootprintCells { width: 5, depth: 4 },
        civic: false,
        ..building("barn", "Barn", &[("plank", 12), ("log", 6)], 600)
    });
    out.push(BuildingDef {
        kind: BuildingKind::Home { beds: 4 },
        footprint: FootprintCells { width: 5, depth: 6 },
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
        ..building(
            "market_stall",
            "Market stall",
            &[("plank", 6), ("log", 2)],
            300,
        )
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
                h.u8(profession.tag());
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
        h.u8(u8::from(def.solid));
        h.u8(u8::from(def.civic));
        h.u32(def.placement.surfaces.len() as u32);
        for surface in &def.placement.surfaces {
            h.u8(*surface as u8);
        }
        h.u32(def.placement.max_slope_mm);
        h.u32(def.placement.clearance);
    }
}
