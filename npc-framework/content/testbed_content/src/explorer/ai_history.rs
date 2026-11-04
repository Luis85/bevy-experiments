//! Read-only, bounded native views of this workspace's owned client outputs.
//! Never reads provider history, credentials, private client state or simulation data.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

pub const PAGE_BYTES: usize = 12 * 1024;
/// Request the final bounded page without inspecting paths in a UI worker.
pub const LATEST_OFFSET: u64 = u64::MAX;
const MAX_ENTRIES: usize = 256;
const MAX_SCAN: usize = 4096;
const MAX_INDEX_BYTES: u64 = 1024 * 1024;
const MAX_JSON_BYTES: u64 = 256 * 1024;
const UNAVAILABLE: &str = "This history file is unavailable or has an unsupported format. Refresh, or check the configured output folder.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Metrics,
    Log,
}
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub key: String,
    pub label: String,
    pub kind: EntryKind,
    pub metrics_key: Option<String>,
}
#[derive(Clone, Debug)]
pub struct History {
    pub entries: Vec<HistoryEntry>,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub title: String,
    pub text: String,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub previous_offset: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Provider {
    Codex,
    Claude,
}
impl Provider {
    fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    owner_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Indexed {
    version: u32,
    owner_id: String,
    kind: EntryKind,
    id: String,
    path: PathBuf,
    provider: Provider,
    created_unix_seconds: u64,
    metrics_id: Option<String>,
}
struct Output {
    entry: HistoryEntry,
    path: PathBuf,
    id: String,
    created: u64,
    provider: Option<Provider>,
}

fn id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn log_id(value: &str) -> Option<u64> {
    let (stamp, suffix) = value.split_once('-')?;
    if stamp.is_empty() || stamp.len() > 12 || !id(suffix) {
        return None;
    }
    stamp.parse().ok()
}
fn regular(path: &Path) -> Result<File, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| UNAVAILABLE)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(UNAVAILABLE.into());
    }
    File::open(path).map_err(|_| UNAVAILABLE.into())
}
fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = regular(path)?;
    if file.metadata().map_err(|_| UNAVAILABLE)?.len() > limit {
        return Err(UNAVAILABLE.into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UNAVAILABLE)?;
    if bytes.len() as u64 > limit {
        return Err(UNAVAILABLE.into());
    }
    Ok(bytes)
}
fn json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    let bytes = bytes(path, limit)?;
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
        .map_err(|_| UNAVAILABLE.into())
}
fn workspace_private(workspace: &Path) -> Result<Option<PathBuf>, String> {
    let root = workspace.canonicalize().map_err(
        |_| "The agent folder is unavailable. Start or resume its world, then refresh history.",
    )?;
    let private = root.join(".ai-client");
    if !private.exists() {
        return Ok(None);
    }
    if fs::symlink_metadata(&private)
        .map_err(|_| UNAVAILABLE)?
        .file_type()
        .is_symlink()
        || !private.is_dir()
        || !private
            .canonicalize()
            .map_err(|_| UNAVAILABLE)?
            .starts_with(&root)
    {
        return Err(UNAVAILABLE.into());
    }
    Ok(Some(private))
}
fn utc_label(timestamp: u64) -> String {
    // Pure proleptic-Gregorian conversion, bounded to printable years 1970..9999.
    // Zero is the history schema's missing-timestamp fallback, not a clock read.
    if timestamp == 0 || timestamp > 253_402_300_799 {
        return "Time unavailable".into();
    }
    let days = timestamp / 86_400 + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    year += u64::from(month <= 2);
    let hour = timestamp % 86_400 / 3600;
    let minute = timestamp % 3600 / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}
fn output(
    kind: EntryKind,
    id: String,
    path: PathBuf,
    provider: Option<Provider>,
    created: u64,
    metrics_id: Option<String>,
) -> Result<Output, String> {
    let (prefix, extension, title) = match kind {
        EntryKind::Metrics if self::id(&id) => ("metrics", "json", "metrics"),
        EntryKind::Log if log_id(&id).is_some() => ("log", "md", "log"),
        _ => return Err(UNAVAILABLE.into()),
    };
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path
            .to_str()
            .is_none_or(|path| path.chars().any(char::is_control))
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || path.file_name().and_then(|name| name.to_str())
            != Some(format!("{id}.{extension}").as_str())
        || metrics_id.as_deref().is_some_and(|id| !self::id(id))
    {
        return Err(UNAVAILABLE.into());
    }
    let key = format!("{prefix}:{id}");
    let provider_label = provider.map_or("AI", Provider::label);
    Ok(Output {
        entry: HistoryEntry {
            key,
            label: format!(
                "{provider_label} {title} | {} | {}",
                utc_label(created),
                &id[id.len() - 6..]
            ),
            kind,
            metrics_key: metrics_id.map(|id| format!("metrics:{id}")),
        },
        path,
        id,
        created,
        provider,
    })
}
fn discover(workspace: &Path) -> Result<(Vec<Output>, bool), String> {
    let Some(private) = workspace_private(workspace)? else {
        return Ok((Vec::new(), false));
    };
    let mut found = BTreeMap::new();
    let mut limited = false;
    let index = private.join("history-index.jsonl");
    if index.exists() {
        let owner: Owner = json(&private.join("history-owner.json"), 1024)?;
        if owner.version != 1 || !id(&owner.owner_id) {
            return Err(UNAVAILABLE.into());
        }
        let mut file = regular(&index)?;
        let len = file.metadata().map_err(|_| UNAVAILABLE)?.len();
        let start = len.saturating_sub(MAX_INDEX_BYTES);
        limited |= start > 0;
        file.seek(SeekFrom::Start(start)).map_err(|_| UNAVAILABLE)?;
        let mut tail = Vec::new();
        file.take(MAX_INDEX_BYTES)
            .read_to_end(&mut tail)
            .map_err(|_| UNAVAILABLE)?;
        let mut lines = tail.split(|byte| *byte == b'\n').collect::<Vec<_>>();
        if start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        for line in lines
            .into_iter()
            .rev()
            .filter(|line| !line.is_empty())
            .take(MAX_SCAN)
        {
            if line.len() > 16 * 1024 {
                limited = true;
                continue;
            }
            let record: Indexed = match serde_json::from_slice(line) {
                Ok(record) => record,
                Err(_) => {
                    limited = true;
                    continue;
                }
            };
            if record.version != 1 || record.owner_id != owner.owner_id {
                limited = true;
                continue;
            }
            match output(
                record.kind,
                record.id,
                record.path,
                Some(record.provider),
                record.created_unix_seconds,
                record.metrics_id,
            ) {
                Ok(item) => {
                    found.entry(item.entry.key.clone()).or_insert(item);
                }
                Err(_) => limited = true,
            }
        }
    }
    // Compatibility is confined to this workspace's output folders and typed
    // canonical metrics ledger. Earlier custom exports may have no index, but
    // their canonical accounting remains here. Shared external folders are
    // NEVER scanned; only exact indexed files are read.
    for (folder, kind, extension) in [
        ("metrics", EntryKind::Metrics, "json"),
        ("logs", EntryKind::Log, "md"),
        ("metrics-ledger", EntryKind::Metrics, "json"),
    ] {
        let directory = private.join(folder);
        if !directory.exists() {
            continue;
        }
        if fs::symlink_metadata(&directory)
            .map_err(|_| UNAVAILABLE)?
            .file_type()
            .is_symlink()
        {
            limited = true;
            continue;
        }
        let entries = fs::read_dir(&directory).map_err(|_| UNAVAILABLE)?;
        let mut paths = Vec::new();
        for (scanned, entry) in entries.take(MAX_SCAN + 1).enumerate() {
            if scanned == MAX_SCAN {
                limited = true;
                break;
            }
            let Ok(entry) = entry else {
                limited = true;
                continue;
            };
            if entry.path().extension().and_then(|value| value.to_str()) == Some(extension) {
                paths.push(entry.path());
            }
        }
        paths.sort();
        paths.reverse();
        limited |= paths.len() > MAX_ENTRIES;
        for path in paths.into_iter().take(MAX_ENTRIES) {
            let Some(id) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            let key = format!(
                "{}:{id}",
                if kind == EntryKind::Metrics {
                    "metrics"
                } else {
                    "log"
                }
            );
            if found.contains_key(&key) {
                continue;
            }
            let details = if kind == EntryKind::Metrics {
                let Ok(metrics) = metrics_document(&path, &id) else {
                    limited = true;
                    continue;
                };
                (
                    Some(metrics.provider),
                    metrics
                        .updated_unix_seconds
                        .or(metrics.created_unix_seconds)
                        .unwrap_or(0),
                )
            } else {
                let Some(created) = log_id(&id) else {
                    continue;
                };
                (None, created)
            };
            if let Ok(item) = output(kind, id, path, details.0, details.1, None) {
                found.insert(item.entry.key.clone(), item);
            }
        }
    }
    let mut found: Vec<_> = found.into_values().collect();
    found.sort_by(|a, b| {
        b.created
            .cmp(&a.created)
            .then_with(|| a.entry.key.cmp(&b.entry.key))
    });
    let mut metrics = 0;
    let mut logs = 0;
    found.retain(|item| {
        let count = match item.entry.kind {
            EntryKind::Metrics => &mut metrics,
            EntryKind::Log => &mut logs,
        };
        if *count == MAX_ENTRIES {
            limited = true;
            false
        } else {
            *count += 1;
            true
        }
    });
    Ok((found, limited))
}

pub fn list(workspace: &Path) -> Result<History, String> {
    let (outputs, limited) = discover(workspace)?;
    let entries: Vec<_> = outputs.into_iter().map(|item| item.entry).collect();
    let message = if entries.is_empty() && limited {
        "No readable history entries were found. Unsupported or unavailable records were omitted; check the output folders and refresh."
    } else if entries.is_empty() {
        "No AI session history yet. Start Codex or Claude to create a log and session metrics. Earlier externally configured files appear after a client records their location."
    } else if limited {
        "Showing up to 256 recent entries per kind. Older files are preserved; some unavailable or unsupported records were omitted."
    } else {
        "Logs show output captured by this game client. Unreported usage, cost, model and effort remain unavailable."
    };
    Ok(History {
        entries,
        message: message.into(),
    })
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cost_microusd: Option<u64>,
    model: Option<String>,
    effort: Option<String>,
}
#[derive(Deserialize)]
struct Selection {
    requested_model: String,
    requested_effort: Option<String>,
}
#[derive(Deserialize)]
struct Turn {
    turn: u64,
    outcome: Option<String>,
    usage: Usage,
    #[serde(default)]
    provider_context: Option<ProviderContext>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct ProviderContext {
    cli_version: Option<String>,
    usage_scope: Option<String>,
    adapter_revision: Option<u32>,
}
#[derive(Deserialize)]
struct MetricsDocument {
    version: u32,
    metrics_id: String,
    provider: Provider,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    selection_intent: Option<Selection>,
    #[serde(default)]
    provider_context: Option<ProviderContext>,
    #[serde(default)]
    totals: Usage,
    #[serde(default)]
    reported_session_usage: Option<Usage>,
    #[serde(default)]
    cost_basis: Option<String>,
    #[serde(default)]
    mixed_cost_basis: bool,
    #[serde(default)]
    accounting_coverage: Option<String>,
    #[serde(default)]
    turns_started: Option<u64>,
    #[serde(default)]
    turns_completed: Option<u64>,
    #[serde(default)]
    turns_failed: Option<u64>,
    #[serde(default)]
    turns_interrupted: Option<u64>,
    #[serde(default)]
    created_unix_seconds: Option<u64>,
    #[serde(default)]
    updated_unix_seconds: Option<u64>,
    #[serde(default)]
    history: Vec<Turn>,
}
fn metrics_document(path: &Path, expected_id: &str) -> Result<MetricsDocument, String> {
    let metrics: MetricsDocument = json(path, MAX_JSON_BYTES)?;
    if !(1..=2).contains(&metrics.version)
        || metrics.metrics_id != expected_id
        || !id(expected_id)
        || metrics.history.len() > 64
    {
        return Err(UNAVAILABLE.into());
    }
    Ok(metrics)
}
fn model(value: Option<&str>) -> &str {
    value
        .filter(|value| {
            value.len() <= 128
                && ["gpt-", "claude-", "codex-", "o1", "o3", "o4"]
                    .iter()
                    .any(|prefix| value.starts_with(prefix))
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_' | b'.')
                })
                && !value
                    .as_bytes()
                    .windows(24)
                    .any(|run| run.iter().all(u8::is_ascii_hexdigit))
        })
        .unwrap_or("Unavailable")
}
fn effort(value: Option<&str>) -> &str {
    value
        .filter(|value| {
            matches!(
                *value,
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
            )
        })
        .unwrap_or("Unavailable")
}
fn number(value: Option<u64>) -> String {
    value.map_or_else(|| "Unavailable".into(), |value| value.to_string())
}
fn cost(value: Option<u64>) -> String {
    value.map_or_else(
        || "Unavailable".into(),
        |value| format!("${}.{:06} USD", value / 1_000_000, value % 1_000_000),
    )
}
fn context_version(context: Option<&ProviderContext>) -> &str {
    context
        .and_then(|context| context.cli_version.as_deref())
        .filter(|version| {
            version.len() <= 32
                && version.split('.').count() == 3
                && version.split('.').all(|part| {
                    !part.is_empty()
                        && part.len() <= 10
                        && (part.len() == 1 || !part.starts_with('0'))
                        && part.bytes().all(|byte| byte.is_ascii_digit())
                        && part.parse::<u32>().is_ok()
                })
        })
        .unwrap_or("Unavailable")
}
fn context_scope(context: Option<&ProviderContext>) -> &'static str {
    match context.and_then(|context| context.usage_scope.as_deref()) {
        Some("turn") => "Per turn",
        Some("session_cumulative") => "Session cumulative",
        _ => "Unknown",
    }
}
fn metrics_text(metrics: &MetricsDocument) -> String {
    let legacy = metrics.version == 1 && matches!(metrics.provider, Provider::Codex);
    let totals_label = if legacy {
        "Unverified legacy"
    } else {
        "Accounted"
    };
    let basis = match metrics.cost_basis.as_deref() {
        _ if metrics.mixed_cost_basis => "Mixed reported cost bases",
        Some("api_equivalent") => "API-equivalent estimate, not a subscription bill",
        Some("actual_billed") => "Provider-reported billed cost",
        Some("mixed") => "Mixed reported cost bases",
        _ => "Cost basis unavailable or unspecified",
    };
    // The user's five core values fit before selection/adapter diagnostics,
    // including the normal narrow native viewport. Keep cost meaning adjacent.
    let mut text = format!(
        "{} session metrics\n{totals_label} input tokens: {}\n{totals_label} output tokens: {}\nReported cost: {}\n{basis}\nActual model: {}\nActual effort: {}\n",
        metrics.provider.label(),
        number(metrics.totals.input_tokens),
        number(metrics.totals.output_tokens),
        cost(metrics.totals.cost_microusd),
        model(metrics.model.as_deref()),
        effort(metrics.effort.as_deref())
    );
    if legacy {
        text.push_str("Earlier Codex scope was not verified. These legacy numbers are not certified totals; originals remain unchanged.\n");
    }
    let coverage = match metrics.accounting_coverage.as_deref() {
        Some("conversation_start") => {
            "Tracked from conversation start; missing reports remain unknown"
        }
        Some("since_adoption_baseline") => {
            "Partial: tracked after adopting an existing conversation"
        }
        Some("since_legacy_rebase") => {
            "Partial: tracked after an accounting migration; earlier figures are unverified"
        }
        _ => "Historical coverage unspecified",
    };
    let _ = writeln!(
        text,
        "\nCached input tokens: {}\nTurns started: {}\nCompleted: {} | Failed: {} | Interrupted: {}\nCoverage: {coverage}",
        number(metrics.totals.cached_input_tokens),
        number(metrics.turns_started),
        number(metrics.turns_completed),
        number(metrics.turns_failed),
        number(metrics.turns_interrupted)
    );
    if let Some(intent) = &metrics.selection_intent {
        let _ = writeln!(
            text,
            "\nRequested model: {}\nRequested effort: {}\nRequested settings are selection intent, not proof of execution.",
            model(Some(&intent.requested_model)),
            effort(intent.requested_effort.as_deref())
        );
    }
    let context = metrics.provider_context.as_ref();
    let _ = writeln!(
        text,
        "\nCLI version: {}\nVerified usage scope: {}\nAdapter revision: {}",
        context_version(context),
        context_scope(context),
        number(
            context
                .and_then(|context| context.adapter_revision)
                .map(u64::from)
        )
    );
    if let Some(reported) = &metrics.reported_session_usage {
        let _ = writeln!(
            text,
            "\nLatest reported cumulative snapshot (separate from accounted deltas):\nInput: {} | Output: {} | Cached: {}\nCost: {}",
            number(reported.input_tokens),
            number(reported.output_tokens),
            number(reported.cached_input_tokens),
            cost(reported.cost_microusd)
        );
    }
    if !metrics.history.is_empty() {
        text.push_str(if legacy {
            "\nUnverified legacy turn usage (scope may be incorrect):\n"
        } else {
            "\nRecorded turns (usage reflects each accounted interval):\n"
        });
    }
    for turn in &metrics.history {
        let outcome = match turn.outcome.as_deref() {
            Some("completed") => "completed",
            Some("failed") => "failed",
            Some("interrupted") => "interrupted",
            _ => "pending or unavailable",
        };
        let _ = writeln!(
            text,
            "Turn {}: {outcome}\n  Input {} | Output {} | Cost {}\n  Actual model {} | Effort {}",
            turn.turn,
            number(turn.usage.input_tokens),
            number(turn.usage.output_tokens),
            cost(turn.usage.cost_microusd),
            model(turn.usage.model.as_deref()),
            effort(turn.usage.effort.as_deref())
        );
        let context = turn.provider_context.as_ref();
        let _ = writeln!(
            text,
            "  CLI {} | Scope {} | Adapter {}",
            context_version(context),
            context_scope(context),
            number(
                context
                    .and_then(|context| context.adapter_revision)
                    .map(u64::from)
            )
        );
    }
    text
}
fn page(title: String, bytes: &[u8], offset: u64, total: u64) -> Result<Page, String> {
    let start = bytes
        .iter()
        .take(3)
        .take_while(|byte| (**byte & 0xc0) == 0x80)
        .count();
    let mut end = PAGE_BYTES.min(bytes.len());
    while end < bytes.len() && (bytes[end] & 0xc0) == 0x80 {
        end += 1;
    }
    let text = std::str::from_utf8(&bytes[start..end]).map_err(|_| UNAVAILABLE)?;
    let text: String = text
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect();
    let next = offset.saturating_add(PAGE_BYTES as u64);
    Ok(Page {
        title,
        text,
        offset,
        next_offset: (next < total).then_some(next),
        previous_offset: (offset > 0).then_some(offset.saturating_sub(PAGE_BYTES as u64)),
    })
}

pub fn read(workspace: &Path, key: &str, offset: u64) -> Result<Page, String> {
    if key.len() > 90 || (offset != LATEST_OFFSET && !offset.is_multiple_of(PAGE_BYTES as u64)) {
        return Err(UNAVAILABLE.into());
    }
    let (outputs, _) = discover(workspace)?;
    let item = outputs.into_iter().find(|item| item.entry.key == key).ok_or("This entry is no longer in the recent history list. Refresh to select an available session.")?;
    if item.entry.kind == EntryKind::Metrics {
        let metrics = metrics_document(&item.path, &item.id)?;
        if item
            .provider
            .is_some_and(|provider| provider != metrics.provider)
        {
            return Err(UNAVAILABLE.into());
        }
        let text = metrics_text(&metrics);
        let offset = if offset == LATEST_OFFSET {
            (text.len() as u64).saturating_sub(1) / PAGE_BYTES as u64 * PAGE_BYTES as u64
        } else {
            offset
        };
        if offset > text.len() as u64 {
            return Err(UNAVAILABLE.into());
        }
        return page(
            item.entry.label,
            &text.as_bytes()[offset as usize..],
            offset,
            text.len() as u64,
        );
    }
    let mut file = regular(&item.path)?;
    let total = file.metadata().map_err(|_| UNAVAILABLE)?.len();
    let offset = if offset == LATEST_OFFSET {
        total.saturating_sub(1) / PAGE_BYTES as u64 * PAGE_BYTES as u64
    } else {
        offset
    };
    if offset > total {
        return Err(UNAVAILABLE.into());
    }
    let mut header = [0; 35];
    let count = file.read(&mut header).map_err(|_| UNAVAILABLE)?;
    if !header[..count].starts_with(b"# AI agent game client session\n")
        && !header[..count].starts_with(b"# AI agent game client session\r\n")
    {
        return Err(UNAVAILABLE.into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| UNAVAILABLE)?;
    let mut body = Vec::new();
    file.take(PAGE_BYTES as u64 + 3)
        .read_to_end(&mut body)
        .map_err(|_| UNAVAILABLE)?;
    page(item.entry.label, &body, offset, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    struct Fixture {
        root: PathBuf,
        base: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().canonicalize().unwrap();
            let root = base.join(format!(
                "npc-native-history-{}",
                npc_framework::agent_io::registration::new_secret().unwrap()
            ));
            fs::create_dir(&root).unwrap();
            Self {
                root: root.canonicalize().unwrap(),
                base,
            }
        }
        fn private(&self) -> PathBuf {
            self.root.join(".ai-client")
        }
        fn file(&self, relative: &str, contents: &[u8]) -> PathBuf {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }
        fn index(&self, records: &[Value]) {
            self.file(
                ".ai-client/history-owner.json",
                &serde_json::to_vec(&json!({"version":1,"owner_id":A})).unwrap(),
            );
            let text = records
                .iter()
                .map(|record| format!("{record}\n"))
                .collect::<String>();
            self.file(".ai-client/history-index.jsonl", text.as_bytes());
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let root = self.root.canonicalize().unwrap();
            assert!(root.starts_with(&self.base) && root != self.base);
            fs::remove_dir_all(root).unwrap();
        }
    }
    fn metrics(id: &str) -> Value {
        json!({"version":2,"metrics_id":id,"provider":"codex","created_unix_seconds":1700000000,
            "turns_started":1,"turns_completed":0,"turns_failed":1,"turns_interrupted":0,
            "totals":{"input_tokens":null,"output_tokens":null,"cost_microusd":null},
            "selection_intent":{"requested_model":"gpt-fixture-default","requested_effort":"high"},
            "model":null,"effort":null,"accounting_coverage":"since_legacy_rebase","cost_basis":"unknown"})
    }
    fn descriptor(
        kind: &str,
        id: &str,
        path: &Path,
        created: u64,
        metrics_id: Option<&str>,
    ) -> Value {
        json!({"version":1,"owner_id":A,"kind":kind,"id":id,"path":path,"provider":"codex","created_unix_seconds":created,"metrics_id":metrics_id})
    }

    #[test]
    fn generated_utc_labels_cover_epoch_leap_century_and_bounded_future_dates() {
        for (timestamp, expected) in [
            (0, "Time unavailable"),
            (1, "1970-01-01 00:00 UTC"),
            (86_400, "1970-01-02 00:00 UTC"),
            (951_782_400, "2000-02-29 00:00 UTC"),
            (951_827_696, "2000-02-29 12:34 UTC"),
            (1_709_164_800, "2024-02-29 00:00 UTC"),
            (4_107_542_400, "2100-03-01 00:00 UTC"),
            (253_402_300_799, "9999-12-31 23:59 UTC"),
            (253_402_300_800, "Time unavailable"),
            (u64::MAX, "Time unavailable"),
        ] {
            assert_eq!(utc_label(timestamp), expected);
            assert!(utc_label(timestamp).is_ascii());
        }
    }

    #[test]
    fn external_paths_remain_discoverable_after_changes_without_scanning_other_agents() {
        let fixture = Fixture::new();
        let old_log_id = format!("1700000000-{A}");
        let new_log_id = format!("1700000100-{B}");
        let old_log = fixture.file(
            &format!("shared-one/{old_log_id}.md"),
            b"# AI agent game client session\n\nold gameplay\n",
        );
        let new_log = fixture.file(
            &format!("shared-two/{new_log_id}.md"),
            b"# AI agent game client session\n\nnew gameplay\n",
        );
        let old_metric = fixture.file(
            &format!("shared-one/{A}.json"),
            &serde_json::to_vec(&metrics(A)).unwrap(),
        );
        let new_metric = fixture.file(
            &format!("shared-two/{B}.json"),
            &serde_json::to_vec(&metrics(B)).unwrap(),
        );
        fixture.file(
            "shared-one/1700000200-cccccccccccccccccccccccccccccccc.md",
            b"# AI agent game client session\nOTHER_AGENT_PRIVATE",
        );
        fixture.file(
            ".ai-client/state.json",
            b"PRIVATE_PROVIDER_STATE_MUST_NOT_BE_READ",
        );
        fixture.index(&[
            descriptor("log", &old_log_id, &old_log, 1700000000, Some(A)),
            descriptor("metrics", A, &old_metric, 1700000000, None),
            descriptor("log", &new_log_id, &new_log, 1700000100, Some(B)),
            descriptor("metrics", B, &new_metric, 1700000100, None),
        ]);
        let before = fs::read(&old_log).unwrap();
        let history = list(&fixture.root).unwrap();
        assert_eq!(history.entries.len(), 4);
        assert!(history.entries[0].label.contains("2023-11-14 22:15 UTC"));
        let old = history
            .entries
            .iter()
            .find(|entry| entry.key == format!("log:{old_log_id}"))
            .unwrap();
        assert_eq!(
            old.metrics_key.as_deref(),
            Some(format!("metrics:{A}").as_str())
        );
        assert!(
            read(&fixture.root, &old.key, 0)
                .unwrap()
                .text
                .contains("old gameplay")
        );
        assert!(
            read(&fixture.root, old.metrics_key.as_deref().unwrap(), 0)
                .unwrap()
                .text
                .contains("Requested model: gpt-fixture-default")
        );
        assert!(!format!("{history:?}").contains("OTHER_AGENT_PRIVATE"));
        assert!(!format!("{history:?}").contains("shared-one"));
        assert_eq!(fs::read(&old_log).unwrap(), before);
        assert_eq!(
            fs::read(fixture.private().join("state.json")).unwrap(),
            b"PRIVATE_PROVIDER_STATE_MUST_NOT_BE_READ"
        );
    }

    #[test]
    fn pre_index_custom_metrics_are_visible_from_canonical_ledger_without_overriding_exports() {
        let fixture = Fixture::new();
        let mut canonical = metrics(A);
        canonical["totals"]["input_tokens"] = 100.into();
        canonical["cumulative_baseline"] =
            json!({"input_tokens":999999,"private":"PRIVATE_BASELINE_MUST_NOT_BE_RENDERED"});
        canonical["last_numeric_scope"] = "session_cumulative".into();
        canonical["cost_basis"] = "api_equivalent".into();
        canonical["mixed_cost_basis"] = true.into();
        let original = serde_json::to_vec(&canonical).unwrap();
        let ledger = fixture.file(&format!(".ai-client/metrics-ledger/{A}.json"), &original);
        let old_export = fixture.file(&format!("old custom metrics folder/{A}.json"), &original);
        fixture.file(
            ".ai-client/state.json",
            b"PRIVATE_SESSION_STATE_MUST_NOT_BE_READ",
        );
        let key = format!("metrics:{A}");
        let history = list(&fixture.root).unwrap();
        assert_eq!(history.entries.len(), 1);
        let page = read(&fixture.root, &key, 0).unwrap();
        assert!(page.text.contains("Accounted input tokens: 100"));
        assert!(page.text.contains("Mixed reported cost bases"));
        assert!(!page.text.contains("999999") && !page.text.contains("PRIVATE_BASELINE"));
        assert!(!page.text.contains("PRIVATE_SESSION_STATE"));
        let mut exported = metrics(A);
        exported["totals"]["input_tokens"] = 300.into();
        fixture.file(
            &format!(".ai-client/metrics/{A}.json"),
            &serde_json::to_vec(&exported).unwrap(),
        );
        assert_eq!(list(&fixture.root).unwrap().entries.len(), 1);
        assert!(
            read(&fixture.root, &key, 0)
                .unwrap()
                .text
                .contains("Accounted input tokens: 300")
        );
        exported["totals"]["input_tokens"] = 450.into();
        let current = fixture.file(
            &format!("current custom metrics folder/{A}.json"),
            &serde_json::to_vec(&exported).unwrap(),
        );
        fixture.index(&[descriptor("metrics", A, &current, 1700000100, None)]);
        assert_eq!(list(&fixture.root).unwrap().entries.len(), 1);
        assert!(
            read(&fixture.root, &key, 0)
                .unwrap()
                .text
                .contains("Accounted input tokens: 450")
        );
        assert_eq!(fs::read(ledger).unwrap(), original);
        assert_eq!(fs::read(old_export).unwrap(), original);
    }

    #[test]
    fn owned_windows_crlf_logs_are_readable_without_rewriting_files() {
        let f = Fixture::new();
        let name = format!("1793534400-{A}");
        let body = b"# AI agent game client session\r\n\r\nWindows saved log\r\n";
        let path = f.file(&format!(".ai-client/logs/{name}.md"), body);
        let page = read(&f.root, &format!("log:{name}"), 0).unwrap();
        assert!(page.text.contains("Windows saved log\n"));
        assert!(!page.text.contains('\r'));
        assert_eq!(fs::read(&path).unwrap(), body);
        fs::write(
            &path,
            b"# AI agent game client sessions\r\nNot an owned header\r\n",
        )
        .unwrap();
        assert!(read(&f.root, &format!("log:{name}"), 0).is_err());
    }
    #[test]
    fn log_pages_preserve_unicode_and_reach_latest_text_without_truncating_history() {
        let fixture = Fixture::new();
        let log_id = format!("1700000000-{A}");
        let source = format!(
            "# AI agent game client session\n\n{}LAST\n",
            "é🎲page text\n".repeat(PAGE_BYTES / 3)
        );
        let path = fixture.file(&format!(".ai-client/logs/{log_id}.md"), source.as_bytes());
        let key = format!("log:{log_id}");
        let latest = read(&fixture.root, &key, LATEST_OFFSET).unwrap();
        assert!(latest.text.ends_with("LAST\n"));
        assert!(latest.next_offset.is_none() && latest.previous_offset.is_some());
        assert_eq!(
            latest.offset,
            (source.len() as u64 - 1) / PAGE_BYTES as u64 * PAGE_BYTES as u64
        );
        let mut offset = 0;
        let mut all = String::new();
        loop {
            let page = read(&fixture.root, &key, offset).unwrap();
            assert!(page.text.len() <= PAGE_BYTES + 3);
            all.push_str(&page.text);
            let Some(next) = page.next_offset else {
                break;
            };
            offset = next;
        }
        assert_eq!(all, source);
        assert_eq!(fs::read(path).unwrap(), source.as_bytes());
        assert!(read(&fixture.root, &key, 1).is_err());
    }

    #[test]
    fn metrics_distinguish_actual_requested_unknown_cost_and_unverified_legacy_values() {
        let fixture = Fixture::new();
        let mut record = metrics(A);
        let path = fixture.file(
            &format!(".ai-client/metrics/{A}.json"),
            &serde_json::to_vec(&record).unwrap(),
        );
        let key = format!("metrics:{A}");
        let text = read(&fixture.root, &key, 0).unwrap().text;
        assert!(
            text.is_ascii(),
            "generated metrics copy uses the native font's supported ASCII"
        );
        for expected in [
            "Actual model: Unavailable",
            "Actual effort: Unavailable",
            "Requested model: gpt-fixture-default",
            "Requested effort: high",
            "input tokens: Unavailable",
            "output tokens: Unavailable",
            "Reported cost: Unavailable",
            "earlier figures are unverified",
            "CLI version: Unavailable",
            "Verified usage scope: Unknown",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        record["totals"] = json!({"input_tokens":100,"output_tokens":30,"cost_microusd":1234});
        record["model"] = "gpt-fixture-reported".into();
        record["effort"] = "low".into();
        record["cost_basis"] = "api_equivalent".into();
        record["provider_context"] = json!({"cli_version":"0.145.0","usage_scope":"session_cumulative","adapter_revision":1});
        record["history"] = json!([{"turn":1,"outcome":"failed","usage":{},"provider_context":{"cli_version":"0.999.0","usage_scope":null,"adapter_revision":1}}]);
        record["private_tool_args"] = json!({"token":"PRIVATE_TOKEN_MARKER"});
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let text = read(&fixture.root, &key, LATEST_OFFSET).unwrap().text;
        assert!(
            text.contains("Actual model: gpt-fixture-reported")
                && text.contains("Actual effort: low")
        );
        assert!(text.contains("$0.001234 USD") && text.contains("not a subscription bill"));
        assert!(
            text.contains("CLI version: 0.145.0")
                && text.contains("Verified usage scope: Session cumulative")
        );
        assert!(text.contains("CLI 0.999.0 | Scope Unknown | Adapter 1"));
        assert!(!text.contains("PRIVATE_TOKEN_MARKER"));
        record["version"] = 1.into();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let text = read(&fixture.root, &key, 0).unwrap().text;
        assert!(text.contains("Unverified legacy input tokens: 100"));
        assert!(!text.contains("Accounted input tokens: 100"));
        record["metrics_id"] = B.into();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(read(&fixture.root, &key, 0).is_err());
    }

    #[test]
    fn core_metrics_and_cost_basis_precede_selection_and_adapter_diagnostics() {
        for version in [1, 2] {
            for zero_cost in [false, true] {
                let mut record = metrics(A);
                record["version"] = version.into();
                record["totals"]["input_tokens"] = 1200.into();
                record["totals"]["output_tokens"] = 80.into();
                if zero_cost {
                    record["totals"]["cost_microusd"] = 0.into();
                    record["cost_basis"] = "api_equivalent".into();
                }
                record["provider_context"] = json!({"cli_version":"0.145.0","usage_scope":"session_cumulative","adapter_revision":1});
                let metrics: MetricsDocument = serde_json::from_value(record).unwrap();
                let text = metrics_text(&metrics);
                let prefix = if version == 1 {
                    "Unverified legacy"
                } else {
                    "Accounted"
                };
                let cost = if zero_cost {
                    "$0.000000 USD"
                } else {
                    "Unavailable"
                };
                let basis = if zero_cost {
                    "API-equivalent estimate, not a subscription bill"
                } else {
                    "Cost basis unavailable or unspecified"
                };
                let fields = [
                    format!("{prefix} input tokens: 1200"),
                    format!("{prefix} output tokens: 80"),
                    format!("Reported cost: {cost}"),
                    "Actual model: Unavailable".into(),
                    "Actual effort: Unavailable".into(),
                ];
                let positions: Vec<_> = fields
                    .iter()
                    .map(|field| text.find(field).unwrap())
                    .collect();
                assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
                let requested = text.find("Requested model:").unwrap();
                let cli = text.find("CLI version:").unwrap();
                assert!(
                    positions
                        .iter()
                        .all(|position| *position < requested && *position < cli)
                );
                assert!(text.contains(&format!("Reported cost: {cost}\n{basis}\nActual model:")));
                let core_end = positions[4] + fields[4].len();
                assert!(
                    text[..core_end].lines().count() <= 7,
                    "normal core summary has a compact initial viewport budget"
                );
                assert!(text.contains("Coverage: Partial:"));
                if version == 1 {
                    assert!(text.contains("legacy numbers are not certified totals"));
                }
            }
        }
    }

    #[test]
    fn malformed_ownership_paths_and_unknown_schema_never_open_arbitrary_files() {
        let fixture = Fixture::new();
        let arbitrary = fixture.file("private/auth.json", b"PRIVATE_AUTH_FILE");
        let mut records = vec![descriptor("metrics", A, &arbitrary, 1, None)];
        let mut wrong_owner = descriptor(
            "metrics",
            B,
            &fixture.root.join(format!("{B}.json")),
            2,
            None,
        );
        wrong_owner["owner_id"] = B.into();
        records.push(wrong_owner);
        let mut future = descriptor(
            "log",
            &format!("1700000000-{A}"),
            &fixture.root.join(format!("1700000000-{A}.md")),
            3,
            None,
        );
        future["version"] = 99.into();
        records.push(future);
        fixture.index(&records);
        let history = list(&fixture.root).unwrap();
        assert!(history.entries.is_empty());
        assert!(history.message.contains("omitted"));
        assert!(!history.message.contains("PRIVATE_AUTH_FILE"));
        assert!(read(&fixture.root, "metrics:../private/auth.json", 0).is_err());
        assert_eq!(fs::read(arbitrary).unwrap(), b"PRIVATE_AUTH_FILE");
    }

    #[test]
    fn empty_and_bounded_catalog_states_are_explicit_and_preserve_files() {
        let fixture = Fixture::new();
        assert!(
            list(&fixture.root)
                .unwrap()
                .message
                .contains("No AI session history")
        );
        let records: Vec<_> = (0..300u64)
            .map(|index| {
                let id = format!("{index:032x}");
                descriptor(
                    "metrics",
                    &id,
                    &fixture.root.join(format!("{id}.json")),
                    index,
                    None,
                )
            })
            .collect();
        fixture.index(&records);
        let before = fs::read(fixture.private().join("history-index.jsonl")).unwrap();
        let history = list(&fixture.root).unwrap();
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert_eq!(history.entries[0].key, format!("metrics:{:032x}", 299));
        assert!(history.entries[0].label.contains("1970-01-01 00:04 UTC"));
        assert!(history.message.contains("256 recent"));
        assert_eq!(
            fs::read(fixture.private().join("history-index.jsonl")).unwrap(),
            before
        );
    }

    #[test]
    fn recent_log_pressure_cannot_hide_the_metrics_tab() {
        let fixture = Fixture::new();
        let metric = fixture.file(
            &format!(".ai-client/metrics/{A}.json"),
            &serde_json::to_vec(&metrics(A)).unwrap(),
        );
        let mut records = vec![descriptor("metrics", A, &metric, 1, None)];
        for index in 0..300u64 {
            let log_id = format!("{}-{index:032x}", 1700000000 + index);
            records.push(descriptor(
                "log",
                &log_id,
                &fixture.root.join(format!("{log_id}.md")),
                1700000000 + index,
                None,
            ));
        }
        fixture.index(&records);
        let history = list(&fixture.root).unwrap();
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Metrics)
                .count(),
            1
        );
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Log)
                .count(),
            MAX_ENTRIES
        );
        assert!(history.entries.len() <= MAX_ENTRIES * 2);
        assert!(read(&fixture.root, &format!("metrics:{A}"), 0).is_ok());
    }
}
