//! A reusable streamed environment. Games keep their own actor AI, physics and UI.
pub mod config;
pub mod environment;
#[cfg(feature = "render")]
pub mod geometry;
pub mod history;
pub mod network;
pub mod politics;
#[cfg(feature = "render")]
pub mod render;
pub mod sandbox;
pub mod simulation;

pub use config::WorldConfig;
pub use environment::{EnvironmentSample, WorldEnvironment};
pub use network::{NetworkSession, NetworkStatus, TerraNetworkPlugin};
pub use politics::PoliticalWorld;
#[cfg(feature = "render")]
pub use render::TerraRenderPlugin;
pub use sandbox::{SandboxIntent, SandboxQueue, SandboxRuntime};
pub use simulation::{
    GeoActor, GeoAnchor, ResidentChunks, TerraWorldPlugin, WorldRuntime, WorldSet,
};
pub use terra_core as core;
pub use terra_sandbox as rules;
