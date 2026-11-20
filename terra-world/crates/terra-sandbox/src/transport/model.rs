use super::*;

pub const MAX_TRANSPORT_VEHICLES: usize = 128;
pub const MAX_PLAYER_VEHICLES: usize = 12;
pub const MAX_TRANSPORT_FACILITIES: usize = 24;
pub const MAX_TRANSPORT_JOBS: usize = 32;
pub const MAX_TRANSPORT_TRAILERS: usize = 32;
pub const MAX_PLAYER_JOBS: usize = 2;
pub const MAX_RACE_COURSES: usize = 8;

/// Cosmetic identity. Indices select a game-owned bounded visual palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterProfile {
    pub name: String,
    pub body_style: u8,
    pub skin: u8,
    pub hair: u8,
    pub outfit: u8,
}
impl Default for CharacterProfile {
    fn default() -> Self {
        Self {
            name: "Explorer".into(),
            body_style: 0,
            skin: 2,
            hair: 1,
            outfit: 0,
        }
    }
}
impl CharacterProfile {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.name.trim() != self.name
            || self.name.is_empty()
            || self.name.len() > 64
            || self.name.chars().count() > 32
            || self.name.chars().any(char::is_control)
            || self.body_style > 2
            || self.skin > 7
            || self.hair > 7
            || self.outfit > 7
        {
            return Err(invalid("character name or appearance"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverLicense {
    Basic,
    Passenger,
    Heavy,
    Emergency,
}
impl DriverLicense {
    pub fn name(self) -> &'static str {
        match self {
            Self::Basic => "Road",
            Self::Passenger => "Passenger",
            Self::Heavy => "Heavy goods",
            Self::Emergency => "Emergency service",
        }
    }
    pub fn cost(self) -> u64 {
        match self {
            Self::Basic => 0,
            Self::Passenger => 5_000,
            Self::Heavy => 10_000,
            Self::Emergency => 12_000,
        }
    }
    pub fn required_jobs(self) -> u32 {
        match self {
            Self::Basic => 0,
            Self::Passenger => 1,
            Self::Heavy => 3,
            Self::Emergency => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleModel {
    Compact,
    Pickup,
    Taxi,
    Bus,
    BoxTruck,
    Semi,
    TowTruck,
    Ambulance,
    FireEngine,
}
pub const VEHICLE_MODELS: [VehicleModel; 9] = [
    VehicleModel::Compact,
    VehicleModel::Pickup,
    VehicleModel::Taxi,
    VehicleModel::Bus,
    VehicleModel::BoxTruck,
    VehicleModel::Semi,
    VehicleModel::TowTruck,
    VehicleModel::Ambulance,
    VehicleModel::FireEngine,
];

#[derive(Debug, Clone, Copy)]
pub struct VehicleSpec {
    pub name: &'static str,
    pub price: u64,
    pub max_speed_m_s: f64,
    pub reverse_speed_m_s: f64,
    pub mass_kg: f64,
    pub wheelbase_m: f64,
    pub engine_power_kw: f64,
    pub brake_force_n: f64,
    pub grip: f64,
    pub fuel_capacity_l: f64,
    pub fuel_l_per_100km: f64,
    pub cargo_capacity: u32,
    pub passenger_seats: u16,
    pub required_license: DriverLicense,
}
impl VehicleModel {
    /// Width, height and total length of the original reference model, in
    /// metres. Shared by visual geometry and local/server swept footprints.
    pub fn dimensions_m(self) -> [f64; 3] {
        match self {
            Self::Compact => [1.9, 1.65, 4.2],
            Self::Pickup => [2.05, 1.95, 5.1],
            Self::Taxi => [2.0, 1.8, 4.8],
            Self::Bus => [2.65, 3.25, 10.8],
            Self::BoxTruck => [2.55, 3.55, 7.2],
            Self::Semi => [2.6, 3.6, 6.7],
            Self::TowTruck => [2.45, 2.85, 6.6],
            Self::Ambulance => [2.35, 2.9, 6.0],
            Self::FireEngine => [2.65, 3.5, 8.2],
        }
    }

    pub fn spec(self) -> VehicleSpec {
        let (name, price, speed, mass, wheelbase, power, tank, consumption, cargo, seats, license) =
            match self {
                Self::Compact => (
                    "Comet hatchback",
                    18_000,
                    32.,
                    1100.,
                    2.55,
                    75.,
                    45.,
                    6.5,
                    8,
                    3,
                    DriverLicense::Basic,
                ),
                Self::Pickup => (
                    "Trail utility pickup",
                    24_000,
                    30.,
                    2100.,
                    3.1,
                    120.,
                    75.,
                    11.,
                    40,
                    3,
                    DriverLicense::Basic,
                ),
                Self::Taxi => (
                    "Metro taxi",
                    28_000,
                    32.,
                    1550.,
                    2.85,
                    105.,
                    60.,
                    8.,
                    12,
                    4,
                    DriverLicense::Passenger,
                ),
                Self::Bus => (
                    "Civic city bus",
                    70_000,
                    22.,
                    9500.,
                    5.8,
                    220.,
                    220.,
                    28.,
                    10,
                    30,
                    DriverLicense::Passenger,
                ),
                Self::BoxTruck => (
                    "Cargo box truck",
                    45_000,
                    25.,
                    5000.,
                    4.3,
                    160.,
                    120.,
                    18.,
                    120,
                    1,
                    DriverLicense::Heavy,
                ),
                Self::Semi => (
                    "Atlas tractor",
                    85_000,
                    25.,
                    8000.,
                    4.0,
                    300.,
                    400.,
                    30.,
                    0,
                    1,
                    DriverLicense::Heavy,
                ),
                Self::TowTruck => (
                    "Recovery tow truck",
                    55_000,
                    25.,
                    4500.,
                    3.8,
                    170.,
                    110.,
                    17.,
                    20,
                    2,
                    DriverLicense::Heavy,
                ),
                Self::Ambulance => (
                    "Rescue ambulance",
                    60_000,
                    30.,
                    3400.,
                    3.7,
                    160.,
                    90.,
                    14.,
                    12,
                    2,
                    DriverLicense::Emergency,
                ),
                Self::FireEngine => (
                    "Brigade fire engine",
                    95_000,
                    23.,
                    12000.,
                    4.8,
                    260.,
                    250.,
                    32.,
                    180,
                    4,
                    DriverLicense::Emergency,
                ),
            };
        VehicleSpec {
            name,
            price,
            max_speed_m_s: speed,
            reverse_speed_m_s: 6.,
            mass_kg: mass,
            wheelbase_m: wheelbase,
            engine_power_kw: power,
            brake_force_n: mass * 7.5,
            grip: 0.95,
            fuel_capacity_l: tank,
            fuel_l_per_100km: consumption,
            cargo_capacity: cargo,
            passenger_seats: seats,
            required_license: license,
        }
    }
    pub fn can_tow(self) -> bool {
        matches!(
            self,
            Self::Pickup | Self::Semi | Self::TowTruck | Self::BoxTruck
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnIndicator {
    #[default]
    Off,
    Left,
    Right,
    Hazard,
}

/// Public presentation of a visible road vehicle. Progress, condition, fuel,
/// cargo, account balances and job reservations are deliberately absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleAppearance {
    pub unit_id: u64,
    pub owner: u64,
    pub model: VehicleModel,
    pub paint: u8,
    pub lights: bool,
    pub indicator: TurnIndicator,
    pub steering_rad: f64,
    #[serde(default)]
    pub trailer: Option<u64>,
}
impl From<&VehicleState> for VehicleAppearance {
    fn from(vehicle: &VehicleState) -> Self {
        Self {
            unit_id: vehicle.unit_id,
            owner: vehicle.owner,
            model: vehicle.model,
            paint: vehicle.paint,
            lights: vehicle.lights,
            indicator: vehicle.indicator,
            steering_rad: vehicle.dynamics.steering_rad,
            trailer: vehicle.trailer,
        }
    }
}

/// Finite prepaid use and security bond. A rental remains the owner's controlled
/// gameplay body until an explicit empty return; expiry never teleports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleRental {
    pub facility_id: u64,
    pub bond_escrow: u64,
    pub remaining_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleState {
    pub unit_id: u64,
    pub owner: u64,
    pub model: VehicleModel,
    pub fuel_l: f64,
    pub condition: f32,
    pub odometer_m: f64,
    pub paint: u8,
    pub engine_level: u8,
    pub brakes_level: u8,
    pub lights: bool,
    pub indicator: TurnIndicator,
    pub parking_brake: bool,
    pub autopilot: bool,
    pub trailer: Option<u64>,
    pub cargo: Inventory,
    #[serde(default)]
    pub dynamics: terra_core::VehicleKinematics,
    #[serde(default)]
    pub rental: Option<VehicleRental>,
}
impl VehicleState {
    /// Base model/upgrade configuration with this vehicle's cargo. Authorities
    /// use TransportWorld::vehicle_dynamics_config to add onboard passengers and
    /// the attached trailer's empty mass. Fuel/parking admission is separate.
    pub fn dynamics_config(&self) -> terra_core::VehicleDynamicsConfig {
        let spec = self.model.spec();
        terra_core::VehicleDynamicsConfig {
            mass_kg: spec.mass_kg
                + self
                    .cargo
                    .values()
                    .map(|n| f64::from(*n) * CARGO_UNIT_MASS_KG)
                    .sum::<f64>(),
            wheelbase_m: spec.wheelbase_m,
            engine_power_kw: spec.engine_power_kw * (1. + f64::from(self.engine_level) * 0.15),
            brake_force_n: spec.brake_force_n * (1. + f64::from(self.brakes_level) * 0.15),
            max_speed_m_s: (spec.max_speed_m_s
                * (1. + f64::from(self.engine_level) * 0.06)
                * (0.5 + f64::from(self.condition) * 0.5))
                .min(40.),
            reverse_speed_m_s: spec.reverse_speed_m_s,
            grip: spec.grip,
            ..terra_core::VehicleDynamicsConfig::default()
        }
    }
    pub fn new(unit_id: u64, owner: u64, model: VehicleModel) -> Self {
        Self {
            unit_id,
            owner,
            model,
            fuel_l: model.spec().fuel_capacity_l * 0.65,
            condition: 1.,
            odometer_m: 0.,
            paint: 0,
            engine_level: 0,
            brakes_level: 0,
            lights: false,
            indicator: TurnIndicator::Off,
            parking_brake: false,
            autopilot: false,
            trailer: None,
            cargo: Inventory::new(),
            dynamics: terra_core::VehicleKinematics::default(),
            rental: None,
        }
    }
    pub fn speed_limit_m_s(&self) -> f64 {
        if self.fuel_l <= 0.
            || self.condition <= 0.05
            || self.parking_brake
            || self
                .rental
                .as_ref()
                .is_some_and(|r| r.remaining_seconds <= 0.)
        {
            return 0.;
        }
        (self.model.spec().max_speed_m_s
            * (1. + f64::from(self.engine_level) * 0.06)
            * (0.5 + f64::from(self.condition) * 0.5))
            .min(40.)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriverProgress {
    pub player_id: u64,
    pub licenses: BTreeSet<DriverLicense>,
    pub jobs_completed: u32,
    pub xp: u64,
    pub earnings: u64,
    pub distance_m: f64,
}
impl DriverProgress {
    pub fn new(player_id: u64) -> Self {
        Self {
            player_id,
            licenses: BTreeSet::from([DriverLicense::Basic]),
            jobs_completed: 0,
            xp: 0,
            earnings: 0,
            distance_m: 0.,
        }
    }
    pub fn level(&self) -> u32 {
        (self.xp / 500).min(99) as u32 + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportFacilityKind {
    Garage,
    Terminal,
    Depot,
    Hospital,
    FireStation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportFacility {
    pub id: u64,
    pub name: String,
    pub kind: TransportFacilityKind,
    pub position: GeoPosition,
    /// Existing producer/shop stock is the real cargo source/destination.
    pub linked_site: Option<u64>,
    pub passengers_waiting: u32,
    pub fuel_l: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportJobKind {
    Hitchhiker,
    Taxi,
    Bus,
    Freight,
    Tow,
    Ambulance,
    FireSupply,
}
impl TransportJobKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Hitchhiker => "Passenger lift",
            Self::Taxi => "Taxi fare",
            Self::Bus => "Bus service",
            Self::Freight => "Freight",
            Self::Tow => "Roadside recovery",
            Self::Ambulance => "Medical transport",
            Self::FireSupply => "Fire-service supplies",
        }
    }
    pub fn accepts(self, model: VehicleModel) -> bool {
        match self {
            Self::Hitchhiker => matches!(
                model,
                VehicleModel::Compact | VehicleModel::Pickup | VehicleModel::Taxi
            ),
            Self::Taxi => model == VehicleModel::Taxi,
            Self::Bus => model == VehicleModel::Bus,
            Self::Freight => matches!(
                model,
                VehicleModel::Pickup | VehicleModel::BoxTruck | VehicleModel::Semi
            ),
            Self::Tow => model == VehicleModel::TowTruck,
            Self::Ambulance => model == VehicleModel::Ambulance,
            Self::FireSupply => model == VehicleModel::FireEngine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportJobPhase {
    ToPickup,
    ToDestination,
    Returning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportAssignment {
    pub owner: u64,
    pub vehicle_id: u64,
    pub phase: TransportJobPhase,
    pub reward_escrow: u64,
    pub reserved_cargo: Inventory,
    pub passengers_onboard: u16,
    pub accepted_at: f64,
    pub deadline: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportJob {
    pub id: u64,
    pub name: String,
    pub kind: TransportJobKind,
    pub pickup_facility: u64,
    pub destination_facility: u64,
    pub item: Option<String>,
    pub amount: u32,
    pub passengers: u16,
    pub reward: u64,
    pub time_limit_seconds: f64,
    pub assignment: Option<TransportAssignment>,
    pub completions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportTrailer {
    pub id: u64,
    pub owner: u64,
    pub position: GeoPosition,
    pub capacity: u32,
    pub attached_to: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaceCourse {
    pub id: u64,
    pub name: String,
    pub checkpoints: Vec<GeoPosition>,
    pub radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaceRun {
    pub player_id: u64,
    pub vehicle_id: u64,
    pub course_id: u64,
    pub next_checkpoint: usize,
    pub started_at: f64,
    /// Total path distance when the previous gate was accepted; a static
    /// repeated command cannot consume the same/adjacent checkpoint repeatedly.
    pub last_gate_odometer_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaceBest {
    pub player_id: u64,
    pub course_id: u64,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportWorld {
    pub enabled: bool,
    pub profiles: BTreeMap<u64, CharacterProfile>,
    pub drivers: BTreeMap<u64, DriverProgress>,
    pub vehicles: BTreeMap<u64, VehicleState>,
    /// Client reconstruction cache only. Authoritative appearance is derived
    /// from private vehicle state and this cache is never serialized into saves.
    #[serde(skip)]
    pub visible_vehicles: BTreeMap<u64, VehicleAppearance>,
    pub facilities: BTreeMap<u64, TransportFacility>,
    pub jobs: BTreeMap<u64, TransportJob>,
    pub trailers: BTreeMap<u64, TransportTrailer>,
    pub courses: BTreeMap<u64, RaceCourse>,
    pub races: BTreeMap<u64, RaceRun>,
    pub personal_bests: Vec<RaceBest>,
    #[serde(default)]
    pub companies: BTreeMap<u64, TransportCompany>,
    /// Finite authored service/dealership/job issuer capital, never replicated.
    pub treasury: u64,
    pub elapsed_seconds: f64,
    pub next_id: u64,
}
impl Default for TransportWorld {
    fn default() -> Self {
        Self {
            enabled: false,
            profiles: BTreeMap::new(),
            drivers: BTreeMap::new(),
            vehicles: BTreeMap::new(),
            visible_vehicles: BTreeMap::new(),
            facilities: BTreeMap::new(),
            jobs: BTreeMap::new(),
            trailers: BTreeMap::new(),
            courses: BTreeMap::new(),
            races: BTreeMap::new(),
            personal_bests: Vec::new(),
            companies: BTreeMap::new(),
            treasury: 0,
            elapsed_seconds: 0.,
            next_id: 1,
        }
    }
}

/// Recipient-private progress/jobs with public, bounded facilities and routes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportView {
    pub enabled: bool,
    pub profile: Option<CharacterProfile>,
    pub driver: Option<DriverProgress>,
    pub vehicles: Vec<VehicleState>,
    #[serde(default)]
    pub visible_vehicles: Vec<VehicleAppearance>,
    pub facilities: Vec<TransportFacility>,
    pub jobs: Vec<TransportJob>,
    pub trailers: Vec<TransportTrailer>,
    pub courses: Vec<RaceCourse>,
    pub race: Option<RaceRun>,
    pub personal_bests: Vec<RaceBest>,
    #[serde(default)]
    pub company: Option<TransportCompany>,
    /// Public visible profiles contain cosmetic identity only.
    #[serde(default, deserialize_with = "super::view::deserialize_profiles")]
    pub visible_profiles: BTreeMap<u64, CharacterProfile>,
    pub elapsed_seconds: f64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransportCommand {
    Company {
        action: CompanyCommand,
    },
    SetCharacterProfile {
        profile: CharacterProfile,
    },
    BuyVehicle {
        model: VehicleModel,
        facility_id: u64,
    },
    RentVehicle {
        model: VehicleModel,
        facility_id: u64,
    },
    RenewRental {
        vehicle_id: u64,
    },
    ReturnRental {
        vehicle_id: u64,
        facility_id: u64,
        quoted_refund: u64,
    },
    SellVehicle {
        vehicle_id: u64,
        facility_id: u64,
    },
    EnterVehicle {
        vehicle_id: u64,
    },
    ExitVehicle,
    Refuel {
        vehicle_id: u64,
        facility_id: u64,
        litres: u16,
    },
    Repair {
        vehicle_id: u64,
        facility_id: u64,
    },
    Paint {
        vehicle_id: u64,
        facility_id: u64,
        paint: u8,
    },
    Upgrade {
        vehicle_id: u64,
        facility_id: u64,
        engine: bool,
    },
    BuyLicense {
        license: DriverLicense,
        facility_id: u64,
    },
    SetSignals {
        vehicle_id: u64,
        lights: bool,
        indicator: TurnIndicator,
        parking_brake: bool,
    },
    SetAutopilot {
        vehicle_id: u64,
        enabled: bool,
        destination: Option<GeoPosition>,
    },
    AcceptJob {
        job_id: u64,
        vehicle_id: u64,
    },
    PickupJob {
        job_id: u64,
    },
    CompleteJob {
        job_id: u64,
    },
    CancelJob {
        job_id: u64,
    },
    BuyTrailer {
        facility_id: u64,
    },
    HitchTrailer {
        vehicle_id: u64,
        trailer_id: u64,
    },
    UnhitchTrailer {
        vehicle_id: u64,
    },
    StartRace {
        course_id: u64,
        vehicle_id: u64,
    },
    RaceCheckpoint,
    AbandonRace,
}
