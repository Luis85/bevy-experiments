//! Trusted host lifecycle, separate from player commands and client visibility.
//!
//! Ownership, inventory, finances and obligations remain in the global ledger.
//! Offline bodies and homes can live in cold regional pages. Small private
//! descriptors preserve references in a working set; only complete assembly
//! validates/exports the actual authoritative records.

use crate::{Inventory, SandboxError, SandboxState, UnitRole};
use serde::{Deserialize, Serialize};
use terra_core::{ActorKind, GeoPosition};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DormantUnitReference {
    pub owner: u64,
    pub kind: ActorKind,
    pub position: GeoPosition,
    /// Only legacy logistics couriers need a cold cargo projection. Delivery
    /// receipts remain global, while the actual owned body can retire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courier_supplies: Option<Inventory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DormantHomeReference {
    pub owner: u64,
    pub faction_owner: Option<u64>,
}

impl SandboxState {
    /// Session activity is host authority, never a command accepted from a peer.
    pub fn is_player_dormant(&self, player: u64) -> bool {
        self.dormant_players.contains(&player)
    }

    pub fn dormant_player_count(&self) -> usize {
        self.dormant_players.len()
    }

    /// Suspend every owned body without changing its orders, goods or needs.
    /// Reconnection must hydrate missing bodies/home before waking the owner.
    /// Motion and possession are cleared so stale input cannot survive resume.
    pub fn set_player_dormant(&mut self, player: u64, dormant: bool) -> Result<(), SandboxError> {
        let principal = self
            .players
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?;
        if !dormant
            && (self.dormant.owned_units.values().any(|u| u.owner == player)
                || principal
                    .home
                    .is_some_and(|id| self.dormant.homes.contains_key(&id)))
        {
            return Err(SandboxError::Invalid(
                "hydrate the owner's bodies and home before resuming the session".into(),
            ));
        }
        if dormant {
            self.dormant_players.insert(player);
            if let Some(account) = self.living.accounts.get_mut(&player) {
                account.possessed = false;
            }
            for unit in self
                .living
                .units
                .values_mut()
                .filter(|u| u.owner == Some(player))
            {
                unit.velocity_enu_m_s = [0.; 3];
            }
            for vehicle in self
                .transport
                .vehicles
                .values_mut()
                .filter(|v| v.owner == player)
            {
                vehicle.dynamics.stop();
            }
        } else {
            self.dormant_players.remove(&player);
        }
        Ok(())
    }

    /// Referential lookup only. Gameplay must require a resident body before
    /// changing it. This returns no private supplies, needs or account values.
    pub fn living_unit_reference(&self, id: u64) -> Option<(Option<u64>, ActorKind, GeoPosition)> {
        self.living
            .units
            .get(&id)
            .map(|u| (u.owner, u.kind, u.position))
            .or_else(|| {
                self.dormant
                    .owned_units
                    .get(&id)
                    .map(|u| (Some(u.owner), u.kind, u.position))
            })
    }

    pub(crate) fn courier_supplies_reference(&self, id: u64) -> Option<&Inventory> {
        self.living
            .units
            .get(&id)
            .filter(|unit| unit.role == UnitRole::Hauler)
            .map(|unit| &unit.supplies)
            .or_else(|| self.dormant.owned_units.get(&id)?.courier_supplies.as_ref())
    }

    pub(crate) fn home_reference(&self, id: u64) -> Option<(u64, Option<u64>)> {
        self.structures
            .get(&id)
            .filter(|s| s.housing)
            .map(|s| (s.owner, s.faction_owner))
            .or_else(|| {
                self.dormant
                    .homes
                    .get(&id)
                    .map(|s| (s.owner, s.faction_owner))
            })
    }

    pub(crate) fn validate_owner_dormancy(&self) -> Result<(), SandboxError> {
        if self
            .dormant_players
            .iter()
            .any(|id| !self.players.contains_key(id))
            || self.dormant.owned_units.len() > self.dormant.living_units
            || self.dormant.homes.len() > self.dormant.structures
        {
            return Err(SandboxError::Invalid(
                "dormant owner references or counts".into(),
            ));
        }
        for (&id, reference) in &self.dormant.owned_units {
            reference.position.validate()?;
            if let Some(supplies) = &reference.courier_supplies {
                crate::catalog::validate_inventory(supplies)?;
                if supplies.len() > 1
                    || supplies.values().any(|count| *count > 100)
                    || (!supplies.is_empty()
                        && !self
                            .living
                            .deliveries
                            .iter()
                            .any(|delivery| delivery.unit_id == id))
                {
                    return Err(SandboxError::Invalid(
                        "dormant courier cargo reference".into(),
                    ));
                }
            }
            if id == 0
                || id >= self.living.next_id
                || self.living.units.contains_key(&id)
                || !self.is_player_dormant(reference.owner)
            {
                return Err(SandboxError::Invalid("dormant owned body reference".into()));
            }
        }
        for (&id, reference) in &self.dormant.homes {
            if id == 0
                || id >= self.next_id
                || self.structures.contains_key(&id)
                || !self.players.contains_key(&reference.owner)
                || reference
                    .faction_owner
                    .is_some_and(|id| !self.factions.contains_key(&id))
            {
                return Err(SandboxError::Invalid("dormant housing reference".into()));
            }
        }
        if self
            .living
            .accounts
            .values()
            .any(|a| self.is_player_dormant(a.player_id) && a.possessed)
            || self.living.units.values().any(|u| {
                u.owner.is_some_and(|p| self.is_player_dormant(p)) && u.velocity_enu_m_s != [0.; 3]
            })
        {
            return Err(SandboxError::Invalid(
                "dormant owner retains manual motion".into(),
            ));
        }
        Ok(())
    }
}
