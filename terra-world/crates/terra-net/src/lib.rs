//! Bounded, server-authoritative networking for a georeferenced world.
//!
//! TCP framing deliberately favors an inspectable reference implementation over
//! competitive-game transport performance. Optional rustls protects transport
//! and validates server certificates. The default bind address is loopback;
//! account enrollment and production authorization remain game-owned.
mod client;
pub mod prediction;
pub mod protocol;
pub mod replication;
mod server;
mod session;
mod surface;
#[cfg(feature = "tls")]
mod tls;
mod transport;

pub use client::NetClient;
pub use protocol::{BaseWorldManifest, ClientMessage, PROTOCOL_VERSION, ServerMessage};
pub use replication::{ActorUpdate, ReplicaResult, ReplicaState, ReplicationStats, SandboxUpdate};
pub use server::{
    FlatEnvironment, OperatorAction, OperatorAuditEntry, OperatorReceipt, PrincipalStatus, Server,
    ServerConfig, ServerReport, WorldEnvironment,
};
pub use session::SessionIdentity;
pub use surface::{ResidentSurface, SurfaceSample};
pub use terra_persistence::RegionStoreStatus;
#[cfg(feature = "tls")]
pub use tls::{ClientTlsConfig, ServerTlsConfig};
pub use transport::{FrameDecoder, MAX_FRAME_BYTES, TransportStats, encode_frame};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("network I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Publication(#[from] terra_sandbox::publication::PublicationError),
    #[error("invalid JSON frame: {0}")]
    Json(#[from] serde_json::Error),
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("peer disconnected")]
    Disconnected,
    #[error("invalid server configuration: {0}")]
    Configuration(String),
    #[error("sandbox rule: {0}")]
    Sandbox(#[from] terra_sandbox::SandboxError),
    #[error("regional storage: {0}")]
    Persistence(String),
}

pub type Result<T> = std::result::Result<T, NetError>;
