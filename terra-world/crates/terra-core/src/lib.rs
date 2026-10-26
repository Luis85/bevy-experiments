//! Rendering-independent geospatial and simulation contracts.
//!
//! Coordinates are WGS84 geodetic degrees and ellipsoidal metres. Local render
//! axes are **east, up, south**, while actor velocity is **east, north, up**.
//! Keep authoritative positions in `f64`; convert into a local frame before
//! casting to a renderer's `f32` transform.

mod actors;
mod environment;
mod geo;
mod tiles;
mod time;
mod world;

pub use actors::*;
pub use environment::*;
pub use geo::*;
pub use tiles::*;
pub use time::*;
pub use world::*;
