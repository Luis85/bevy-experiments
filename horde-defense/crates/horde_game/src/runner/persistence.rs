//! Single-writer storage service. Only immutable engine/settings snapshots cross
//! the worker boundary; commands, epochs and navigation remain on the main thread.
use super::{apply, command::SaveIntent};
use crate::settings::Settings;
use horde_core::sim::Game;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Mutex, mpsc},
    thread::JoinHandle,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct PersistenceMetrics {
    pub completed_background_saves: u64,
    pub failed_background_saves: u64,
    pub max_background_write_ms: f64,
    pub pending: bool,
}
#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub game: Game,
    pub settings: Settings,
    pub campaign_path: PathBuf,
    pub settings_path: PathBuf,
    /// Copy the settings file aside before its first replacement
    /// (`profile::preserve_settings_original`).
    pub preserve_settings: bool,
}
#[derive(Debug)]
enum Payload {
    Save(Box<Snapshot>),
    Reset {
        old: Box<Game>,
        fresh: Box<Game>,
        path: PathBuf,
    },
}
#[derive(Clone, Debug)]
struct Identity {
    ticket: u64,
    epoch: u64,
    intents: Vec<SaveIntent>,
    reset: bool,
}
#[derive(Debug)]
struct Job {
    identity: Identity,
    payload: Payload,
}
#[derive(Debug)]
pub(super) enum Commit {
    Saved,
    Reset {
        game: Box<Game>,
        archive: Option<PathBuf>,
    },
}
#[derive(Debug)]
pub(super) struct Completion {
    pub epoch: u64,
    pub intents: Vec<SaveIntent>,
    pub reset: bool,
    pub result: Result<Commit, String>,
    pub recovered: Vec<PathBuf>,
    ticket: u64,
    write_ms: f64,
}
impl Completion {
    fn failed(identity: Identity, error: String) -> Self {
        Self {
            epoch: identity.epoch,
            intents: identity.intents,
            reset: identity.reset,
            result: Err(error),
            recovered: Vec::new(),
            ticket: identity.ticket,
            write_ms: 0.0,
        }
    }
}
trait Backend: Send + 'static {
    fn execute(&mut self, job: Job) -> Completion;
}
#[derive(Default)]
struct FileStore {
    preserved: Vec<(PathBuf, u64)>,
    /// Settings files already copied aside this session; later saves replace
    /// only this session's own writes and must not copy them over the original.
    settings_preserved: Vec<PathBuf>,
}
impl Backend for FileStore {
    fn execute(&mut self, job: Job) -> Completion {
        let start = Instant::now();
        let mut recovered = Vec::new();
        let result = (|| match job.payload {
            Payload::Save(snapshot) => {
                recovered =
                    apply::preserve_unreadable(&snapshot.campaign_path, &mut self.preserved)?;
                if snapshot.preserve_settings
                    && !self.settings_preserved.contains(&snapshot.settings_path)
                {
                    crate::profile::preserve_settings_original(&snapshot.settings_path)?;
                    self.settings_preserved.push(snapshot.settings_path.clone());
                }
                snapshot.game.save(&snapshot.campaign_path)?;
                apply::write_settings(&snapshot.settings, &snapshot.settings_path)?;
                Ok(Commit::Saved)
            }
            Payload::Reset { old, fresh, path } => {
                recovered = apply::preserve_unreadable(&path, &mut self.preserved)?;
                let archive = archive_old_journey(&old, &path)?;
                // This commit is the reset linearization point. Settings are
                // retained; nothing fallible may follow it before success.
                fresh.save(&path)?;
                Ok(Commit::Reset {
                    game: fresh,
                    archive,
                })
            }
        })();
        Completion {
            ticket: job.identity.ticket,
            epoch: job.identity.epoch,
            intents: job.identity.intents,
            reset: job.identity.reset,
            result,
            recovered,
            write_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }
}
fn archive_old_journey(old: &Game, path: &std::path::Path) -> Result<Option<PathBuf>, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if !(path.try_exists().map_err(|e| e.to_string())?
        || old.campaign().selected_survivor.is_some()
        || old.campaign().world.start.is_some()
        || old.campaign().attempts > 0)
    {
        return Ok(None);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = path
        .parent()
        .ok_or("Profile path has no parent")?
        .join("journey-archives")
        .join(format!(
            "journey-{stamp}-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(directory.parent().expect("archive parent"))
        .map_err(|e| format!("Previous journey could not be archived: {e}"))?;
    std::fs::create_dir(&directory)
        .map_err(|e| format!("Previous journey could not be archived: {e}"))?;
    let archive = directory.join("campaign.json");
    old.save(&archive).map_err(|e| {
        format!("Previous journey could not be archived: {e}; current journey retained")
    })?;
    Ok(Some(archive))
}
#[derive(Debug)]
struct Worker {
    sender: Option<mpsc::SyncSender<Job>>,
    receiver: Mutex<mpsc::Receiver<Completion>>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn new(mut backend: Box<dyn Backend>) -> Result<Self, String> {
        let (sender, input) = mpsc::sync_channel::<Job>(1);
        let (output, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("lastlight-storage".into())
            .spawn(move || {
                while let Ok(job) = input.recv() {
                    let identity = job.identity.clone();
                    let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        backend.execute(job)
                    }))
                    .unwrap_or_else(|_| {
                        Completion::failed(
                            identity,
                            "Storage worker failed; current game retained".into(),
                        )
                    });
                    if output.send(done).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Cannot start storage worker: {e}"))?;
        Ok(Self {
            sender: Some(sender),
            receiver: Mutex::new(receiver),
            thread: Some(thread),
        })
    }
}
#[derive(Debug, Default)]
pub(super) struct Service {
    worker: Option<Worker>,
    in_flight: Option<Identity>,
    pending: Option<Job>,
    ready: VecDeque<Completion>,
    epoch: u64,
    next_ticket: u64,
    metrics: PersistenceMetrics,
}
pub(super) fn dedup(into: &mut Vec<SaveIntent>, from: impl IntoIterator<Item = SaveIntent>) {
    for intent in from {
        if !into.contains(&intent) {
            into.push(intent);
        }
    }
}
fn critical(intents: &[SaveIntent]) -> bool {
    intents.iter().any(|i| {
        matches!(
            i,
            SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
        )
    })
}
impl Service {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    #[cfg(any(test, feature = "review"))]
    pub fn pending(&self) -> bool {
        self.in_flight.is_some() || self.pending.is_some() || !self.ready.is_empty()
    }
    pub fn reset_pending(&self) -> bool {
        self.in_flight.as_ref().is_some_and(|i| i.reset)
            || self.pending.as_ref().is_some_and(|j| j.identity.reset)
            || self.ready.iter().any(|c| c.reset)
    }
    pub fn blocks_gameplay(&self) -> bool {
        self.reset_pending()
            || self
                .in_flight
                .as_ref()
                .is_some_and(|i| i.epoch == self.epoch && critical(&i.intents))
            || self
                .pending
                .as_ref()
                .is_some_and(|j| critical(&j.identity.intents))
    }
    pub fn user_pending(&self) -> bool {
        self.reset_pending()
            || self
                .ready
                .iter()
                .any(|c| c.epoch == self.epoch && critical(&c.intents))
            || self.in_flight.as_ref().is_some_and(|i| {
                i.epoch == self.epoch && i.intents.iter().any(|i| *i != SaveIntent::Autosave)
            })
            || self.pending.as_ref().is_some_and(|j| {
                j.identity
                    .intents
                    .iter()
                    .any(|i| *i != SaveIntent::Autosave)
            })
            || self.ready.iter().any(|c| {
                c.epoch == self.epoch && c.intents.iter().any(|i| *i != SaveIntent::Autosave)
            })
    }
    #[cfg(any(test, feature = "review"))]
    pub fn metrics(&self) -> PersistenceMetrics {
        PersistenceMetrics {
            pending: self.pending(),
            ..self.metrics
        }
    }
    fn identity(&mut self, intents: Vec<SaveIntent>, reset: bool) -> Result<Identity, String> {
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or("Storage ticket exhausted")?;
        Ok(Identity {
            ticket: self.next_ticket,
            epoch: self.epoch,
            intents,
            reset,
        })
    }
    fn launch(&mut self, job: Job) -> Result<(), String> {
        if self.worker.is_none() {
            self.worker = Some(Worker::new(Box::<FileStore>::default())?);
        }
        let identity = job.identity.clone();
        self.worker
            .as_ref()
            .expect("worker")
            .sender
            .as_ref()
            .expect("active sender")
            .try_send(job)
            .map_err(|e| format!("Storage dispatch failed: {e}"))?;
        self.in_flight = Some(identity);
        Ok(())
    }
    pub fn save(&mut self, snapshot: Snapshot, mut intents: Vec<SaveIntent>) -> Result<(), String> {
        if self.reset_pending() {
            return Err("A new journey is being committed".into());
        }
        if let Some(previous) = self.pending.take() {
            dedup(&mut intents, previous.identity.intents);
        }
        let job = Job {
            identity: self.identity(intents, false)?,
            payload: Payload::Save(Box::new(snapshot)),
        };
        if self.in_flight.is_some() {
            self.pending = Some(job);
            Ok(())
        } else {
            self.launch(job)
        }
    }
    pub fn reset(
        &mut self,
        old: Game,
        fresh: Game,
        path: PathBuf,
    ) -> Result<Vec<SaveIntent>, String> {
        if self.reset_pending() {
            return Err("A new journey is already being committed".into());
        }
        let mut cancelled = Vec::new();
        if let Some(job) = self.pending.take() {
            dedup(&mut cancelled, job.identity.intents);
        }
        if let Some(identity) = &self.in_flight {
            dedup(&mut cancelled, identity.intents.iter().copied());
        }
        self.epoch = self.epoch.checked_add(1).ok_or("Profile epoch exhausted")?;
        let job = Job {
            identity: self.identity(Vec::new(), true)?,
            payload: Payload::Reset {
                old: Box::new(old),
                fresh: Box::new(fresh),
                path,
            },
        };
        if self.in_flight.is_some() {
            self.pending = Some(job);
        } else {
            self.launch(job)?;
        }
        Ok(cancelled)
    }
    pub fn poll(&mut self) -> Option<Completion> {
        let mut done = self.ready.pop_front();
        if done.is_none()
            && let Some(worker) = &mut self.worker
        {
            match worker
                .receiver
                .get_mut()
                .expect("private storage receiver")
                .try_recv()
            {
                Ok(value) => done = Some(value),
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(identity) = self.in_flight.clone() {
                        done = Some(Completion::failed(
                            identity,
                            "Storage worker disconnected".into(),
                        ));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(value) = &done {
            debug_assert!(
                self.in_flight
                    .as_ref()
                    .is_none_or(|i| i.ticket == value.ticket)
            );
            self.in_flight = None;
            if value.result.is_ok() {
                self.metrics.completed_background_saves += 1;
            } else {
                self.metrics.failed_background_saves += 1;
            }
            self.metrics.max_background_write_ms =
                self.metrics.max_background_write_ms.max(value.write_ms);
            if let Some(job) = self.pending.take() {
                let identity = job.identity.clone();
                if let Err(error) = self.launch(job) {
                    self.ready.push_back(Completion::failed(identity, error));
                }
            }
        }
        done
    }
    #[cfg(test)]
    fn wait_ready(&mut self) {
        if self.ready.is_empty() && self.in_flight.is_some() {
            let done = self
                .worker
                .as_mut()
                .expect("worker")
                .receiver
                .get_mut()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("storage worker must finish");
            self.ready.push_back(done);
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        if let Some(worker) = &mut self.worker {
            // Shutdown is the only blocking boundary. Drain the one latest
            // pending immutable job, close input, then join; recv cannot hang.
            if let Some(job) = self.pending.take()
                && let Some(sender) = &worker.sender
            {
                let _ = sender.send(job);
            }
            worker.sender.take();
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
#[cfg(test)]
pub(super) fn wait_for_test(service: &mut Service) {
    service.wait_ready();
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Stamp {
    pub epoch: u64,
    pub reset: bool,
    pub coins: u64,
    /// Transient event records the job's game still carries (counted on a
    /// clone, so the job itself is written unchanged).
    pub events: usize,
}
#[cfg(test)]
pub(crate) struct Gate {
    started: mpsc::Receiver<Stamp>,
    release: mpsc::Sender<()>,
}
#[cfg(test)]
impl Gate {
    pub fn started(&self) -> Stamp {
        self.started
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("worker must receive job")
    }
    pub fn release(&self) {
        self.release.send(()).expect("worker gate connected");
    }
    pub fn no_next_job(&self) -> bool {
        matches!(self.started.try_recv(), Err(mpsc::TryRecvError::Empty))
    }
}
#[cfg(test)]
pub(super) fn gated() -> (Service, Gate) {
    struct Controlled {
        store: FileStore,
        started: mpsc::Sender<Stamp>,
        release: mpsc::Receiver<()>,
    }
    impl Backend for Controlled {
        fn execute(&mut self, job: Job) -> Completion {
            let game = match &job.payload {
                Payload::Save(s) => &s.game,
                Payload::Reset { old, .. } => &**old,
            };
            let coins = game.campaign().coins;
            let events = game.clone().drain_events().count();
            self.started
                .send(Stamp {
                    epoch: job.identity.epoch,
                    reset: job.identity.reset,
                    coins,
                    events,
                })
                .unwrap();
            self.release
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("test must release storage gate");
            self.store.execute(job)
        }
    }
    let (started_tx, started) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let worker = Worker::new(Box::new(Controlled {
        store: FileStore::default(),
        started: started_tx,
        release: release_rx,
    }))
    .unwrap();
    (
        Service {
            worker: Some(worker),
            in_flight: None,
            pending: None,
            ready: VecDeque::new(),
            epoch: 0,
            next_ticket: 0,
            metrics: PersistenceMetrics::default(),
        },
        Gate { started, release },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::SaveIntent;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "lastlight-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn snapshot(&self, coins: u64) -> Snapshot {
            let mut game = Game::default();
            game.campaign_mut().coins = coins;
            Snapshot {
                game,
                settings: Settings::default(),
                campaign_path: self.0.join("campaign.json"),
                settings_path: self.0.join("settings.json"),
                preserve_settings: false,
            }
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            if let (Ok(root), Ok(target)) =
                (std::env::temp_dir().canonicalize(), self.0.canonicalize())
                && target.starts_with(root)
                && target
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("lastlight-storage-")
            {
                let _ = std::fs::remove_dir_all(target);
            }
        }
    }
    #[test]
    fn enqueue_and_poll_never_wait_and_only_the_latest_pending_snapshot_is_written() {
        let directory = Directory::new();
        let (mut service, gate) = gated();
        service
            .save(directory.snapshot(1), vec![SaveIntent::Autosave])
            .unwrap();
        assert_eq!(gate.started().coins, 1);
        assert!(service.poll().is_none()); // worker is deliberately still blocked
        service
            .save(directory.snapshot(2), vec![SaveIntent::Manual])
            .unwrap();
        let mut latest = directory.snapshot(3);
        latest.settings.master = 0.37;
        service
            .save(latest, vec![SaveIntent::SettingsBack])
            .unwrap();
        assert!(gate.no_next_job());
        assert!(service.blocks_gameplay());
        let pending = service.pending.as_ref().unwrap();
        assert!(pending.identity.intents.contains(&SaveIntent::Manual));
        assert!(pending.identity.intents.contains(&SaveIntent::SettingsBack));
        assert_eq!(pending.identity.intents.len(), 2);
        gate.release();
        service.wait_ready();
        let first = service.poll().unwrap();
        assert_eq!(first.intents, vec![SaveIntent::Autosave]);
        assert!(first.result.is_ok());
        assert_eq!(gate.started().coins, 3);
        assert_eq!(
            Game::load(&directory.0.join("campaign.json"))
                .unwrap()
                .campaign()
                .coins,
            1
        );
        gate.release();
        service.wait_ready();
        let last = service.poll().unwrap();
        assert!(last.result.is_ok());
        assert_eq!(last.intents.len(), 2);
        assert_eq!(
            Game::load(&directory.0.join("campaign.json"))
                .unwrap()
                .campaign()
                .coins,
            3
        );
        assert_eq!(
            Settings::load(&directory.0.join("settings.json")).master,
            0.37
        );
        assert!(!service.pending());
        assert_eq!(service.metrics().completed_background_saves, 2);
    }
    #[test]
    fn serialized_reset_barrier_archives_exact_latest_state_and_obsoletes_old_quit() {
        let directory = Directory::new();
        let (mut service, gate) = gated();
        service
            .save(directory.snapshot(1), vec![SaveIntent::Quit])
            .unwrap();
        assert_eq!(gate.started().epoch, 0);
        let old = directory.snapshot(7).game;
        let expected = serde_json::to_value(&old).unwrap();
        let cancelled = service
            .reset(old, Game::default(), directory.0.join("campaign.json"))
            .unwrap();
        assert_eq!(cancelled, vec![SaveIntent::Quit]);
        assert!(service.reset_pending());
        assert!(service.blocks_gameplay());
        assert!(gate.no_next_job());
        gate.release();
        service.wait_ready();
        let stale = service.poll().unwrap();
        assert_ne!(stale.epoch, service.epoch());
        assert!(stale.result.is_ok());
        let reset = gate.started();
        assert!(reset.reset);
        assert_eq!(reset.coins, 7);
        assert_eq!(reset.epoch, service.epoch());
        gate.release();
        service.wait_ready();
        let done = service.poll().unwrap();
        let Commit::Reset { game, archive } = done.result.unwrap() else {
            panic!("reset completion")
        };
        assert_eq!(
            serde_json::to_value(Game::load(&archive.unwrap()).unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(Game::load(&directory.0.join("campaign.json")).unwrap()).unwrap(),
            serde_json::to_value(game).unwrap()
        );
        assert!(!service.pending());
    }
    #[test]
    fn worker_snapshot_preserves_real_report_and_rng_while_live_game_advances_independently() {
        use horde_core::{content::Mode, sim::Input, survivors::SurvivorId};
        let directory = Directory::new();
        let (mut service, gate) = gated();
        let mut snapshot = directory.snapshot(0);
        snapshot
            .game
            .campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        snapshot
            .game
            .campaign_mut()
            .choose_start_country("DEU")
            .unwrap();
        snapshot
            .game
            .deploy_country_stage(0, Mode::Personal)
            .unwrap();
        for _ in 0..130 {
            snapshot.game.tick(Input::default());
        }
        snapshot.game.withdraw();
        let expected = serde_json::to_value(&snapshot.game).unwrap();
        let mut live = snapshot.game.clone();
        service.save(snapshot, vec![SaveIntent::Manual]).unwrap();
        gate.started();
        live.deploy_country_stage(0, Mode::Personal).unwrap();
        for _ in 0..75 {
            live.tick(Input::default());
        }
        assert_ne!(serde_json::to_value(&live).unwrap(), expected);
        gate.release();
        service.wait_ready();
        assert!(service.poll().unwrap().result.is_ok());
        assert_eq!(
            serde_json::to_value(Game::load(&directory.0.join("campaign.json")).unwrap()).unwrap(),
            expected
        );
    }
    #[test]
    fn shutdown_drains_inflight_and_latest_pending_before_joining_the_worker() {
        let directory = Directory::new();
        let (mut service, gate) = gated();
        service
            .save(directory.snapshot(1), vec![SaveIntent::Autosave])
            .unwrap();
        service
            .save(directory.snapshot(9), vec![SaveIntent::Quit])
            .unwrap();
        let controller = std::thread::spawn(move || {
            gate.started();
            gate.release();
            gate.started();
            gate.release();
        });
        drop(service);
        controller.join().unwrap();
        assert_eq!(
            Game::load(&directory.0.join("campaign.json"))
                .unwrap()
                .campaign()
                .coins,
            9
        );
    }
}
