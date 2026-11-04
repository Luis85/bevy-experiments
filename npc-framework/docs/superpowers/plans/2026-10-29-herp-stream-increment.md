# Herp in Island Village — stream increment

Execution plan for the requested next increment. Build on the current
`codex/island-first-playtest` checkout and its unfinished first-playtest work;
preserve unrelated changes, especially the separate world-projection work and
Terra workspace. No dependency upgrades, protected oracle changes, push or PR.

## Intended experience

Herp is an external AI controlling one ordinary newcomer. Nine autonomous
residents satisfy needs and run the physical supply chains. The audience can
follow their work, Herp's economic choices, failures and recovery. Permanent
death, finite currency and perception remain real constraints. The operator
can start, pause, save and recover the session without editing code.

## Increments and ownership

1. **Baseline and contract — coordinator.** Read applicable rules and prior
   evidence, identify dirty files, run existing tests, record a fresh native
   performance baseline. Reuse existing offers/activity/scheduler contracts.
2. **Economic reliability — economy subagent.** Audit all nine roles; extend
   seeds 1/7/42 through three normal days (one simulation hour), with and without
   an opening visitor trade cycle; test refill boundaries, failed trades and
   finite money. Change balance only for reproduced blockers.
3. **Herp and rehearsal — onboarding subagent.** Clarify declining reserves,
   pending versus completed actions, cancellation and recovery. Maintain the
   registered out-of-process earn/buy/eat/drink/rest/save/resume and separate
   death/privacy fixtures. Supply a stream-specific operator run sheet.
4. **Visible daily life — observer subagent.** Add bounded, throttled activity
   feedback to incumbent actor captions. Preserve fog, label collision rules,
   native controls and read-only presentation. Cover long/narrow layouts and
   unchanged authoritative state.
5. **Integrated readiness — coordinator.** Review diffs and run focused tests;
   build native apps once; capture desktop, narrow, DPI, fog, busy/refusal,
   scheduler, diary and death/successor states. Inspect every capture. Repair
   reproduced defects in one batch and perform one confirmation batch.
6. **Authorized provider smoke — coordinator.** One newly created disposable
   Codex conversation, at most two turns and ten minutes, existing included
   usage only (authorized in this chat). Demonstrate connection, recorded action,
   initial turn completion, exact-session scheduler continuation and a second
   valid action. Never infer a private session or change model/permissions/trust.
7. **Independent review and closeout.** Reuse a finished worker for a read-only
   review of implementation and evidence. Fix actionable blockers; run full
   `cargo xtask verify` with no competing builders/owned test hosts. Record its
   JSON verdict, test counts/skips, native performance and remaining limits.

### Added requirement: event-aware AI scheduling

The user explicitly added event dispatch during execution. Extend the generic
host supervisor, without simulation registrations or a content dependency:

- Route meaningful own-actor outcomes/needs/failures and public phase changes
  from the existing event ring to the actor's currently connected controller.
- Deduplicate by event sequence, bound and coalesce pending batches, and expose
  safe delivery counts/triggers in read-only scheduler status.
- A pending event can trigger the exact linked conversation before its idle
  threshold. Pause, actor work, active provider turns, control revocation,
  cooldown, concurrency and rate caps still govern dispatch.
- Include historical seq/tick/kind notifications in prompt stdin; the AI must
  observe again before acting. Never include arbitrary event payloads, another
  actor's state, private notebooks, credentials or provider session IDs.
- Add a preset/editor event switch, route known island outcomes through a
  generic host adapter if required, and cover isolation, deduplication, queues,
  suppressed dispatch and actual fake-runner prompt delivery with tests.

The scheduler worker owns generic implementation/tests. The coordinator owns
configuration UI, documentation/ADR and integration; the independent reviewer
reviews the expanded feature before the final gate.

### Added requirement: speech and audience action bubbles

The user added actor dialogue and visible AI action bubbles during execution.
The content worker owns recorded bounded `talk` speech, direct/aloud hearing,
nearby actor affordances and privacy/determinism coverage. The coordinator owns
an explicit generic MCP `talk` shortcut through ordinary registered `agent/act`
and bounded public accepted-action notices. The view worker owns transient
thought/action and speech bubbles, fog/layout/expiry and read-only tests.

Speech text is a public, single-line utterance (240 characters / 1024 bytes),
not a private notebook. An addressed actor must be within ordinary reach;
aloud speech has a fixed 6m hearing radius. Recipient-scoped journal events
provide heard dialogue without giving distant actors private speech. The event
scheduler receives only a closed incoming-speech signal, and the actor reads the
dialogue through normal observations/events. Thought bubbles label actual
submitted public actions; they never fabricate or expose internal reasoning.

Re-run focused content/MCP/view checks, extend the native capture matrix with
speech/action bubbles, and review the new paths before the final full gate.

### Added requirement: clean copied Start and Resume prompts

The user reported invisible characters after copying both prompt types into
Claude. Audit templates and final render/copy paths without reading the user's
clipboard. Remove encoding markers from Windows copy transport, normalize
generated prompt formatting while preserving Unicode spelling/meaningful space,
and apply the same boundary in native and detached prompt copy flows. Refresh
the handbook's memory/discovery/continuation wording to match the implementation.
Verify all four prompt variants, UTF-8 stdin payload and mocked copy equality;
do not claim an actual clipboard round trip from pure transport tests.

## Acceptance and evidence

### Follow-up from the authenticated model-access failure

The October 30 authorized smoke stopped at its first failure with
`model_unavailable`, zero completed turns and owned-resource cleanup. Its summary
is preserved under `target/playtest-client-smoke-3ipwn1de/`. Read-only CLI metadata
confirmed the effective configured model was absent from the complete visible
catalog, which advertised one usable default and compatible reasoning choices.

The dedicated client owner will add bounded pre-turn configuration/catalog
resolution in the exact workspace. Preserve an available configured choice;
otherwise select the unique visible advertised default and apply it only to the
owned process. Preserve supported effort or use its advertised default. Unknown,
incomplete or ambiguous metadata must stop setup without trial conversations,
global configuration changes or automatic failure retries. Selection intent must
remain separate from provider-reported model, effort, usage and cost.

Verify fake-provider compatibility/failure cases, read-only real-CLI preflight and
unchanged global configuration, then rerun the full gate for the changed candidate.
Further authenticated action and exact-session event-continuation proof requires
fresh authorization; the failed attempt's allowance is consumed.

### Follow-up from the cancelled registration

The next separately authorized attempt completed one SDK turn but produced none
of the required game artifacts. Its immutable summary is preserved under
`target/playtest-client-smoke-_7wyymk7/`. A threadless metadata probe found all 16
NPC tools and no tool filters. With a subsequent explicit, one-conversation
diagnostic-read authorization, the coordinator inspected only this new smoke's
final reply and game-tool names/statuses: its `register` call was cancelled.
No tool arguments/results or other conversation were inspected.

Pinned CLI source explains the boundary: noninteractive execution cancels MCP
approval requests. Keep conservative tool annotations truthful. Add a default-off
`--approve-game-tools` option restricted to the validated registered local NPC
server's 16 tools in owned Codex processes. Preserve filters, reject conflicting
explicit NPC approval preferences, and change no global, shell or sandbox policy.
Native Start must state its world-action/private-Notebook scope before the click.
Also recognize a cancelled NPC tool as a closed, sticky failure and stop for
operator attention rather than treating a later SDK terminal event as success.

Verify default-off/new/resume scope, conflict guards, privacy and cancellation
through fake providers; review the native permission copy, rebuild and run the
full gate. Any further real smoke requires fresh authorization for both the
disposable conversation budget and the bounded NPC-tool approval option.

### Follow-up: successor notebook readiness

The next full gate exposed a successor whose registered identity was available
before its notebook stores. Reproduce this without delays: apply `NewLife`, make
fresh registration the first host request, then access all three books before
any host frame or control claim. Assign life-specific stores before publishing
credentials, preserve prior-life mapping/privacy and prove the simulation hash
does not change. Keep the original out-of-process death/recovery test unchanged.
Run formatting, all-target Clippy, release builds and the full gate again.

- Registered Herp discovers ordinary opportunities, earns coins, purchases and
  uses bread/water/lodging, and resumes with the same inventory and identity.
- Rejected/cancelled work produces no phantom inventory or money. Restart and
  succession require valid control; only public Diary content survives death.
- Seeds 1/7/42 retain all residents, conserve money and demonstrate production
  through stream-length fixtures. Operator fixture intervention is labeled.
- Actual UI captures show useful daily-life/activity feedback at 1280×720
  logical and 480-wide stress layout; record actual viewport and DPI too.
- Native unpaused 1x sampling records achieved TPS/frame timing on this machine.
  Accelerated headless fixtures are behavior evidence only.
- A real-provider pass is distinct from fake runners and scripted clients.
  Missing authentication/trust/provider evidence is a live readiness blocker.
- Full gate must report `"ok":true`. Never silently rebaseline or weaken policy.

Sanitized outcomes belong in `docs/_notes/2026-10-29-herp-stream-closeout.md`;
raw logs, captures, credentials and disposable saves stay under ignored `target/`.
The run sheet belongs in `docs/island-village-stream.md`. A GO claim requires
all gates; otherwise name the outstanding blocker and reproducible recovery.

## Executed closeout — 2026-10-30

All implementation and polishing increments are complete. The final candidate's
full gate is **`ok:true`**, **1521/1521 tests passed**, **12 skipped** (1 slow / 2
leaky classifications), with all architecture, replay, golden and dependency
checks passed. Native/browser presentation and stream-length deterministic NPC
economy fixtures are documented in the closeout note.

The final accounting audit also corrected pinned Codex 0.145 cumulative usage
across resumes. Version-2 ledgers preserve private legacy preimages, mark unknown
attribution explicitly and separate reported session totals from tracked deltas.
Exact exported-snapshot regressions and independent reviews passed; the original
live artifacts remain unchanged. The current final gate is
`target/stream-verify-accounting.json`.

The separately authorized scoped Codex rehearsal passed **two turns in 92.89s**:
both turns spoke, wrote a physical Diary page and added a notebook note. The
second followed the recorded need fixture in the exact same conversation, with
zero failures/archives and owned cleanup. No provider history was read. This is
bounded engineering proof, not a long unaided AI run or live Claude validation.
No additional provider conversation is authorized by completing this plan.
