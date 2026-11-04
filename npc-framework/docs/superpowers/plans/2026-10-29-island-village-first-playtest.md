# Island Village: first-playtest readiness

Status: proposed execution plan, not implemented by this planning session.

## Goal and scope

Make the current village ready for a first **AI-controlled** playtest on Windows: an operator creates a
world, connects one external AI, watches it earn a living, and can recover from interruptions. Keep the
current KayKit art/native UI, permanent lives, persistent economy and scene-configurable CLI supervisor.

Default first playtest: one human-bodied visitor, one explicitly chosen AI provider, normal 1x simulation,
and a 30–45 minute observed session. Testers have the selected CLI installed and signed in. Installation
and provider login are prerequisites, not part of the ten-minute setup target below. A shipped day is
20 minutes; seasons need separate accelerated fixtures rather than waiting seven hours for winter.

Do not expand this into multiplayer, multiple LLM-driven villagers, dynamic pricing, a new art direction,
a general quest system, new production chains or a scheduler service rewrite. Fix demonstrated blockers
and add the minimum feedback and tooling needed to run and diagnose the first session.

## Starting evidence

Last verified baseline: local main `a1b65ba`, source merge `0fd4744`. Recheck HEAD/status at execution;
these references identify evidence, not a request to reset the checkout.

- `docs/ai-village.md`: current launch, economy, needs, death/diary and scheduler behavior.
- `docs/_notes/2026-10-29-island-village-review.md`: 1,313 tests passed, 12 skipped, 3 nextest leaky
  classifications; architecture, eight replays, six goldens and dependency policy passed.
- Native normal/narrow/fog/diary/death captures and an owned fake CLI continuation loop passed.
- Authenticated provider continuation and a cold real AI playing the whole loop remain unverified.
- Existing economic tests use privileged fixture setup and known IDs in places. They are valuable unit/
  integration evidence, but do not prove a new registered AI can discover and play the game unaided.
- Some reproduction scripts currently live under ignored `target/`. A fresh session must not depend on
  those files existing: promote useful checks into maintained tests/tooling with documented prerequisites.

Read root and nested `AGENTS.md`, `CLAUDE.md`, relevant `.claude/rules/`, ADRs 0016–0017 and 0022–0023.
Existing plans are historical context. This document defines the requested new work.

## Readiness contract

These are proposed acceptance targets for this first playtest, not claims about current performance.

1. **Cold setup:** an operator with an installed/signed-in provider reaches a controlled, observable actor
   in at most ten minutes using the runbook, without editing Rust, looking up entity IDs or reading code.
   Paused world, queued control claim and disconnected/unlinked CLI states explain their recovery.
2. **Playable economic loop:** through registered game tools and perceived affordances, the AI gathers
   or crafts, sells useful output, buys food/water/lodging and consumes/rests. Target a first sale within
   ten real minutes and the first needs cycle within one normal simulation day. Record simulation time,
   model/tool latency and operator intervention separately; investigate misses rather than hiding them.
3. **Observable play:** the operator can tell what the actor is doing, what it can afford, why an action
   failed and whether the next step belongs to the operator, the AI or the scheduler. No hidden NPC stock,
   wallet or location is revealed to the AI to accomplish this.
4. **Working continuation:** one explicitly authorized disposable CLI session connects, finishes a turn,
   becomes idle and is resumed in that same conversation. Its resumed turn issues a valid game request.
   No overlapping turn, wrong conversation, model/permission override or unrelated process termination.
5. **Recovery:** save/stop/resume preserves the world and inventory; a separate controlled death fixture
   proves permanent death, new identity/control handshake, fresh private books and recovery of the old
   public diary. Do not kill the main live actor just to demonstrate this branch.
6. **Sustainable/diagnosable:** all nine production roles remain usable over a normal-day soak and seasonal
   regression fixtures. There is a viable way to earn/spend; no currency duplication or recurring visitor
   subsidy. At 1x, the unpaused host sustains its configured 20 ticks/s on the recorded test machine.
   Record render/frame timing and compare with baseline; fix persistent stalls, not the profiler budgets.
7. **Reproducible result:** a sanitized evidence bundle, exact build/test commands, a tester run sheet and
   an explicit GO/NO-GO decision exist. A fake runner alone cannot satisfy the real-provider gate.

## Execution model for subagents

Use the collaboration subagent tools, not new user-owned chats. Use at most **three active workers plus
the coordinator**. Inherit the session's model/settings; no model overrides are required.

The coordinator owns git operations, shared contract decisions, Cargo builds, native app processes and
final integration. Workers edit only their assigned paths. They return patches, tests, evidence and
unresolved risks; they do not switch branches, merge, run broad gates, launch providers or change other
workers' files. Because agents share this checkout, file ownership is mandatory. A worker needing a
shared file requests a coordinator edit or an explicit ownership transfer first.
Workers do not spawn nested teams; request any additional review through the coordinator's slot budget.

Keep one Cargo builder, especially for Bevy. Do not give every agent its own cold build or repeatedly
toggle feature combinations. Workers may request focused checks; the coordinator batches compatible
checks. No test game or release build runs beside the final full verification gate. Use an isolated
worktree only if current checkout concurrency makes it necessary, preserving this plan and its own
build output. Never stash/reset the monorepo or disturb unrelated Terra changes.

Every worker gets this common contract:

> Implement only the assigned package. Read its source and applicable repository rules. Reproduce
> behavioral defects before fixing them; add meaningful regression coverage. Preserve integer state,
> sorted SimIds, recorded inputs, fog/ownership/privacy and legacy schemas. Do not change protected
> oracles, generated lint policy, dependencies or global configuration. Report files changed, exact
> checks requested/run, evidence and remaining blockers. Do not run Cargo or launch apps/providers
> independently; the coordinator owns those resources. Ask the coordinator before crossing file scope.

## Phase 0 — coordinator: establish the contract and baseline

1. Inspect current checkout/status and preserve unrelated changes. Create `codex/island-first-playtest`
   if a new branch is appropriate. Do not reset to the historical baseline or automatically merge main.
2. Reproduce the current path with disposable agent/world folders and free ports: New world → start
   prompt → operator resumes the world → registration/enter/control → observe → first useful action.
   Never advance ticks from the agent client merely to complete a pending control claim.
3. Write a short prioritized gap list. Distinguish reproduced defects, missing evidence and optional
   improvements. Reuse existing working behavior instead of rebuilding it.
4. Freeze the small data contract before parallel edits: perceived merchant identity; actor-perspective
   buy/sell direction, kind, integer unit price, stock/demand and concrete availability/refusal reason;
   current actor activity/last result; supervisor eligibility and recovery instruction. Extend existing
   observations/affordances additively. A large new generic API or UI dashboard is unnecessary.
5. Keep one source for prices/costs and rules. The UI, prompt and agent data must not drift into separate
   hardcoded copies. Names and prices can be public; hidden map truth and private provider data cannot.
6. Request the provider choice, disposable-session authorization and usage cap needed for the later live
   smoke now. Continue all independent offline work while waiting; the detailed boundary is below.

Deliverable: `docs/_notes/<date>-island-first-playtest-baseline.md`, including acceptance targets,
fixture commands, ownership assignments and the chosen primary viewport/provider prerequisites.

## Wave 1 — three implementation workers in parallel

### A. Agent onboarding and actionable game information

Own: `content/testbed_content/src/village/agent.rs`, explorer prompt/workspace rendering and their
focused tests. Coordinate edits to `explorer/host.rs` or shared structures through the coordinator.

- Give an island-specific opening objective: earn a living, satisfy needs, keep a Diary, and respect
  permanent death. Explain tool-only play and where the operator must resume/connect. Preserve generic
  explorer prompts for other worlds; do not supply hidden coordinates, IDs or an optimal route.
- Make observed/afforded opportunities sufficient to discover a first sale and a needs purchase. Show
  useful perceived merchant offers/demand, actor-perspective prices and concrete rejection/recovery
  reasons. Deduplicate repetitive entries and keep observations bounded.
- Explain busy/walking/gathering/crafting/resting versus idle, and how to cancel or retry when a target
  moved or a shop has no stock/funds. Reuse existing command validation as the behavioral authority.
- Add registered-interface tests, including negative fog, ownership and unavailable-action cases.

Accept: a scripted client can choose its next action from normal returned information without source
inspection; prompts for legacy worlds are unchanged; no new world-knowledge leak.

### B. Economy and survival reliability

Own: `content/testbed_content/src/village/{ai,economy,catalog}.rs`,
`content/testbed_content/tests/island_village.rs`, and new economy-specific test modules. Any change to
the shipped `island_village.json`, shared commands/model or price contract requires coordinator review.

- Run seeded soaks (at least seeds 1, 7 and 42) with a scripted visiting actor trading as well as NPC-only
  production. Include a normal-length day and accelerated seasonal fixtures; record population health,
  outputs, inventories, transactions, currency total and periods of unavailable necessities/buyers.
- Exercise first-income demand, depleted stock, poor merchants, input shortages, night/rest, interrupted
  crafting and a moving trader. Distinguish waiting for replenishment from a genuine production deadlock.
- Probe self-supply/free gathering and repeated trade paths. Document intentional alternatives; fix
  unambiguous duplication/conservation/authorization failures. Do not invent access laws, taxes, prices
  or a mandatory purchase restriction merely to force a scripted route.
- Tune only reproduced readiness blockers. Preserve finite currency and zero visitor starting coins;
  do not add a refill/stipend, suppress death or weaken need decay just to make the test pass. Preserve
  immutable saved presets and explain any behavior/content-manifest compatibility implications.

Accept: a viable first trading route and all production stages are demonstrated; money is conserved;
observed stalls have either a working recovery path or a documented, deliberately deferred balance issue.

### C. Native observer feedback and playtest usability

Own: `content/testbed_view/src/village.rs`, `village_world.rs` and focused view tests. The coordinator
owns changes to shared menu/launcher files. Keep current art, focus conventions and label placement.

- Consume the frozen contract to make current activity, need urgency, cash and last action/refusal
  understandable. Add a compact contextual merchant view only if existing feedback cannot support the
  playtest. Keep private/debug world truth out of AI-facing observations.
- Make pause, queued control, unlinked provider, idle countdown, running/cooldown and actionable errors
  distinct. Keep the one-time exact-session linking requirement explicit; do not infer a conversation.
- Check diary navigation, permanent death/New life, reconnect guidance and long names/error strings.
  Preserve tab/focus/disabled semantics, bounded scrolling, label decluttering and fog behavior.
- Use actual catalog/configuration values, throttle observational refresh and keep UI state out of sim
  hashes. All gameplay changes remain recorded commands.

Accept: a new operator can explain the actor's state and next responsible party from the UI. Validate a
primary desktop viewport (default 1280×720 logical), the actual user viewport if available, 480-logical
width stress layout, and representative DPI scaling. Capture states together, fix in one batch and use
one confirmation batch; do not perform an open-ended redesign/polish loop.

## Wave 2 — scheduler and maintained rehearsal

Start after Wave 1 integration/data-contract review. Run D and E in parallel with disjoint files;
reserve the third worker slot for the subsequent independent review. Reuse idle workers if appropriate.

### D. Operator setup and continuation preflight

Own: `apps/npcctl/src/scheduler.rs`, `crates/npc_agent_io/src/scheduler.rs` and focused tests. The
coordinator integrates any host/UI changes, avoiding conflicts with A/C.

- Rehearse linking, pause/busy suppression, idle → running → cooldown, failure/backoff, restart/reset,
  unlinking and changed actor/session. Preserve exact named-session selection and owned-process cleanup.
- Make provider/program/workspace/link errors diagnosable using existing status/bind paths; add a small
  read-only preflight only if those paths cannot provide it. Checking readiness must never launch a turn.
- Verify installed CLI help before changing adapters. Do not change provider permissions, model, global
  configuration or select `--last`. Interactive clients still need an explicit finished-turn signal.
- Cover stale link/lock, missing CLI, logged-out provider guidance and timeout without exposing tokens,
  conversation UUIDs or transcripts. Never kill an original client or a pre-existing game/MCP host.

Accept: the operator can establish and troubleshoot one link from the runbook; fake-runner regression
coverage remains green; each failure state identifies an appropriate recovery.

### E. Maintained registered-client rehearsal and playtest kit

Own: new maintained files under `apps/npcctl/tests/` and, if needed, `scripts/playtest/`; a new
`docs/island-village-playtest.md`. Reuse existing test fixtures. Do not edit production modules or the
other workers' tests. Coordinate run requests through the central builder.

- Turn useful ignored reproduction scripts into supported, documented tooling. Do not hardcode this
  user's bundled Python path, credentials, private folders or numeric entity IDs. Prefer existing Rust
  integration-test helpers for headless checks; document any native-capture tool prerequisites.
- Exercise the **registered**, out-of-process path: new disposable world, credentials, enter/control,
  perceived discovery, gathering/crafting, sale, food/water/bed purchase and actual consumption/rest.
  Actor decisions use normal game tools; no direct component edits, teleportation, free items or wallet/
  need injection. A separate operator harness may drive its test clock; the agent cannot bypass pause
  or authority to finish control claims. Record who performs each privileged fixture operation.
- Add separate save/stop/resume and death/new-life/diary/privacy fixtures. Forced death or accelerated
  seasons are allowed only in those labeled disposable fixtures, not as proof of an unaided live run.
- Provide a 30–45 minute tester run sheet: setup checklist, briefing, tasks, observation questions,
  restart/recovery steps, issue template and clear stop criteria. Track interventions, stuck time,
  tool rejections/retries, first sale/purchase times, needs/deaths, nudge count and achieved TPS/frame data.
- Produce a sanitized, bounded evidence manifest with build revision, seed, immutable preset, commands,
  journal references and screenshots. Reuse existing journal/profiler/export facilities. No new telemetry
  platform; no tokens/session IDs/private Memory/Proposals/transcripts in committed or public artifacts.

Accept: another session can run the rehearsal from committed tooling/docs, using only ignored disposable
state, and obtain a machine-readable pass/fail summary with owned hosts stopped afterward.

## Wave 3 — F. Independent first-playtest review

Start only after D/E have been integrated and the coordinator has completed the integrated native and
rehearsal/live-smoke steps below. If live authorization is absent, the reviewer must score that evidence
gap as BLOCKED rather than assuming a fake-runner result covers it.

Read-only reviewer, fresh context. Receive requirements, changed-file list, test results, screenshots and
evidence manifest; do not inherit implementation discussion or run competing builds/apps.

Review against the readiness contract, especially cold onboarding, hidden knowledge, economy recovery,
private notes/control succession, exact-session targeting, process ownership and misleading feedback.
Return only actionable P0/P1 playtest blockers, P2 improvements and evidence gaps, with file/line references.
Do not reopen the art direction or demand unrelated new features. The coordinator fixes all P0/P1 items
in one integration batch and requests a verdict on those fixes. Deferred P2 items need a rationale.

## Integration and final verification — coordinator

1. Review each worker's diff and evidence. Resolve shared-file changes centrally; run meaningful focused
   tests, then build the native apps once for the integrated capture batch. Never weaken an assertion,
   bypass a policy hook, raise an existing budget or silently rebaseline a protected oracle.
2. Walk the actual launcher/explorer path, not only standalone `ai_village`. Inspect normal, narrow,
   fog, paused/unlinked, busy/error, diary and death/reconnect states. Open every PNG. Record which inputs
   were protocol/widget automation and which were physical; do not claim screen-reader/physical QA from
   compilation or screenshots alone. Any UI skill workflow must honor the fixed incumbent visual scope.
3. Confirm the chosen provider and explicit permission for a **new disposable playtest CLI
   conversation**, bounded wall-time/spend and turn count. Continue offline work while waiting. This
   plan authorizes planning/implementation, not resuming a private existing conversation or buying usage.
4. With that authorization, run a short authenticated engineering smoke: connect/control, make a valid
   game action, finish the original turn, link its exact UUID, let the actor become idle, and observe
   the same conversation resume and issue another valid game request. Confirm no overlap and stop owned
   resources at the agreed cap. If inputs/permission are absent, mark the live gate BLOCKED, never passed.
   One provider is sufficient for the first playtest; other adapters remain an explicitly unverified scope.
5. Run focused checks before the final gate, as applicable:

   ```powershell
   cargo test --locked -p testbed_content --test island_village
   cargo test --locked -p npc_agent_io --lib scheduler
   cargo test --locked -p testbed_view --lib
   # Run the maintained registered-client rehearsal added by E.
   cargo build --locked --release -p actor_testbed -p village -p rts_units_demo -p game_menu -p npcctl --bins
   ```

6. Obtain F's independent review/verdict after the integrated evidence exists. After its corrections,
   stop disposable hosts/runners and concurrent builds. Run
   **`cargo xtask verify`**, not just `--quick`, and require JSON **`ok: true`**. Record the exact counts,
   skips, ignored/leaky classifications and tool availability. Protect all user processes/workspaces.
7. Leave reviewable, validated changes on the task branch with a clean NPC Framework scope. No push,
   PR, main merge or monorepo-wide cleanup unless separately requested. Do not treat historical local
   merge/release ledgers as authorization for repository operations in this new session.

## GO / NO-GO and deliverables

**GO** requires the readiness contract, no unresolved P0/P1 issues, the maintained rehearsal passing,
current native evidence, a successful explicitly authorized real-provider connection/continuation smoke,
the tester run sheet, and the full gate passing. The longer observed first playtest can then be scheduled
or run by the operator; do not create a recurring automation merely because the game has a scheduler.

If the live smoke is missing, report **OFFLINE READY / LIVE SETUP BLOCKED**, not GO. If economic or
control/persistence blockers remain, report **NO-GO**, with a short prioritized recovery list. Do not keep
adding optional polish to avoid resolving a blocker or to avoid declaring an external prerequisite.

Final handoff includes: concise changes; known limits/deferred P2 issues; reproducible launch/rehearsal
commands; exact verification results; sanitized evidence; and the first-playtest run sheet. Separate
what was observed from what is inferred or still untested.

## Fresh-session start prompt

> Work in `C:\Projects\bevy-experiments\npc-framework` and execute
> `docs/superpowers/plans/2026-10-29-island-village-first-playtest.md` through its readiness decision.
> Use collaboration subagents: at most three active workers plus you, with the plan's file ownership,
> dependency waves and independent reviewer. You own shared-file integration, Cargo/native processes and
> final verification. Recheck current HEAD/status; preserve unrelated changes. Keep the current native
> art/style and deterministic simulation boundaries. Implement and verify the focused improvements;
> do not stop at another plan or report. Request the provider/session authorization and usage cap only
> for the disposable authenticated smoke; complete independent offline work while waiting, and never
> infer or resume an existing private AI conversation. Require `cargo xtask verify` JSON `ok: true` and
> report GO, OFFLINE READY / LIVE SETUP BLOCKED, or NO-GO honestly. Do not push, open a PR, merge main,
> weaken tests/policy, rebaseline protected oracles or touch unrelated projects without explicit approval.
