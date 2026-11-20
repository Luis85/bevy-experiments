//! Private finite storage bays at existing garages or freight facilities. Source-world facilities are
//! never acquired or converted into private source data by opening a bay.
use super::*;

impl SandboxState {
    pub(super) fn open_company_depot(
        &mut self,
        owner: u64,
        facility: u64,
    ) -> Result<Outcome, SandboxError> {
        self.require_company_depot_reach(owner, facility)?;
        let company = &self.transport.companies[&owner];
        if company.depots.len() >= MAX_COMPANY_DEPOTS {
            return Err(SandboxError::Capacity);
        }
        if company.depots.iter().any(|d| d.facility_id == facility) {
            return Err(invalid("company already has a depot at this facility"));
        }
        let cash = company
            .cash
            .checked_sub(COMPANY_DEPOT_PRICE)
            .ok_or_else(|| SandboxError::Insufficient("company depot funds".into()))?;
        let treasury = self
            .transport
            .treasury
            .checked_add(COMPANY_DEPOT_PRICE)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        let accounting = company.prepare_accounting(
            self.transport.elapsed_seconds,
            &[(CompanyCashCategory::DepotAccess, COMPANY_DEPOT_PRICE)],
        )?;
        let company = self.transport.companies.get_mut(&owner).unwrap();
        company.cash = cash;
        company.accounting = Some(accounting);
        company.depots.push(CompanyDepot {
            facility_id: facility,
            stock: Inventory::new(),
        });
        self.transport.treasury = treasury;
        Ok(outcome(
            "Depot bay opened: 2,000 units of private storage. Use depot freight to supply a shop or transfer between your bays; garage bays also waive onsite service callout",
        ))
    }

    pub(super) fn close_company_depot(
        &mut self,
        owner: u64,
        facility: u64,
    ) -> Result<Outcome, SandboxError> {
        self.require_company_depot_reach(owner, facility)?;
        let company = &self.transport.companies[&owner];
        let depot = company
            .depots
            .iter()
            .find(|d| d.facility_id == facility)
            .ok_or_else(|| invalid("owned company depot"))?;
        if !depot.stock.is_empty() {
            return Err(invalid("withdraw all depot goods before closing the bay"));
        }
        if company.routes.iter().any(|r| {
            matches!(
                r.service,
                FleetService::DepotFreight { .. } | FleetService::DepotTransfer { .. }
            ) && (r.stops.first() == Some(&facility) || r.stops.last() == Some(&facility))
        }) {
            return Err(invalid(
                "delete depot freight routes using this bay before closing it",
            ));
        }
        self.transport
            .companies
            .get_mut(&owner)
            .unwrap()
            .depots
            .retain(|d| d.facility_id != facility);
        Ok(outcome(
            "Empty depot closed. Opening fees are not refunded; company routes and employee wages are retained",
        ))
    }

    pub(super) fn transfer_company_depot_stock(
        &mut self,
        owner: u64,
        facility: u64,
        item: &str,
        amount: u32,
        deposit: bool,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        self.require_company_depot_reach(owner, facility)?;
        if amount == 0
            || u64::from(amount) > COMPANY_DEPOT_CAPACITY
            || !catalog.items.contains(item)
        {
            return Err(invalid("depot transfer item or quantity"));
        }
        let account = &self.living.accounts[&owner];
        if account.active_unit != account.avatar_unit {
            return Err(invalid("leave the vehicle before moving depot goods"));
        }
        let depot = self.transport.companies[&owner]
            .depots
            .iter()
            .find(|d| d.facility_id == facility)
            .ok_or_else(|| invalid("owned company depot"))?;
        let goods = inventory(&[(item, amount)]);
        let player_stock = &self.players[&owner].inventory;
        let (player_next, depot_next) = if deposit {
            (
                exchange(player_stock, &goods, &Inventory::new(), 1)?,
                exchange(&depot.stock, &Inventory::new(), &goods, 1)?,
            )
        } else {
            (
                exchange(player_stock, &Inventory::new(), &goods, 1)?,
                exchange(&depot.stock, &goods, &Inventory::new(), 1)?,
            )
        };
        self.validate_company_depot_stock(owner, facility, &depot_next, None)?;
        let mut accounting = self.transport.companies[&owner]
            .prepare_accounting(self.transport.elapsed_seconds, &[])?;
        let transferred = if deposit {
            &mut accounting.unpriced_stock_in_units
        } else {
            &mut accounting.unpriced_stock_out_units
        };
        *transferred = transferred
            .checked_add(u64::from(amount))
            .filter(|n| *n <= MAX_COMPANY_ACCOUNTING_TOTAL)
            .ok_or(SandboxError::Capacity)?;
        self.transport.companies.get_mut(&owner).unwrap().accounting = Some(accounting);
        self.players.get_mut(&owner).unwrap().inventory = player_next;
        self.transport
            .companies
            .get_mut(&owner)
            .unwrap()
            .depots
            .iter_mut()
            .find(|d| d.facility_id == facility)
            .unwrap()
            .stock = depot_next;
        Ok(outcome(if deposit {
            "Goods stored in your company depot"
        } else {
            "Goods withdrawn from your company depot"
        }))
    }

    pub(super) fn require_company_depot_reach(
        &self,
        owner: u64,
        facility: u64,
    ) -> Result<(), SandboxError> {
        let position = self
            .players
            .get(&owner)
            .ok_or(SandboxError::PlayerMissing)?
            .position;
        let facility = self
            .transport
            .facilities
            .get(&facility)
            .ok_or_else(|| invalid("depot facility"))?;
        if !matches!(
            facility.kind,
            TransportFacilityKind::Garage | TransportFacilityKind::Depot
        ) {
            return Err(invalid("open a bay at a garage or freight facility"));
        }
        if !near(position, facility.position) {
            return Err(SandboxError::OutOfReach);
        }
        Ok(())
    }

    pub(super) fn company_depot_stock(
        &self,
        owner: u64,
        facility: u64,
    ) -> Result<&Inventory, SandboxError> {
        self.transport
            .companies
            .get(&owner)
            .and_then(|c| c.depots.iter().find(|d| d.facility_id == facility))
            .map(|d| &d.stock)
            .ok_or_else(|| invalid("owned company depot"))
    }

    pub(super) fn company_depot_stock_mut(
        &mut self,
        owner: u64,
        facility: u64,
    ) -> Result<&mut Inventory, SandboxError> {
        self.transport
            .companies
            .get_mut(&owner)
            .and_then(|c| c.depots.iter_mut().find(|d| d.facility_id == facility))
            .map(|d| &mut d.stock)
            .ok_or_else(|| invalid("owned company depot"))
    }

    /// Loaded depot work reserves its source refund space and private destination
    /// space. A later manual transfer cannot strand a shipment by filling its bay.
    pub fn company_depot_reserved_units(&self, owner: u64, facility: u64) -> u64 {
        self.company_depot_reservations_except(owner, facility, None)
    }

    pub(super) fn company_depot_reservations_except(
        &self,
        owner: u64,
        facility: u64,
        except_driver: Option<u64>,
    ) -> u64 {
        self.transport
            .companies
            .get(&owner)
            .into_iter()
            .flat_map(|c| &c.drivers)
            .filter(|d| Some(d.id) != except_driver)
            .filter_map(|d| d.trip.as_ref())
            .filter_map(|t| match &t.service {
                FleetService::DepotFreight { amount, .. } if t.pickup == facility => {
                    Some(u64::from(*amount))
                }
                FleetService::DepotTransfer { amount, .. }
                    if t.pickup == facility || (!t.returning && t.destination == facility) =>
                {
                    Some(u64::from(*amount))
                }
                _ => None,
            })
            .sum()
    }

    pub(super) fn validate_company_depot_stock(
        &self,
        owner: u64,
        facility: u64,
        stock: &Inventory,
        except_driver: Option<u64>,
    ) -> Result<(), SandboxError> {
        crate::catalog::validate_inventory(stock)?;
        let used = stock.values().map(|n| u64::from(*n)).sum::<u64>();
        let reserved = self.company_depot_reservations_except(owner, facility, except_driver);
        if used + reserved > COMPANY_DEPOT_CAPACITY {
            return Err(invalid(
                "depot storage is full or reserved for a loaded delivery/return",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_company_depot_route(
        &self,
        owner: u64,
        route: &CompanyRoute,
        item: &str,
    ) -> Result<(), SandboxError> {
        self.company_depot_stock(owner, route.stops[0])?;
        let destination = *route
            .stops
            .last()
            .ok_or_else(|| invalid("depot freight destination"))?;
        if matches!(route.service, FleetService::DepotTransfer { .. }) {
            self.company_depot_stock(owner, destination)?;
            return Ok(());
        }
        let shop = self
            .transport
            .facilities
            .get(&destination)
            .and_then(|f| f.linked_site)
            .and_then(|id| self.living.shops.get(&id))
            .ok_or_else(|| invalid("warehouse sale destination must be a buying shop"))?;
        if !shop.prices.contains_key(item) {
            return Err(invalid("destination shop does not buy this item"));
        }
        Ok(())
    }

    pub(super) fn company_freight_delivery_stock(
        &self,
        owner: u64,
        facility: u64,
        goods: &Inventory,
        except_driver: Option<u64>,
    ) -> Result<Inventory, SandboxError> {
        if let Ok(stock) = self.company_depot_stock(owner, facility) {
            let next = exchange(stock, &Inventory::new(), goods, 1)?;
            self.validate_company_depot_stock(owner, facility, &next, except_driver)?;
            Ok(next)
        } else {
            self.transport_delivery_stock(facility, goods)
        }
    }

    pub(super) fn set_company_freight_stock(
        &mut self,
        owner: u64,
        facility: u64,
        stock: Inventory,
    ) -> Result<(), SandboxError> {
        if self.company_depot_stock(owner, facility).is_ok() {
            *self.company_depot_stock_mut(owner, facility)? = stock;
        } else {
            *self.transport_stock_mut(facility)? = stock;
        }
        Ok(())
    }
}
