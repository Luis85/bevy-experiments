//! Bounded transcript persistence. Supervisor-facing checks never acquire an I/O lock.
use std::{
    fs::File,
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
const MAX_MESSAGES: usize = 256;
const SYNC_BYTES: usize = 64 * 1024;
const SYNC_INTERVAL: Duration = Duration::from_millis(50);
const QUEUE_ERROR: &str = "AI client log capture queue exceeded its bounded budget; client stopped to avoid silently losing output";
const DEADLINE_ERROR: &str =
    "AI client log did not finish flushing before its deadline; captured output may be incomplete";
const CLOSED_ERROR: &str = "AI client session log is already closed";

pub(super) trait Sink: Write + Send {
    fn sync(&mut self) -> io::Result<()>;
}
impl Sink for File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}
#[derive(Default)]
struct State {
    queued: AtomicUsize,
    producers: AtomicUsize,
    failed: AtomicU8,
    closed: AtomicBool,
    finished: AtomicBool,
}
impl State {
    fn fail(&self, code: u8) {
        let _ = self
            .failed
            .compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
    }
    fn check(&self) -> Result<(), String> {
        match self.failed.load(Ordering::SeqCst) {
            0 => Ok(()),
            2 => Err(QUEUE_ERROR.into()),
            3 => Err(DEADLINE_ERROR.into()),
            _ => Err(super::IO_ERROR.into()),
        }
    }
}
type Ack = SyncSender<Result<(), String>>;
enum Command {
    Data(Vec<u8>),
    Flush(Ack),
    Finish(Ack),
    #[cfg(test)]
    ReplaceFile(File, Ack),
}
struct Handle {
    sender: SyncSender<Command>,
    state: Arc<State>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        // No filesystem work or join in a destructor. A disconnected queue also
        // lets the worker finish all accepted entries before closing its file.
        if !self.state.closed.swap(true, Ordering::SeqCst) {
            let (ack, _) = mpsc::sync_channel(1);
            let _ = self.sender.try_send(Command::Finish(ack));
        }
    }
}
#[derive(Clone)]
pub(super) struct Writer(Arc<Handle>);
impl Writer {
    pub(super) fn spawn<F>(open: F) -> Result<Self, String>
    where
        F: FnOnce() -> Result<Box<dyn Sink>, String> + Send + 'static,
    {
        let state = Arc::new(State::default());
        let (sender, receiver) = mpsc::sync_channel(MAX_MESSAGES);
        let worker_state = state.clone();
        thread::Builder::new()
            .name("npc-ai-log-writer".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker(open, receiver, &worker_state)
                }));
                if result.is_err() {
                    worker_state.fail(1);
                }
                worker_state.finished.store(true, Ordering::SeqCst);
            })
            .map_err(|_| super::IO_ERROR)?;
        Ok(Self(Arc::new(Handle { sender, state })))
    }
    pub(super) fn check(&self) -> Result<(), String> {
        self.0.state.check()
    }
    pub(super) fn reject_capacity(&self) -> Result<(), String> {
        self.0.state.fail(2);
        self.check()
    }
    pub(super) fn entry(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.check()?;
        let state = &self.0.state;
        state.producers.fetch_add(1, Ordering::SeqCst);
        let result = (|| {
            if state.closed.load(Ordering::SeqCst) {
                return Err(CLOSED_ERROR.into());
            }
            let len = bytes.len();
            if state
                .queued
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |queued| {
                    queued
                        .checked_add(len)
                        .filter(|total| *total <= MAX_QUEUED_BYTES)
                })
                .is_err()
            {
                state.fail(2);
                return state.check();
            }
            if self.0.sender.try_send(Command::Data(bytes)).is_err() {
                state.queued.fetch_sub(len, Ordering::SeqCst);
                state.fail(2);
                return state.check();
            }
            Ok(())
        })();
        state.producers.fetch_sub(1, Ordering::SeqCst);
        result
    }
    fn acknowledge(
        &self,
        command: impl FnOnce(Ack) -> Command,
        timeout: Duration,
    ) -> Result<(), String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        if self.0.sender.try_send(command(sender)).is_err() {
            self.0.state.fail(2);
            return self.check();
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                self.0.state.fail(3);
                self.check()
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.0.state.fail(1);
                self.check()
            }
        }
    }
    pub(super) fn flush(&self, timeout: Duration) -> Result<(), String> {
        if self.0.state.closed.load(Ordering::SeqCst) {
            return self.wait_finished(timeout);
        }
        self.acknowledge(Command::Flush, timeout)
    }
    fn wait_finished(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while !self.0.state.finished.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                self.0.state.fail(3);
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.check()
    }
    pub(super) fn shutdown(&self, timeout: Duration) -> Result<(), String> {
        if self.0.state.closed.swap(true, Ordering::SeqCst) {
            return self.wait_finished(timeout);
        }
        let deadline = Instant::now() + timeout;
        while self.0.state.producers.load(Ordering::SeqCst) != 0 {
            if Instant::now() >= deadline {
                self.0.state.fail(3);
                return self.check();
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.acknowledge(
            Command::Finish,
            deadline.saturating_duration_since(Instant::now()),
        )
    }
    #[cfg(test)]
    pub(super) fn replace_file(&self, file: File) -> Result<(), String> {
        self.acknowledge(
            |ack| Command::ReplaceFile(file, ack),
            Duration::from_secs(2),
        )
    }
}
fn sync(sink: &mut Option<Box<dyn Sink>>, state: &State, dirty: &mut usize) {
    if *dirty > 0 {
        if sink.as_mut().is_some_and(|sink| sink.sync().is_err()) {
            state.fail(1);
            *sink = None;
        }
        *dirty = 0;
    }
}
fn worker<F>(open: F, receiver: Receiver<Command>, state: &State)
where
    F: FnOnce() -> Result<Box<dyn Sink>, String>,
{
    let mut sink = match open() {
        Ok(sink) => Some(sink),
        Err(_) => {
            state.fail(1);
            None
        }
    };
    let mut dirty = 0usize;
    let mut sync_at: Option<Instant> = None;
    loop {
        let command = if let Some(deadline) = sync_at {
            receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        } else {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match command {
            Ok(Command::Data(bytes)) => {
                let len = bytes.len();
                if let Some(writer) = &mut sink {
                    if writer.write_all(&bytes).is_err() {
                        state.fail(1);
                        sink = None;
                    } else {
                        dirty = dirty.saturating_add(len);
                        sync_at.get_or_insert_with(|| Instant::now() + SYNC_INTERVAL);
                    }
                }
                drop(bytes);
                state.queued.fetch_sub(len, Ordering::SeqCst);
                if dirty >= SYNC_BYTES {
                    sync(&mut sink, state, &mut dirty);
                    sync_at = None;
                }
            }
            Ok(Command::Flush(ack)) => {
                sync(&mut sink, state, &mut dirty);
                sync_at = None;
                let _ = ack.send(state.check());
            }
            Ok(Command::Finish(ack)) => {
                sync(&mut sink, state, &mut dirty);
                drop(sink);
                state.finished.store(true, Ordering::SeqCst);
                let _ = ack.send(state.check());
                return;
            }
            #[cfg(test)]
            Ok(Command::ReplaceFile(file, ack)) => {
                sync(&mut sink, state, &mut dirty);
                sync_at = None;
                sink = Some(Box::new(file));
                let _ = ack.send(state.check());
            }
            Err(RecvTimeoutError::Timeout) => {
                sync(&mut sink, state, &mut dirty);
                sync_at = None;
            }
            Err(RecvTimeoutError::Disconnected) => {
                sync(&mut sink, state, &mut dirty);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, Mutex};
    struct Memory {
        bytes: Arc<Mutex<Vec<u8>>>,
        syncs: Arc<AtomicUsize>,
    }
    impl Write for Memory {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Sink for Memory {
        fn sync(&mut self) -> io::Result<()> {
            self.syncs.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    struct Blocked {
        gate: Arc<(Mutex<bool>, Condvar)>,
        started: Option<SyncSender<()>>,
    }
    impl Write for Blocked {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            let (lock, signal) = &*self.gate;
            let mut release = lock.lock().unwrap();
            while !*release {
                release = signal.wait(release).unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Sink for Blocked {
        fn sync(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Release(Arc<(Mutex<bool>, Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            *self.0.0.lock().unwrap() = true;
            self.0.1.notify_all();
        }
    }
    fn blocked() -> (Writer, Receiver<()>, Release) {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Release(gate.clone());
        let (started, receiver) = mpsc::sync_channel(1);
        let writer = Writer::spawn(move || {
            Ok(Box::new(Blocked {
                gate,
                started: Some(started),
            }))
        })
        .unwrap();
        (writer, receiver, release)
    }
    #[test]
    fn blocked_writer_does_not_block_checks_or_bounded_flush() {
        let (writer, started, release) = blocked();
        writer.entry(b"owned blocking output".to_vec()).unwrap();
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        let before = Instant::now();
        assert!(writer.check().is_ok());
        writer.entry(b"queued output".to_vec()).unwrap();
        assert_eq!(
            writer.flush(Duration::from_millis(10)).unwrap_err(),
            DEADLINE_ERROR
        );
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(
            writer
                .entry(b"no further untracked output".to_vec())
                .is_err()
        );
        drop(release);
        assert!(writer.shutdown(Duration::from_secs(1)).is_err());
    }
    #[test]
    fn queue_budget_counts_in_flight_and_queued_bytes_and_latches_pressure() {
        let (writer, started, release) = blocked();
        writer.entry(vec![b'x'; MAX_QUEUED_BYTES]).unwrap();
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(writer.entry(vec![b'y']).unwrap_err(), QUEUE_ERROR);
        assert!(writer.0.state.queued.load(Ordering::SeqCst) <= MAX_QUEUED_BYTES);
        assert_eq!(writer.check().unwrap_err(), QUEUE_ERROR);
        drop(release);
        assert!(writer.shutdown(Duration::from_secs(1)).is_err());
    }
    #[test]
    fn batched_sync_and_shutdown_preserve_every_accepted_entry_in_order() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let syncs = Arc::new(AtomicUsize::new(0));
        let memory = Memory {
            bytes: bytes.clone(),
            syncs: syncs.clone(),
        };
        let writer = Writer::spawn(move || Ok(Box::new(memory))).unwrap();
        let mut expected = Vec::new();
        for index in 0..64u8 {
            let entry = vec![index; 1024];
            expected.extend_from_slice(&entry);
            writer.entry(entry).unwrap();
        }
        writer.flush(Duration::from_secs(1)).unwrap();
        writer.shutdown(Duration::from_secs(1)).unwrap();
        assert_eq!(*bytes.lock().unwrap(), expected);
        assert!(syncs.load(Ordering::SeqCst) < 64);
        assert!(writer.entry(b"after close".to_vec()).is_err());
        writer.shutdown(Duration::from_secs(1)).unwrap();
    }
}
