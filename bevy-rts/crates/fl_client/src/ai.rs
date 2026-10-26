//! The AI controller host (spec §8, ADR-0016): players whose controller is `Controller::Ai(id)`
//! think here, on the client, through a [`PlayerView`] built from the newest snapshot.
//!
//! The host thinks at most once per frame, in a frame where the newest snapshot's tick changed
//! (several ticks received in one frame give one think, on the newest snapshot). A controller's
//! drafts go through the [`LocalDriver`] like a human's orders, so they are stamped, delayed,
//! sequenced and recorded the same way. A draft for another player is dropped with a warning.
use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use fl_protocol::{PlayerId, TeamId, Tick};
use fl_view::{AiController, PlayerView, TickDriver};

use crate::driver::LocalDriver;
use crate::link::{RunnerLink, SnapshotBuffer};
use crate::view::SimViewSystems;

/// The hosted AI controllers, by the player they play.
#[derive(Resource, Default)]
pub struct AiHost {
    /// Each AI player's team and controller, in player order.
    pub players: BTreeMap<PlayerId, (TeamId, Box<dyn AiController>)>,
    /// How many times a controller has thought (one per controller per new tick).
    pub thinks: u64,
    /// How many drafts were dropped for naming a player other than their controller's.
    pub dropped_drafts: u64,
    /// Controllers already warned about a foreign draft (one warning per controller).
    warned: BTreeSet<PlayerId>,
    /// The snapshot tick the controllers last thought on. It lives in the host, so a host
    /// inserted for a newly loaded match thinks on that match's first snapshot whatever its
    /// tick (ruling R91).
    thought: Option<Tick>,
}

/// The systems of [`AiPlugin`], in `Update`, after [`SimViewSystems`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiSystems;

/// Hosts the [`AiHost`]'s controllers.
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiHost>().add_systems(
            Update,
            ai_think
                .run_if(resource_exists::<RunnerLink>)
                .run_if(resource_exists::<LocalDriver>)
                .after(SimViewSystems)
                .in_set(AiSystems),
        );
    }
}

/// Lets every controller think once on the newest snapshot, if its tick differs from the last
/// one thought on, and issues the drafts of each as that controller's player.
fn ai_think(
    mut host: ResMut<AiHost>,
    buffer: Res<SnapshotBuffer>,
    link: Res<RunnerLink>,
    mut driver: ResMut<LocalDriver>,
) {
    let Some(snapshot) = buffer.curr.as_deref() else {
        return;
    };
    // `!=`, not `>`: a loaded match may restart at an earlier tick.
    if host.thought == Some(snapshot.tick) {
        return;
    }
    host.thought = Some(snapshot.tick);
    let host = &mut *host;
    for (&player, (team, controller)) in &mut host.players {
        let view = PlayerView::from_snapshot(snapshot, player, *team);
        let drafts = controller.think(&view, &link.header.string_tables);
        host.thinks += 1;
        for draft in drafts {
            if draft.player == player {
                driver.issue(draft);
            } else {
                host.dropped_drafts += 1;
                if host.warned.insert(player) {
                    tracing::warn!(
                        ai = controller.id(),
                        player = player.0,
                        drafted_for = draft.player.0,
                        "AI draft for another player dropped (warned once per controller)"
                    );
                }
            }
        }
    }
}
