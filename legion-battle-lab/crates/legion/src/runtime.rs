//! Single-owner simulation worker and immutable presentation snapshots.
//!
//! A slow million-unit tick must never block window events, menus or camera input.
//! Only one tick job is submitted at a time. Resets are generation-tagged and
//! coalesced; stale work cannot overwrite a newer scenario. There is one latest
//! result slot, so a slow renderer cannot accumulate an unbounded snapshot queue.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    time::Instant,
};

use battle_core::{Battle, BattleStats, Outcome, ScenarioConfig, UnitStore};

#[derive(Debug)]
pub struct BattleSnapshot {
    pub config: ScenarioConfig,
    pub units: UnitStore,
    pub stats: BattleStats,
    pub outcome: Option<Outcome>,
    pub completed_at: Instant,
    pub span_ticks: u32,
    pub snapshot_ms: f32,
}

impl BattleSnapshot {
    fn capture(battle: &Battle, previous: Option<&Self>) -> Self {
        let began = Instant::now();
        let mut units = battle.units.clone();
        // A publication may cover multiple fixed ticks. Interpolate from the
        // last visible state, not just the immediately preceding physics tick.
        if let Some(previous) = previous {
            units
                .previous_positions
                .clone_from(&previous.units.positions);
        } else {
            units.previous_positions.clone_from(&units.positions);
        }
        Self {
            config: battle.config.clone(),
            units,
            stats: battle.stats.clone(),
            outcome: battle.outcome,
            completed_at: Instant::now(),
            span_ticks: previous
                .map_or(0, |p| battle.stats.tick.saturating_sub(p.stats.tick) as u32),
            snapshot_ms: began.elapsed().as_secs_f32() * 1000.0,
        }
    }
}

enum Command {
    Reset {
        generation: u64,
        config: ScenarioConfig,
    },
    Advance {
        generation: u64,
        ticks: u32,
    },
    Shutdown,
}

pub enum WorkerReply {
    Snapshot {
        generation: u64,
        snapshot: Arc<BattleSnapshot>,
    },
    Failed {
        generation: u64,
        message: String,
    },
}

pub struct SimulationWorker {
    sender: Sender<Command>,
    latest: Arc<Mutex<Option<WorkerReply>>>,
    requested_generation: Arc<AtomicU64>,
    stop_batch: Arc<AtomicBool>,
}

impl SimulationWorker {
    pub fn new() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let latest = Arc::new(Mutex::new(None));
        let requested_generation = Arc::new(AtomicU64::new(0));
        let stop_batch = Arc::new(AtomicBool::new(false));
        let worker_latest = Arc::clone(&latest);
        let worker_generation = Arc::clone(&requested_generation);
        let worker_stop = Arc::clone(&stop_batch);
        std::thread::Builder::new()
            .name("legion-simulation".into())
            .spawn(move || {
                let mut battle: Option<Battle> = None;
                let mut previous: Option<Arc<BattleSnapshot>> = None;
                let mut active_generation = 0;
                while let Ok(mut command) = receiver.recv() {
                    if matches!(command, Command::Reset { .. }) {
                        // Fast configuration changes should build the newest
                        // scenario once, not retain a queue of million-unit worlds.
                        while let Ok(newer) = receiver.try_recv() {
                            match newer {
                                Command::Reset { .. } => command = newer,
                                Command::Shutdown => return,
                                Command::Advance { .. } => {}
                            }
                        }
                    }
                    let generation = match &command {
                        Command::Reset { generation, .. } | Command::Advance { generation, .. } => *generation,
                        Command::Shutdown => return,
                    };
                    if generation != worker_generation.load(Ordering::Acquire) {
                        continue;
                    }
                    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Option<Arc<BattleSnapshot>>, String> {
                        match command {
                            Command::Reset { config, .. } => {
                                // The displayed immutable snapshot preserves the old
                                // picture; do not retain two authoritative backends.
                                battle = None;
                                previous = None;
                                let next = Battle::new(config)?;
                                battle = Some(next);
                                active_generation = generation;
                            }
                            Command::Advance { ticks, .. } => {
                                if active_generation != generation {
                                    return Err("Der Simulationsauftrag gehört zu einem veralteten Szenario.".into());
                                }
                                let state = battle.as_mut().ok_or("Es wurde noch kein Szenario erzeugt.")?;
                                for _ in 0..ticks {
                                    if state.outcome.is_some() || worker_stop.load(Ordering::Acquire)
                                        || generation != worker_generation.load(Ordering::Acquire) {
                                        break;
                                    }
                                    state.step();
                                }
                            }
                            Command::Shutdown => unreachable!(),
                        }
                        if generation != worker_generation.load(Ordering::Acquire) {
                            return Ok(None);
                        }
                        let state = battle.as_ref().ok_or("Der Simulationszustand fehlt.")?;
                        let snapshot = Arc::new(BattleSnapshot::capture(state, previous.as_deref()));
                        previous = Some(Arc::clone(&snapshot));
                        Ok(Some(snapshot))
                    }));
                    let reply = match result {
                        Ok(Ok(Some(snapshot))) => Some(WorkerReply::Snapshot { generation, snapshot }),
                        Ok(Ok(None)) => None,
                        Ok(Err(message)) => Some(WorkerReply::Failed { generation, message }),
                        Err(payload) => {
                            battle = None;
                            previous = None;
                            let detail = payload.downcast_ref::<String>().cloned()
                                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                                .unwrap_or_else(|| "Unbekannter interner Fehler".into());
                            Some(WorkerReply::Failed { generation, message: format!("Simulation angehalten: {detail}") })
                        }
                    };
                    if let Some(reply) = reply {
                        let mut slot = worker_latest.lock().unwrap_or_else(|e| e.into_inner());
                        if generation == worker_generation.load(Ordering::Acquire) {
                            *slot = Some(reply);
                        }
                    }
                }
            })
            .map_err(|error| format!("Simulations-Thread konnte nicht gestartet werden: {error}"))?;
        Ok(Self {
            sender,
            latest,
            requested_generation,
            stop_batch,
        })
    }

    pub fn reset(&self, generation: u64, config: ScenarioConfig) -> Result<(), String> {
        {
            let mut slot = self.latest.lock().unwrap_or_else(|e| e.into_inner());
            self.requested_generation
                .store(generation, Ordering::Release);
            self.stop_batch.store(false, Ordering::Release);
            *slot = None;
        }
        self.sender
            .send(Command::Reset { generation, config })
            .map_err(|_| "Der Simulations-Thread ist nicht mehr verfügbar.".into())
    }

    pub fn advance(&self, generation: u64, ticks: u32) -> Result<(), String> {
        self.stop_batch.store(false, Ordering::Release);
        self.sender
            .send(Command::Advance { generation, ticks })
            .map_err(|_| "Der Simulations-Thread ist nicht mehr verfügbar.".into())
    }

    pub fn take_reply(&self) -> Option<WorkerReply> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn stop_after_current_tick(&self) {
        self.stop_batch.store(true, Ordering::Release);
    }
}

impl Drop for SimulationWorker {
    fn drop(&mut self) {
        self.requested_generation.store(u64::MAX, Ordering::Release);
        let _ = self.sender.send(Command::Shutdown);
        // Do not join on the UI thread. A currently executing fixed tick may
        // finish; the generation guard prevents further work or publication.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn receive(worker: &SimulationWorker) -> WorkerReply {
        let began = Instant::now();
        loop {
            if let Some(reply) = worker.take_reply() {
                return reply;
            }
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "worker did not respond"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn new_generation_replaces_old_work_and_snapshots_are_immutable() {
        let worker = SimulationWorker::new().unwrap();
        let mut config = ScenarioConfig::default();
        config.blue.count = 12;
        config.red.count = 12;
        worker.reset(1, config.clone()).unwrap();
        config.blue.count = 15;
        worker.reset(2, config).unwrap();
        let initial = match receive(&worker) {
            WorkerReply::Snapshot {
                generation: 2,
                snapshot,
            } => snapshot,
            _ => panic!("stale generation published"),
        };
        assert_eq!(initial.units.len(), 27);
        worker.advance(2, 3).unwrap();
        let next = match receive(&worker) {
            WorkerReply::Snapshot {
                generation: 2,
                snapshot,
            } => snapshot,
            _ => panic!("expected completed tick batch"),
        };
        assert_eq!(initial.stats.tick, 0);
        assert_eq!(next.stats.tick, 3);
        assert_eq!(next.span_ticks, 3);
        assert_eq!(next.units.previous_positions, initial.units.positions);
    }

    #[test]
    fn invalid_scenario_reports_failure_instead_of_hanging_the_ui() {
        let worker = SimulationWorker::new().unwrap();
        let mut config = ScenarioConfig::default();
        config.blue.count = 0;
        worker.reset(7, config).unwrap();
        match receive(&worker) {
            WorkerReply::Failed {
                generation,
                message,
            } => {
                assert_eq!(generation, 7);
                assert!(!message.is_empty());
            }
            _ => panic!("invalid scenario was accepted"),
        }
        let mut valid = ScenarioConfig::default();
        valid.blue.count = 4;
        valid.red.count = 4;
        worker.reset(8, valid).unwrap();
        match receive(&worker) {
            WorkerReply::Snapshot {
                generation: 8,
                snapshot,
            } => {
                assert_eq!(snapshot.units.len(), 8);
                assert_eq!(snapshot.stats.tick, 0);
            }
            _ => panic!("worker did not recover after an invalid scenario"),
        }
        worker.advance(8, 1).unwrap();
        match receive(&worker) {
            WorkerReply::Snapshot {
                generation: 8,
                snapshot,
            } => {
                assert_eq!(snapshot.stats.tick, 1);
            }
            _ => panic!("recovered worker could not advance"),
        }
    }
}
