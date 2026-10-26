//! Deterministic maths for the Frontline simulation.
//!
//! Everything the simulation computes goes through this crate: [`Fx`]
//! fixed-point scalars, [`FxVec2`]/[`FxVec3`] vectors, [`Angle`] binary
//! angles with table-driven trigonometry, and keyed counter-based random
//! numbers ([`roll`], [`Rolls`]), plus [`Pcg32`] for tools. All of it is
//! pure integer arithmetic, so results are bit-identical on every platform.
//! Floating point appears only at the edges: loading content
//! (`Fx::from_f64`) and presentation (`to_f32`, `to_f64`,
//! `Angle::to_radians_f32`).

#![deny(clippy::float_arithmetic)]

mod angle;
mod fx;
mod rng;
mod trig;
mod vec;

pub use angle::Angle;
pub use fx::{Fx, FxRangeError};
pub use rng::{Pcg32, Rolls, roll, stream_key};
pub use vec::{FxVec2, FxVec3};
