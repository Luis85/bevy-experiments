use super::*;

pub const MAX_COMPANY_DRIVERS: usize = 4;
pub const MAX_COMPANY_ROUTES: usize = 8;
pub const MAX_COMPANY_ROUTE_STOPS: usize = 8;
pub const COMPANY_REGISTRATION_PRICE: u64 = 2_000;
pub const COMPANY_HIRE_PRICE: u64 = 1_000;
pub const COMPANY_DEPOT_PRICE: u64 = 2_500;
pub const COMPANY_DEPOT_CAPACITY: u64 = 2_000;
pub const MAX_COMPANY_DEPOTS: usize = 2;
pub const COMPANY_ROADSIDE_CALLOUT_PRICE: u64 = 1_500;
pub const COMPANY_ROADSIDE_FUEL_PRICE_PER_LITRE: u64 = 150;
/// Minor credits per physical second while dispatched, including road delays.
pub const COMPANY_WAGE_PER_SECOND: f64 = 2.;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FleetService {
    /// Each leg transfers real waiting passengers; queues never replenish here.
    Passengers { count: u16 },
    /// Load at the first stop, deliver at the last, then return empty. Middle
    /// stops are physical waypoints. Destination demand bounds every load.
    Freight { item: String, amount: u32 },
    /// Load actual company-owned depot stock for a buying public shop. The
    /// buyer reserves purchase money until physical delivery; no public fare.
    DepotFreight { item: String, amount: u32 },
    /// Move owned goods between owned bays, paying payroll but earning no fare.
    DepotTransfer { item: String, amount: u32 },
}

impl FleetService {
    pub fn cargo_load(&self) -> Option<(&str, u32)> {
        match self {
            Self::Freight { item, amount }
            | Self::DepotFreight { item, amount }
            | Self::DepotTransfer { item, amount } => Some((item, *amount)),
            Self::Passengers { .. } => None,
        }
    }

    pub fn uses_company_stock(&self) -> bool {
        matches!(self, Self::DepotFreight { .. } | Self::DepotTransfer { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyRoute {
    pub id: u64,
    pub name: String,
    pub stops: Vec<u64>,
    pub service: FleetService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetStatus {
    Unassigned,
    Paused,
    Held,
    Driving,
    Delivering,
    WaitingPassengers,
    WaitingGoods,
    WaitingDemand,
    WaitingIssuerFunds,
    WaitingBuyerFunds,
    WaitingCompanyFunds,
    AccountingFull,
    NeedsFuel,
    NeedsRepair,
    RentalExpired,
    DestinationFull,
    RouteBlocked,
}
impl FleetStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unassigned => "Choose a vehicle and route",
            Self::Paused => "Parked — route paused",
            Self::Held => "Held in place — load and payroll paused",
            Self::Driving => "Driving to stop",
            Self::Delivering => "Finishing loaded delivery",
            Self::WaitingPassengers => "Waiting for passengers",
            Self::WaitingGoods => "Waiting for source goods",
            Self::WaitingDemand => "Destination has enough stock",
            Self::WaitingIssuerFunds => "Service issuer needs funds",
            Self::WaitingBuyerFunds => "Destination shop needs purchase funds",
            Self::WaitingCompanyFunds => "Deposit company funds for payroll",
            Self::AccountingFull => "Company accounting limit reached",
            Self::NeedsFuel => "Refuel the fleet vehicle",
            Self::NeedsRepair => "Repair the fleet vehicle",
            Self::RentalExpired => "Rental expired — renew in Garage",
            Self::DestinationFull => "Delivery blocked — destination full",
            Self::RouteBlocked => "Route blocked — clear the path, then resume",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetTrip {
    pub pickup: u64,
    pub destination: u64,
    pub service: FleetService,
    pub reward_escrow: u64,
    pub pickup_odometer_m: f64,
    pub pickup_elapsed_seconds: f64,
    /// Cancellation is physical: cargo/passengers go back to their pickup.
    #[serde(default)]
    pub returning: bool,
    /// Travel already admitted before a nearby physical vehicle handover.
    #[serde(default)]
    pub transferred_distance_m: f64,
    /// A shop-funded purchase of company warehouse goods, fixed when loaded.
    /// This is escrow, not another spendable shop or company wallet.
    #[serde(default)]
    pub goods_payment: Option<DepotGoodsPayment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepotGoodsPayment {
    pub shop_id: u64,
    pub unit_price: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyDriver {
    pub id: u64,
    pub name: String,
    pub vehicle_id: Option<u64>,
    pub route_id: Option<u64>,
    /// Index of the stop currently approached. Kept across pause/resume.
    pub next_stop: usize,
    pub running: bool,
    pub status: FleetStatus,
    pub trip: Option<FleetTrip>,
    /// A finite NPC wallet: payroll transfers company cash into this ledger.
    pub wages_earned: u64,
    /// Earned money spent by an off-duty household; historical, not a wallet.
    #[serde(default)]
    pub wages_spent: u64,
    /// Fractional minor-credit liability survives assignment and save/load.
    pub wage_fraction: f64,
    #[serde(default)]
    pub stalled_seconds: f64,
    pub deliveries_completed: u64,
}

/// An owned storage bay at an existing garage or freight facility. It is private
/// gameplay storage, separate from the site's public finite fuel and inventory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyDepot {
    pub facility_id: u64,
    pub stock: Inventory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportCompany {
    pub owner: u64,
    pub name: String,
    pub cash: u64,
    /// Absent in legacy saves. The first new event opens an explicit imported
    /// balance without inventing the missing historical expenses or transfers.
    #[serde(default)]
    pub accounting: Option<CompanyAccounting>,
    pub drivers: Vec<CompanyDriver>,
    pub routes: Vec<CompanyRoute>,
    /// Cumulative paid fares and goods sales, not another spendable account.
    pub revenue: u64,
    pub next_id: u64,
    #[serde(default)]
    pub depots: Vec<CompanyDepot>,
    /// Dismissed employees retain their earned money in this aggregate wallet.
    #[serde(default)]
    pub departed_wages: u64,
    #[serde(default)]
    pub departed_spending: u64,
    /// Material consumed by paid employee households, not available inventory.
    #[serde(default)]
    pub household_consumed: Inventory,
    #[serde(default)]
    pub household_elapsed: f64,
}

/// Read-only service offer. Authority recomputes it at command acceptance; a
/// client-provided quoted total is a spending expectation, never a price setter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanyRoadsideQuote {
    pub vehicle_id: u64,
    pub facility_id: u64,
    pub litres: u16,
    pub repair: bool,
    pub callout_cost: u64,
    pub fuel_cost: u64,
    pub repair_cost: u64,
    pub total_cost: u64,
}

/// Actual nearby shop quote against the company's private warehouse and cash.
/// The authority rechecks price, reach, stock and reserved storage on acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyDepotTradeQuote {
    pub facility_id: u64,
    pub shop_id: u64,
    pub item: String,
    pub amount: u32,
    pub buy: bool,
    pub unit_price: u64,
    pub total_cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompanyCommand {
    Create {
        name: String,
    },
    Deposit {
        amount: u64,
    },
    Withdraw {
        amount: u64,
    },
    FundPublicServices {
        amount: u64,
    },
    HireDriver {
        name: String,
    },
    DismissDriver {
        driver_id: u64,
    },
    ReturnLoad {
        driver_id: u64,
    },
    HoldVehicle {
        driver_id: u64,
    },
    HandoverLoad {
        driver_id: u64,
        relief_driver_id: u64,
        vehicle_id: u64,
    },
    OpenDepot {
        facility_id: u64,
    },
    CloseDepot {
        facility_id: u64,
    },
    TransferDepotStock {
        facility_id: u64,
        item: String,
        amount: u32,
        deposit: bool,
    },
    TradeDepotStock {
        facility_id: u64,
        shop_id: u64,
        item: String,
        amount: u32,
        buy: bool,
        quoted_total: u64,
    },
    CreateRoute {
        name: String,
        stops: Vec<u64>,
        service: FleetService,
    },
    DeleteRoute {
        route_id: u64,
    },
    Assign {
        driver_id: u64,
        vehicle_id: u64,
        route_id: u64,
    },
    SetDriverRunning {
        driver_id: u64,
        running: bool,
    },
    ReleaseVehicle {
        driver_id: u64,
    },
    RoadsideService {
        vehicle_id: u64,
        facility_id: u64,
        litres: u16,
        repair: bool,
        quoted_total: u64,
    },
}

impl TransportCompany {
    pub fn payroll_paid(&self) -> u64 {
        self.departed_wages
            + self.departed_spending
            + self
                .drivers
                .iter()
                .map(|driver| driver.wages_earned + driver.wages_spent)
                .sum::<u64>()
    }
}
