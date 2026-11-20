# Independent archive verifier preparation — 0.17

This records preparation checks, not final 0.17 archive acceptance. The final
two-archive audit has not run at this stage. Root must first freeze the release,
finish both archives and their receipts, and authorize that audit.

## Independent scope

The external helper is `/tmp/terra-build/verify_final_archives_017.py`. It does not
import the source publisher or execute Cargo, the game, or the verification
runner. Its final operation creates fresh extraction trees and an exclusive
external JSON review record; it does not modify the project or delivered ZIPs.

The helper binds preservation to the exact accepted source baseline
`terra-world-0.16.0.zip`, SHA-256
`985d95792ddcc6d84239c17ff1e79c12a79a4146aad3afa7c7567ec5afb436ca`.
It derives the complete prior path and protected maps from that archive and its
inherited preservation record: 1,488 prior paths, 102 original inputs, and 1,068
historical evidence files. Prior current review and validation documents must
also be archived exactly as 0.16. No old evidence is relabeled as a current run.

The prepared checks cover:

- ZIP CRCs, canonical unique regular Unix members, exact file modes, bounded
  extraction, source/desktop manifest coverage, every member hash, and the
  complete current project snapshot.
- Private runtime and credential payload screening for new or changed source
  members, exact preserved input/history bytes, and external dependency lock
  stability.
- The actual frozen runner's gate sequence and exact commands, retention of
  inherited gate commands, passing summaries rederived from each log, and
  distinct Rust test identities independently resolved from archived Cargo
  target inputs and harness output. Every required Cargo test gate must execute
  passing tests. The helper interprets only a whitelist of declaration
  expressions; it never executes the runner.
- Direct native process start/finish identity, exact accepted binary and source
  digest, retained capture PID and time interval, actual PNG dimensions and
  chunk CRCs, and complete current evidence references.
- Single-session telemetry sample ordering, process/session timestamps,
  actual export/world/storage/model errors, bounded model reservations, a
  settled final checkpoint observation, and separate successful process exit.
  Startup loading, ordinary pending work, fallback, paused/saving phases, and
  bounded draining are allowed. The final capture is an exit-request
  observation; it is not described as a post-exit capture.
- Both extracted desktop runs, retained file-access traces, actual packaged
  source/model opens, exact application execution, accepted model/source
  identity, and a fresh comparison of every original tested extraction member
  against the delivered desktop archive.
- Actual private SQLite backup and profile copies opened immutable/query-only,
  with size and operation bounds. The helper rederives the agreed descriptor,
  catalog, player, inventory, money, avatar, character and clock summary fields,
  checks the preserved subset across ordered named-profile play/save and reopen
  processes, and verifies that reopen telemetry reported a real restore.

The launcher resolves `./terra-demo` only after a successful same-PID directory
change to the exact extraction. Its strace wrapper PID is never substituted for
the application PID. Desktop PNGs without capture receipts receive only byte,
structure and dimension validation; a PID/time claim requires the separate
capture receipt.

The save/reopen summary schema has been coordinated with root; its actual final
paths and receipts remain pending. Native coverage is derived from observed
menu, named-profile play/save/reopen, prepared-source and actually larger
rendered-region processes. The named-world case does not imply first-ever
creation. No future comparison, current gate count, current test count, current
source count, or final native process count is claimed here.

## Preparation checks actually executed

The isolated helper check is
`/tmp/terra-build/test_verify_final_archives_017.py`. The successful run checked
reviewer helper SHA-256
`e1d8791be9b3a8d677bce8545c772195a33a8b37b95832123f6926d13793e354`.
This identifies that preparation snapshot; the final audit records the hash of
its actual reviewer source separately.

Using retained 0.16 evidence read-only, the check independently reconstructed
the exact **990 passing test executions and 904 distinct test identities**,
validated one real PNG/capture/process association, and resolved the real
extracted launcher and prepared-world application executions. Those counts are
historical preparation fixtures and are not 0.17 acceptance counts.

Isolated synthetic cases rejected changed capture PID/time, a corrupt PNG,
foreign telemetry PID, prior writer failure, storage error, false final pending
state, an over-budget model reservation, a stale top-level diagnostic snapshot,
an unrelated executable, a changed test filter, a zero-test gate, an existing
output, a symlink output, attempted extraction overwrite, renamed private
session/SQLite content, duplicate JSON keys, nonfinite JSON and an overflowed
JSON number. Positive cases accepted ordinary startup work, paused/saving
observations, and null committed status for an explicitly disabled regional
store. Separate synthetic SQLite snapshots exercised the actual read-only
summary derivation; an untrue money summary and a different reopened profile
were rejected. Temporary fixtures were outside the project and removed by the
test harness.

The diagnostics reviewer independently examined the helper source without
executing the final audit. They found a concrete zero-test/changed-command
acceptance gap in the earlier gate checks; exact command binding and mandatory
passing tests now close that gap and are covered by the preparation checks.
They reported no preservation blocker in the exclusive report publication,
archive path/mode/CRC handling, tested desktop-member comparison, native
capture binding or single-session telemetry checks. Their review preceded the
private SQLite summary addition. Final archive execution and its findings belong
in the separate frozen release audit record.
