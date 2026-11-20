//! Finite, paid roadside support without moving a vehicle or its shipment.
use super::*;

impl SandboxState {
    /// Quote a bounded service offer for an assigned vehicle. Funds, physical
    /// reach and stopped motion are checked when the command is accepted, so a
    /// remote UI can explain the price before its owner walks to the vehicle.
    pub fn company_roadside_quote(
        &self,
        owner: u64,
        vehicle_id: u64,
        facility_id: u64,
        litres: u16,
        repair: bool,
    ) -> Result<CompanyRoadsideQuote, SandboxError> {
        let company = self
            .transport
            .companies
            .get(&owner)
            .ok_or_else(|| invalid("register a transport company first"))?;
        let vehicle = self.owned_vehicle(owner, vehicle_id)?;
        if !company
            .drivers
            .iter()
            .any(|driver| driver.vehicle_id == Some(vehicle_id))
        {
            return Err(invalid(
                "roadside company service requires an assigned vehicle",
            ));
        }
        if self.is_player_dormant(owner) || !self.living.units.contains_key(&vehicle_id) {
            return Err(invalid(
                "resume the owner and hydrate the vehicle before roadside service",
            ));
        }
        let facility = self
            .transport
            .facilities
            .get(&facility_id)
            .filter(|facility| facility.kind == TransportFacilityKind::Garage)
            .ok_or_else(|| invalid("roadside service requires a supplying garage"))?;
        if litres > 500 || (litres == 0 && !repair) {
            return Err(invalid("request 1–500 litres or a needed repair"));
        }
        if vehicle.fuel_l + f64::from(litres) > vehicle.model.spec().fuel_capacity_l + 1e-6 {
            return Err(invalid("requested fuel exceeds remaining tank capacity"));
        }
        if facility.fuel_l < f64::from(litres) {
            return Err(SandboxError::Insufficient("supplying garage fuel".into()));
        }
        if repair && vehicle.condition >= 0.9999 {
            return Err(invalid("vehicle does not need repair"));
        }
        let fuel_cost = u64::from(litres) * COMPANY_ROADSIDE_FUEL_PRICE_PER_LITRE;
        let repair_cost = if repair { repair_price(vehicle) } else { 0 };
        let at_owned_depot = company
            .depots
            .iter()
            .any(|depot| depot.facility_id == facility_id)
            && near(self.living.units[&vehicle_id].position, facility.position);
        let callout_cost = if at_owned_depot {
            0
        } else {
            COMPANY_ROADSIDE_CALLOUT_PRICE
        };
        let total_cost = callout_cost
            .checked_add(fuel_cost)
            .and_then(|cost| cost.checked_add(repair_cost))
            .filter(|cost| *cost <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        Ok(CompanyRoadsideQuote {
            vehicle_id,
            facility_id,
            litres,
            repair,
            callout_cost,
            fuel_cost,
            repair_cost,
            total_cost,
        })
    }

    pub(super) fn service_company_vehicle(
        &mut self,
        owner: u64,
        vehicle_id: u64,
        facility_id: u64,
        litres: u16,
        repair: bool,
        quoted_total: u64,
    ) -> Result<Outcome, SandboxError> {
        let quote = self.company_roadside_quote(owner, vehicle_id, facility_id, litres, repair)?;
        if quote.total_cost != quoted_total {
            return Err(invalid(
                "roadside service quote changed; review the new price",
            ));
        }
        let body = &self.living.units[&vehicle_id];
        if body_speed(body) > 0.5 {
            return Err(invalid("stop the fleet vehicle before roadside service"));
        }
        if !near(self.players[&owner].position, body.position) {
            return Err(SandboxError::OutOfReach);
        }
        let cash = self.transport.companies[&owner]
            .cash
            .checked_sub(quote.total_cost)
            .ok_or_else(|| SandboxError::Insufficient("company roadside service funds".into()))?;
        let treasury = self
            .transport
            .treasury
            .checked_add(quote.total_cost)
            .filter(|cash| *cash <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        // Every admission check precedes mutation. The surrounding transport
        // candidate also validates before publication. Shipments, clocks,
        // orders, driver pause state and movement evidence are untouched.
        let accounting = self.transport.companies[&owner].prepare_accounting(
            self.transport.elapsed_seconds,
            &[
                (CompanyCashCategory::ServiceCallout, quote.callout_cost),
                (CompanyCashCategory::Fuel, quote.fuel_cost),
                (CompanyCashCategory::Repairs, quote.repair_cost),
            ],
        )?;
        let company = self.transport.companies.get_mut(&owner).unwrap();
        company.cash = cash;
        company.accounting = Some(accounting);
        self.transport.treasury = treasury;
        self.transport
            .facilities
            .get_mut(&facility_id)
            .unwrap()
            .fuel_l -= f64::from(litres);
        let vehicle = self.transport.vehicles.get_mut(&vehicle_id).unwrap();
        vehicle.fuel_l += f64::from(litres);
        if repair {
            vehicle.condition = 1.;
        }
        Ok(outcome(format!(
            "Roadside service completed for {:.2} company credits; shipment and route preserved",
            quote.total_cost as f64 / 100.
        )))
    }
}
