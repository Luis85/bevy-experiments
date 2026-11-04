# Dedicated AI-Agent-Game-Client

## Product decision

The user supplies a logged-in Codex CLI or Claude Code CLI. The game and a dedicated
client supply the agent folder, MCP tools, initial prompt and every continuation.
No copied prompts, conversation IDs or manual scheduler bindings are required.
The client is a separate executable with native game controls and a scriptable CLI.
It uses persisted conversation continuity between provider processes, with no paid
keepalive prompts. Existing local subscription authentication remains provider-owned.

## Product review and invariants

- Player experience: select provider, start, inspect status and stop from existing
  native explorer and detached-world surfaces. Missing installations/login produce
  actionable messages. Stopping the agent leaves the game running.
- Gameplay: observe and act through the registered MCP interface; no operator
  polling may acknowledge events, take control or advance simulation time.
- Architecture: the game owns deterministic simulation and authorization; the
  client owns external processes, provider sessions and orchestration. Host leases
  prevent concurrent legacy scheduler/client dispatch.
- Reliability: exact session resumes, bounded events/output/retries/timeouts,
  fail-closed revocation, actor/world change handling, owned-process cleanup and
  explicit restart behavior. Persist state privately, use exclusive client locks.
- Privacy: never expose credentials, raw provider output or conversation IDs in
  public status/logs/prompts. Do not alter global provider settings, models or
  permission policy. Reuse generated agent workspaces and preserve owned files.
- Delivery: retain current checkout and unrelated changes. No protected oracle,
  lint, CI or tooling edits, dependency upgrades, pushes or PRs.

## Implementation sequence and ownership

1. Host integration (architecture subagent): generic `client/claim`, `client/poll`,
   `client/release` host-only RPC, bounded classified event feed, current activity,
   eligibility and policy snapshot, renewable exclusive leases and regression tests.
2. Client runtime (provider subagent): dedicated `ai-agent-game-client` binary in
   the npcctl Cargo package; preserve default npcctl invocation. Commands `run`,
   `check`, `status`, `stop`, using `--workspace DIR` and `--provider codex|claude`.
   State lives in `DIR/.ai-client/`; own initial and exact resumed CLI turns,
   parse structured lifecycle output, limit and cancel processes, update sanitized
   status atomically, handle stop requests and game discovery through `run.json`.
3. Native integration (primary): shared explorer client launch/status helper,
   provider/start/stop/status controls in windowed onboarding and detached menu,
   replacing the requirement to copy an initial prompt. Follow incumbent native
   button, keyboard, palette and layout conventions.
4. End-to-end coverage and docs (product subagent): fake provider processes against
   real game host; new/resume, events, ownership, failure, stop/restart and redaction;
   operator guide and migration notes. Do not run paid turns as an automated test.
5. Independent improvement pass (all subagents): cross-review implementation for
   product/UX, session reliability, privacy, security, architecture, operability,
   documentation and tests. Implement actionable findings, then targeted retest.

## Acceptance and verification

- A generated explorer workspace and a running game are sufficient to start an
  installed logged-in provider from native controls, without user prompt handling.
- Initial conversation identity is captured automatically and every later turn
  targets that identity. Concurrent clients/legacy runners cannot overlap ownership.
- Events during turns are deferred. Work/pause/revocation/death gate continuations.
- Client status is understandable and redacted; stop releases only owned resources.
- Process-level fake-provider tests and focused Cargo tests pass; native UI receives
  an actual local playtest/screenshot review. Authenticated provider gameplay is
  separately reported and never inferred from fake-provider tests or compilation.
- Run `cargo xtask verify` once at the end, report exact JSON verdict and skipped
  checks. Preserve any pre-existing failures with evidence rather than weakening gates.

## Added user requirements: persistent session metrics

The user additionally requires per-session JSON metrics in a configurable folder,
including costs, input/output tokens, model and reasoning effort.

- Extract only allowlisted usage and model/effort metadata from each provider's
  structured lifecycle output (and documented startup metadata where necessary).
  Never retain transcripts, tool payloads or credentials in metrics.
- Preserve provider-reported cost semantics: unavailable subscription costs are
  null with an explicit availability/source field, never invented from pricing.
  Reported API-equivalent cost is distinguished from an actual subscription charge.
- Persist aggregate metrics across exact-session resumes/restarts; failed turns
  retain any reported usage, and missing usage remains distinguishable from zero.
  New conversations receive independent metrics. Model/effort changes remain visible.
- Configure the destination with `--metrics-dir`, an operator environment default,
  and native menu settings; no changes to immutable world presets or global
  provider configuration. Process-local selection is refined by the runtime
  finding below.
- Use atomic JSON writes and private files. Keep a canonical local ledger so a
  destination change does not discard the ongoing session's previous totals.
- Add fake-provider, persistence, redaction and compatibility regressions. Verify
  configurable-folder behavior without consuming any additional model turn.

Implementation contract: export `<metrics_id>.json` using a private local
32-hex identity, with a canonical `.ai-client/metrics-ledger/` record. Destination
precedence is `--metrics-dir`, `NPC_AI_METRICS_DIR`, then workspace-private
`.ai-client/metrics`; native settings supply a process-local preference. Include
explicit cost availability/basis, nullable reported totals and contribution
counts, current metadata provenance and a bounded 64-turn history. Claude
session-cumulative usage/cost is differenced against persisted baselines;
old-session adoption does not invent past deltas. Codex missing dollar values
remain null; Claude reported estimates retain API-equivalent labeling.

Acceptance fixtures exercise nonzero usage and costs through two turns plus
restart, a changed destination, a new independent session, failed reported
usage, missing-field nulls and metadata changes. No real provider turn or replay
of the failed 22.713-second smoke is authorized by this work; original evidence
is preserved and authenticated gameplay remains unproven.

## Sources checked during provider review

- https://learn.chatgpt.com/docs/non-interactive-mode
- https://learn.chatgpt.com/docs/auth
- https://developers.openai.com/blog/codex-as-a-platform
- https://code.claude.com/docs/en/headless
- https://code.claude.com/docs/en/authentication
- https://code.claude.com/docs/en/cli-reference

Installed help reviewed: Codex CLI 0.145.0 and Claude Code 2.1.281.

## Runtime finding: unsupported inherited Codex model

The fresh authorized real run stopped at `model_unavailable`, before any completed
turn. Read-only metadata confirmed that the effective workspace inherited a model
absent from its visible catalog, while a unique default supported the configured
effort. This finding requires a bounded client correction, not a global CLI edit.

1. Provider/lifecycle subagent: read effective workspace configuration and the
   visible catalog through an owned, bounded metadata-only app-server; choose only
   supported configured choices or unique advertised defaults; pass process-local
   arguments to initial and exact-resume turns, with no trial calls or retries.
2. Metrics subagent: retain optional selection intent separately at session and
   attempt level; preserve provider-reported actual metadata and legacy nulls.
3. Product/testing subagent: extend the native fake provider with metadata RPC,
   refusal/privacy/cwd/environment cases, exact-resume argument ordering and
   unchanged configuration assertions. Keep all existing coverage and bounds.
4. Primary: wire native Run and optional workspace Check, update guide/ADR, then
   coordinate focused tests, metadata-only installed CLI proof, release and the
   full gate. Previous 1,478-test evidence is a baseline, not proof of this batch.

No additional model conversation is authorized by this implementation. A new live
check requires fresh approval after the corrected candidate is reviewable.
