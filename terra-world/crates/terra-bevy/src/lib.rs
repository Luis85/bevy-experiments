//! A reusable streamed environment. Games keep their own actor AI, physics and UI.
pub mod asset_pack;
pub mod config;
pub mod environment;
#[cfg(feature = "render")]
pub mod geometry;
pub mod history;
pub mod living;
pub mod network;
pub mod politics;
pub mod prediction;
pub mod recovery;
pub mod regions;
#[cfg(feature = "render")]
pub mod render;
pub mod sandbox;
pub mod simulation;
pub mod telemetry;

pub use config::WorldConfig;
pub use environment::{EnvironmentSample, WorldEnvironment};
pub use living::{EcologyRuntime, LivingDiagnostics, LivingMotion, TerraLivingPlugin};
pub use network::{
    NetworkSecurity, NetworkSession, NetworkStatus, SecuredNetworkPlugin, TerraNetworkPlugin,
};
pub use politics::PoliticalWorld;
pub use regions::{RegionPersistenceSet, RegionalPersistence};
#[cfg(feature = "render")]
pub use render::TerraRenderPlugin;
pub use sandbox::{ReviewedRestore, SandboxIntent, SandboxQueue, SandboxRuntime};
pub use simulation::{
    GeoActor, GeoAnchor, ResidentChunks, TerraWorldPlugin, WorldRuntime, WorldSet,
};
pub use telemetry::{TimingSummary, WorldDiagnostics, WorldStage, WorldTimingReport};
pub use terra_core as core;
pub use terra_net::PROTOCOL_VERSION as NETWORK_PROTOCOL_VERSION;
pub use terra_sandbox as rules;
