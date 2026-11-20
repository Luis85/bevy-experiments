//! Rendering-independent geospatial and simulation contracts.
//!
//! Coordinates are WGS84 geodetic degrees and ellipsoidal metres. Local render
//! axes are **east, up, south**, while actor velocity is **east, north, up**.
//! Keep authoritative positions in `f64`; convert into a local frame before
//! casting to a renderer's `f32` transform.

mod actors;
mod contacts;
mod environment;
mod geo;
mod tiles;
mod time;
mod vehicle;
mod vehicle_ground;
mod world;

pub use actors::*;
pub use contacts::*;
pub use environment::*;
pub use geo::*;
pub use tiles::*;
pub use time::*;
pub use vehicle::*;
pub use vehicle_ground::*;
pub use world::*;
