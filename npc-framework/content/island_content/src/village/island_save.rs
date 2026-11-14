//! What an island save says about the build that made it, and what this
//! build can do with it.
//!
//! The kernel's snapshot already carries its format version and the content
//! hash of the build that saved it; any change to the living world's data or
//! sim changes that hash, so a strict load of an older save fails with a
//! bare hash mismatch. [`save_island`] stamps the snapshot's header (host
//! data, outside every hash) with the island's save version, and
//! [`check_island_save`] turns a mismatch into a typed answer the host can
//! say plainly: the save is current, or was made by an older (or newer)
//! version of the island, or by another build of the same version. Only a
//! current save loads; nothing is migrated.
//!
//! The content hash covers the registered names and the trees only, not the
//! world's data; the state hash stamped at save (`island_state_hash`) is
//! recomputed after the load, so a save restored into different rules is
//! refused. [`load_island_save`] is the one gate: stamp, structure, strict
//! load, nothing defaulted or skipped, the state hash.
use bevy_ecs::prelude::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::registry::content_hash;
use npc_framework::snapshot::{self, LoadError, LoadMode, LoadReport, WorldSnapshot};
use serde::Serialize;
use serde_json::Value;

/// The island's save version: bumped with every change to the living world's
/// saved state. Saves made before the stamp existed have none.
pub const ISLAND_SAVE_VERSION: u32 = 4;
/// Header keys the stamp uses.
pub const HEADER_VERSION: &str = "island_save_version";
pub const HEADER_CONTENT: &str = "island_content_hash";
pub const HEADER_STATE: &str = "island_state_hash";

/// Who made a save and what this build makes of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IslandSaveCheck {
    /// Made by this build: loads as saved.
    Current,
    /// Made by an older version of the island (`saved` is `None` for a save
    /// from before the stamp): start a new island.
    OlderIsland { saved: Option<u32>, current: u32 },
    /// Made by a newer version of the island than this build.
    NewerIsland { saved: u32, current: u32 },
    /// Same island version, but made by a different build of the game (its
    /// registered names or trees differ).
    OtherBuild,
    /// Not a snapshot this build can read at all.
    Unreadable { reason: String },
}

impl IslandSaveCheck {
    /// One sentence for the player.
    pub fn sentence(&self) -> String {
        match self {
            Self::Current => "This save was made by this version of the island.".into(),
            Self::OlderIsland { .. } => "This save was made by an older version of the island and cannot be continued in this one. Start a new island.".into(),
            Self::NewerIsland { .. } => "This save was made by a newer version of the island. Update the game to continue it.".into(),
            Self::OtherBuild => "This save was made by a different build of the game and cannot be continued.".into(),
            Self::Unreadable { reason } => reason.clone(),
        }
    }
}

/// The island's identity in this build: save version and content hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct IslandIdentity {
    pub island_save_version: u32,
    pub content_hash: u64,
}

/// This build's island identity.
pub fn identity(world: &mut World) -> IslandIdentity {
    IslandIdentity {
        island_save_version: ISLAND_SAVE_VERSION,
        content_hash: content_hash(world),
    }
}

/// [`snapshot::save`] with the island's stamp in the header.
///
/// # Errors
/// What [`snapshot::save`] reports.
pub fn save_island(world: &mut World) -> Result<WorldSnapshot, String> {
    let mut snap = snapshot::save(world)?;
    stamp(world, &mut snap);
    Ok(snap)
}

/// Writes this build's identity into a snapshot's header.
pub fn stamp(world: &mut World, snap: &mut WorldSnapshot) {
    let me = identity(world);
    snap.header
        .insert(HEADER_VERSION.into(), Value::from(me.island_save_version));
    // As strings: a u64 does not survive every JSON reader.
    snap.header.insert(
        HEADER_CONTENT.into(),
        Value::from(format!("{:016x}", me.content_hash)),
    );
    snap.header.insert(
        HEADER_STATE.into(),
        Value::from(format!("{:016x}", state_hash(world))),
    );
}

/// A 16-hex-digit header stamp.
fn hex_stamp(snap: &WorldSnapshot, key: &str) -> Option<u64> {
    let text = snap.header.get(key)?.as_str()?;
    (text.len() == 16)
        .then(|| u64::from_str_radix(text, 16).ok())
        .flatten()
}

const NO_VERSION: &str = "This save has no readable island version.";
const NO_CONTENT: &str = "This save has no readable content stamp.";
const OTHER_WORLD: &str = "This save does not match the world it was made in.";

/// The island save version a snapshot was stamped with, if any.
pub fn saved_version(snap: &WorldSnapshot) -> Option<u32> {
    snap.header
        .get(HEADER_VERSION)
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
}

/// What this build makes of `snap` (`world` is a fresh world of this build).
pub fn check_island_save(world: &mut World, snap: &WorldSnapshot) -> IslandSaveCheck {
    check_against(world, snap, ISLAND_SAVE_VERSION)
}

/// [`check_island_save`] against a given current version (tests use it to
/// stand in for a later build).
pub fn check_against(world: &mut World, snap: &WorldSnapshot, current: u32) -> IslandSaveCheck {
    let unreadable = |reason: &str| IslandSaveCheck::Unreadable {
        reason: reason.into(),
    };
    let Some(saved) = saved_version(snap) else {
        return unreadable(NO_VERSION);
    };
    let Some(stamped) = hex_stamp(snap, HEADER_CONTENT) else {
        return unreadable(NO_CONTENT);
    };
    if saved > current {
        return IslandSaveCheck::NewerIsland { saved, current };
    }
    if saved < current {
        return IslandSaveCheck::OlderIsland {
            saved: Some(saved),
            current,
        };
    }
    if stamped != content_hash(world) {
        return IslandSaveCheck::OtherBuild;
    }
    match snapshot::check(world, snap) {
        Ok(()) => IslandSaveCheck::Current,
        Err(LoadError::Bad(reason)) => IslandSaveCheck::Unreadable {
            reason: format!("This save cannot be read: {reason}."),
        },
        Err(_) => IslandSaveCheck::OtherBuild,
    }
}

/// Loads a save of this build (a strict load); refuses every other.
///
/// # Errors
/// The [`IslandSaveCheck`] that refuses it (older, newer, unreadable), or
/// [`IslandSaveCheck::Unreadable`] when the load itself fails. The world is
/// untouched unless the load started (then build a fresh app before trying
/// again, as for [`snapshot::load`]).
pub fn load_island_save(
    world: &mut World,
    snap: &WorldSnapshot,
) -> Result<LoadReport, IslandSaveCheck> {
    let unreadable = |reason: String| IslandSaveCheck::Unreadable { reason };
    match check_island_save(world, snap) {
        IslandSaveCheck::Current => {}
        refused => return Err(refused),
    }
    let Some(stamped_state) = hex_stamp(snap, HEADER_STATE) else {
        return Err(unreadable(OTHER_WORLD.into()));
    };
    // The rules this app was built from; the save must carry the same.
    let built = world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .map(crate::world_runtime::ActiveWorld::canonical);
    let expected = snapshot::save(world).map_err(unreadable)?;
    super::integrity::validate_snapshot_structure(&expected, snap).map_err(unreadable)?;
    let report = snapshot::load(world, snap, LoadMode::Strict)
        .map_err(|error| unreadable(format!("This save could not be loaded: {error}.")))?;
    if !report.defaulted.is_empty() || !report.skipped.is_empty() {
        return Err(unreadable("This save is incomplete.".into()));
    }
    let loaded = world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .map(crate::world_runtime::ActiveWorld::canonical);
    if state_hash(world) != stamped_state || loaded != built {
        return Err(unreadable(OTHER_WORLD.into()));
    }
    Ok(report)
}

/// The world rules a save was made under, read from the snapshot without
/// an app: build the app for Continue from these (never from the installed
/// world file), then [`load_island_save`].
///
/// # Errors
/// [`IslandSaveCheck::Unreadable`] when the save carries no readable rules.
pub fn saved_rules(snap: &WorldSnapshot) -> Result<crate::worlds::WorldPreset, IslandSaveCheck> {
    use npc_framework::kernel::hash::SimHashResource;
    let value = snap
        .resources
        .get(crate::world_runtime::ActiveWorld::NAME)
        .ok_or_else(|| IslandSaveCheck::Unreadable {
            reason: "This save does not carry its world rules.".into(),
        })?;
    serde_json::from_value::<crate::world_runtime::ActiveWorld>(value.clone())
        .map(|rules| rules.0)
        .map_err(|error| IslandSaveCheck::Unreadable {
            reason: format!("This save's world rules cannot be read: {error}."),
        })
}
