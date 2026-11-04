# Herp stream increment — execution evidence

Readiness: **GO FOR THE BOUNDED CODEX PLAYTEST — INCREMENT COMPLETE**. Reconciled on 2026-10-30.
The closing source audit confirmed that Codex CLI 0.145 reports session-cumulative
usage across resumes. The existing turn-scope accounting double-counted its first
snapshot. Accounting and explicit legacy-ledger handling are now corrected,
independently reviewed and verified by the final green gate below. The gameplay
proof remains valid; no additional model call or provider-history read occurred.

The separately authorized scoped Codex smoke passed in **92.89 seconds**:
**2 completed turns**, **0 failures**, exact-session event continuation, one
spoken line / physical Diary page / notebook note on **each** turn, **0 archives**,
and all owned resources stopped. No provider history was read in this run.
Evidence: `target/playtest-client-smoke-7my4yxvi/summary.json`.

The final candidate passed `cargo xtask verify --pretty` with **`ok:true`**:
all **7 steps passed**, **1521/1521 tests passed**, **12 skipped**, **1 slow / 2
leaky** classifications; doc tests: **1 passed / 10 ignored**. Architecture:
**7 passed**; replays: **8 verified**; goldens: **6**, none pending; dependency
policy: all passed. No optional verification tool was skipped. The authoritative
report is `target/stream-verify-accounting.json`; release passed in **2m14s**
(`target/stream-release-accounting.log`).

The preceding compatibility candidate passed the full gate with
`ok:true` and **1492/1492 tests passed**. The paired native/browser game-view
confirmation remains evidence for those views. New explicit approval was received and
used for one fresh smoke: the provider SDK completed one turn, but the required
game speech, physical Diary page and notebook note were all absent. Gameplay
acceptance failed; no successful event continuation is claimed. That approval is
consumed and no retry has been made.

Subsequent diagnosis found cancelled MCP registration. The scoped game-tool
approval, cancellation/identity cleanup and native consent-copy follow-up is
implemented, with focused checks, new captures and the final full gate passed.
Earlier style and notebook lifecycle failures and their causal repairs remain
recorded below. The later, separately authorized smoke above completes the live
engineering gate; its allowance is consumed and no additional run is authorized.

Read-only CLI probes subsequently confirmed the effective model `gpt-6.1-sol`
with `high` effort is absent from this installed CLI's visible catalog. Its unique
advertised default is `gpt-5.6-sol`, which supports `high`. Both probes used only
initialization/configuration/catalog methods, started zero threads or turns, and
stopped their owned servers. Evidence is under
`target/playtest-model-catalog-d56f65117f/diagnostics/catalog-report.json` and
`target/playtest-model-config-025251ffef/diagnostics/config-catalog-report.json`.
The client now resolves compatible choices before a turn and applies them only
to its owned CLI process. The metadata-only check below confirms its selection
intent, while global settings and the failed fixture remain untouched. This
does not retroactively supply missing provider-reported metrics or authorize a retry.

## Delivered scope

The user requested a comprehensive plan and subagent-driven next increment for
Herp in Island Village, then added event-aware scheduling, actor dialogue, an
explicit MCP talk action and audience action/thought bubbles. The executed plan
is `../superpowers/plans/2026-10-29-herp-stream-increment.md`; the operator run
sheet is `../island-village-stream.md`.

Work uses the existing `codex/island-first-playtest` checkout, preserving prior
unfinished playtest, world-projection and unrelated Terra changes. No push,
PR, main merge, protected baseline, dependency or global configuration change.

- Herp briefing, perceived economic offers, safe recovery feedback and a
  maintained registered HTTP/MCP rehearsal.
- Three normal-day economy fixtures for seeds 1/7/42, seasonal boundary tests,
  and cancellation, persistence, permanent-death/Diary privacy regressions.
- Host-only event notification queues, closed summaries, exact read cursors,
  bounded coalescing and safe exact-conversation continuation. Rate/backoff
  history survives releasing/reclaiming control. The editor exposes event policy.
- Recorded addressed/aloud dialogue, bounded heard speech, one resident reply,
  explicit registered MCP `talk`, and recipient-scoped scheduler signals.
- Read-only activity captions, safe public action notices and transient thought
  and speech bubbles. Malformed input and notebook text cannot become public
  action labels. Fog, lifetime and caption/HUD placement govern visibility.
- Both native 3D and pixel-art browser presentations consume public feedback.
  Native output flags and host-only handover metadata keep one world feeding
  both views; browser parsing/placement/expiry and display polish have thirteen
  passing tests. Public action text is preserved in transport; only its redundant
  display prefix is removed beneath the action heading.
- A dedicated concurrent AI game client runs alongside the native window and
  browser observer. Focused metrics scope-transition tests and the repaired
  queued-terminal Stop fixture passed; the combined full recheck also passed.
- Start/Resume variants use canonical prompt text and a BOM-free UTF-8 Windows
  copy transport. Mock copy/encoding tests preserve meaningful Unicode; an
  actual Windows PowerShell stdin-to-stdout Unicode round trip passed without
  accessing the clipboard. Templates themselves contained no suspect controls.

Independent source review found and verified corrections for distant resource
stock disclosure, queued commands bypassing rest, bed-wallet overflow, partial
coalesced-event acknowledgments, release/reclaim rate-limit resets, rejected-input
echoes in public feedback and missing cancel/clear action notices.
Later integration found export flags lost during handover and a stop-response
flush race. Further polish corrected camera clamp convergence/current-frame
projection, pan override, speech glyphs/attribution, narrow dialogue priority and
Settings/Worlds footer reachability. Focused results, the completed full recheck
and visual limits follow.

## Intermediate checks (not substitutes for the final gate)

| Command / evidence | Observed result |
|---|---|
| `cargo test --locked -p npcctl --test island_playtest -- --nocapture` (before speech/MCP extension) | 2 passed; registered Herp first sale tick 3404, needs cycle tick 4070, restart/Diary/cancellation and separate death/privacy passed |
| `cargo test --locked -p testbed_content --test island_economy_soak --test island_agent_information -- --nocapture` | 4 economy + 5 then-current information tests passed; three-day seed 1/7/42 fixtures retain all nine residents and exactly 1800 coins |
| `cargo test --locked -p testbed_content --test island_agent_information --test island_scheduler_events --test island_village` | 8 + 4 + 22 passed before the dialogue extension |
| `cargo test --locked -p npc_agent_io --lib scheduler` | 18 passed after notification ACK/rate fixes |
| `cargo test --locked -p testbed_view --lib` | 80 passed, 1 ignored before bubbles |
| `cargo test --locked -p game_menu --bin game_menu worlds::tests` | 11 passed, including editable event-policy default |
| Native pre-change sample | 19.904 TPS, 60.023 FPS / 16.660ms; Windows 11, Intel Family 6 Model 170; 2560×1440 physical / 2× scaling; four baseline PNGs opened |
| Repaired `registered_mcp_stdio_connects_controls_observes_acts_and_keeps_notes` (`target/stream-mcp-repair2.log`) | 1 passed; 13 registered MCP requests against an actual HTTP explorer host; recorded action and notebook round trip completed, credential fields not returned, owned host stopped; `real_provider:false` |
| `island_speech` focused target (`target/stream-speech-focused.log`) | 7 passed: addressed/aloud range, recipient isolation, bounded/nonrecursive replies, invalid text, expiry, registered actor-to-actor dialogue and replay/hash equivalence |
| Mocked explorer-modal copy test (`target/stream-copy-final.log`) | 1 passed after the earlier focused failure; canonical copied text and Copied feedback verified through the mock, without reading/writing the actual OS clipboard |
| Viewer `npm test` after narrow dialogue repair | 13 passed, including exact 188-character Herp speech + Ada reply + thought placement, schema/defaults, reset/despawn, expiry, Unicode wrapping, connector collisions and display-only prefix handling; older archived viewer logs contain earlier 11/12-test runs |
| Viewer `npm run build` after narrow dialogue repair | TypeScript and Vite passed; current built asset `index-CWXr2NfR.js`; existing upstream Zod annotation and large-bundle advisories remain |
| Isolated native sample (`target/playtest-native-741581279d/summary.json`) | Earlier complete run: 19.9295 achieved TPS, 59.9768 FPS / 16.6731ms over 12.042 seconds at 1×; a short sample, not sustained-stream evidence |

Raw intermediate logs use `target/stream-*`; baseline captures and profiler
data are under `target/playtest-native-806cffa0d6/`. Fixtures explicitly separate
operator clock/state arrangements from agent decisions. The opening visitor
fixture stops acting after its economic cycle; it does not prove one hour of
AI survival. The normal day lasts twenty minutes, so seasonal fixtures accelerate
the calendar separately.

## Current visual evidence and automation limits

The earlier complete disposable protocol fixture
`target/playtest-native-741581279d/summary.json` reported **`ok:true`**; all 20
native PNGs were opened and owned hosts stopped. It covered the 188-character
Herp-to-Ada line/reply, permanent death, successor control and public Diary recovery.
Later inspection exposed presentation defects, so this earlier pass is not treated
as proof of the final visual corrections by itself.

An earlier native selective confirmation,
`target/playtest-native-c3afbe82c4/summary.json` reports **`ok:true`**, with six
captures: normal view, strict visible F7 notebook, Herp speech, addressed dialogue,
480-wide dialogue and recovered Diary. All six were opened in the coordinated
review; the coordinator also inspected desktop dialogue and the recovered Diary.
The run confirms the first strict F7 attempt, the licensed speech font/public
speaker labels, fog-aware visibility and narrow dialogue placement. Its owned
native process exited. This is selective confirmation, not a new 20-state run;
the summary lists the other capture states as skipped.

The strict F7 failures in `target/playtest-native-05030c9bbb/summary.json` and
`target/playtest-native-3c200f103e/summary.json` remain preserved as **`ok:false`**
(`Expected visible F7 NOTES UI; hidden text is not capture evidence`). They were
not reproduced in the current selective confirmation. Focus restoration and the
PrimaryWindow guard have focused coverage; one current success does not erase
those earlier failures or establish every possible intermittent-failure cause.

Actual in-app-browser inspection at 1280px desktop and 390px narrow width showed
all three public bubbles: Herp speech, Ada's reply and Herp's thought/action label.
The narrow starvation defect was repaired with speech-first ordering and bounded
obstacle-edge candidates. Public text, geometry and retained stale-state behavior
were inspected separately from the automated harness status.

Final paired confirmation, `target/playtest-native-925c18f471/summary.json`, reports
**`ok:true`** (`target/stream-pair-callback.log`). All six final selective native
PNGs were opened by the coordinator: normal view, visible F7 notebook, Herp speech,
addressed dialogue, 480-wide dialogue and recovered Diary. The protocol fixture
continued through the successor's recovery of the original public Diary, despite
skipping separate death/successor screenshots in this selective capture mode.
Both owned native and browser processes stopped.

The same final world was inspected in the live browser at 1280px desktop and
390px narrow width: both speakers' public speech and Herp's action bubble were
visible. An initial 30-second Cua timeout was recovered without creating a new
world. The final combined call took both viewport captures and visited the
completion callback before the native harness's terminal result. The target-only
checkpoint helper exited 0 with `checkpoint_recorded:true` and
`server_stopped:true`. Here `browser-done` precedes the successful terminal summary
(12:11:50 versus 12:11:54 local), rather than arriving after cleanup. This is
current paired proof; it does not rewrite the failed hold runs below.

The paired browser-hold runs `target/playtest-native-ce4da28105/summary.json` and
`target/playtest-native-dbbb830097/summary.json` both report **`ok:false`** with
`bounded browser check timed out`; each confirms native/browser owned-process
cleanup. In the latter run, `browser-done` was created **after** the terminal
summary/cleanup (10:05:40 versus 10:03:53 local). That late marker is not a pass and
does not change the failed summary. The first browser-hold timeout under
`target/playtest-native-c61ea20187/` is also retained as incomplete evidence.

Native samples in the overlapping `c3afbe82c4` and `ce4da28105` runs are excluded
from performance comparisons. The latest isolated `925c18f471` sample, without
competing test apps/builders, measured **19.98136 TPS, 59.5301 FPS, 16.79821ms**
over **12.061 seconds** at 1×. The earlier isolated `741581279d` sample measured
**19.9295 TPS, 59.9768 FPS, 16.6731ms** over roughly 12 seconds. Resolution was
2560×1440 physical / 2× scaling (1280×720 logical). Neither short sample establishes
30-minute or sustained-stream performance.

Launcher footer corrections have actual screenshot evidence:

- `target/ai-client-final-visual-88c4cdca6a2b/metrics-settings.png`: Settings Save
  and Back visible. Its no-provider report is `ok:true`, with paused ticks unchanged
  and adopted/owned hosts saved and stopped.
- `target/stream-worlds-footer-ready.png`: Worlds AI scheduler page shows Save,
  Revert and Back within the viewport. The bounded form/footer and focus invariant
  test passed; both screenshot-only selector tests passed. Screenshot-mode selection
  does not edit the preset or claim that the visible Save button was physically used.

Background captures were explicitly authorized after the physical Computer Use
path was stopped. These post-fix images are render evidence, not post-fix physical
mouse/keyboard or screen-reader validation. Earlier physical QA verified the Worlds
event switch could flip without saving; its clipped pre-fix capture is not proof
of the corrected footer layout.

## Original provider smoke — failed, no retry

The original disposable Codex engineering smoke stopped on its first provider
failure, within 22.713 seconds of the 600-second/two-turn limit. It completed
**zero turns**. The final public client status is `state:stopped`,
`turns_completed:0`, **`failures:1`**. The original harness summary recorded
`provider_failures:0`; that early counter must not be reported as a successful
provider outcome. Its failure code is `first_provider_failure_no_retry_authorized`.

Evidence: `target/stream-real-client.log`, the disposable run's `summary.json`
under `target/playtest-client-smoke-vrtglnt7/`, and its public `.ai-client/status.json`.
No private provider history was read and no retry was launched. Owned resources
were stopped. The need/event fixture was not applied, the same-session continuation
was not demonstrated and a second turn after that fixture did not occur. This
result does not identify an authenticated-provider readiness pass or establish a
specific provider-failure cause.

## Fresh authorized Codex smoke — model unavailable

The user answered **Authorize one Codex smoke**. The maintained smoke script
created one fresh disposable session, bounded to two turns / 600 seconds, and
stopped at the first failure. Its terminal evidence is
`target/playtest-client-smoke-3ipwn1de/summary.json`:

- **`ok:false`**, Codex, **0 turns completed**, **1 provider failure**.
- Closed diagnostic **`last_failure_code:model_unavailable`**; run failure code
  `first_provider_failure_no_retry_authorized`.
- Elapsed **18.629 seconds**, `owned_resources_stopped:true`,
  `private_history_read:false` and `final_status_sampled:true`.
- No need/event fixture was applied, no second turn occurred and exact-session
  continuation was not demonstrated.

Read-only configuration inspection found selected model **`gpt-6.1-sol`**, effort
**`high`**, and installed **codex-cli 0.145.0**. The fixture's managed MCP
configuration contains **no model override**. These are operator configuration
and CLI metadata, **not provider-reported metrics or proof of model availability**.
For this failed turn, provider-reported model, effort, token usage and cost remain
**unknown**; they must not be populated from configuration, guessed, or recorded
as zero.

The original `vrtglnt7` attempt above retains its separate, unidentified failure;
the new closed diagnostic does not retroactively identify the old cause. Subsequent
read-only initialization/configuration/model-list probes completed without threads,
turns, private-history reads or configuration changes. The resolved pre-turn
selection intent and its metadata-only readiness check are reported below; they
are not evidence that a provider turn has executed successfully.

## Earlier full verification — 1478-case baseline

The release preceding the first full gate completed in **2m36s**
(`target/stream-release-ready.log`), and workspace/all-target Clippy with warnings
denied passed (`target/stream-clippy-ready.log`). The terminal first full-gate
artifact, **`target/stream-verify.json`**, reports **`ok:false`**:

| First-gate step | Terminal result |
|---|---|
| Format | Passed |
| Clippy | Passed, no warnings |
| Tests | Nextest: 1478 run, 1473 passed, 5 failed, 12 skipped; 2 leaky classifications. Doc tests: 1 passed, 0 failed, 10 ignored. |
| Architecture | 7 passed |
| Replays | 8 verified |
| Goldens | 6 checked, none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

The five first-gate failures were the four menu cases below and
`npcctl::ai_client::closed_provider_failure_categories_survive_stop_without_leaking_structured_or_stderr_text`.
The failed artifact remains the record of that run; subsequent focused passes do
not retroactively turn it green.

Focused proof for the combined increment:

| Check | Current evidence |
|---|---|
| Explorer content | 86 passed, 1 ignored (`target/stream-explorer-confirm.log`); the ignored Windows clipboard test is manual. Canonical prompt/copy mocks and the separate UTF-8 stdin/stdout probe do not touch the OS clipboard. |
| Menu AI controls | 4 passed (`target/stream-menu-ai-final.log`), including metrics destination persistence. |
| Camera/bubble geometry and privacy | 14 passed (`target/stream-view-last.log`), including clamp convergence, actual pan override, same-frame root-camera projection, delayed queued actions and private-input canaries. |
| F7 explorer panel / PrimaryWindow guard | 26 passed (`target/stream-f7-final.log`); actual strict visible notebook evidence is the current selective capture above. |
| Dedicated client base checks | 27 unit tests passed and 12 integration cases passed in `target/stream-client-metrics.log`; the remaining queued-terminal Stop regression subsequently passed in isolation (`target/ai-client-queued-stop-confirm.log`, 1 passed / 12 filtered). No real provider was invoked. |
| Latest metrics scope transitions | 13 passed / 17 filtered (`target/stream-metrics-scope-final.log`), including the three added transition cases; exports remain separate from provider history and game state. |
| Worlds bounded scrolling/fixed footer | Regression passed (`target/stream-worlds-footer.log`); actual scheduler-page Save/Revert/Back screenshot is recorded above. |
| Screenshot-only selectors | Both focused cases passed (`target/stream-worlds-capture-test.log` and `target/stream-capture-args-test.log`); selectors do not launch gameplay actions or dirty the draft. |
| Browser | 13 tests and the production build passed after the narrow three-bubble repair; current asset `index-CWXr2NfR.js`. Existing upstream Zod annotation / bundle-size advisories remain. |
| Repaired menu failures | All four passed in 2.346s (`target/stream-menu-repair.log`); the other 125 menu cases were excluded by this focused selector, not removed from the full recheck. |
| Dedicated client integration recheck | All 13 cases passed concurrently in 114.215s (`target/stream-client-pressure.log`, 1 slow, 0 skipped), including the original diagnostic-failure case. Fake providers only. |

The repaired menu cases are:

- `explorer::tests::new_screen_lists_valid_presets_only`
- `explorer::tests::new_screen_names_the_agent_folder`
- `explorer::tests::selecting_world_revalidates_actor_detection_and_launches_both_choices`
- `worlds::tests::worlds_screen_lists_invalid_reasons_and_pages_without_replacing_typed_field`

Direct gate-binary diagnostics showed accessibility-label inserts targeting
buttons already despawned by dynamic picker/editor rebuilds. Ordering those
rebuild groups before accessibility synchronization fixes that deferred-command
race without silencing errors. The folder case instead expected obsolete copy:
its assertions now require the full dedicated-client guidance and exact slugged
folder, retain changing-name coverage, and reject an invalid-name folder route.
Detection, preset-list and typed-field assertions remain intact.

The original client diagnostic failure was **not reproduced** by the focused
old-binary recheck or the concurrent 13-case integration run. Only additional test
failure context was changed; assertions, timeouts and production behavior were
not changed. Its original default-load failure is not explained by these later
passes and remains a stated limitation.

The similarly named `target/stream-client-stop-confirm.log` contains NUL bytes
and is not used as evidence. Historical failed runs and focused repairs remain
separate; isolated passes are not represented as a new combined suite pass.

After format check and a fresh release build, the second
`cargo xtask verify --pretty` completed. The terminal artifact
**`target/stream-verify-recheck.json` reports `ok:true` and every step `ok:true`**;
its companion log is `target/stream-verify-recheck.log`.

| Recheck step | Terminal result |
|---|---|
| Format | Passed |
| Clippy | Passed, no warnings |
| Tests | Nextest: **1478 run, 1478 passed, 0 failed, 12 skipped**; 1 slow and 2 leaky classifications. Doc tests: **1 passed, 0 failed, 10 ignored**. |
| Architecture | **7 passed** |
| Replays | **8 verified** |
| Goldens | **6 checked**, none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

`NEXTEST_TEST_THREADS=4` is scoped to that process as a resource bound; it changes
concurrency, not test cases or assertions, and all full-gate cases are preserved.
This cap is not a causal explanation or a claimed repair of the first run's
unreproduced client failure. No global runner/configuration change is made.

The separate 13-case concurrent client integration pass remains additional evidence,
not a substitute for this full recheck. The original default-load client diagnostic
failure remains unreproduced and unexplained; the bounded recheck is not represented
as proof that the first failure's cause was concurrency or that unlimited-load
stability was repaired.

## Model compatibility batch — earlier verified baseline

The compatibility implementation resolves a supported pre-turn model/effort intent
before starting a provider turn. It preserves compatible configured choices and
uses only a uniquely advertised default when a fallback is needed; uncertain
metadata fails closed. These are owned-process choices, not global TOML edits.

| Compatibility check | Result |
|---|---|
| Client unit checks | **41 passed**, 0 skipped, 13.156s (`target/stream-model-unit.log`) |
| Full client integration target | **16 passed**, 0 skipped, 87.729s, 1 slow (`target/stream-model-integration.log`); fake providers only |
| Focused Clippy | Passed in 50.96s (`target/stream-model-clippy.log`) |
| Release build | Passed in 47.17s (`target/stream-model-release.log`) |
| Real CLI metadata-only preflight | Exit 0, **`ready:true`**, **`model_selection_ready:true`** (`target/stream-model-preflight.json`); no conversation, thread or turn |

That real metadata-only preflight reports selection intent **`gpt-5.6-sol` / `high`**,
with **`model_source:advertised_default`** and **`effort_source:cli_config`**. Installed
CLI 0.145.0 is present and logged in. The read-only evidence verifies the global
TOML, failed fixture private state and failed fixture public status hashes remained
**unchanged**. Neither the old failed conversation nor a new provider turn was run.

Keep three facts separate: the earlier CLI configuration requested `gpt-6.1-sol` /
`high`; the new resolved **selection intent** is `gpt-5.6-sol` / `high`; actual
**provider-reported** model, effort, tokens and cost for the earlier `3ipwn1de`
model-unavailable smoke remain **unknown**. A catalog/default selection and readiness result must not be copied
into usage metrics or represented as successful authenticated execution.

The compatibility-batch terminal full-gate artifact is **`target/stream-verify-model.json`**:
**`ok:true`, all seven steps `ok:true`**.

| Compatibility full-gate step | Terminal result |
|---|---|
| Format | Passed |
| Clippy | Passed, no warnings |
| Tests | Nextest: **1492 run, 1492 passed, 0 failed, 12 skipped**; 1 slow and 1 leaky classification. Doc tests: **1 passed, 0 failed, 10 ignored**. |
| Architecture | **7 passed** |
| Replays | **8 verified** |
| Goldens | **6 checked**, none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

This invocation scoped **`NEXTEST_TEST_THREADS=4`** and
**`NEXTEST_FAILURE_OUTPUT=immediate-final`** to the process. The first bounds resource
use; the second controls failure reporting. Neither removes cases or weakens
assertions. The earlier 1478-case baseline, original failed default-load run and
separate concurrent 13-case client pass remain recorded above. The original
unreproduced client diagnostic failure is still a limitation; the resource bound
is not a claimed explanation for it. The `925c18f471` paired UI proof remains
applicable because this compatibility batch did not change either presentation.

## Latest authorized smoke — SDK success, gameplay acceptance failed

The user explicitly authorized the new one-run smoke with selection intent
`gpt-5.6-sol` / `high`, bounded to two turns / 600 seconds. The maintained script
ran exactly one fresh session. Its terminal artifact is
`target/playtest-client-smoke-_7wyymk7/summary.json`:

- **`ok:false`**, elapsed **47.719 seconds**; **1 provider SDK turn completed**,
  **0 provider failures**.
- **0 initial spoken lines**, **0 physical Diary pages**, **0 notebook notes**.
  Strict acceptance therefore failed with
  **`initial_turn_missing_required_game_artifacts`**.
- `owned_resources_stopped:true`, `private_history_read:false`,
  `final_status_sampled:true`; no retry was launched.
- `need_fixture_applied:false`, `second_turn_after_fixture:false`,
  `same_session_reused:false`: the fixture/second turn were not reached and
  exact-session continuation remains **unproven**, not diagnosed as a wrong session.

The coordinator's allowlisted metrics review recorded **94,806 input tokens**,
**667 output tokens** and **78,720 cached input tokens**, each with **one report
contribution**. Cached input is reported separately; it is not additional input
to add to the input total. Cost is **`null` / unavailable**, not zero. Actual
provider-reported **model and effort remain `null` / unknown**. The selected
`gpt-5.6-sol` / `high` values, sourced from `advertised_default` / `cli_config`,
are still **selection intent only** and must not fill the unknown actual fields.

Completion of an authenticated SDK turn is distinct from completion of the
required game actions. The zero-artifact gate is preserved; no counters,
assertions or required artifacts have been weakened to call this run successful.
The two earlier failed attempts remain separate evidence above. Their outcomes
and this original smoke summary remain unchanged by the later diagnosis below.

## Historical live boundary before the successful scoped smoke

The **1492/1492 passing full gate** describes the candidate used in this smoke;
`925c18f471` remains paired game-view evidence. Subsequent scoped-approval and
runtime cleanup changes have new focused/UI evidence below; their current full
gate is not green because of style-only failures.
The remaining live gate is actual completion of the required game actions and
then exact-session event continuation. The latest one-run approval is consumed;
**no retry or further conversation/turn has been authorized**.

The threadless metadata probe
`target/playtest-mcp-metadata-02443bfaaa/diagnostics/mcp-metadata-report.json`
found all **16 registered NPC tools**, including `talk`, with no configured tool
filters. It took **5.722 seconds**, made no conversation, model turn or game-tool
call, and stopped all owned processes.

The user then explicitly authorized one narrow diagnostic read: only this newly
created smoke's final assistant reply and game-tool names/statuses, with secrets
redacted. `authorized-diagnostic.json` in that same smoke folder records one
`register` call with outer error status **cancelled**, and a final reply explaining
that client cancellation prevented entering the world. No tool arguments/result
bodies or other conversation were inspected; no new model call occurred. This
later, authorized read is separate from the original smoke's historical
`private_history_read:false` field.

Pinned CLI source confirms that noninteractive execution cancels MCP approval
requests. The follow-up candidate keeps conservative tool annotations and adds
default-off, owned-process `--approve-game-tools` for exactly the validated local
NPC server's 16 tools. It preserves filters and rejects explicit conflicting NPC
approval preferences. Native Start explains world-action/private-Notebook scope
before the click. A closed `game_tool_cancelled` diagnostic now stops for operator
attention, even if the SDK subsequently emits terminal success. No new real smoke
is authorized; its conversation budget and scoped game-tool approval must both be
explicitly approved after verification.

## Scoped game-tool approval and cleanup — focused confirmation

Runtime cleanup now retains fatal game-tool cancellation **or polling errors**
from queued messages around the owned provider's teardown, including messages
observed after its `Drop`. Final writes are attempted independently so a write
failure does not erase the fatal result. Cleanup uses separately bounded,
sequential drains; their individual budgets are not a total-shutdown guarantee.

Conflicting conversation identity rolls back to the original identity and remains
a sticky rejection. End-message identity is checked **before usage accounting**,
so foreign-identity usage cannot be attributed to the retained session. Operator
policy is checked at client start: after changing CLI/approval configuration,
**Stop and restart the client**, rather than silently changing the running policy.

| Focused follow-up proof | Result |
|---|---|
| Final client unit checks | **53 passed**, 2.16s (`target/ai-client-identity-fixed.log`) |
| Full client integration target | **21 passed**, 83.78s (`target/ai-client-identity-fixed.log`); fake providers only |
| Native controls | **4 passed** (`target/ai-client-reconnect-native.log`); later backend cleanup did not change the captured consent copy |
| Final backend release before style recheck | Passed in **51.00s** (`target/stream-release-final.log`) |
| Independent scoped source review | Herp review returned **SHIP**, with no remaining scoped P0/P1/P2; this is not a substitute for the full gate or a real gameplay pass |

Two old-behavior negative controls are retained:
`target/ai-client-reconnect-negative.json` and
`target/ai-client-identity-negative.json`. Temporarily removing the fatal cleanup
guard, or swallowing cleanup polling errors, made the existing regressions fail
at the expected second-dispatch assertion (exit 101). Their `ok:true` records mean
the negative controls detected the bad behavior, not that the bad behavior passed.
Both verify source restoration before final checks; source SHA restoration was
confirmed. No assertion was weakened and no real provider ran for those controls.

Current native consent capture
`target/stream-approval-visual-7249cad0a21e/report.json` reports **`ok:true`**.
The coordinator opened all three 1280/760/480 captures, at report scale 2, saved
tick 0, `provider_started:false`, and `owned_native_stopped:true`. They expose the
world-action/private-Notebook permission scope before Start; these are debug-API
render captures, not physical-input or real-provider approval evidence.

Detached capture `target/stream-approval-detached-d6b2b5d8e05b/report.json` also
reports **`ok:true`**. Both scope and **synthetic** closed `game_tool_cancelled`
status images were opened by the coordinator. The report records no provider,
unchanged paused ticks and saved/stopped adopted/owned hosts. A synthetic status
image is not a new real cancellation event.

The first `target/stream-approval-visual-27f0f5c3c30f/report.json` remains
**`ok:false`**, with owned cleanup: the target-only capture helper used
`session/stop`, which is not the native host's quit method. Correcting that helper
to `app/quit` produced the successful `7249cad0a21e` run. This was an automation
cleanup correction, not a product behavior change; the original report stays failed.

## Historical full-gate result — behavior passed, style recheck pending

The terminal artifact **`target/stream-verify-final.json` is `ok:false`**:

| Step | Terminal result |
|---|---|
| Format | Failed: `apps/game_menu/src/detached.rs` needed formatting |
| Clippy | Failed: three style diagnostics, collapsed `if` in `provider.rs` at 402/635 and `as_chunks` guidance in `mcp_config.rs` at 264 |
| Tests | **1511 run, 1511 passed, 0 failed, 12 skipped**; 1 slow and 3 leaky classifications. Doc tests: **1 passed, 0 failed, 10 ignored**. |
| Architecture | **7 passed** |
| Replays | **8 verified** |
| Goldens | **6 checked**, none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

The earlier `target/stream-verify-approval-aborted.json` explicitly records
**`aborted_before_completion`**, `complete_gate_evidence:false`, with the owned
verifier stopped. It is neither a completed gate nor a pass.

The three Clippy issues and detached formatting have been repaired. Formatting
passed (`target/ai-client-final-style-format.log`); focused owner Clippy then
finished successfully in 3.79s (`target/ai-client-final-style-clippy.log`). These
individual results do **not** replace the failed full-gate verdict. A fresh
release/full-gate recheck remained pending at that point; no `ok:true` gate or GO
was claimed for that candidate. The final green gate is recorded above.
No new live authorization or provider calls occurred in this follow-up. Its latest
live outcome remains the 47.719-second failed gameplay attempt and the separately
authorized narrow final-reply/tool-status read. Earlier failed smokes remain
recorded in their own sections above.

### Successor notebook publication repair

The following full gate, `target/stream-verify-ready.json`, passed formatting,
Clippy, architecture, replays, goldens and dependency policy, but reported
**1510/1511 tests passed**, **1 failed**, **12 skipped**, **1 slow / 3 leaky**.
The unchanged registered death/new-life/privacy rehearsal failed with
`this agent has no assigned notebooks`; its failure is preserved in that report.

The exact out-of-process case reproduced in isolation. Credential reload could
publish a successor's assigned actor before the later host frame set its
`NotesDirs` mapping. Four host-only lines now call the existing notebook setup
before publishing the credential. No core API, simulation state or oracle changed.

The new `island_notes_lifecycle` regression makes fresh registration the first
post-`NewLife` request, with no intervening frame, reload or control claim. It
failed before the repair (`target/island-notes-before-fix.log`) and passed after
it (`target/island-notes-after-fix.log`, **1/1**, **0.12s**). It verifies three
fresh usable stores, retained prior-life mapping/privacy, the life-2 path and
unchanged simulation hash. The original CLI rehearsal then passed unchanged
(**1/1**, **2.11s**, `target/island-successor-notebooks-fixed.log`). Independent
review approved the causal fix. Formatting and all-target Clippy passed; Clippy
took **25.05s**. The rebuilt **1512-test** candidate's final gate passed with
**`ok:true`** under `target/stream-verify-lifecycle.json`, as summarized above.
The earlier failed and aborted gates remain historical failed evidence.

The provider smoke is an engineering check, not a 30–45 minute unaided AI economic
playtest. Fake-runner tests, registered scripted clients, screenshots and compilation
provide different evidence and cannot substitute for an authenticated-provider pass.
The successful final paired confirmation also does not convert earlier timed-out
automation runs into passes. Current status: **GO FOR THE BOUNDED CODEX PLAYTEST**.

### Final accounting correction

Pinned CLI source proves that `turn.completed` in Codex **0.145.0** carries
session-cumulative usage restored across resumes. The original exported snapshots
were input **400537 → 1133389**, output **1614 → 3039**, cached input
**349312 → 1039232**. Correct reported session totals are the second snapshot;
second-turn deltas are **732852 / 1425 / 689920**. Cached input is a subset of
input, not an additional amount. Cost and actual reported model/effort remain
unavailable; selection intent does not fill them.

The original smoke summary and exports remain byte-identical. The separate
`target/ai-client-codex-usage-audit-2026-10-30.json` records the arithmetic and
source hashes. It is not a new model run or a rewritten original export.

The correction recognizes only proven Codex 0.145.0 as cumulative; unverified
versions omit numeric usage. Ledger format **2** prevents an older client from
silently reaggregating its fields. V1 Codex migration first preserves exact
canonical/available-export preimages privately, rejects conflicts or preservation
failures, retains identity and lifecycle counts, and marks old numbers unverified.
The first verified cumulative snapshot establishes an unknown legacy baseline;
later deltas are explicitly partial. Typed `reported_session_usage` is separate
from certified tracked totals. V1 Claude values/baselines remain preserved.

Focused checks passed **61/61 unit tests** and **22/22 integration tests**, including
the real snapshots, resume/restart, complete/pruned/interrupted legacy records,
archive failure/retry/idempotence, downgrade refusal, privacy and foreign-identity
rejection. Formatting and all-target Clippy passed; independent reviews approved
the bounded accounting fix. The final release/full-gate run passed with
**`ok:true`**, **1521/1521 tests**, under `target/stream-verify-accounting.json`.
No additional provider call is authorized
or needed for this metadata-only correction; the gameplay proof below is retained.

### Final authorized Codex proof

The user explicitly authorized one new disposable conversation and automatic
approval of only the validated local NPC server's 16 game tools in owned CLI
processes. Global settings, shell/sandbox and other-server permissions were not
changed. `client_smoke.py --authorize-disposable --approve-game-tools` ran once.

Its immutable summary reports `ok:true`, `turns_completed:2`,
`provider_failures:0`, `same_session_reused:true`, `need_fixture_applied:true` and
`second_turn_after_fixture:true`. Initial and resumed turns each produced exactly
one required spoken line, physical Diary page and notebook note. It created no
session archives, read no provider history, and stopped all owned resources.
Elapsed time: **92.89s**, within the **2-turn / 600s** ceiling. No retry occurred.

This is the labeled engineering fixture: the operator injected one recorded
own-actor hunger threshold after turn one, while idle recovery was beyond the
entire smoke budget. It proves real game-tool execution and event-driven
continuation of the same conversation. It does not establish a long unaided AI
economic run, live Claude behavior or physical keyboard/clipboard validation.
Those limits do not retroactively change the earlier failed evidence.
