//! Log file output and crash reports.
//!
//! Bevy's `LogPlugin` owns the subscriber; this module only supplies the extra
//! file layer and a panic hook. Logs live in `<save dir>/logs/`.
//!
//! The file layer also feeds a bounded in-memory ring of the newest lines, so
//! the panic hook can append them to the crash report without re-reading the
//! file. The hook only ever `try_lock`s the ring and degrades to a note.
use bevy::{
    log::{
        BoxedLayer,
        tracing_subscriber::{self, Layer, fmt::MakeWriter},
    },
    prelude::*,
};
use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError, TryLockError},
    time::{SystemTime, UNIX_EPOCH},
};

/// Default tracing filter. `RUST_LOG`, when set, replaces it entirely.
pub const DEFAULT_FILTER: &str = "warn,horde_core=info,horde_defense=info,wgpu=error,naga=warn";
const LOG_PREFIX: &str = "lastlight-";
const LOG_SUFFIX: &str = ".log";
/// Number of `lastlight-*.log` files kept, including the current run.
const KEEP_LOGS: usize = 10;
/// Log lines kept in memory for the crash report.
const TAIL_LINES: usize = 200;
/// Longest line kept in the ring; the rest of a runaway line is dropped.
const MAX_LINE_BYTES: usize = 2048;

/// The ring the file layer writes and the panic hook reads.
static LOG_TAIL: Mutex<LogRing> = Mutex::new(LogRing::new(TAIL_LINES));

/// The newest complete log lines plus the line still being written.
struct LogRing {
    lines: VecDeque<String>,
    capacity: usize,
    open: Vec<u8>,
}
impl LogRing {
    const fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            capacity,
            open: Vec::new(),
        }
    }
    fn push_bytes(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some(end) = rest.iter().position(|&b| b == b'\n') {
            self.extend_open(&rest[..end]);
            self.close_line();
            rest = &rest[end + 1..];
        }
        self.extend_open(rest);
    }
    fn extend_open(&mut self, bytes: &[u8]) {
        let room = MAX_LINE_BYTES.saturating_sub(self.open.len());
        self.open.extend_from_slice(&bytes[..bytes.len().min(room)]);
    }
    fn close_line(&mut self) {
        if self.open.last() == Some(&b'\r') {
            self.open.pop();
        }
        let line = String::from_utf8_lossy(&self.open).into_owned();
        self.open.clear();
        if self.capacity == 0 {
            return;
        }
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }
    /// Complete lines oldest first, then the open line if it has text.
    fn snapshot(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.lines.iter().cloned().collect();
        if !self.open.is_empty() {
            lines.push(String::from_utf8_lossy(&self.open).into_owned());
        }
        lines
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TailUnavailable {
    /// The ring was locked, for example by the panicking thread itself.
    Busy,
}
impl std::fmt::Display for TailUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("log ring busy"),
        }
    }
}

/// `try_lock` attempts before the crash report gives up on the tail.
const SNAPSHOT_ATTEMPTS: usize = 64;

/// Never blocks: a lock still held after a few yields (for example by the
/// panicking thread itself) reports `Busy`; a poisoned ring is still read.
fn snapshot_of(ring: &Mutex<LogRing>) -> Result<Vec<String>, TailUnavailable> {
    for _ in 0..SNAPSHOT_ATTEMPTS {
        match ring.try_lock() {
            Ok(ring) => return Ok(ring.snapshot()),
            Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner().snapshot()),
            Err(TryLockError::WouldBlock) => std::thread::yield_now(),
        }
    }
    Err(TailUnavailable::Busy)
}

/// Blocks normally, but only `try_lock`s while this thread is panicking so the
/// panic hook's own log lines cannot deadlock on a lock the panic left held.
fn lock_for_log<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    if std::thread::panicking() {
        match mutex.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    } else {
        Some(mutex.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// One log write: to the file (when present) and into the ring.
struct TeeWriter<'a, S> {
    sink: Option<S>,
    ring: &'a Mutex<LogRing>,
}
impl<S> Write for TeeWriter<'_, S>
where
    S: DerefMut,
    S::Target: Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = match self.sink.as_mut() {
            Some(sink) => sink.write(buf)?,
            None => buf.len(),
        };
        if let Some(mut ring) = lock_for_log(self.ring) {
            ring.push_bytes(&buf[..written]);
        }
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.sink.as_mut().map_or(Ok(()), |sink| sink.flush())
    }
}

/// `MakeWriter` for the file layer: the optional log file plus the shared ring.
struct TailedLog {
    file: Option<Mutex<File>>,
    ring: &'static Mutex<LogRing>,
}
impl<'a> MakeWriter<'a> for TailedLog {
    type Writer = TeeWriter<'a, MutexGuard<'a, File>>;
    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter {
            sink: self.file.as_ref().and_then(lock_for_log),
            ring: self.ring,
        }
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `LogPlugin::custom_layer`: a plain-text file layer that also feeds the
/// crash-report ring. When the log file cannot be created the layer still
/// feeds the ring. `None` only without a `LaunchConfig`.
/// The directory comes from the `LaunchConfig` resource `main` inserts first.
pub fn file_layer(app: &mut App) -> Option<BoxedLayer> {
    let directory = app
        .world()
        .get_resource::<crate::launch::LaunchConfig>()?
        .log_directory();
    let file = match open_log_file(&directory, unix_seconds()) {
        Ok(file) => Some(Mutex::new(file)),
        Err(error) => {
            // The subscriber is not installed yet, so stderr is the only channel.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Log file unavailable in {}: {error}", directory.display());
            }
            None
        }
    };
    Some(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(TailedLog {
                file,
                ring: &LOG_TAIL,
            })
            .boxed(),
    )
}

fn open_log_file(directory: &Path, stamp: u64) -> std::io::Result<File> {
    std::fs::create_dir_all(directory)?;
    prune_logs(directory, KEEP_LOGS.saturating_sub(1));
    File::create(directory.join(format!("{LOG_PREFIX}{stamp}{LOG_SUFFIX}")))
}

/// Deletes all but the newest `keep` log files (ordered by the timestamp in the
/// name). Failures are ignored: pruning must never block startup.
fn prune_logs(directory: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut logs: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stamp = name
                .strip_prefix(LOG_PREFIX)?
                .strip_suffix(LOG_SUFFIX)?
                .parse()
                .ok()?;
            Some((stamp, entry.path()))
        })
        .collect();
    logs.sort_by(|a, b| b.cmp(a));
    for (_, path) in logs.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// Installs a panic hook that writes `crash-<unix-ts>.txt` (with the last log
/// lines) and logs at `error`, then chains the previously installed hook.
pub fn install_panic_hook(directory: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Snapshot before logging so the tail ends at the panic, not the report.
        let tail = snapshot_of(&LOG_TAIL);
        let report = crash_report(
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
            &tail,
        );
        error!("{report}");
        let path = directory.join(format!("crash-{}.txt", unix_seconds()));
        if std::fs::create_dir_all(&directory)
            .and_then(|()| std::fs::write(&path, &report))
            .is_ok()
        {
            error!("Crash report written to {}", path.display());
        }
        previous(info);
    }));
}

fn crash_report(
    panic: &str,
    backtrace: &str,
    tail: &Result<Vec<String>, TailUnavailable>,
) -> String {
    let mut report = format!(
        "Lastlight crash report\nversion: {}\nos: {} {}\npanic: {panic}\n\nbacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    match tail {
        Ok(lines) if lines.is_empty() => report.push_str("\nlast log lines: none recorded\n"),
        Ok(lines) => {
            report.push_str("\nlast log lines:\n");
            for line in lines {
                report.push_str(line);
                report.push('\n');
            }
        }
        Err(reason) => report.push_str(&format!("\nlast log lines: unavailable ({reason})\n")),
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_newest_logs_and_ignores_other_files() {
        let dir = std::env::temp_dir().join(format!("lastlight-logs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for stamp in [5u64, 100, 20, 3] {
            std::fs::write(dir.join(format!("lastlight-{stamp}.log")), "x").unwrap();
        }
        std::fs::write(dir.join("crash-1.txt"), "x").unwrap();
        prune_logs(&dir, 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["crash-1.txt", "lastlight-100.log", "lastlight-20.log"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crash_report_names_version_os_panic_and_backtrace() {
        let report = crash_report("boom at x.rs:1", "frame 0", &Ok(Vec::new()));
        assert!(report.contains(env!("CARGO_PKG_VERSION")));
        assert!(report.contains(std::env::consts::OS));
        assert!(report.contains("boom at x.rs:1"));
        assert!(report.contains("frame 0"));
    }

    #[test]
    fn log_ring_keeps_only_the_newest_lines() {
        let mut ring = LogRing::new(3);
        for i in 0..5 {
            ring.push_bytes(format!("line {i}\n").as_bytes());
        }
        assert_eq!(ring.snapshot(), ["line 2", "line 3", "line 4"]);
    }

    #[test]
    fn shared_ring_is_bounded_to_the_tail_length() {
        assert_eq!(TAIL_LINES, 200);
        let mut ring = LogRing::new(TAIL_LINES);
        for i in 0..TAIL_LINES * 2 {
            ring.push_bytes(format!("{i}\n").as_bytes());
        }
        let lines = ring.snapshot();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], TAIL_LINES.to_string());
    }

    #[test]
    fn log_ring_joins_partial_writes_and_keeps_the_open_line() {
        let mut ring = LogRing::new(4);
        ring.push_bytes(b"first ");
        ring.push_bytes(b"half\r\nsecond\nthi");
        ring.push_bytes(b"rd");
        assert_eq!(ring.snapshot(), ["first half", "second", "third"]);
    }

    #[test]
    fn log_ring_caps_a_single_runaway_line() {
        let mut ring = LogRing::new(2);
        ring.push_bytes(&vec![b'x'; MAX_LINE_BYTES * 2]);
        ring.push_bytes(b"\n");
        assert_eq!(ring.snapshot()[0].len(), MAX_LINE_BYTES);
    }

    #[test]
    fn crash_report_appends_last_log_lines_after_the_backtrace() {
        let tail = Ok(vec!["INFO boot".to_owned(), "WARN low ammo".to_owned()]);
        let report = crash_report("boom", "frame 0", &tail);
        let backtrace = report.find("backtrace:").unwrap();
        let section = report
            .find("\nlast log lines:\nINFO boot\nWARN low ammo\n")
            .unwrap();
        assert!(section > backtrace, "{report}");
    }

    #[test]
    fn crash_report_degrades_when_the_tail_is_unavailable() {
        let report = crash_report("boom", "frame 0", &Err(TailUnavailable::Busy));
        assert!(
            report.contains("last log lines: unavailable (log ring busy)"),
            "{report}"
        );
        let report = crash_report("boom", "frame 0", &Ok(Vec::new()));
        assert!(report.contains("last log lines: none recorded"), "{report}");
    }

    #[test]
    fn snapshot_does_not_block_while_the_ring_lock_is_held() {
        let ring = Mutex::new(LogRing::new(4));
        ring.lock().unwrap().push_bytes(b"kept\n");
        let guard = ring.lock().unwrap();
        assert_eq!(snapshot_of(&ring), Err(TailUnavailable::Busy));
        drop(guard);
        assert_eq!(snapshot_of(&ring), Ok(vec!["kept".to_owned()]));
    }

    #[test]
    fn snapshot_survives_a_poisoned_ring() {
        let ring = std::sync::Arc::new(Mutex::new(LogRing::new(4)));
        ring.lock().unwrap().push_bytes(b"before panic\n");
        let poisoner = ring.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("poison the ring");
        })
        .join();
        assert!(ring.is_poisoned());
        assert_eq!(snapshot_of(&ring), Ok(vec!["before panic".to_owned()]));
    }

    #[test]
    fn tee_writer_feeds_the_ring_and_the_file_sink() {
        let ring = Mutex::new(LogRing::new(4));
        let mut sink = Vec::new();
        {
            let mut tee = TeeWriter {
                sink: Some(&mut sink),
                ring: &ring,
            };
            std::io::Write::write_all(&mut tee, b"hello\n").unwrap();
        }
        assert_eq!(sink, b"hello\n");
        assert_eq!(snapshot_of(&ring), Ok(vec!["hello".to_owned()]));
    }

    #[test]
    fn tee_writer_without_a_file_still_feeds_the_ring() {
        let ring = Mutex::new(LogRing::new(4));
        let mut tee: TeeWriter<'_, &mut Vec<u8>> = TeeWriter {
            sink: None,
            ring: &ring,
        };
        std::io::Write::write_all(&mut tee, b"console only\n").unwrap();
        assert_eq!(snapshot_of(&ring), Ok(vec!["console only".to_owned()]));
    }
}
