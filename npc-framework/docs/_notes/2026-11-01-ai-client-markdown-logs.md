# AI client Markdown logs — 2026-11-01

Request: automatically save a full Markdown log for every client session.

The client creates a unique private file per accepted Run under `.ai-client/logs`,
including setup failures before a provider conversation exists. Each prompt links
the conversation's existing local metrics ID. All turns in the run share its
log; restart/fresh-session operations preserve previous files. `--logs-dir`
overrides `NPC_AI_LOGS_DIR`, independently of the JSON metrics destination.

Capture includes prompts, closed game notifications, CLI-emitted assistant/tool
events, stdout/stderr, selection intent, reported usage and lifecycle outcomes.
Credentials and identifiers are redacted before writing, including nested JSON
and secret object keys. Reserved key names prevent redaction collisions from
overwriting ordinary transcript values. Indented Markdown blocks contain hostile
fences/HTML. Per-turn stream/line limits and interrupted capture are explicit.

Logging failure stops dispatch while draining accepted identity/usage to the
independent state/metrics destinations. Foreign conversation identity still
latches before accepting its reports. Public status remains transcript-free and
can record an actionable logging error even when the log itself is unwritable.

## Validation

- `cargo test --locked -p npcctl --bin ai-agent-game-client`: 67 passed,
  including split UTF-8/credentials, nested JSON/key redaction, collision
  preservation, oversized lines, safe Markdown, sticky I/O failure, public-status
  persistence and accepted/foreign identity accounting during failure cleanup.
  The final cleanup regression preserves a first verified conversation ID learned
  only during shutdown, despite a log write error. It failed before the fix with
  `None` instead of the verified ID (`target/ai-client-markdown-cleanup-before.log`)
  and passes in the final 67-test run (5.29 seconds). Identity rollback now depends
  on explicit identity rejection, rather than every fatal polling error.
  Evidence: `target/ai-client-markdown-units.log`.
- `cargo test --locked -p npcctl --bin ai-agent-game-client --test ai_client -- --test-threads=4`:
  initial run 65 unit tests and 23 integration tests passed; integration duration
  133.97 seconds. Real explorer HTTP/MCP with native fake providers verifies both
  protocol formats, prompt/assistant/tool transcript content, two-turn runs,
  restart preservation and metrics links, separate fresh-conversation logs,
  explicit folder precedence and failing destination before any provider probe.
  Windows crash recovery also checks an immutable partial log without a final
  success entry. Evidence: `target/ai-client-markdown-focused.log`.
- Independent lifecycle and product/docs subagent reviews: SHIP after correcting
  credential-key redaction, generated-key collisions and log-error accounting
  draining. Reviewers ran no apps, builds, provider calls or history reads.
- Full current-source `cargo xtask verify --pretty`: **`ok:true`**, all seven
  steps passed. Format and workspace/all-target Clippy (`-D warnings`) passed;
  **1,528/1,528 Nextest tests passed**, with one slow test, five leaky-test warnings
  and twelve skipped tests. Doc tests: one passed, zero failed, ten ignored.
  Architecture: seven passed. Eight replays verified. Six goldens, none pending.
  Dependency advisories, bans, licenses and sources passed; no optional tool was
  skipped. The captured report does not identify the five leaky tests; these are
  warnings, not a claim that every subprocess in the workspace was leak-free.
  Evidence: `target/ai-client-markdown-verify.json` and
  `target/ai-client-markdown-verify-progress.log`.
- `cargo build --locked --release -p npcctl --bins`: passed in 38.74 seconds
  after the final cleanup fix. The release client and sibling `npcctl` are
  refreshed for native launches. Release `ai-agent-game-client run --help` exits
  zero and exposes `--logs-dir` / `NPC_AI_LOGS_DIR` / the workspace default.
  Evidence: `target/ai-client-markdown-release.log` and
  `target/ai-client-markdown-help.txt`.

An earlier full gate passed format and Clippy, then was deliberately stopped
during tests when the cleanup-only regression was found. Its empty verdict and
progress/owned-stop evidence remain under
`target/ai-client-markdown-verify-superseded*` and
`target/ai-client-markdown-superseded-stop.log`. It is not a verification pass.

No real model turn, provider-history read or GUI input was used for this change.
This is host-side logging; simulation and native UI layouts are unchanged. Logs
contain private gameplay/Notebook content after redaction. They cannot reconstruct
output the CLI did not emit or unavailable trailing output after interruption.
