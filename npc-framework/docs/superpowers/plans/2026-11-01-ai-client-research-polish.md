# Research-driven AI game client improvement pass

User scope: same-PC authoritative simulation server, including headless hosting;
dedicated AI client connects and controls its assigned actor using a logged-in CLI.

Research: [primary-source report](../../research/2026-11-01-ai-game-server-client.md).

Execution status: all four owned streams and independent improvement reviews are
complete, including native proof and a single visual summary pass. See the
[validation record](../../_notes/2026-11-01-ai-client-research-polish.md).
The shared full gate awaits the separately owned economy-recovery correction.

## Implementation workstreams

1. **Root: bounded game control transport.** Add an explicit `RpcClient`
   deadline/response-budget method for `client/*`. Validate response envelopes,
   avoid unbounded HTTP allocation and enforce an overall deadline across reads.
   Preserve the ordinary action/event long-poll path. Regression tests use local
   fault servers: no reply, trickled reply, oversized headers/bodies/chunks,
   malformed/crossed IDs and normal JSON-RPC errors.
2. **Lifecycle subagent:** classify connection refusals and protocol mismatch;
   wire bounded control calls; latch narrowly structured game-access revocation
   despite later CLI success. Preserve accepted session/usage during fatal
   cleanup. Report server/provider/actor-control phases truthfully. Add fake-CLI
   and real-host regressions proving no extra prompt after revocation/permanent
   refusal and no simulation stepping to establish control.
3. **Observability subagent:** isolate transcript persistence behind bounded
   storage work; cap encoded amplification and flush/drain waits; improve compact
   secret redaction; reserve catalog space per kind; record per-attempt CLI/scope
   provenance. Preserve cumulative accounting, original archives, unavailable
   cost and requested-versus-actual metadata. Add stalled-writer, pressure,
   redaction, fairness and version/provenance regressions.
4. **Native product subagent:** correct fast shortcut and clipboard ordering
   through supported app input APIs. Keep non-English logical key behavior,
   focus/IME boundaries and delayed clipboard results safe. Do not modify Bevy
   dependencies, depend on observer ordering or hide the defect with delays.
   If a robust narrow fix needs a different design, bring concrete evidence back
   before changing the input architecture. Qualify any Claude startup/denial
   parsing against documented/version-proven frames.

## Coordination and integration

Root owns Cargo, native captures, research/ADR/guide updates and final integration.
Subagents do not build or invoke providers. Runtime touch points for new metrics
provenance and log flushing are coordinated through the lifecycle owner. The
parallel economy chat owns village/world behavior files; preserve those changes.
At most one Cargo chain here and one coordinated chain there.

## Acceptance

- Game alone assigns actors, validates observations/intents and owns simulation time.
- Stop/monitoring is not held hostage by control-RPC waits or transcript locks.
- Permanent refusal and revoked access cannot become silent automatic continuation.
- Connection/provider progress is not misrepresented as actor control.
- Logs are bounded, sensitive values scrubbed, and native histories remain useful.
- Usage provenance is explicit; existing accounting/archives are not guessed or reset.
- Native folder entry and clipboard operations work with rapid input and safe focus.
- Independent subagents review each other's final seams after implementation.
- Focused tests, appropriate native screenshots and `cargo xtask verify` complete;
  exact results and limitations are recorded. No new live provider inference,
  global settings changes, dependency upgrades, protected-oracle edits or pushes.
