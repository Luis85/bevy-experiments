//! `Noise` stimuli: what hearing reacts to.
//!
//! Any crate can make noise without depending on the plugin being present:
//! [`emit_noise`] (with `&mut World`, e.g. from an action's `start`) or the
//! [`EmitNoise`] command (from a system's `Commands`) are no-ops when the
//! [`PerceptionPlugin`](crate::PerceptionPlugin) is not installed. A system
//! may also write `Option<ResMut<Messages<Noise>>>` directly.
//!
//! Noises written during tick `t` before `NpcSet::Sense` are heard in tick
//! `t`, later ones in tick `t + 1`. Each is heard once. Within one hearing
//! pass noises are processed sorted ([`Noise::sort_key`]), so the order in
//! which systems wrote them never matters.

use bevy_ecs::message::{Message, Messages};
use bevy_ecs::prelude::*;
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

/// A sound at `pos` that carries `radius_mm`. A listener hears it when it is
/// within both the noise's radius and its own hearing radius. `source` (if
/// any) becomes a hearing percept; `kind` is a content bit set ("chop",
/// "door"), matched by `heard::<MASK>` conditions.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Noise {
    pub source: Option<SimId>,
    pub pos: SimPos,
    pub radius_mm: i32,
    pub kind: u32,
}

impl Noise {
    /// A noise without a source or kind.
    pub const fn at(pos: SimPos, radius_mm: i32) -> Self {
        Self {
            source: None,
            pos,
            radius_mm,
            kind: 0,
        }
    }

    pub const fn from_source(mut self, source: SimId) -> Self {
        self.source = Some(source);
        self
    }

    pub const fn kind(mut self, kind: u32) -> Self {
        self.kind = kind;
        self
    }

    /// The order noises are processed in within one tick.
    pub fn sort_key(&self) -> (Option<SimId>, i32, i32, i32, u32) {
        (
            self.source,
            self.pos.x,
            self.pos.z,
            self.radius_mm,
            self.kind,
        )
    }
}

/// Writes `noise` (no-op without the perception plugin).
pub fn emit_noise(world: &mut World, noise: Noise) {
    if let Some(mut messages) = world.get_resource_mut::<Messages<Noise>>() {
        messages.write(noise);
    }
}

/// `commands.queue(EmitNoise(noise))`: [`emit_noise`] at the next sync point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmitNoise(pub Noise);

impl Command for EmitNoise {
    type Out = ();
    fn apply(self, world: &mut World) {
        emit_noise(world, self.0);
    }
}
