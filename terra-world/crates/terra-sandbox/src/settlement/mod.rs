//! Authoritative gameplay territories, funded work and the reference settlement economy.
//! Regions are player-created rights, not claims about political sovereignty.
use crate::catalog::{exchange, inventory, validate_inventory};
use crate::{
    Catalog, Inventory, MAX_MONEY, Outcome, SandboxError, SandboxState, TradePrice, UnitRole,
    finite_range, valid_id,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::GeoPosition;

mod commands;
mod demand;
mod economy;
mod model;
mod supply;
mod validation;

pub use model::*;
pub use supply::{SupplyContractAction, SupplyContractBlocker};
