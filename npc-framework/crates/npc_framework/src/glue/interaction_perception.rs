//! Interaction × perception (features `interaction` + `perception`):
//! interaction noises become perception stimuli.
//!
//! [`InteractionPerceptionPlugin`] sets [`perception_noise`] as the
//! interaction noise hook (name `npc_perception`, which joins the content
//! hash): a successful verb with a `NoiseSpec` (doors, tool work) emits a
//! `Noise` from the actor, heard by `Hearing` sensors. Doors already affect
//! line of sight without it (they toggle the grid's occluding flag).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::{SimId, SimPos};
use npc_perception::{Noise, emit_noise};

use npc_interaction::{InteractionAppExt, NoiseSpec};

/// Noise hook backed by `npc_perception`: a `Noise` from the actor
/// (heard by `Hearing` sensors; a no-op without `PerceptionPlugin`).
pub fn perception_noise(world: &mut World, source: SimId, pos: SimPos, spec: NoiseSpec) {
    emit_noise(
        world,
        Noise::at(pos, spec.radius_mm)
            .from_source(source)
            .kind(spec.kind),
    );
}

/// Sets [`perception_noise`] as the interaction noise hook (name
/// `npc_perception`).
pub struct InteractionPerceptionPlugin;

impl Plugin for InteractionPerceptionPlugin {
    fn build(&self, app: &mut App) {
        app.set_noise_hook("npc_perception", perception_noise);
    }
}
