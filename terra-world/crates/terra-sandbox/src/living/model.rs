use super::*;

pub const MAX_LIVING_UNITS: usize = 512;
/// Avatar, two crew members, utility rover and survey helicopter per principal.
pub const STARTER_UNIT_COUNT: usize = 5;
pub const MAX_LIVING_SITES: usize = 32;
pub const MAX_LIVING_ROADS: usize = 32;
pub const MAX_MONEY: u64 = 1_000_000_000;
pub const MAX_GROUP_ORDER: usize = 32;
pub const LIVING_INTERACTION_REACH_M: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Needs {
    /// Satisfaction: 1 is fed/hydrated/rested; 0 is depleted.
    pub food: f32,
    pub water: f32,
    pub energy: f32,
}
impl Default for Needs {
    fn default() -> Self {
        Self {
            food: 1.,
            water: 1.,
            energy: 1.,
        }
    }
}
impl Needs {
    pub fn speed_factor(self) -> f64 {
        (0.35 + 0.65 * f64::from(self.energy.min(self.water).min(self.food))).clamp(0.35, 1.)
    }
    pub(super) fn validate(self) -> bool {
        [self.food, self.water, self.energy]
            .iter()
            .all(|n| n.is_finite() && (0.0..=1.0).contains(n))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitRole {
    Player,
    Worker,
    Citizen,
    Hauler,
    Traffic,
    Animal,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum UnitOrder {
    Idle,
    Move { destination: GeoPosition },
    Rest,
    Patrol,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivingUnit {
    pub id: u64,
    pub name: String,
    pub kind: ActorKind,
    pub owner: Option<u64>,
    pub role: UnitRole,
    pub position: GeoPosition,
    pub heading_deg: f32,
    pub velocity_enu_m_s: [f64; 3],
    pub needs: Needs,
    pub order: UnitOrder,
    pub home: GeoPosition,
    /// Current road/foot route. It is a bounded path, not a navigation mesh.
    pub path: Vec<GeoPosition>,
    pub path_index: usize,
    pub patrol: Vec<GeoPosition>,
    pub patrol_reverse: bool,
    pub pocket_money: u64,
    pub supplies: Inventory,
}
impl LivingUnit {
    /// Hard client-intent cap, independent of delayed needs snapshots.
    pub fn base_speed_limit_m_s(&self) -> f64 {
        match self.kind {
            ActorKind::Human => 7.,
            ActorKind::Animal => 6.,
            ActorKind::Creature => 8.,
            ActorKind::Vehicle => 14.,
            ActorKind::Ship => 10.,
            ActorKind::Helicopter | ActorKind::Balloon | ActorKind::Airplane => 20.,
        }
    }
    pub fn speed_limit_m_s(&self) -> f64 {
        self.base_speed_limit_m_s()
            * if matches!(
                self.kind,
                ActorKind::Human | ActorKind::Animal | ActorKind::Creature
            ) {
                self.needs.speed_factor()
            } else {
                1.
            }
    }
    pub fn is_person(&self) -> bool {
        matches!(
            self.kind,
            ActorKind::Human | ActorKind::Creature | ActorKind::Animal
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivingAccount {
    pub player_id: u64,
    /// Integer minor credits: 100 == 1.00 credit.
    pub money: u64,
    pub avatar_unit: u64,
    pub active_unit: u64,
    pub possessed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradePrice {
    /// Price the customer pays.
    pub buy: u64,
    /// Price the shop pays the customer.
    pub sell: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shop {
    pub id: u64,
    pub name: String,
    pub position: GeoPosition,
    pub stock: Inventory,
    pub prices: BTreeMap<String, TradePrice>,
    pub money: u64,
    pub source_feature: Option<String>,
    pub authored: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionKind {
    Farm,
    Well,
    Woodlot,
    Sawmill,
    Mill,
    Bakery,
    SeedHouse,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionSite {
    pub id: u64,
    pub name: String,
    pub kind: ProductionKind,
    pub position: GeoPosition,
    pub stock: Inventory,
    pub inputs: Inventory,
    pub outputs: Inventory,
    pub cycle_seconds: f64,
    pub progress_seconds: f64,
    pub enabled: bool,
    pub owner: Option<u64>,
    pub source_feature: Option<String>,
    pub authored: bool,
    /// Finite remaining extraction cycles for wells/woodlots; None for conversion recipes.
    pub resource_remaining: Option<u32>,
    pub cycles_completed: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadRoute {
    pub id: String,
    pub points: Vec<GeoPosition>,
    pub width_m: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPhase {
    ToPickup,
    ToDropoff,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: u64,
    pub unit_id: u64,
    pub from_site: u64,
    /// Destination IDs are unique across sites and shops.
    pub to_site: u64,
    pub item: String,
    pub amount: u32,
    pub phase: DeliveryPhase,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LivingMetrics {
    pub production_cycles: u64,
    pub deliveries_completed: u64,
    pub units_traded: u64,
    pub credits_traded: u64,
    pub npc_purchases: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivingWorld {
    pub enabled: bool,
    pub origin: Option<GeoPosition>,
    pub units: BTreeMap<u64, LivingUnit>,
    pub accounts: BTreeMap<u64, LivingAccount>,
    pub shops: BTreeMap<u64, Shop>,
    pub producers: BTreeMap<u64, ProductionSite>,
    pub deliveries: Vec<Delivery>,
    pub roads: Vec<RoadRoute>,
    pub elapsed_seconds: f64,
    pub dispatch_accumulator: f64,
    pub next_id: u64,
    pub metrics: LivingMetrics,
}
impl Default for LivingWorld {
    fn default() -> Self {
        Self {
            enabled: false,
            origin: None,
            units: BTreeMap::new(),
            accounts: BTreeMap::new(),
            shops: BTreeMap::new(),
            producers: BTreeMap::new(),
            deliveries: vec![],
            roads: vec![],
            elapsed_seconds: 0.,
            dispatch_accumulator: 0.,
            next_id: 1_000_000,
            metrics: LivingMetrics::default(),
        }
    }
}

/// Bounded public replication. Other owners' wallets are never included.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LivingView {
    pub enabled: bool,
    pub origin: Option<GeoPosition>,
    pub account: Option<LivingAccount>,
    pub units: Vec<LivingUnit>,
    pub shops: Vec<Shop>,
    pub producers: Vec<ProductionSite>,
    pub deliveries: Vec<Delivery>,
    pub roads: Vec<RoadRoute>,
    pub elapsed_seconds: f64,
    pub metrics: LivingMetrics,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LivingCommand {
    Possess {
        unit_id: u64,
    },
    /// Select the principal's interaction body in RTS mode without cancelling its order.
    SelectActive {
        unit_id: u64,
    },
    Release,
    MoveUnits {
        unit_ids: Vec<u64>,
        destination: GeoPosition,
    },
    StopUnits {
        unit_ids: Vec<u64>,
    },
    Rest {
        unit_ids: Vec<u64>,
    },
    Buy {
        shop_id: u64,
        item: String,
        amount: u32,
    },
    Sell {
        shop_id: u64,
        item: String,
        amount: u32,
    },
    Consume {
        item: String,
    },
    SetProduction {
        site_id: u64,
        enabled: bool,
    },
}

impl LivingWorld {
    pub fn active_unit(&self, player: u64) -> Option<&LivingUnit> {
        self.accounts
            .get(&player)
            .and_then(|a| self.units.get(&a.active_unit))
    }

    pub fn nearest_shop(&self, position: GeoPosition) -> Option<&Shop> {
        self.shops.values().min_by(|a, b| {
            position
                .surface_distance_m(a.position)
                .total_cmp(&position.surface_distance_m(b.position))
                .then(a.id.cmp(&b.id))
        })
    }

    pub(super) fn next(&mut self) -> Result<u64, SandboxError> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(SandboxError::Capacity)?;
        Ok(id)
    }

    pub(super) fn make_unit(
        &mut self,
        name: String,
        kind: ActorKind,
        role: UnitRole,
        owner: Option<u64>,
        position: GeoPosition,
    ) -> Result<u64, SandboxError> {
        if self.units.len() >= MAX_LIVING_UNITS {
            return Err(SandboxError::Capacity);
        }
        let id = self.next()?;
        self.units.insert(
            id,
            LivingUnit {
                id,
                name,
                kind,
                owner,
                role,
                position,
                heading_deg: 0.,
                velocity_enu_m_s: [0.; 3],
                needs: Needs::default(),
                order: UnitOrder::Idle,
                home: position,
                path: vec![],
                path_index: 0,
                patrol: vec![],
                patrol_reverse: false,
                pocket_money: if owner.is_none() && kind == ActorKind::Human {
                    10_000
                } else {
                    0
                },
                supplies: Inventory::new(),
            },
        );
        Ok(id)
    }
}

impl Shop {
    /// Keep an accessible service point south of the authored building.
    pub fn model_position(&self) -> GeoPosition {
        LocalFrame::new(self.position)
            .to_geo([0., 0., -6.])
            .unwrap_or(self.position)
    }
    pub fn collision_radius_m(&self) -> f64 {
        2.5
    }
}
impl ProductionSite {
    /// Service point and unloading forecourt remain outside the building.
    pub fn model_position(&self) -> GeoPosition {
        LocalFrame::new(self.position)
            .to_geo([0., 0., -12.])
            .unwrap_or(self.position)
    }
    pub fn collision_radius_m(&self) -> f64 {
        match self.kind {
            ProductionKind::Farm => 7.5,
            ProductionKind::Well => 1.3,
            ProductionKind::Woodlot => 7.,
            _ => 6.8,
        }
    }
}
impl LivingWorld {
    /// Imported features use their provider's true footprint; only fallback
    /// facilities contribute these authored collision circles.
    pub fn facility_footprints(&self) -> Vec<(GeoPosition, f64)> {
        self.shops
            .values()
            .filter(|s| s.source_feature.is_none())
            .map(|s| (s.model_position(), s.collision_radius_m()))
            .chain(
                self.producers
                    .values()
                    .filter(|s| s.source_feature.is_none())
                    .map(|s| (s.model_position(), s.collision_radius_m())),
            )
            .collect()
    }
}

/// Conservative swept footprint check for ordinary ground actors. A body that
/// starts inside can escape outward; flight is handled by the host's altitude rules.
pub fn facility_path_clear(
    kind: ActorKind,
    from: GeoPosition,
    to: GeoPosition,
    footprints: &[(GeoPosition, f64)],
) -> bool {
    let airborne = matches!(
        kind,
        ActorKind::Helicopter | ActorKind::Balloon | ActorKind::Airplane
    );
    let body = if matches!(kind, ActorKind::Vehicle | ActorKind::Ship) {
        1.2
    } else {
        0.35
    };
    footprints.iter().all(|(center, radius)| {
        if airborne && from.altitude_m.min(to.altitude_m) > center.altitude_m + 12. {
            return true;
        }
        let frame = LocalFrame::new(*center);
        let a = frame.to_local(from);
        let b = frame.to_local(to);
        let radius = radius + body;
        let start = a[0].hypot(a[2]);
        let end = b[0].hypot(b[2]);
        if start < radius {
            return end > start + 0.001
                || (end >= start - 0.001 && to.altitude_m >= from.altitude_m - 0.001);
        }
        let dx = b[0] - a[0];
        let dz = b[2] - a[2];
        let denominator = dx * dx + dz * dz;
        let t = if denominator > 1e-12 {
            (-(a[0] * dx + a[2] * dz) / denominator).clamp(0., 1.)
        } else {
            0.
        };
        (a[0] + t * dx).hypot(a[2] + t * dz) >= radius
    })
}

pub(super) fn validate_living_position(position: GeoPosition) -> Result<(), SandboxError> {
    position.validate()?;
    if !finite_range(position.altitude_m, -12_000., 100_000.) {
        return Err(SandboxError::Invalid(
            "living position altitude must be -12000..100000m".into(),
        ));
    }
    Ok(())
}
