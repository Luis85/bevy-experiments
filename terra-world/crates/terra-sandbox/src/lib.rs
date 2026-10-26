//! Persistent sandbox overlays and authoritative, transactional game commands.

mod catalog;
mod commands;
mod factions;
mod persistence;
mod state;

pub use catalog::*;
pub use commands::*;
pub use state::*;

use thiserror::Error;

pub const SAVE_VERSION: u32 = 1;
pub const INTERACTION_REACH_M: f64 = 20.0;
pub const MAX_ENTITIES: usize = 10_000;
pub const MAX_PLAYERS: usize = 128;
pub const MAX_FACTIONS: usize = 64;
pub const MAX_PLAYER_FACTIONS: usize = 8;
pub const MAX_INVENTORY_ITEMS: usize = 64;
pub const MAX_ITEM_COUNT: u32 = 1_000_000;
pub const MAX_SAVE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("player does not exist")]
    PlayerMissing,
    #[error("object does not exist: {0}")]
    Missing(String),
    #[error("invalid command or state: {0}")]
    Invalid(String),
    #[error("target is outside the {INTERACTION_REACH_M} metre interaction range")]
    OutOfReach,
    #[error("your ownership or faction role does not permit this action on the target or land")]
    NotOwner,
    #[error("land claim or structure overlaps an occupied area")]
    Overlap,
    #[error("not enough {0}")]
    Insufficient(String),
    #[error("world or inventory capacity reached")]
    Capacity,
    #[error("crop is not ready to harvest")]
    NotReady,
    #[error("a nearby personal or faction-shared workbench is required")]
    WorkbenchRequired,
    #[error("unsupported save version {0}")]
    SaveVersion(u32),
    #[error(transparent)]
    Geo(#[from] terra_core::GeoError),
    #[error(transparent)]
    Data(#[from] terra_core::WorldDataError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:-./".contains(&b))
}

pub(crate) fn finite_range(value: f64, min: f64, max: f64) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}
