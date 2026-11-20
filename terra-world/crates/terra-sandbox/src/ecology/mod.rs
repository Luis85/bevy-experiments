//! Bounded, authored wildlife and natural-resource simulation.
//!
//! Geographic source features suggest habitat placement. They are not evidence
//! of animal counts, mineral deposits or harvest yields. Bodies use the same
//! movement resolver and persistence envelope as other living actors.
use crate::{
    Catalog, Inventory, LivingUnit, LivingView, LivingWorld, MAX_ENTITIES, MAX_LIVING_UNITS, Needs,
    ResourceNode, SandboxError, SandboxState, UnitOrder, UnitRole, finite_range, valid_id,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{ActorKind, FeatureKind, GeoPosition, LocalFrame, WorldChunk};

mod model;
mod schedule;
mod seed;
mod simulation;
mod validation;

pub use model::*;
pub use schedule::*;

fn position_valid(position: GeoPosition) -> bool {
    position.validate().is_ok() && finite_range(position.altitude_m, -12_000., 100_000.)
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn offset(origin: GeoPosition, east: f64, north: f64) -> Option<GeoPosition> {
    LocalFrame::new(origin)
        .to_geo([east, 0., -north])
        .ok()
        .filter(|p| position_valid(*p))
}

impl SandboxState {
    /// Additional tool policy for authored ecology nodes. Existing source/game
    /// nodes are unaffected. Call before the normal Gather transaction commits.
    pub fn validate_ecology_gather(
        &self,
        player_id: u64,
        node_id: &str,
    ) -> Result<(), SandboxError> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(SandboxError::PlayerMissing)?;
        let Some(tool) = self.ecology.resource_requirement(node_id) else {
            return Ok(());
        };
        if player.inventory.get(tool).copied().unwrap_or(0) == 0
            && !(tool == "tool" && player.inventory.get("iron_tool").copied().unwrap_or(0) > 0)
        {
            return Err(SandboxError::Insufficient(format!(
                "{tool} for this resource"
            )));
        }
        Ok(())
    }
}
