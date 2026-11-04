# AI game-client research and polishing evidence

Final verdict: **verified**. The shared `cargo xtask verify --pretty` run exited
0 with `"ok":true`. Its terminal artifact is
`target/living-economy-verify.json`. All seven stages passed on the frozen source.
This final record supersedes earlier pending closeout descriptions in the plan
and the pre-gate frozen-manifest checkpoint.

Scope: same-PC authoritative game server, including headless hosting. The
[primary-source research](../research/2026-11-01-ai-game-server-client.md),
[implementation plan](../superpowers/plans/2026-11-01-ai-client-research-polish.md),
[ADR 0031](../adr/0031-bounded-ai-client-control-and-observability.md) and
[client guide](../ai-agent-game-client.md) describe the architecture, decisions
and operator workflow.

## Delivered behavior

- The game owns actor assignment, authorized observations/actions and simulation
  time. The client owns logged-in CLI conversations, prompts, supervision and
  private observability; the user does not manually prompt the agent.
- Separate deadline/response-bounded loopback control RPCs preserve existing game
  action/event waits. Closed connection faults distinguish temporary waiting from
  permanent operator failures.
- Canonical game revocation and required-tool failures remain fatal despite later
  provider success. Cleanup retains accepted conversation identity and usage,
  excluding a later conflicting identity and its usage.
- Native status distinguishes server connection, established provider identity
  and confirmed actor control, including attachment changes during an active turn.
- Private Markdown logs use bounded queued persistence, durable setup/dispatch
  barriers, batched synchronization, encoded capture limits and stronger compact
  credential redaction. JSON accounting remains independent.
- History capacity is reserved per record kind. Per-attempt metrics retain CLI
  version, verified/unknown usage scope and adapter revision alongside reported
  usage/model/effort/cost and requested selection intent. Original accounting and
  archives are preserved; unavailable values are not invented as zero.
- Native keyboard/clipboard/form-save handling preserves rapid shortcuts, edit
  ordering, focus, delayed-result safety and IME confirmation. Core metrics appear
  before setup diagnostics in the first viewport.

## Final shared gate

| Stage | Terminal result |
|---|---|
| Format | Pass; all files formatted |
| Clippy | Pass; no warnings |
| Nextest | 1,695 run, **1,695 passed**, 12 skipped; one slow and four leaky classifications |
| Doc tests | One passed, zero failed, ten ignored |
| Architecture | Seven passed, zero failed |
| Replays | Eight verified: one session, three testbed, four village |
| Goldens | Six snapshots, none pending; `INSTA_UPDATE=no` |
| Dependency policy | Advisories, bans, licenses and sources all pass |

No optional verification tool was skipped. The final runner changed only
process-local Nextest parallelism to eight; test scope, retries and skip policy
were unchanged. The verifier owner reported session 6190 terminal with exit 0;
this chat independently read and validated the complete JSON rather than
inferring success from partial output.

At final audit, all **501 candidate source hashes** still matched
`target/living-economy-final-candidate.json`, fingerprint
`cc24f5fd46140bf0a22ed0ec291194281cda1f613fb14e23b20e6b7ccd3b743a`.
All **26 refreshed client component hashes** also matched
`target/ai-client-research-frozen.json`. The final verdict was newer than the
manifest refresh. Its JSON SHA-256 was
`d46fdb9c3434168c979df084f11a174e51868952c1b854cefa033340083fe154`.
Only this validation document was updated after that source audit; the candidate
fingerprint covers `.rs`, `.toml`, `.lock` and `.json` source/config files.

## Focused and compatibility evidence

- `cargo test --locked -p npcctl --bin ai-agent-game-client`: 81 passed,
  including fresh-identity cleanup, blocked writer/queue pressure, cumulative
  accounting, redaction and backward compatibility.
- `cargo test --locked -p game_menu --bin game_menu`: 156 passed, with
  `quit::tests::hands_off_quit_with_the_real_binaries` intentionally ignored
  because it starts real windowed binaries. Actual settings-file assertions cover
  same-frame typing/Enter or Save, delayed paste/Enter/cancellation and IME confirmation.
- `cargo test --locked -p testbed_content ai_history --lib`: ten passed,
  including history fairness, provenance and core-summary ordering.
- Nine bounded transport fault tests passed. The earlier complete
  `npc_agent_io` library checkpoint passed 85 tests; the final full gate covers
  the later interrupted-header regression and all current library tests.
- `python -m unittest discover -s scripts/playtest -p test_client_smoke.py`:
  three no-provider privacy/diagnostic harness tests passed.
- The initial real-host/fake-CLI integration run had 22 passes and four failures.
  Preserving the Markdown heading, flushing before provider probes and publishing
  active actor-phase changes fixed those cases; their targeted nextest recheck
  passed all four in 27.709 seconds. The final full gate then covered the entire
  current integration suite.
- Changed-package Clippy with warnings denied and release builds of the menu,
  actor testbed and `npcctl` binaries passed. The shared owner rebuilt all five
  release packages after the compatibility correction.

The first shared full gate was preserved at
`target/living-economy-verify-before-maturation-fix.json`: 1,690 of 1,691 tests
passed; its sole failure was the exact legacy-status display assertion. The
reader now distinguishes absent phase fields from explicit false values. The
original legacy assertion was preserved unchanged; new regressions cover old
process states and modern false-valued startup/stopped/error records.

`cargo test --locked -p testbed_content explorer::ai_client::tests --lib` then
passed **8/8**, including the original assertion and both new regressions.
The terminal log is `target/living-final-client-status.log`. Only
`content/testbed_content/src/explorer/ai_client.rs` changed from the prior
26-file snapshot. The verified manifest was refreshed and the exact previous
manifest preserved at
`target/ai-client-research-frozen-before-legacy-status.json`. Independent source
review accepted the correction. The final shared gate proves the corrected
source, including its current Clippy and workspace compatibility.

## Native and independent review

The first capture at `target/ai-research-native-4d7610ccce21/report.json` proved
server/provider/actor phases and live log/metrics updates in the actor view, with
the saved tick unchanged at zero. It also caught a real launcher PostUpdate
cycle missed by minimal text-input tests. That original failed report remains
intact. A GPU-free real UI/text/focus graph regression, with a negative control
restoring the bad edge, now prevents the startup failure.

Launcher confirmation passed at
`target/ai-research-menu-confirm-0b98069b4ee5/report.json`: Settings, logs and
metrics opened with navigation/footer controls visible. A single visual
formatting pass moved input/output/cost/basis/model/effort ahead of setup
diagnostics; final confirmation passed at
`target/ai-research-menu-confirm-77c2b8d0765f/report.json`. Both final 480- and
960-wide native windows show all five core values in their first body viewport.
Every generated PNG was inspected. Actor narrow shots use a reflected render
resize; actual initial-size launcher captures supply the responsive evidence.
No physical-input or screen-reader QA is claimed.

Lifecycle, observability and native-product subagents reviewed each other's
seams. Their findings led to accepted-identity cleanup, rapid edit/save, actor
phase publishing, IME confirmation and real-schedule ordering fixes. The native
product subagent independently opened and accepted both final metrics PNGs;
there were no further visual changes after that confirmation.

The final compatibility correction changes only missing-field legacy display.
Canonical modern records retain the phase strings covered by the native proof;
the focused presence/false-value regressions and full gate verify that distinction.
All owned windows/processes stopped, with no provider started for these captures.

## Completion audit and limits

| Requirement | Authoritative evidence |
|---|---|
| Same-PC game server and actor control, including headless hosting | Real explorer HTTP/MCP fixtures and server-owned assignment/action validation in the green full gate |
| Logged-in CLI session management, automatic prompts and exact continuation | Provider/session/integration regressions; earlier separately recorded live Codex rehearsal |
| Thorough research, plan and implementation | Linked primary-source report, four implementation streams, ADR and guide |
| Bounded control, recovery and fatal revocation | Transport fault, closed-diagnostic and real-host revocation tests |
| Accepted identity and accounting preservation | Fresh A followed by rejected B cleanup regressions in the client/full suite |
| Markdown logs and JSON metrics/model/effort/cost | Writer, redaction, accounting/provenance and archive regressions |
| Native configurable folders and live history | File-saving/next-start tests and live reader proof at unchanged paused tick zero |
| Native input, focus, IME and startup | Menu suite, real system-graph negative control and successful native captures |
| Subagent-driven improvement and polishing | Cross-agent source reviews and independent final visual acceptance |
| Complete verification on current source | Terminal `ok:true`, all seven gate stages, 501/26 matching source hashes |

No required implementation or verification work remains for this pass. No new
real provider inference was started. Native fake providers exercised the real
host/MCP path; the earlier
[real Codex playtest](2026-11-01-real-native-codex-playtest.md) remains evidence
for its earlier build, not a fresh live-provider claim. Live Claude is not verified.
Synchronous identity/metrics filesystem work still requires responsive storage;
the transcript worker is not a hard real-time guarantee for every filesystem
operation. Remote servers, persistent provider-adapter experiments and stronger
per-MCP connection fencing remain explicitly deferred. The lease is coordination,
not a promise of an OS sandbox against same-user processes. No protected oracle,
global setting, dependency version or repository publication was changed for this pass.
