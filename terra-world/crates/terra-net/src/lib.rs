//! Bounded, server-authoritative networking for a georeferenced world.
//!
//! TCP framing deliberately favors an inspectable reference implementation over
//! competitive-game transport performance. No authentication or TLS is included;
//! the default bind address is loopback. Public deployment requires a secure
//! transport and an identity/authorization layer.
mod client;
pub mod protocol;
mod server;
mod session;
mod transport;

pub use client::NetClient;
pub use protocol::{BaseWorldManifest, ClientMessage, PROTOCOL_VERSION, ServerMessage};
pub use server::{FlatEnvironment, Server, ServerConfig, ServerReport, WorldEnvironment};
pub use session::SessionIdentity;
pub use transport::{FrameDecoder, MAX_FRAME_BYTES, encode_frame};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("network I/O: {0}")]
    Io(#[from] std::io::Error),
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
}

pub type Result<T> = std::result::Result<T, NetError>;
