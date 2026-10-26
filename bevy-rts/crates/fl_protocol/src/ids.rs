//! Tick, player, sim and interned id newtypes.

use core::fmt;
use serde::{Deserialize, Serialize};

/// Fixed simulation rate in ticks per second.
pub const SIM_TICK_HZ: u32 = 20;

/// Simulation tick counter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct Tick(pub u32);

impl Tick {
    /// The following tick.
    #[must_use]
    pub fn next(self) -> Tick {
        Tick(self.0 + 1)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Player slot id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct PlayerId(pub u8);

/// Team id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct TeamId(pub u8);

/// Stable simulation entity id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct SimId(pub u32);

impl fmt::Display for SimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Interned command kind id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct CommandTypeId(pub u16);

/// Interned unit type id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct UnitTypeId(pub u16);

/// Interned faction id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct FactionId(pub u16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_ordering() {
        assert!(Tick(3) < Tick(4));
        assert_eq!(Tick(3).next(), Tick(4));
        assert_eq!(Tick(5).to_string(), "5");
    }

    #[test]
    fn sim_id_display() {
        assert_eq!(SimId(12).to_string(), "#12");
    }
}
