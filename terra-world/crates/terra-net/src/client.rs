use std::{
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

use terra_core::GeoPosition;

use crate::{
    BaseWorldManifest, ClientMessage, NetError, PROTOCOL_VERSION, Result, ServerMessage,
    TransportStats,
    protocol::validate_position,
    replication::{ReplicaResult, ReplicaState, ReplicationStats},
    transport::Connection,
};

/// Connect once outside latency-critical frame work; subsequent sends and polls
/// are nonblocking and bounded. Pending writes are flushed by `poll` as well.
pub struct NetClient {
    connection: Connection,
    replica: ReplicaState,
    expected_manifest: Option<BaseWorldManifest>,
    resync_pending: bool,
    actor_keyframe_pending: bool,
    sandbox_keyframe_pending: bool,
    resync_needed: bool,
    resync_retry_at: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn welcome() -> ServerMessage {
        ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client_id: 1,
            actor_id: 1,
            manifest: BaseWorldManifest::default(),
            tick: 0,
            unix_seconds: 100.,
            time_scale: 60.,
            paused: false,
            tick_hz: 20,
            spawn: GeoPosition::new(0., 0., 0.).unwrap(),
            resume_token: "a".repeat(64),
            next_sandbox_sequence: 0,
        }
    }

    fn peer() -> (NetClient, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = NetClient::connect_with_manifest(
            listener.local_addr().unwrap(),
            GeoPosition::new(0., 0., 0.).unwrap(),
            None,
            BaseWorldManifest::default(),
        )
        .unwrap();
        let (socket, _) = listener.accept().unwrap();
        socket.set_nodelay(true).unwrap();
        (client, socket)
    }

    fn next(client: &mut NetClient) -> Result<Vec<ServerMessage>> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match client.poll() {
                Ok(messages) if messages.is_empty() => {}
                result => return result,
            }
            assert!(
                Instant::now() < deadline,
                "peer did not produce an admission outcome"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn malformed_wire_welcomes_never_bind_or_publish_capabilities() {
        use std::io::Write;
        let changes = [
            ("protocol", serde_json::json!(0)),
            ("client_id", serde_json::json!(0)),
            ("actor_id", serde_json::json!(0)),
            ("unix_seconds", serde_json::json!(1e12 + 1.)),
            ("time_scale", serde_json::json!(-1.)),
            ("time_scale", serde_json::json!(86401.)),
            ("tick_hz", serde_json::json!(0)),
            ("tick_hz", serde_json::json!(121)),
            ("resume_token", serde_json::json!("z".repeat(64))),
            ("resume_token", serde_json::json!("a".repeat(63))),
            ("next_sandbox_sequence", serde_json::json!(u64::MAX)),
            (
                "spawn",
                serde_json::json!({"latitude":0.,"longitude":181.,"altitude_m":0.}),
            ),
        ];
        for (field, value) in changes {
            let (mut client, mut socket) = peer();
            let mut bad = serde_json::to_value(welcome()).unwrap();
            bad[field] = value;
            let message: ServerMessage = serde_json::from_value(bad).unwrap();
            socket
                .write_all(&crate::encode_frame(&message).unwrap())
                .unwrap();
            assert!(
                matches!(next(&mut client), Err(NetError::Protocol(_))),
                "{field}"
            );
            assert_eq!(client.replication_stats().keyframes, 0);
            assert_eq!(client.replica.actor_revision(), 0);
            assert_eq!(client.replica.sandbox_revision(), 0);
            // A second valid welcome can still bind, proving the bad frame did
            // not install an identity. No capability-bearing event was returned.
            socket
                .write_all(&crate::encode_frame(&welcome()).unwrap())
                .unwrap();
            assert!(matches!(
                next(&mut client).unwrap().as_slice(),
                [ServerMessage::Welcome { actor_id: 1, .. }]
            ));
        }
    }

    #[test]
    fn requested_source_is_checked_before_binding_but_world_names_are_independent() {
        use std::io::Write;
        let (mut client, mut socket) = peer();
        let mut bad = welcome();
        if let ServerMessage::Welcome { manifest, .. } = &mut bad {
            manifest.seed += 1;
        }
        socket
            .write_all(&crate::encode_frame(&bad).unwrap())
            .unwrap();
        assert!(matches!(next(&mut client), Err(NetError::Protocol(_))));
        let mut good = welcome();
        if let ServerMessage::Welcome { manifest, .. } = &mut good {
            manifest.world_id = "another-save-name".into();
        }
        socket
            .write_all(&crate::encode_frame(&good).unwrap())
            .unwrap();
        assert!(matches!(
            next(&mut client).unwrap().as_slice(),
            [ServerMessage::Welcome { .. }]
        ));
        socket
            .write_all(&crate::encode_frame(&welcome()).unwrap())
            .unwrap();
        assert!(matches!(next(&mut client), Err(NetError::Protocol(_))));
    }

    #[test]
    fn normalized_snapshots_are_output_events_not_remote_replication_shortcuts() {
        use std::io::Write;
        let origin = GeoPosition::new(0., 0., 0.).unwrap();
        let mut state = terra_sandbox::SandboxState::new();
        state.add_player(1, origin).unwrap();
        for admitted in [false, true] {
            for raw in [
                ServerMessage::Snapshot {
                    tick: 1,
                    unix_seconds: 100.,
                    actors: vec![],
                    truncated: false,
                    total_in_interest: 0,
                },
                ServerMessage::SandboxSnapshot {
                    tick: 1,
                    view: state.snapshot_for(1, 5000.).unwrap(),
                },
            ] {
                let (mut client, mut socket) = peer();
                if admitted {
                    socket
                        .write_all(&crate::encode_frame(&welcome()).unwrap())
                        .unwrap();
                    assert!(matches!(
                        next(&mut client).unwrap().as_slice(),
                        [ServerMessage::Welcome { .. }]
                    ));
                }
                socket
                    .write_all(&crate::encode_frame(&raw).unwrap())
                    .unwrap();
                assert!(matches!(next(&mut client), Err(NetError::Protocol(_))));
                assert_eq!(client.replica.actor_revision(), 0);
                assert_eq!(client.replica.sandbox_revision(), 0);
                assert_eq!(client.replication_stats().keyframes, 0);
            }
        }
    }

    #[test]
    fn failed_resync_queue_admission_does_not_install_a_phantom_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = NetClient::connect(
            listener.local_addr().unwrap(),
            GeoPosition::new(0., 0., 0.).unwrap(),
        )
        .unwrap();
        let (_socket, _) = listener.accept().unwrap();
        // The first maximum JSON string costs MAX_FRAME_BYTES+4. The second
        // leaves exactly two bytes free, too little for a Resync frame.
        client
            .connection
            .queue(&"a".repeat(crate::MAX_FRAME_BYTES - 2))
            .unwrap();
        client
            .connection
            .queue(&"b".repeat(crate::MAX_FRAME_BYTES - 12))
            .unwrap();
        let before = client.connection.stats();
        assert!(client.send(ClientMessage::Resync {}).is_err());
        assert!(!client.resync_pending());
        assert_eq!(client.replication_stats().resync_requests, 0);
        assert_eq!(
            client.connection.stats().pending_bytes,
            before.pending_bytes
        );
        assert!(!client.actor_keyframe_pending && !client.sandbox_keyframe_pending);
    }
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
        Self::from_connection(
            Connection::new(stream)?,
            interest,
            resume_token,
            expected_manifest,
        )
    }

    /// TLS setup is nonblocking after the initial bounded TCP connect. Resume
    /// capabilities remain buffered until trusted-CA and hostname verification
    /// succeeds; `poll` progresses the TLS and application handshakes.
    #[cfg(feature = "tls")]
    pub fn connect_tls(
        address: SocketAddr,
        interest: GeoPosition,
        resume_token: Option<String>,
        expected_manifest: Option<BaseWorldManifest>,
        tls: crate::ClientTlsConfig,
        timeout: Duration,
    ) -> Result<Self> {
        if !validate_position(interest) {
            return Err(NetError::Protocol("invalid interest position".into()));
        }
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        Self::from_connection(
            Connection::new_tls_client(stream, &tls)?,
            interest,
            resume_token,
            expected_manifest,
        )
    }

    fn from_connection(
        mut connection: Connection,
        interest: GeoPosition,
        resume_token: Option<String>,
        expected_manifest: Option<BaseWorldManifest>,
    ) -> Result<Self> {
        connection.queue(&ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            interest,
            resume_token,
            expected_manifest: expected_manifest.clone(),
        })?;
        connection.flush()?;
        Ok(Self {
            connection,
            replica: ReplicaState::default(),
            expected_manifest,
            resync_pending: false,
            actor_keyframe_pending: false,
            sandbox_keyframe_pending: false,
            resync_needed: false,
            resync_retry_at: None,
        })
    }

    pub fn send(&mut self, message: ClientMessage) -> Result<()> {
        let resync = matches!(message, ClientMessage::Resync {});
        if resync {
            if self.resync_pending {
                return Ok(());
            }
            if self.resync_retry_at.is_some_and(|at| Instant::now() < at) {
                self.resync_needed = true;
                return Ok(());
            }
        }
        // Queue admission precedes installing request state. A full outbound
        // queue must not leave a phantom request suppressing all later retries.
        self.connection.queue(&message)?;
        if resync {
            self.resync_pending = true;
            self.resync_needed = true;
            self.resync_retry_at = None;
            self.actor_keyframe_pending = true;
            self.sandbox_keyframe_pending = true;
            self.replica.stats.resync_requests =
                self.replica.stats.resync_requests.saturating_add(1);
        }
        self.connection.flush()
    }

    pub fn replication_stats(&self) -> &ReplicationStats {
        &self.replica.stats
    }
    pub fn transport_stats(&self) -> TransportStats {
        self.connection.stats()
    }
    /// True only after TLS certificate/protocol verification has completed.
    pub fn is_encrypted(&self) -> bool {
        self.connection.is_encrypted()
    }
    pub fn resync_pending(&self) -> bool {
        self.resync_pending || self.resync_needed
    }

    /// Normalize complete/delta wire updates to the existing view API. Missing
    /// baselines trigger one resync request; malformed payloads fail atomically.
    /// Welcome admission and a requested source manifest are checked before
    /// binding. Games still validate their catalog/gameplay policy. Direct wire
    /// Snapshot/SandboxSnapshot events are rejected; only revisioned updates
    /// produce these normalized output variants.
    pub fn poll(&mut self) -> Result<Vec<ServerMessage>> {
        self.connection.flush()?;
        let received = self.connection.receive::<ServerMessage>(32)?;
        let mut messages = Vec::with_capacity(received.len());
        for message in received {
            let result = match message {
                ServerMessage::Welcome {
                    actor_id,
                    ref manifest,
                    ..
                } => {
                    message.validate_welcome()?;
                    if self
                        .expected_manifest
                        .as_ref()
                        .is_some_and(|expected| !manifest.matches_data(expected))
                    {
                        return Err(NetError::Protocol(
                            "server source manifest differs from requested data".into(),
                        ));
                    }
                    self.replica.bind_player(actor_id)?;
                    messages.push(message);
                    continue;
                }
                // These variants are SDK events produced by ReplicaState,
                // never revisionless shortcuts accepted from a remote peer.
                ServerMessage::Snapshot { .. } | ServerMessage::SandboxSnapshot { .. } => {
                    return Err(NetError::Protocol(
                        "normalized snapshot received directly on wire".into(),
                    ));
                }
                ServerMessage::ActorUpdate { update } => {
                    let full = update.base_revision.is_none();
                    let result = self.replica.apply_actor(update)?;
                    if full && matches!(result, ReplicaResult::Applied(_)) {
                        self.actor_keyframe_pending = false;
                    }
                    result
                }
                ServerMessage::SandboxUpdate { update } => {
                    let full = update.base_revision.is_none();
                    let result = self.replica.apply_sandbox(update)?;
                    if full && matches!(result, ReplicaResult::Applied(_)) {
                        self.sandbox_keyframe_pending = false;
                    }
                    result
                }
                ServerMessage::Error { ref code, .. } if code == "resync_rate" => {
                    // A rejected request will not produce its promised pair of
                    // keyframes. Retry once the host's bounded cooldown elapses,
                    // including if no more deltas arrive in the meantime.
                    if self.resync_needed {
                        self.resync_pending = false;
                        self.resync_retry_at = Some(Instant::now() + Duration::from_secs(1));
                    }
                    messages.push(message);
                    continue;
                }
                other => {
                    messages.push(other);
                    continue;
                }
            };
            match result {
                ReplicaResult::Applied(message) => messages.push(message),
                ReplicaResult::NeedsKeyframe => self.send(ClientMessage::Resync {})?,
            }
            if self.resync_needed && !self.actor_keyframe_pending && !self.sandbox_keyframe_pending
            {
                self.resync_pending = false;
                self.resync_needed = false;
                self.resync_retry_at = None;
            }
        }
        if self.resync_needed
            && !self.resync_pending
            && self.resync_retry_at.is_none_or(|at| Instant::now() >= at)
        {
            self.send(ClientMessage::Resync {})?;
        }
        self.connection.flush()?;
        Ok(messages)
    }
}
