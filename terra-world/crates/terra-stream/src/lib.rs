//! Real geographic chunks, bounded background loading, and traceable data provenance.
//!
//! [`StreamConfig::default`] never accesses the network. Set `live` explicitly to
//! enable the real-data adapters. A failed layer is either reported as a synthetic
//! fallback with a warning, or returned as an error when `allow_fallback` is false.
//! Public API servers are demo inputs, not a production world distribution CDN.

mod cache;
mod clip;
mod config;
mod procedural;
pub mod providers;
mod streamer;

pub use config::{StreamConfig, StreamError};
pub use procedural::{procedural_chunk, procedural_height};
pub use providers::{fetch_weather, load_chunk};
pub use streamer::{ChunkPayload, ChunkProvider, ChunkStreamer, StreamEvent, StreamStats};
