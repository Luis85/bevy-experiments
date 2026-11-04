//! Private canonical accounting and atomic, transcript-free session exports.
use super::{Provider, codex_model::SelectionIntent, storage};
use npc_framework::agent_io::registration::new_secret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const HISTORY_LIMIT: usize = 64;
const LEDGER_VERSION: u32 = 2;
const ACCOUNTING_REVISION: u32 = 2;
const PROVIDER_ADAPTER_REVISION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 256 * 1024;
const IO_ERROR: &str = "Cannot persist AI session metrics; check --metrics-dir and private client folder permissions before restarting";
const ARCHIVE_ERROR: &str =
    "Cannot preserve the original AI metrics accounting record; session stopped before migration";

fn legacy_accounting_revision() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageScope {
    #[default]
    Turn,
    SessionCumulative,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasis {
    #[default]
    Unknown,
    ApiEquivalent,
    ActualBilled,
    ReportedUnspecified,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    #[default]
    Unknown,
    ProviderReported,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageSample {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub scope: UsageScope,
    pub cost_basis: CostBasis,
    pub provenance: Provenance,
}

impl UsageSample {
    fn has_numeric(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cached_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
            || self.cost_microusd.is_some()
    }
    fn zero_baseline() -> Self {
        Self {
            input_tokens: Some(0),
            output_tokens: Some(0),
            cached_input_tokens: Some(0),
            cache_creation_input_tokens: Some(0),
            cost_microusd: Some(0),
            ..Self::default()
        }
    }
    pub(crate) fn merge(&mut self, sample: &Self) {
        fn merge(a: &mut Option<u64>, b: Option<u64>) {
            if let Some(b) = b {
                *a = Some(a.map_or(b, |a| a.max(b)));
            }
        }
        merge(&mut self.input_tokens, sample.input_tokens);
        merge(&mut self.output_tokens, sample.output_tokens);
        merge(&mut self.cached_input_tokens, sample.cached_input_tokens);
        merge(
            &mut self.cache_creation_input_tokens,
            sample.cache_creation_input_tokens,
        );
        merge(&mut self.cost_microusd, sample.cost_microusd);
        if sample.model.as_deref().is_some_and(|s| valid_label(s, 128)) {
            self.model = sample.model.clone();
        }
        if sample.effort.as_deref().is_some_and(|s| valid_label(s, 32)) {
            self.effort = sample.effort.clone();
        }
        self.scope = sample.scope;
        if sample.cost_microusd.is_some() {
            self.cost_basis = sample.cost_basis;
        }
        if sample.provenance == Provenance::ProviderReported {
            self.provenance = sample.provenance;
        }
    }
    fn delta(&self, baseline: &Self) -> Self {
        fn subtract(current: Option<u64>, previous: Option<u64>) -> Option<u64> {
            current
                .zip(previous)
                .and_then(|(current, previous)| current.checked_sub(previous))
        }
        if self.scope == UsageScope::Turn {
            return self.clone();
        }
        Self {
            input_tokens: subtract(self.input_tokens, baseline.input_tokens),
            output_tokens: subtract(self.output_tokens, baseline.output_tokens),
            cached_input_tokens: subtract(self.cached_input_tokens, baseline.cached_input_tokens),
            cache_creation_input_tokens: subtract(
                self.cache_creation_input_tokens,
                baseline.cache_creation_input_tokens,
            ),
            cost_microusd: if baseline.cost_microusd == Some(0)
                || self.cost_basis == baseline.cost_basis
            {
                subtract(self.cost_microusd, baseline.cost_microusd)
            } else {
                None
            },
            ..self.clone()
        }
    }
}

fn valid_label(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !super::provider::uuid(value)
        && !(value.len() >= 32 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':' | b'+')
        })
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_selection(selection: &SelectionIntent) -> bool {
    super::provider::usage::model(&selection.requested_model).is_some()
        && selection
            .requested_effort
            .as_deref()
            .is_none_or(|effort| super::provider::usage::effort(effort).is_some())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Totals {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cost_microusd: Option<u64>,
    reported_input_turns: u64,
    reported_output_turns: u64,
    reported_cost_turns: u64,
    actual_billed_microusd: Option<u64>,
    api_equivalent_microusd: Option<u64>,
    unspecified_cost_microusd: Option<u64>,
}
fn add(total: &mut Option<u64>, value: Option<u64>) -> Result<(), String> {
    if let Some(value) = value {
        *total = Some(
            total
                .unwrap_or(0)
                .checked_add(value)
                .ok_or("AI metrics counter overflow; session stopped to preserve accounting")?,
        );
    }
    Ok(())
}
impl Totals {
    fn has_numeric(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cached_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
            || self.cost_microusd.is_some()
    }
    fn apply(&mut self, usage: &UsageSample) -> Result<(), String> {
        add(&mut self.input_tokens, usage.input_tokens)?;
        add(&mut self.output_tokens, usage.output_tokens)?;
        add(&mut self.cached_input_tokens, usage.cached_input_tokens)?;
        add(
            &mut self.cache_creation_input_tokens,
            usage.cache_creation_input_tokens,
        )?;
        add(&mut self.cost_microusd, usage.cost_microusd)?;
        match usage.cost_basis {
            CostBasis::ActualBilled => add(&mut self.actual_billed_microusd, usage.cost_microusd)?,
            CostBasis::ApiEquivalent => {
                add(&mut self.api_equivalent_microusd, usage.cost_microusd)?
            }
            CostBasis::Unknown | CostBasis::ReportedUnspecified => {
                add(&mut self.unspecified_cost_microusd, usage.cost_microusd)?
            }
        }
        self.reported_input_turns += u64::from(usage.input_tokens.is_some());
        self.reported_output_turns += u64::from(usage.output_tokens.is_some());
        self.reported_cost_turns += u64::from(usage.cost_microusd.is_some());
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Failed,
    Interrupted,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AccountingInterval {
    #[default]
    CurrentProcess,
    SincePreviousSessionReport,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TurnRecord {
    turn: u64,
    outcome: Option<TurnOutcome>,
    usage: UsageSample,
    #[serde(default)]
    reported_scope: UsageScope,
    #[serde(default)]
    accounting_interval: AccountingInterval,
    #[serde(default)]
    selection_intent: Option<SelectionIntent>,
    #[serde(default)]
    provider_context: Option<ProviderContext>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProviderContext {
    cli_version: Option<String>,
    usage_scope: Option<UsageScope>,
    adapter_revision: u32,
}
impl ProviderContext {
    fn valid(&self) -> bool {
        self.cli_version.as_deref().is_none_or(valid_version)
            && (self.cli_version.is_some() || self.usage_scope.is_none())
    }
}
fn valid_version(value: &str) -> bool {
    value.len() <= 32
        && value.split('.').count() == 3
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 10
                && (part.len() == 1 || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AccountingCoverage {
    #[default]
    LegacyUnspecified,
    ConversationStart,
    SinceAdoptionBaseline,
    SinceLegacyRebase,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyAccountingStatus {
    UnverifiedScope,
}
#[derive(Clone, Serialize, Deserialize)]
struct LegacyAccounting {
    status: LegacyAccountingStatus,
    totals: Totals,
    cost_basis: Option<CostBasis>,
    mixed_cost_basis: bool,
    history: Vec<TurnRecord>,
    active_turn: Option<TurnRecord>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Ledger {
    version: u32,
    #[serde(default = "legacy_accounting_revision")]
    accounting_revision: u32,
    #[serde(default)]
    accounting_coverage: AccountingCoverage,
    #[serde(default)]
    reported_session_usage: Option<UsageSample>,
    #[serde(default)]
    legacy_unverified: Option<LegacyAccounting>,
    metrics_id: String,
    provider: Provider,
    #[serde(default)]
    agent_label: Option<String>,
    #[serde(default)]
    world_label: Option<String>,
    turns_started: u64,
    turns_completed: u64,
    turns_failed: u64,
    turns_interrupted: u64,
    created_unix_seconds: u64,
    updated_unix_seconds: u64,
    totals: Totals,
    model: Option<String>,
    effort: Option<String>,
    metadata_source: Provenance,
    cost_basis: Option<CostBasis>,
    mixed_cost_basis: bool,
    history: Vec<TurnRecord>,
    active_turn: Option<TurnRecord>,
    cumulative_baseline: UsageSample,
    #[serde(default)]
    last_numeric_scope: Option<UsageScope>,
    #[serde(default)]
    selection_intent: Option<SelectionIntent>,
    #[serde(default)]
    provider_context: Option<ProviderContext>,
}
fn ledger_turns(ledger: &Ledger) -> impl Iterator<Item = &TurnRecord> {
    ledger
        .history
        .iter()
        .chain(ledger.active_turn.iter())
        .chain(
            ledger
                .legacy_unverified
                .iter()
                .flat_map(|legacy| legacy.history.iter().chain(legacy.active_turn.iter())),
        )
}

pub struct Metrics {
    canonical: PathBuf,
    export: PathBuf,
    ledger: Ledger,
}

fn bounded_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| IO_ERROR)?;
    let metadata = file.metadata().map_err(|_| IO_ERROR)?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(IO_ERROR.into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| IO_ERROR)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(IO_ERROR.into());
    }
    Ok(bytes)
}

fn preserve_preimage(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return if bounded_bytes(path).is_ok_and(|existing| existing == bytes) {
                Ok(())
            } else {
                Err(ARCHIVE_ERROR.into())
            };
        }
        Err(_) => return Err(ARCHIVE_ERROR.into()),
    };
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        // Only this call's newly created archive file is removed on failure.
        let _ = fs::remove_file(path);
        return Err(ARCHIVE_ERROR.into());
    }
    Ok(())
}

fn archive_accounting(
    private: &Path,
    id: &str,
    canonical_bytes: &[u8],
    export: &Path,
) -> Result<(), String> {
    let archive = private.join("metrics-ledger-archives").join(id);
    fs::create_dir_all(&archive).map_err(|_| ARCHIVE_ERROR)?;
    preserve_preimage(&archive.join("canonical-v1.json"), canonical_bytes)?;
    match fs::symlink_metadata(export) {
        Ok(_) => {
            let bytes = bounded_bytes(export).map_err(|_| ARCHIVE_ERROR)?;
            preserve_preimage(&archive.join("export-v1.json"), &bytes)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ARCHIVE_ERROR.into()),
    }
    Ok(())
}

impl Metrics {
    pub fn directory(private: &Path, configured: Option<&Path>) -> Result<PathBuf, String> {
        let configured = configured
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("NPC_AI_METRICS_DIR").map(PathBuf::from))
            .unwrap_or_else(|| private.join("metrics"));
        if configured.as_os_str().is_empty() {
            return Err("Metrics directory must not be empty".into());
        }
        fs::create_dir_all(&configured).map_err(|_| IO_ERROR)?;
        configured.canonicalize().map_err(|_| IO_ERROR.into())
    }
    pub fn open(
        private: &Path,
        directory: &Path,
        provider: Provider,
        id: Option<&str>,
        fresh_conversation: bool,
    ) -> Result<Self, String> {
        let existing_id = id.is_some();
        let id =
            match id {
                Some(id) if valid_id(id) => id.to_owned(),
                Some(_) => return Err(
                    "Invalid private metrics identity; preserve state and repair the client folder"
                        .into(),
                ),
                None => new_secret().map_err(|_| "Cannot allocate private metrics identity")?[..32]
                    .to_owned(),
            };
        let base = private.join("metrics-ledger");
        fs::create_dir_all(&base).map_err(|_| IO_ERROR)?;
        let base = base.canonicalize().map_err(|_| IO_ERROR)?;
        if directory.canonicalize().map_err(|_| IO_ERROR)? == base {
            return Err(
                "Metrics export folder must differ from the private metrics-ledger folder".into(),
            );
        }
        let canonical = base.join(format!("{id}.json"));
        let export = directory.join(format!("{id}.json"));
        let ledger = if canonical.is_file() {
            // Decode the same bounded bytes that a migration preserves exactly.
            let bytes = bounded_bytes(&canonical)?;
            let mut ledger: Ledger =
                serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                    .map_err(|_| IO_ERROR)?;
            if !(1..=LEDGER_VERSION).contains(&ledger.version)
                || !(1..=ACCOUNTING_REVISION).contains(&ledger.accounting_revision)
                || (ledger.version == LEDGER_VERSION
                    && ledger.accounting_revision != ACCOUNTING_REVISION)
                || ledger.metrics_id != id
                || ledger.provider != provider
            {
                return Err("Private metrics ledger does not match this AI session".into());
            }
            if ledger
                .selection_intent
                .iter()
                .chain(ledger_turns(&ledger).filter_map(|turn| turn.selection_intent.as_ref()))
                .any(|selection| !valid_selection(selection))
            {
                return Err("Private metrics selection metadata is invalid".into());
            }
            if ledger
                .provider_context
                .iter()
                .chain(ledger_turns(&ledger).filter_map(|turn| turn.provider_context.as_ref()))
                .any(|context| !context.valid())
            {
                return Err("Private metrics provider metadata is invalid".into());
            }
            if ledger.version == 1 {
                if provider == Provider::Codex {
                    // V1 did not record a trustworthy Codex usage scope or
                    // fresh/adopted origin. Even complete history cannot prove
                    // which earlier sums are valid. Preserve originals before
                    // changing ANY accounting, without resetting the AI session.
                    archive_accounting(private, &id, &bytes, &export)?;
                    ledger.legacy_unverified = Some(LegacyAccounting {
                        status: LegacyAccountingStatus::UnverifiedScope,
                        totals: std::mem::take(&mut ledger.totals),
                        cost_basis: ledger.cost_basis.take(),
                        mixed_cost_basis: ledger.mixed_cost_basis,
                        history: std::mem::take(&mut ledger.history),
                        active_turn: ledger.active_turn.clone(),
                    });
                    if let Some(active) = &mut ledger.active_turn {
                        active.usage = UsageSample::default();
                    }
                    ledger.cumulative_baseline = UsageSample::default();
                    ledger.last_numeric_scope = None;
                    ledger.reported_session_usage = None;
                    ledger.mixed_cost_basis = false;
                    ledger.accounting_coverage = AccountingCoverage::SinceLegacyRebase;
                }
                // Claude's V1 totals, baseline and history retain their existing
                // accounting semantics. Its unrecorded coverage is not guessed.
                ledger.version = LEDGER_VERSION;
                ledger.accounting_revision = ACCOUNTING_REVISION;
            } else if provider == Provider::Codex
                && ledger.last_numeric_scope.is_none()
                && ledger.totals.has_numeric()
            {
                // Compatibility for older accounting records without a scope
                // marker: retain totals while rebasing an uncertain next delta.
                ledger.cumulative_baseline = UsageSample::default();
            }
            ledger
        } else if existing_id {
            return Err("Private session metrics ledger is missing; restore it before resuming tracked turns".into());
        } else {
            Ledger {
                version: LEDGER_VERSION,
                accounting_revision: ACCOUNTING_REVISION,
                accounting_coverage: if fresh_conversation {
                    AccountingCoverage::ConversationStart
                } else {
                    AccountingCoverage::SinceAdoptionBaseline
                },
                reported_session_usage: None,
                legacy_unverified: None,
                metrics_id: id.clone(),
                provider,
                agent_label: None,
                world_label: None,
                turns_started: 0,
                turns_completed: 0,
                turns_failed: 0,
                turns_interrupted: 0,
                created_unix_seconds: storage::now(),
                updated_unix_seconds: storage::now(),
                totals: Totals::default(),
                model: None,
                effort: None,
                metadata_source: Provenance::Unknown,
                cost_basis: None,
                mixed_cost_basis: false,
                history: Vec::new(),
                active_turn: None,
                last_numeric_scope: None,
                selection_intent: None,
                provider_context: None,
                cumulative_baseline: if fresh_conversation {
                    UsageSample::zero_baseline()
                } else {
                    UsageSample::default()
                },
            }
        };
        let mut metrics = Self {
            canonical,
            export,
            ledger,
        };
        if metrics.ledger.active_turn.is_some() {
            metrics.finish(TurnOutcome::Interrupted)?;
        }
        metrics.persist()?;
        Ok(metrics)
    }
    pub fn id(&self) -> &str {
        &self.ledger.metrics_id
    }
    pub fn set_selection(&mut self, selection: Option<&SelectionIntent>) -> Result<(), String> {
        if selection.is_some_and(|selection| !valid_selection(selection)) {
            return Err("AI selection metadata is invalid; metrics were not changed".into());
        }
        self.ledger.selection_intent = selection.cloned();
        self.persist()
    }
    pub fn set_provider_context(
        &mut self,
        version: Option<&str>,
        scope: Option<UsageScope>,
    ) -> Result<(), String> {
        if version.is_some_and(|version| !valid_version(version))
            || (version.is_none() && scope.is_some())
        {
            return Err("Invalid provider adapter metadata; metrics were not changed".into());
        }
        self.ledger.provider_context = Some(ProviderContext {
            cli_version: version.map(str::to_owned),
            usage_scope: scope,
            adapter_revision: PROVIDER_ADAPTER_REVISION,
        });
        self.persist()
    }
    pub fn set_labels(&mut self, agent: Option<&str>, world: Option<&str>) -> Result<(), String> {
        fn label(value: Option<&str>) -> Option<String> {
            value
                .filter(|s| {
                    !s.trim().is_empty()
                        && s.chars().count() <= 200
                        && !s.chars().any(char::is_control)
                })
                .map(str::to_owned)
        }
        self.ledger.agent_label = label(agent);
        self.ledger.world_label = label(world);
        self.persist()
    }
    pub fn has_started(&self) -> bool {
        self.ledger.turns_started > 0
    }
    pub fn begin(&mut self) -> Result<(), String> {
        if self.ledger.active_turn.is_some() {
            return Err("Previous metrics turn is still active".into());
        }
        self.ledger.turns_started = self
            .ledger
            .turns_started
            .checked_add(1)
            .ok_or("AI metrics turn limit exceeded")?;
        self.ledger.active_turn = Some(TurnRecord {
            turn: self.ledger.turns_started,
            outcome: None,
            usage: UsageSample::default(),
            reported_scope: UsageScope::Turn,
            accounting_interval: AccountingInterval::CurrentProcess,
            selection_intent: self.ledger.selection_intent.clone(),
            provider_context: self.ledger.provider_context.clone(),
        });
        self.persist()
    }
    pub fn record(&mut self, usage: &UsageSample) -> Result<(), String> {
        let Some(active) = self.ledger.active_turn.as_mut() else {
            return Ok(());
        };
        let previous = active.usage.clone();
        active.usage.merge(usage);
        if previous != active.usage {
            if active.usage.scope == UsageScope::SessionCumulative && active.usage.has_numeric() {
                self.ledger.reported_session_usage = Some(active.usage.clone());
            }
            self.persist()?;
        }
        Ok(())
    }
    pub fn finish(&mut self, outcome: TurnOutcome) -> Result<(), String> {
        let Some(mut active) = self.ledger.active_turn.clone() else {
            return Ok(());
        };
        let mut usage = active.usage.delta(&self.ledger.cumulative_baseline);
        let mut next = self.ledger.clone();
        next.totals.apply(&usage)?;
        if active.usage.scope == UsageScope::SessionCumulative && active.usage.has_numeric() {
            next.reported_session_usage = Some(active.usage.clone());
            // A crash result can contain zeroed or regressing counters for the
            // SAME provider session. Never lower its accounted high-water mark.
            // New provider conversations have separate ledgers and baselines.
            let mut baseline = active.usage.clone();
            macro_rules! retain_missing {
                ($field:ident) => {
                    baseline.$field = match (baseline.$field, next.cumulative_baseline.$field) {
                        (Some(current), Some(previous)) => Some(current.max(previous)),
                        (current, previous) => current.or(previous),
                    };
                };
            }
            retain_missing!(input_tokens);
            retain_missing!(output_tokens);
            retain_missing!(cached_input_tokens);
            retain_missing!(cache_creation_input_tokens);
            retain_missing!(cost_microusd);
            next.cumulative_baseline = baseline;
            next.last_numeric_scope = Some(UsageScope::SessionCumulative);
        } else if active.usage.has_numeric() {
            // A later cumulative report includes these already counted per-call
            // reports. Its first report must rebase with an unknown delta;
            // totals alone cannot reconstruct missing historical reports.
            next.cumulative_baseline = UsageSample::default();
            next.last_numeric_scope = Some(UsageScope::Turn);
        }
        next.model = usage.model.clone();
        next.effort = usage.effort.clone();
        next.metadata_source = if next.model.is_some() || next.effort.is_some() {
            Provenance::ProviderReported
        } else {
            Provenance::Unknown
        };
        if usage.cost_microusd.is_some() {
            if next.cost_basis.is_some_and(|old| old != usage.cost_basis) {
                next.mixed_cost_basis = true;
            }
            next.cost_basis = Some(usage.cost_basis);
        }
        match outcome {
            TurnOutcome::Completed => next.turns_completed += 1,
            TurnOutcome::Failed => next.turns_failed += 1,
            TurnOutcome::Interrupted => next.turns_interrupted += 1,
        }
        active.reported_scope = active.usage.scope;
        active.accounting_interval = if active.usage.scope == UsageScope::SessionCumulative {
            AccountingInterval::SincePreviousSessionReport
        } else {
            AccountingInterval::CurrentProcess
        };
        usage.scope = UsageScope::Turn;
        active.usage = usage;
        active.outcome = Some(outcome);
        next.history.push(active);
        if next.history.len() > HISTORY_LIMIT {
            next.history.remove(0);
        }
        next.active_turn = None;
        self.ledger = next;
        self.persist()
    }
    fn public(&self) -> Result<Value, String> {
        let mut value = serde_json::to_value(&self.ledger).map_err(|_| IO_ERROR)?;
        value
            .as_object_mut()
            .expect("ledger object")
            .remove("cumulative_baseline");
        value
            .as_object_mut()
            .expect("ledger object")
            .remove("last_numeric_scope");
        value["cost_available"] = Value::Bool(self.ledger.totals.cost_microusd.is_some());
        value["cost_basis"] = Value::String(if self.ledger.mixed_cost_basis {
            "mixed".into()
        } else {
            serde_json::to_value(self.ledger.cost_basis.unwrap_or_default())
                .map_err(|_| IO_ERROR)?
                .as_str()
                .unwrap_or("unknown")
                .into()
        });
        value["model_source"] = serde_json::to_value(if self.ledger.model.is_some() {
            Provenance::ProviderReported
        } else {
            Provenance::Unknown
        })
        .map_err(|_| IO_ERROR)?;
        value["effort_source"] = serde_json::to_value(if self.ledger.effort.is_some() {
            Provenance::ProviderReported
        } else {
            Provenance::Unknown
        })
        .map_err(|_| IO_ERROR)?;
        Ok(value)
    }
    fn persist(&mut self) -> Result<(), String> {
        self.ledger.updated_unix_seconds = storage::now();
        storage::write_json(&self.canonical, &self.ledger).map_err(|_| IO_ERROR)?;
        storage::write_json(&self.export, &self.public()?).map_err(|_| IO_ERROR.into())
    }
}
impl Drop for Metrics {
    fn drop(&mut self) {
        if self.ledger.active_turn.is_some() {
            let _ = self.finish(TurnOutcome::Interrupted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        base: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().canonicalize().unwrap();
            let root = base.join(format!(
                "npc-client-metrics-{}-{}",
                std::process::id(),
                new_secret().unwrap()
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
        fn export(&self, name: &str) -> PathBuf {
            let path = self.root.join(name);
            fs::create_dir_all(&path).unwrap();
            path
        }
        fn open(&self, fresh: bool) -> Metrics {
            Metrics::open(
                &self.private(),
                &self.export("export metrics"),
                Provider::Codex,
                None,
                fresh,
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let root = self.root.canonicalize().unwrap();
            assert!(root.starts_with(&self.base) && root != self.base);
            let _ = fs::remove_dir_all(root);
        }
    }
    fn usage(input: u64) -> UsageSample {
        UsageSample {
            input_tokens: Some(input),
            output_tokens: Some(30),
            cached_input_tokens: Some(20),
            model: Some("gpt-fixture".into()),
            effort: Some("high".into()),
            provenance: Provenance::ProviderReported,
            ..Default::default()
        }
    }
    fn value(metrics: &Metrics) -> Value {
        storage::read_json(&metrics.export).unwrap()
    }

    #[test]
    fn exact_resume_and_export_folder_change_preserve_totals_without_double_counting() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let id = metrics.id().to_owned();
        metrics.begin().unwrap();
        metrics.record(&usage(100)).unwrap();
        metrics.record(&usage(100)).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 100);
        assert_eq!(value(&metrics)["turns_completed"], 1);
        drop(metrics);
        let mut resumed = Metrics::open(
            &fixture.private(),
            &fixture.export("changed export"),
            Provider::Codex,
            Some(&id),
            false,
        )
        .unwrap();
        assert_eq!(value(&resumed)["totals"]["input_tokens"], 100);
        resumed.begin().unwrap();
        let mut changed = usage(50);
        changed.model = Some("gpt-fixture-next".into());
        changed.effort = Some("medium".into());
        resumed.record(&changed).unwrap();
        resumed.finish(TurnOutcome::Failed).unwrap();
        let json = value(&resumed);
        assert_eq!(json["totals"]["input_tokens"], 150);
        assert_eq!(json["turns_failed"], 1);
        assert_eq!(json["history"].as_array().unwrap().len(), 2);
        assert_eq!(json["history"][0]["usage"]["model"], "gpt-fixture");
        assert_eq!(json["model"], "gpt-fixture-next");
        assert_eq!(json["effort"], "medium");
        assert!(!json["cost_available"].as_bool().unwrap());
        assert!(json["totals"]["cost_microusd"].is_null());
    }

    #[test]
    fn cumulative_provider_reports_are_differenced_across_restarts() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let id = metrics.id().to_owned();
        for index in 1..=2 {
            let mut sample = usage(100 * index);
            sample.scope = UsageScope::SessionCumulative;
            sample.cost_microusd = Some(1000 * index);
            sample.cost_basis = CostBasis::ApiEquivalent;
            metrics.begin().unwrap();
            metrics.record(&sample).unwrap();
            metrics.finish(TurnOutcome::Completed).unwrap();
        }
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 200);
        assert_eq!(value(&metrics)["totals"]["cost_microusd"], 2000);
        drop(metrics);
        let mut metrics = Metrics::open(
            &fixture.private(),
            &fixture.export("export metrics"),
            Provider::Codex,
            Some(&id),
            false,
        )
        .unwrap();
        let mut sample = usage(300);
        sample.scope = UsageScope::SessionCumulative;
        sample.cost_microusd = Some(3000);
        sample.cost_basis = CostBasis::ApiEquivalent;
        metrics.begin().unwrap();
        metrics.record(&sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        let json = value(&metrics);
        assert_eq!(json["totals"]["input_tokens"], 300);
        assert_eq!(json["totals"]["cost_microusd"], 3000);
        assert_eq!(json["totals"]["api_equivalent_microusd"], 3000);
        assert!(json["totals"]["actual_billed_microusd"].is_null());
        assert_eq!(json["cost_basis"], "api_equivalent");
        assert_eq!(json["history"][2]["usage"]["input_tokens"], 100);
        assert_eq!(json["history"][2]["reported_scope"], "session_cumulative");
        assert_eq!(json["history"][2]["usage"]["scope"], "turn");
    }

    fn priced_usage(input: u64, scope: UsageScope) -> UsageSample {
        UsageSample {
            scope,
            cost_microusd: Some(input * 10),
            cost_basis: CostBasis::ApiEquivalent,
            ..usage(input)
        }
    }
    fn complete(metrics: &mut Metrics, sample: &UsageSample) {
        metrics.begin().unwrap();
        metrics.record(sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
    }
    fn reopen(fixture: &Fixture, id: &str) -> Metrics {
        Metrics::open(
            &fixture.private(),
            &fixture.export("export metrics"),
            Provider::Codex,
            Some(id),
            false,
        )
        .unwrap()
    }

    struct LegacyFixture {
        id: String,
        canonical: PathBuf,
        export: PathBuf,
        canonical_bytes: Vec<u8>,
        export_bytes: Vec<u8>,
        public: Value,
        active: bool,
    }
    fn legacy_fixture(mut metrics: Metrics) -> LegacyFixture {
        let mut canonical = serde_json::to_value(&metrics.ledger).unwrap();
        let mut public = metrics.public().unwrap();
        for record in [&mut canonical, &mut public] {
            record["version"] = 1.into();
            for field in [
                "accounting_revision",
                "accounting_coverage",
                "reported_session_usage",
                "legacy_unverified",
            ] {
                record.as_object_mut().unwrap().remove(field);
            }
        }
        let canonical_bytes = [
            b"\xef\xbb\xbf".as_slice(),
            &serde_json::to_vec_pretty(&canonical).unwrap(),
        ]
        .concat();
        let export_bytes = [
            b"\n".as_slice(),
            &serde_json::to_vec_pretty(&public).unwrap(),
        ]
        .concat();
        fs::write(&metrics.canonical, &canonical_bytes).unwrap();
        fs::write(&metrics.export, &export_bytes).unwrap();
        let fixture = LegacyFixture {
            id: metrics.id().to_owned(),
            canonical: metrics.canonical.clone(),
            export: metrics.export.clone(),
            canonical_bytes,
            export_bytes,
            public,
            active: metrics.ledger.active_turn.is_some(),
        };
        // The synthetic V1 active attempt models an interrupted old process;
        // this fixture's destructor must not finish it with the new writer.
        metrics.ledger.active_turn = None;
        fixture
    }
    fn assert_archived(fixture: &Fixture, old: &LegacyFixture) {
        let archive = fixture
            .private()
            .join("metrics-ledger-archives")
            .join(&old.id);
        assert_eq!(
            fs::read(archive.join("canonical-v1.json")).unwrap(),
            old.canonical_bytes
        );
        assert_eq!(
            fs::read(archive.join("export-v1.json")).unwrap(),
            old.export_bytes
        );
    }

    #[test]
    fn recorded_codex_cumulative_snapshots_have_exact_session_totals_and_resumed_delta() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let mut sample = UsageSample {
            input_tokens: Some(400_537),
            output_tokens: Some(1_614),
            cached_input_tokens: Some(349_312),
            scope: UsageScope::SessionCumulative,
            provenance: Provenance::ProviderReported,
            ..Default::default()
        };
        complete(&mut metrics, &sample);
        let id = metrics.id().to_owned();
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        sample.input_tokens = Some(1_133_389);
        sample.output_tokens = Some(3_039);
        sample.cached_input_tokens = Some(1_039_232);
        complete(&mut metrics, &sample);
        let json = value(&metrics);
        assert_eq!(json["version"], LEDGER_VERSION);
        assert_eq!(json["accounting_revision"], ACCOUNTING_REVISION);
        assert_eq!(json["accounting_coverage"], "conversation_start");
        assert_eq!(json["totals"]["input_tokens"], 1_133_389);
        assert_eq!(json["totals"]["output_tokens"], 3_039);
        assert_eq!(json["totals"]["cached_input_tokens"], 1_039_232);
        assert_eq!(json["history"][1]["usage"]["input_tokens"], 732_852);
        assert_eq!(json["history"][1]["usage"]["output_tokens"], 1_425);
        assert_eq!(json["history"][1]["usage"]["cached_input_tokens"], 689_920);
        assert_eq!(json["history"][1]["reported_scope"], "session_cumulative");
        assert_eq!(json["reported_session_usage"]["input_tokens"], 1_133_389);
        assert_eq!(json["reported_session_usage"]["output_tokens"], 3_039);
        assert_eq!(
            json["reported_session_usage"]["cached_input_tokens"],
            1_039_232
        );
        assert_eq!(
            json["reported_session_usage"]["scope"],
            "session_cumulative"
        );
        assert_eq!(json["cost_available"], false);
        assert!(json["totals"]["cost_microusd"].is_null());
        assert!(json["reported_session_usage"]["cost_microusd"].is_null());
        assert!(json["model"].is_null() && json["effort"].is_null());
        assert_eq!(json["metadata_source"], "unknown");
    }

    #[test]
    fn v1_codex_complete_pruned_and_interrupted_records_are_preserved_then_rebased_once() {
        for mode in ["complete", "pruned", "interrupted"] {
            let fixture = Fixture::new();
            let mut metrics = fixture.open(true);
            let turns = if mode == "pruned" {
                HISTORY_LIMIT + 3
            } else {
                2
            };
            for turn in 1..=turns {
                if mode == "interrupted" && turn == 2 {
                    metrics.begin().unwrap();
                    metrics.finish(TurnOutcome::Failed).unwrap();
                } else {
                    complete(
                        &mut metrics,
                        &priced_usage(turn as u64 * 100, UsageScope::Turn),
                    );
                }
            }
            if mode == "interrupted" {
                metrics.begin().unwrap();
                metrics.record(&usage(999)).unwrap();
            }
            let old = legacy_fixture(metrics);
            let mut metrics = reopen(&fixture, &old.id);
            assert_archived(&fixture, &old);
            let migrated = value(&metrics);
            assert_eq!(metrics.id(), old.id);
            assert_eq!(migrated["version"], LEDGER_VERSION);
            assert_eq!(migrated["accounting_revision"], ACCOUNTING_REVISION);
            assert_eq!(migrated["accounting_coverage"], "since_legacy_rebase");
            assert_eq!(migrated["turns_started"], old.public["turns_started"]);
            assert_eq!(migrated["turns_completed"], old.public["turns_completed"]);
            assert_eq!(migrated["turns_failed"], old.public["turns_failed"]);
            assert_eq!(
                migrated["turns_interrupted"].as_u64().unwrap(),
                old.public["turns_interrupted"].as_u64().unwrap() + u64::from(old.active)
            );
            assert_eq!(migrated["legacy_unverified"]["status"], "unverified_scope");
            assert_eq!(
                migrated["legacy_unverified"]["totals"],
                old.public["totals"]
            );
            assert_eq!(
                migrated["legacy_unverified"]["history"],
                old.public["history"]
            );
            assert_eq!(
                migrated["legacy_unverified"]["active_turn"],
                old.public["active_turn"]
            );
            assert!(migrated["totals"]["input_tokens"].is_null());
            assert!(migrated["totals"]["cost_microusd"].is_null());
            assert!(migrated["reported_session_usage"].is_null());
            assert_eq!(
                migrated["history"].as_array().unwrap().len(),
                usize::from(old.active)
            );
            drop(metrics);
            metrics = reopen(&fixture, &old.id);
            assert_archived(&fixture, &old);
            assert_eq!(
                value(&metrics)["legacy_unverified"],
                migrated["legacy_unverified"]
            );
            assert_eq!(
                value(&metrics)["turns_interrupted"],
                migrated["turns_interrupted"]
            );
            complete(
                &mut metrics,
                &priced_usage(10_000, UsageScope::SessionCumulative),
            );
            assert!(value(&metrics)["totals"]["input_tokens"].is_null());
            assert_eq!(
                value(&metrics)["reported_session_usage"]["input_tokens"],
                10_000
            );
            let boundary = value(&metrics)["history"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .clone();
            assert!(boundary["usage"]["input_tokens"].is_null());
            drop(metrics);
            let mut metrics = reopen(&fixture, &old.id);
            complete(
                &mut metrics,
                &priced_usage(10_100, UsageScope::SessionCumulative),
            );
            assert_eq!(value(&metrics)["totals"]["input_tokens"], 100);
            assert_eq!(value(&metrics)["totals"]["cost_microusd"], 1000);
            assert_eq!(
                value(&metrics)["accounting_coverage"],
                "since_legacy_rebase"
            );
            assert_eq!(
                value(&metrics)["legacy_unverified"],
                migrated["legacy_unverified"]
            );
            assert_archived(&fixture, &old);
        }
    }

    #[test]
    fn migration_archive_failure_never_rewrites_original_records_and_can_retry_idempotently() {
        for obstruction in ["archive_root", "canonical_preimage", "export_preimage"] {
            let fixture = Fixture::new();
            let mut metrics = fixture.open(true);
            complete(&mut metrics, &usage(100));
            let old = legacy_fixture(metrics);
            let archive_root = fixture.private().join("metrics-ledger-archives");
            let archive = archive_root.join(&old.id);
            if obstruction == "archive_root" {
                fs::write(&archive_root, b"owned archive obstruction").unwrap();
            } else {
                fs::create_dir_all(&archive).unwrap();
                if obstruction == "canonical_preimage" {
                    fs::write(
                        archive.join("canonical-v1.json"),
                        b"owned different preimage",
                    )
                    .unwrap();
                } else {
                    fs::create_dir(archive.join("export-v1.json")).unwrap();
                }
            }
            let error = Metrics::open(
                &fixture.private(),
                &fixture.export("export metrics"),
                Provider::Codex,
                Some(&old.id),
                false,
            )
            .err()
            .expect("archive failure must stop opening before any tracked turn");
            assert_eq!(error, ARCHIVE_ERROR);
            assert_eq!(fs::read(&old.canonical).unwrap(), old.canonical_bytes);
            assert_eq!(fs::read(&old.export).unwrap(), old.export_bytes);
            if obstruction == "export_preimage" {
                assert_eq!(
                    fs::read(archive.join("canonical-v1.json")).unwrap(),
                    old.canonical_bytes
                );
                fs::remove_dir(archive.join("export-v1.json")).unwrap();
                let metrics = reopen(&fixture, &old.id);
                assert_eq!(
                    value(&metrics)["accounting_coverage"],
                    "since_legacy_rebase"
                );
                assert_archived(&fixture, &old);
            }
        }
    }

    #[test]
    fn v1_claude_totals_baseline_and_history_remain_unchanged_on_repeated_open() {
        let fixture = Fixture::new();
        let mut metrics = Metrics::open(
            &fixture.private(),
            &fixture.export("export metrics"),
            Provider::Claude,
            None,
            true,
        )
        .unwrap();
        complete(
            &mut metrics,
            &priced_usage(100, UsageScope::SessionCumulative),
        );
        complete(
            &mut metrics,
            &priced_usage(200, UsageScope::SessionCumulative),
        );
        let baseline = serde_json::to_value(&metrics.ledger.cumulative_baseline).unwrap();
        let old = legacy_fixture(metrics);
        let mut canonical: Value = storage::read_json(&old.canonical).unwrap();
        canonical
            .as_object_mut()
            .unwrap()
            .remove("last_numeric_scope");
        storage::write_json(&old.canonical, &canonical).unwrap();
        for _ in 0..2 {
            let metrics = Metrics::open(
                &fixture.private(),
                &fixture.export("export metrics"),
                Provider::Claude,
                Some(&old.id),
                false,
            )
            .unwrap();
            assert_eq!(value(&metrics)["version"], LEDGER_VERSION);
            assert_eq!(value(&metrics)["totals"], old.public["totals"]);
            assert_eq!(value(&metrics)["history"], old.public["history"]);
            assert_eq!(
                serde_json::to_value(&metrics.ledger.cumulative_baseline).unwrap(),
                baseline
            );
            assert!(value(&metrics)["legacy_unverified"].is_null());
            assert!(!fixture.private().join("metrics-ledger-archives").exists());
        }
    }

    #[test]
    fn version_two_reopens_same_identity_and_unsupported_formats_never_rewrite_state() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(&mut metrics, &usage(37));
        let id = metrics.id().to_owned();
        let canonical = metrics.canonical.clone();
        let export = metrics.export.clone();
        assert_eq!(value(&metrics)["version"], LEDGER_VERSION);
        drop(metrics);
        let metrics = reopen(&fixture, &id);
        assert_eq!(metrics.id(), id);
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 37);
        let good: Value = storage::read_json(&canonical).unwrap();
        let public_before = fs::read(&export).unwrap();
        drop(metrics);
        let state = fixture.private().join("state.json");
        fs::write(&state, b"owned unchanged AI identity state").unwrap();
        for (version, revision) in [(0, 2), (3, 2), (2, 1), (2, 3)] {
            let mut unsupported = good.clone();
            unsupported["version"] = version.into();
            unsupported["accounting_revision"] = revision.into();
            let before = serde_json::to_vec_pretty(&unsupported).unwrap();
            fs::write(&canonical, &before).unwrap();
            assert!(
                Metrics::open(
                    &fixture.private(),
                    &fixture.export("export metrics"),
                    Provider::Codex,
                    Some(&id),
                    false
                )
                .is_err()
            );
            assert_eq!(fs::read(&canonical).unwrap(), before);
            assert_eq!(fs::read(&export).unwrap(), public_before);
            assert_eq!(
                fs::read(&state).unwrap(),
                b"owned unchanged AI identity state"
            );
        }
    }

    #[test]
    fn migrated_public_metrics_exclude_unknown_private_fields_and_keep_unknown_actuals() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(
            &mut metrics,
            &UsageSample {
                input_tokens: Some(100),
                ..Default::default()
            },
        );
        let old = legacy_fixture(metrics);
        let mut canonical: Value = storage::read_json(&old.canonical).unwrap();
        canonical["provider_session"] =
            Value::String("11111111-1111-4111-8111-111111111111".into());
        canonical["transcript"] = Value::String("private-transcript-marker".into());
        canonical["history"][0]["tool_args"] = serde_json::json!({"token":"private-token-marker"});
        storage::write_json(&old.canonical, &canonical).unwrap();
        let mut metrics = reopen(&fixture, &old.id);
        complete(
            &mut metrics,
            &UsageSample {
                input_tokens: Some(200),
                scope: UsageScope::SessionCumulative,
                provenance: Provenance::ProviderReported,
                ..Default::default()
            },
        );
        let public = value(&metrics);
        let text = public.to_string();
        for secret in [
            "11111111-1111-4111-8111-111111111111",
            "private-transcript-marker",
            "private-token-marker",
            "metrics-ledger-archives",
            "canonical-v1.json",
        ] {
            assert!(!text.contains(secret));
        }
        assert!(public.get("cumulative_baseline").is_none());
        assert!(public.get("last_numeric_scope").is_none());
        assert!(public["totals"]["input_tokens"].is_null());
        assert_eq!(public["reported_session_usage"]["input_tokens"], 200);
        assert!(public["model"].is_null() && public["effort"].is_null());
        assert_eq!(public["metadata_source"], "unknown");
        assert_eq!(public["cost_available"], false);
    }

    #[test]
    fn per_call_to_cumulative_upgrade_rebases_without_double_counting_across_restart() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(&mut metrics, &priced_usage(100, UsageScope::Turn));
        let id = metrics.id().to_owned();
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        complete(
            &mut metrics,
            &priced_usage(200, UsageScope::SessionCumulative),
        );
        let boundary = value(&metrics);
        assert_eq!(boundary["totals"]["input_tokens"], 100);
        assert_eq!(boundary["totals"]["cost_microusd"], 1000);
        assert!(boundary["history"][1]["usage"]["input_tokens"].is_null());
        assert!(boundary["history"][1]["usage"]["cost_microusd"].is_null());
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        complete(
            &mut metrics,
            &priced_usage(250, UsageScope::SessionCumulative),
        );
        let json = value(&metrics);
        assert_eq!(json["totals"]["input_tokens"], 150);
        assert_eq!(json["totals"]["cost_microusd"], 1500);
        assert_eq!(json["totals"]["reported_input_turns"], 2);
        assert_eq!(json["totals"]["reported_cost_turns"], 2);
        assert_eq!(json["history"][2]["usage"]["input_tokens"], 50);
        assert!(json.get("last_numeric_scope").is_none());
    }

    #[test]
    fn metadata_only_reports_preserve_baselines_but_numeric_per_call_reports_invalidate_them() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(
            &mut metrics,
            &priced_usage(100, UsageScope::SessionCumulative),
        );
        complete(
            &mut metrics,
            &UsageSample {
                model: Some("gpt-fixture-next".into()),
                ..Default::default()
            },
        );
        complete(&mut metrics, &UsageSample::default());
        complete(
            &mut metrics,
            &priced_usage(150, UsageScope::SessionCumulative),
        );
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 150);
        complete(&mut metrics, &priced_usage(20, UsageScope::Turn));
        let id = metrics.id().to_owned();
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        complete(
            &mut metrics,
            &priced_usage(200, UsageScope::SessionCumulative),
        );
        complete(
            &mut metrics,
            &priced_usage(230, UsageScope::SessionCumulative),
        );
        let json = value(&metrics);
        assert_eq!(json["totals"]["input_tokens"], 200);
        assert_eq!(json["totals"]["cost_microusd"], 2000);
        assert!(json["history"][5]["usage"]["input_tokens"].is_null());
        assert_eq!(json["history"][6]["usage"]["input_tokens"], 30);
    }

    #[test]
    fn revision_two_without_numeric_scope_preserves_totals_and_rebases_once() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(&mut metrics, &priced_usage(100, UsageScope::Turn));
        let id = metrics.id().to_owned();
        let canonical = metrics.canonical.clone();
        drop(metrics);
        let mut legacy: Value = storage::read_json(&canonical).unwrap();
        legacy.as_object_mut().unwrap().remove("last_numeric_scope");
        // The older implementation retained this zero baseline after a per-call report.
        legacy["cumulative_baseline"] = serde_json::to_value(UsageSample::zero_baseline()).unwrap();
        storage::write_json(&canonical, &legacy).unwrap();
        let mut metrics = reopen(&fixture, &id);
        complete(
            &mut metrics,
            &priced_usage(200, UsageScope::SessionCumulative),
        );
        assert!(value(&metrics)["history"][1]["usage"]["input_tokens"].is_null());
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        complete(
            &mut metrics,
            &priced_usage(250, UsageScope::SessionCumulative),
        );
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 150);
        assert_eq!(value(&metrics)["totals"]["cost_microusd"], 1500);
    }

    #[test]
    fn adopting_an_existing_session_does_not_invent_pre_client_usage() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(false);
        let id = metrics.id().to_owned();
        let mut sample = usage(1000);
        sample.scope = UsageScope::SessionCumulative;
        metrics.begin().unwrap();
        metrics.record(&sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        assert!(value(&metrics)["totals"]["input_tokens"].is_null());
        assert_eq!(
            value(&metrics)["accounting_coverage"],
            "since_adoption_baseline"
        );
        assert_eq!(
            value(&metrics)["reported_session_usage"]["input_tokens"],
            1000
        );
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        sample.input_tokens = Some(1100);
        metrics.begin().unwrap();
        metrics.record(&sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 100);
        assert_eq!(
            value(&metrics)["reported_session_usage"]["input_tokens"],
            1100
        );
        assert_eq!(
            value(&metrics)["accounting_coverage"],
            "since_adoption_baseline"
        );
    }

    #[test]
    fn zeroed_failed_cumulative_report_never_lowers_the_accounted_baseline() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        for (input, cost, outcome) in [
            (100, 1000, TurnOutcome::Completed),
            (0, 0, TurnOutcome::Failed),
            (200, 2000, TurnOutcome::Completed),
        ] {
            let sample = UsageSample {
                input_tokens: Some(input),
                cost_microusd: Some(cost),
                scope: UsageScope::SessionCumulative,
                cost_basis: CostBasis::ApiEquivalent,
                provenance: Provenance::ProviderReported,
                ..Default::default()
            };
            metrics.begin().unwrap();
            metrics.record(&sample).unwrap();
            metrics.finish(outcome).unwrap();
        }
        let json = value(&metrics);
        assert_eq!(json["totals"]["input_tokens"], 200);
        assert_eq!(json["totals"]["cost_microusd"], 2000);
        assert!(json["history"][1]["usage"]["input_tokens"].is_null());
        assert!(json["history"][1]["usage"]["cost_microusd"].is_null());
        assert_eq!(json["history"][2]["usage"]["input_tokens"], 100);
        assert_eq!(
            json["history"][2]["accounting_interval"],
            "since_previous_session_report"
        );
    }

    #[test]
    fn missing_usage_zero_and_failed_or_interrupted_turns_remain_distinct() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        assert!(value(&metrics)["totals"]["input_tokens"].is_null());
        metrics.begin().unwrap();
        metrics
            .record(&UsageSample {
                input_tokens: Some(0),
                ..Default::default()
            })
            .unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 0);
        assert_eq!(value(&metrics)["totals"]["reported_input_turns"], 1);
        metrics.begin().unwrap();
        metrics.record(&usage(17)).unwrap();
        let id = metrics.id().to_owned();
        std::mem::forget(metrics);
        let metrics = Metrics::open(
            &fixture.private(),
            &fixture.export("export metrics"),
            Provider::Codex,
            Some(&id),
            false,
        )
        .unwrap();
        let json = value(&metrics);
        assert_eq!(json["totals"]["input_tokens"], 17);
        assert_eq!(json["turns_failed"], 2);
        assert_eq!(json["turns_interrupted"], 1);
        assert!(json["active_turn"].is_null());
    }

    #[test]
    fn bounded_history_keeps_lossless_totals_and_new_sessions_are_independent() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        for _ in 0..HISTORY_LIMIT + 3 {
            metrics.begin().unwrap();
            metrics.record(&usage(1)).unwrap();
            metrics.finish(TurnOutcome::Completed).unwrap();
        }
        let json = value(&metrics);
        assert_eq!(json["history"].as_array().unwrap().len(), HISTORY_LIMIT);
        assert_eq!(json["totals"]["input_tokens"], (HISTORY_LIMIT + 3) as u64);
        let new = fixture.open(true);
        assert_ne!(new.id(), metrics.id());
        assert_eq!(value(&new)["turns_started"], 0);
    }

    #[test]
    fn exports_never_include_private_session_or_payload_fields() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let sample:UsageSample=serde_json::from_value(serde_json::json!({"input_tokens":5,"model":"11111111-1111-4111-8111-111111111111","effort":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","token":"private-token-marker","transcript":"private-provider-marker","tool_args":{"secret":"private-tool-marker"}})).unwrap();
        metrics.begin().unwrap();
        metrics.record(&sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        let text = value(&metrics).to_string();
        for secret in [
            "11111111-1111-4111-8111-111111111111",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "private-token-marker",
            "private-provider-marker",
            "private-tool-marker",
        ] {
            assert!(!text.contains(secret));
        }
        assert!(value(&metrics)["model"].is_null());
        assert!(value(&metrics)["effort"].is_null());
        assert!(valid_id(metrics.id()));
    }

    fn selection(model: &str, effort: Option<&str>) -> SelectionIntent {
        SelectionIntent {
            requested_model: model.into(),
            requested_effort: effort.map(str::to_owned),
            model_source: super::super::codex_model::SelectionSource::AdvertisedDefault,
            effort_source: super::super::codex_model::SelectionSource::CliConfig,
        }
    }

    #[test]
    fn selection_intent_persists_and_attempt_history_keeps_actual_metadata_separate() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let first = selection("gpt-fixture-requested", Some("medium"));
        metrics.set_selection(Some(&first)).unwrap();
        metrics.begin().unwrap();
        // Changing future intent must not rewrite an already started attempt.
        let next = selection("gpt-fixture-next", None);
        metrics.set_selection(Some(&next)).unwrap();
        metrics.record(&usage(100)).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        let json = value(&metrics);
        assert_eq!(json["model"], "gpt-fixture");
        assert_eq!(json["effort"], "high");
        assert_eq!(json["model_source"], "provider_reported");
        assert_eq!(
            json["history"][0]["selection_intent"]["requested_model"],
            "gpt-fixture-requested"
        );
        assert_eq!(
            json["history"][0]["selection_intent"]["requested_effort"],
            "medium"
        );
        assert_eq!(
            json["history"][0]["selection_intent"]["model_source"],
            "advertised_default"
        );
        assert_eq!(
            json["history"][0]["selection_intent"]["effort_source"],
            "cli_config"
        );
        let id = metrics.id().to_owned();
        drop(metrics);
        let mut metrics = Metrics::open(
            &fixture.private(),
            &fixture.export("changed export"),
            Provider::Codex,
            Some(&id),
            false,
        )
        .unwrap();
        assert_eq!(
            value(&metrics)["selection_intent"]["requested_model"],
            "gpt-fixture-next"
        );
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        let json = value(&metrics);
        assert_eq!(json["turns_started"], 2);
        assert_eq!(json["turns_failed"], 1);
        assert_eq!(
            json["history"][1]["selection_intent"]["requested_model"],
            "gpt-fixture-next"
        );
        assert!(json["history"][1]["usage"]["model"].is_null());
        assert!(json["history"][1]["usage"]["effort"].is_null());
        assert!(json["history"][1]["usage"]["input_tokens"].is_null());
        assert!(json["history"][1]["usage"]["cost_microusd"].is_null());
        assert!(json["model"].is_null() && json["effort"].is_null());
        assert_eq!(json["model_source"], "unknown");
        assert_eq!(json["effort_source"], "unknown");
        assert_eq!(json["history"][0]["usage"]["model"], "gpt-fixture");
        metrics.set_selection(None).unwrap();
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        assert!(value(&metrics)["history"][2]["selection_intent"].is_null());
        assert_eq!(
            value(&metrics)["history"][1]["selection_intent"]["requested_model"],
            "gpt-fixture-next"
        );
    }

    #[test]
    fn provider_context_snapshots_version_and_unknown_scope_without_changing_accounting() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        metrics
            .set_provider_context(Some("0.145.0"), Some(UsageScope::SessionCumulative))
            .unwrap();
        metrics.begin().unwrap();
        metrics.set_provider_context(Some("0.999.0"), None).unwrap();
        let mut sample = usage(100);
        sample.scope = UsageScope::SessionCumulative;
        metrics.record(&sample).unwrap();
        metrics.finish(TurnOutcome::Completed).unwrap();
        let first = value(&metrics);
        assert_eq!(
            first["history"][0]["provider_context"]["cli_version"],
            "0.145.0"
        );
        assert_eq!(
            first["history"][0]["provider_context"]["usage_scope"],
            "session_cumulative"
        );
        assert_eq!(
            first["history"][0]["provider_context"]["adapter_revision"],
            PROVIDER_ADAPTER_REVISION
        );
        let id = metrics.id().to_owned();
        drop(metrics);
        let mut metrics = reopen(&fixture, &id);
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        let json = value(&metrics);
        assert_eq!(
            json["history"][1]["provider_context"]["cli_version"],
            "0.999.0"
        );
        assert!(json["history"][1]["provider_context"]["usage_scope"].is_null());
        assert_eq!(json["totals"]["input_tokens"], 100);
        assert!(json["history"][1]["usage"]["input_tokens"].is_null());
        assert!(json["history"][1]["usage"]["cost_microusd"].is_null());
        assert!(json["model"].is_null() && json["effort"].is_null());
        metrics.set_provider_context(None, None).unwrap();
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        assert!(value(&metrics)["history"][2]["provider_context"]["cli_version"].is_null());
    }

    #[test]
    fn provider_context_rejects_secret_metadata_and_existing_v2_without_context_remains_readable() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        complete(&mut metrics, &usage(42));
        let before = fs::read(&metrics.export).unwrap();
        for version in [
            "private-token-marker",
            "0.145.0\nprivate",
            "C:/private/version",
            "000.145.0",
        ] {
            let error = metrics
                .set_provider_context(Some(version), None)
                .unwrap_err();
            assert!(!error.contains("private"));
            assert_eq!(fs::read(&metrics.export).unwrap(), before);
        }
        assert!(
            metrics
                .set_provider_context(None, Some(UsageScope::Turn))
                .is_err()
        );
        let id = metrics.id().to_owned();
        let canonical = metrics.canonical.clone();
        drop(metrics);
        let mut old: Value = storage::read_json(&canonical).unwrap();
        old.as_object_mut().unwrap().remove("provider_context");
        old["history"][0]
            .as_object_mut()
            .unwrap()
            .remove("provider_context");
        storage::write_json(&canonical, &old).unwrap();
        let metrics = reopen(&fixture, &id);
        assert_eq!(value(&metrics)["totals"]["input_tokens"], 42);
        assert!(value(&metrics)["provider_context"].is_null());
        assert!(value(&metrics)["history"][0]["provider_context"].is_null());
    }

    #[test]
    fn failed_first_attempt_keeps_selection_intent_without_claiming_execution_metadata() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        metrics
            .set_selection(Some(&selection("gpt-fixture-default", Some("high"))))
            .unwrap();
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        let json = value(&metrics);
        assert_eq!(json["turns_started"], 1);
        assert_eq!(json["turns_failed"], 1);
        assert_eq!(json["turns_completed"], 0);
        assert_eq!(json["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            json["history"][0]["selection_intent"]["requested_model"],
            "gpt-fixture-default"
        );
        assert_eq!(
            json["history"][0]["selection_intent"]["requested_effort"],
            "high"
        );
        for field in ["model", "effort"] {
            assert!(json[field].is_null());
            assert!(json["history"][0]["usage"][field].is_null());
        }
        for field in ["input_tokens", "output_tokens", "cost_microusd"] {
            assert!(json["totals"][field].is_null());
            assert!(json["history"][0]["usage"][field].is_null());
        }
        assert_eq!(json["metadata_source"], "unknown");
        assert_eq!(json["model_source"], "unknown");
        assert_eq!(json["effort_source"], "unknown");
        assert_eq!(json["cost_available"], false);
    }

    #[test]
    fn invalid_selection_metadata_is_rejected_without_writes_or_secret_echoes() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        let original_export = fs::read(&metrics.export).unwrap();
        let original_canonical = fs::read(&metrics.canonical).unwrap();
        for invalid in [
            selection("11111111-1111-4111-8111-111111111111", None),
            selection("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None),
            selection("gpt-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None),
            selection("C:/private/provider-config", None),
            selection("model = private-config-marker", None),
            selection("gpt-fixture", Some("high private-effort-marker")),
        ] {
            let error = metrics.set_selection(Some(&invalid)).unwrap_err();
            assert!(!error.contains(&invalid.requested_model));
            assert!(!error.contains("private-effort-marker"));
            assert_eq!(fs::read(&metrics.export).unwrap(), original_export);
            assert_eq!(fs::read(&metrics.canonical).unwrap(), original_canonical);
        }
        let mut forged: Value = storage::read_json(&metrics.canonical).unwrap();
        forged["selection_intent"] =
            serde_json::to_value(selection("private-config-marker", None)).unwrap();
        storage::write_json(&metrics.canonical, &forged).unwrap();
        let error = Metrics::open(
            &fixture.private(),
            &fixture.export("changed export"),
            Provider::Codex,
            Some(metrics.id()),
            false,
        )
        .err()
        .expect("invalid persisted selection must not be exported");
        assert!(!error.contains("private-config-marker"));
        assert!(
            !fixture
                .export("changed export")
                .join(format!("{}.json", metrics.id()))
                .exists()
        );
        assert_eq!(fs::read(&metrics.export).unwrap(), original_export);
    }

    #[test]
    fn legacy_metrics_without_selection_keep_failed_actuals_unknown_without_retrofilling() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        metrics.begin().unwrap();
        metrics.finish(TurnOutcome::Failed).unwrap();
        let id = metrics.id().to_owned();
        let canonical = metrics.canonical.clone();
        drop(metrics);
        let mut legacy: Value = storage::read_json(&canonical).unwrap();
        legacy.as_object_mut().unwrap().remove("selection_intent");
        for turn in legacy["history"].as_array_mut().unwrap() {
            turn.as_object_mut().unwrap().remove("selection_intent");
        }
        storage::write_json(&canonical, &legacy).unwrap();
        let mut metrics = reopen(&fixture, &id);
        metrics
            .set_selection(Some(&selection("gpt-fixture-default", Some("high"))))
            .unwrap();
        let json = value(&metrics);
        assert_eq!(json["turns_started"], 1);
        assert_eq!(json["turns_failed"], 1);
        assert!(json["history"][0]["selection_intent"].is_null());
        assert!(json["history"][0]["usage"]["model"].is_null());
        assert!(json["history"][0]["usage"]["effort"].is_null());
        assert!(json["model"].is_null() && json["effort"].is_null());
        assert!(json["totals"]["input_tokens"].is_null());
        assert!(json["totals"]["output_tokens"].is_null());
        assert!(json["totals"]["cost_microusd"].is_null());
        assert_eq!(json["metadata_source"], "unknown");
    }

    #[test]
    fn public_operator_labels_are_bounded_without_exporting_identity_or_paths() {
        let fixture = Fixture::new();
        let mut metrics = fixture.open(true);
        metrics.set_labels(Some("Herp"), Some("Island")).unwrap();
        assert_eq!(value(&metrics)["agent_label"], "Herp");
        assert_eq!(value(&metrics)["world_label"], "Island");
        metrics
            .set_labels(Some("private\ncontrol-marker"), Some(&"x".repeat(201)))
            .unwrap();
        assert!(value(&metrics)["agent_label"].is_null());
        assert!(value(&metrics)["world_label"].is_null());
        assert!(!value(&metrics).to_string().contains("control-marker"));
    }

    #[test]
    fn export_write_failure_preserves_canonical_ledger_and_blocks_more_tracking() {
        let fixture = Fixture::new();
        let export = fixture.export("blocked export");
        let mut metrics =
            Metrics::open(&fixture.private(), &export, Provider::Codex, None, true).unwrap();
        let id = metrics.id().to_owned();
        let path = metrics.export.clone();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(metrics.begin().is_err());
        let canonical: Value = storage::read_json(&metrics.canonical).unwrap();
        assert_eq!(canonical["turns_started"], 1);
        assert!(canonical["active_turn"].is_object());
        drop(metrics);
        assert!(
            Metrics::open(
                &fixture.private(),
                &fixture.export("restored export"),
                Provider::Codex,
                Some(&id),
                false
            )
            .is_ok()
        );
        assert!(
            Metrics::open(
                &fixture.private(),
                &fixture.export("restored export"),
                Provider::Codex,
                Some("../private-token-marker"),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn explicit_destination_paths_accept_spaces_and_reject_files_without_replacing_them() {
        let fixture = Fixture::new();
        let folder = fixture.root.join("operator metrics folder");
        assert_eq!(
            Metrics::directory(&fixture.private(), Some(&folder)).unwrap(),
            folder.canonicalize().unwrap()
        );
        let file = fixture.root.join("owned-file");
        fs::write(&file, b"keep").unwrap();
        assert!(Metrics::directory(&fixture.private(), Some(&file)).is_err());
        assert_eq!(fs::read(file).unwrap(), b"keep");
        let canonical_folder = fixture.private().join("metrics-ledger");
        fs::create_dir_all(&canonical_folder).unwrap();
        assert!(
            Metrics::open(
                &fixture.private(),
                &canonical_folder,
                Provider::Codex,
                None,
                true
            )
            .is_err()
        );
        assert_eq!(
            fs::read_dir(canonical_folder).unwrap().count(),
            0,
            "an export must never overwrite its own canonical accounting state"
        );
    }
}
