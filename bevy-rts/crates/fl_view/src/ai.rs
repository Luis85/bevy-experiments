//! The AI seam (spec §8, ADR-0016): an [`AiController`] reads one player's [`PlayerView`] and
//! returns [`CommandDraft`]s, which go through the same driver as a human's orders.
//!
//! Until vision exists (M3), the client builds the view from the snapshot
//! ([`PlayerView::from_snapshot`]); M3 moves the builder into the sim's `Publish` phase.

use fl_protocol::{PlayerId, StringTables, TeamId, Tick};

use crate::driver::CommandDraft;
use crate::snapshot::{EntityRecord, SimSnapshot};

/// What one player may see after a tick.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerView {
    pub tick: Tick,
    pub player: PlayerId,
    pub team: TeamId,
    /// The player's own entities, in `sim_id` order.
    pub own: Vec<EntityRecord>,
    /// Other players' entities whose `visible_to` bit for `team` is set, in `sim_id` order.
    pub visible: Vec<EntityRecord>,
}

impl PlayerView {
    /// The view of `player` (on `team`) of `snapshot`: its own records, and the other
    /// players' records visible to `team`, both in the snapshot's (`sim_id`) order. A team
    /// beyond the 8 bits of `visible_to` sees nothing of other players.
    #[must_use]
    pub fn from_snapshot(snapshot: &SimSnapshot, player: PlayerId, team: TeamId) -> PlayerView {
        let team_bit = 1u8.checked_shl(u32::from(team.0)).unwrap_or(0);
        let records = |keep: &dyn Fn(&EntityRecord) -> bool| -> Vec<EntityRecord> {
            snapshot
                .entities
                .iter()
                .filter(|record| keep(record))
                .cloned()
                .collect()
        };
        PlayerView {
            tick: snapshot.tick,
            player,
            team,
            own: records(&|record| record.owner == player),
            visible: records(&|record| record.owner != player && record.visible_to & team_bit != 0),
        }
    }
}

/// A computer player. It runs outside the sim, may use floats, and nothing it does is hashed;
/// its drafts are stamped, delayed, sequenced and recorded like a human's.
pub trait AiController: Send + Sync + 'static {
    /// The id a scenario names it by (`Controller::Ai(id)`).
    fn id(&self) -> &'static str;
    /// The orders for `view.player` after the tick `view.tick`; `tables` names the dense ids.
    fn think(&mut self, view: &PlayerView, tables: &StringTables) -> Vec<CommandDraft>;
}

/// The do-nothing AI, id `"null"`: it never issues a command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NullAi;

impl AiController for NullAi {
    fn id(&self) -> &'static str {
        "null"
    }

    fn think(&mut self, _view: &PlayerView, _tables: &StringTables) -> Vec<CommandDraft> {
        Vec::new()
    }
}

/// The AI with id `id`: `"null"` is [`NullAi`]; any other id is `None`.
#[must_use]
pub fn ai_controller(id: &str) -> Option<Box<dyn AiController>> {
    match id {
        "null" => Some(Box::new(NullAi)),
        _ => None,
    }
}
