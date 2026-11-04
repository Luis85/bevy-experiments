//! Immutable bind result shared by pixels, previews and effect sockets.
use super::{LocalMesh, MaskRect, SourceChannel, SourceToDisplay};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerAction {
    Bind,
    Hold,
    Reload { phase: f32 },
    Melee { phase: f32 },
    Throw { phase: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerPoseSample {
    pub tick: u64,
    pub aim: [f32; 2],
    pub action: PlayerAction,
    pub recoil: f32,
    pub brace: f32,
    pub hit: f32,
    pub reduced_effects: bool,
}

// Read by review verification (`review` feature) and tests.
#[cfg_attr(not(feature = "review"), allow(dead_code))]
#[derive(Clone, Debug)]
pub struct DrawPacket {
    pub part_id: String,
    pub source_id: String,
    pub channel: SourceChannel,
    pub slot: String,
    pub material: String,
    pub draw_order: i32,
    pub rectangles: Arc<[MaskRect]>,
    pub transform: SourceToDisplay,
    /// Cached source-local topology; the backend only transforms positions.
    pub local_mesh: Arc<LocalMesh>,
    pub source_to_bind: super::bind::Affine2,
    pub bone_transform: super::bind::Affine2,
}
// Read by review verification (`review` feature) and tests.
#[cfg_attr(not(feature = "review"), allow(dead_code))]
#[derive(Clone, Debug)]
pub struct ResolvedPlayerPose {
    pub model_id: String,
    pub skin_id: String,
    pub packets: Vec<DrawPacket>,
    pub sockets: BTreeMap<String, [f32; 2]>,
    pub source_sockets: BTreeMap<String, [f32; 2]>,
    pub muzzle: [f32; 2],
    pub throw_origin: [f32; 2],
}
