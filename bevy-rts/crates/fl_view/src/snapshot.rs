//! The per-tick snapshot the sim's `Publish` phase writes (spec §5.3).

use fl_math::{Angle, FxVec2};
use fl_protocol::{PlayerId, SimId, Tick, UnitTypeId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The sim state the client may see after one tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimSnapshot {
    pub tick: Tick,
    /// One record per entity, sorted by `sim_id` ([`SimSnapshot::get`] relies on it).
    pub entities: Vec<EntityRecord>,
    /// View channels as `(name, encoded value)`, sorted by name.
    pub channels: Vec<(String, Vec<u8>)>,
}

impl SimSnapshot {
    /// The record of `id`, by binary search over the sorted `entities`.
    #[must_use]
    pub fn get(&self, id: SimId) -> Option<&EntityRecord> {
        self.entities
            .binary_search_by_key(&id, |record| record.sim_id)
            .ok()
            .map(|index| &self.entities[index])
    }

    /// The value of channel `C`: `None` when the snapshot doesn't carry it, `Some(Err)` when it
    /// doesn't decode as `C`.
    #[must_use]
    pub fn channel<C: ViewChannel>(&self) -> Option<Result<C, String>> {
        let index = self.channel_index(C::NAME).ok()?;
        Some(postcard::from_bytes(&self.channels[index].1).map_err(|err| err.to_string()))
    }

    /// Stores `value` as channel `C` (postcard-encoded, so `Fx` keeps its exact raw value),
    /// replacing any previous value and keeping `channels` sorted by name.
    ///
    /// # Panics
    ///
    /// If `C` can't be encoded, which is a bug in the channel type (for example a
    /// `#[serde(skip_serializing_if)]` field or a sequence of unknown length).
    pub fn set_channel<C: ViewChannel>(&mut self, value: &C) {
        let bytes = postcard::to_allocvec(value)
            .unwrap_or_else(|err| panic!("view channel `{}` failed to encode: {err}", C::NAME));
        match self.channel_index(C::NAME) {
            Ok(index) => self.channels[index].1 = bytes,
            Err(index) => self.channels.insert(index, (C::NAME.to_owned(), bytes)),
        }
    }

    fn channel_index(&self, name: &str) -> Result<usize, usize> {
        self.channels
            .binary_search_by(|(channel, _)| channel.as_str().cmp(name))
    }
}

/// What the client sees of one sim entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRecord {
    pub sim_id: SimId,
    pub unit: UnitTypeId,
    pub owner: PlayerId,
    /// Position in metres, `x` east and `y` north.
    pub pos: FxVec2,
    pub heading: Angle,
    /// Turret angles, in the unit's turret order.
    pub turrets: Vec<Angle>,
    /// Hit points as a fraction of maximum, `0..=65535` (see [`EntityRecord::hp_frac`]).
    pub hp_frac: u16,
    /// State bits from [`flags`].
    pub flags: u32,
    /// Bit `t` is set when team `t` sees the entity.
    pub visible_to: u8,
}

impl EntityRecord {
    /// `hp * 65535 / max`, rounded down and capped at 65535; 0 when `max` is 0.
    #[must_use]
    pub fn hp_frac(hp: u32, max: u32) -> u16 {
        if max == 0 {
            return 0;
        }
        let frac = u64::from(hp) * u64::from(u16::MAX) / u64::from(max);
        u16::try_from(frac).unwrap_or(u16::MAX)
    }
}

/// Bits of [`EntityRecord::flags`].
pub mod flags {
    /// The entity is moving this tick.
    pub const MOVING: u32 = 1;
    /// The entity takes no damage (`debug.invulnerable`).
    pub const INVULNERABLE: u32 = 2;
}

/// A feature's snapshot extension: plain data published under a stable, namespaced name.
///
/// Channel data types live in the feature sim crate's `view` module and are never `Component`s.
pub trait ViewChannel: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Unique, stable name such as `"movement.paths"`.
    const NAME: &'static str;
}
