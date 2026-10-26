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
