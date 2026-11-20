//! Bounded road-vehicle ownership, physical transport work and driver progress.
//! Scenario services and offers are authored; coordinates and referenced stocks
//! use the existing world. Authority supplies movement and elapsed physical time.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{ActorKind, GeoPosition, LocalFrame};

mod actions;
mod boarding;
mod company;
mod mechanics;
mod model;
mod parking;
mod rentals;
mod seed;
mod simulation;
mod validation;
mod view;

pub use actions::{repair_price, upgrade_price};
pub use company::*;
pub use mechanics::*;
pub use model::*;
pub use rentals::{RentalOffer, RentalReturnQuote, rental_offer};

fn invalid(message: &str) -> SandboxError {
    SandboxError::Invalid(message.into())
}

fn near(a: GeoPosition, b: GeoPosition) -> bool {
    a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m) <= INTERACTION_REACH_M
}

fn outcome(message: impl Into<String>) -> Outcome {
    Outcome {
        message: message.into(),
        created_id: None,
    }
}
