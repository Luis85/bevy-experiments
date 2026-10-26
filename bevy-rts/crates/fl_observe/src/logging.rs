//! Logging setup: run directories, the JSON-lines log file and the headless subscriber
//! (spec §15 A).

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, LineWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::{LevelFilter, filter_fn};
use tracing_subscriber::layer::{Context, Filter, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::DebugConfig;
use crate::profiler::{PHASE_TARGET, Profiler, global_profiler};

/// The directory of one run's files (`log.jsonl`, `journal.jsonl`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunDir {
    /// `<YYYYMMDD-HHMMSS>-<pid>` (UTC), with `-<n>` appended if that already exists.
    pub id: String,
    pub path: PathBuf,
}

/// Creates `root/<YYYYMMDD-HHMMSS>-<pid>/` (UTC) and writes its id to `root/latest.txt`.
///
/// # Errors
/// Any I/O error creating the directories or writing `latest.txt`.
pub fn new_run_dir(root: &Path) -> io::Result<RunDir> {
    std::fs::create_dir_all(root)?;
    let base = format!(
        "{}-{}",
        UtcTime::from_unix(unix_now()).compact(),
        std::process::id()
    );
    let mut n = 1_u32;
    loop {
        let id = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let path = root.join(&id);
        match std::fs::create_dir(&path) {
            Ok(()) => {
                std::fs::write(root.join("latest.txt"), format!("{id}\n"))?;
                return Ok(RunDir { id, path });
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(err) => return Err(err),
        }
    }
}

static RUN_DIR: OnceLock<RunDir> = OnceLock::new();

/// Makes `run` the process's run directory, for code that cannot be handed one (the layer
/// factory of Bevy's `LogPlugin` is a plain `fn`). Only the first call has an effect.
pub fn set_global_run_dir(run: RunDir) {
    // A second run dir is ignored: files already went to the first.
    let _ = RUN_DIR.set(run);
}

/// The run directory set by [`set_global_run_dir`] or [`init_headless`].
pub fn global_run_dir() -> Option<&'static RunDir> {
    RUN_DIR.get()
}

#[expect(
    clippy::disallowed_types,
    reason = "observe-only wall clock: log timestamps (spec §15)"
)]
fn unix_now() -> Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

/// A UTC calendar time, enough for run ids and log timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UtcTime {
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
    micros: u32,
}

impl UtcTime {
    fn from_unix(since_epoch: Duration) -> Self {
        let secs = since_epoch.as_secs();
        let (year, month, day) = civil_from_days(i64::try_from(secs / 86_400).unwrap_or(0));
        let of_day = secs % 86_400;
        UtcTime {
            year,
            month,
            day,
            hour: of_day / 3600,
            minute: of_day % 3600 / 60,
            second: of_day % 60,
            micros: since_epoch.subsec_micros(),
        }
    }

    /// `YYYYMMDD-HHMMSS`.
    fn compact(self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// RFC 3339 with microseconds: `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
    fn rfc3339(self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.micros
        )
    }
}

/// `(year, month, day)` of a day count since 1970-01-01 (H. Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // Both are small and positive by construction.
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

thread_local! {
    /// The `tick` of each `sim_tick` span entered on this thread, innermost last, with the id
    /// of the JSON layer that saw it.
    static OPEN_TICKS: RefCell<Vec<(u64, Option<Value>)>> = const { RefCell::new(Vec::new()) };
}

/// A span's recorded fields, kept in its extensions by the JSON layer.
struct SpanFields(Map<String, Value>);

/// Records fields into a JSON object; `message` and other `Debug` values become strings.
struct JsonVisitor<'a>(&'a mut Map<String, Value>);

impl Visit for JsonVisitor<'_> {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.0
            .insert(field.name().to_owned(), Value::from(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_owned(), Value::from(format!("{value:?}")));
    }
}

/// Writes every event as one JSON object per line; see [`json_file_layer`].
#[derive(Debug)]
pub struct JsonFileLayer {
    /// Tells this layer's open ticks apart in the thread-local stack.
    id: u64,
    out: Mutex<LineWriter<File>>,
}

impl JsonFileLayer {
    fn new(file: File) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        JsonFileLayer {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            out: Mutex::new(LineWriter::new(file)),
        }
    }

    /// The `tick` of the innermost `sim_tick` span this layer saw entered on this thread.
    fn open_tick(&self) -> Option<Value> {
        OPEN_TICKS.with_borrow(|open| {
            open.iter()
                .rev()
                .find(|(owner, _)| *owner == self.id)
                .and_then(|(_, tick)| tick.clone())
        })
    }
}

impl<S> Layer<S> for JsonFileLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Map::new();
        attrs.record(&mut JsonVisitor(&mut fields));
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanFields(fields));
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(fields) = span.extensions_mut().get_mut::<SpanFields>()
        {
            values.record(&mut JsonVisitor(&mut fields.0));
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if span.name() != "sim_tick" {
            return;
        }
        let tick = span
            .extensions()
            .get::<SpanFields>()
            .and_then(|fields| fields.0.get("tick").cloned());
        OPEN_TICKS.with_borrow_mut(|open| open.push((self.id, tick)));
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if span.name() != "sim_tick" {
            return;
        }
        OPEN_TICKS.with_borrow_mut(|open| {
            if let Some(at) = open.iter().rposition(|(owner, _)| *owner == self.id) {
                open.remove(at);
            }
        });
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut fields = Map::new();
        event.record(&mut JsonVisitor(&mut fields));

        let mut tick = None;
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let span_fields = extensions
                    .get::<SpanFields>()
                    .map(|f| f.0.clone())
                    .unwrap_or_default();
                if span.name() == "sim_tick" {
                    tick = span_fields.get("tick").cloned();
                }
                let mut entry = Map::new();
                entry.insert("name".to_owned(), Value::from(span.name()));
                entry.insert("fields".to_owned(), Value::Object(span_fields));
                spans.push(Value::Object(entry));
            }
        }
        // bevy_ecs system spans have no parent, so the tick usually comes from the thread.
        let tick = tick.or_else(|| self.open_tick());

        let mut line = Map::new();
        line.insert(
            "timestamp".to_owned(),
            Value::from(UtcTime::from_unix(unix_now()).rfc3339()),
        );
        line.insert("level".to_owned(), Value::from(meta.level().as_str()));
        line.insert("target".to_owned(), Value::from(meta.target()));
        if let Some(tick) = tick {
            line.insert("tick".to_owned(), tick);
        }
        line.insert("fields".to_owned(), Value::Object(fields));
        line.insert("spans".to_owned(), Value::Array(spans));

        let Ok(mut bytes) = serde_json::to_vec(&line) else {
            return;
        };
        bytes.push(b'\n');
        let Ok(mut out) = self.out.lock() else { return };
        // A log line that cannot be written has nowhere else to go.
        let _ = out.write_all(&bytes);
    }
}

/// A layer that writes `log.jsonl` in `run`: one JSON object per event with `timestamp`
/// (RFC 3339, UTC), `level`, `target`, `fields`, `spans` (name and fields of each span in
/// scope, outermost first) and `tick`, the `tick` field of the enclosing `sim_tick` span (or
/// of the `sim_tick` span entered on this thread, since bevy_ecs system spans have no
/// parent). If the file cannot be opened, the error goes to stderr and the layer does
/// nothing.
pub fn json_file_layer<S>(run: &RunDir) -> impl Layer<S> + use<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let path = run.path.join("log.jsonl");
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => Some(JsonFileLayer::new(file)),
        Err(err) => {
            eprintln!(
                "fl_observe: cannot open {}: {err}; no JSON log",
                path.display()
            );
            None
        }
    }
}

fn is_sim_tick(meta: &Metadata<'_>) -> bool {
    meta.is_span() && meta.name() == "sim_tick"
}

/// The `log_filter`, plus every `sim_tick` span, so each logged record keeps its tick.
struct LogFilter(EnvFilter);

impl LogFilter {
    fn new(directives: &str) -> Self {
        let filter = EnvFilter::try_new(directives).unwrap_or_else(|err| {
            eprintln!("fl_observe: bad log filter {directives:?}: {err}; using \"info\"");
            EnvFilter::new("info")
        });
        LogFilter(filter)
    }
}

impl<S> Filter<S> for LogFilter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, meta: &Metadata<'_>, cx: &Context<'_, S>) -> bool {
        is_sim_tick(meta) || Filter::<S>::enabled(&self.0, meta, cx)
    }

    fn callsite_enabled(&self, meta: &'static Metadata<'static>) -> Interest {
        if is_sim_tick(meta) {
            Interest::always()
        } else {
            Filter::<S>::callsite_enabled(&self.0, meta)
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        // `sim_tick` is an INFO span.
        Filter::<S>::max_level_hint(&self.0).map(|hint| hint.max(LevelFilter::INFO))
    }

    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        Filter::<S>::on_new_span(&self.0, attrs, id, ctx);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        Filter::<S>::on_record(&self.0, id, values, ctx);
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        Filter::<S>::on_enter(&self.0, id, ctx);
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        Filter::<S>::on_exit(&self.0, id, ctx);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        Filter::<S>::on_close(&self.0, id, ctx);
    }
}

/// The spans and events the profiler reads, whatever the log filter: `system`, `sim_tick` and
/// `frame` spans, and the phase marker events on [`PHASE_TARGET`].
pub fn is_profiled(meta: &Metadata<'_>) -> bool {
    if meta.is_span() {
        matches!(meta.name(), "system" | "sim_tick" | "frame")
    } else {
        meta.target() == PHASE_TARGET
    }
}

/// The layers of a process's subscriber, each with its own filter:
/// - `cfg.log_filter` (plus `sim_tick` spans) over a stderr formatter and, if `run` is given
///   and `cfg.json_log`, `log.jsonl`;
/// - `profiler`'s layer, which sees what [`is_profiled`] selects whatever the log filter, if
///   `cfg.profiler` (and nothing otherwise; the logs are unaffected either way).
///
/// A subscriber built on these needs no global filter; one that has one (Bevy's `LogPlugin`)
/// must let the profiled spans and events through.
pub fn observe_layers<S>(
    cfg: &DebugConfig,
    run: Option<&RunDir>,
    profiler: &'static Profiler,
) -> impl Layer<S> + use<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let json = run.filter(|_| cfg.json_log).map(json_file_layer);
    let logs = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .and_then(json)
        .with_filter(LogFilter::new(&cfg.log_filter));
    // Always a layer, off by its filter when `cfg.profiler` is false: a `None` layer would hint
    // `LevelFilter::OFF` to the whole stack and silence the logs too.
    let enabled = cfg.profiler;
    let profiling = profiler
        .layer()
        .with_filter(filter_fn(move |meta| enabled && is_profiled(meta)));
    logs.and_then(profiling)
}

/// Installs the global subscriber of a headless process: [`observe_layers`] with the
/// [`global_profiler`]. Sets `run` as the [`global_run_dir`] and the profiler's budgets.
///
/// Idempotent: only the first call installs anything, and a subscriber installed elsewhere
/// first is kept. Returns the [`global_profiler`].
pub fn init_headless(cfg: &DebugConfig, run: Option<&RunDir>) -> &'static Profiler {
    static INIT: OnceLock<()> = OnceLock::new();
    let profiler = global_profiler();
    INIT.get_or_init(|| {
        profiler.set_budgets(cfg.budgets.clone());
        if let Some(run) = run {
            set_global_run_dir(run.clone());
        }
        // Another global subscriber may already be set (a test harness, say): keep it.
        let _ = tracing_subscriber::registry()
            .with(observe_layers(cfg, run, profiler))
            .try_init();
    });
    profiler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_calendar() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        let t = UtcTime::from_unix(Duration::new(19_723 * 86_400 + 3_723, 5_000));
        assert_eq!(t.compact(), "20240101-010203");
        assert_eq!(t.rfc3339(), "2024-01-01T01:02:03.000005Z");
    }

    #[test]
    fn run_dirs_never_collide() {
        let root = tempfile::tempdir().expect("temp dir");
        let a = new_run_dir(root.path()).expect("first");
        let b = new_run_dir(root.path()).expect("second");
        assert_ne!(a.path, b.path);
        assert!(a.path.is_dir() && b.path.is_dir());
        let latest = std::fs::read_to_string(root.path().join("latest.txt")).expect("latest");
        assert_eq!(latest.trim(), b.id);
    }
}
