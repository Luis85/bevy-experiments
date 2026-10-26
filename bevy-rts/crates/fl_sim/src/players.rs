//! The players of a match.

use bevy_ecs::resource::Resource;
use fl_protocol::{Controller, FactionId, PlayerId, TeamId};
use serde::{Deserialize, Serialize};

use crate::classify::{Codec, from_postcard, trace_type};
use crate::hash::{SimHash, StateHasher};
use crate::save::{LoadCtx, SaveCtx};

/// The string table of faction ids: content kind `*.faction.ron`, in id order, so a
/// [`FactionId`] is a dense index into it.
pub(crate) const FACTIONS: &str = "content.faction";

/// One player slot: faction, team (allies share vision) and who controls it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub faction: FactionId,
    pub team: TeamId,
    pub controller: Controller,
}

/// The match's players, in slot order. Hashed `"core.players"` with [`players_codec`]: each
/// faction is hashed by name and remapped by name on load.
#[derive(Resource, Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Players {
    pub list: Vec<PlayerInfo>,
}

impl Players {
    pub fn get(&self, id: PlayerId) -> Option<&PlayerInfo> {
        self.list.iter().find(|p| p.id == id)
    }
}

impl SimHash for PlayerInfo {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.id.sim_hash(h);
        h.write_registry_id(FACTIONS, u32::from(self.faction.0));
        h.write_u8(self.team.0);
        // The controller's postcard bytes: `Controller` is non-exhaustive.
        let controller =
            postcard::to_allocvec(&self.controller).expect("controllers serialize to postcard");
        h.write_u64(controller.len() as u64);
        h.write_bytes(&controller);
    }
}

impl SimHash for Players {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.list.sim_hash(h);
    }
}

/// Saves the players with dense faction ids, valid in the save's [`FACTIONS`] table.
fn encode_players(players: &Players, _ctx: &SaveCtx<'_>) -> Vec<u8> {
    postcard::to_allocvec(players).expect("players serialize to postcard")
}

/// The saved players, each faction remapped by name to this sim's [`FACTIONS`] table.
fn decode_players(_version: u32, bytes: &[u8], ctx: &LoadCtx<'_>) -> Result<Players, String> {
    let mut players: Players = from_postcard(bytes)?;
    for player in &mut players.list {
        let current = ctx.remap(FACTIONS, u32::from(player.faction.0))?;
        player.faction = FactionId(
            u16::try_from(current)
                .map_err(|_| format!("faction id {current} does not fit a u16"))?,
        );
    }
    Ok(players)
}

/// The codec of `"core.players"`, version 1. Its format trace covers every [`Controller`].
pub(crate) fn players_codec() -> Codec<Players> {
    Codec::custom(1, encode_players, decode_players, trace_type::<Players>)
        .trace_enum::<Controller>()
}

#[cfg(test)]
mod tests {
    use fl_content::{ContentKind, Faction};

    use super::*;

    #[test]
    fn faction_table_is_the_faction_kind() {
        assert_eq!(FACTIONS, format!("content.{}", Faction::EXT));
    }
}
