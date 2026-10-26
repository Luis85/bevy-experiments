//! The lockstep command envelope (spec §5.5, ADR-0007).

use core::fmt::Debug;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::ids::{CommandTypeId, PlayerId, SimId, Tick};

/// A command type plus its opaque, type-specific payload (postcard bytes of a
/// [`CommandPayload`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandKind {
    pub type_id: CommandTypeId,
    pub payload: Vec<u8>,
}

/// A command issued by a player, executed at `exec_tick`.
///
/// Commands apply in `(player, seq)` order; `seq` is strictly increasing per player over the
/// match. Drivers deliver each command in the input of exactly its `exec_tick`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerCommand {
    pub exec_tick: Tick,
    pub player: PlayerId,
    pub seq: u32,
    pub units: Vec<SimId>,
    pub kind: CommandKind,
    pub queue: bool,
}

/// A command payload type: the data a [`CommandKind`] carries, encoded with postcard.
///
/// Content ids inside a payload are names (`"base.de.pz3"`), never dense ids, so replays
/// survive content renumbering. Applying a payload is the sim's business, not this crate's.
pub trait CommandPayload: Serialize + DeserializeOwned + Debug + Send + Sync + 'static {
    /// Unique, stable name such as `"core.move"`.
    const NAME: &'static str;
    /// Whether the command needs at least one valid unit.
    const NEEDS_UNITS: bool = true;
    /// Whether the command is accepted only when `MatchHeader::cheats_allowed` is set.
    const DEBUG_ONLY: bool = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_command_postcard_roundtrip() {
        let cmd = PlayerCommand {
            exec_tick: Tick(7),
            player: PlayerId(1),
            seq: 3,
            units: vec![SimId(3), SimId(9)],
            kind: CommandKind {
                type_id: CommandTypeId(2),
                payload: vec![1, 2, 3],
            },
            queue: true,
        };
        let bytes = postcard::to_allocvec(&cmd).unwrap();
        let back: PlayerCommand = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(cmd, back);
    }
}
