# AI-Agent-Game-Client implementation and validation

Started: 2026-10-29; metrics requirements added 2026-10-30. Scope: dedicated NPC
Framework explorer client, session accounting and native integration.

The operator supplies an installed, logged-in Codex or Claude CLI. Native game
controls start the dedicated client, which supplies the initial mission, registered
MCP tools and subsequent exact-conversation prompts. No prompt copying, provider
conversation lookup or manual scheduler binding is required.

Design and execution: [implementation plan](../superpowers/plans/2026-10-29-ai-agent-game-client.md),
[ADR 0027](../adr/0027-dedicated-ai-agent-game-client.md),
[operator guide](../ai-agent-game-client.md).

## Delivered product

- Separate `ai-agent-game-client` executable in the existing npcctl Cargo package;
  existing `cargo run -p npcctl` remains unambiguous through `default-run`.
- Codex and Claude subscription preflight, structured provider lifecycle parsing,
  automatic session creation, exact resume, private persistence and explicit
  fresh-session recovery with archival.
- Per-turn validated registered MCP configuration, avoiding manual Codex project
  trust setup while preserving provider models, permissions and global settings.
- Authenticated host leases and shared turn slots; bounded classified events,
  observation receipt pruning, cooldown/rate/backoff/timeout and stop handling.
- Native provider/start/stop/status/recovery controls, F10 access, pause preservation,
  keyboard focus and detached-world status isolation. Manual details are optional.
- OS advisory workspace ownership; Windows kill-on-close provider jobs and tested
  crash recovery. Provider output and session identifiers are excluded from public status.

## Subagent improvement pass

| Perspective | Findings addressed |
|---|---|
| Product/onboarding | Removed manual prompt/session setup from normal flow and generated briefings; explicit fresh-session recovery; native launch/status controls. |
| Architecture/gameplay | Read-only operator monitoring, private claim authentication, shared client/legacy concurrency, preserved recorded game-command path. |
| Session reliability | Exact identity checks, OS locks, Windows crash cleanup, reconnect bounds, output limits, semantic completion and owned descendant cleanup. |
| Events/subscription use | Closed summaries, observation acknowledgments, disabled-event backlog clearing, no paid keepalive prompts or inferred latest conversations. |
| Native usability | Duplicate Start prevention, readable errors, pause preservation through both control/help panels, modal focus, collapsed manual help, workspace-tagged background status. |
| Workspace integrity | Explicit current briefing rather than mtime inference; stable named variants/configs and preservation of owned files. |
| Documentation/design | New operator guide, README/experiment/Island links, plan and ADR; inherited native visual system retained. |
| Metrics/accounting | Nullable reported cost/tokens/model/effort, persistent private ledger and configurable export; failed/interrupted outcomes, high-water cumulative reports and CLI scope-transition regressions. |
| Final native input | Actual keyboard Save and event-checkbox input; fixed hidden initial AI focus blocking F7, stale manual-link HUD copy and clipped Settings actions. Rebuilt confirmations tracked below. |

## Completed focused checks before the final diagnostic and metrics batches

Run from `npc-framework/`, with one Cargo builder:

| Exact command | Result |
|---|---|
| `cargo test --locked -p npcctl --test ai_client -- --test-threads=1` | 7 passed, 0 failed; 98.58 seconds after compilation. |
| `cargo test --locked -p npc_agent_io --lib scheduler::` | 28 passed, 0 failed. |
| `cargo test --locked -p npcctl --bin ai-agent-game-client` | 10 passed, 0 failed. |
| `cargo test --locked -p testbed_content --lib explorer::workspace` | 27 passed, 0 failed. |
| `cargo test --locked -p actor_testbed --bin actor_testbed ai_client::tests` | 6 passed, 0 failed. |
| `cargo test --locked -p game_menu --bin game_menu ai_` | 2 passed, 0 failed. |
| `cargo fmt --all -- --check` | Exit 0. |
| `git diff --check` on changed client files | Exit 0. |

Total: 80 focused tests passed. The native mechanical design detector returned
an empty findings list for the new actor panel and detached controls.

Installed CLI checks for both providers returned `installed:true`, `logged_in:true`
and `ready:true`, without starting a model turn. These are login-readiness checks,
not proof of model entitlement or authenticated gameplay.

The integration suite uses native fake providers with a real explorer HTTP/MCP
host. It covers both protocols, initial/exact resumed sessions, registered game
actions, notifications, auth refusal, ownership, stop, failures/privacy, Windows
crash recovery and explicit fresh-session archival.

## Coordinated final verification

Native actor-control captures were made with the current built debug game in a
new disposable world, without launching a provider:

- `target/ai-client-visual-44431ee2e5cc456a82f19de66cc78b24/out/onboarding.png`:
  1280×720 logical / 2560×1440 physical, DPI scale 2.
- `.../out/onboarding-small.png` and `.../out/paused-controls-small.png`:
  760×520 logical / 1520×1040 physical, DPI scale 2.
- `.../native-controls-report.json`: `ok:true`,
  `return_preserves_paused:true`, `owned_game_stopped:true`, `provider_started:false`.

This exercises BRP widget/key messages and view-only window sizing, not physical
keyboard or screen-reader input. A fresh finishing subagent returned
`disposition: ship` for the actor controls at both tested sizes, with no material
fixes. This is a controls-only visual verdict, not authenticated gameplay evidence.

The existing playtest chat owns the release build, native captures, one separately
authorized two-turn/600-second Codex engineering smoke, and one full
`cargo xtask verify` against the combined checkout. Its final workspace verification
is tracked separately. No authenticated provider run was started by the client
implementation agents.
The maintained `scripts/playtest/client_smoke.py` passed syntax and `--help`
checks; it stops at the first provider failure and reports only closed results/counts.

### Authorized Codex smoke: failed first turn, no retry

The separately authorized disposable run
`target/playtest-client-smoke-vrtglnt7/` ended after **22.713 seconds** with
**zero completed turns**. Its independent guard stopped at the first provider
failure; no retry or second model turn was authorized or attempted. The final
public client status is `state:stopped`, `turns_completed:0`, `failures:1`,
`session_ready:true`, `connected:false`. Session readiness only confirms that a
conversation identity was captured, not that its game task completed.

The original `summary.json` reported `provider_failures:0` because its main-loop
sample preceded the guard's failure observation. The final public `status.json`
records one failure. These original evidence artifacts remain unchanged; this
record explicitly documents the sampling discrepancy. The maintained script
now refreshes allowlisted final public counters and an optional closed failure
category after client shutdown, before emitting future summaries.

The old client discarded provider error details. The underlying cause of this
attempt is therefore **unknown**; it cannot be recovered by adding diagnostics
afterward. No unaided survival, authenticated gameplay, two-turn continuity or
event-triggered live continuation passed in this attempt. Its report says the
owned resources were stopped, and the event fixture was not reached.

The final diagnostic batch adds only closed error categories and fixed operator
guidance, retaining no raw provider text. Fake-provider regression cases cover
recognized structured/stderr authentication, rate and MCP failures, unknown and
secret-bearing errors, Stop retention and older public-status compatibility.
The later focused results below cover this batch; no additional authenticated
provider turn was used for those checks.

### Added user requirement: session metrics candidate

The user subsequently requested configurable per-session JSON metrics for
costs, input/output tokens, model and effort. The candidate adds a private local
metrics identity and canonical ledger, configurable exports through
`--metrics-dir` / `NPC_AI_METRICS_DIR` / native menu settings, provider-reported
usage extraction, explicit unknown values, cumulative-Claude delta accounting
and separate new-session records. Costs are never fabricated for subscription
output: unavailable Codex dollars remain null; Claude's reported API-equivalent
estimate is distinguished from subscription billing.

Fake-provider cases now cover both successful protocols across initial,
continued and restarted client processes, a changed export folder, independent
fresh-session totals, model/effort metadata changes, missing metrics, environment
configuration, failed usage retention and redaction. These are offline integration
checks, not additional live provider evidence.
No provider run or retry was authorized by this metrics extension, and the
original failed smoke artifacts are unchanged. Its historical token/cost/model
details cannot be retroactively inferred from the current chat or new code.
The final accounting candidate also distinguishes known older Claude per-call
reports from 2.1.277+ cumulative reports, declines numeric aggregation for
unknown versions, and labels cumulative deltas as intervals since the previous
session report. Such an interval can span missing reports and does not claim
exact process-level billing. Targeted fake cases cover version selection and
Stop immediately after an exited provider's queued terminal event.

### Later focused results, 2026-10-30

| Exact command | Result and evidence |
|---|---|
| `cargo test --locked -p npcctl --bin ai-agent-game-client --test ai_client -- --test-threads=1` | Binary unit tests: 27 passed. Integration: 12 passed, one fixture invocation failed before provider startup because `--poll-millis` was supplied twice. Original log retained at `target/stream-client-metrics.log`. |
| `cargo test --locked -p npcctl --test ai_client stop_drains_a_queued_terminal -- --test-threads=1` | After fixing the fixture to supply exactly one poll value, 1 passed / 12 filtered, 18.02 seconds. `target/ai-client-queued-stop-confirm.log`. Assertions and timing bounds were preserved. |
| `cargo test --locked -p testbed_content --lib explorer::` | 86 passed, 1 ignored, 11 filtered. `target/stream-explorer-confirm.log`; includes generated briefing and native helper compatibility/privacy coverage. |
| `cargo test --locked -p game_menu --bin game_menu ai_` | 4 passed, 121 filtered. `target/stream-menu-ai-final.log`; includes metrics-folder validation, native save/default behavior and background status isolation. |
| `cargo test --locked -p npcctl --bin ai-agent-game-client ai_client::metrics::tests` | 13 passed, 17 filtered. `target/stream-metrics-scope-final.log`; includes all three final scope-transition regressions. |
| `cargo test --locked -p actor_testbed --bin actor_testbed ai_client::tests` | 8 passed, 55 filtered. `target/stream-actor-focus.log`; includes both final focus/draft/pause regressions. |
| `cargo test --locked -p game_menu --bin game_menu screens::tests` | 19 passed, 107 filtered. `target/stream-settings-polish.log`; includes Settings save/default and changed-focus reveal behavior. |
| `cargo test --locked -p testbed_view --lib explorer_panel::tests` | 26 passed, 69 filtered. `target/stream-f7-final.log`; includes real primary-window startup focus, foreign-control protection and notebook draft/stale-save coverage. |

The final independent source pass found a further accounting boundary: changing
an exact Claude conversation from an older per-call CLI to a cumulative-reporting
version must not count prior usage again. The fix persists the last numeric
accounting scope privately and establishes an unknown delta at that boundary;
subsequent cumulative deltas resume normal accounting. Three added regressions
cover upgrade/restart, reverse transitions with missing reports and legacy
ledger migration. A second independent reviewer found no defect in this fix;
all 13 metrics tests subsequently passed. These results establish the tested
batches only; they do not replace the required final workspace gate or native
recaptures.

### Final native client captures and input checks

`target/ai-client-final-visual-5f88835e51c8/report.json` reports `ok:true`,
`provider_started:false`, unchanged tick zero and stopped owned hosts. Its
1920×1440 physical / 960×720 logical menu captures show the metrics field,
corrected **Stop world** controls and a synthetic, closed `mcp_startup`
diagnostic. A fresh product reviewer returned **ship** for detached controls and
diagnostics; source TabIndex values 0–8 are unique. The diagnostic fixture never
started a provider and does not diagnose the historical real smoke failure.

Physical Windows Computer Use checks observed Agents → Actors → Worlds →
AI metrics → Fog → Save keyboard order and a successful Enter Save receipt;
the configured metrics folder remained in the disposable settings file.
`worlds-input.png` and `native-input-observations.json` record the actual event
dispatch checkbox changing from checked to unchecked in an unsaved disposable
editor draft. No running world, provider or user preset was affected.

These checks also exposed a clipped Settings footer at the default window size.
A Settings-only bounded form/footer layout and focus reveal correction is now
confirmed by the background release capture at
`target/ai-client-final-visual-88c4cdca6a2b/metrics-settings.png`: Save and Back
are fully visible and metrics copy is readable at 960×720 logical. The new
capture report is `ok:true`, provider-free and all owned hosts stopped; a fresh
reviewer returned **ship** for this bounded correction. Post-fix physical wheel
behavior is not claimed: the user stopped Computer Use, and desktop input was
not resumed. The coordinated native
rehearsal additionally exposed an F7 blocker after initial onboarding: persistent
startup autofocus saved/restored a hidden AI control. That source correction
removes startup autofocus, filters own controls during focus restoration and adds
two draft/pause regressions; all eight agent-control tests passed, while actual
rebuilt F7 capture initially failed again at
`target/playtest-native-05030c9bbb/summary.json`. A separate unchanged-keypress
BRP repro at `target/ai-client-focus-repro-af3e8821bc/summary.json` proved focus
exactly equalled the primary Window after Return and before/after F7, while the
notes root remained hidden. The notebook's foreign-control guard rejected
ordinary Window focus. A second bounded fix admits Window focus while retaining
foreign-widget/text-field guards; its regression uses real InputFocusPlugin
startup and PrimaryWindow. All 26 notebook tests passed. The rebuilt strict
capture at `target/playtest-native-3c200f103e/` still failed at F7; that original
record is preserved. Two further current-release BRP reproductions passed
without edits or key resends, including the exact maintained capture sequence.
`target/ai-client-exact-F7-1793349159821865100.json` proves primary-window focus
and `Display::Flex` through the full notes ancestor chain at that checkpoint.
Its diagnostic fixture deliberately ended at the checkpoint and cleaned up;
it is not a passing full capture. The earlier current-release failure remains
unreproduced, with no retrospective cause claimed. The complete native-only
confirmation subsequently passed at `target/playtest-native-c3afbe82c4/summary.json`
with `ok:true`, all six requested images and clean owned-host shutdown. Its
strict F7 assertion passed on the first call, without resending the key or
using the diagnostic wrapper's post-failure observations. All six PNGs were
opened: F7 notes, corrected legacy HUD guidance, speech, normal/narrow dialogue
and recovered diary render visibly. This is the six-state polishing subset,
not a rerun of the earlier twenty-state visual matrix.

Coordination messages crossed while the sibling paired capture was starting.
The native-only run had already finished before its owned-process cleanup
check; no sibling process was stopped. Because overlap is possible, this run
is excluded from exclusive performance claims. Its visual and control evidence
is retained. The sibling paired run at `target/playtest-native-ce4da28105/`
ended with `bounded browser check timed out`, `owned_browser_server_exited:true`
and `owned_process_exited:true`. This is a failed orchestration record, not a
passing complete paired fixture. The subsequent paired attempt at
`target/playtest-native-dbbb830097/` also timed out at that manual checkpoint
and stopped both owned processes. Neither failed orchestration record is
converted into a pass. Any separately captured browser observations must be
reported with their own evidence by the playtest owner.

The final callback-assisted paired run at
`target/playtest-native-925c18f471/summary.json` subsequently reports **`ok:true`**,
all six polishing captures, full fixture completion through diary recovery,
`owned_browser_server_exited:true` and `owned_process_exited:true`. The local
completion helper recorded its receipt and stopped; final success was taken
from the maintained fixture's summary, not from the receipt alone. The playtest
owner inspected both browser sizes through background APIs. Desktop input was
not resumed. The earlier timed-out records remain unchanged.
The Island HUD's stale manual-link message now directs F10 Start controls and
labels the optional legacy scheduler explicitly.

The corrected Worlds scheduler footer is confirmed by the opened background
capture `target/stream-worlds-footer-ready.png`: Save world, Revert and Back
are fully visible while the long form remains inside its scrolling area.
The screenshot-only page selector is restricted to screenshot mode and leaves
the preset and draft unchanged; its two focused tests passed. No desktop input
or provider was used for that capture.

An earlier final capture attempt at `target/ai-client-final-visual-3bf7b5312ee0/`
is preserved with `ok:false`: its harness incorrectly expected a menu to leave
an adopted host running on exit. The menu correctly saved/stopped it. The
successful fixture uses an independent fresh host per menu and verifies saved
tick zero and clean shutdown.

### First full workspace gate and focused repairs

`cargo xtask verify --pretty` completed with **`ok:false`** in
`target/stream-verify.json`. Formatting, Clippy, seven architecture tests, all
eight replay files, six goldens and cargo-deny passed. Nextest ran 1,478 tests:
1,473 passed, five failed and 12 were skipped; doc tests had one pass and ten
ignored cases. This original failed report is preserved.

Four menu failures exposed an accessibility-update ordering race during screen
rebuilds and an obsolete manual-instruction expectation. The repair orders
rebuilds before accessibility updates and asserts the complete automatic-client
guidance with the exact agent-folder path. All four repaired cases passed in
`target/stream-menu-repair.log` (2.346 seconds).

The fifth failure was the client diagnostic/privacy integration case. Its exact
original panic was outside the verifier's retained log tail, and the local
Nextest store reported no recorded runs. It subsequently passed both the
five-case focused rerun and the complete 13-case concurrent client run:
`target/stream-client-pressure.log` reports **13 passed, zero skipped**, in
114.215 seconds. Provider/mode labels and sanitized early-exit context were
added to the diagnostic test; its assertions, limits and CLI Stop action remain
unchanged. No production cause or fix is claimed for the unreproduced failure.
A separate direct stress attempt was cancelled for build-queue coordination;
its cancellation record is not a test pass.

### Second full workspace gate: passed

`cargo xtask verify --pretty` with process-scoped `NEXTEST_TEST_THREADS=4`
completed with **`ok:true`** in `target/stream-verify-recheck.json`. All seven
steps passed: formatting, Clippy, workspace tests, architecture fitness,
replays, goldens and cargo-deny. No optional tool was skipped.

Nextest ran **1,478 tests: 1,478 passed**, with 12 skipped tests, one slow test
and two leaky-test warnings. Doc tests had one pass and ten ignored cases.
Architecture checks passed all seven tests; all eight replay files verified;
six goldens had no pending snapshots; advisories, bans, licenses and sources
passed. The repaired release applications were rebuilt before this gate.

The caller restored the worker-count environment setting afterward; no global
or repository policy configuration changed. This resource-bounded full run is
reported alongside the separate **13-case concurrent client pass** above. The
earlier default-concurrency diagnostic failure remains unreproduced, and its
original failed report is retained. A successful authenticated provider scenario
is still separate from this complete offline gate.

### Fresh authorized Codex rehearsal: model availability blocked

The newly authorized run at `target/playtest-client-smoke-3ipwn1de/summary.json`
stopped at its first provider failure after **18.629 seconds**. It records
`ok:false`, zero completed turns, one failure, `last_failure_code:model_unavailable`,
`final_status_sampled:true` and `owned_resources_stopped:true`. The event fixture
was not applied and session reuse was not demonstrated. No retry or model change
was made under this single-run authorization. The closed category is a diagnostic
hint; this record does not reconstruct raw provider errors or diagnose the older
22.713-second attempt retrospectively.

Allowlisted inspection of this run's metrics export found exactly one JSON
record: started one, failed one, completed/interrupted zero, last outcome failed.
Model, effort, input/output tokens and cost are null with unknown sources and
zero reporting contribution counts; `cost_available` is false. This proves
failure accounting and missing-data handling, not a zero token charge or a
selected-model identity. No private session state or provider history was read
for that inspection. Read-only effective-model/catalog diagnosis remains with
the playtest owner; any new model turn requires fresh authorization.

### Model-resolution correction: previous candidate

Metadata-only probes confirmed effective workspace model `gpt-6.1-sol` was absent
from the visible catalog, which exposed exactly one default supporting the
configured high effort. Neither probe created a thread or turn; owned processes
stopped. Sanitized reports are under
`target/playtest-model-catalog-d56f65117f/diagnostics/` and
`target/playtest-model-config-025251ffef/diagnostics/`.

The client now reads effective configuration and the visible catalog in its exact
workspace before dispatch. Supported configured choices are preserved; otherwise
only a unique visible advertised default and supported effort are used. Options
apply only to the owned initial/resume process. Invalid or incomplete metadata
and custom/unknown providers fail closed. Global settings remain unchanged, and
there are no trial model calls, automatic model retries or new picker UI.
Catalog metadata alone does not guarantee inference entitlement.

Optional `check --workspace DIR` verifies selection without a conversation or
turn. Separate `selection_intent` fields describe requested configuration in
session exports and attempt history; actual execution model/effort and usage
retain their provider-reported meaning. Historical failed exports remain null.

All **41 binary unit tests passed** in `target/stream-model-unit.log`, including
17 metrics tests, six metadata/selection/parser regressions and a command-argument
case. All **16 integration cases passed** in `target/stream-model-integration.log`
(87.729 seconds), retaining the original 13 and adding model-selection, refusal
and metadata-only workspace-preflight coverage. Workspace all-target Clippy
passed with warnings denied (`target/stream-model-clippy.log`), and the repaired
release binaries built successfully (`target/stream-model-release.log`).

The shipped client's installed-CLI `check --workspace` proof at
`target/stream-model-preflight.json` reports `ok:true`, exit zero, login ready
and `model_selection_ready:true`. It selects advertised-default `gpt-5.6-sol`
with configured `high` effort. Global configuration, the failed fixture's private
state and its public status remained unchanged. This was metadata-only and
started no conversation. It does not establish inference access.

### Third full workspace gate: model-resolution candidate passed

The model-resolution candidate completed `cargo xtask verify --pretty` with **`ok:true`**
in `target/stream-verify-model.json`. Process-scoped
`NEXTEST_TEST_THREADS=4` and `NEXTEST_FAILURE_OUTPUT=immediate-final` bounded the
run and retained failure diagnostics without changing global configuration.

Nextest ran **1,492 tests: 1,492 passed**, with 12 skipped tests, one slow test
and one leaky-test warning. Doc tests had one pass and ten ignored cases.
Formatting, workspace Clippy, all seven architecture checks, eight replays,
six goldens with none pending, and every cargo-deny policy passed. No optional
tool was skipped. This result supersedes the previous candidate's 1,478-test
green result for that source; earlier reports remain unchanged. Later runtime
changes require their own full gate, recorded below.

### Compatible-model live rehearsal: gameplay acceptance failed

After fresh authorization, the maintained two-turn/600-second rehearsal on the
verified candidate stopped after **47.719 seconds**. Its sanitized report at
`target/playtest-client-smoke-_7wyymk7/summary.json` records **one completed turn,
zero provider failures**, `ok:false`, and
`failure_code:initial_turn_missing_required_game_artifacts`. The initial named
notebook note, physical diary page and spoken line each counted zero. The need
fixture was not applied, and session reuse and event continuation were not
demonstrated. Owned resources stopped. This is a failed gameplay acceptance
check, not a successful two-turn scenario or a provider failure. No retry is
authorized by this result.

Allowlisted inspection of its exported metrics confirms requested model
`gpt-5.6-sol` and effort `high`, with selection sources `advertised_default` and
`cli_config`. Provider-reported usage totals are 94,806 input tokens, 667 output
tokens and 78,720 cached input tokens, with one input/output reporting
contribution. Actual execution model and effort remain null with unknown
provenance; cost remains null with `cost_available:false`. Requested selection
is not substituted for missing execution metadata. No private history, session
state or provider command line was read for this inspection.

### Scoped game-tool consent and fatal cleanup: current candidate

A subsequently authorized inspection of only the latest rehearsal's final reply
and game-tool names/statuses identified one cancelled `register` call. No tool
arguments, result bodies or other conversation were inspected. The original
run summary remains unchanged. Metadata-only
`target/playtest-mcp-metadata-02443bfaaa/diagnostics/mcp-metadata-report.json`
listed all 16 tools with no filters or approval keys. Pinned CLI source review
identified the noninteractive MCP approval-cancellation path.

The client now offers explicit Codex-only `--approve-game-tools`, off by default.
Native Start explains world/private-Notebook access before enabling it. Only the
16 known registered tools receive process-local approval options after trusted
executable/current-world validation. Explicit server disable or conflicting
approval preferences refuse setup. Global files, sandbox settings, other servers
and filters are not changed; managed approval precedence remains CLI-dependent.
Configuration is checked at startup, so configuration edits require a restart.
Tool annotations were not falsely labelled read-only to avoid approval.

Canonical cancellation remains failed across later completion and reconnect
cleanup. Polling errors are fatal after owned shutdown and independent accounting.
Conflicting identities preserve the original private conversation, permanently
reject later progress and exclude foreign usage. Both synchronized cleanup fault
variants retain exactly the original session's reported usage and dispatch once.

`target/ai-client-identity-fixed.log` reports **53/53 unit tests passed** (2.16
seconds) and **21/21 integration cases passed** (83.78 seconds), with four test
threads and no real provider. The preceding 51/20 run remains in
`target/ai-client-reconnect-fixed-recheck.log`. Negative controls in
`target/ai-client-reconnect-negative.json` and
`target/ai-client-identity-negative.json` reproduced the unwanted second dispatch
with the corresponding cleanup fix removed; each restored original source bytes
before the corrected checks. Their intentionally failed test logs are retained.

Native helper tests passed **4/4** in `target/ai-client-reconnect-native.log`.
The earlier stale-copy assertion failure and missing-export-directory unit-fixture
failure remain in their original logs; their fixture repairs kept assertions.
Current actor captures at `target/stream-approval-visual-7249cad0a21e/report.json`
passed at 1280/760/480 widths, saved tick zero and stopped the owned app. Detached
captures at `target/stream-approval-detached-d6b2b5d8e05b/report.json` passed with
unchanged ticks and clean adopted-host shutdown. Both reports record no provider
start. Subagent review opened all five images and returned SHIP. The detached
cancellation example is explicitly synthetic. The first actor capture's incorrect
cleanup-method failure remains preserved at `target/stream-approval-visual-27f0f5c3c30f/`.

The release build before the identity-fault refinement passed in 2m25s. Its
in-progress full gate was deliberately stopped for the newly confirmed cleanup
edge: `target/stream-verify-approval-aborted.json` records `ok:false`,
`verdict:aborted_before_completion`, owned verifier stopped and no source change
during the run. It is not a passing gate.

### Current release and complete workspace gate: passed

`target/stream-verify-final.json` remains the failed style gate: all 1,511 tests
passed, but detached-view formatting and three Clippy diagnostics prevented an
`ok:true` verdict. The corrections used the compiler's if-chain and fixed-array
chunk suggestions, with no behavior, assertion or policy change. Workspace
formatting and all-target Clippy subsequently passed.

The next report, `target/stream-verify-ready.json`, remains **`ok:false`**: 1,510
of 1,511 tests passed, and the original death/new-life privacy case failed because
successor notebook assignment was not yet available. All six other gate steps
passed. The exact failure was reproduced in
`target/island-successor-notebooks-focused.log`, rather than dismissed as flaky.

The host now installs life-specific notebooks before publishing successor
credentials. The unchanged out-of-process case passed in **2.11 seconds** in
`target/island-successor-notebooks-fixed.log`. A deterministic regression makes
registration the first API call after new life, before another host frame or
control claim. Its pre-fix failure is retained in
`target/island-notes-before-fix.log`; the fixed case passed in **0.12 seconds** in
`target/island-notes-after-fix.log`. It verifies fresh diary/memory/proposals,
preserved prior mapping, revoked old control, usable pre-control notes and an
unchanged simulation hash. Independent source review returned SHIP.

The current all-five-application release build passed in **2m03s**, recorded in
`target/stream-release-lifecycle.log`. The subsequent
`cargo xtask verify --pretty` completed with **`ok:true`** in
`target/stream-verify-lifecycle.json`, using process-scoped four-worker Nextest
and immediate/final failure output. All seven steps passed; no optional tool
was skipped. Nextest ran **1,512 tests: 1,512 passed**, with 12 skipped tests,
one slow test and two leaky-test warnings. Doc tests had one pass and ten ignored
cases. Formatting, warnings-denied workspace Clippy, seven architecture tests,
eight replays, six goldens with none pending, and all cargo-deny policies passed.
The caller restores process environment settings. Earlier failures and warnings
remain recorded; none were silently reclassified as success.

This establishes the lifecycle candidate's offline gate and reviewed native
controls. The subsequent live proof and accounting correction are recorded below.

### Authorized bounded Codex gameplay: passed

With fresh authorization for two turns, 600 seconds and the 16 registered game
tools, `target/playtest-client-smoke-7my4yxvi/summary.json` records **`ok:true`**
after **92.89 seconds**. Two turns completed with zero provider failures, exact
session reuse and no archives. The recorded need fixture triggered the second
turn. Each turn produced one spoken line, one physical diary page and one private
Notebook note. Owned resources stopped; final status was sampled. No provider
history was read. This proves the bounded engineering scenario, rather than
prolonged unaided survival. Its authorization is consumed; no further provider
run is authorized by the result.

Allowlisted exported metrics inspection confirms two completed attempts and
requested `gpt-5.6-sol`/`high` selection intent. Actual execution model/effort remain
null/unknown and cost remains null with `cost_available:false`, because the CLI
did not report them. Those unavailable values are not retrofilled from intent.

### Final accounting audit: cumulative-scope defect and correction

Pinned Codex 0.145.0 source
[restores prior token-count information on resume](https://github.com/openai/codex/blob/rust-v0.145.0/codex-rs/core/src/session/mod.rs#L1225),
adds new usage to its total, and
[emits that total in `turn.completed`](https://github.com/openai/codex/blob/rust-v0.145.0/codex-rs/exec/src/event_processor_with_jsonl_output.rs#L107). The client
incorrectly interpreted these reports as per-turn values. The successful run's
original JSON/exports remain unchanged. Its reported input snapshots are
400,537 and 1,133,389; output snapshots are 1,614 and 3,039; cached-input snapshots
are 349,312 and 1,039,232. Correct session totals are therefore **1,133,389 input,
3,039 output and 1,039,232 cached input**. Second-turn deltas are 732,852, 1,425
and 689,920. The original exported sums overcount the first snapshot.

The correction recognizes only proven Codex CLI 0.145.0 as cumulative; numeric
usage from unverified versions remains unavailable. Metrics format/revision 2
separates certified tracked deltas, `reported_session_usage` and explicit
`accounting_coverage`. Legacy V1 Codex canonical/export preimages are preserved
privately before any rewrite; archive failure stops setup. Old figures remain
under `legacy_unverified`, identity and lifecycle counts continue, and a new
verified baseline precedes partial deltas. Existing Claude accounting is preserved.
The format bump prevents old clients from silently restoring wrong aggregation.

`target/ai-client-cumulative-focused.log` reports **61/61 unit tests passed**
(6.42 seconds) and **22/22 integration cases passed** (152.25 seconds), using four
threads and no real provider. Coverage includes the exact exported snapshots
across restart, fresh/adopted coverage, complete/pruned/interrupted legacy records,
archive failure/retry/idempotence, downgrade protection, Claude compatibility,
unknown CLI versions and foreign-session isolation with a genuine first-turn
baseline. Workspace format check passed in
`target/ai-client-cumulative-format.log`; warnings-denied all-target Clippy passed
in **1m14s** in `target/ai-client-cumulative-clippy.log`. Independent accounting
review returned SHIP with no in-scope contradiction.

`target/ai-client-codex-usage-audit-2026-10-30.json` is a separate calculation from
the already exported snapshots. It records corrected totals/deltas, original
sums and source hashes; the original smoke summary and metrics bytes are unchanged.
No provider replay or history access was used. The bounded gameplay proof remains
valid because the correction changes accounting, not game interactions.

### Final corrected-accounting release and complete gate: passed

The all-five-application release rebuild passed in **2m14s** in
`target/stream-release-accounting.log`. The subsequent
`cargo xtask verify --pretty` returned **`ok:true`** in
`target/stream-verify-accounting.json`. All seven steps passed, with no optional
tool skipped. Nextest ran **1,521 tests: 1,521 passed**, with 12 skipped tests,
one slow test and two leaky-test warnings. Doc tests had one pass and ten ignored
cases. Formatting, warnings-denied all-target Clippy, seven architecture checks,
eight replays, six goldens with none pending, and every cargo-deny policy passed.
The four-worker/immediate-final settings were process-local and restored.

Completion evidence covers the dedicated binary and native Start/Stop flow,
existing CLI login, automatic mission/tools and exact-session event continuation,
private ownership/cleanup and lifecycle boundaries, configurable JSON metrics,
model/effort provenance and corrected cumulative accounting. Both provider
adapters have real-host/fake-provider coverage. Five current native control
captures were opened and approved. The bounded real Codex scenario proves the
gameplay path; the real exported snapshots and current parser/ledger tests prove
the accounting correction without another paid call. Implementation, product,
lifecycle/privacy and independent accounting reviews returned SHIP. The original
failed reports and original live exports remain preserved.

## Limits

Unix abrupt-crash descendant cleanup has not been verified; Windows crash
cleanup and recovery have process-level test coverage. Claude's live authenticated
gameplay remains separate from its installed-login and fake-provider coverage.
Native controls/layout captures and the resource-bounded full workspace gate
have the evidence above, and the final paired-browser checkpoint passed.
The successful two-turn Codex rehearsal proves bounded authenticated gameplay,
continuity and event continuation. Earlier model-availability and tool-cancellation
failures remain historical evidence. The accounting correction above passed its
focused/style checks, independent review and fresh complete workspace gate.
Live Claude, prolonged unaided gameplay and Unix abrupt-crash cleanup remain
validation limits rather than claims established by the Codex rehearsal.
Further live attempts require fresh authorization.
Compilation, native captures and fake-provider tests do not establish the live
subscription outcome.
