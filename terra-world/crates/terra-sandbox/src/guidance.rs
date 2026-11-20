//! Read-only player goals derived from the same state used by commands.
//!
//! A guide never grants rewards, reserves goods or records a second completion
//! ledger. Recompute it after a command or restore. Actions are suggestions;
//! authority must validate the resulting command again at execution time.
use crate::catalog::exchange;
use crate::*;
use std::cell::OnceCell;
use terra_core::GeoPosition;

mod commitments;
mod company;

pub use commitments::ActivityCommitments;

/// Expensive permission checks inspect only this nearest resource shortlist.
pub const MAX_GUIDE_RESOURCE_CANDIDATES: usize = 128;
const URGENT_NEED_LEVEL: f32 = 0.20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuideTrack {
    #[default]
    Suggested,
    Supply,
    Home,
    Community,
    Road,
    Company,
}

impl GuideTrack {
    pub fn label(self) -> &'static str {
        match self {
            Self::Suggested => "Suggested journey",
            Self::Supply => "Supply work",
            Self::Home => "A place to call home",
            Self::Community => "Grow a community",
            Self::Road => "Make a living on the road",
            Self::Company => "Run your transport company",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Suggested => Self::Supply,
            Self::Supply => Self::Home,
            Self::Home => Self::Community,
            Self::Community => Self::Road,
            Self::Road => Self::Company,
            Self::Company => Self::Suggested,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideSurface {
    Command,
    Nature,
    Build,
    Market,
    Territory,
    Driving,
    Garage,
    Fleet,
}

/// A destination within the company workflow, independent of the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideCompanySection {
    Company,
    Drivers,
    Routes,
    Recovery,
    Depot,
    Finances,
    Garage,
}

/// Retains the service the player actually read. Payroll and vehicle movement
/// are deliberately excluded: those may advance without changing this intent.
/// A new assignment, stop, hold or return must be reviewed again.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideCompanyFocus {
    pub owner: u64,
    pub section: GuideCompanySection,
    pub driver_id: Option<u64>,
    pub vehicle_id: Option<u64>,
    pub route_id: Option<u64>,
    pub facility_id: Option<u64>,
    pub next_stop: Option<usize>,
    pub status: Option<FleetStatus>,
    pub running: bool,
    pub trip: Option<FleetTrip>,
}

#[derive(Debug, Clone)]
pub enum GuideAction {
    /// Submit through the ordinary local/server command queue.
    Command(Command),
    /// Use supported movement/order admission; never teleport a player.
    Travel {
        position: GeoPosition,
        service: bool,
    },
    Open(GuideSurface),
    /// Open the seller named by acquisition guidance, independently of another
    /// nearer shop or a market the player previously inspected.
    OpenMarket {
        shop_id: u64,
    },
    /// Resume exactly the assignment described in the guide, independently of
    /// another offer/vehicle previously browsed in the Driving panel.
    OpenDriving {
        job_id: u64,
        vehicle_id: u64,
    },
    /// Open the named owner-private service and its relevant controls. This
    /// never starts, stops, pays for or returns a shipment by itself.
    OpenCompany(GuideCompanyFocus),
    Choose(GuideTrack),
    /// Open the existing name editor; founding remains an explicit command.
    NameNeighborhood,
}

impl GuideAction {
    /// Compare the intent shown to the player with a freshly derived action.
    /// A refresh must never turn a click on one transaction into another.
    pub fn same_intent(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Command(a), Self::Command(b)) => {
                match (serde_json::to_value(a), serde_json::to_value(b)) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                }
            }
            (
                Self::Travel {
                    position: a,
                    service: sa,
                },
                Self::Travel {
                    position: b,
                    service: sb,
                },
            ) => a == b && sa == sb,
            (Self::Open(a), Self::Open(b)) => a == b,
            (Self::OpenMarket { shop_id: a }, Self::OpenMarket { shop_id: b }) => a == b,
            (
                Self::OpenDriving {
                    job_id: a,
                    vehicle_id: av,
                },
                Self::OpenDriving {
                    job_id: b,
                    vehicle_id: bv,
                },
            ) => a == b && av == bv,
            (Self::OpenCompany(a), Self::OpenCompany(b)) => a == b,
            (Self::Choose(a), Self::Choose(b)) => a == b,
            (Self::NameNeighborhood, Self::NameNeighborhood) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivityGuidance {
    pub track: GuideTrack,
    pub step: u8,
    pub total_steps: u8,
    pub title: String,
    pub detail: String,
    pub action_label: String,
    pub action: Option<GuideAction>,
    /// Retains the exact request behind a supply suggestion. Two requests may
    /// propose the same gathering action; switching the obligation still needs
    /// the player's review. This is a derived snapshot, never a saved ledger.
    pub supply_contract: Option<SupplyContract>,
}

impl ActivityGuidance {
    fn new(
        track: GuideTrack,
        step: u8,
        total: u8,
        title: impl Into<String>,
        detail: impl Into<String>,
        label: impl Into<String>,
        action: GuideAction,
    ) -> Self {
        Self {
            track,
            step,
            total_steps: total,
            title: title.into(),
            detail: detail.into(),
            action_label: label.into(),
            action: Some(action),
            supply_contract: None,
        }
    }

    fn waiting(detail: &str) -> Self {
        Self {
            track: GuideTrack::Suggested,
            step: 0,
            total_steps: 0,
            title: "Getting your bearings".into(),
            detail: detail.into(),
            action_label: "Waiting for your world".into(),
            action: None,
            supply_contract: None,
        }
    }

    pub fn description(&self) -> String {
        if self.total_steps == 0 {
            return self.detail.clone();
        }
        format!(
            "{} · step {} of {}\n{}\n\n{}",
            self.track.label(),
            self.step,
            self.total_steps,
            self.title,
            self.detail
        )
    }

    /// Retain both the action and its obligation across a displayed handoff.
    /// Passing time alone does not invalidate a request, but a new owner,
    /// deadline, reward, quantity or destination does.
    pub fn same_intent(&self, other: &Self) -> bool {
        self.track == other.track
            && self.supply_contract == other.supply_contract
            && self
                .action
                .as_ref()
                .zip(other.action.as_ref())
                .is_some_and(|(shown, current)| shown.same_intent(current))
    }
}

fn reach(a: GeoPosition, b: GeoPosition) -> f64 {
    a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m)
}
fn item_name(item: &str) -> String {
    item.replace('_', " ")
}
fn money(value: u64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

/// Short world-clock deadline text. The final minutes do not round down to
/// "0.0 hours", and an expired request cannot look like one with time left.
pub fn supply_deadline_label(remaining_world_seconds: f64) -> String {
    if remaining_world_seconds <= 0. {
        "deadline passed".into()
    } else if remaining_world_seconds < 60. {
        let seconds = remaining_world_seconds.ceil();
        format!(
            "{seconds:.0} world {} left",
            if seconds == 1. { "second" } else { "seconds" }
        )
    } else if remaining_world_seconds < 3600. {
        let minutes = (remaining_world_seconds / 60.).ceil();
        format!(
            "{minutes:.0} world {} left",
            if minutes == 1. { "minute" } else { "minutes" }
        )
    } else {
        let hours = format!("{:.1}", remaining_world_seconds / 3600.);
        format!(
            "{hours} world {} left",
            if hours == "1.0" { "hour" } else { "hours" }
        )
    }
}

struct GuideContext<'a> {
    state: &'a SandboxState,
    catalog: &'a Catalog,
    player: u64,
    position: GeoPosition,
    inaccessible_plots: OnceCell<Option<GeoSpatialIndex<u64>>>,
}

/// Acquisition alternatives retain a small, explicit ordering for urgent needs:
/// an immediate command, nearby market, direct travel, then recipe preparation.
/// A blocked path sorts last instead of hiding another available food source.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum AcquisitionEffort {
    Ready,
    NearbyMarket,
    Travel,
    Preparation,
    Unavailable,
}

/// Supply already shows a complete-quantity price comparison. Its acquisition
/// instructions retain the actual seller/item and blockers without repeating
/// unit prices, stock and wallet; other journeys keep the full market detail.
#[derive(Clone, Copy)]
enum MarketDetail {
    Full,
    Task,
}

struct Acquisition {
    detail: String,
    label: String,
    action: GuideAction,
    effort: AcquisitionEffort,
    distance: f64,
}

impl Acquisition {
    fn new(
        detail: impl Into<String>,
        label: impl Into<String>,
        action: GuideAction,
        effort: AcquisitionEffort,
        distance: f64,
    ) -> Self {
        Self {
            detail: detail.into(),
            label: label.into(),
            action,
            effort,
            distance,
        }
    }

    fn parts(self) -> (String, String, GuideAction) {
        (self.detail, self.label, self.action)
    }
}

impl SandboxState {
    /// Returns bounded, state-derived guidance for a full world or a private
    /// replica. Missing regional infrastructure is explicitly described as such.
    pub fn activity_guidance(
        &self,
        player: u64,
        catalog: &Catalog,
        track: GuideTrack,
        preferred_contract: Option<u64>,
    ) -> ActivityGuidance {
        let Some(principal) = self.players.get(&player) else {
            return ActivityGuidance::waiting(
                "Waiting for your character and inventory from the world.",
            );
        };
        if self.is_player_dormant(player) {
            return ActivityGuidance::waiting(
                "Your session is inactive. Resume this owner before continuing work; existing goods, money and commitments remain unchanged.",
            );
        }
        if !self.living.enabled || !self.living.accounts.contains_key(&player) {
            return ActivityGuidance::waiting(
                "Terrain and local services are loading. Your next action will appear when your character is ready.",
            );
        }
        let context = GuideContext {
            state: self,
            catalog,
            player,
            position: principal.position,
            inaccessible_plots: OnceCell::new(),
        };
        let selected = if track == GuideTrack::Suggested {
            if let Some(urgent) = context.urgent_needs() {
                return urgent;
            }
            if self
                .transport
                .jobs
                .values()
                .any(|j| j.assignment.as_ref().is_some_and(|a| a.owner == player))
            {
                GuideTrack::Road
            } else if self
                .settlements
                .contracts
                .values()
                .any(|c| c.claimed_by == Some(player))
            {
                GuideTrack::Supply
            } else if self.transport.companies.contains_key(&player) {
                GuideTrack::Company
            } else if self
                .settlements
                .progress
                .get(&player)
                .is_none_or(|p| p.contracts_completed == 0)
            {
                GuideTrack::Supply
            } else if context.territory().is_none() {
                GuideTrack::Community
            } else if principal.home.is_none() {
                GuideTrack::Home
            } else {
                GuideTrack::Community
            }
        } else {
            track
        };
        match selected {
            GuideTrack::Supply => context.supply(preferred_contract),
            GuideTrack::Home => context.home(),
            GuideTrack::Community => context.community(),
            GuideTrack::Road => context.road(),
            GuideTrack::Company => context.company(),
            GuideTrack::Suggested => unreachable!(),
        }
    }
}

impl GuideContext<'_> {
    /// Urgent suggestions use the same active person as Consume. A registered
    /// vehicle uses its account's walking avatar, as driving's needs cap does;
    /// it never receives an eating, drinking or resting command itself.
    fn urgent_needs(&self) -> Option<ActivityGuidance> {
        let account = self.state.living.accounts.get(&self.player)?;
        let active = self.state.living.units.get(&account.active_unit)?;
        let driving = self
            .state
            .transport
            .vehicles
            .get(&active.id)
            .is_some_and(|vehicle| vehicle.owner == self.player);
        let body = if driving {
            self.state.living.units.get(&account.avatar_unit)?
        } else {
            active
        };
        if !body.is_person() || body.owner != Some(self.player) {
            return None;
        }
        let (need, level, verb) = if body.needs.water <= URGENT_NEED_LEVEL {
            ("Water", body.needs.water, "drink")
        } else if body.needs.food <= URGENT_NEED_LEVEL {
            ("Food", body.needs.food, "eat")
        } else if body.needs.energy <= URGENT_NEED_LEVEL {
            ("Energy", body.needs.energy, "rest")
        } else {
            return None;
        };
        let status = format!(
            "{need} {:.0}%. Low needs reduce movement and driving speed.",
            level * 100.
        );
        if driving {
            return Some(ActivityGuidance::new(
                GuideTrack::Suggested,
                1,
                2,
                format!("Park and exit to {verb}"),
                format!(
                    "Driver · {status} Stop in a supported, clear place and choose Exit on foot in Driving. Your next suggestion will help your driver {verb}."
                ),
                "Open Driving to park and exit",
                GuideAction::Open(GuideSurface::Driving),
            ));
        }
        if verb == "rest" {
            let resting = matches!(body.order, UnitOrder::Rest);
            return Some(ActivityGuidance::new(
                GuideTrack::Suggested,
                1,
                1,
                if resting {
                    "Recover your energy"
                } else {
                    "Rest before continuing"
                },
                format!(
                    "{status} Use Rest units in Command to stop this actor's current order and recover while you remain still. Moving starts activity again."
                ),
                if resting {
                    "View rest and needs"
                } else {
                    "Open Command to rest"
                },
                GuideAction::Open(GuideSurface::Command),
            ));
        }
        let consumables: &[&str] = if verb == "drink" {
            &["water"]
        } else {
            &["bread", "cooked_fish", "berries"]
        };
        if let Some(item) = consumables.iter().copied().find(|item| self.have(item) > 0) {
            return Some(ActivityGuidance::new(
                GuideTrack::Suggested,
                1,
                1,
                if verb == "drink" {
                    "Drink before continuing"
                } else {
                    "Eat before continuing"
                },
                format!(
                    "{status} Use one carried {}. Your existing work remains available after you recover.",
                    item_name(item)
                ),
                format!(
                    "{} 1 {}",
                    if verb == "drink" { "Drink" } else { "Eat" },
                    item_name(item)
                ),
                GuideAction::Command(Command::Living {
                    action: LivingCommand::Consume { item: item.into() },
                }),
            ));
        }
        let (item, acquisition) = consumables
            .iter()
            .copied()
            .filter(|item| self.catalog.items.contains(*item))
            .map(|item| (item, self.acquire(item, 1, 0)))
            .min_by(|(_, a), (_, b)| {
                a.effort
                    .cmp(&b.effort)
                    .then_with(|| a.distance.total_cmp(&b.distance))
            })
            .unwrap_or_else(|| (consumables[0], self.acquire(consumables[0], 1, 0)));
        let (detail, label, action) = acquisition.parts();
        Some(ActivityGuidance::new(
            GuideTrack::Suggested,
            1,
            2,
            format!("Obtain {} so you can {verb}", item_name(item)),
            format!(
                "{status} You carry no suitable supply. {detail}\nAfter obtaining it, Suggested journey will offer the ordinary consume action."
            ),
            label,
            action,
        ))
    }

    fn have(&self, item: &str) -> u32 {
        self.state.players[&self.player]
            .inventory
            .get(item)
            .copied()
            .unwrap_or(0)
    }
    fn wallet(&self) -> u64 {
        self.state
            .living
            .accounts
            .get(&self.player)
            .map_or(0, |a| a.money)
    }
    fn territory(&self) -> Option<&Settlement> {
        self.state
            .settlements
            .territories
            .values()
            .filter(|t| {
                self.state
                    .may_build(self.player, t.faction_owner, t.founder)
            })
            .min_by(|a, b| {
                reach(self.position, a.center).total_cmp(&reach(self.position, b.center))
            })
    }

    fn supply(&self, preferred: Option<u64>) -> ActivityGuidance {
        let contracts = &self.state.settlements.contracts;
        // Accepted work wins over a browsed unclaimed offer; another player's
        // offer can never become the recipient's next action.
        let eligible = |c: &&SupplyContract| {
            c.claimed_by == Some(self.player)
                || (c.claimed_by.is_none()
                    && c.expires_at > self.state.settlements.elapsed_seconds
                    && !self.state.is_obsolete_public_supply(c))
        };
        let contract = preferred
            .and_then(|id| contracts.get(&id))
            .filter(|c| c.claimed_by == Some(self.player))
            .or_else(|| {
                contracts
                    .values()
                    .filter(|c| c.claimed_by == Some(self.player))
                    .min_by(|a, b| a.expires_at.total_cmp(&b.expires_at).then(a.id.cmp(&b.id)))
            })
            .or_else(|| preferred.and_then(|id| contracts.get(&id)).filter(eligible))
            .or_else(|| {
                contracts.values().filter(eligible).min_by(|a, b| {
                    let a_ready = self.have(&a.item) >= a.amount;
                    let b_ready = self.have(&b.item) >= b.amount;
                    b_ready.cmp(&a_ready).then_with(|| {
                        reach(self.position, a.target_position)
                            .total_cmp(&reach(self.position, b.target_position))
                    })
                })
            });
        let Some(c) = contract else {
            return ActivityGuidance::new(
                GuideTrack::Supply,
                1,
                4,
                "Look for funded work",
                "No current funded supply request is visible. Expired public offers cannot be accepted; requests whose demand is already filled wait for a protected issuer refund. Wait for the noticeboard to refresh, explore resources or try Driving. In multiplayer, visit the settlement to receive its local offers.",
                "Explore resources",
                GuideAction::Open(GuideSurface::Nature),
            );
        };
        let mut guidance = self.supply_contract(c);
        guidance.supply_contract = Some(c.clone());
        guidance
    }

    fn supply_contract(&self, c: &SupplyContract) -> ActivityGuidance {
        let deadline = format!(
            "{}; this deadline follows the world clock.",
            supply_deadline_label(c.expires_at - self.state.settlements.elapsed_seconds)
        );
        if c.claimed_by == Some(self.player)
            && c.expires_at <= self.state.settlements.elapsed_seconds
        {
            return ActivityGuidance::new(
                GuideTrack::Supply,
                1,
                4,
                "This supply deadline has passed",
                "Delivery is no longer available. Return this request, then review other funded work. Returning it consumes no carried goods and earns no reward; the issuer's reserved money remains protected.",
                "Return expired supply request",
                GuideAction::Command(Command::Settlement {
                    action: SettlementCommand::CancelContract { contract_id: c.id },
                }),
            );
        }
        let target = self
            .state
            .living
            .producers
            .get(&c.target_site)
            .map(|s| s.name.as_str())
            .or_else(|| {
                self.state
                    .living
                    .shops
                    .get(&c.target_site)
                    .map(|s| s.name.as_str())
            })
            .unwrap_or("the receiving service");
        if c.claimed_by.is_none() {
            return ActivityGuidance::new(
                GuideTrack::Supply,
                1,
                4,
                format!("Accept {} {} for {}", c.amount, item_name(&c.item), target),
                format!(
                    "Carry {} / {}. Accept → obtain goods → travel → deliver. Accepting spends no goods and resets the deadline to {:.1} world hours.\n\n{}",
                    self.have(&c.item),
                    c.amount,
                    ((self.state.settlements.elapsed_seconds
                        + self.state.settlements.contract_duration_seconds)
                        .min(1e12)
                        - self.state.settlements.elapsed_seconds)
                        / 3600.,
                    self.supply_cost_brief(c),
                ),
                "Accept this supply request",
                GuideAction::Command(Command::Settlement {
                    action: SettlementCommand::AcceptContract { contract_id: c.id },
                }),
            );
        }
        if self.have(&c.item) < c.amount {
            let (detail, label, action) = self
                .acquire_with_market_detail(&c.item, c.amount, 0, MarketDetail::Task)
                .parts();
            return ActivityGuidance::new(
                GuideTrack::Supply,
                2,
                4,
                format!(
                    "Collect {} {} · {} / {}",
                    c.amount,
                    item_name(&c.item),
                    self.have(&c.item),
                    c.amount
                ),
                format!(
                    "{deadline}\n{detail}\nDeliver all {} {} to {target} within 20 m. Goods remain yours until delivery.\n\n{}",
                    c.amount,
                    item_name(&c.item),
                    self.supply_cost_brief(c),
                ),
                label,
                action,
            );
        }
        let readiness = self.state.supply_contract_readiness(
            self.player,
            c.id,
            SupplyContractAction::Deliver,
            self.catalog,
        );
        let destination_outside_view = matches!(
            &readiness,
            Err(SupplyContractBlocker::DestinationUnavailable)
        );
        if let Err(blocker) = &readiness {
            let can_travel = matches!(blocker, SupplyContractBlocker::OutOfReach)
                || (destination_outside_view
                    && reach(self.position, c.target_position) > INTERACTION_REACH_M);
            if !can_travel {
                return ActivityGuidance::new(
                    GuideTrack::Supply,
                    4,
                    4,
                    "Delivery needs attention",
                    format!(
                        "{deadline}\n{blocker}\nYou may keep this request while resolving the blocker, or return it explicitly below. Returning spends no carried goods and pays no reward."
                    ),
                    "Return this supply request",
                    GuideAction::Command(Command::Settlement {
                        action: SettlementCommand::CancelContract { contract_id: c.id },
                    }),
                );
            }
        }
        if reach(self.position, c.target_position) > INTERACTION_REACH_M {
            return ActivityGuidance::new(
                GuideTrack::Supply,
                3,
                4,
                format!("Bring your supplies to {target}"),
                format!(
                    "{deadline}\nAll {} {} are ready. Destination {:.0} m away; deliver within 20 m. The order uses your active owned actor and requires loaded, dry terrain.{}\n\n{}",
                    c.amount,
                    item_name(&c.item),
                    reach(self.position, c.target_position),
                    if destination_outside_view {
                        " The receiving service is outside the current view; arrival does not guarantee capacity. Review again after it loads."
                    } else {
                        " Capacity and reward-wallet space are checked again at delivery."
                    },
                    self.supply_cost_brief(c),
                ),
                "Travel to the delivery",
                GuideAction::Travel {
                    position: c.target_position,
                    service: true,
                },
            );
        }
        ActivityGuidance::new(
            GuideTrack::Supply,
            4,
            4,
            format!("Deliver {} {}", c.amount, item_name(&c.item)),
            format!(
                "{deadline}\nYou are in reach of {target}. Delivery consumes the requested goods and transfers the reserved {} gross credits plus {} community XP. Supply and travel costs are not deducted from that displayed reward. Your next goal updates after the command succeeds.",
                money(c.reward),
                c.experience
            ),
            "Deliver and collect reward",
            GuideAction::Command(Command::Settlement {
                action: SettlementCommand::FulfillContract { contract_id: c.id },
            }),
        )
    }

    fn supply_cost_brief(&self, c: &SupplyContract) -> String {
        let reward = format!(
            "Gross reserved reward: {} cr + {} community XP.",
            money(c.reward),
            c.experience
        );
        let missing = c.amount.saturating_sub(self.have(&c.item));
        if missing == 0 {
            return format!(
                "{reward} Your goods' acquisition cost is not recorded and travel is unpriced; this is not a profit figure."
            );
        }
        let seller = self
            .state
            .living
            .shops
            .values()
            .filter_map(|shop| {
                let price = shop.prices.get(&c.item)?.buy;
                (price > 0 && shop.stock.get(&c.item).copied().unwrap_or(0) >= missing)
                    .then_some((price.checked_mul(u64::from(missing))?, shop))
            })
            .min_by(|(a_cost, a), (b_cost, b)| {
                a_cost
                    .cmp(b_cost)
                    .then_with(|| {
                        reach(self.position, a.position)
                            .total_cmp(&reach(self.position, b.position))
                    })
                    .then(a.id.cmp(&b.id))
            });
        let Some((cost, seller)) = seller else {
            return format!(
                "{reward} Missing {missing} {}. No full-quantity price comparison is available from visible sellers. Gather, craft or check another region. Carried goods and travel are unpriced; the reward is not profit.",
                item_name(&c.item)
            );
        };
        let comparison = if cost > c.reward {
            format!(
                " This exceeds the gross reward by {} cr before other costs.",
                money(cost - c.reward)
            )
        } else {
            String::new()
        };
        let wallet = if cost > self.wallet() {
            format!(
                " Wallet: {} cr; shortfall: {} cr.",
                money(self.wallet()),
                money(cost - self.wallet())
            )
        } else {
            String::new()
        };
        format!(
            "{reward} Lowest visible full-quantity comparison: {missing} {} at {}, {} cr.{comparison}{wallet} Carried goods and travel are unpriced; the reward is not profit. Review the live Market quote; prices and stock can change. Gathering or crafting remain alternatives.",
            item_name(&c.item),
            seller.name,
            money(cost),
        )
    }

    /// Resolve one bounded acquisition action. Recipe chains are limited to
    /// three levels so a custom cyclic catalog cannot recurse indefinitely.
    fn acquire(&self, item: &str, desired: u32, depth: u8) -> Acquisition {
        self.acquire_with_market_detail(item, desired, depth, MarketDetail::Full)
    }

    fn acquire_with_market_detail(
        &self,
        item: &str,
        desired: u32,
        depth: u8,
        market_detail: MarketDetail,
    ) -> Acquisition {
        let missing = desired.saturating_sub(self.have(item));
        let recipe = self
            .catalog
            .recipes
            .iter()
            .find(|(_, r)| r.outputs.get(item).is_some_and(|n| *n > 0));
        let mut prepared_recipe = None;
        let mut inventory_blocked = false;
        if let Some((id, r)) = recipe {
            let desired_count = missing.div_ceil(r.outputs[item]).clamp(1, 20);
            // At most 20 bounded inventory preparations. Include every output
            // and remove consumed inputs first, exactly as Craft does. A
            // smaller executable batch can make progress without overfilling.
            let count = (1..=desired_count).rev().find(|count| {
                match exchange(
                    &self.state.players[&self.player].inventory,
                    &r.inputs,
                    &r.outputs,
                    *count,
                ) {
                    Ok(_) => true,
                    Err(SandboxError::Capacity) => {
                        inventory_blocked = true;
                        false
                    }
                    Err(_) => false,
                }
            });
            if let Some(count) = count {
                if r.requires_workbench {
                    let bench = self
                        .state
                        .structures
                        .values()
                        .filter(|s| {
                            s.workbench && self.state.may_use(self.player, s.faction_owner, s.owner)
                        })
                        .min_by(|a, b| {
                            reach(self.position, a.position)
                                .total_cmp(&reach(self.position, b.position))
                        });
                    prepared_recipe = match bench {
                        Some(b) if reach(self.position, b.position) > INTERACTION_REACH_M => {
                            Some(Acquisition::new(
                                "Your ingredients are ready. Processing needs an accessible workbench within 20 m.",
                                "Go to your workbench",
                                GuideAction::Travel {
                                    position: b.position,
                                    service: false,
                                },
                                AcquisitionEffort::Travel,
                                reach(self.position, b.position),
                            ))
                        }
                        None => Some(Acquisition::new(
                            "Your ingredients are ready, but no accessible workbench is loaded. Build one on your land, or visit an existing shared workbench.",
                            "Open building tools",
                            GuideAction::Open(GuideSurface::Build),
                            AcquisitionEffort::Unavailable,
                            0.,
                        )),
                        _ => None,
                    };
                }
                if prepared_recipe.is_none() {
                    return Acquisition::new(
                        format!(
                            "Crafting uses {}. Outputs go to the same inventory as your supply work.",
                            r.inputs
                                .iter()
                                .map(|(item, n)| format!(
                                    "{} {}",
                                    u64::from(*n) * u64::from(count),
                                    item_name(item)
                                ))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        format!(
                            "Craft {} {}",
                            u64::from(r.outputs[item]) * u64::from(count),
                            item_name(item)
                        ),
                        GuideAction::Command(Command::Craft {
                            recipe: id.clone(),
                            count,
                        }),
                        AcquisitionEffort::Ready,
                        0.,
                    );
                }
            }
        }
        let mut candidates: Vec<_> = self
            .state
            .resource_nodes
            .values()
            .filter(|r| r.item == item && r.remaining > 0)
            .map(|r| (reach(self.position, r.position), r))
            .collect();
        let compare = |a: &(f64, &ResourceNode), b: &(f64, &ResourceNode)| {
            a.0.total_cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id))
        };
        if candidates.len() > MAX_GUIDE_RESOURCE_CANDIDATES {
            candidates.select_nth_unstable_by(MAX_GUIDE_RESOURCE_CANDIDATES, compare);
            candidates.truncate(MAX_GUIDE_RESOURCE_CANDIDATES);
        }
        candidates.sort_unstable_by(compare);
        let patch = candidates.into_iter().map(|(_, r)| r).find(|r| {
            self.state
                .check_settlement_interact(self.player, r.position)
                .is_ok()
                && self
                    .state
                    .validate_ecology_gather(self.player, &r.id)
                    .is_ok()
                && self.patch_permissions_allow(r.position)
        });
        if let Some(r) = patch {
            let requested = missing.min(5).min(r.remaining).max(1);
            let amount = (1..=requested).rev().find(|amount| {
                exchange(
                    &self.state.players[&self.player].inventory,
                    &Inventory::new(),
                    &inventory(&[(&r.item, *amount)]),
                    1,
                )
                .is_ok()
            });
            if let Some(amount) = amount {
                let distance = reach(self.position, r.position);
                let detail = format!(
                    "Need {missing} more {}. Accessible patch has {} units, {:.0} m away. Gather within 20 m; depleted or protected patches are skipped.",
                    item_name(item),
                    r.remaining,
                    distance
                );
                if distance > INTERACTION_REACH_M {
                    return Acquisition::new(
                        detail,
                        format!("Go to {}", item_name(item)),
                        GuideAction::Travel {
                            position: r.position,
                            service: false,
                        },
                        AcquisitionEffort::Travel,
                        distance,
                    );
                }
                return Acquisition::new(
                    detail,
                    format!("Gather {amount} {}", item_name(item)),
                    GuideAction::Command(Command::Gather {
                        node_id: r.id.clone(),
                        amount,
                    }),
                    AcquisitionEffort::Ready,
                    distance,
                );
            }
            inventory_blocked = true;
        }
        // An available finished item remains useful when inputs are absent or
        // held ingredients cannot be processed without another workbench.
        if let Some(action) = self.market_acquisition(item, true, market_detail) {
            return action;
        }
        if let Some(action) = prepared_recipe {
            return action;
        }
        if inventory_blocked {
            let shop = self.state.living.shops.values().min_by(|a, b| {
                reach(self.position, a.position)
                    .total_cmp(&reach(self.position, b.position))
                    .then(a.id.cmp(&b.id))
            });
            return Acquisition::new(
                format!(
                    "Your inventory cannot fit the proposed {} acquisition, including every crafting output. Sell or use carried goods to free item slots or stack space, then review this step again. No ingredients or resources were spent.",
                    item_name(item)
                ),
                if shop.is_some() {
                    "Review inventory and selling"
                } else {
                    "Inspect your inventory"
                },
                shop.map_or(GuideAction::Open(GuideSurface::Command), |shop| {
                    GuideAction::OpenMarket { shop_id: shop.id }
                }),
                AcquisitionEffort::Unavailable,
                0.,
            );
        }
        if depth < 3
            && let Some((_, r)) = recipe
        {
            let count = missing.div_ceil(r.outputs[item]).clamp(1, 20);
            if let Some((input, n)) = r
                .inputs
                .iter()
                .find(|(input, n)| u64::from(self.have(input)) < u64::from(**n) * u64::from(count))
            {
                let required = n.saturating_mul(count);
                let mut acquisition =
                    self.acquire_with_market_detail(input, required, depth + 1, market_detail);
                acquisition.detail = format!(
                    "To make {}, first collect {required} {}.\n{}",
                    item_name(item),
                    item_name(input),
                    acquisition.detail
                );
                if acquisition.effort != AcquisitionEffort::Unavailable {
                    acquisition.effort = AcquisitionEffort::Preparation;
                }
                return acquisition;
            }
        }
        if let Some(action) = self.market_acquisition(item, false, market_detail) {
            return action;
        }
        Acquisition::new(
            format!(
                "No accessible, equipped {} patch was found among the nearest 128 loaded candidates, and no stocked market is loaded. Explore another region or obtain the required equipment in Nature. Supply contracts can also be returned without spending their goods.",
                item_name(item)
            ),
            "Find resources and equipment",
            GuideAction::Open(GuideSurface::Nature),
            AcquisitionEffort::Unavailable,
            0.,
        )
    }

    fn market_acquisition(
        &self,
        item: &str,
        available_only: bool,
        market_detail: MarketDetail,
    ) -> Option<Acquisition> {
        let can_buy = |shop: &Shop| {
            shop.prices.get(item).is_some_and(|price| {
                price.buy > 0
                    && self.wallet() >= price.buy
                    && shop
                        .money
                        .checked_add(price.buy)
                        .is_some_and(|money| money <= MAX_MONEY)
                    && self.have(item) < MAX_ITEM_COUNT
                    && (self.state.players[&self.player]
                        .inventory
                        .contains_key(item)
                        || self.state.players[&self.player].inventory.len() < MAX_INVENTORY_ITEMS)
            })
        };
        let seller = self
            .state
            .living
            .shops
            .values()
            .filter(|shop| {
                shop.stock.get(item).copied().unwrap_or(0) > 0
                    && shop.prices.contains_key(item)
                    && (!available_only || can_buy(shop))
            })
            .min_by(|a, b| {
                reach(self.position, a.position)
                    .total_cmp(&reach(self.position, b.position))
                    .then(a.id.cmp(&b.id))
            })?;
        let price = seller.prices[item].buy;
        let available = can_buy(seller);
        let explanation = if available {
            "Buy only what you need from the actual market quote."
        } else if self.wallet() < price {
            "You cannot afford one unit yet. Sell surplus goods or earn a funded reward before buying."
        } else {
            "This exchange is currently blocked by inventory or market cash capacity. Inspect the quote and make room, or visit another seller."
        };
        let distance = reach(self.position, seller.position);
        let detail = match market_detail {
            MarketDetail::Full => format!(
                "{} stocks {} {} at {} credits each; you have {} credits. {explanation}",
                seller.name,
                seller.stock[item],
                item_name(item),
                money(price),
                money(self.wallet())
            ),
            MarketDetail::Task if available && distance > INTERACTION_REACH_M => {
                format!("Travel to {} for {}.", seller.name, item_name(item))
            }
            MarketDetail::Task => {
                let next = format!(
                    "Review {}'s live {} quote in Market.",
                    seller.name,
                    item_name(item)
                );
                if available {
                    next
                } else {
                    format!("{next} {explanation}")
                }
            }
        };
        if available && distance > INTERACTION_REACH_M {
            return Some(Acquisition::new(
                detail,
                "Travel to the market",
                GuideAction::Travel {
                    position: seller.position,
                    service: true,
                },
                AcquisitionEffort::Travel,
                distance,
            ));
        }
        Some(Acquisition::new(
            detail,
            "Inspect market prices",
            GuideAction::OpenMarket { shop_id: seller.id },
            if available {
                AcquisitionEffort::NearbyMarket
            } else {
                AcquisitionEffort::Unavailable
            },
            distance,
        ))
    }

    fn patch_permissions_allow(&self, position: GeoPosition) -> bool {
        let index = self.inaccessible_plots.get_or_init(|| {
            GeoSpatialIndex::from_positions(
                100.,
                self.state
                    .plots
                    .values()
                    .filter(|plot| {
                        !self
                            .state
                            .may_interact(self.player, plot.faction_owner, plot.owner)
                    })
                    .map(|plot| (plot.id, plot.center)),
            )
            .ok()
        });
        let Some(index) = index else {
            return false;
        };
        // Persisted plot radii are bounded by 100m. The broad phase returns only
        // nearby denied plots; exact reach follows the authority's own rule.
        index.query(position, 100.).is_ok_and(|nearby| {
            nearby.ids.into_iter().all(|id| {
                let plot = &self.state.plots[&id];
                plot.center.surface_distance_m(position) > plot.radius_m
            })
        })
    }

    fn home(&self) -> ActivityGuidance {
        if self.state.players[&self.player].home.is_some() {
            return ActivityGuidance::new(
                GuideTrack::Home,
                4,
                4,
                "Your home is established",
                "Your home is part of the saved world. Continue with public works, farming and funded supply requests. Access still follows the house owner's current faction rules.",
                "Continue with your community",
                GuideAction::Choose(GuideTrack::Community),
            );
        }
        if let Some(house) = self
            .state
            .structures
            .values()
            .filter(|s| s.housing && self.state.may_use(self.player, s.faction_owner, s.owner))
            .min_by(|a, b| {
                reach(self.position, a.position).total_cmp(&reach(self.position, b.position))
            })
        {
            if reach(self.position, house.position) > INTERACTION_REACH_M {
                return ActivityGuidance::new(
                    GuideTrack::Home,
                    4,
                    4,
                    "Move into an accessible home",
                    format!(
                        "An accessible house is {:.0} m away. Set it as home within 20 m; another owner's private house cannot be selected.",
                        reach(self.position, house.position)
                    ),
                    "Travel to the house",
                    GuideAction::Travel {
                        position: house.position,
                        service: false,
                    },
                );
            }
            return ActivityGuidance::new(
                GuideTrack::Home,
                4,
                4,
                "Set this house as home",
                "This house permits your access and is within reach. Setting home records this house without charging additional materials.",
                "Set this house as home",
                GuideAction::Command(Command::SetHome {
                    structure_id: house.id,
                }),
            );
        }
        let plot = self
            .state
            .plots
            .values()
            .filter(|p| self.state.may_build(self.player, p.faction_owner, p.owner))
            .min_by(|a, b| {
                reach(self.position, a.center).total_cmp(&reach(self.position, b.center))
            });
        let Some(plot) = plot else {
            return ActivityGuidance::new(
                GuideTrack::Home,
                1,
                4,
                "Claim a housing plot",
                format!(
                    "Open Build and choose Claim land, then place a plot on supported, unoccupied land near your active actor. Claims must respect territory rights.{}",
                    if self.state.is_partial() {
                        " Only loaded plots are visible; visit an existing plot before making another."
                    } else {
                        ""
                    }
                ),
                "Open building tools",
                GuideAction::Open(GuideSurface::Build),
            );
        };
        if let Some(blueprint) = self.catalog.blueprints.get("house") {
            if let Some((item, n)) = blueprint
                .costs
                .iter()
                .find(|(item, n)| self.have(item) < **n)
            {
                let (detail, label, action) = self.acquire(item, *n, 0).parts();
                return ActivityGuidance::new(
                    GuideTrack::Home,
                    2,
                    4,
                    format!(
                        "Prepare house materials · {} {}/{}",
                        item_name(item),
                        self.have(item),
                        n
                    ),
                    detail,
                    label,
                    action,
                );
            }
        } else {
            return ActivityGuidance::new(
                GuideTrack::Home,
                2,
                4,
                "Choose housing from your game's catalog",
                "This catalog has no standard house blueprint. Open Build to inspect available buildings; no material cost is assumed.",
                "Open building tools",
                GuideAction::Open(GuideSurface::Build),
            );
        }
        if reach(self.position, plot.center) > INTERACTION_REACH_M {
            return ActivityGuidance::new(
                GuideTrack::Home,
                3,
                4,
                "Return to your housing plot",
                format!(
                    "House materials are ready. Your accessible plot is {:.0} m away. Build near the active actor and keep the footprint clear of people and vehicles.",
                    reach(self.position, plot.center)
                ),
                "Travel to your plot",
                GuideAction::Travel {
                    position: plot.center,
                    service: false,
                },
            );
        }
        ActivityGuidance::new(
            GuideTrack::Home,
            3,
            4,
            "Place your house",
            "Your plot and materials are ready. Choose House in Build, place its complete footprint inside your plot, then return here to set it as home.",
            "Place a house in Build",
            GuideAction::Open(GuideSurface::Build),
        )
    }

    fn community(&self) -> ActivityGuidance {
        let Some(t) = self.territory() else {
            for (item, n) in SettlementTier::Neighborhood.material_cost() {
                if self.have(&item) < n {
                    let (detail, label, action) = self.acquire(&item, n, 0).parts();
                    return ActivityGuidance::new(
                        GuideTrack::Community,
                        1,
                        5,
                        format!(
                            "Prepare founding supplies · {} {}/{}",
                            item_name(&item),
                            self.have(&item),
                            n
                        ),
                        detail,
                        label,
                        action,
                    );
                }
            }
            let cost = SettlementTier::Neighborhood.credit_cost();
            if self.wallet() < cost {
                return ActivityGuidance::new(
                    GuideTrack::Community,
                    1,
                    5,
                    "Fund your first neighborhood",
                    format!(
                        "Founding deposits {} credits into its public treasury; you have {}. Complete funded work or sell surplus goods. The guide never creates money.",
                        money(cost),
                        money(self.wallet())
                    ),
                    "Find funded supply work",
                    GuideAction::Choose(GuideTrack::Supply),
                );
            }
            if self
                .state
                .check_settlement_land(
                    self.player,
                    self.position,
                    SettlementTier::Neighborhood.radius_m(),
                )
                .is_err()
            {
                return ActivityGuidance::new(
                    GuideTrack::Community,
                    1,
                    5,
                    "Find space for your neighborhood",
                    "The 100 m claim here conflicts with another owner's territory. Move to supported land clear of protected territory, then review founding again.",
                    "Inspect territory and rights",
                    GuideAction::Open(GuideSurface::Territory),
                );
            }
            return ActivityGuidance::new(
                GuideTrack::Community,
                1,
                5,
                "Name your neighborhood",
                "Found here with 20 wood, 10 stone and 50 credits. The credits become the public treasury. The final command validates the complete claim and existing plots. You choose the name before committing.",
                "Name and found a neighborhood",
                GuideAction::NameNeighborhood,
            );
        };
        let Ok(readiness) = self.state.settlement_readiness(self.player, t.id) else {
            return ActivityGuidance::new(
                GuideTrack::Community,
                2,
                5,
                "Inspect your community",
                "Visit the territory to refresh its requirements.",
                "Open Territory",
                GuideAction::Open(GuideSurface::Territory),
            );
        };
        let Some(cost) = readiness.next else {
            return ActivityGuidance::new(
                GuideTrack::Community,
                5,
                5,
                format!("{} holds a city charter", t.name),
                "Keep services supplied, maintain your farms and help other players through shared land. Road work and a company fleet provide another use for your economy.",
                "Explore road work",
                GuideAction::Choose(GuideTrack::Road),
            );
        };
        if readiness.experience < cost.experience {
            return ActivityGuidance::new(
                GuideTrack::Community,
                2,
                5,
                format!(
                    "Earn community XP · {} / {}",
                    readiness.experience, cost.experience
                ),
                format!(
                    "{} needs community experience for its {} charter. Complete funded supply requests; simply accepting a contract grants no XP.",
                    t.name,
                    cost.tier.label()
                ),
                "Work on supply contracts",
                GuideAction::Choose(GuideTrack::Supply),
            );
        }
        if readiness.houses < cost.houses || readiness.farms < cost.farms {
            return ActivityGuidance::new(
                GuideTrack::Community,
                3,
                5,
                format!("Build public infrastructure for {}", t.name),
                format!(
                    "{}houses {} / {} · farms {} / {}. Buildings must fit inside this territory and permit your builder role.{}",
                    if self.state.is_partial() {
                        "Loaded "
                    } else {
                        ""
                    },
                    readiness.houses,
                    cost.houses,
                    readiness.farms,
                    cost.farms,
                    if self.state.is_partial() {
                        " Visit the territory; authority checks the full world before upgrading. If the structures already exist, use Upgrade in Territory."
                    } else {
                        " Place houses on claimed plots and plant crops in Build."
                    }
                ),
                "Open building and farming",
                GuideAction::Open(GuideSurface::Build),
            );
        }
        for (item, n) in &cost.materials {
            let remaining = n.saturating_sub(t.stock.get(item).copied().unwrap_or(0));
            if remaining == 0 {
                continue;
            }
            if self.have(item) == 0 {
                let (detail, label, action) = self.acquire(item, remaining, 0).parts();
                return ActivityGuidance::new(
                    GuideTrack::Community,
                    4,
                    5,
                    format!(
                        "Supply public works · {remaining} {} needed",
                        item_name(item)
                    ),
                    detail,
                    label,
                    action,
                );
            }
            if reach(self.position, t.center) > INTERACTION_REACH_M {
                return self.return_to_center(t, 4);
            }
            let amount = remaining.min(self.have(item)).min(5);
            return ActivityGuidance::new(
                GuideTrack::Community,
                4,
                5,
                format!("Contribute to {}", t.name),
                format!(
                    "Public works still need {remaining} {}. You carry {}. This transfers {amount} to the public stock; the charter consumes it only when upgraded.",
                    item_name(item),
                    self.have(item)
                ),
                format!("Contribute {amount} {}", item_name(item)),
                GuideAction::Command(Command::Settlement {
                    action: SettlementCommand::Contribute {
                        settlement_id: t.id,
                        item: item.clone(),
                        amount,
                    },
                }),
            );
        }
        if t.treasury < cost.credits {
            let remaining = cost.credits - t.treasury;
            if self.wallet() == 0 {
                return ActivityGuidance::new(
                    GuideTrack::Community,
                    4,
                    5,
                    "Raise the charter funds",
                    format!(
                        "The public treasury needs {} more credits. Earn credits through funded work before contributing.",
                        money(remaining)
                    ),
                    "Find supply work",
                    GuideAction::Choose(GuideTrack::Supply),
                );
            }
            if reach(self.position, t.center) > INTERACTION_REACH_M {
                return self.return_to_center(t, 4);
            }
            let amount = remaining.min(self.wallet()).min(2500);
            return ActivityGuidance::new(
                GuideTrack::Community,
                4,
                5,
                "Fund the charter treasury",
                format!(
                    "Public treasury {} / {} credits; personal wallet {}. The next contribution transfers {} credits from your wallet.",
                    money(t.treasury),
                    money(cost.credits),
                    money(self.wallet()),
                    money(amount)
                ),
                format!("Contribute {} credits", money(amount)),
                GuideAction::Command(Command::Settlement {
                    action: SettlementCommand::Fund {
                        settlement_id: t.id,
                        amount,
                    },
                }),
            );
        }
        if reach(self.position, t.center) > INTERACTION_REACH_M {
            return self.return_to_center(t, 5);
        }
        ActivityGuidance::new(
            GuideTrack::Community,
            5,
            5,
            format!("Apply for the {} charter", cost.tier.label()),
            "The visible requirements are met. Authority checks land rights, the full infrastructure, stock and treasury again before the upgrade spends public resources.",
            format!("Upgrade to {}", cost.tier.label()),
            GuideAction::Command(Command::Settlement {
                action: SettlementCommand::Upgrade {
                    settlement_id: t.id,
                },
            }),
        )
    }

    fn return_to_center(&self, t: &Settlement, step: u8) -> ActivityGuidance {
        ActivityGuidance::new(
            GuideTrack::Community,
            step,
            5,
            format!("Return to {}", t.name),
            format!(
                "Public contributions and charter applications require the active actor within 20 m of the center, {:.0} m away.",
                reach(self.position, t.center)
            ),
            "Travel to the community center",
            GuideAction::Travel {
                position: t.center,
                service: false,
            },
        )
    }

    fn road(&self) -> ActivityGuidance {
        let jobs = &self.state.transport.jobs;
        if let Some(job) = jobs.values().find(|j| {
            j.assignment
                .as_ref()
                .is_some_and(|a| a.owner == self.player)
        }) {
            let a = job.assignment.as_ref().unwrap();
            let id = if a.phase == TransportJobPhase::ToDestination {
                job.destination_facility
            } else {
                job.pickup_facility
            };
            let target = self.state.transport.facilities.get(&id);
            let title = match a.phase {
                TransportJobPhase::ToPickup => "Collect your assigned load",
                TransportJobPhase::ToDestination => "Deliver your assigned load",
                TransportJobPhase::Returning => "Return the interrupted load",
            };
            return ActivityGuidance::new(
                GuideTrack::Road,
                if a.phase == TransportJobPhase::ToPickup {
                    2
                } else {
                    3
                },
                4,
                title,
                format!(
                    "{} · {}. Drive the assigned vehicle to {} and stop before serving the stop. Driving shows navigation, cargo and the exact service action.{}",
                    job.name,
                    if a.phase == TransportJobPhase::Returning {
                        "returning reserved goods"
                    } else {
                        "funded work in progress"
                    },
                    target.map_or("its assigned stop", |f| f.name.as_str()),
                    if a.phase == TransportJobPhase::Returning {
                        " Returning protects the reserved cargo and escrow; it is not a completed paid delivery."
                    } else {
                        " Payment follows physical arrival, not merely choosing a route."
                    }
                ),
                "Continue the assigned driving job",
                GuideAction::OpenDriving {
                    job_id: job.id,
                    vehicle_id: a.vehicle_id,
                },
            );
        }
        let completed = self
            .state
            .transport
            .drivers
            .get(&self.player)
            .map_or(0, |p| p.jobs_completed);
        if completed > 0 {
            return ActivityGuidance::new(
                GuideTrack::Road,
                4,
                4,
                format!("{completed} driving jobs completed"),
                "Keep driving funded jobs or build a company: register, deposit personal funds, hire a driver, create a route and assign a suitable vehicle. Fleet explains real payroll, private stock and recovery.",
                "Inspect company opportunities",
                GuideAction::Open(GuideSurface::Fleet),
            );
        }
        if self
            .state
            .transport
            .vehicles
            .values()
            .any(|v| v.owner == self.player)
        {
            ActivityGuidance::new(
                GuideTrack::Road,
                1,
                4,
                "Choose a funded driving job",
                "Open Driving, select your owned vehicle and a job it can carry, then accept. Enter the vehicle, collect at pickup, follow its route and serve the destination. Licenses and real vehicle capacity still apply.",
                "Choose a driving job",
                GuideAction::Open(GuideSurface::Driving),
            )
        } else {
            ActivityGuidance::new(
                GuideTrack::Road,
                1,
                4,
                "Find a vehicle you can use",
                "No owned vehicle is visible. Visit your parked vehicle or inspect the garage's actual prices, rental deposits and license requirements. Company-reserved vehicles must be released before personal use.",
                "Visit Garage",
                GuideAction::Open(GuideSurface::Garage),
            )
        }
    }
}
