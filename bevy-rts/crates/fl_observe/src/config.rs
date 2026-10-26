//! `debug.ron`: the log filter, the log and journal files, the profiler and its budgets
//! (spec §15 D).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// What a process logs and measures; read from `debug.ron` by [`load_debug_config`].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct DebugConfig {
    /// An `EnvFilter` directive string; `RUST_LOG` overrides it.
    pub log_filter: String,
    /// Write `log.jsonl` into the run directory.
    pub json_log: bool,
    /// Write `journal.jsonl` into the run directory.
    pub journal_file: bool,
    /// Install the profiler layer.
    pub profiler: bool,
    pub budgets: Budgets,
    /// Watch the mod files of a testbed match and hot-reload the content when one changes
    /// (dev builds only; ADR-0012).
    pub hot_reload: bool,
    /// How often the hot-reload watcher looks at the mod files, in milliseconds.
    pub hot_reload_poll_ms: u64,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            log_filter: "info,fl_sim=debug,fl_sim::spawn=info".to_owned(),
            json_log: true,
            journal_file: true,
            profiler: true,
            budgets: Budgets::default(),
            hot_reload: true,
            hot_reload_poll_ms: 500,
        }
    }
}

/// Time budgets in milliseconds. An overrun only logs a warning on target
/// `fl_observe::budget`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Budgets {
    /// One whole sim tick.
    pub tick_ms: f64,
    /// Per phase, by its marker name (`"Input"`, `"Think"`, ...).
    pub per_phase_ms: BTreeMap<String, f64>,
    /// Per feature, by its [`feature_of`](crate::feature_of) name (`"core.movement"`, ...).
    pub per_feature_ms: BTreeMap<String, f64>,
}

impl Default for Budgets {
    /// A 15 ms tick budget and no phase or feature budgets.
    fn default() -> Self {
        Self {
            tick_ms: 15.0,
            per_phase_ms: BTreeMap::new(),
            per_feature_ms: BTreeMap::new(),
        }
    }
}

/// The debug config: from `path` if given, else from `./debug.ron` if it exists, else the
/// defaults. A non-empty `RUST_LOG` replaces `log_filter`. A file that cannot be read or
/// parsed is reported on stderr (logging is not set up yet) and the defaults are used.
pub fn load_debug_config(path: Option<&Path>) -> DebugConfig {
    let rust_log = std::env::var("RUST_LOG").ok();
    load_with(path, Path::new("debug.ron"), rust_log.as_deref())
}

fn load_with(path: Option<&Path>, fallback: &Path, rust_log: Option<&str>) -> DebugConfig {
    let file = match path {
        Some(path) => Some(path),
        None => fallback.exists().then_some(fallback),
    };
    let mut cfg = file.map_or_else(DebugConfig::default, read_or_default);
    if let Some(filter) = rust_log.filter(|f| !f.trim().is_empty()) {
        filter.clone_into(&mut cfg.log_filter);
    }
    cfg
}

fn read_or_default(path: &Path) -> DebugConfig {
    let parsed = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| ron::from_str(&text).map_err(|e| e.to_string()));
    parsed.unwrap_or_else(|err| {
        eprintln!(
            "fl_observe: {}: {err}; using the default debug config",
            path.display()
        );
        DebugConfig::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_config_defaults_and_rust_log_override() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("debug.ron");

        let cfg = load_with(None, &missing, None);
        assert_eq!(cfg.log_filter, "info,fl_sim=debug,fl_sim::spawn=info");
        assert!(cfg.json_log && cfg.journal_file && cfg.profiler);
        assert!((cfg.budgets.tick_ms - 15.0).abs() < f64::EPSILON);
        assert!(cfg.hot_reload);
        assert_eq!(cfg.hot_reload_poll_ms, 500);

        let cfg = load_with(None, &missing, Some("warn,fl_sim=trace"));
        assert_eq!(cfg.log_filter, "warn,fl_sim=trace");
        let cfg = load_with(None, &missing, Some(""));
        assert_eq!(
            cfg.log_filter, "info,fl_sim=debug,fl_sim::spawn=info",
            "an empty RUST_LOG is ignored"
        );

        // Fields left out keep their defaults; RUST_LOG still wins over the file.
        std::fs::write(
            &missing,
            "(json_log: false, budgets: (per_phase_ms: {\"Act\": 3.0}))",
        )
        .expect("write debug.ron");
        let cfg = load_with(None, &missing, None);
        assert!(!cfg.json_log && cfg.profiler);
        assert_eq!(cfg.log_filter, "info,fl_sim=debug,fl_sim::spawn=info");
        assert!((cfg.budgets.tick_ms - 15.0).abs() < f64::EPSILON);
        assert_eq!(cfg.budgets.per_phase_ms.get("Act"), Some(&3.0));
        let cfg = load_with(Some(&missing), Path::new("nowhere.ron"), Some("debug"));
        assert!(!cfg.json_log);
        assert_eq!(cfg.log_filter, "debug");

        // Unknown fields are errors: reported, and the defaults are used.
        std::fs::write(&missing, "(profiler: false, colour: true)").expect("write debug.ron");
        let cfg = load_with(None, &missing, None);
        assert!(cfg.profiler);
    }

    #[test]
    fn repo_debug_ron_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../debug.ron");
        let text = std::fs::read_to_string(&path).expect("debug.ron at the repo root");
        let cfg: DebugConfig = ron::from_str(&text).expect("debug.ron parses");
        assert_eq!(cfg.log_filter, "info,fl_sim=debug,fl_sim::spawn=info");
        assert_eq!(cfg.budgets.per_phase_ms.len(), 8);
        assert!(cfg.hot_reload, "the testbed hot-reloads its content");
        assert_eq!(cfg.hot_reload_poll_ms, 500);
        assert!(
            cfg.budgets
                .per_phase_ms
                .values()
                .all(|ms| (ms - 3.0).abs() < f64::EPSILON)
        );
    }
}
