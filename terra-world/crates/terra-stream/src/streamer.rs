use crate::{StreamConfig, StreamError, providers};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};
use terra_core::{GeoPosition, TileKey, WorldChunk};

/// Provider extension point for local packs, COG importers, own CDN, or test data.
/// The loader executes on a bounded worker pool; never mutate Bevy state here.
pub trait ChunkProvider: Send + Sync + 'static {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError>;
}

pub struct ChunkPayload {
    pub chunk: WorldChunk,
    pub cache_hit: bool,
}

struct BuiltinProvider(StreamConfig);
impl ChunkProvider for BuiltinProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        let (chunk, cache_hit) = providers::load_chunk_cached(key, &self.0)?;
        Ok(ChunkPayload { chunk, cache_hit })
    }
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    Loaded(Arc<WorldChunk>),
    Evicted(TileKey),
    Failed { key: TileKey, error: String },
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StreamStats {
    pub resident: usize,
    pub pending: usize,
    pub in_flight: usize,
    pub completed: u64,
    pub failed: u64,
    pub cache_hits: u64,
    pub dropped_stale: u64,
    /// Requests beyond the resident budget are omitted in caller priority order.
    pub dropped_budget: u64,
    pub last_load_ms: f64,
}

struct Job {
    key: TileKey,
    serial: u64,
}
struct Completion {
    job: Job,
    result: Result<ChunkPayload, StreamError>,
    elapsed: Duration,
}

/// Bounded chunk residency and loading. `request_tiles` priority order is retained.
/// Focus changes cancel queued work immediately; already running requests finish
/// in background and are discarded if no longer wanted. HTTP has fixed deadlines.
/// `poll` never blocks on disk, network, worker joins, or channel sends.
pub struct ChunkStreamer {
    config: StreamConfig,
    resident: BTreeMap<TileKey, Arc<WorldChunk>>,
    desired: Vec<TileKey>,
    desired_set: BTreeSet<TileKey>,
    pending: VecDeque<TileKey>,
    in_flight: BTreeMap<TileKey, u64>,
    failures: BTreeMap<TileKey, (u32, Instant)>,
    evictions: VecDeque<TileKey>,
    tx: Option<SyncSender<Job>>,
    rx: Receiver<Completion>,
    next_serial: u64,
    counters: StreamStats,
}

impl ChunkStreamer {
    pub fn new(config: StreamConfig) -> Result<Self, StreamError> {
        let provider = Arc::new(BuiltinProvider(config.clone()));
        Self::with_provider(config, provider)
    }

    pub fn with_provider(
        config: StreamConfig,
        provider: Arc<dyn ChunkProvider>,
    ) -> Result<Self, StreamError> {
        config.validate()?;
        let (tx, jobs) = mpsc::sync_channel::<Job>(config.max_in_flight);
        let jobs = Arc::new(Mutex::new(jobs));
        let (completed, rx) = mpsc::sync_channel::<Completion>(config.max_in_flight);
        for worker_id in 0..config.max_in_flight {
            let jobs = jobs.clone();
            let completed = completed.clone();
            let provider = provider.clone();
            thread::Builder::new()
                .name(format!("terra-loader-{worker_id}"))
                .spawn(move || {
                    loop {
                        let job = jobs.lock().unwrap_or_else(|p| p.into_inner()).recv();
                        let Ok(job) = job else { break };
                        let began = Instant::now();
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let payload = provider.load(job.key)?;
                            if payload.chunk.key != job.key {
                                return Err(StreamError::Payload(
                                    "provider returned wrong tile key".into(),
                                ));
                            }
                            // Validation can touch many geometry vertices: keep
                            // it in this worker, not in the frame's poll call.
                            payload
                                .chunk
                                .validate()
                                .map_err(|error| StreamError::Payload(error.to_string()))?;
                            Ok(payload)
                        }))
                        .unwrap_or_else(|_| {
                            Err(StreamError::Payload(
                                "chunk provider panicked; request isolated".into(),
                            ))
                        });
                        if completed
                            .send(Completion {
                                job,
                                result,
                                elapsed: began.elapsed(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })?;
        }
        drop(completed);
        Ok(Self {
            config,
            resident: BTreeMap::new(),
            desired: vec![],
            desired_set: BTreeSet::new(),
            pending: VecDeque::new(),
            in_flight: BTreeMap::new(),
            failures: BTreeMap::new(),
            evictions: VecDeque::new(),
            tx: Some(tx),
            rx,
            next_serial: 1,
            counters: StreamStats::default(),
        })
    }

    /// Nearby tiles sorted by wrapped geographic tile distance from the focus.
    pub fn set_focus(
        &mut self,
        position: GeoPosition,
        level: u8,
        radius: u32,
    ) -> Result<(), StreamError> {
        if radius > 32 {
            return Err(StreamError::Configuration(
                "streaming radius must be ≤32".into(),
            ));
        }
        let focus =
            TileKey::at(position, level).map_err(|e| StreamError::Geography(e.to_string()))?;
        let mut keys = focus.neighbors(radius);
        let count = 1u32 << level;
        keys.sort_by_key(|key| {
            let dx = key.x.abs_diff(focus.x).min(count - key.x.abs_diff(focus.x)) as u64;
            let dy = key.y.abs_diff(focus.y) as u64;
            (dx * dx + dy * dy, key.y, key.x)
        });
        self.request_tiles(keys)
    }

    /// Request a union of player/camera areas, ordered most important first.
    /// An unchanged set is cheap and does not invalidate outstanding requests.
    pub fn request_tiles(&mut self, keys: Vec<TileKey>) -> Result<(), StreamError> {
        let mut unique = BTreeSet::new();
        let mut desired = Vec::with_capacity(keys.len().min(self.config.max_resident));
        let mut dropped = 0u64;
        for key in keys {
            key.validate()
                .map_err(|e| StreamError::Geography(e.to_string()))?;
            if !unique.insert(key) {
                continue;
            }
            if desired.len() < self.config.max_resident {
                desired.push(key);
            } else {
                dropped += 1;
            }
        }
        if self.desired == desired {
            return Ok(());
        }
        self.counters.dropped_budget += dropped;
        self.desired = desired;
        self.desired_set = self.desired.iter().copied().collect();
        self.pending.clear();
        self.failures
            .retain(|key, _| self.desired_set.contains(key));
        self.resident.retain(|key, _| {
            if self.desired_set.contains(key) {
                true
            } else {
                self.evictions.push_back(*key);
                false
            }
        });
        self.refill_pending();
        self.dispatch();
        Ok(())
    }

    fn refill_pending(&mut self) {
        let mut queued: BTreeSet<_> = self.pending.iter().copied().collect();
        for &key in &self.desired {
            if self.pending.len() >= self.config.max_queued {
                break;
            }
            if self.resident.contains_key(&key)
                || self.in_flight.contains_key(&key)
                || queued.contains(&key)
            {
                continue;
            }
            if self
                .failures
                .get(&key)
                .is_some_and(|(_, retry)| *retry > Instant::now())
            {
                continue;
            }
            queued.insert(key);
            self.pending.push_back(key);
        }
    }

    fn dispatch(&mut self) {
        while self.in_flight.len() < self.config.max_in_flight {
            let Some(key) = self.pending.pop_front() else {
                break;
            };
            let serial = self.next_serial;
            self.next_serial = self.next_serial.wrapping_add(1);
            let Some(tx) = &self.tx else { break };
            match tx.try_send(Job { key, serial }) {
                Ok(()) => {
                    self.in_flight.insert(key, serial);
                }
                Err(TrySendError::Full(_)) => {
                    self.pending.push_front(key);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.pending.push_front(key);
                    self.tx = None;
                    break;
                }
            }
        }
    }

    /// Integrate at most `max_completions` finished chunks, with a hard cap of 64.
    /// Evictions are reported first so client render resources can be released.
    /// Zero budget performs no completion integration but still dispatches work.
    pub fn poll(&mut self, max_completions: usize) -> Vec<StreamEvent> {
        let mut events = self
            .evictions
            .drain(..)
            .map(StreamEvent::Evicted)
            .collect::<Vec<_>>();
        for _ in 0..max_completions.min(64) {
            let completion = match self.rx.try_recv() {
                Ok(c) => c,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    for key in std::mem::take(&mut self.in_flight).into_keys() {
                        self.counters.failed += 1;
                        events.push(StreamEvent::Failed {
                            key,
                            error: "all stream workers disconnected".into(),
                        });
                    }
                    self.tx = None;
                    break;
                }
            };
            if self.in_flight.get(&completion.job.key) == Some(&completion.job.serial) {
                self.in_flight.remove(&completion.job.key);
            }
            if !self.desired_set.contains(&completion.job.key) {
                self.counters.dropped_stale += 1;
                continue;
            }
            self.counters.last_load_ms = completion.elapsed.as_secs_f64() * 1000.0;
            match completion.result {
                Ok(payload) => {
                    self.counters.completed += 1;
                    if payload.cache_hit {
                        self.counters.cache_hits += 1;
                    }
                    self.failures.remove(&completion.job.key);
                    let chunk = Arc::new(payload.chunk);
                    self.resident.insert(chunk.key, chunk.clone());
                    events.push(StreamEvent::Loaded(chunk));
                }
                Err(error) => {
                    self.counters.failed += 1;
                    let key = completion.job.key;
                    let attempts = self
                        .failures
                        .get(&key)
                        .map_or(1, |(count, _)| count.saturating_add(1));
                    // Bounded exponential retry; no request storm on outages.
                    let seconds = (1u64 << attempts.min(6)).min(60);
                    self.failures.insert(
                        key,
                        (attempts, Instant::now() + Duration::from_secs(seconds)),
                    );
                    events.push(StreamEvent::Failed {
                        key,
                        error: error.to_string(),
                    });
                }
            }
        }
        self.refill_pending();
        self.dispatch();
        events
    }

    pub fn resident(&self) -> &BTreeMap<TileKey, Arc<WorldChunk>> {
        &self.resident
    }
    pub fn get(&self, key: TileKey) -> Option<&Arc<WorldChunk>> {
        self.resident.get(&key)
    }
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            resident: self.resident.len(),
            pending: self.pending.len(),
            in_flight: self.in_flight.len(),
            ..self.counters
        }
    }
}

// Drop only closes channels. Detached workers finish their bounded request and
// exit; joining here would freeze a game frame for the HTTP timeout duration.
impl Drop for ChunkStreamer {
    fn drop(&mut self) {
        self.tx.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedural_chunk;

    fn key(x: u32) -> TileKey {
        TileKey {
            level: 16,
            x,
            y: 31_000,
        }
    }
    fn config() -> StreamConfig {
        StreamConfig {
            max_resident: 3,
            max_in_flight: 1,
            max_queued: 2,
            terrain_resolution: 5,
            ..Default::default()
        }
    }
    fn finish(stream: &mut ChunkStreamer, count: usize) -> Vec<StreamEvent> {
        let began = Instant::now();
        let mut events = vec![];
        while stream.stats().resident < count && began.elapsed() < Duration::from_secs(3) {
            events.extend(stream.poll(1));
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(stream.stats().resident, count);
        events
    }

    #[test]
    fn repeated_focus_is_deduplicated_and_all_budgets_hold() {
        let mut stream = ChunkStreamer::new(config()).unwrap();
        for _ in 0..8 {
            stream
                .request_tiles(vec![key(1), key(2), key(3), key(4), key(1)])
                .unwrap();
        }
        assert!(stream.stats().in_flight <= 1 && stream.stats().pending <= 2);
        finish(&mut stream, 3);
        assert_eq!(stream.stats().completed, 3);
        assert!(stream.get(key(4)).is_none());
        for _ in 0..10 {
            assert!(stream.poll(1).is_empty());
        }
    }

    struct GateProvider {
        gate: Arc<Mutex<Receiver<()>>>,
    }
    impl ChunkProvider for GateProvider {
        fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
            self.gate.lock().unwrap().recv().unwrap();
            Ok(ChunkPayload {
                chunk: procedural_chunk(key, 5, 42),
                cache_hit: false,
            })
        }
    }

    #[test]
    fn teleport_rejects_old_completion_and_evicts_residents() {
        let (tx, rx) = mpsc::channel();
        let provider = Arc::new(GateProvider {
            gate: Arc::new(Mutex::new(rx)),
        });
        let mut stream = ChunkStreamer::with_provider(config(), provider).unwrap();
        stream.request_tiles(vec![key(1)]).unwrap();
        stream.request_tiles(vec![key(2)]).unwrap();
        tx.send(()).unwrap();
        tx.send(()).unwrap();
        finish(&mut stream, 1);
        assert!(stream.get(key(1)).is_none());
        assert!(stream.get(key(2)).is_some());
        assert_eq!(stream.stats().dropped_stale, 1);
        stream.request_tiles(vec![]).unwrap();
        assert!(matches!(stream.poll(1).as_slice(),[StreamEvent::Evicted(k)] if *k==key(2)));
    }

    #[test]
    fn zero_integration_budget_does_not_load_completed_result() {
        let mut stream = ChunkStreamer::new(config()).unwrap();
        stream.request_tiles(vec![key(1)]).unwrap();
        for _ in 0..5 {
            stream.poll(0);
            thread::yield_now();
        }
        assert_eq!(stream.stats().resident, 0);
        finish(&mut stream, 1);
    }

    struct WrongKeyProvider;
    impl ChunkProvider for WrongKeyProvider {
        fn load(&self, _key: TileKey) -> Result<ChunkPayload, StreamError> {
            Ok(ChunkPayload {
                chunk: procedural_chunk(key(2), 5, 42),
                cache_hit: false,
            })
        }
    }

    #[test]
    fn invalid_provider_result_is_rejected_with_backoff() {
        let mut stream =
            ChunkStreamer::with_provider(config(), Arc::new(WrongKeyProvider)).unwrap();
        stream.request_tiles(vec![key(1)]).unwrap();
        let began = Instant::now();
        let mut failed = false;
        while !failed && began.elapsed() < Duration::from_secs(1) {
            failed = stream
                .poll(1)
                .iter()
                .any(|event| matches!(event, StreamEvent::Failed { .. }));
            thread::sleep(Duration::from_millis(2));
        }
        assert!(failed);
        assert_eq!(stream.stats().resident, 0);
        for _ in 0..10 {
            stream.poll(1);
        }
        assert_eq!(stream.stats().failed, 1);
        assert_eq!(stream.stats().in_flight, 0);
    }
}
