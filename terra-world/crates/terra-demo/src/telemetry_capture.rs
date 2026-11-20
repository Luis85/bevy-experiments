//! Bounded, opt-in diagnostic retention. This writer never owns game state.
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use terra_bevy::WorldConfig;

pub(crate) const HISTORY_LIMIT: usize = 16;
pub(crate) const MENU_INTERVAL_SECONDS: u64 = 10;
pub(crate) const FINAL_ERROR_PREFIX: &str = "TERRA_DIAGNOSTICS_ERROR ";
const MAX_SAMPLE_BYTES: usize = 64 * 1024;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const LOCK_MAGIC: &[u8] = b"Terra diagnostics lock v1\n";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Menu capture has one bounded destination, outside worlds and private identities.
pub(crate) fn menu_report_path(user_data: &Path) -> PathBuf {
    user_data.join("diagnostics/latest.json")
}

/// The inode stays in place across report replacements and process exits. The
/// operating-system lock, rather than the existence of this file, owns the writer.
pub(crate) fn lock_path(path: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(".terra-diagnostics.lock");
    path.with_file_name(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublishedCapture {
    pub sequence: u64,
    pub captured_unix_ms: u128,
}

#[derive(Debug, Clone)]
pub(crate) struct ReportSummary {
    pub session: String,
    pub sequence: u64,
    pub captured_unix_ms: u64,
    pub status: String,
    pub phase: Option<String>,
    pub checkpoint_generation: Option<u64>,
    pub storage_pending: bool,
    pub storage_blocked: bool,
    pub reported_errors: bool,
}

impl ReportSummary {
    pub(crate) fn describe(&self) -> String {
        let status = match self.status.as_str() {
            "graceful_exit" => "final capture after an exit request",
            "exit_error" => "an error was reported before final capture",
            _ => "running capture; later completion is unknown",
        };
        let mut text = format!(
            "Latest report · {}\nSession {} · capture {} · {status}",
            capture_time_label(self.captured_unix_ms),
            self.session,
            self.sequence
        );
        if let Some(phase) = &self.phase {
            text.push_str(&format!("\nSession state at capture · {phase}"));
        }
        if let Some(generation) = self.checkpoint_generation {
            text.push_str(&format!("\nLast committed checkpoint · {generation}"));
        }
        if self.storage_blocked {
            text.push_str(" · storage blocked");
        } else if self.storage_pending {
            text.push_str(" · checkpoint work pending");
        }
        if self.reported_errors {
            text.push_str(
                "\nThis capture contains errors; review the report before relying on its outcome.",
            );
        }
        text.push_str("\nAn old or missing capture cannot establish current health.");
        text
    }
}

fn capture_time_label(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    if seconds == 0 || seconds > 253_402_300_799 {
        return format!("Unix {milliseconds} ms");
    }
    let stamp = terra_core::WorldClock {
        unix_seconds: seconds as f64,
        ..Default::default()
    }
    .label();
    format!("{}:{:02} UTC", stamp.trim_end_matches(" UTC"), seconds % 60)
}

/// Read only the validated, bounded retained envelope. Never infer a successful
/// exit from file existence, and never parse a world or private session export.
pub(crate) fn read_summary(path: &Path) -> Result<Option<ReportSummary>, String> {
    reject_symlinks(path)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => (),
    }
    let history = read_history(path)?;
    let sample = history
        .back()
        .expect("validated nonempty diagnostic history");
    Ok(Some(ReportSummary {
        session: sample["capture"]["session"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(80)
            .collect(),
        sequence: sample["capture"]["sequence"].as_u64().unwrap_or_default(),
        captured_unix_ms: sample["capture"]["captured_unix_ms"]
            .as_u64()
            .unwrap_or_default(),
        status: sample["capture"]["status"]
            .as_str()
            .unwrap_or_default()
            .into(),
        phase: sample["session_state"]["phase"]
            .as_str()
            .filter(|phase| matches!(*phase, "loading" | "playing" | "paused" | "saving"))
            .map(str::to_owned),
        checkpoint_generation: sample["regional_persistence"]["committed"]["generation"].as_u64(),
        storage_pending: sample["regional_persistence"]["pending"] == true,
        storage_blocked: sample["regional_persistence"]["blocked"] == true,
        reported_errors: [
            &sample["last_world_error"],
            &sample["last_simulation_error"],
            &sample["regional_persistence"]["last_error"],
            &sample["capture"]["last_export_error"],
        ]
        .into_iter()
        .any(Value::is_string),
    }))
}

struct Job {
    sample: Value,
    completion: Option<SyncSender<Result<PublishedCapture, String>>>,
}

/// One in-flight write and one queued sample. Periodic capture never waits on disk.
pub(crate) struct Recorder {
    jobs: SyncSender<Job>,
    outcomes: Mutex<Receiver<Result<PublishedCapture, String>>>,
}

impl Recorder {
    pub(crate) fn start(
        path: PathBuf,
        config: WorldConfig,
        managed_profile: Option<PathBuf>,
    ) -> Result<Self, String> {
        let (jobs, incoming) = mpsc::sync_channel::<Job>(1);
        let (completed, outcomes) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("terra-diagnostics".into())
            .spawn(move || {
                let mut writer = RetainedWriter::new(path, config);
                writer.managed_profile = managed_profile;
                while let Ok(job) = incoming.recv() {
                    let result = writer.write(job.sample);
                    if let Some(reply) = job.completion {
                        // Release the lifetime file lock before acknowledging
                        // final export, so a returned menu can start a new run.
                        drop(writer);
                        let _ = reply.send(result);
                        break;
                    } else {
                        // Never block the writer behind a client that is exiting.
                        let _ = completed.try_send(result);
                    }
                }
            })
            .map_err(|error| format!("Cannot start diagnostic writer: {error}"))?;
        Ok(Self {
            jobs,
            outcomes: Mutex::new(outcomes),
        })
    }

    /// False means the bounded queue is busy; the caller records the skipped sample.
    pub(crate) fn try_capture(&self, sample: Value) -> Result<bool, String> {
        match self.jobs.try_send(Job {
            sample,
            completion: None,
        }) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => Err("Diagnostic writer stopped.".into()),
        }
    }

    pub(crate) fn poll(&self) -> Vec<Result<PublishedCapture, String>> {
        self.outcomes
            .lock()
            .expect("diagnostic result mutex")
            .try_iter()
            .collect()
    }

    /// Final exit is an explicit maintenance barrier, after the world's checkpoint.
    pub(crate) fn finish(&self, sample: Value) -> Result<PublishedCapture, String> {
        let (completed, result) = mpsc::sync_channel(1);
        self.jobs
            .send(Job {
                sample,
                completion: Some(completed),
            })
            .map_err(|_| "Diagnostic writer stopped before final export.".to_owned())?;
        result
            .recv()
            .map_err(|_| "Diagnostic writer did not finish the final export.".to_owned())?
    }
}

pub(crate) fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn session_id() -> String {
    format!(
        "{}-{}-{}",
        unix_millis(),
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Validate before opening a window, and again before every publication.
pub(crate) fn validate_managed_destination(
    config: &WorldConfig,
    profile: Option<&Path>,
    path: &Path,
) -> Result<(), String> {
    let lock = lock_path(path);
    for output in [path, lock.as_path()] {
        crate::output::check_managed_metadata(profile, output)?;
    }
    validate_destination(config, path)
}

pub(crate) fn validate_destination(config: &WorldConfig, path: &Path) -> Result<(), String> {
    if path.file_name().is_none() || path.as_os_str().is_empty() {
        return Err("Telemetry needs a file path.".into());
    }
    let lock = lock_path(path);
    for output in [path, lock.as_path()] {
        if let Some(database) = &config.region_store {
            protect_file_ancestors(database, output)?;
        }
        protect_file_ancestors(&config.save_path, output)?;
        reject_symlinks(output)?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&lock)
        && !metadata.is_file()
    {
        return Err(
            "Diagnostic lock must be a regular Terra lock file; choose another report path.".into(),
        );
    }
    // An explicit output flag is not permission to overwrite a world, asset or
    // unrelated document. Reuse only our bounded diagnostic envelope.
    if path.exists() {
        read_history(path)?;
    }
    Ok(())
}

/// Recursive directory creation must not occupy a future save or SQLite
/// sidecar. Checking only the final report filename would miss those aliases.
pub(crate) fn protect_file_ancestors(protected: &Path, output: &Path) -> Result<(), String> {
    for candidate in output.ancestors().filter(|path| path.file_name().is_some()) {
        terra_bevy::regions::validate_output_path(protected, candidate).map_err(|_| {
            "Telemetry output or its parent would occupy a protected world/session file or journal; choose a separate folder.".to_owned()
        })?;
    }
    Ok(())
}

fn reject_symlinks(path: &Path) -> Result<(), String> {
    let mut current = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(|error| error.to_string())?
    };
    for part in path.components() {
        match part {
            Component::CurDir => continue,
            Component::ParentDir => {
                current.pop();
            }
            _ => current.push(part.as_os_str()),
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Telemetry paths must not contain symbolic links.".into());
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn valid_sample(value: &Value) -> bool {
    value.is_object()
        && value.get("history").is_none()
        && value.get("telemetry_schema").is_none()
        && value["release"].is_string()
        && value["timings"].is_object()
        && value["capture"]["session"].is_string()
        && value["capture"]["sequence"].as_u64().is_some()
        && value["capture"]["captured_unix_ms"].as_u64().is_some()
        && matches!(
            value["capture"]["status"].as_str(),
            Some("running" | "graceful_exit" | "exit_error")
        )
}

fn read_history(path: &Path) -> Result<VecDeque<Value>, String> {
    let meta = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_FILE_BYTES as u64 {
        return Err("Telemetry destination must be a regular Terra diagnostic file of at most 2 MiB; choose a new path.".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Telemetry file exceeds 2 MiB.".into());
    }
    let document: Value = serde_json::from_slice(&bytes).map_err(|_| {
        "Telemetry destination is not a Terra diagnostic report; choose a new path.".to_owned()
    })?;
    let snapshots = document["history"]["snapshots"]
        .as_array()
        .ok_or("Telemetry destination has no retained diagnostic history; choose a new path.")?;
    if document["telemetry_schema"] != 1
        || snapshots.is_empty()
        || snapshots.len() > HISTORY_LIMIT
        || snapshots.iter().any(|sample| {
            !valid_sample(sample)
                || serde_json::to_vec(sample).map_or(true, |bytes| bytes.len() > MAX_SAMPLE_BYTES)
        })
    {
        return Err("Telemetry destination has an unsupported or invalid diagnostic history; choose a new path.".into());
    }
    Ok(snapshots.iter().cloned().collect())
}

struct RetainedWriter {
    path: PathBuf,
    config: WorldConfig,
    managed_profile: Option<PathBuf>,
    history: Option<VecDeque<Value>>,
    failed_writes: u64,
    lock: Option<File>,
}

impl RetainedWriter {
    fn new(path: PathBuf, config: WorldConfig) -> Self {
        Self {
            path,
            config,
            managed_profile: None,
            history: None,
            failed_writes: 0,
            lock: None,
        }
    }

    fn write(&mut self, mut sample: Value) -> Result<PublishedCapture, String> {
        let result = (|| {
            validate_managed_destination(
                &self.config,
                self.managed_profile.as_deref(),
                &self.path,
            )?;
            if !valid_sample(&sample) {
                return Err("Invalid telemetry sample.".into());
            }
            sample["capture"]["prior_write_failures"] = self.failed_writes.into();
            if serde_json::to_vec(&sample)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_SAMPLE_BYTES
            {
                return Err("Diagnostic sample exceeds 64 KiB; previous report preserved.".into());
            }
            if self.lock.is_none() {
                self.lock = Some(acquire_lock(&self.path)?);
            }
            let published = PublishedCapture {
                sequence: sample["capture"]["sequence"].as_u64().unwrap_or_default(),
                captured_unix_ms: u128::from(
                    sample["capture"]["captured_unix_ms"]
                        .as_u64()
                        .unwrap_or_default(),
                ),
            };
            let mut history = match &self.history {
                Some(previous) => previous.clone(),
                None if self.path.exists() => read_history(&self.path)?,
                None => VecDeque::new(),
            };
            history.push_back(sample.clone());
            while history.len() > HISTORY_LIMIT {
                history.pop_front();
            }
            let mut document = sample;
            document["telemetry_schema"] = 1.into();
            document["history"] = serde_json::json!({
                "capacity": HISTORY_LIMIT,
                "snapshots": history,
                "scope": "Most recent successful captures at this path, including prior runs; bounded samples, not a continuous trace. A running capture does not certify a healthy or completed session."
            });
            let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
            if bytes.len() > MAX_FILE_BYTES {
                return Err("Retained telemetry exceeds 2 MiB; previous report preserved.".into());
            }
            publish(&self.path, &bytes)?;
            self.history = Some(history);
            Ok(published)
        })();
        if result.is_err() {
            self.failed_writes = self.failed_writes.saturating_add(1);
        }
        result
    }
}

fn acquire_lock(path: &Path) -> Result<File, String> {
    let lock = lock_path(path);
    let parent = lock
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    terra_bevy::rules::publication::create_private_directories(parent)
        .map_err(|error| error.to_string())?;
    reject_symlinks(&lock)?;
    if !lock.exists() {
        // Publish a complete marker without replacing an existing inode. A
        // crash during initial marker writing cannot leave an empty stale lease.
        let temporary = parent.join(format!(".terra-diagnostics-lock-{}.tmp", session_id()));
        let result = (|| -> Result<Option<File>, String> {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            file.write_all(LOCK_MAGIC)
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            try_lock(&file)?;
            match fs::hard_link(&temporary, &lock) {
                Ok(()) => Ok(Some(file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
                Err(error) => Err(format!("Cannot publish the diagnostic lock: {error}")),
            }
        })();
        let _ = fs::remove_file(&temporary);
        if let Some(file) = result? {
            return Ok(file);
        }
    }
    reject_symlinks(&lock)?;
    let metadata = fs::symlink_metadata(&lock).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() != LOCK_MAGIC.len() as u64 {
        return Err("Diagnostic lock is not a Terra lock file; choose another report path.".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .map_err(|error| format!("Cannot open the diagnostic lock: {error}"))?;
    try_lock(&file)?;
    let mut marker = Vec::new();
    (&file)
        .take(LOCK_MAGIC.len() as u64 + 1)
        .read_to_end(&mut marker)
        .map_err(|error| error.to_string())?;
    if marker != LOCK_MAGIC {
        return Err("Diagnostic lock is not a Terra lock file; choose another report path.".into());
    }
    Ok(file)
}

fn try_lock(file: &File) -> Result<(), String> {
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => "Another session is using this diagnostic report. Close that session or choose a separate report path; its last complete report is preserved.".into(),
        fs::TryLockError::Error(error) => format!("Cannot lock the diagnostic report: {error}"),
    })
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    reject_symlinks(path)?;
    terra_bevy::rules::publication::write_private(
        path,
        bytes,
        terra_bevy::rules::publication::PublishMode::Replace,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("terra-retained-{label}-{}", session_id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn sample(session: &str, sequence: u64, status: &str) -> Value {
        serde_json::json!({"release":"0.14.0", "timings":{}, "capture":{
            "session":session, "sequence":sequence, "status":status, "captured_unix_ms":12345
        }})
    }

    #[test]
    fn managed_recorder_cannot_create_metadata_files_or_parent_directories() {
        let dir = directory("managed-metadata");
        let profile = dir.join("world.json");
        let witness = crate::launcher::model::managed_start_witness_path(&profile);
        let lease = crate::launcher::model::managed_session_lease_path(&profile);
        for protected in [&profile, &witness, &lease] {
            for destination in [protected.clone(), protected.join("report.json")] {
                let recorder = Recorder::start(
                    destination.clone(),
                    WorldConfig::default(),
                    Some(profile.clone()),
                )
                .unwrap();
                assert!(
                    recorder
                        .finish(sample("managed", 1, "graceful_exit"))
                        .is_err()
                );
                assert!(!destination.exists());
                assert!(!lock_path(&destination).exists());
                assert!(!protected.exists());
            }
        }
        let report = dir.join("capture.json");
        let future_profile = lock_path(&report);
        let recorder = Recorder::start(
            report.clone(),
            WorldConfig::default(),
            Some(future_profile.clone()),
        )
        .unwrap();
        assert!(
            recorder
                .finish(sample("managed", 1, "graceful_exit"))
                .is_err()
        );
        assert!(!report.exists());
        assert!(!future_profile.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retained_managed_writer_rechecks_metadata_aliases_on_every_publication() {
        let dir = directory("managed-runtime-alias");
        let path = dir.join("report.json");
        let profile = dir.join("world.json");
        let witness = crate::launcher::model::managed_start_witness_path(&profile);
        fs::write(&profile, b"private profile").unwrap();
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        writer.managed_profile = Some(profile.clone());
        writer.write(sample("managed", 1, "running")).unwrap();
        let original_report = fs::read(&path).unwrap();
        let original_lock = fs::read(lock_path(&path)).unwrap();
        // A valid diagnostic envelope alone must not authorize replacement if
        // its inode becomes protected metadata after the prior capture.
        let lease = crate::launcher::model::managed_session_lease_path(&profile);
        for protected in [&witness, &lease] {
            fs::hard_link(&path, protected).unwrap();
            assert!(writer.write(sample("managed", 2, "running")).is_err());
            assert_eq!(fs::read(&path).unwrap(), original_report);
            assert_eq!(fs::read(protected).unwrap(), original_report);
            fs::remove_file(protected).unwrap();
            fs::hard_link(lock_path(&path), protected).unwrap();
            assert!(writer.write(sample("managed", 3, "running")).is_err());
            assert_eq!(fs::read(&path).unwrap(), original_report);
            assert_eq!(fs::read(protected).unwrap(), original_lock);
            fs::remove_file(protected).unwrap();
        }
        drop(writer);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retention_survives_reopening_and_only_drops_the_oldest_successful_samples() {
        let dir = directory("retention");
        let path = dir.join("report.json");
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        for sequence in 1..=20 {
            writer.write(sample("first", sequence, "running")).unwrap();
        }
        drop(writer);
        let recorder = Recorder::start(path.clone(), WorldConfig::default(), None).unwrap();
        recorder
            .finish(sample("second", 1, "graceful_exit"))
            .unwrap();
        let report: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["capture"]["status"], "graceful_exit");
        let history = report["history"]["snapshots"].as_array().unwrap();
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(history[0]["capture"]["sequence"], 6);
        assert_eq!(history[15]["capture"]["session"], "second");
        assert!(fs::metadata(&path).unwrap().len() < MAX_FILE_BYTES as u64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(recorder);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_capture_preserves_previous_report_and_next_success_records_the_failure() {
        let dir = directory("failed");
        let path = dir.join("report.json");
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        writer.write(sample("test", 1, "running")).unwrap();
        let before = fs::read(&path).unwrap();
        let mut too_big = sample("test", 2, "running");
        too_big["oversized"] = "x".repeat(MAX_SAMPLE_BYTES).into();
        assert!(writer.write(too_big).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        writer.write(sample("test", 3, "exit_error")).unwrap();
        let report: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["capture"]["prior_write_failures"], 1);
        assert_eq!(report["history"]["snapshots"].as_array().unwrap().len(), 2);
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "only the report and its persistent lock remain"
        );
        drop(writer);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn one_writer_owns_a_report_and_the_next_run_recovers_its_history() {
        let dir = directory("exclusive-writer");
        let path = dir.join("report.json");
        let mut first = RetainedWriter::new(path.clone(), WorldConfig::default());
        first.write(sample("first", 1, "running")).unwrap();
        let before = fs::read(&path).unwrap();
        let mut second = RetainedWriter::new(path.clone(), WorldConfig::default());
        assert!(
            second
                .write(sample("second", 1, "running"))
                .unwrap_err()
                .contains("Another session")
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(first);
        second.write(sample("second", 2, "graceful_exit")).unwrap();
        let report: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["capture"]["session"], "second");
        assert_eq!(report["capture"]["prior_write_failures"], 1);
        let history = report["history"]["snapshots"].as_array().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["capture"]["session"], "first");
        assert_eq!(history[1]["capture"]["sequence"], 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(lock_path(&path)).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(second);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn final_acknowledgement_releases_the_lock_before_a_new_session_starts() {
        let dir = directory("final-unlocks");
        let path = dir.join("report.json");
        let first = Recorder::start(path.clone(), WorldConfig::default(), None).unwrap();
        let published = first.finish(sample("first", 7, "graceful_exit")).unwrap();
        assert_eq!(published.sequence, 7);
        assert_eq!(published.captured_unix_ms, 12345);
        // Keep the first Recorder alive: its final acknowledgement, not a later
        // resource drop or scheduler turn, must permit the next menu launch.
        let second = Recorder::start(path.clone(), WorldConfig::default(), None).unwrap();
        second.finish(sample("second", 1, "graceful_exit")).unwrap();
        let summary = read_summary(&path).unwrap().unwrap();
        assert_eq!(summary.session, "second");
        assert_eq!(summary.sequence, 1);
        drop(first);
        drop(second);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diagnostic_lock_child_process() {
        let Some(path) = std::env::var_os("TERRA_TEST_DIAGNOSTIC_LOCK_PATH") else {
            return;
        };
        let mut writer = RetainedWriter::new(path.into(), WorldConfig::default());
        writer.write(sample("terminated", 1, "running")).unwrap();
        loop {
            std::thread::park_timeout(std::time::Duration::from_secs(5));
        }
    }

    #[test]
    fn forced_process_termination_releases_the_operating_system_lock() {
        struct ChildGuard(std::process::Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let dir = directory("terminated-lock");
        let path = dir.join("report.json");
        let mut child = ChildGuard(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "telemetry_capture::tests::diagnostic_lock_child_process",
                    "--nocapture",
                ])
                .env("TERRA_TEST_DIAGNOSTIC_LOCK_PATH", &path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !path.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "diagnostic fixture exited before publication"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "diagnostic fixture did not publish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let before = fs::read(&path).unwrap();
        let mut next = RetainedWriter::new(path.clone(), WorldConfig::default());
        assert!(next.write(sample("next", 1, "running")).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert!(
            lock_path(&path).is_file(),
            "persistent marker is not a stale lease"
        );
        next.write(sample("next", 2, "graceful_exit")).unwrap();
        let report: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let history = report["history"]["snapshots"].as_array().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["capture"]["status"], "running");
        assert_eq!(history[1]["capture"]["session"], "next");
        drop(next);
        drop(child);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lock_paths_preserve_unrelated_files_and_protected_world_outputs() {
        let dir = directory("lock-protection");
        let path = dir.join("report.json");
        let lock = lock_path(&path);
        let unrelated = vec![b'x'; LOCK_MAGIC.len()];
        fs::write(&lock, &unrelated).unwrap();
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        assert!(writer.write(sample("test", 1, "running")).is_err());
        assert_eq!(fs::read(&lock).unwrap(), unrelated);
        assert!(!path.exists());
        fs::remove_file(&lock).unwrap();
        let config = WorldConfig {
            save_path: lock.clone(),
            ..Default::default()
        };
        assert!(validate_destination(&config, &path).is_err());
        assert!(!lock.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn report_summary_is_bounded_read_only_and_uses_the_latest_retained_sample() {
        let dir = directory("summary");
        let missing = dir.join("not-created/report.json");
        assert!(read_summary(&missing).unwrap().is_none());
        assert!(!missing.parent().unwrap().exists());
        let path = dir.join("report.json");
        let unrelated = br#"{"private":"not a diagnostic report"}"#;
        fs::write(&path, unrelated).unwrap();
        assert!(read_summary(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), unrelated);
        fs::remove_file(&path).unwrap();
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        let mut recorded = sample("latest", 4, "running");
        recorded["session_state"] = serde_json::json!({"phase":"paused"});
        recorded["regional_persistence"] =
            serde_json::json!({"committed":{"generation":9},"pending":true});
        writer.write(recorded).unwrap();
        drop(writer);
        let mut document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        document["capture"]["status"] = "graceful_exit".into();
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let before = fs::read(&path).unwrap();
        let summary = read_summary(&path).unwrap().unwrap();
        assert_eq!(
            summary.status, "running",
            "envelope metadata cannot override retained evidence"
        );
        assert_eq!(summary.phase.as_deref(), Some("paused"));
        assert_eq!(summary.checkpoint_generation, Some(9));
        assert!(summary.storage_pending);
        assert!(summary.describe().contains("later completion is unknown"));
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, vec![b' '; MAX_FILE_BYTES + 1]).unwrap();
        assert!(read_summary(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn existing_world_json_and_save_database_aliases_are_never_replaced() {
        let dir = directory("protected");
        let path = dir.join("world.json");
        let original = br#"{"private":"world state"}"#;
        fs::write(&path, original).unwrap();
        let mut writer = RetainedWriter::new(path.clone(), WorldConfig::default());
        assert!(writer.write(sample("test", 1, "running")).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let config = WorldConfig {
            save_path: dir.join("save.json"),
            region_store: Some(dir.join("world.db")),
            ..Default::default()
        };
        for destination in [
            &config.save_path,
            config.region_store.as_ref().unwrap(),
            &dir.join("world.db-wal"),
        ] {
            assert!(validate_destination(&config, destination).is_err());
            assert!(!destination.exists());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recursive_output_folders_cannot_occupy_future_save_or_database_sidecars() {
        let dir = directory("ancestor-protection");
        let config = WorldConfig {
            save_path: dir.join("save.json"),
            region_store: Some(dir.join("world.db")),
            ..Default::default()
        };
        for name in [
            "save.json",
            "world.db",
            "world.db-wal",
            "world.db-shm",
            "world.db-journal",
        ] {
            let parent = dir.join(name);
            let mut writer =
                RetainedWriter::new(parent.join("reports/capture.json"), config.clone());
            assert!(writer.write(sample("test", 1, "running")).is_err());
            assert!(
                !parent.exists(),
                "must not create the protected parent {name}"
            );
        }
        let session = dir.join("session.json");
        assert!(protect_file_ancestors(&session, &session.join("nested/report.json")).is_err());
        assert!(!session.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn symbolic_output_and_parent_aliases_are_rejected_without_writes() {
        use std::os::unix::fs::symlink;
        let dir = directory("links");
        let real = dir.join("real");
        fs::create_dir(&real).unwrap();
        symlink(&real, dir.join("alias")).unwrap();
        symlink(real.join("missing.json"), dir.join("report.json")).unwrap();
        for destination in [dir.join("alias/report.json"), dir.join("report.json")] {
            assert!(validate_destination(&WorldConfig::default(), &destination).is_err());
        }
        assert_eq!(fs::read_dir(&real).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn diagnostic_lock_symlinks_and_protected_hard_links_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = directory("lock-aliases");
        let path = dir.join("report.json");
        let lock = lock_path(&path);
        let protected = dir.join("private.json");
        fs::write(&protected, LOCK_MAGIC).unwrap();
        symlink(&protected, &lock).unwrap();
        assert!(validate_destination(&WorldConfig::default(), &path).is_err());
        fs::remove_file(&lock).unwrap();
        fs::hard_link(&protected, &lock).unwrap();
        let config = WorldConfig {
            save_path: protected.clone(),
            ..Default::default()
        };
        assert!(validate_destination(&config, &path).is_err());
        assert_eq!(fs::read(&protected).unwrap(), LOCK_MAGIC);
        assert!(!path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
