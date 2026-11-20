//! A bounded opaque TCP relay for reproducible transport acceptance fixtures.
//! It delays and fragments byte delivery without rewriting application/TLS
//! records. A paused downstream stops forwarding and eventually backpressures
//! the real host socket. Write calls are not claims about Internet packet shape.
use serde::Serialize;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_BUFFERED: usize = 128 * 1024;
const READ_BYTES: usize = 8 * 1024;
const WRITE_BUDGET: usize = 32 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub latency_ms: u64,
    pub jitter_ms: u64,
    pub fragment_bytes: usize,
}
impl Default for Conditions {
    fn default() -> Self {
        Self {
            latency_ms: 25,
            jitter_ms: 15,
            fragment_bytes: 997,
        }
    }
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct LinkStats {
    pub upstream_bytes: u64,
    pub downstream_bytes: u64,
    pub upstream_write_fragments: u64,
    pub downstream_write_fragments: u64,
    pub peak_upstream_buffer_bytes: usize,
    pub peak_downstream_buffer_bytes: usize,
    pub downstream_paused_polls: u64,
    pub error: Option<String>,
}

pub struct ConditionedLink {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    stats: Arc<Mutex<LinkStats>>,
    thread: Option<JoinHandle<()>>,
}
impl ConditionedLink {
    pub fn start(upstream: SocketAddr, conditions: Conditions) -> io::Result<Self> {
        if conditions.latency_ms > 1000
            || conditions.jitter_ms > 1000
            || !(1..=8192).contains(&conditions.fragment_bytes)
        {
            return Err(io::Error::other("condition profile exceeds fixture bounds"));
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(LinkStats::default()));
        let thread_stop = stop.clone();
        let thread_pause = pause.clone();
        let thread_stats = stats.clone();
        let thread = thread::spawn(move || {
            if let Err(error) = relay(
                listener,
                upstream,
                conditions,
                &thread_stop,
                &thread_pause,
                &thread_stats,
            ) {
                if !thread_stop.load(Ordering::Relaxed) {
                    thread_stats.lock().unwrap().error = Some(error.to_string());
                }
            }
        });
        Ok(Self {
            address,
            stop,
            pause,
            stats,
            thread: Some(thread),
        })
    }

    pub fn pause_downstream(&self, paused: bool) {
        self.pause.store(paused, Ordering::Relaxed);
    }

    pub fn stats(&self) -> LinkStats {
        self.stats.lock().unwrap().clone()
    }
}
impl Drop for ConditionedLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Fragment {
    bytes: Vec<u8>,
    offset: usize,
    ready: Instant,
}
#[derive(Default)]
struct Direction {
    queue: VecDeque<Fragment>,
    buffered: usize,
    peak: usize,
    read_batches: u64,
    writes: u64,
    written: u64,
}
impl Direction {
    fn receive(&mut self, stream: &mut TcpStream, conditions: Conditions) -> io::Result<bool> {
        let mut bytes = [0; READ_BYTES];
        // One read has bounded work and never allocates beyond the fixed cap.
        let capacity = READ_BYTES.min(MAX_BUFFERED.saturating_sub(self.buffered));
        if capacity == 0 {
            return Ok(true);
        }
        match stream.read(&mut bytes[..capacity]) {
            Ok(0) => Ok(false),
            Ok(count) => {
                let jitter = if conditions.jitter_ms == 0 {
                    0
                } else {
                    self.read_batches.wrapping_mul(17) % (conditions.jitter_ms + 1)
                };
                self.read_batches += 1;
                self.queue.push_back(Fragment {
                    bytes: bytes[..count].to_vec(),
                    offset: 0,
                    ready: Instant::now() + Duration::from_millis(conditions.latency_ms + jitter),
                });
                self.buffered += count;
                self.peak = self.peak.max(self.buffered);
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    fn forward(&mut self, stream: &mut TcpStream, conditions: Conditions) -> io::Result<()> {
        let mut budget = WRITE_BUDGET;
        while budget > 0 {
            let Some(front) = self.queue.front_mut() else {
                break;
            };
            if Instant::now() < front.ready {
                break;
            }
            let variation = 1 + self.writes.wrapping_mul(73) as usize % conditions.fragment_bytes;
            let count = (front.bytes.len() - front.offset)
                .min(variation)
                .min(budget);
            match stream.write(&front.bytes[front.offset..front.offset + count]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(written) => {
                    front.offset += written;
                    self.buffered -= written;
                    self.written += written as u64;
                    self.writes += 1;
                    budget -= written;
                    if front.offset == front.bytes.len() {
                        self.queue.pop_front();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn relay(
    listener: TcpListener,
    upstream: SocketAddr,
    conditions: Conditions,
    stop: &AtomicBool,
    pause: &AtomicBool,
    stats: &Mutex<LinkStats>,
) -> io::Result<()> {
    let mut downstream = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(error) => return Err(error),
        }
    };
    let mut upstream = TcpStream::connect_timeout(&upstream, Duration::from_secs(3))?;
    for stream in [&downstream, &upstream] {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
    }
    let mut up = Direction::default();
    let mut down = Direction::default();
    while !stop.load(Ordering::Relaxed) {
        if !up.receive(&mut downstream, conditions)? || !down.receive(&mut upstream, conditions)? {
            return Ok(());
        }
        up.forward(&mut upstream, conditions)?;
        let paused = pause.load(Ordering::Relaxed);
        if !paused {
            down.forward(&mut downstream, conditions)?;
        }
        {
            let mut stats = stats.lock().unwrap();
            stats.upstream_bytes = up.written;
            stats.downstream_bytes = down.written;
            stats.upstream_write_fragments = up.writes;
            stats.downstream_write_fragments = down.writes;
            stats.peak_upstream_buffer_bytes = up.peak;
            stats.peak_downstream_buffer_bytes = down.peak;
            stats.downstream_paused_polls += u64::from(paused);
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
