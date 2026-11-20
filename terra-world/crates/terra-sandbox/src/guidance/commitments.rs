//! A bounded returning-player briefing derived from owner-visible state.
use super::*;

/// Current obligations, not a second quest ledger. At most three supply rows
/// are retained, matching ordinary acceptance; aggregate counts remain explicit.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityCommitments {
    pub detail: String,
    pub supply_requests: Vec<SupplyContract>,
}

impl SandboxState {
    pub fn activity_commitments(&self, player: u64) -> Option<ActivityCommitments> {
        let principal = self.players.get(&player)?;
        let account = self.living.accounts.get(&player)?;
        let mut supply: Vec<_> = self
            .settlements
            .contracts
            .values()
            .filter(|request| request.claimed_by == Some(player))
            .collect();
        supply.sort_by(|a, b| a.expires_at.total_cmp(&b.expires_at).then(a.id.cmp(&b.id)));
        let mut lines = vec![format!("Personal wallet: {} cr", money(account.money))];
        if supply.is_empty() {
            lines.push("No accepted supply requests.".into());
        } else {
            lines.push(format!("Accepted supply: {}", supply.len()));
            for request in supply.iter().take(MAX_ACTIVE_CONTRACTS) {
                let carried = principal.inventory.get(&request.item).copied().unwrap_or(0);
                let remaining = request.expires_at - self.settlements.elapsed_seconds;
                lines.push(format!(
                    "#{} · {} {} · carry {} / {} · {}",
                    request.id,
                    request.amount,
                    item_name(&request.item),
                    carried.min(request.amount),
                    request.amount,
                    supply_deadline_label(remaining)
                ));
            }
        }
        let road_jobs = self
            .transport
            .jobs
            .values()
            .filter(|job| job.assignment.as_ref().is_some_and(|a| a.owner == player))
            .count();
        if road_jobs > 0 {
            lines.push(format!("Accepted road jobs: {road_jobs} · review Road."));
        }
        if let Some(company) = self.transport.companies.get(&player) {
            let assigned = company
                .drivers
                .iter()
                .filter(|d| d.vehicle_id.is_some())
                .count();
            let loaded = company.drivers.iter().filter(|d| d.trip.is_some()).count();
            lines.push(format!(
                "Company cash: {} cr · {assigned} assigned drivers · {loaded} loaded commitments. Review Company.",
                money(company.cash)
            ));
        }
        let mut rentals = 0;
        let mut expired = 0;
        let mut bond = 0_u64;
        for rental in self
            .transport
            .vehicles
            .values()
            .filter(|vehicle| vehicle.owner == player)
            .filter_map(|vehicle| vehicle.rental.as_ref())
        {
            rentals += 1;
            expired += usize::from(rental.remaining_seconds <= 0.);
            bond = bond.saturating_add(rental.bond_escrow);
        }
        if rentals > 0 {
            lines.push(format!(
                "Rentals: {rentals} · {expired} expired · {} cr in bonds, not spendable cash. Review Garage.",
                money(bond)
            ));
        }
        if self.is_partial() {
            lines.push("Current received view; distant services may be unavailable.".into());
        }
        Some(ActivityCommitments {
            detail: lines.join("\n"),
            supply_requests: supply
                .into_iter()
                .take(MAX_ACTIVE_CONTRACTS)
                .cloned()
                .collect(),
        })
    }
}
