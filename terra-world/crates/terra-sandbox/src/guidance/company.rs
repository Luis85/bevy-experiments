//! A continuing owner-private service briefing. It observes the ordinary fleet
//! state; opening it cannot create funds, reserve stock or restart a vehicle.
use super::*;

fn needs_attention(status: FleetStatus) -> bool {
    !matches!(
        status,
        FleetStatus::Unassigned
            | FleetStatus::Paused
            | FleetStatus::Driving
            | FleetStatus::Delivering
    )
}

fn service_priority(driver: &CompanyDriver) -> (u8, u8, u64) {
    // A blocked loaded commitment comes before an empty waiting service. Idle
    // spare employees must not hide an already assigned vehicle's problem.
    let rank = if needs_attention(driver.status) {
        0
    } else if driver.trip.is_some() {
        1
    } else if driver.running {
        2
    } else {
        3
    };
    (rank, u8::from(driver.trip.is_none()), driver.id)
}

impl GuideContext<'_> {
    fn company_action(
        &self,
        section: GuideCompanySection,
        driver: Option<&CompanyDriver>,
    ) -> GuideAction {
        let facility_id = driver.and_then(|driver| {
            if section == GuideCompanySection::Garage {
                return driver
                    .vehicle_id
                    .and_then(|id| self.state.transport.vehicles.get(&id))
                    .and_then(|vehicle| vehicle.rental.as_ref())
                    .map(|rental| rental.facility_id);
            }
            driver
                .trip
                .as_ref()
                .filter(|trip| trip.returning)
                .map(|trip| trip.pickup)
                .or_else(|| {
                    self.state
                        .transport
                        .companies
                        .get(&self.player)?
                        .routes
                        .iter()
                        .find(|route| Some(route.id) == driver.route_id)?
                        .stops
                        .get(driver.next_stop)
                        .copied()
                })
        });
        GuideAction::OpenCompany(GuideCompanyFocus {
            owner: self.player,
            section,
            driver_id: driver.map(|d| d.id),
            vehicle_id: driver.and_then(|d| d.vehicle_id),
            route_id: driver.and_then(|d| d.route_id),
            facility_id,
            next_stop: driver.map(|d| d.next_stop),
            status: driver.map(|d| d.status),
            running: driver.is_some_and(|d| d.running),
            trip: driver.and_then(|d| d.trip.clone()),
        })
    }

    pub(super) fn company(&self) -> ActivityGuidance {
        let Some(company) = self.state.transport.companies.get(&self.player) else {
            return ActivityGuidance::new(
                GuideTrack::Company,
                1,
                5,
                "Plan your first transport company",
                format!(
                    "Registration at a garage costs {} personal credits; your wallet holds {}. After registration, deposit operating cash, hire a driver, compare a route's first departure and assign a suitable owned or rented vehicle. Registration alone earns no fare.",
                    money(COMPANY_REGISTRATION_PRICE),
                    money(self.wallet())
                ),
                "Review company setup",
                self.company_action(GuideCompanySection::Company, None),
            );
        };
        let assigned = company.drivers.iter().filter(|d| d.vehicle_id.is_some());
        if let Some(driver) = assigned.min_by_key(|d| service_priority(d)) {
            return self.company_service(company, driver);
        }
        let summary = format!(
            "{} · company cash {} cr · personal wallet {} cr.",
            company.name,
            money(company.cash),
            money(self.wallet())
        );
        let needed = if company.drivers.is_empty() {
            COMPANY_HIRE_PRICE
        } else {
            1
        };
        if company.cash < needed {
            return ActivityGuidance::new(
                GuideTrack::Company,
                2,
                5,
                "Fund the next company step",
                format!(
                    "{summary} Deposit personal money before hiring or dispatching. Hiring costs {} company credits; travel payroll is {:.2} credits per driver per physical minute, before fuel and vehicle costs. Deposits are transfers, not earnings.{}",
                    money(COMPANY_HIRE_PRICE),
                    COMPANY_WAGE_PER_SECOND * 60. / 100.,
                    if self.wallet() < needed.saturating_sub(company.cash) {
                        " Your wallet cannot cover this step yet. Earn personal income from funded work, then return."
                    } else {
                        " Keep cash available for the whole journey and any return."
                    }
                ),
                "Review company funding",
                self.company_action(GuideCompanySection::Company, None),
            );
        }
        if company.drivers.is_empty() {
            return ActivityGuidance::new(
                GuideTrack::Company,
                3,
                5,
                "Hire your first driver",
                format!(
                    "{summary} At a garage, hire a driver for {} company credits. Payroll starts with dispatched travel; leave money for operations after recruitment. Hiring creates an employee contract, not a free vehicle or passenger supply.",
                    money(COMPANY_HIRE_PRICE)
                ),
                "Open driver recruitment",
                self.company_action(GuideCompanySection::Drivers, None),
            );
        }
        if company.routes.is_empty() {
            return ActivityGuidance::new(
                GuideTrack::Company,
                4,
                5,
                "Compare your first service",
                format!(
                    "{summary} Create a route with at least two stops. Before dispatch shows the real first-departure load, current receipt and visible stock or queue. Choose a load your vehicle can carry; a receipt is not profit or a funding guarantee. Warehouse transfers earn no fare."
                ),
                "Plan a route",
                self.company_action(GuideCompanySection::Routes, None),
            );
        }
        let driver = company.drivers.iter().min_by_key(|d| d.id);
        ActivityGuidance::new(
            GuideTrack::Company,
            4,
            5,
            "Assign a vehicle and route",
            format!(
                "{summary} Select a driver, a suitable stopped vehicle and a saved route in Drivers. Exit the vehicle first and finish its personal job or race. Assignment reserves the vehicle for this company; start it only after reviewing payroll, stock and capacity."
            ),
            "Review driver assignment",
            self.company_action(GuideCompanySection::Drivers, driver),
        )
    }

    fn company_service(
        &self,
        company: &TransportCompany,
        driver: &CompanyDriver,
    ) -> ActivityGuidance {
        let route = company
            .routes
            .iter()
            .find(|r| Some(r.id) == driver.route_id);
        let returning = driver.trip.as_ref().is_some_and(|trip| trip.returning);
        let next_stop = driver
            .trip
            .as_ref()
            .filter(|trip| trip.returning)
            .map(|trip| trip.pickup)
            .or_else(|| route.and_then(|r| r.stops.get(driver.next_stop)).copied());
        let stop_name = next_stop
            .and_then(|id| self.state.transport.facilities.get(&id))
            .map_or("stop outside the current view", |stop| stop.name.as_str());
        let assigned = company
            .drivers
            .iter()
            .filter(|d| d.vehicle_id.is_some())
            .count();
        let attention = company
            .drivers
            .iter()
            .filter(|d| d.vehicle_id.is_some() && needs_attention(d.status))
            .count();
        let summary = format!(
            "{} · {} assigned · {} need attention\n{} · {}\n{} · {} paid deliveries\n{}: {} · company cash {} cr.",
            company.name,
            assigned,
            attention,
            driver.name,
            driver.status.label(),
            route.map_or("route outside the current view", |r| r.name.as_str()),
            driver.deliveries_completed,
            if returning {
                "Return pickup"
            } else {
                "Next stop"
            },
            stop_name,
            money(company.cash),
        );
        let loaded = if driver.trip.is_some() {
            " The load and reserved payment remain committed. A return unloads physically at pickup and pays no delivery reward."
        } else {
            ""
        };
        let (title, section, label, explanation) = match driver.status {
            FleetStatus::RouteBlocked => (
                "Clear the obstruction before resuming",
                GuideCompanySection::Recovery,
                "Review blocked service",
                "This service stopped after no physical progress. Payroll is paused. Inspect the vehicle and clear the obstruction, then use Resume in Drivers. If the vehicle cannot continue, Recovery offers paid roadside service or a nearby stopped relief handover; neither bypasses terrain or moves a vehicle instantly.",
            ),
            FleetStatus::Held => (
                "Choose how to continue the held service",
                GuideCompanySection::Recovery,
                "Review held service",
                "The vehicle is held in place and payroll is paused. In Recovery, choose Resume delivery / return to continue the current commitment. You can instead return a loaded trip to pickup or hand it to a suitable nearby stopped relief vehicle. Review the named action before confirming.",
            ),
            FleetStatus::NeedsFuel | FleetStatus::NeedsRepair => (
                "Service the stopped vehicle",
                GuideCompanySection::Recovery,
                "Review roadside service",
                "Fuel or condition prevents further travel. Payroll is paused. Approach the stopped vehicle within 20 m, review the actual supplying garage and company-funded quote in Recovery, then explicitly buy service. The quote depends on available fuel, cash and capacity; opening it charges nothing.",
            ),
            FleetStatus::RentalExpired => (
                "Renew the assigned rental",
                GuideCompanySection::Garage,
                "Review the rental renewal",
                "The lease expired, so travel and payroll are paused. Garage opens the assigned rental. Review its renewal price, then pay from your personal wallet; renewal works from anywhere. A held or route-blocked service still needs an explicit Resume after recovery. Do not release a loaded vehicle or assume a refund before physical return.",
            ),
            FleetStatus::WaitingCompanyFunds if driver.wages_earned == MAX_MONEY => (
                "Review the employee's money limit",
                GuideCompanySection::Finances,
                "Review payroll limits",
                "The employee's wage wallet has reached its money limit. Additional company deposits cannot authorize another wage payment. Inspect Finances and preserve a verified checkpoint for diagnosis. No earned wages, load or reserved payment is erased to restart the service.",
            ),
            FleetStatus::WaitingCompanyFunds => (
                "Restore payroll funds",
                GuideCompanySection::Company,
                "Review payroll funding",
                "The next travel step cannot pay payroll from available company cash into the employee's wage wallet. Review funding; deposit personal funds if cash is short. A money limit cannot be repaired by another deposit. Valid services retry once the payment fits unless held or route-blocked. Deposits do not earn revenue.",
            ),
            FleetStatus::WaitingIssuerFunds => (
                "Review finite public funding",
                GuideCompanySection::Company,
                "Review service funding",
                "The shared service issuer cannot reserve this departure's fare. Waiting at the stop incurs no payroll. You may wait, pause the empty service or explicitly contribute company cash to public services. That contribution is shared and nonrefundable; a fare paid from it does not create profit.",
            ),
            FleetStatus::WaitingBuyerFunds => (
                "Review the buyer's ability to pay",
                GuideCompanySection::Routes,
                "Review waiting sale route",
                "The destination shop cannot reserve the warehouse purchase. Its private balance is not exposed. Waiting at the stop incurs no payroll; keep the goods, wait for trade, or pause and release an empty service before assigning a different route. Choosing a new route does not fund the buyer.",
            ),
            FleetStatus::WaitingGoods => (
                "Check the departure's real stock",
                if route.is_some_and(|r| r.service.uses_company_stock()) {
                    GuideCompanySection::Depot
                } else {
                    GuideCompanySection::Routes
                },
                "Review source goods",
                "The pickup lacks the service's required available goods. Check its actual stock and reservations. A warehouse route needs your private depot stock, acquired or contributed at its real cost. Waiting at the stop incurs no payroll; the guide supplies no free goods.",
            ),
            FleetStatus::WaitingPassengers => (
                "Check the waiting passenger queue",
                GuideCompanySection::Routes,
                "Review passenger service",
                "The departure lacks waiting passengers for this service. Queues are finite. Waiting at the stop incurs no payroll; allow returning passengers or pause and release the empty vehicle before assigning a different route. Check capacity and the first-departure preview before restarting.",
            ),
            FleetStatus::WaitingDemand => (
                "Review destination demand",
                GuideCompanySection::Routes,
                "Review destination demand",
                "The destination currently needs no available load of these goods. Waiting at the stop incurs no payroll. Check stock and reservations, allow goods to be consumed or traded, or pause and release the empty service before changing routes. A saved route cannot create demand.",
            ),
            FleetStatus::DestinationFull if driver.trip.is_none() => (
                "Review the departure's capacity",
                if route.is_some_and(|route| route.service.uses_company_stock()) {
                    GuideCompanySection::Depot
                } else {
                    GuideCompanySection::Routes
                },
                "Review the blocked departure",
                "The next load could not be created because an inventory or receiving reservation would exceed a limit. Check pickup, vehicle and destination capacity before retrying. This service has no loaded trip to return. Pause and release the empty vehicle before changing its assignment.",
            ),
            FleetStatus::DestinationFull if returning => (
                "Review the blocked return",
                GuideCompanySection::Recovery,
                "Review return capacity",
                "The physical return cannot unload or refund its reservation yet. Check arrival, pickup receiving space and refund-account capacity. Hold while deciding how to free space, then resume the same return. The guide cannot promise that another return command or more company funds will fix the limit.",
            ),
            FleetStatus::DestinationFull
                if driver.trip.as_ref().is_some_and(|trip| {
                    company
                        .cash
                        .checked_add(trip.reward_escrow)
                        .and_then(|cash| {
                            cash.checked_add(
                                trip.goods_payment
                                    .as_ref()
                                    .map_or(0, |payment| payment.total),
                            )
                        })
                        .is_none_or(|cash| cash > MAX_MONEY)
                }) =>
            {
                (
                    "Make room for the delivery payment",
                    GuideCompanySection::Company,
                    "Review company money capacity",
                    "Company cash plus this load's reserved payment would exceed the money limit. Review an owner withdrawal if your personal wallet has room; a deposit makes this limit worse. The shipment stays loaded and payment reserved until physical delivery can finish. No balance is silently reset.",
                )
            }
            FleetStatus::DestinationFull => (
                "Protect the undelivered load",
                GuideCompanySection::Recovery,
                "Review delivery recovery",
                "Delivery cannot complete under the current arrival, receiving or accounting limits. Hold while checking the named destination and company records. Clear the actual limit, resume, or review a physical return to pickup. A loaded vehicle cannot simply be released; return space and refund-account capacity are checked too.",
            ),
            FleetStatus::AccountingFull => (
                "Review the accounting limit",
                GuideCompanySection::Finances,
                "Inspect company records",
                "An accounting or money limit prevents another authorized payment. Inspect Finances and preserve a verified checkpoint for diagnosis. More funding or repeated Resume clicks cannot reset a full ledger. No automatic reset, write-off or migration is performed.",
            ),
            FleetStatus::Paused | FleetStatus::Unassigned => (
                "Review before starting the service",
                GuideCompanySection::Drivers,
                "Review the assigned departure",
                "This assigned vehicle is parked and payroll is paused. Review Before dispatch, company cash, fuel, condition and any rental time, then choose Start / resume. Starting authorizes real travel and payroll; receipts require funded loading and physical arrival.",
            ),
            FleetStatus::Driving | FleetStatus::Delivering if returning => (
                "Continue the physical return",
                GuideCompanySection::Recovery,
                "Inspect the returning service",
                "The vehicle is returning its current commitment to the original pickup. Travel still uses fuel and company payroll. Keep its path and funds available; unloading refunds the reservation under the original rules, then the service parks. This is not a paid delivery.",
            ),
            FleetStatus::Driving | FleetStatus::Delivering => (
                "Monitor the current service",
                GuideCompanySection::Drivers,
                "Inspect the running service",
                "Travel uses fuel and company payroll. Inspect the named next stop and delivery progress in Drivers, then compare actual receipts and costs in Finances. Pause safely finishes the loaded commitment before parking; Hold in place stops now and retains the load. Finite demand and funding can stop the next departure.",
            ),
        };
        let outside = if driver
            .vehicle_id
            .is_some_and(|id| !self.state.living.units.contains_key(&id))
        {
            " The vehicle body is outside the current view; approach it before commands requiring a resident vehicle."
        } else {
            ""
        };
        let lease = if !matches!(
            driver.status,
            FleetStatus::Driving | FleetStatus::Delivering
        ) && driver
            .vehicle_id
            .and_then(|id| self.state.transport.vehicles.get(&id))
            .is_some_and(|vehicle| vehicle.rental.is_some())
        {
            " Rental time continues while stopped."
        } else {
            ""
        };
        ActivityGuidance::new(
            GuideTrack::Company,
            5,
            5,
            title,
            format!("{summary}\n\n{explanation}{loaded}{outside}{lease}"),
            label,
            self.company_action(section, Some(driver)),
        )
    }
}
