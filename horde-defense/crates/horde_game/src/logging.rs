//! Log file output and crash reports.
//!
//! Bevy's `LogPlugin` owns the subscriber; this module only supplies the extra
//! file layer and a panic hook. Logs live in `<save dir>/logs/`.
use bevy::{
    log::{
        BoxedLayer,
        tracing_subscriber::{self, Layer},
    },
    prelude::*,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// Default tracing filter. `RUST_LOG`, when set, replaces it entirely.
pub const DEFAULT_FILTER: &str = "warn,horde_core=info,horde_defense=info,wgpu=error,naga=warn";
const LOG_PREFIX: &str = "lastlight-";
const LOG_SUFFIX: &str = ".log";
/// Number of `lastlight-*.log` files kept, including the current run.
const KEEP_LOGS: usize = 10;

pub fn log_directory() -> PathBuf {
    crate::settings::save_directory().join("logs")
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `LogPlugin::custom_layer`: a plain-text file layer, or `None` (console only)
/// when the log file cannot be created.
pub fn file_layer(_app: &mut App) -> Option<BoxedLayer> {
    let directory = log_directory();
    let file = match open_log_file(&directory, unix_seconds()) {
        Ok(file) => file,
        Err(error) => {
            // The subscriber is not installed yet, so stderr is the only channel.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Log file unavailable in {}: {error}", directory.display());
            }
            return None;
        }
    };
    Some(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(file))
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

/// Installs a panic hook that writes `crash-<unix-ts>.txt` and logs at `error`,
/// then chains the previously installed (default) hook.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = crash_report(
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
        );
        error!("{report}");
        let directory = log_directory();
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

fn crash_report(panic: &str, backtrace: &str) -> String {
    format!(
        "Lastlight crash report\nversion: {}\nos: {} {}\npanic: {panic}\n\nbacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
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
        let report = crash_report("boom at x.rs:1", "frame 0");
        assert!(report.contains(env!("CARGO_PKG_VERSION")));
        assert!(report.contains(std::env::consts::OS));
        assert!(report.contains("boom at x.rs:1"));
        assert!(report.contains("frame 0"));
    }
}
