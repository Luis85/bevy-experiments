use super::*;

pub const MAX_SETTLEMENTS: usize = 32;
pub const MAX_SUPPLY_CONTRACTS: usize = 16;
pub const MAX_ACTIVE_CONTRACTS: usize = 3;
pub const MAX_SETTLEMENT_STOCK: u64 = 4000;
pub const MAX_EXPERIENCE: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementTier {
    Neighborhood,
    Village,
    City,
}
impl SettlementTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Neighborhood => "Neighborhood",
            Self::Village => "Village",
            Self::City => "City",
        }
    }
    pub fn radius_m(self) -> f64 {
        match self {
            Self::Neighborhood => 100.,
            Self::Village => 300.,
            Self::City => 700.,
        }
    }
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Neighborhood => Some(Self::Village),
            Self::Village => Some(Self::City),
            Self::City => None,
        }
    }
    pub fn plot_capacity(self) -> usize {
        match self {
            Self::Neighborhood => 6,
            Self::Village => 24,
            Self::City => 64,
        }
    }
    /// Costs to achieve this tier; paid materials are consumed by public works.
    pub fn material_cost(self) -> Inventory {
        match self {
            Self::Neighborhood => inventory(&[("wood", 20), ("stone", 10)]),
            Self::Village => inventory(&[("planks", 30), ("stone_blocks", 10)]),
            Self::City => inventory(&[("planks", 120), ("stone_blocks", 50), ("tool", 8)]),
        }
    }
    /// Founding deposits a treasury. Upgrades pay the charter service to a market.
    pub fn credit_cost(self) -> u64 {
        match self {
            Self::Neighborhood => 5000,
            Self::Village => 2500,
            Self::City => 15000,
        }
    }
    pub fn experience_required(self) -> u64 {
        match self {
            Self::Neighborhood => 0,
            Self::Village => 50,
            Self::City => 250,
        }
    }
    pub fn houses_required(self) -> usize {
        match self {
            Self::Neighborhood => 0,
            Self::Village => 2,
            Self::City => 6,
        }
    }
    pub fn farms_required(self) -> usize {
        match self {
            Self::City => 3,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settlement {
    pub id: u64,
    pub name: String,
    /// Audit identity; faction-owned territory never grants this identity a bypass.
    pub founder: u64,
    pub faction_owner: Option<u64>,
    pub center: GeoPosition,
    pub tier: SettlementTier,
    pub stock: Inventory,
    pub treasury: u64,
}
impl Settlement {
    pub fn contains(&self, position: GeoPosition, radius_m: f64) -> bool {
        self.center.surface_distance_m(position) + radius_m <= self.tier.radius_m()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerProgress {
    pub player_id: u64,
    pub experience: u64,
    pub contracts_completed: u64,
    pub settlements_founded: u64,
}
impl PlayerProgress {
    pub fn rank(&self) -> &'static str {
        match self.experience {
            0..50 => "Newcomer",
            50..250 => "Contributor",
            250..1000 => "Steward",
            _ => "Pathfinder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplyContract {
    pub id: u64,
    pub issuer_shop: u64,
    /// Unique living ID of a producer or shop receiving the stock.
    pub target_site: u64,
    pub target_position: GeoPosition,
    pub item: String,
    pub amount: u32,
    /// Money removed from the issuer at posting and held in this contract's escrow.
    pub reward: u64,
    pub experience: u64,
    pub claimed_by: Option<u64>,
    /// Deadline in SettlementWorld.elapsed_seconds; accepting refreshes it.
    pub expires_at: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementWorld {
    pub territories: BTreeMap<u64, Settlement>,
    pub contracts: BTreeMap<u64, SupplyContract>,
    pub progress: BTreeMap<u64, PlayerProgress>,
    pub next_id: u64,
    pub elapsed_seconds: f64,
    pub update_accumulator: f64,
    pub wage_accumulator: f64,
    pub contract_duration_seconds: f64,
    /// Immutable price anchors avoid compounding dynamic prices every update.
    pub reference_prices: BTreeMap<u64, BTreeMap<String, TradePrice>>,
    pub last_paid_cycles: u64,
    pub wages_paid: u64,
}
impl Default for SettlementWorld {
    fn default() -> Self {
        Self {
            territories: BTreeMap::new(),
            contracts: BTreeMap::new(),
            progress: BTreeMap::new(),
            next_id: 2_000_000,
            elapsed_seconds: 0.,
            update_accumulator: 60.,
            wage_accumulator: 0.,
            reference_prices: BTreeMap::new(),
            last_paid_cycles: 0,
            wages_paid: 0,
            contract_duration_seconds: 259_200.,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettlementView {
    pub territories: Vec<Settlement>,
    /// Open offers and the recipient's accepted work only.
    pub contracts: Vec<SupplyContract>,
    pub progress: Option<PlayerProgress>,
    pub wages_paid: u64,
    pub elapsed_seconds: f64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettlementCommand {
    Found {
        name: String,
        center: GeoPosition,
        faction_owner: Option<u64>,
    },
    Contribute {
        settlement_id: u64,
        item: String,
        amount: u32,
    },
    Fund {
        settlement_id: u64,
        amount: u64,
    },
    Upgrade {
        settlement_id: u64,
    },
    AcceptContract {
        contract_id: u64,
    },
    FulfillContract {
        contract_id: u64,
    },
    CancelContract {
        contract_id: u64,
    },
}

impl SettlementWorld {
    pub fn snapshot(&self, player: u64, position: GeoPosition, radius: f64) -> SettlementView {
        let territories: Vec<_> = self
            .territories
            .values()
            .filter(|t| {
                t.founder == player
                    || t.center.surface_distance_m(position) <= radius + t.tier.radius_m()
            })
            .take(MAX_SETTLEMENTS)
            .cloned()
            .collect();
        let contracts = self
            .contracts
            .values()
            .filter(|c| c.claimed_by.is_none() || c.claimed_by == Some(player))
            .cloned()
            .collect();
        SettlementView {
            territories,
            contracts,
            progress: self.progress.get(&player).cloned(),
            wages_paid: self.wages_paid,
            elapsed_seconds: self.elapsed_seconds,
            truncated: false,
        }
    }
    pub fn from_view(view: SettlementView) -> Self {
        let mut state = Self::default();
        state.territories = view.territories.into_iter().map(|t| (t.id, t)).collect();
        state.contracts = view.contracts.into_iter().map(|c| (c.id, c)).collect();
        if let Some(progress) = view.progress {
            state.progress.insert(progress.player_id, progress);
        }
        state.wages_paid = view.wages_paid;
        state.elapsed_seconds = view.elapsed_seconds;
        state
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementUpgradeCost {
    pub tier: SettlementTier,
    pub materials: Inventory,
    pub credits: u64,
    pub experience: u64,
    pub houses: usize,
    pub farms: usize,
}
impl SettlementTier {
    pub fn upgrade_cost(self) -> Option<SettlementUpgradeCost> {
        self.next().map(|tier| SettlementUpgradeCost {
            tier,
            materials: tier.material_cost(),
            credits: tier.credit_cost(),
            experience: tier.experience_required(),
            houses: tier.houses_required(),
            farms: tier.farms_required(),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementReadiness {
    pub next: Option<SettlementUpgradeCost>,
    pub houses: usize,
    pub farms: usize,
    pub plots: usize,
    pub experience: u64,
}
