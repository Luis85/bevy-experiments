use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};

use terra_core::GeoPosition;

use crate::{
    BaseWorldManifest, ClientMessage, NetError, PROTOCOL_VERSION, Result, ServerMessage,
    protocol::validate_position, transport::Connection,
};

/// Connect once outside latency-critical frame work; subsequent sends and polls
/// are nonblocking and bounded. Pending writes are flushed by `poll` as well.
pub struct NetClient {
    connection: Connection,
}

impl NetClient {
    pub fn connect(address: SocketAddr, interest: GeoPosition) -> Result<Self> {
        Self::connect_timeout(address, interest, Duration::from_secs(3))
    }

    pub fn connect_timeout(
        address: SocketAddr,
        interest: GeoPosition,
        timeout: Duration,
    ) -> Result<Self> {
        Self::connect_with_token_timeout(address, interest, None, None, timeout)
    }

    pub fn connect_with_token(
        address: SocketAddr,
        interest: GeoPosition,
        resume_token: Option<String>,
    ) -> Result<Self> {
        Self::connect_with_token_timeout(
            address,
            interest,
            resume_token,
            None,
            Duration::from_secs(3),
        )
    }

    pub fn connect_with_manifest(
        address: SocketAddr,
        interest: GeoPosition,
        resume_token: Option<String>,
        expected_manifest: BaseWorldManifest,
    ) -> Result<Self> {
        Self::connect_with_token_timeout(
            address,
            interest,
            resume_token,
            Some(expected_manifest),
            Duration::from_secs(3),
        )
    }

    fn connect_with_token_timeout(
        address: SocketAddr,
        interest: GeoPosition,
        resume_token: Option<String>,
        expected_manifest: Option<BaseWorldManifest>,
        timeout: Duration,
    ) -> Result<Self> {
        if !validate_position(interest) {
            return Err(NetError::Protocol("invalid interest position".into()));
        }
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        let mut connection = Connection::new(stream)?;
        connection.queue(&ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            interest,
            resume_token,
            expected_manifest,
        })?;
        connection.flush()?;
        Ok(Self { connection })
    }

    pub fn send(&mut self, message: ClientMessage) -> Result<()> {
        self.connection.queue(&message)?;
        self.connection.flush()
    }

    pub fn poll(&mut self) -> Result<Vec<ServerMessage>> {
        self.connection.flush()?;
        self.connection.receive(32)
    }
}
