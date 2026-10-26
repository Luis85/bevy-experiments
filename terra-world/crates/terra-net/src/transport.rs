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

pub(crate) struct Connection {
    stream: TcpStream,
    decoder: FrameDecoder,
    writes: VecDeque<Vec<u8>>,
    write_offset: usize,
    pending_bytes: usize,
}

impl Connection {
    pub(crate) fn new(stream: TcpStream) -> Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            decoder: FrameDecoder::default(),
            writes: VecDeque::new(),
            write_offset: 0,
            pending_bytes: 0,
        })
    }

    pub(crate) fn queue<T: Serialize>(&mut self, message: &T) -> Result<()> {
        let frame = encode_frame(message)?;
        if self.pending_bytes.saturating_add(frame.len()) > MAX_PENDING_BYTES {
            return Err(NetError::Protocol("outbound queue budget exceeded".into()));
        }
        self.pending_bytes += frame.len();
        self.writes.push_back(frame);
        Ok(())
    }

    pub(crate) fn flush(&mut self) -> Result<()> {
        let mut remaining = IO_BUDGET;
        while let Some(front) = self.writes.front() {
            if remaining == 0 {
                break;
            }
            let end = front.len().min(self.write_offset + remaining);
            match self.stream.write(&front[self.write_offset..end]) {
                Ok(0) => return Err(NetError::Disconnected),
                Ok(written) => {
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
