//! Earned employee money circulates through actual retail stock and the public
//! transport fund. This is a bounded household abstraction, never a stock refill
//! or a treasury grant. Companies without a depot retain the legacy policy.
use super::*;

impl SandboxState {
    pub(in crate::transport) fn advance_company_households(&mut self, elapsed: f64) {
        let owners: Vec<_> = self.transport.companies.keys().copied().collect();
        for owner in owners {
            if self.is_player_dormant(owner) || self.transport.companies[&owner].depots.is_empty() {
                continue;
            }
            let company = self.transport.companies.get_mut(&owner).unwrap();
            company.household_elapsed = (company.household_elapsed + elapsed).min(65.);
            if company.household_elapsed < 60. {
                continue;
            }
            company.household_elapsed = company.household_elapsed.rem_euclid(60.);
            let depot = company.depots[0].facility_id;
            let position = self.transport.facilities[&depot].position;
            let workers: Vec<_> = company
                .drivers
                .iter()
                .filter(|d| {
                    !d.running
                        && d.trip.is_none()
                        && d.vehicle_id.is_none_or(|id| {
                            self.living
                                .units
                                .get(&id)
                                .is_some_and(|u| near(u.position, position) && body_speed(u) <= 0.5)
                        })
                })
                .map(|d| Some(d.id))
                .chain((company.departed_wages > 0).then_some(None))
                .collect();
            for worker in workers {
                let company = &self.transport.companies[&owner];
                let (wallet, spent) = worker
                    .map(|id| {
                        let d = company.drivers.iter().find(|d| d.id == id).unwrap();
                        (d.wages_earned, d.wages_spent)
                    })
                    .unwrap_or((company.departed_wages, company.departed_spending));
                // Alternate the least consumed essential to create recurrent
                // demand for both chains; no consumption when either is absent.
                let item = if company
                    .household_consumed
                    .get("water")
                    .copied()
                    .unwrap_or(0)
                    <= company
                        .household_consumed
                        .get("bread")
                        .copied()
                        .unwrap_or(0)
                {
                    "water"
                } else {
                    "bread"
                };
                let previous = company.household_consumed.get(item).copied().unwrap_or(0);
                if previous >= 1_000_000_000 {
                    continue;
                }
                let shop = self
                    .living
                    .shops
                    .values()
                    .filter_map(|s| {
                        let distance = s
                            .position
                            .surface_distance_m(position)
                            .hypot(s.position.altitude_m - position.altitude_m);
                        if distance > 200. || s.stock.get(item).copied().unwrap_or(0) == 0 {
                            return None;
                        }
                        let price = s.prices.get(item)?.buy;
                        let total = price.checked_add(50).filter(|n| *n <= wallet)?;
                        let next_spent = spent
                            .checked_add(total)
                            .filter(|n| *n <= 1_000_000_000_000)?;
                        let shop_cash = s.money.checked_add(price).filter(|n| *n <= MAX_MONEY)?;
                        Some((distance, s.id, total, next_spent, shop_cash))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                // A nearer unaffordable or capped till must not hide another
                // feasible local purchase. Every candidate is checked first.
                let Some((_, shop_id, total, next_spent, shop_cash)) = shop else {
                    continue;
                };
                let Some(treasury) = self
                    .transport
                    .treasury
                    .checked_add(50)
                    .filter(|n| *n <= MAX_MONEY)
                else {
                    continue;
                };
                let shop = self.living.shops.get_mut(&shop_id).unwrap();
                let count = shop.stock[item];
                if count == 1 {
                    shop.stock.remove(item);
                } else {
                    shop.stock.insert(item.into(), count - 1);
                }
                shop.money = shop_cash;
                self.transport.treasury = treasury;
                let company = self.transport.companies.get_mut(&owner).unwrap();
                if let Some(id) = worker {
                    let d = company.drivers.iter_mut().find(|d| d.id == id).unwrap();
                    d.wages_earned -= total;
                    d.wages_spent = next_spent;
                } else {
                    company.departed_wages -= total;
                    company.departed_spending = next_spent;
                }
                company.household_consumed.insert(item.into(), previous + 1);
                self.living.metrics.npc_purchases =
                    self.living.metrics.npc_purchases.saturating_add(1);
            }
        }
    }
}
