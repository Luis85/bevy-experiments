//! Save and load (spec §5.8, ADR-0006).
//!
//! A save is the postcard bytes of a [`SaveFile`]: the [`MatchHeader`](fl_protocol::MatchHeader)
//! with every string table, then every `Hashed` resource and every entity's `Hashed`
//! components, each as `(name, version, bytes)` from its [`Codec`](crate::Codec). Entities are
//! saved by [`SimId`](fl_protocol::SimId); a Bevy `Entity` never appears. [`load_world`]
//! builds a fresh sim, decodes everything by name (migrating older versions, remapping dense
//! ids through the string tables) and rebuilds the `Derived` state; [`load_world_with`] can
//! drop the units whose type the content lost (the hot-reload rebuild, ADR-0012).
//!
//! Saving the loaded world gives the same bytes (the round trip), and a loaded sim runs on
//! with the same state hashes as the sim it was saved from (the branch test).

mod format;
mod load;

use bevy_ecs::message::Messages;
use bevy_ecs::world::World;
use fl_protocol::PlayerCommand;

use crate::build::MatchHeaderRes;
use crate::classify::{StateClass, StateRegistry};
use crate::clock::SimClock;
use crate::ids::{SimIdAllocator, SimIndex};
use crate::spawn::{DespawnRequest, SpawnRequest};

pub use format::{LoadCtx, SAVE_FORMAT_VERSION, SaveCtx, SaveFile, SavedValue, SimSave};
pub(crate) use load::rebuild_derived;
pub use load::{LoadError, LoadOptions, Loaded, load_world, load_world_with};

/// Why the world cannot be saved now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SaveError {
    /// Spawn or despawn requests are waiting: they are `Transient`, so a save would lose
    /// them. `step_tick` clears every message buffer at the end of a tick, so they wait only
    /// when something wrote them between ticks.
    #[error(
        "cannot save with {spawns} spawn request(s) and {despawns} despawn request(s) \
         waiting: saves happen between ticks, with no request outstanding"
    )]
    PendingRequests { spawns: usize, despawns: usize },
}

/// The save of `world` between ticks, with the driver's `pending` commands (stamped but not
/// yet delivered), as postcard bytes of a [`SaveFile`]. `pending` is stored sorted by
/// `(exec_tick, player, seq)`.
///
/// # Errors
/// [`SaveError::PendingRequests`] if spawn or despawn requests are waiting.
///
/// # Panics
/// If `world` was not built by [`build_sim_app`](crate::build_sim_app), or a codec cannot
/// encode its value.
pub fn save_world(world: &World, pending: &[PlayerCommand]) -> Result<Vec<u8>, SaveError> {
    check_no_requests(world)?;
    let header = world.resource::<MatchHeaderRes>().0.clone();
    let state = world.resource::<StateRegistry>();
    let ctx = SaveCtx::new(&header.string_tables);

    let resources = state
        .resources
        .iter()
        .filter_map(|(_, name, entry)| {
            let StateClass::Hashed { version } = entry.class else {
                return None;
            };
            let codec = entry.codec.as_deref().expect("Hashed types have a codec");
            let bytes = codec.save(world, &ctx)?;
            Some((name.to_owned(), version, bytes))
        })
        .collect();

    let codecs: Vec<_> = state
        .components
        .iter()
        .filter_map(|(_, name, entry)| match entry.class {
            StateClass::Hashed { version } => Some((
                name,
                version,
                entry.codec.as_deref().expect("Hashed types have a codec"),
            )),
            _ => None,
        })
        .collect();
    let entities = world
        .resource::<SimIndex>()
        .iter()
        .map(|(id, entity)| {
            let components = codecs
                .iter()
                .filter_map(|&(name, version, codec)| {
                    let bytes = codec.save(world, entity, &ctx)?;
                    Some((name.to_owned(), version, bytes))
                })
                .collect();
            (id, components)
        })
        .collect();

    let mut pending_commands = pending.to_vec();
    pending_commands.sort_by_key(|cmd| (cmd.exec_tick, cmd.player, cmd.seq));
    let save = SaveFile {
        format_version: SAVE_FORMAT_VERSION,
        header,
        sim: SimSave {
            tick: world.resource::<SimClock>().tick,
            sim_id_next: world.resource::<SimIdAllocator>().issued(),
            pending_commands,
            resources,
            entities,
        },
    };
    Ok(postcard::to_allocvec(&save).expect("a SaveFile serializes to postcard"))
}

/// # Errors
/// [`SaveError::PendingRequests`] if a spawn or despawn request is waiting.
fn check_no_requests(world: &World) -> Result<(), SaveError> {
    let spawns = world
        .get_resource::<Messages<SpawnRequest>>()
        .map_or(0, Messages::len);
    let despawns = world
        .get_resource::<Messages<DespawnRequest>>()
        .map_or(0, Messages::len);
    if spawns == 0 && despawns == 0 {
        Ok(())
    } else {
        Err(SaveError::PendingRequests { spawns, despawns })
    }
}
