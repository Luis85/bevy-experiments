# Gameplay-owner cross-review — 0.20

Source/test inspection, 29 October 2026. Peer source was not edited and Cargo
was not executed by this reviewer.

## Persistence hydration

The complete-residency shortcut now calls the incumbent transactional
`read_partition(true)` barrier before returning. It checks schema/descriptor and
generation first, including generation-zero empty-store invariants; guarded
cache reuse occurs only after SQLite external-write epoch and own-write counters
match. The returned committed partition is deliberately not installed over the
caller's complete live state, so legitimate uncommitted edits survive.

The tests cover externally modified page/global/metadata bytes and a schema
extension at the same generation, then preserve full caller serialization,
residency status, acknowledged/stored generation and checkpoint metrics. A new
empty store covers generation-zero admission. The positive route explicitly
checks cache hits without extra decoded checkpoints, retained depleted-resource
edits and a later successful checkpoint/restore. No source blocker identified.
Cache behavior is correctness evidence, not measured latency or power-loss /
Windows qualification; retention/RPO/RTO remain separate product work.

## Network admission

The shared Welcome validator matches the actual host ranges: protocol10,
nonzero identities, validated position, finite bounded clock/time scale,
1..120 tick rate, bounded next sequence and 64 hexadecimal token bytes. The SDK
checks an explicitly requested data manifest before binding a replica, while
world/session names remain independent of source identity. Native admission
reuses the scalar validator. This does not assert source authenticity or impose
unrequested catalog/business policy.

Raw Snapshot/SandboxSnapshot variants reject before and after handshake; SDK
normalized events still arise from revisioned ReplicaState reconstruction.
Existing replica admission prevents revisioned updates before Welcome and rejects
repeat binding. Loopback tests deliberately send invalid fields/source then a
valid Welcome separately, proving no premature identity mutation or capability
publication. Nonfinite values need the pure validator test because JSON cannot
represent NaN/inf; existing network/normalization/resync suites remain necessary
integration acceptance. No source blocker identified in the selected scope.

The poll path remains incremental per message, not a transactional batch API;
this review does not add a blanket guarantee that all prior valid updates roll
back if a later frame fails. Public account enrollment/moderation, server trust,
WAN capacity and device/native observation remain outside these bounded fixes.
