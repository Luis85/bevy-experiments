//! Deterministic settlement simulation. Money uses integer hundredths of a credit.
//! Source-derived positions are evidence about placement, never claims that the
//! invented inhabitants, stock, businesses or production rates exist in reality.
use crate::catalog::{exchange, inventory, validate_inventory};
use crate::{Catalog, Inventory, Outcome, SandboxError, SandboxState, finite_range, valid_id};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{ActorKind, ActorState, FeatureKind, GeoPosition, LocalFrame, WorldFeature};

mod commands;
mod model;
mod routing;
mod seed;
mod simulation;
mod validation;
mod view;

pub use model::*;
