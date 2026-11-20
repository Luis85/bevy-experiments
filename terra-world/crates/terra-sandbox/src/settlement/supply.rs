//! Read-only supply admission and the exact preparation committed by authority.
//! A successful inspection is not a reservation: submit the ordinary command,
//! which prepares again against the current authoritative state.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyContractAction {
    Accept,
    Deliver,
    Return,
}

/// The first unmet condition for the named action, in deterministic order.
/// Destination absence in a recipient replica is deliberately not described
/// as permanent deletion; only authority has the complete receiving inventory.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SupplyContractBlocker {
    #[error("Your character is not available. Wait for the world to finish loading.")]
    PlayerMissing,
    #[error("Resume your session before changing supply work.")]
    PlayerDormant,
    #[error("Your living account is not available. Wait for local services to load.")]
    AccountUnavailable,
    #[error("This supply request is no longer available. Review another funded request.")]
    ContractMissing,
    #[error("This request is already accepted. Review its current owner and status.")]
    AlreadyAccepted,
    #[error(
        "This request is not accepted by you. Accept an available request before delivering or returning it."
    )]
    NotAccepted,
    #[error(
        "This supply request has expired. Return accepted work or wait for the noticeboard to refresh; your carried goods remain yours."
    )]
    Expired,
    #[error(
        "This public request is no longer needed. Wait for the noticeboard to refresh or choose another funded request; its reserved reward stays protected until the issuer can receive it."
    )]
    ObsoleteOffer,
    #[error(
        "You already have three accepted supply requests. Complete or return one before accepting another."
    )]
    ActiveLimit,
    #[error("Carry {needed} {item} before delivery; you currently have {carried}.")]
    MissingGoods {
        item: String,
        needed: u32,
        carried: u32,
    },
    #[error(
        "The receiving service is not in the current view or is unavailable. Travel to the recorded destination and wait for it to load; authority checks again before delivery."
    )]
    DestinationUnavailable,
    #[error(
        "The receiving service cannot fit this delivery. Wait for its stock to make room, or explicitly return the request. Your goods and reserved reward remain unchanged."
    )]
    DestinationCapacity,
    #[error(
        "Your wallet cannot fit the reserved reward. Spend existing credits or transfer them through an available funding action, then try again; returning the request spends no goods."
    )]
    WalletCapacity,
    #[error("Move your active body within 20 m of the recorded delivery destination.")]
    OutOfReach,
    #[error(
        "The requested item is unavailable in this catalog. Return the request or ask the world operator to inspect its content."
    )]
    CatalogItemUnavailable,
    #[error(
        "This request or inventory cannot be prepared safely. Preserve your progress and ask the world operator to inspect it; no goods or reward were moved."
    )]
    InvalidState,
}

impl SupplyContractBlocker {
    pub(super) fn into_sandbox_error(self) -> SandboxError {
        match self {
            Self::PlayerMissing => SandboxError::PlayerMissing,
            Self::ContractMissing => SandboxError::Missing("supply contract".into()),
            Self::NotAccepted => SandboxError::NotOwner,
            Self::MissingGoods { item, .. } => SandboxError::Insufficient(item),
            Self::DestinationUnavailable => SandboxError::Missing("contract destination".into()),
            Self::DestinationCapacity | Self::WalletCapacity | Self::ActiveLimit => {
                SandboxError::Capacity
            }
            Self::OutOfReach => SandboxError::OutOfReach,
            other => SandboxError::Invalid(other.to_string()),
        }
    }
}

pub(super) struct PreparedSupplyDelivery {
    pub contract: SupplyContract,
    pub player_stock: Inventory,
    pub receiver_stock: Inventory,
    pub wallet: u64,
}

impl SandboxState {
    /// Inspect the ordinary command's current preconditions without changing
    /// stock, escrow, ownership, progression or time. This accepts complete
    /// authority or a recipient replica; a replica's success cannot reserve
    /// capacity, and an absent destination does not prove global absence.
    pub fn supply_contract_readiness(
        &self,
        player: u64,
        contract_id: u64,
        action: SupplyContractAction,
        catalog: &Catalog,
    ) -> Result<(), SupplyContractBlocker> {
        match action {
            SupplyContractAction::Accept => self.prepare_supply_acceptance(player, contract_id),
            SupplyContractAction::Deliver => self
                .prepare_supply_delivery(player, contract_id, catalog)
                .map(|_| ()),
            SupplyContractAction::Return => self.prepare_supply_return(player, contract_id),
        }
    }

    fn supply_context(
        &self,
        player: u64,
        contract_id: u64,
    ) -> Result<&SupplyContract, SupplyContractBlocker> {
        if !self.players.contains_key(&player) {
            return Err(SupplyContractBlocker::PlayerMissing);
        }
        if self.is_player_dormant(player) {
            return Err(SupplyContractBlocker::PlayerDormant);
        }
        if !self.living.enabled || !self.living.accounts.contains_key(&player) {
            return Err(SupplyContractBlocker::AccountUnavailable);
        }
        self.settlements
            .contracts
            .get(&contract_id)
            .ok_or(SupplyContractBlocker::ContractMissing)
    }

    pub(super) fn prepare_supply_acceptance(
        &self,
        player: u64,
        contract_id: u64,
    ) -> Result<(), SupplyContractBlocker> {
        let contract = self.supply_context(player, contract_id)?;
        if contract.claimed_by.is_some() {
            return Err(SupplyContractBlocker::AlreadyAccepted);
        }
        if self.settlements.elapsed_seconds >= contract.expires_at {
            return Err(SupplyContractBlocker::Expired);
        }
        if self.is_obsolete_public_supply(contract) {
            return Err(SupplyContractBlocker::ObsoleteOffer);
        }
        if self
            .settlements
            .contracts
            .values()
            .filter(|contract| contract.claimed_by == Some(player))
            .count()
            >= MAX_ACTIVE_CONTRACTS
        {
            return Err(SupplyContractBlocker::ActiveLimit);
        }
        Ok(())
    }

    pub(super) fn prepare_supply_return(
        &self,
        player: u64,
        contract_id: u64,
    ) -> Result<(), SupplyContractBlocker> {
        let contract = self.supply_context(player, contract_id)?;
        if contract.claimed_by != Some(player) {
            return Err(SupplyContractBlocker::NotAccepted);
        }
        Ok(())
    }

    pub(super) fn prepare_supply_delivery(
        &self,
        player: u64,
        contract_id: u64,
        catalog: &Catalog,
    ) -> Result<PreparedSupplyDelivery, SupplyContractBlocker> {
        let contract = self.supply_context(player, contract_id)?;
        if contract.claimed_by != Some(player) {
            return Err(SupplyContractBlocker::NotAccepted);
        }
        if self.settlements.elapsed_seconds >= contract.expires_at {
            return Err(SupplyContractBlocker::Expired);
        }
        if !catalog.items.contains(&contract.item) {
            return Err(SupplyContractBlocker::CatalogItemUnavailable);
        }
        let principal = &self.players[&player];
        let carried = principal
            .inventory
            .get(&contract.item)
            .copied()
            .unwrap_or(0);
        if carried < contract.amount {
            return Err(SupplyContractBlocker::MissingGoods {
                item: contract.item.clone(),
                needed: contract.amount,
                carried,
            });
        }
        let goods = inventory(&[(&contract.item, contract.amount)]);
        let player_stock = exchange(&principal.inventory, &goods, &Inventory::new(), 1)
            .map_err(|_| SupplyContractBlocker::InvalidState)?;
        let receiver = self
            .living
            .producers
            .get(&contract.target_site)
            .map(|site| (&site.stock, site.position, true))
            .or_else(|| {
                self.living
                    .shops
                    .get(&contract.target_site)
                    .map(|shop| (&shop.stock, shop.position, false))
            })
            .ok_or(SupplyContractBlocker::DestinationUnavailable)?;
        if receiver.1 != contract.target_position {
            return Err(SupplyContractBlocker::InvalidState);
        }
        let receiver_stock = exchange(receiver.0, &Inventory::new(), &goods, 1)
            .map_err(|_| SupplyContractBlocker::DestinationCapacity)?;
        if receiver.2 && receiver_stock.values().map(|n| u64::from(*n)).sum::<u64>() > 2000 {
            return Err(SupplyContractBlocker::DestinationCapacity);
        }
        let wallet = self.living.accounts[&player]
            .money
            .checked_add(contract.reward)
            .filter(|money| *money <= MAX_MONEY)
            .ok_or(SupplyContractBlocker::WalletCapacity)?;
        super::commands::require_reach(principal.position, contract.target_position).map_err(
            |error| match error {
                SandboxError::OutOfReach => SupplyContractBlocker::OutOfReach,
                _ => SupplyContractBlocker::InvalidState,
            },
        )?;
        Ok(PreparedSupplyDelivery {
            contract: contract.clone(),
            player_stock,
            receiver_stock,
            wallet,
        })
    }
}
