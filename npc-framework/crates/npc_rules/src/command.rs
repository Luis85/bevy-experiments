//! The recorded command that spends banked points: [`SpendPoints`].
//!
//! `RulesPlugin` does **not** register it (registering a command changes the
//! content hash): a host that wants replayable spending registers it with
//! `app.register_command::<SpendPoints>()`, and the facade's rules × agent-io
//! glue does so for agents. JSON:
//! `{"type":"SpendPoints","unit":5,"purchase":{"type":"Skill","skill":"Climbing","points":2}}`.

use npc_kernel::SimId;
use npc_kernel::commands::{CommandError, ContentCommand};
use npc_kernel::ids::SimIndex;
use serde::{Deserialize, Serialize};

use crate::roll::{purchase_cost, sheet, spend_points};
use crate::sheet::{Purchase, SpendError};

/// Pays `purchase` from `unit`'s banked points when it applies (validated
/// at submit and again at apply: unit, sheet, purchase, enough points).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendPoints {
    pub unit: u32,
    pub purchase: Purchase,
}

/// A [`SpendError`] as a command error (`field: "purchase"`, the error's
/// JSON as the reason).
pub fn spend_command_error(e: &SpendError) -> CommandError {
    CommandError::InvalidValue {
        field: "purchase".to_owned(),
        reason: serde_json::to_string(e).unwrap_or_else(|_| format!("{e:?}")),
    }
}

impl SpendPoints {
    /// The validated cost of the purchase for its unit now.
    pub fn check(&self, world: &bevy_ecs::world::World) -> Result<i32, SpendError> {
        let unit = SimId(self.unit);
        let s = sheet(world, unit).ok_or(SpendError::NoSheet { unit: self.unit })?;
        let cost = purchase_cost(world, unit, &self.purchase)?;
        if cost > s.unspent {
            return Err(SpendError::NotEnoughPoints {
                cost,
                unspent: s.unspent,
            });
        }
        Ok(cost)
    }
}

impl ContentCommand for SpendPoints {
    const NAME: &'static str = "SpendPoints";

    fn validate(&self, world: &bevy_ecs::world::World) -> Result<(), CommandError> {
        let exists = world
            .get_resource::<SimIndex>()
            .and_then(|i| i.get(SimId(self.unit)))
            .is_some();
        if !exists {
            return Err(CommandError::UnknownUnit { unit: self.unit });
        }
        match self.check(world) {
            Ok(_) => Ok(()),
            Err(SpendError::NoSheet { unit }) => Err(CommandError::WrongKind {
                unit,
                expected: "unit with a character sheet".to_owned(),
            }),
            Err(e) => Err(spend_command_error(&e)),
        }
    }

    fn apply(self, world: &mut bevy_ecs::world::World) {
        // Validated just before; a failure leaves the sheet unchanged.
        let _ = spend_points(world, SimId(self.unit), &self.purchase);
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
