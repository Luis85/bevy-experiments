//! Derived public demand. Accepted work keeps its original delivery commitment.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) enum SupplyDemand<'a> {
    /// A missing or replaced receiver is not proof that its demand disappeared.
    Unknown,
    /// The observed receiver no longer requests this item.
    Obsolete,
    /// Current useful need. Inventory admission is evaluated only for posting;
    /// per-frame readiness and guidance need only the classification.
    Needed {
        priority: u8,
        amount: u32,
        stock: &'a Inventory,
        unit_limit: Option<u64>,
    },
}

impl SupplyDemand<'_> {
    pub(super) fn posting_amount(self, item: &str) -> Option<(u8, u32)> {
        let Self::Needed {
            priority,
            amount,
            stock,
            unit_limit,
        } = self
        else {
            return None;
        };
        let amount = fitting_amount(stock, item, amount, unit_limit);
        (amount > 0).then_some((priority, amount))
    }
}

impl SandboxState {
    /// Use the same stock targets and capacity admission for posting and public
    /// offer maintenance. A temporarily full receiver still has demand; that
    /// alone must not revoke an existing funded offer or accepted commitment.
    pub(super) fn supply_demand(
        &self,
        target: u64,
        position: GeoPosition,
        item: &str,
    ) -> SupplyDemand<'_> {
        if let Some(site) = self.living.producers.get(&target) {
            if site.position != position {
                return SupplyDemand::Unknown;
            }
            let Some(per_cycle) = site.inputs.get(item).filter(|_| site.enabled) else {
                return SupplyDemand::Obsolete;
            };
            let current = site.stock.get(item).copied().unwrap_or(0);
            let missing = per_cycle.saturating_mul(12).saturating_sub(current);
            if missing == 0 {
                return SupplyDemand::Obsolete;
            }
            return SupplyDemand::Needed {
                priority: u8::from(current >= *per_cycle),
                amount: missing.min(12),
                stock: &site.stock,
                unit_limit: Some(2000),
            };
        }
        if let Some(shop) = self.living.shops.get(&target) {
            if shop.position != position {
                return SupplyDemand::Unknown;
            }
            let current = shop.stock.get(item).copied().unwrap_or(0);
            if !shop.prices.contains_key(item) || current >= 40 {
                return SupplyDemand::Obsolete;
            }
            return SupplyDemand::Needed {
                priority: 2,
                amount: (40 - current).min(12),
                stock: &shop.stock,
                unit_limit: None,
            };
        }
        SupplyDemand::Unknown
    }

    pub(crate) fn is_obsolete_public_supply(&self, contract: &SupplyContract) -> bool {
        contract.claimed_by.is_none()
            && matches!(
                self.supply_demand(
                    contract.target_site,
                    contract.target_position,
                    &contract.item,
                ),
                SupplyDemand::Obsolete
            )
    }
}

fn fitting_amount(stock: &Inventory, item: &str, requested: u32, unit_limit: Option<u64>) -> u32 {
    (1..=requested.min(12))
        .rev()
        .find(|amount| {
            exchange(stock, &Inventory::new(), &inventory(&[(item, *amount)]), 1).is_ok_and(
                |next| {
                    unit_limit.is_none_or(|limit| {
                        next.values().map(|amount| u64::from(*amount)).sum::<u64>() <= limit
                    })
                },
            )
        })
        .unwrap_or(0)
}
