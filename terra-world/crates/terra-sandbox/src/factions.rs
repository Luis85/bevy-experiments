use crate::state::valid_faction_name;
use crate::{
    Faction, FactionRole, MAX_FACTIONS, MAX_PLAYER_FACTIONS, MAX_PLAYERS, Outcome, SandboxError,
    SandboxState,
};
use std::collections::{BTreeMap, BTreeSet};

impl SandboxState {
    pub(crate) fn create_faction(
        &mut self,
        player: u64,
        name: String,
    ) -> Result<Outcome, SandboxError> {
        if !valid_faction_name(&name) {
            return Err(SandboxError::Invalid(
                "faction name must be 1..64 printable characters (at most 128 UTF-8 bytes), trimmed".into(),
            ));
        }
        if self.factions.len() >= MAX_FACTIONS
            || self.affiliation_count(player) >= MAX_PLAYER_FACTIONS
        {
            return Err(SandboxError::Capacity);
        }
        let id = self.fresh_id()?;
        self.factions.insert(
            id,
            Faction {
                id,
                name,
                leader: player,
                members: BTreeMap::from([(player, FactionRole::Leader)]),
                invites: BTreeSet::new(),
            },
        );
        self.next_id += 1;
        Ok(Outcome {
            message: "Faction created".into(),
            created_id: Some(id),
        })
    }

    pub(crate) fn invite_faction(
        &mut self,
        leader: u64,
        faction_id: u64,
        invitee: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != leader {
            return Err(SandboxError::NotOwner);
        }
        if !self.players.contains_key(&invitee) {
            return Err(SandboxError::PlayerMissing);
        }
        if faction.members.contains_key(&invitee) || faction.invites.contains(&invitee) {
            return Err(SandboxError::Invalid(
                "player is already a member or invited".into(),
            ));
        }
        if faction.invites.len() >= MAX_PLAYERS
            || self.affiliation_count(invitee) >= MAX_PLAYER_FACTIONS
        {
            return Err(SandboxError::Capacity);
        }
        self.factions
            .get_mut(&faction_id)
            .expect("faction validated")
            .invites
            .insert(invitee);
        Ok(Outcome {
            message: "Faction invitation created".into(),
            created_id: None,
        })
    }

    pub(crate) fn accept_faction(
        &mut self,
        player: u64,
        faction_id: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if !faction.invites.contains(&player) || faction.members.contains_key(&player) {
            return Err(SandboxError::NotOwner);
        }
        if faction.members.len() >= MAX_PLAYERS
            || self.membership_count(player) >= MAX_PLAYER_FACTIONS
        {
            return Err(SandboxError::Capacity);
        }
        let faction = self
            .factions
            .get_mut(&faction_id)
            .expect("faction validated");
        faction.invites.remove(&player);
        faction.members.insert(player, FactionRole::Member);
        Ok(Outcome {
            message: "Joined faction as member".into(),
            created_id: None,
        })
    }

    pub(crate) fn set_faction_role(
        &mut self,
        leader: u64,
        faction_id: u64,
        player: u64,
        role: FactionRole,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != leader {
            return Err(SandboxError::NotOwner);
        }
        if player == faction.leader || role == FactionRole::Leader {
            return Err(SandboxError::Invalid(
                "leadership cannot be changed through a role grant".into(),
            ));
        }
        if !faction.members.contains_key(&player) {
            return Err(SandboxError::Missing(format!("member:{player}")));
        }
        self.factions
            .get_mut(&faction_id)
            .expect("faction validated")
            .members
            .insert(player, role);
        Ok(Outcome {
            message: "Faction role updated".into(),
            created_id: None,
        })
    }

    pub(crate) fn decline_faction_invite(
        &mut self,
        player: u64,
        faction_id: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get_mut(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if !faction.invites.contains(&player) {
            return Err(SandboxError::NotOwner);
        }
        faction.invites.remove(&player);
        Ok(Outcome {
            message: "Faction invitation declined; your affiliation slot is available again".into(),
            created_id: None,
        })
    }

    pub(crate) fn revoke_faction_invite(
        &mut self,
        leader: u64,
        faction_id: u64,
        invitee: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get_mut(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != leader {
            return Err(SandboxError::NotOwner);
        }
        if !faction.invites.contains(&invitee) {
            return Err(SandboxError::Missing(format!("invitation:{invitee}")));
        }
        faction.invites.remove(&invitee);
        Ok(Outcome {
            message: "Faction invitation revoked".into(),
            created_id: None,
        })
    }

    pub(crate) fn leave_faction(
        &mut self,
        player: u64,
        faction_id: u64,
    ) -> Result<Outcome, SandboxError> {
        self.end_faction_membership(player, faction_id, player)
    }

    /// Check whether the active sole leader may remove an unused faction.
    /// This is read-only and uses the same checks as `DissolveFaction`.
    ///
    /// On authoritative complete state or a validated regional working set,
    /// every plot and territory is global. Valid shared houses and farms must
    /// belong to a matching faction plot, so those global references also
    /// protect cold property. A filtered client replica can report visible
    /// blockers, but absence there is not proof of whole-world eligibility;
    /// the host must check its current authority before applying the command.
    pub fn faction_dissolution_readiness(
        &self,
        player: u64,
        faction_id: u64,
    ) -> Result<(), SandboxError> {
        if !self.players.contains_key(&player) {
            return Err(SandboxError::PlayerMissing);
        }
        if self.is_player_dormant(player) {
            return Err(SandboxError::Invalid(
                "resume the owner's session before dissolving a faction".into(),
            ));
        }
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != player || faction.members.get(&player) != Some(&FactionRole::Leader) {
            return Err(SandboxError::NotOwner);
        }
        if faction.members.len() != 1 {
            return Err(SandboxError::Invalid(
                "only a sole-member faction can dissolve; other members must leave first".into(),
            ));
        }
        if self
            .plots
            .values()
            .any(|plot| plot.faction_owner == Some(faction_id))
            || self
                .settlements
                .territories
                .values()
                .any(|territory| territory.faction_owner == Some(faction_id))
        {
            return Err(SandboxError::Invalid(
                "a faction with shared land or a settlement cannot dissolve; its property, public stock and treasury must be retained".into(),
            ));
        }
        // Defensive explicit references supplement the global ownership proof.
        // In a validated world these all imply an existing shared plot.
        if self
            .structures
            .values()
            .any(|body| body.faction_owner == Some(faction_id))
            || self
                .farms
                .values()
                .any(|farm| farm.faction_owner == Some(faction_id))
            || self.players.values().any(|principal| {
                principal.home.is_some_and(|id| {
                    self.home_reference(id)
                        .is_some_and(|(_, owner)| owner == Some(faction_id))
                })
            })
        {
            return Err(SandboxError::Invalid(
                "a faction with shared housing or crops cannot dissolve".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn dissolve_faction(
        &mut self,
        player: u64,
        faction_id: u64,
    ) -> Result<Outcome, SandboxError> {
        self.faction_dissolution_readiness(player, faction_id)?;
        // Only the faction and its own invitations disappear. Memberships are
        // stored in faction rosters, never duplicated on player records. There
        // are no assets to transfer/refund, and fresh IDs remain monotonic.
        let faction = self.factions.remove(&faction_id).expect("faction checked");
        Ok(Outcome {
            message: format!(
                "Dissolved {} #{}; {} invitations revoked. No property or funds moved.",
                faction.name,
                faction.id,
                faction.invites.len()
            ),
            created_id: None,
        })
    }

    pub(crate) fn remove_faction_member(
        &mut self,
        leader: u64,
        faction_id: u64,
        player: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != leader {
            return Err(SandboxError::NotOwner);
        }
        self.end_faction_membership(leader, faction_id, player)
    }

    /// Only leave/remove callers enter here. References are checked against the
    /// same current faction authority used by every shared-property command.
    /// Cold home descriptors let an offline member leave without loading or
    /// changing the house itself. Reservation descriptors remain unchanged
    /// until the host merges the working set with the original cold pages.
    fn end_faction_membership(
        &mut self,
        caller: u64,
        faction_id: u64,
        player: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if !faction.members.contains_key(&player) {
            return Err(SandboxError::Missing(format!("member:{player}")));
        }
        if player == faction.leader {
            return Err(SandboxError::Invalid(
                "transfer leadership to another member before leaving; a faction must retain its leader".into(),
            ));
        }
        let principal = self
            .players
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?;
        let clear_home = principal.home.is_some_and(|id| {
            self.home_reference(id)
                .is_some_and(|(_, owner)| owner == Some(faction_id))
        });

        // Every fallible check precedes mutation. Shared assets, contributions,
        // bodies, personal property, inventories and money are never reassigned.
        self.factions
            .get_mut(&faction_id)
            .expect("faction validated")
            .members
            .remove(&player);
        if clear_home {
            self.players
                .get_mut(&player)
                .expect("player validated")
                .home = None;
        }
        let action = if caller == player {
            "Left faction".to_string()
        } else {
            format!("Removed player #{player} from faction")
        };
        Ok(Outcome {
            message: format!(
                "{action}; shared property and contributions remain with the faction{}",
                if clear_home {
                    "; shared home unset"
                } else {
                    ""
                },
            ),
            created_id: None,
        })
    }

    pub(crate) fn transfer_faction_leadership(
        &mut self,
        leader: u64,
        faction_id: u64,
        successor: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != leader {
            return Err(SandboxError::NotOwner);
        }
        if successor == leader {
            return Err(SandboxError::Invalid(
                "choose another existing faction member to lead".into(),
            ));
        }
        if !faction.members.contains_key(&successor) {
            return Err(SandboxError::Missing(format!("member:{successor}")));
        }

        // An explicit handover, never an inactivity election. Keeping the
        // outgoing leader as Builder preserves shared public-works access; only
        // the new leader can change the roster or transfer leadership again.
        let faction = self
            .factions
            .get_mut(&faction_id)
            .expect("faction validated");
        faction.members.insert(leader, FactionRole::Builder);
        faction.members.insert(successor, FactionRole::Leader);
        faction.leader = successor;
        Ok(Outcome {
            message: format!(
                "Player #{successor} now leads the faction; you remain a Builder. Shared property and treasury stay with the faction"
            ),
            created_id: None,
        })
    }

    pub(crate) fn transfer_plot_to_faction(
        &mut self,
        player: u64,
        plot_id: u64,
        faction_id: u64,
    ) -> Result<Outcome, SandboxError> {
        let faction = self
            .factions
            .get(&faction_id)
            .ok_or_else(|| SandboxError::Missing(format!("faction:{faction_id}")))?;
        if faction.leader != player {
            return Err(SandboxError::NotOwner);
        }
        let plot = self
            .plots
            .get(&plot_id)
            .ok_or_else(|| SandboxError::Missing(format!("plot:{plot_id}")))?;
        if plot.owner != player || plot.faction_owner.is_some() {
            return Err(SandboxError::NotOwner);
        }
        let position = self
            .players
            .get(&player)
            .ok_or(SandboxError::PlayerMissing)?
            .position;
        if position
            .surface_distance_m(plot.center)
            .hypot(position.altitude_m - plot.center.altitude_m)
            > crate::INTERACTION_REACH_M
        {
            return Err(SandboxError::OutOfReach);
        }
        let structures: Vec<u64> = self
            .structures
            .values()
            .filter(|s| plot.center.surface_distance_m(s.position) + s.radius_m <= plot.radius_m)
            .map(|s| s.id)
            .collect();
        if structures.iter().any(|id| {
            self.structures
                .get(id)
                .is_some_and(|s| s.owner != player || s.faction_owner.is_some())
        }) || self
            .farms
            .values()
            .any(|f| f.plot_id == plot_id && (f.owner != player || f.faction_owner.is_some()))
        {
            return Err(SandboxError::Invalid(
                "plot contains property that cannot be transferred".into(),
            ));
        }
        // All checks precede mutation. Existing assets transfer together, while
        // their creator IDs remain available for audit/history.
        self.plots
            .get_mut(&plot_id)
            .expect("plot validated")
            .faction_owner = Some(faction_id);
        for id in structures {
            self.structures
                .get_mut(&id)
                .expect("structure validated")
                .faction_owner = Some(faction_id);
        }
        for farm in self.farms.values_mut().filter(|f| f.plot_id == plot_id) {
            farm.faction_owner = Some(faction_id);
        }
        Ok(Outcome {
            message: "Land and contained property transferred to faction".into(),
            created_id: None,
        })
    }
}
