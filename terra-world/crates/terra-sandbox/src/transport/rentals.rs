//! Explicit prepaid rentals. Time, bond and fuel are durable private ledgers;
//! blocked or expired vehicles retain all jobs and shipments until recovery.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RentalOffer {
    pub fee: u64,
    pub bond: u64,
    pub fuel_cost: u64,
    pub total: u64,
    pub seconds: u32,
}
pub fn rental_offer(model: VehicleModel) -> RentalOffer {
    let fee = model.spec().price / 100;
    let bond = model.spec().price / 4;
    let fuel_cost = 1_500; // Ten litres, debited from the supplying garage.
    RentalOffer {
        fee,
        bond,
        fuel_cost,
        total: fee + bond + fuel_cost,
        seconds: 300,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RentalReturnQuote {
    pub damage_cost: u64,
    pub bond_refund: u64,
    pub fuel_refund: u64,
    pub total_refund: u64,
}

impl SandboxState {
    pub(super) fn rent_vehicle(
        &mut self,
        owner: u64,
        model: VehicleModel,
        facility: u64,
        aggregate_units: usize,
        position: GeoPosition,
    ) -> Result<Outcome, SandboxError> {
        self.require_garage(owner, facility)?;
        self.require_license(owner, model.spec().required_license)?;
        if self.is_player_dormant(owner) {
            return Err(invalid("resume the owner before renting"));
        }
        if aggregate_units >= MAX_LIVING_UNITS
            || self.transport.vehicles.len() >= MAX_TRANSPORT_VEHICLES
            || self
                .transport
                .vehicles
                .values()
                .filter(|v| v.owner == owner)
                .count()
                >= MAX_PLAYER_VEHICLES
        {
            return Err(SandboxError::Capacity);
        }
        if self.transport.facilities[&facility].fuel_l < 10. {
            return Err(SandboxError::Insufficient("rental delivery fuel".into()));
        }
        let offer = rental_offer(model);
        let wallet = self.living.accounts[&owner]
            .money
            .checked_sub(offer.total)
            .ok_or_else(|| SandboxError::Insufficient("rental fee, fuel and bond".into()))?;
        let treasury = self
            .transport
            .treasury
            .checked_add(offer.fee + offer.fuel_cost)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        let id = self.living.make_unit(
            format!("{} rental", model.spec().name),
            ActorKind::Vehicle,
            UnitRole::Worker,
            Some(owner),
            position,
        )?;
        let mut vehicle = VehicleState::new(id, owner, model);
        vehicle.fuel_l = 10.;
        vehicle.parking_brake = true;
        vehicle.rental = Some(VehicleRental {
            facility_id: facility,
            bond_escrow: offer.bond,
            remaining_seconds: f64::from(offer.seconds),
        });
        self.transport.vehicles.insert(id, vehicle);
        self.living.accounts.get_mut(&owner).unwrap().money = wallet;
        self.transport.treasury = treasury;
        self.transport.facilities.get_mut(&facility).unwrap().fuel_l -= 10.;
        Ok(Outcome {message:"Rental ready: five physical minutes prepaid, ten litres supplied and security bond held. Renew remotely or return empty to this garage".into(),created_id:Some(id)})
    }

    pub(super) fn renew_vehicle_rental(
        &mut self,
        owner: u64,
        id: u64,
    ) -> Result<Outcome, SandboxError> {
        if self.is_player_dormant(owner) {
            return Err(invalid("resume the owner before renewing"));
        }
        let vehicle = self.owned_vehicle(owner, id)?;
        let rental = vehicle
            .rental
            .as_ref()
            .ok_or_else(|| invalid("this vehicle is not a rental"))?;
        let next = rental.remaining_seconds + 300.;
        if next > 3600. {
            return Err(invalid("at most one physical hour may be prepaid"));
        }
        let fee = rental_offer(vehicle.model).fee;
        self.charge_owner_paid_company_vehicle(
            owner,
            id,
            CompanyCashCategory::OwnerPaidLease,
            fee,
        )?;
        self.transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .rental
            .as_mut()
            .unwrap()
            .remaining_seconds = next;
        Ok(outcome(
            "Added five physical minutes. Existing load, route, fuel and bond remain intact",
        ))
    }

    pub fn rental_return_quote(
        &self,
        owner: u64,
        id: u64,
    ) -> Result<RentalReturnQuote, SandboxError> {
        let vehicle = self.owned_vehicle(owner, id)?;
        let rental = vehicle
            .rental
            .as_ref()
            .ok_or_else(|| invalid("this vehicle is not a rental"))?;
        let damage_cost =
            ((1. - f64::from(vehicle.condition)) * vehicle.model.spec().price as f64 * 0.45).ceil()
                as u64;
        let damage_cost = damage_cost.min(rental.bond_escrow);
        let bond_refund = rental.bond_escrow - damage_cost;
        let fuel_refund = (vehicle.fuel_l * 150.).floor() as u64;
        Ok(RentalReturnQuote {
            damage_cost,
            bond_refund,
            fuel_refund,
            total_refund: bond_refund + fuel_refund,
        })
    }

    pub(super) fn return_vehicle_rental(
        &mut self,
        owner: u64,
        id: u64,
        facility: u64,
        quoted_refund: u64,
    ) -> Result<Outcome, SandboxError> {
        if self.is_player_dormant(owner) {
            return Err(invalid("resume the owner before returning"));
        }
        self.require_vehicle_at_garage(owner, id, facility)?;
        let vehicle = self.owned_vehicle(owner, id)?;
        let rental = vehicle
            .rental
            .as_ref()
            .ok_or_else(|| invalid("this vehicle is not a rental"))?;
        if rental.facility_id != facility {
            return Err(invalid("return to the garage that supplied this rental"));
        }
        if self.is_company_vehicle(id)
            || self.vehicle_has_job(id)
            || self.transport.races.values().any(|r| r.vehicle_id == id)
            || !vehicle.cargo.is_empty()
            || vehicle.trailer.is_some()
            || self.living.accounts[&owner].active_unit == id
        {
            return Err(invalid(
                "exit the empty rental, finish or return its work, release company assignment and unhitch its trailer",
            ));
        }
        let quote = self.rental_return_quote(owner, id)?;
        if quote.total_refund != quoted_refund {
            return Err(invalid("rental refund changed; review the new quote"));
        }
        let fuel = self.transport.facilities[&facility].fuel_l + vehicle.fuel_l;
        if fuel > 1_000_000. {
            return Err(SandboxError::Capacity);
        }
        let treasury = self
            .transport
            .treasury
            .checked_add(quote.damage_cost)
            .and_then(|n| n.checked_sub(quote.fuel_refund))
            .filter(|n| *n <= MAX_MONEY)
            .ok_or_else(|| SandboxError::Insufficient("issuer fuel-refund funds".into()))?;
        let wallet = self.living.accounts[&owner]
            .money
            .checked_add(quote.total_refund)
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        self.transport.vehicles.remove(&id);
        self.living.units.remove(&id);
        self.transport.facilities.get_mut(&facility).unwrap().fuel_l = fuel;
        self.transport.treasury = treasury;
        self.living.accounts.get_mut(&owner).unwrap().money = wallet;
        Ok(outcome(format!(
            "Rental returned; {:.2} credits refunded after damage and remaining fuel accounting",
            quote.total_refund as f64 / 100.
        )))
    }
}
