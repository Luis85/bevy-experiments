# Release owner's peer cross-review — 0.20

Scope: current source of source-bound mesh retry/telemetry, SDK Welcome/snapshot admission and their regressions. No peer source edits or Cargo execution were performed.

## Render recovery

No material source blocker found. A current-source receiver disconnect or poisoned lock removes the tracked job, preserves old valid meshes, retains pending dirty work and stores retry context. Exponential real-time delay is capped at30s (first500ms); dispatch filters the deadline instead of spawning per frame. Failed-attempt count survives retry dispatch, so repeated failure does not reset backoff. Success clears active failure state; changed Arc/revision/LOD and eviction drop obsolete contexts. Source identity and existing integration/two-job budgets remain authoritative. The failure ledger is bounded by resident tiles. F12 explicitly calls the message the last failure and shows current retrying count, so historical error remains distinguishable after recovery. Local diagnostic JSON exposes those fields only through incumbent opt-in behavior.

Tests inject real disconnected/poisoned channels into the actual render system after a dirty-edit dispatch, retain old mesh handles, force only retry deadlines, and inject successful completion. This is stronger than a duplicate delay helper; it proves pending/dispatch/integration state. Maximum-backoff and context-reset cases complement the source-identity regressions. Hung live worker threads, broad edit invalidation and actual GPU/native qualification remain declared followups, not implied closures. No other RenderTelemetry struct literals were found that would need new fields.

## Network admission

No material source blocker found. Shared scalar/capability Welcome checks run before SDK bind; requested data identity is retained and checked before bind; ReplicaState rejects repeated identity. The native adapter additionally rejects a repeated connected Welcome and retains game-owned data/catalog policy. Direct raw Snapshot/SandboxSnapshot variants reject before replica normalization, while ActorUpdate/SandboxUpdate continue through revision/baseline validation. Protocol10/serde shape stays unchanged. Existing token-redacting Debug is preserved; errors name generic admission failure without echoing bearer values.

Malicious loopback fixtures exercise bad handshakes followed by a good one, proving rejected input did not bind a replica; direct normalized-only snapshots reject before and after admission. Nonfinite scalar cases need the pure validator because JSON cannot encode them. This closes selected SDK admission inconsistencies, not all untrusted-server semantics: game-owned catalog/source-truth policy, other event interpretation, public accounts/moderation and WAN qualification remain outside scope. A trusted network transport is not a trustworthy server.

## Execution boundary

These conclusions are source review. Central focused/full Rust runs establish actual compilation and regression outcomes. Native windows/devices remain unavailable here. Release-owner Python tests are separately recorded in release-tests.log.
