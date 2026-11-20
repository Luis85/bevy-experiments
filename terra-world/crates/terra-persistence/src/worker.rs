//! Bounded, serial background storage. Hosts retain their visible working set
//! and suspend authoritative mutation until a matching completion is installed.
use crate::{
    RegionCheckpointMetrics, RegionKey, RegionStore, RegionStoreStatus, Result, StoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};
use terra_sandbox::SandboxState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionWorkKind {
    Hydrate,
    Checkpoint,
}

struct Request {
    id: u64,
    state: SandboxState,
    operation: Operation,
}

enum Operation {
    Hydrate,
    Checkpoint {
        metadata: Value,
        requested: BTreeSet<RegionKey>,
    },
}

/// A result belongs to exactly one submitted revision. State is returned only
/// on success. A failed checkpoint cannot replace the host's retained state.
pub struct RegionCompletion {
    pub id: u64,
    pub kind: RegionWorkKind,
    pub state: Option<SandboxState>,
    pub status: RegionStoreStatus,
    pub checkpoint_metrics: RegionCheckpointMetrics,
    pub elapsed_ms: f64,
    pub error: Option<String>,
}

/// Exactly one operation may be outstanding per authority. Both queues are
/// bounded at one; repeated submissions return Busy rather than buffering world
/// copies. `try_complete` never waits for disk or validation. Snapshot copying
/// remains the caller's bounded cost and should be measured separately.
pub struct RegionWorker {
    store: Arc<Mutex<RegionStore>>,
    path: PathBuf,
    sender: Option<mpsc::SyncSender<Option<Request>>>,
    receiver: Mutex<mpsc::Receiver<RegionCompletion>>,
    thread: Option<JoinHandle<()>>,
    pending: Option<(u64, RegionWorkKind)>,
    // A lost result cannot tell the caller whether SQLite committed. Keep the
    // last acknowledged status and require a newly opened, validated store.
    failure: Option<String>,
    next_id: u64,
    status: RegionStoreStatus,
}

impl RegionWorker {
    pub fn new(store: RegionStore) -> Result<Self> {
        let path = store.path.clone();
        let status = store.status();
        let store = Arc::new(Mutex::new(store));
        let task_store = Arc::clone(&store);
        let (sender, requests) = mpsc::sync_channel::<Option<Request>>(1);
        let (completions, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("terra-regional-storage".into())
            .spawn(move || {
                while let Ok(Some(mut request)) = requests.recv() {
                    let start = Instant::now();
                    let kind = match request.operation {
                        Operation::Hydrate => RegionWorkKind::Hydrate,
                        Operation::Checkpoint { .. } => RegionWorkKind::Checkpoint,
                    };
                    let result = match task_store.lock() {
                        Ok(mut store) => {
                            let result = match request.operation {
                                Operation::Hydrate => store.hydrate(&mut request.state),
                                Operation::Checkpoint {
                                    metadata,
                                    requested,
                                } => store
                                    .checkpoint(&mut request.state, metadata, &requested)
                                    .map(|_| ()),
                            };
                            (
                                store.status(),
                                result.err().map(|e| e.to_string()),
                                store.checkpoint_metrics(),
                            )
                        }
                        Err(_) => (
                            RegionStoreStatus::default(),
                            Some("regional store lock poisoned".into()),
                            RegionCheckpointMetrics::default(),
                        ),
                    };
                    let completion = RegionCompletion {
                        id: request.id,
                        kind,
                        state: result.1.is_none().then_some(request.state),
                        status: result.0,
                        checkpoint_metrics: result.2,
                        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
                        error: result.1,
                    };
                    if completions.send(completion).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            store,
            path,
            sender: Some(sender),
            receiver: Mutex::new(receiver),
            thread: Some(thread),
            pending: None,
            failure: None,
            next_id: 1,
            status,
        })
    }

    /// True only while a result is outstanding. A terminal channel failure
    /// clears this flag but still fails every subsequent operation until reopen.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Storage is poisoned or the outcome of submitted work can no longer be
    /// acknowledged safely. Idle read-only callers observe poison as well.
    /// Reopen and validate the committed store before creating a replacement
    /// worker; neither this flag nor a cleared pending flag implies rollback.
    pub fn requires_reopen(&self) -> bool {
        self.failure.is_some() || self.store.is_poisoned()
    }
    pub fn pending_kind(&self) -> Option<RegionWorkKind> {
        self.pending.map(|(_, kind)| kind)
    }
    pub fn status(&self) -> RegionStoreStatus {
        self.status.clone()
    }
    pub fn check_output_path(&self, path: impl AsRef<Path>) -> Result<()> {
        crate::validate_output_path(&self.path, path.as_ref())
    }

    pub fn submit_hydrate(&mut self, state: SandboxState) -> Result<u64> {
        self.submit(state, Operation::Hydrate)
    }
    pub fn submit_checkpoint(
        &mut self,
        state: SandboxState,
        metadata: Value,
        requested: BTreeSet<RegionKey>,
    ) -> Result<u64> {
        self.submit(
            state,
            Operation::Checkpoint {
                metadata,
                requested,
            },
        )
    }
    fn submit(&mut self, state: SandboxState, operation: Operation) -> Result<u64> {
        self.require_available()?;
        if self.pending() {
            return Err(StoreError::Busy);
        }
        let id = self.next_id;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| StoreError::Worker("request id exhausted".into()))?;
        let kind = match operation {
            Operation::Hydrate => RegionWorkKind::Hydrate,
            Operation::Checkpoint { .. } => RegionWorkKind::Checkpoint,
        };
        let Some(sender) = self.sender.as_ref() else {
            return Err(self.fail("request channel closed"));
        };
        match sender.try_send(Some(Request {
            id,
            state,
            operation,
        })) {
            Ok(()) => (),
            Err(mpsc::TrySendError::Full(_)) => return Err(StoreError::Busy),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(self.fail("request channel disconnected"));
            }
        }
        self.pending = Some((id, kind));
        self.next_id = next_id;
        Ok(id)
    }

    /// Poll once. Hosts must reject a completion if their submitted authority
    /// revision changed. Never allow mutation between submission and install.
    pub fn try_complete(&mut self) -> Result<Option<RegionCompletion>> {
        self.require_available()?;
        if !self.pending() {
            return Ok(None);
        }
        let received = self
            .receiver
            .lock()
            .map(|receiver| receiver.try_recv())
            .map_err(|_| ());
        match received {
            Ok(Ok(result)) => self.accept(result).map(Some),
            Ok(Err(mpsc::TryRecvError::Empty)) => Ok(None),
            Ok(Err(mpsc::TryRecvError::Disconnected)) => {
                Err(self.fail("result channel disconnected"))
            }
            Err(()) => Err(self.fail("result lock poisoned")),
        }
    }

    /// Explicit startup/shutdown/maintenance barrier. Ordinary updates use
    /// `try_complete`, never this method. This waits for already submitted work.
    pub fn finish(&mut self) -> Result<Option<RegionCompletion>> {
        self.require_available()?;
        if !self.pending() {
            return Ok(None);
        }
        let result = self
            .receiver
            .lock()
            .map(|receiver| receiver.recv())
            .map_err(|_| ());
        match result {
            Ok(Ok(result)) => self.accept(result).map(Some),
            Ok(Err(_)) => Err(self.fail("result channel disconnected")),
            Err(()) => Err(self.fail("result lock poisoned")),
        }
    }
    fn accept(&mut self, result: RegionCompletion) -> Result<RegionCompletion> {
        if self.store.is_poisoned() {
            return Err(self.fail("regional store lock poisoned"));
        }
        if self.pending != Some((result.id, result.kind)) {
            return Err(self.fail("completion does not match pending request"));
        }
        self.pending = None;
        self.status = result.status.clone();
        Ok(result)
    }

    fn require_available(&self) -> Result<()> {
        match &self.failure {
            Some(error) => Err(StoreError::Worker(error.clone())),
            None if self.store.is_poisoned() && !self.pending() => Err(StoreError::Worker(
                "regional store lock poisoned; Stop and reopen the world to verify its committed checkpoint before retrying".into(),
            )),
            None => Ok(()),
        }
    }

    fn fail(&mut self, cause: &str) -> StoreError {
        self.pending = None;
        self.sender.take();
        let error = self.failure.get_or_insert_with(|| {
            format!("{cause}; the last operation may have committed. Stop and reopen the world to verify its committed checkpoint before retrying")
        });
        StoreError::Worker(error.clone())
    }

    /// Synchronous explicit maintenance/startup access while idle. Busy returns
    /// immediately; it never acquires a lock held by ongoing background work.
    pub fn with_store<T>(
        &mut self,
        operation: impl FnOnce(&mut RegionStore) -> Result<T>,
    ) -> Result<T> {
        self.require_available()?;
        if self.pending() {
            return Err(StoreError::Busy);
        }
        let mut store = self
            .store
            .lock()
            .map_err(|_| StoreError::Worker("store lock poisoned".into()))?;
        let result = operation(&mut store);
        self.status = store.status();
        result
    }
    pub fn export(&self, active: &SandboxState) -> Result<SandboxState> {
        self.require_available()?;
        if self.pending() {
            return Err(StoreError::Busy);
        }
        self.store
            .lock()
            .map_err(|_| StoreError::Worker("store lock poisoned".into()))?
            .export(active)
    }
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<()> {
        self.require_available()?;
        if self.pending() {
            return Err(StoreError::Busy);
        }
        self.store
            .lock()
            .map_err(|_| StoreError::Worker("store lock poisoned".into()))?
            .backup_to(path)
    }
}

impl Drop for RegionWorker {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(None);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
