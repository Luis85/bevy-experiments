//! Allowlisted provider usage and startup metadata; never retains agent/tool text.
use super::super::{
    Provider,
    metrics::{CostBasis, Provenance, UsageSample, UsageScope},
};
use serde_json::Value;

pub fn version(provider: Provider, output: &[u8]) -> Option<String> {
    let output = std::str::from_utf8(output).ok()?.trim();
    if output.len() > 128 || output.contains(['\n', '\r']) {
        return None;
    }
    let version = match provider {
        Provider::Codex => output.strip_prefix("codex-cli ")?,
        Provider::Claude => output.strip_suffix(" (Claude Code)")?,
    };
    version_tuple(version)?;
    Some(version.into())
}
fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    let values = version
        .split('.')
        .map(|part| {
            if part.is_empty()
                || part.len() > 10
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            part.parse::<u32>().ok()
        })
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 3 {
        return None;
    }
    Some((values[0], values[1], values[2]))
}
pub fn scope(provider: Provider, version: Option<&str>) -> Option<UsageScope> {
    let version = version_tuple(version?)?;
    match provider {
        // Pinned 0.145.0 exec emits usage from last_total_token_usage, including
        // earlier turns after exact resume. Other versions need their own proof.
        Provider::Codex if version == (0, 145, 0) => Some(UsageScope::SessionCumulative),
        Provider::Codex => None,
        Provider::Claude if version >= (2, 1, 277) => Some(UsageScope::SessionCumulative),
        Provider::Claude => Some(UsageScope::Turn),
    }
}

pub fn model(value: &str) -> Option<String> {
    let prefix = ["gpt-", "claude-", "codex-", "o1-", "o3-", "o4-"]
        .iter()
        .any(|prefix| value.starts_with(prefix) && value.len() > prefix.len())
        || ["o1", "o3", "o4"].contains(&value);
    let mut hex_run = 0;
    if !prefix
        || value.len() > 128
        || value.is_empty()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return None;
    }
    for byte in value.bytes() {
        hex_run = if byte.is_ascii_hexdigit() {
            hex_run + 1
        } else {
            0
        };
        if hex_run >= 24 {
            return None;
        }
    }
    Some(value.into())
}
pub fn effort(value: &str) -> Option<String> {
    matches!(
        value,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
    .then(|| value.into())
}

pub fn merge(target: &mut UsageSample, sample: &UsageSample) {
    fn counter(target: &mut Option<u64>, sample: Option<u64>) {
        if let Some(value) = sample {
            *target = Some(target.map_or(value, |old| old.max(value)));
        }
    }
    counter(&mut target.input_tokens, sample.input_tokens);
    counter(&mut target.output_tokens, sample.output_tokens);
    counter(&mut target.cached_input_tokens, sample.cached_input_tokens);
    counter(
        &mut target.cache_creation_input_tokens,
        sample.cache_creation_input_tokens,
    );
    counter(&mut target.cost_microusd, sample.cost_microusd);
    if sample.model.is_some() {
        target.model = sample.model.clone();
    }
    if sample.effort.is_some() {
        target.effort = sample.effort.clone();
    }
    if sample.input_tokens.is_some()
        || sample.output_tokens.is_some()
        || sample.cached_input_tokens.is_some()
        || sample.cache_creation_input_tokens.is_some()
        || sample.cost_microusd.is_some()
    {
        target.scope = sample.scope;
    }
    if sample.cost_microusd.is_some() {
        target.cost_basis = sample.cost_basis;
    }
    if sample.provenance == Provenance::ProviderReported {
        target.provenance = sample.provenance;
    }
}

fn micro_usd(value: &Value) -> Option<u64> {
    // Round the provider's decimal estimate to micro-USD without binary floats or price inference.
    let number = value.as_number()?.to_string();
    if number.starts_with('-') || number.len() > 64 {
        return None;
    }
    let (mantissa, exponent) = number
        .split_once(['e', 'E'])
        .map_or(Some((number.as_str(), 0)), |(m, e)| {
            Some((m, e.parse::<i32>().ok()?))
        })?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let coefficient = format!("{whole}{fraction}").parse::<u128>().ok()?;
    let scale = 6i32
        .checked_add(exponent)?
        .checked_sub(i32::try_from(fraction.len()).ok()?)?;
    let rounded = if scale >= 0 {
        coefficient.checked_mul(10u128.checked_pow(scale.try_into().ok()?)?)?
    } else {
        let divisor = 10u128.checked_pow(scale.checked_neg()?.try_into().ok()?)?;
        coefficient.checked_add(divisor / 2)? / divisor
    };
    u64::try_from(rounded).ok()
}

fn observed(sample: &UsageSample) -> bool {
    sample.input_tokens.is_some()
        || sample.output_tokens.is_some()
        || sample.cached_input_tokens.is_some()
        || sample.cache_creation_input_tokens.is_some()
        || sample.cost_microusd.is_some()
        || sample.model.is_some()
        || sample.effort.is_some()
}

pub fn event(
    provider: Provider,
    value: &Value,
    target: &mut UsageSample,
    scope: Option<UsageScope>,
) {
    let mut sample = UsageSample::default();
    match provider {
        Provider::Codex
            if matches!(
                value["type"].as_str(),
                Some("turn.completed" | "turn.failed")
            ) =>
        {
            let Some(scope) = scope else {
                return;
            };
            sample.scope = scope;
            sample.input_tokens = value["usage"]["input_tokens"].as_u64();
            sample.output_tokens = value["usage"]["output_tokens"].as_u64();
            sample.cached_input_tokens = value["usage"]["cached_input_tokens"].as_u64();
            // Codex reports no subscription charge or price estimate in the exec usage schema.
        }
        Provider::Claude if value["type"] == "system" && value["subtype"] == "init" => {
            sample.model = value["model"].as_str().and_then(model);
            sample.effort = value["effort"].as_str().and_then(effort);
        }
        Provider::Claude if value["type"] == "result" => {
            sample.scope = scope.unwrap_or_default();
            sample.cost_microusd = scope.and_then(|_| micro_usd(&value["total_cost_usd"]));
            if sample.cost_microusd.is_some() {
                sample.cost_basis = CostBasis::ApiEquivalent;
            }
            if let Some(models) = value["modelUsage"]
                .as_object()
                .filter(|models| !models.is_empty() && models.len() <= 16)
            {
                if scope.is_some() {
                    let sum = |field: &str| {
                        models.values().try_fold(0u64, |total, usage| {
                            total.checked_add(usage[field].as_u64()?)
                        })
                    };
                    let uncached = sum("inputTokens");
                    sample.cached_input_tokens = sum("cacheReadInputTokens");
                    sample.cache_creation_input_tokens = sum("cacheCreationInputTokens");
                    sample.output_tokens = sum("outputTokens");
                    sample.input_tokens = uncached
                        .zip(sample.cached_input_tokens)
                        .zip(sample.cache_creation_input_tokens)
                        .and_then(|((uncached, cached), creation)| {
                            uncached.checked_add(cached)?.checked_add(creation)
                        });
                }
                if models.len() == 1 && target.model.is_none() {
                    sample.model = models.keys().next().and_then(|name| model(name));
                }
            }
            // result.usage is main-loop scoped; do not mix it with whole-tree modelUsage/cost.
            // Their per-call/cumulative scope is selected from the CLI version at preflight.
        }
        _ => return,
    }
    if observed(&sample) {
        sample.provenance = Provenance::ProviderReported;
        merge(target, &sample);
    }
}

/// Recognize only the initial Codex startup header, never later prompt/tool output.
#[derive(Default)]
pub struct StartupMetadata {
    state: u8,
    lines: u8,
    sample: UsageSample,
}
impl StartupMetadata {
    pub fn line(&mut self, line: &str) -> Option<UsageSample> {
        let line = line.trim_end_matches('\r');
        match self.state {
            0 => {
                if line.is_empty() {
                    return None;
                }
                self.state = if line.starts_with("OpenAI Codex v") && line.len() <= 80 {
                    1
                } else {
                    3
                };
            }
            1 => self.state = if line == "--------" { 2 } else { 3 },
            2 => {
                self.lines = self.lines.saturating_add(1);
                if self.lines > 24 {
                    self.state = 3;
                    return None;
                }
                if line == "--------" {
                    self.state = 3;
                    if observed(&self.sample) {
                        self.sample.provenance = Provenance::ProviderReported;
                        return Some(self.sample.clone());
                    }
                } else if let Some(value) = line.strip_prefix("model: ") {
                    self.sample.model = model(value);
                } else if let Some(value) = line.strip_prefix("reasoning effort: ") {
                    self.sample.effort = effort(value);
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn codex_usage_is_deduplicated_and_missing_cost_is_unknown() {
        let event = json!({"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30},"model":"private-untrusted-value"});
        let mut sample = UsageSample::default();
        let scope = scope(Provider::Codex, Some("0.145.0"));
        super::event(Provider::Codex, &event, &mut sample, scope);
        super::event(Provider::Codex, &event, &mut sample, scope);
        assert_eq!(sample.input_tokens, Some(100));
        assert_eq!(sample.output_tokens, Some(30));
        assert_eq!(sample.cached_input_tokens, Some(20));
        assert_eq!(sample.cost_microusd, None);
        assert_eq!(sample.model, None);
        assert_eq!(sample.scope, UsageScope::SessionCumulative);
    }
    #[test]
    fn claude_cumulative_failure_usage_keeps_cache_and_estimate_semantics() {
        let mut sample = UsageSample::default();
        super::event(
            Provider::Claude,
            &json!({"type":"result","is_error":true,"total_cost_usd":0.001,"usage":{"input_tokens":9999},"modelUsage":{"claude-sonnet-4-6":{"inputTokens":100,"cacheReadInputTokens":20,"cacheCreationInputTokens":5,"outputTokens":30}}}),
            &mut sample,
            Some(UsageScope::SessionCumulative),
        );
        assert_eq!(sample.input_tokens, Some(125));
        assert_eq!(sample.output_tokens, Some(30));
        assert_eq!(sample.cost_microusd, Some(1000));
        assert_eq!(sample.scope, UsageScope::SessionCumulative);
        assert_eq!(sample.cost_basis, CostBasis::ApiEquivalent);
        assert_eq!(micro_usd(&json!(0.0000005)), Some(1));
        assert_eq!(micro_usd(&json!(-1)), None);
    }
    #[test]
    fn startup_metadata_ignores_late_or_malformed_model_lookalikes() {
        let mut startup = StartupMetadata::default();
        let mut found = None;
        for line in [
            "OpenAI Codex v0.145.0",
            "--------",
            "model: gpt-6.1-sol",
            "reasoning effort: high",
            "--------",
        ] {
            found = startup.line(line).or(found);
        }
        let sample = found.unwrap();
        assert_eq!(sample.model.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(sample.effort.as_deref(), Some("high"));
        assert!(startup.line("model: gpt-private-token").is_none());
        assert!(model("11111111-1111-4111-8111-111111111111").is_none());
        assert!(effort("high private-token").is_none());
        let mut late = StartupMetadata::default();
        assert!(late.line("user payload").is_none());
        assert!(late.line("OpenAI Codex v0.145.0").is_none());
        assert!(late.line("--------").is_none());
        assert!(late.line("model: gpt-private-token").is_none());
    }
    #[test]
    fn cli_version_selects_documented_usage_scope_without_assuming_unknown_versions() {
        for (output, expected) in [
            ("2.1.276 (Claude Code)", Some(UsageScope::Turn)),
            ("2.1.277 (Claude Code)", Some(UsageScope::SessionCumulative)),
            ("2.1.281 (Claude Code)", Some(UsageScope::SessionCumulative)),
            ("owned fixture private-token", None),
        ] {
            let version = version(Provider::Claude, output.as_bytes());
            assert_eq!(scope(Provider::Claude, version.as_deref()), expected);
        }
        assert!(version(Provider::Claude, b"2.1.281 (Claude Code)\nprivate-token").is_none());
        assert_eq!(
            scope(
                Provider::Codex,
                version(Provider::Codex, b"codex-cli 0.145.0").as_deref()
            ),
            Some(UsageScope::SessionCumulative)
        );
        let value = json!({"type":"result","modelUsage":{"claude-sonnet-4-6":{"inputTokens":100,"outputTokens":30,"cacheReadInputTokens":20,"cacheCreationInputTokens":5}},"total_cost_usd":0.001});
        let mut old = UsageSample::default();
        event(Provider::Claude, &value, &mut old, Some(UsageScope::Turn));
        assert_eq!(old.input_tokens, Some(125));
        assert_eq!(old.scope, UsageScope::Turn);
        let mut unknown = UsageSample::default();
        event(Provider::Claude, &value, &mut unknown, None);
        assert!(unknown.input_tokens.is_none() && unknown.cost_microusd.is_none());
        assert_eq!(unknown.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn codex_cumulative_scope_is_limited_to_the_proven_cli_version() {
        for (output, expected) in [
            ("codex-cli 0.145.0", Some(UsageScope::SessionCumulative)),
            ("codex-cli 0.144.0", None),
            ("codex-cli 0.145.1", None),
            ("codex-cli 0.146.0", None),
            ("codex-cli 1.0.0", None),
            ("codex-cli 0.145.0-preview", None),
            ("owned fixture unknown-version", None),
        ] {
            let version = version(Provider::Codex, output.as_bytes());
            let observed_scope = scope(Provider::Codex, version.as_deref());
            assert_eq!(observed_scope, expected);
            if observed_scope.is_none() {
                for kind in ["turn.completed", "turn.failed"] {
                    let mut sample = UsageSample::default();
                    event(
                        Provider::Codex,
                        &json!({"type":kind,"usage":{"input_tokens":400537,"cached_input_tokens":20,"output_tokens":30}}),
                        &mut sample,
                        observed_scope,
                    );
                    assert!(sample.input_tokens.is_none());
                    assert!(sample.output_tokens.is_none());
                    assert!(sample.cached_input_tokens.is_none());
                    assert!(sample.cost_microusd.is_none());
                    assert_eq!(sample.provenance, Provenance::Unknown);
                }
            }
        }
        assert_eq!(scope(Provider::Codex, None), None);
    }

    #[test]
    fn exported_codex_resume_samples_keep_reported_session_totals_for_ledger_deltas() {
        // Input counters already exported by the authorized two-turn rehearsal.
        // Parsing must preserve both raw reports; the ledger computes the delta.
        let mut reports = Vec::new();
        for input_tokens in [400_537, 1_133_389] {
            let mut sample = UsageSample::default();
            event(
                Provider::Codex,
                &json!({"type":"turn.completed","usage":{"input_tokens":input_tokens}}),
                &mut sample,
                scope(Provider::Codex, Some("0.145.0")),
            );
            assert_eq!(sample.scope, UsageScope::SessionCumulative);
            assert_eq!(sample.input_tokens, Some(input_tokens));
            assert!(sample.output_tokens.is_none());
            assert!(sample.cost_microusd.is_none());
            reports.push(sample.input_tokens.unwrap());
        }
        assert_eq!(reports, [400_537, 1_133_389]);
    }
}
