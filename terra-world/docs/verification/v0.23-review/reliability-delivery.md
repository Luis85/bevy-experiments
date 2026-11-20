# Terra World 0.23 — reliability, recovery and desktop review

Reviewed on 30 October 2026 against the delivered 0.22.0 source archive,
SHA-256 `54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e`.
This specialist owns WP05–WP07 in the [implementation plan](../../IMPLEMENTATION_PLAN_v0.23.md).
The review is source- and execution-based. Native screenshots and final release
acceptance are separate integrator evidence; no Windows execution is claimed here.

## Review coverage and findings

| Perspective | Evidence and implication | Decision |
|---|---|---|
| Returning-player progress safety | Managed play holds `world-session.lock` through final settings, but `rename_world` performed an independent profile read and replacement. `persist_settings` correctly reads the latest profile, so a sequential rename is already retained; overlapping read–replace intervals can still lose a rename or the session's final metadata. | **R1 / WP05:** metadata operations acquire the same cooperative lease, re-read the selected profile under ownership and retain ownership through publication. |
| Backup coherence and recovery | `create_backup_with_sync` checked profile equality before and after the SQLite snapshot. That correctly rejects changed metadata and preserves a completed inner bundle, but cooperating live play could still change the profile and produce an avoidable incomplete native wrapper. | **R2 / WP06:** native backup requires a quiescent managed world and holds its maintenance lease through final wrapper publication. Explicit backend online backup remains unchanged. |
| Resource-bounded diagnostics | `smoke_help` limited retained output to 64 KiB only after the child exited. Its temporary file could grow without that limit throughout the timeout. A pipe inherited by descendants also needs a deadline independent of the leader's exit. | **R3 / WP07:** bounded pipe capture, immediate overflow detection, one deadline covering both process and output completion, separately finite cleanup. |
| Privacy and failure truth | Private publication already distinguishes unpublished output from published-but-unconfirmed output. Nested recovery already tracks separate completion markers. Diagnostics are opt-in and do not export inventories or resume capabilities. | Preserve these contracts. New maintenance does not change checkpoint, launch-witness, identity or publication semantics. No raw process arguments, environment or new startup-log export is introduced. |
| Operational support | Failure before the first running update can leave no fresh telemetry; the bounded launcher stderr tail is temporary. The existing UI distinguishes failed diagnostics from a failed world checkpoint. | Keep this limitation explicit. A persistent startup-result feature requires a separate reviewed privacy and retention design; it is not necessary to fix the selected concurrency and smoke defects. |
| Windows and trusted delivery | A Linux/Windows CI matrix exists, but source definitions do not prove Windows clean-machine startup. Action references still float tags. Hash-consistent packages do not authenticate their publisher. | Keep actual Windows/device qualification and authenticated delivery open. Do not relabel them as closed by Linux tests or a portable Python implementation. |
| Framework and maintenance APIs | Backend SQLite online snapshots, validated generations and no-clobber restore already cover a separate operator use case. | Add only a launcher-internal maintenance guard. No new database schema, wire command, dependency or general online-backup restriction. |

## Implemented maintenance design

`model::begin_managed_maintenance` returns a `ManagedMaintenanceLease` that owns
the existing OS lock handle. It shares acquisition and protected-path validation
with managed play. It does **not** call first-launch admission, create
`world-started.json`, initialize progress or mark a checkpoint present. The
stable coordination marker may be created when an older or unstarted world is
first maintained. Closing or terminating the holder releases the lock; the
marker is never deleted to force admission.

Rename and backup first reject a selection already known to be stale without
creating coordination metadata. After acquisition they re-read and compare the
entire selected profile. That second read is authoritative: a session or another
maintenance operation may have published between preflight and acquisition.
The guard stays in the operation's scope through its last publication or error.

An active session or another maintenance owner receives a concrete next action:
close that session or wait for maintenance, refresh the selected world and
retry. A busy native backup fails before destination-directory creation. Rename
changes the title while keeping game bytes, sources, character, other settings
and last-played/creation/checkpoint fields. The first rename may additionally
create its stable coordination marker; documentation and tree assertions name
that expected addition explicitly.

Native backup continues to support regional checkpoints only. Portable JSON
worlds receive coordinated rename and first-launch safety, but their complete
manual archival policy remains. The operator/SDK's online SQLite snapshot
workflow still permits committed source writes under its existing checks.
Completed inner bundles survive later native-wrapper publication failure, and
post-publication uncertainty remains an error with retained output.

This is a cooperative local protocol. Older executables, direct SDK writers,
network filesystem semantics and arbitrary administrative path replacement are
not brought under a universal writer lock or power-loss guarantee.

## Implemented desktop startup design

The optional smoke command now pipes stdout/stderr into an unbuffered reader.
It retains no more than **65,537 bytes**: the accepted 64 KiB plus the first byte
that establishes overflow. It never creates a temporary output spool. The pipe
provides bounded backpressure; an overproducing process is failed and cleanup is
requested promptly.

Success requires the leader's successful exit, completed output, the expected
help markers and output within the limit. A leader that exits successfully while
a descendant retains stdout cannot pass or wait forever: output completion uses
the same requested deadline. Nonzero exit, read failure, overflow and deadline
are distinct failures. Passive package inspection still never executes a binary.

Cleanup is separately bounded: Windows `taskkill` has a ten-second limit,
leader observation has two seconds and reader completion one second. POSIX
cleanup signals the owned process group; deliberately detached processes are
outside it. Windows cannot always confirm descendants after the original leader
has exited. The message retains that uncertainty. A daemon reader cannot force
the CLI to wait indefinitely for such a pipe; this is diagnostic containment,
not a sandbox for untrusted executables.

Independent integrator review found a further success-path case: a child could
print valid help, redirect its own descendant's output and exit normally while
that descendant kept running. On POSIX, success now additionally requires the
owned process group to be absent. A surviving group causes failure and bounded
termination; a kill request is never mislabeled verified exit. A real redirected
descendant fixture verifies this refusal and observes the Linux descendant as
terminated. The Windows success report explicitly records
`descendant_check.status=unconfirmed`: it proves leader exit/output closure, not
whole-tree quiescence or a device-qualified finite background workload.

## Meaningful validation

### Executed Python validation

Before the independent success-path correction, the focused standalone suite
passed **15 tests**. The complete Python packaging,
verification and publication suite then passed **49 tests**, with no failed or
skipped tests, in **7.256 seconds**. The changed verifier/test inputs remained
unchanged during that run. Exact command, timestamps, hashes and exit status are
in [the machine-readable record](reliability-python-tests.json); complete output
is in [the retained log](reliability-python-tests.log).

The subsequent focused standalone suite passed **16 tests**, including the
redirected-descendant case. A fresh complete Python run after that correction is
retained separately as [polish results](reliability-python-polish-tests.json) and
[polish log](reliability-python-polish-tests.log): **50 tests passed**, no failures
or skips, in **7.317 seconds**, with both changed inputs unchanged during the run.
The original 49-test evidence
is preserved and is not described as validation of the later change.

Real child processes establish:

- Normal expected help succeeds; wrong help and a nonzero exit fail.
- Exactly 65,536 bytes succeeds; one more byte fails.
- A continuously noisy process fails while running without a disk spool and
  never reaches its later sentinel action.
- A sleeping child hits the deadline.
- A normally exited leader with a descendant holding stdout hits the output
  completion deadline and does not return an accepted success.
- Existing passive inventory, path, alias, hard-link, hash, architecture and
  receipt tests remain passing.

These are Linux executions. The Windows branches are implemented with explicit
limits but not qualified by this record. The descendant fixture cleans its own
known child separately on Windows; ordinary verification does not pretend to
know every descendant PID.

### Executed native candidate validation

The release coordinator ran the full current 0.23 `terra-demo` suite after shared
formatting: **238 passed, 0 failed, 0 ignored**, exit code 0. The retained
[candidate result](../v0.23-preflight/native-integration.json) records
`cargo test --locked --offline -p terra-demo`; the
[complete candidate log](../v0.23-preflight/native-integration.log) reports
2.05 seconds of test execution. This reviewer independently matched the log's
SHA-256 to the recorded
`4ababa4d3cab09ef988f93994a176fdd0dffb1456359b17e78053440c264b07d`.

The successful suite includes the new real-process
`managed_maintenance_coordinates_real_sessions_and_other_maintenance` and
`json_world_maintenance_does_not_admit_first_launch_or_create_progress` fixtures,
plus the amended nested-publication and world-name checks. They cover actual
process-held session and maintenance leases, clean and
abrupt release, immediate busy rename/backup/start rejection with profile,
checkpoint and witness byte preservation, and absence of backup output.
Successful release is followed by a renamed world and exact regional
backup/verify/recovery comparison. Another fixture uses an unstarted JSON world
to prove metadata maintenance cannot consume its fresh-launch admission.

Existing nested recovery fault tests now assert that maintenance ownership is
still held at backend and native final-marker synchronization. Their normal
retry also proves the lease is released on the failure path and complete inner
output is retained. The existing world-name tree check explicitly accepts only
the selected profile change plus its newly created stable coordination marker.

This is the full native candidate test suite. It remains separate from the
subsequent canonical release acceptance and final native confirmation below.

### Completed canonical acceptance and final native association

The coordinator's actual [terminal receipt](../v0.23-release-acceptance/completed-gates.json)
completed all **44 gates** with **1,070 distinct Rust tests**, **1,171 Rust test
executions**, no failed or ignored tests, and no timeouts. The complete
[Python gate log](../v0.23-release-acceptance/desktop-packaging-tests.log) reports
**50 tests passed**. The final native test gate again passed all **238 tests**,
including the maintenance and recovery regressions above; the named recovery
and world-name journeys also passed their dedicated gates.

The independent [accepted native provenance audit](../v0.23-native/confirmation-provenance-accepted.json)
rehashes all 44 gate logs and 36 accepted artifacts, validates receipt/test
counts and the pinned compiler, and binds the final direct confirmation to the
same **381 unchanged implementation inputs** and accepted executable. The actual
terminal receipt SHA-256 is
`12b4a3f72591c7431bbc0aeda6fe189c5650b81da29d29c7fa8ef955c903cfa0`.

That audit associates ten final captures with directly recorded recovered-world
and menu processes, both exit 0, and executable SHA-256
`432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.
It preserves the earlier candidate child-association limitation, the original
pending report and the executed missing-terminal refusal. Linux software-rendered
native confirmation and canonical acceptance do not assert Windows or physical
device qualification. Final extracted-package verification is a distinct
delivery check.

## Primary research and how it informs the design

1. [Rust `File` locks and synchronization](https://doc.rust-lang.org/std/fs/struct.File.html)
   describe participating-handle coordination, release on handle closure and
   platform-specific advisory/mandatory semantics. Reusing one stable lock
   boundary is appropriate; interpreting a marker's presence as active ownership
   is not. `sync_all` also exposes errors that ordinary handle drop does not.
2. [Microsoft `LockFileEx`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex)
   documents nonblocking acquisition and lock release when the owning process
   terminates, with platform caveats including mapped-file access. This supports
   lifetime ownership while preserving explicit Windows execution limits.
3. [Microsoft buffered-I/O flushing](https://learn.microsoft.com/en-us/windows/win32/fileio/flushing-system-buffered-i-o-data-to-disk)
   explains why closing a file alone is not proof that buffered data reached
   durable media. The existing publication/error contract is retained; the new
   lease must not be presented as a durability guarantee.
4. [SQLite online backup](https://www.sqlite.org/backup.html) and
   [its API contract](https://www.sqlite.org/c3ref/backup_finish.html) describe
   consistent snapshots, completion and retryable lock contention. Native
   metadata coordination complements the established backend copy; it does not
   replace SQLite copying with raw database-file copying or restrict the explicit
   online operator API.
5. [Python subprocess documentation](https://docs.python.org/3/library/subprocess.html)
   explains pipe draining, memory buffering and timeout cleanup. The
   implementation bounds the reader directly and treats both process and EOF as
   conditions of success, with real-process regressions.
6. [Rust MSVC target requirements](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html)
   and [Microsoft runtime guidance](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/)
   establish build/runtime requirements. They cannot qualify Terra's own window,
   GPU, save, input or recovery experience on the intended Windows PC.
7. [GitHub Actions secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use)
   recommends workflow dependency lockfiles, or verified immutable action commit
   references when lockfiles cannot be adopted. Existing floating action tags
   remain a distinct trusted-delivery improvement; this pass does not assert
   publisher authentication from local package consistency.

## Changed implementation and guides

- [Lease API](../../../crates/terra-demo/src/launcher/model.rs)
- [Coordinated rename and native backup](../../../crates/terra-demo/src/launcher/recovery.rs)
- [Maintenance/recovery regressions](../../../crates/terra-demo/src/launcher/recovery_tests.rs)
- [Standalone verifier](../../../scripts/verify_desktop.py)
- [Verifier process regressions](../../../scripts/test_verify_desktop.py)
- [Desktop delivery guide](../../DESKTOP_DELIVERY.md)
- [Recovery guide](../../RECOVERY_BUNDLES.md)

All pre-existing evidence and assets remain untouched. Final peer-review,
native confirmation and canonical acceptance records retain their separate
scope. The integrator associates the final delivery identities before handoff.
