//! Authoritative civic state: construction sites, finished buildings, stores
//! and the planner's memory. Stable names and encodings are save contracts.
use crate::village::Role;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Owner of goods that belong to the settlement: sales credit the civic fund.
pub const CIVIC_OWNER: SimId = SimId(0);

/// Goods by key.
pub type Goods = BTreeMap<String, u32>;

fn hash_goods(h: &mut Fnv64, goods: &Goods) {
    h.u32(goods.len() as u32);
    for (kind, count) in goods {
        h.str(kind);
        h.u32(*count);
    }
}

/// Where a building stands: its entrance cell, quarter turns and body cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Plot {
    /// Entrance cell.
    pub x: i32,
    pub z: i32,
    /// Quarter turns: the body lies towards +z, +x, -z, -x of the entrance.
    pub rotation: u8,
    /// Inclusive body rectangle in cells.
    pub lo_x: i32,
    pub lo_z: i32,
    pub hi_x: i32,
    pub hi_z: i32,
    /// A solid body blocks walking and sight.
    pub solid: bool,
}
impl Plot {
    pub fn contains(&self, x: i32, z: i32) -> bool {
        (self.lo_x..=self.hi_x).contains(&x) && (self.lo_z..=self.hi_z).contains(&z)
    }
    /// Body cells, z-major ascending.
    pub fn cells(&self) -> impl Iterator<Item = CellCoord> + '_ {
        (self.lo_z..=self.hi_z)
            .flat_map(move |z| (self.lo_x..=self.hi_x).map(move |x| CellCoord::new(x, z)))
    }
    /// Centre of the entrance cell.
    pub fn door(&self) -> SimPos {
        crate::village::geography::cell_centre(self.x, self.z)
    }
    fn sim_hash(&self, h: &mut Fnv64) {
        for value in [self.x, self.z, self.lo_x, self.lo_z, self.hi_x, self.hi_z] {
            h.i32(value);
        }
        h.u8(self.rotation);
        h.u8(u8::from(self.solid));
    }
}

/// A hauler's time-limited claim: units promised to a site or a stall, or
/// units set aside in a store. `loaded` is set once the goods are in the pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Hold {
    pub kind: String,
    pub count: u32,
    pub until: u64,
    pub loaded: bool,
}
fn hash_holds(h: &mut Fnv64, holds: &BTreeMap<SimId, Hold>) {
    h.u32(holds.len() as u32);
    for (holder, hold) in holds {
        h.u32(holder.0);
        h.str(&hold.kind);
        h.u32(hold.count);
        h.u64(hold.until);
        h.u8(u8::from(hold.loaded));
    }
}
/// Units of `kind` claimed by holders other than `except` at `now`.
pub fn held(holds: &BTreeMap<SimId, Hold>, kind: &str, except: Option<SimId>, now: u64) -> u32 {
    holds
        .iter()
        .filter(|(holder, hold)| Some(**holder) != except && hold.kind == kind && hold.until > now)
        .map(|(_, hold)| hold.count)
        .fold(0, u32::saturating_add)
}

/// What a construction site becomes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum SiteKind {
    /// A building of the catalog (`ConstructionSite.building` is its key).
    #[default]
    Building,
    /// A prop of the catalog (`ConstructionSite.building` is the prop's key),
    /// placed with the cell and quarter turns of its `PlaceProp`.
    Prop { x: i32, z: i32, rotation: u8 },
    /// Repairs to a standing building (`ConstructionSite.building` is the
    /// key of its definition): a share of its materials and work, laid out
    /// on the cell at its door; finished, it restores the building's
    /// condition (`upkeep`) and the site goes.
    Repair { target: SimId },
}

/// A building under construction. The entity keeps its `SimId` when finished.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ConstructionSite {
    /// Key of the building definition.
    pub building: String,
    pub label: String,
    pub settlement: SimId,
    pub plot: Plot,
    /// The separate solid body, when the building is solid.
    pub blocker: Option<SimId>,
    pub cost: Goods,
    pub delivered: Goods,
    /// Deliveries on their way, by hauler.
    pub incoming: BTreeMap<SimId, Hold>,
    pub work_ticks: u32,
    pub work_done: u32,
    pub placed_at: u64,
    /// Commissioned by the civic planner rather than placed by command.
    pub civic: bool,
    pub reason: String,
    /// Coins the civic fund has paid for delivered materials.
    pub paid: u32,
    /// The resident walking to or working on the site, until `builder_until`.
    pub builder: Option<SimId>,
    pub builder_until: u64,
    /// Tick of the last delivery of materials.
    #[serde(default)]
    pub last_delivery_at: Option<u64>,
    /// A building or a prop.
    pub kind: SiteKind,
}
impl ConstructionSite {
    pub fn is_prop(&self) -> bool {
        matches!(self.kind, SiteKind::Prop { .. })
    }
    /// The building a repair site mends; `None` for any other site.
    pub fn repairs(&self) -> Option<SimId> {
        match self.kind {
            SiteKind::Repair { target } => Some(target),
            _ => None,
        }
    }
    /// Whether `actor` may take the builder's place now.
    pub fn free_for(&self, actor: SimId, now: u64) -> bool {
        self.builder.is_none_or(|builder| builder == actor) || self.builder_until <= now
    }
    /// Units of `kind` still to be delivered.
    pub fn missing(&self, kind: &str) -> u32 {
        self.cost
            .get(kind)
            .copied()
            .unwrap_or(0)
            .saturating_sub(self.delivered.get(kind).copied().unwrap_or(0))
    }
    /// Units of `kind` neither delivered nor promised by another hauler.
    pub fn open(&self, kind: &str, hauler: Option<SimId>, now: u64) -> u32 {
        self.missing(kind)
            .saturating_sub(held(&self.incoming, kind, hauler, now))
    }
    pub fn supplied(&self) -> bool {
        self.cost.keys().all(|kind| self.missing(kind) == 0)
    }
}
impl SimHash for ConstructionSite {
    const NAME: &'static str = "regional_construction_site_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.building);
        h.str(&self.label);
        h.u32(self.settlement.0);
        self.plot.sim_hash(h);
        h.opt(self.blocker.as_ref(), |h, id| h.u32(id.0));
        hash_goods(h, &self.cost);
        hash_goods(h, &self.delivered);
        hash_holds(h, &self.incoming);
        h.u32(self.work_ticks);
        h.u32(self.work_done);
        h.u64(self.placed_at);
        h.u8(u8::from(self.civic));
        h.str(&self.reason);
        h.u32(self.paid);
        h.opt(self.builder.as_ref(), |h, id| h.u32(id.0));
        h.u64(self.builder_until);
        h.opt(self.last_delivery_at.as_ref(), |h, tick| h.u64(*tick));
        match &self.kind {
            SiteKind::Building => h.u8(0),
            SiteKind::Prop { x, z, rotation } => {
                h.u8(1);
                h.i32(*x);
                h.i32(*z);
                h.u8(*rotation);
            }
            SiteKind::Repair { target } => {
                h.u8(2);
                h.u32(target.0);
            }
        }
    }
}

/// A finished building placed after the world was founded.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Building {
    pub building: String,
    pub label: String,
    pub settlement: SimId,
    pub plot: Plot,
    pub blocker: Option<SimId>,
    pub completed_at: u64,
    /// Seats of a workplace; 0 for every other kind.
    pub seats: u32,
}
impl SimHash for Building {
    const NAME: &'static str = "regional_building_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.building);
        h.str(&self.label);
        h.u32(self.settlement.0);
        self.plot.sim_hash(h);
        h.opt(self.blocker.as_ref(), |h, id| h.u32(id.0));
        h.u64(self.completed_at);
        h.u32(self.seats);
    }
}

/// A stockpile, barn or market stall: goods lie here on behalf of their
/// owners and are sold at the quoted price while the owner is elsewhere.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Depot {
    pub capacity: u32,
    /// A stall sells to anyone; storage only to residents.
    pub public: bool,
    /// Good → owner → units. [`CIVIC_OWNER`] is the settlement itself.
    pub lots: BTreeMap<String, BTreeMap<SimId, u32>>,
    /// Units set aside for a hauler who is on the way.
    pub holds: BTreeMap<SimId, Hold>,
    /// Restocking deliveries on their way, by hauler.
    pub incoming: BTreeMap<SimId, Hold>,
    pub sold_units: u64,
    pub turnover: u64,
    /// Goods this store takes; empty takes anything a store can hold. A
    /// field's granary takes only the seed and harvest of its crop.
    #[serde(default)]
    pub only: Vec<String>,
    /// Units of capacity a worn or broken building cannot take new goods
    /// into (`upkeep`); goods already there stay and sell.
    #[serde(default)]
    pub withheld: u32,
}
impl Depot {
    pub fn new(capacity: u32, public: bool) -> Self {
        Self {
            withheld: 0,
            only: Vec::new(),
            capacity,
            public,
            lots: BTreeMap::new(),
            holds: BTreeMap::new(),
            incoming: BTreeMap::new(),
            sold_units: 0,
            turnover: 0,
        }
    }
    /// Whether goods of `kind` may be laid here.
    pub fn accepts(&self, kind: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|only| only == kind)
    }
    pub fn stock(&self, kind: &str) -> u32 {
        self.lots.get(kind).map_or(0, |owners| {
            owners.values().copied().fold(0, u32::saturating_add)
        })
    }
    pub fn used(&self) -> u32 {
        self.lots
            .values()
            .flat_map(|owners| owners.values())
            .copied()
            .fold(0, u32::saturating_add)
    }
    pub fn free(&self) -> u32 {
        self.capacity.saturating_sub(self.used())
    }
    /// Units of `kind` on the shelf that no other hauler has claimed.
    pub fn available(&self, kind: &str, buyer: Option<SimId>, now: u64) -> u32 {
        self.stock(kind)
            .saturating_sub(held(&self.holds, kind, buyer, now))
    }
    /// Free room after restocking deliveries that are on their way.
    pub fn room(&self, hauler: Option<SimId>, now: u64) -> u32 {
        let promised = self
            .incoming
            .iter()
            .filter(|(holder, hold)| Some(**holder) != hauler && hold.until > now)
            .map(|(_, hold)| hold.count)
            .fold(0, u32::saturating_add);
        self.capacity
            .saturating_sub(self.withheld)
            .saturating_sub(self.used())
            .saturating_sub(promised)
    }
    pub(crate) fn add(&mut self, kind: &str, owner: SimId, count: u32) {
        if count > 0 {
            *self
                .lots
                .entry(kind.into())
                .or_default()
                .entry(owner)
                .or_default() += count;
        }
    }
    /// Removes `count` units, oldest owner id first; returns `(owner, units)`.
    pub(crate) fn take(&mut self, kind: &str, count: u32) -> Vec<(SimId, u32)> {
        let mut taken = Vec::new();
        let mut left = count;
        if let Some(owners) = self.lots.get_mut(kind) {
            for (owner, units) in owners.iter_mut() {
                let part = (*units).min(left);
                if part > 0 {
                    *units -= part;
                    left -= part;
                    taken.push((*owner, part));
                }
                if left == 0 {
                    break;
                }
            }
            owners.retain(|_, units| *units > 0);
            if owners.is_empty() {
                self.lots.remove(kind);
            }
        }
        taken
    }
}
impl SimHash for Depot {
    const NAME: &'static str = "regional_depot_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.capacity);
        h.u8(u8::from(self.public));
        h.u32(self.lots.len() as u32);
        for (kind, owners) in &self.lots {
            h.str(kind);
            h.u32(owners.len() as u32);
            for (owner, units) in owners {
                h.u32(owner.0);
                h.u32(*units);
            }
        }
        hash_holds(h, &self.holds);
        hash_holds(h, &self.incoming);
        h.u64(self.sold_units);
        h.u64(self.turnover);
        h.u32(self.only.len() as u32);
        for kind in &self.only {
            h.str(kind);
        }
        h.u32(self.withheld);
    }
}

/// One stage of building work in progress on a site.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct SiteWork {
    pub site: SimId,
    pub ends: u64,
    /// Work ticks this stage adds to the site.
    pub ticks: u32,
}
/// A building stage a critical need interrupted (review fix S3.5): the
/// builder comes back to the same site and finishes the stage's remaining
/// ticks instead of starting it again. Forgotten after `expires_at` or when
/// the site is gone.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct SuspendedWork {
    pub site: SimId,
    /// Work ticks the stage adds when finished.
    pub ticks: u32,
    /// Clock ticks of the stage still to go.
    pub remaining: u64,
    pub expires_at: u64,
}
impl SimHash for SuspendedWork {
    const NAME: &'static str = "island_suspended_work_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.site.0);
        h.u32(self.ticks);
        h.u64(self.remaining);
        h.u64(self.expires_at);
    }
}

/// Ticks an interrupted building stage is kept for its builder.
pub const SUSPEND_TICKS: u64 = 1200;

impl SimHash for SiteWork {
    const NAME: &'static str = "regional_site_work_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.site.0);
        h.u64(self.ends);
        h.u32(self.ticks);
    }
}

/// What the planner of one settlement remembers between reviews.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct PlannerState {
    /// Consecutive reviews with residents nobody could house.
    pub bed_shortage_reviews: u32,
    /// Consecutive reviews with more unstored surplus than the threshold.
    pub overflow_reviews: u32,
    /// Consecutive reviews with unmet demand and every seat taken, by profession.
    pub demand_reviews: BTreeMap<Role, u32>,
    /// The open civic projects of this settlement, ascending by identity. At
    /// most one answers each need (homes, a field of one crop, storage, one
    /// workplace), and the planner keeps no more going than the settlement
    /// can staff (`tuning.planner_residents_per_project`).
    pub projects: Vec<SimId>,
    pub last_review: u64,
    /// Figures of the last review, for inspectors.
    pub homeless: u32,
    pub free_beds: u32,
    pub surplus_units: u32,
    /// Last decision in words; a new one is journaled when it changes.
    pub last_decision: String,
    /// Consecutive reviews with a crop shortage and no fallow plot to sow.
    #[serde(default)]
    pub field_reviews: u32,
    /// Consecutive reviews with a grown child waiting for a workplace seat.
    #[serde(default)]
    pub seat_reviews: u32,
    /// Estimated cost of the project the planner wants and cannot pay yet.
    #[serde(default)]
    pub pending_cost: u32,
    /// Residents and children living in a home with too few beds.
    #[serde(default)]
    pub crowded: u32,
    /// The last decision as a sentence for players, with the building's
    /// display name and the reason.
    #[serde(default)]
    pub last_decision_text: String,
}

/// Index of civic entities by settlement and the planners' memory. Lists are
/// kept sorted by `SimId`, so nothing here scans the world to find a store.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct CivicState {
    /// Open construction sites by settlement.
    pub sites: BTreeMap<SimId, Vec<SimId>>,
    /// Finished buildings of every kind by settlement.
    pub buildings: BTreeMap<SimId, Vec<SimId>>,
    /// Finished stores and stalls by settlement.
    pub depots: BTreeMap<SimId, Vec<SimId>>,
    pub planners: BTreeMap<SimId, PlannerState>,
    pub completed: u64,
    pub cancelled: u64,
    /// Coins the fund has paid for building materials so far.
    pub spent: u64,
    /// Destitution grants paid above the levy on each of the last levy days,
    /// oldest first; the dynamic part of the civic reserve follows them.
    #[serde(default)]
    pub grant_history: Vec<u32>,
    /// Coins of the fund raised by the building levy and not yet spent: they
    /// pay for building materials whatever the reserve says.
    #[serde(default)]
    pub earmarked: u32,
    /// Coins the building levy has raised so far.
    #[serde(default)]
    pub building_levied: u64,
    /// Buildings the simulation laid out with the founding (fields, the
    /// trading post), ascending: a world export leaves them out.
    #[serde(default)]
    pub founded: Vec<SimId>,
}
fn hash_index(h: &mut Fnv64, index: &BTreeMap<SimId, Vec<SimId>>) {
    h.u32(index.len() as u32);
    for (settlement, ids) in index {
        h.u32(settlement.0);
        h.u32(ids.len() as u32);
        for id in ids {
            h.u32(id.0);
        }
    }
}
impl SimHashResource for CivicState {
    const NAME: &'static str = "regional_civic_state_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        hash_index(h, &self.sites);
        hash_index(h, &self.buildings);
        hash_index(h, &self.depots);
        h.u32(self.planners.len() as u32);
        for (settlement, planner) in &self.planners {
            h.u32(settlement.0);
            h.u32(planner.bed_shortage_reviews);
            h.u32(planner.overflow_reviews);
            h.u32(planner.demand_reviews.len() as u32);
            for (role, reviews) in &planner.demand_reviews {
                role.sim_hash(h);
                h.u32(*reviews);
            }
            h.u32(planner.projects.len() as u32);
            for id in &planner.projects {
                h.u32(id.0);
            }
            h.u64(planner.last_review);
            h.u32(planner.homeless);
            h.u32(planner.free_beds);
            h.u32(planner.surplus_units);
            h.str(&planner.last_decision);
            h.u32(planner.field_reviews);
            h.u32(planner.seat_reviews);
            h.u32(planner.pending_cost);
            h.u32(planner.crowded);
            h.str(&planner.last_decision_text);
        }
        h.u64(self.completed);
        h.u64(self.cancelled);
        h.u64(self.spent);
        h.u32(self.grant_history.len() as u32);
        for grant in &self.grant_history {
            h.u32(*grant);
        }
        h.u32(self.earmarked);
        h.u64(self.building_levied);
        h.u32(self.founded.len() as u32);
        for id in &self.founded {
            h.u32(id.0);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}
