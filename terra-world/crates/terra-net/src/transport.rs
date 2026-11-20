use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpStream,
};

use serde::{Serialize, de::DeserializeOwned};

use crate::{NetError, Result};

/// Four-byte big-endian length followed by this many JSON bytes at most.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
const READ_CHUNK: usize = 8 * 1024;
const IO_BUDGET: usize = 64 * 1024;
const MAX_PENDING_BYTES: usize = 2 * MAX_FRAME_BYTES;
// Optional world views leave room for reliable command results and control
// traffic. A deferred view is rebuilt from current state after the queue drains;
// an already queued frame is never removed from its revision chain.
const RELIABLE_RESERVE_BYTES: usize = 64 * 1024;

pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(value)?;
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(NetError::Protocol(
            "frame length outside allowed range".into(),
        ));
    }
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Incremental decoder: handles both TCP fragmentation and coalescing.
#[derive(Default, Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<()> {
        if self.buffer.len().saturating_add(bytes.len()) > MAX_FRAME_BYTES + 4 + READ_CHUNK {
            return Err(NetError::Protocol("receive buffer budget exceeded".into()));
        }
        self.buffer.extend_from_slice(bytes);
        self.body_length()?;
        Ok(())
    }

    fn body_length(&self) -> Result<Option<usize>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            return Err(NetError::Protocol(format!("invalid frame length {len}")));
        }
        Ok(Some(len))
    }

    pub fn decode<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        let Some(len) = self.body_length()? else {
            return Ok(None);
        };
        if self.buffer.len() < len + 4 {
            return Ok(None);
        }
        let value = serde_json::from_slice(&self.buffer[4..4 + len])?;
        self.buffer.drain(..4 + len);
        Ok(Some(value))
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }
}

/// Actual socket/queue counters; independent of replication compression estimates.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct TransportStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub frames_queued: u64,
    /// Current application bytes, including plaintext retained inside TLS until
    /// its encrypted records drain. OS socket buffers are not included.
    pub pending_bytes: usize,
    pub peak_pending_bytes: usize,
}

pub(crate) struct Connection {
    stream: TcpStream,
    #[cfg(feature = "tls")]
    tls: Option<rustls::Connection>,
    #[cfg(feature = "tls")]
    tls_pending_plaintext: usize,
    decoder: FrameDecoder,
    writes: VecDeque<Vec<u8>>,
    write_offset: usize,
    pending_bytes: usize,
    stats: TransportStats,
}

impl Connection {
    pub(crate) fn new(stream: TcpStream) -> Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            #[cfg(feature = "tls")]
            tls: None,
            #[cfg(feature = "tls")]
            tls_pending_plaintext: 0,
            decoder: FrameDecoder::default(),
            writes: VecDeque::new(),
            write_offset: 0,
            pending_bytes: 0,
            stats: TransportStats::default(),
        })
    }

    #[cfg(feature = "tls")]
    pub(crate) fn new_tls_server(
        stream: TcpStream,
        config: &crate::ServerTlsConfig,
    ) -> Result<Self> {
        let tls = rustls::ServerConnection::new(config.config.clone())
            .map_err(|error| NetError::Configuration(format!("TLS: {error}")))?;
        let mut connection = Self::new(stream)?;
        let mut tls = rustls::Connection::Server(tls);
        tls.set_buffer_limit(Some(MAX_PENDING_BYTES));
        connection.tls = Some(tls);
        Ok(connection)
    }

    #[cfg(feature = "tls")]
    pub(crate) fn new_tls_client(
        stream: TcpStream,
        config: &crate::ClientTlsConfig,
    ) -> Result<Self> {
        let tls = rustls::ClientConnection::new(config.config.clone(), config.server_name.clone())
            .map_err(|error| NetError::Configuration(format!("TLS: {error}")))?;
        let mut connection = Self::new(stream)?;
        let mut tls = rustls::Connection::Client(tls);
        tls.set_buffer_limit(Some(MAX_PENDING_BYTES));
        connection.tls = Some(tls);
        Ok(connection)
    }

    pub(crate) fn queue<T: Serialize>(&mut self, message: &T) -> Result<()> {
        let frame = encode_frame(message)?;
        if self.pending_bytes.saturating_add(frame.len()) > MAX_PENDING_BYTES {
            return Err(NetError::Protocol("outbound queue budget exceeded".into()));
        }
        self.pending_bytes += frame.len();
        self.stats.peak_pending_bytes = self.stats.peak_pending_bytes.max(self.pending_bytes);
        self.stats.frames_queued = self.stats.frames_queued.saturating_add(1);
        self.writes.push_back(frame);
        Ok(())
    }

    pub(crate) fn stats(&self) -> TransportStats {
        TransportStats {
            pending_bytes: self.pending_bytes,
            ..self.stats
        }
    }

    /// A maximum-size frame can be admitted without a queue-budget failure.
    /// Check before consuming a reliable request or advancing a view baseline.
    pub(crate) fn has_frame_headroom(&self) -> bool {
        self.pending_bytes <= MAX_PENDING_BYTES - (MAX_FRAME_BYTES + 4)
    }

    pub(crate) fn has_snapshot_headroom(&self) -> bool {
        self.pending_bytes <= MAX_PENDING_BYTES - (MAX_FRAME_BYTES + 4) - RELIABLE_RESERVE_BYTES
    }

    pub(crate) fn is_encrypted(&self) -> bool {
        #[cfg(feature = "tls")]
        {
            self.tls
                .as_ref()
                .is_some_and(|tls| !tls.is_handshaking() && validate_tls_protocol(tls).is_ok())
        }
        #[cfg(not(feature = "tls"))]
        {
            false
        }
    }

    pub(crate) fn flush(&mut self) -> Result<()> {
        #[cfg(feature = "tls")]
        if self.tls.is_some() {
            return self.flush_tls();
        }
        let mut remaining = IO_BUDGET;
        while let Some(front) = self.writes.front() {
            if remaining == 0 {
                break;
            }
            let end = front.len().min(self.write_offset + remaining);
            match self.stream.write(&front[self.write_offset..end]) {
                Ok(0) => return Err(NetError::Disconnected),
                Ok(written) => {
                    self.stats.bytes_sent = self.stats.bytes_sent.saturating_add(written as u64);
                    self.write_offset += written;
                    self.pending_bytes -= written;
                    remaining -= written;
                    if self.write_offset == front.len() {
                        self.writes.pop_front();
                        self.write_offset = 0;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(crate) fn receive<T: DeserializeOwned>(&mut self, max_messages: usize) -> Result<Vec<T>> {
        #[cfg(feature = "tls")]
        if self.tls.is_some() {
            return self.receive_tls(max_messages);
        }
        let mut messages = Vec::new();
        let mut scratch = [0_u8; READ_CHUNK];
        let mut remaining = IO_BUDGET;
        loop {
            while messages.len() < max_messages {
                match self.decoder.decode()? {
                    Some(message) => messages.push(message),
                    None => break,
                }
            }
            if messages.len() >= max_messages || remaining == 0 {
                break;
            }
            let request = scratch.len().min(remaining);
            match self.stream.read(&mut scratch[..request]) {
                Ok(0) if messages.is_empty() => return Err(NetError::Disconnected),
                Ok(0) => break,
                Ok(count) => {
                    self.stats.bytes_received =
                        self.stats.bytes_received.saturating_add(count as u64);
                    remaining -= count;
                    self.decoder.push(&scratch[..count])?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(messages)
    }

    #[cfg(feature = "tls")]
    fn flush_tls(&mut self) -> Result<()> {
        let tls = self.tls.as_mut().unwrap();
        let mut wire = BudgetedSocket {
            stream: &mut self.stream,
            remaining: IO_BUDGET,
            used: 0,
        };
        // Protocol negotiation and certificate validation complete before any
        // queued Hello (including resume capability) is passed to TLS plaintext.
        if !tls.is_handshaking() {
            validate_tls_protocol(tls)?;
            let mut plaintext = IO_BUDGET;
            while let Some(front) = self.writes.front() {
                if plaintext == 0 {
                    break;
                }
                let end = front.len().min(self.write_offset + plaintext);
                match tls.writer().write(&front[self.write_offset..end]) {
                    Ok(0) => break,
                    Ok(written) => {
                        self.write_offset += written;
                        // Keep bytes charged to the same queue budget until
                        // rustls has drained their encrypted records. Otherwise
                        // its internal buffer could double the effective cap.
                        self.tls_pending_plaintext += written;
                        plaintext -= written;
                        if self.write_offset == front.len() {
                            self.writes.pop_front();
                            self.write_offset = 0;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        while tls.wants_write() && wire.remaining > 0 {
            match tls.write_tls(&mut wire) {
                Ok(0) => return Err(NetError::Disconnected),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        self.stats.bytes_sent = self.stats.bytes_sent.saturating_add(wire.used as u64);
        if !tls.wants_write() {
            self.pending_bytes = self
                .pending_bytes
                .saturating_sub(self.tls_pending_plaintext);
            self.tls_pending_plaintext = 0;
        }
        Ok(())
    }

    #[cfg(feature = "tls")]
    fn receive_tls<T: DeserializeOwned>(&mut self, max_messages: usize) -> Result<Vec<T>> {
        let tls = self.tls.as_mut().unwrap();
        let mut wire = BudgetedSocket {
            stream: &mut self.stream,
            remaining: IO_BUDGET,
            used: 0,
        };
        let mut closed = false;
        while wire.remaining > 0 && tls.wants_read() {
            match tls.read_tls(&mut wire) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(_) => {
                    tls.process_new_packets().map_err(|error| {
                        NetError::Protocol(format!("TLS verification/record failure: {error}"))
                    })?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        self.stats.bytes_received = self.stats.bytes_received.saturating_add(wire.used as u64);
        if !tls.is_handshaking() {
            validate_tls_protocol(tls)?;
        }
        let mut messages = Vec::new();
        let mut scratch = [0_u8; READ_CHUNK];
        let mut remaining = IO_BUDGET;
        loop {
            while messages.len() < max_messages {
                match self.decoder.decode()? {
                    Some(message) => messages.push(message),
                    None => break,
                }
            }
            if messages.len() >= max_messages || remaining == 0 {
                break;
            }
            let request = scratch.len().min(remaining);
            match tls.reader().read(&mut scratch[..request]) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(count) => {
                    remaining -= count;
                    self.decoder.push(&scratch[..count])?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::UnexpectedEof
                        && !messages.is_empty() =>
                {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        if closed && messages.is_empty() {
            return Err(NetError::Disconnected);
        }
        Ok(messages)
    }
}

#[cfg(feature = "tls")]
fn validate_tls_protocol(tls: &rustls::Connection) -> Result<()> {
    if tls.alpn_protocol() != Some(crate::tls::alpn().as_slice()) {
        return Err(NetError::Protocol(
            "TLS server did not negotiate this Terra protocol".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "tls")]
struct BudgetedSocket<'a> {
    stream: &'a mut TcpStream,
    remaining: usize,
    used: usize,
}
#[cfg(feature = "tls")]
impl Read for BudgetedSocket<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(self.remaining);
        let read = self.stream.read(&mut bytes[..count])?;
        self.remaining -= read;
        self.used += read;
        Ok(read)
    }
}
#[cfg(feature = "tls")]
impl Write for BudgetedSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(self.remaining);
        let written = self.stream.write(&bytes[..count])?;
        self.remaining -= written;
        self.used += written;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientMessage;

    #[test]
    fn fragmented_header_and_body_decode_once_complete() {
        let frame = encode_frame(&ClientMessage::Ping { nonce: 9 }).unwrap();
        let mut decoder = FrameDecoder::default();
        for byte in &frame[..frame.len() - 1] {
            decoder.push(&[*byte]).unwrap();
            assert!(decoder.decode::<ClientMessage>().unwrap().is_none());
        }
        decoder.push(&frame[frame.len() - 1..]).unwrap();
        assert!(matches!(
            decoder.decode().unwrap(),
            Some(ClientMessage::Ping { nonce: 9 })
        ));
        assert_eq!(decoder.buffered_bytes(), 0);
    }

    #[test]
    fn coalesced_frames_are_separate_messages() {
        let mut bytes = encode_frame(&ClientMessage::Ping { nonce: 1 }).unwrap();
        bytes.extend(encode_frame(&ClientMessage::Ping { nonce: 2 }).unwrap());
        let mut decoder = FrameDecoder::default();
        decoder.push(&bytes).unwrap();
        assert!(matches!(
            decoder.decode().unwrap(),
            Some(ClientMessage::Ping { nonce: 1 })
        ));
        assert!(matches!(
            decoder.decode().unwrap(),
            Some(ClientMessage::Ping { nonce: 2 })
        ));
        assert!(decoder.decode::<ClientMessage>().unwrap().is_none());
    }

    #[test]
    fn malicious_length_is_rejected_before_body_allocation() {
        for len in [0_u32, MAX_FRAME_BYTES as u32 + 1, u32::MAX] {
            let mut decoder = FrameDecoder::default();
            assert!(decoder.push(&len.to_be_bytes()).is_err());
            assert!(decoder.buffered_bytes() <= 4);
        }
    }

    #[test]
    fn receive_buffer_and_serialize_budgets_are_enforced() {
        let mut decoder = FrameDecoder::default();
        assert!(decoder.push(&vec![1; MAX_FRAME_BYTES * 2]).is_err());
        assert!(encode_frame(&"a".repeat(MAX_FRAME_BYTES)).is_err());
    }
}
