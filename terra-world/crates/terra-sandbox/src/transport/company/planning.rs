//! Read-only route arithmetic from the currently visible world. A preview is
//! neither a dispatch preflight nor a reservation of stock, demand or payment.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CompanyRoutePreview {
    pub pickup: u64,
    pub destination: u64,
    pub requested_load: u32,
    pub direct_distance_m: f64,
    /// Receipt for the entire requested load at today's issuer/buyer price.
    /// None means the buying shop or its price is absent from this view.
    pub full_load_receipt: Option<u64>,
    pub public_fare: bool,
    /// Observed stock/queue, before other commitments or capacity checks.
    pub visible_pickup_units: Option<u32>,
    pub visible_destination_units: Option<u32>,
    pub destination_target_units: Option<u32>,
    pub buyer_unit_price: Option<u64>,
    /// Lowest visible retail unit price with some stock. The named seller may
    /// lack a full load or be far away; this is a comparison, not a trade quote.
    pub retail_comparison: Option<CompanyRouteRetailComparison>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyRouteRetailComparison {
    pub shop_id: u64,
    pub unit_price: u64,
    pub available_units: u32,
    pub full_load_cost: u64,
    /// Buyer receipt minus this hypothetical retail acquisition, before every
    /// operating cost. None when the buyer price is not visible.
    pub merchandise_margin: Option<i64>,
}

impl CompanyRoutePreview {
    /// Maximum paid travel time that the full receipt alone could cover.
    /// Fuel, acquisition, empty returns, setup and vehicle costs still matter.
    /// This is not a journey duration or a profit/break-even forecast.
    pub fn receipt_payroll_seconds(&self) -> Option<f64> {
        self.full_load_receipt
            .map(|amount| amount as f64 / COMPANY_WAGE_PER_SECOND)
    }
}

impl SandboxState {
    /// Describe the FIRST departure of a draft or saved route using current,
    /// visible prices and inventories. Passenger routes use their first two
    /// stops; freight uses the first and last. This method deliberately does
    /// not inspect private issuer cash, infer absent inventories as empty, or
    /// promise that competing reservations/road conditions permit dispatch.
    pub fn company_route_preview(
        &self,
        owner: u64,
        stops: &[u64],
        service: &FleetService,
    ) -> Result<CompanyRoutePreview, SandboxError> {
        // Reuse the route's bounded shape/quantity/distance contract. Identity
        // and name are irrelevant to a projection; no company is created.
        CompanyRoute {
            id: 1,
            name: "Route preview".into(),
            stops: stops.to_vec(),
            service: service.clone(),
        }
        .validate(&self.transport.facilities, None)?;
        let pickup = stops[0];
        let destination = if matches!(service, FleetService::Passengers { .. }) {
            stops[1]
        } else {
            *stops.last().unwrap()
        };
        let source = &self.transport.facilities[&pickup];
        let target = &self.transport.facilities[&destination];
        let public_fare = !service.uses_company_stock();
        let mut preview = CompanyRoutePreview {
            pickup,
            destination,
            requested_load: match service {
                FleetService::Passengers { count } => u32::from(*count),
                _ => service.cargo_load().unwrap().1,
            },
            direct_distance_m: source.position.surface_distance_m(target.position),
            full_load_receipt: public_fare
                .then(|| fleet_fare(service, source.position, target.position)),
            public_fare,
            visible_pickup_units: None,
            visible_destination_units: None,
            destination_target_units: None,
            buyer_unit_price: None,
            retail_comparison: None,
        };
        match service {
            FleetService::Passengers { .. } => {
                preview.visible_pickup_units = Some(source.passengers_waiting);
                preview.visible_destination_units = Some(target.passengers_waiting);
            }
            FleetService::Freight { item, .. } => {
                preview.visible_pickup_units = self
                    .transport_stock(pickup)
                    .ok()
                    .map(|stock| stock.get(item).copied().unwrap_or(0));
                preview.visible_destination_units = self
                    .transport_stock(destination)
                    .ok()
                    .map(|stock| stock.get(item).copied().unwrap_or(0));
                preview.destination_target_units = preview
                    .visible_destination_units
                    .map(|_| self.fleet_destination_target(destination, item));
            }
            FleetService::DepotFreight { item, amount } => {
                preview.visible_pickup_units = self
                    .company_depot_stock(owner, pickup)
                    .ok()
                    .map(|stock| stock.get(item).copied().unwrap_or(0));
                if let Some(shop) = target.linked_site.and_then(|id| self.living.shops.get(&id)) {
                    preview.visible_destination_units =
                        Some(shop.stock.get(item).copied().unwrap_or(0));
                    preview.destination_target_units =
                        Some(self.fleet_destination_target(destination, item));
                    if let Some(price) = shop.prices.get(item) {
                        preview.buyer_unit_price = Some(price.sell);
                        preview.full_load_receipt = Some(
                            price
                                .sell
                                .checked_mul(u64::from(*amount))
                                .ok_or(SandboxError::Capacity)?,
                        );
                    }
                }
                let retail = self
                    .living
                    .shops
                    .values()
                    .filter_map(|shop| {
                        let stock = shop.stock.get(item).copied().unwrap_or(0);
                        let price = shop.prices.get(item)?;
                        (stock > 0).then_some((price.buy, shop.id, stock))
                    })
                    .min_by_key(|(price, id, _)| (*price, *id));
                if let Some((unit_price, shop_id, available_units)) = retail {
                    let full_load_cost = unit_price
                        .checked_mul(u64::from(*amount))
                        .ok_or(SandboxError::Capacity)?;
                    let merchandise_margin = preview
                        .full_load_receipt
                        .map(|receipt| {
                            i64::try_from(receipt)
                                .ok()
                                .zip(i64::try_from(full_load_cost).ok())
                                .and_then(|(receipt, cost)| receipt.checked_sub(cost))
                                .ok_or(SandboxError::Capacity)
                        })
                        .transpose()?;
                    preview.retail_comparison = Some(CompanyRouteRetailComparison {
                        shop_id,
                        unit_price,
                        available_units,
                        full_load_cost,
                        merchandise_margin,
                    });
                }
            }
            FleetService::DepotTransfer { item, .. } => {
                preview.full_load_receipt = Some(0);
                preview.visible_pickup_units = self
                    .company_depot_stock(owner, pickup)
                    .ok()
                    .map(|stock| stock.get(item).copied().unwrap_or(0));
                preview.visible_destination_units = self
                    .company_depot_stock(owner, destination)
                    .ok()
                    .map(|stock| stock.get(item).copied().unwrap_or(0));
                // Capacity is mixed-item and includes in-flight reservations;
                // an item count must never be mistaken for free bay space.
            }
        }
        Ok(preview)
    }
}
