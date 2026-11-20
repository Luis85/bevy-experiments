# Terra World 0.22 — reliability, security and delivery review

Reviewed 29 October 2026 against the available 0.21.0 source. This is a fresh
source/document review and primary-source research. The findings and proposed
criteria below describe that baseline; the implementation follow-through is
recorded at the end. The user goal is a reusable Rust/Bevy game framework with a
dependable native reference game, preserved ownership/progress and honest
delivery evidence. Source review does not establish an executed Windows journey.

## Recommendation

Make publication outcomes explicit and prevent concurrent managed play of one
world. These reliability changes need no domain-save, regional-schema or wire
migration. The parallel community package deliberately advances wire protocol to
11; domain save 1 and regional schema 1 remain. Preserve the regional store's existing
SQLite transaction, generation comparison and complete validation. Do not trade
those guarantees for faster-looking save feedback.

## Assessment across operational product perspectives

| Perspective | Finding | Priority / decision |
|---|---|---|
| Player progress and trust | Several publishers acknowledge success after ignoring a failed directory flush. The UI can therefore imply a confirmed save without confirming the filename publication. | P1: implement staged publication outcomes and honest recovery guidance. |
| Returning player / concurrent use | The first-launch witness serializes initial creation only. Two resumed managed JSON sessions can load the same snapshot and overwrite each other's later progress. | P1: hold a cooperative process-lifetime managed-world lease. |
| Operators / backup and recovery | SQLite online backup and complete bundle identity are good foundations. A post-publication error needs different cleanup from failure before publication, including nested recovery wrappers. | P1: retain visible output after uncertain publication; never delete it as if it were an unpublished temporary. |
| SDK integrator | Atomic file-writing behavior is duplicated across domain, Bevy, network and launcher layers. Their private permissions and flush contracts differ. | P2, implement with the publication work: one small, dependency-light contract and caller-level integration. |
| Security and privacy | Native snapshots use 0600 temporaries; the domain `SandboxState::save` still uses the ambient umask. Network identity saves have private temporaries but no parent sync. | P2: align private publication; preserve token redaction and current output alias guards. |
| Networking / authority | Welcome validation, sequence ownership, TLS/ALPN, finite buffers and durable regional acknowledgement are retained. Public enrollment, account recovery, moderation and WAN capacity remain unimplemented. | Preserve; do not claim Internet service readiness from local loopback tests. |
| Windows user | There is a Windows CI/packaging definition but no executed Windows receipt or clean-machine game journey. Linux compilation, tests or software rendering cannot supply that evidence. | G02 remains P1. Improve reproducible inspection/handoff if root allocates it, but do not represent that tooling as platform qualification. |
| Release engineering | Exact roster, inputs, logs, binary identity and timeout handling are present. The actions workflow uses mutable release tags; artifacts are integrity checked, not publisher authenticated. | P2 follow-up: pin verified action commits and define signed release trust once a real release environment is selected. |
| Support / observability | Telemetry publication silently ignores the same parent-sync errors. A diagnostic record should describe a failed publication while retaining previous valid data or new visible uncertain data according to stage. | Include in publication integration; retain bounded privacy-safe diagnostics. |

## R22-01 — acknowledged publication is not consistently confirmed

### Evidence

- `crates/terra-sandbox/src/persistence.rs::SandboxState::save` flushes file
  contents, renames, then ignores directory-open and directory-sync failures.
  It also skips directory sync entirely for a relative basename.
- `crates/terra-bevy/src/sandbox.rs::atomic_write` has private creation but ignores
  the post-rename parent result. `LocalWorldSave::write` and the save-feedback
  path depend on that success result.
- `crates/terra-persistence/src/operations.rs::write_new_private_file` ignores
  parent sync after `persist_noclobber`. `backup_with_options` propagates an I/O
  error after creating its final hard link, without identifying that the final
  validated snapshot is already visible.
- `crates/terra-net/src/session.rs::SessionIdentity::save` and
  `server.rs::Server::save_world` flush the temporary and persist it but never
  sync the containing directory.
- `crates/terra-demo/src/launcher/model.rs::atomic_json`,
  `launcher/recovery.rs::restore_reviewed` and
  `telemetry_capture.rs::publish` ignore parent synchronization failures.
- Recovery code currently removes its claimed destination on any error:
  `terra-bevy/src/recovery.rs::{create_recovery_bundle,restore_recovery_bundle}`
  and `terra-demo/src/launcher/recovery.rs::{create_backup,restore_reviewed}`.
  Adding `?` after a final-marker rename without changing that cleanup would
  delete a newly visible completed artifact after a flush error.
- `publish_start_witness_with_sync` already uses the better wording for its
  directory-sync failure. Its temporary-unlink error occurs after hard-link
  publication too and should receive the same stage classification.

### Design for dispatch

Use a shared small publication contract with two error stages:

1. **Not published:** serialization, validation, temporary creation/write/file
   sync or namespace publication failed. Preserve any previous destination;
   clean only a temporary this invocation successfully created.
2. **Published, durability unconfirmed:** replacement/create succeeded but the
   relevant final synchronization or post-publication cleanup failed. Keep final
   bytes. Do not say the old destination is intact or that rollback occurred.
   Advise inspecting/reopening the visible output and retaining recovery copies.

Private same-directory temporary creation, `create_new`, file sync, explicit
replace versus no-clobber modes, `.` for relative filenames, and explicit Unix
parent sync should be shared. Avoid a new external dependency. A new
`terra_sandbox::publication` module is a possible dependency direction because
domain persistence, regional persistence, networking and Bevy already depend on
that crate; placing it in `terra-persistence` would cause a cycle for domain
saves. Root may choose another existing low-level home.

For final recovery markers, track publication without parsing human error text.
Before a valid final marker is published, a failure may clean the owned incomplete
tree. After marker publication, keep the final tree even if the function returns
uncertain durability. Synchronize copied payloads/directories before issuing a
durable completed-tree receipt; syncing only the last index is insufficient.
Directory creation and final marker visibility need an explicit ordering. Keep
the existing bounded inventory and symlink/alias protection.

Windows must retain a separately described contract. Do not invent a portable
directory-fsync equivalent or silently add an unsupported Windows flag. A Linux
fault-injection test establishes program behavior for that injected boundary;
it does not prove power-loss behavior on the user's hardware.

### Acceptance criteria

- Real filesystem tests inject write/file-sync/publication/parent-open/parent-sync
  failures at the actual shared boundary, with both replace and new-file modes.
- Prepublication errors preserve old bytes and never remove a pre-existing
  colliding temporary. Postpublication errors retain exact new bytes and report
  the published stage. No-clobber outputs never replace an existing file.
- Unix first writes/replacements use 0600 under a permissive umask in an isolated
  subprocess. Relative-basename destinations sync `.`.
- Native final save does not exit as successfully saved on uncertainty. Network
  identity failure does not admit a new connected state; its visible bytes remain
  recoverable, and tokens are not disclosed by new error formatting.
- Inject a post-final-marker error through backend bundle and managed wrapper
  flows; completed outputs remain, are independently verifiable, and no success
  receipt is issued. Inject an earlier error; no completed artifact is offered.
- Existing backup/source/recovery and domain/net/native save tests remain green.
- Documentation states the exact Unix/Windows and external-filesystem limits.

## R22-02 — managed resumed sessions lack a lifetime writer lease

`prepare_managed_launch` checks the witness and validates existing progress, then
returns only a boolean. Once the witness exists there is no process-lifetime
mutual exclusion. `main.rs` later runs the game and persists presentation
metadata. Regional generation comparison protects a later stale database commit,
but JSON has no corresponding generation comparison. Even regional players can
enter two interactive sessions before discovering that the second cannot save.

### Design for dispatch

Claim a stable private per-profile lock file with `std::fs::File::try_lock` before
managed initialization, retain its handle through game shutdown and final
metadata publication, and reject contention with a message to close the existing
session. Never remove the lock file on normal release: unlinking a locked inode
would let another process acquire a different inode under the same name. OS
handle closure, including abrupt process termination, releases the lock.

Validate the lock path against profile, witness, progress and aliases. Add the
lock path to preflight and runtime export/diagnostic protections. A persistent
unlocked valid lock file is not a stale session; only actual lock acquisition
decides admission. Keep the existing witness for interrupted first-session continuity.
Release only after final save/settings handoff. Do not hold a lease across
spawning/re-entering a menu. Consider a small private `ManagedSessionLease`
type; no protocol/save migration is needed.

The guarantee is cooperative: current managed runtimes for the same canonical
profile participate. Older releases, independent SDK/direct-file writers,
administrative unlink/replacement, network filesystems and arbitrary path races
are not brought under a universal writer guarantee by this lock.

### Acceptance criteria

- Two actual subprocesses cannot hold the same managed lease; contention returns
  promptly and does not modify progress, witness or profile.
- Distinct managed worlds can run concurrently.
- The second process can acquire after clean close and after abrupt holder exit;
  an existing unlocked lock file does not block it.
- The runtime owns the handle until its final metadata persistence completes.
- Existing first-launch race and interrupted-first-save tests retain their
  meaning. New tests distinguish first-start admission from resumed exclusivity.
- Screenshot, political and telemetry/report-lock outputs reject the lease path,
  future ancestors and filesystem aliases before writes and at runtime.
- Windows native behavior remains unqualified until that same subprocess matrix
  and full journey runs there.

## R22-03 — delivery opportunities and retained boundaries

A platform-neutral extracted-package verifier would help the intended Windows
user distinguish missing/corrupt payloads from runtime/driver failures. It should
read the package's exact manifest, bound names/counts/bytes, reject traversal and
links, validate the actual binary architecture, verify hashes, and optionally
run `--help` with a finite timeout. It should not download DLLs, require an
administrator, mutate a user's worlds, or treat a self-authored manifest as a
signature. Root owns whether this fits the increment.

The existing GitHub workflow has least-privilege `contents: read`; action tags
remain mutable. Pinning verified action commits is feasible repository work,
but actual Windows runs require a connected Windows execution environment. Do
not dispatch a fake Windows receipt or count static workflow review as execution.

## Research and application

Primary sources checked during this review:

1. [Linux `fsync(2)`](https://man7.org/linux/man-pages/man2/fsync.2.html): file sync
   does not necessarily make its directory entry durable; directory sync is an
   explicit operation and can fail. Application: do not ignore its outcome or
   conflate post-rename uncertainty with an untouched destination.
2. [Rust `std::fs::File`](https://doc.rust-lang.org/std/fs/struct.File.html):
   `try_lock` is nonblocking, reports contention, and releases with file-handle
   closure. Current mappings use Unix flock and Windows LockFileEx. The API is
   available since Rust 1.89, below this project's minimum. Application: use an
   OS-managed cooperative session lease instead of stale PID-file ownership.
3. [Microsoft `LockFileEx`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex):
   exclusive fail-immediately locking and process/handle lifecycle are explicit.
   Application: retain cross-platform tests and avoid append-only handles.
4. [Microsoft `ReplaceFileW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew):
   `REPLACEFILE_WRITE_THROUGH` is unsupported; documented failure states can
   involve moved files. Application: do not use that flag to claim Windows
   crash durability, and keep platform-specific qualification separate.
5. [SQLite WAL](https://sqlite.org/wal.html) and
   [SQLite online backup](https://sqlite.org/backup.html): WAL is local-host
   shared-memory coordination, and an online backup supplies a consistent
   snapshot through the SQLite API. Application: retain the existing API-based
   backup and exact committed-generation verification; no raw active DB copy.
   The current WAL page records a fixed historical WAL-reset bug. The locked
   `libsqlite3-sys 0.38.2` bundled header in the installed Cargo source reports
   SQLite **3.53.2**, newer than the named 3.51.3 fix. This review therefore does
   not recommend an ungrounded dependency upgrade for that issue.
6. [GitHub secure use reference](https://docs.github.com/en/actions/reference/security/secure-use):
   minimum token permissions and immutable action references strengthen the
   build boundary. Application: keep current read-only permissions and consider
   verified commit pinning; do not equate artifact hashes with publisher trust.

## Retained gaps

G02 physical Windows execution; G11 signing, release selection and rollback;
G13 Internet enrollment/recovery/moderation; G14 WAN/soak capacity; G15 measured
storage tail latency; G16 retention, offsite recovery, target-filesystem power-loss
evidence and operational RPO/RTO; G17 deployment alerting; and G18 distributed
authority remain independent acceptance projects. The proposed work closes named
publication and managed-session defects, not these whole product areas.

## Implementation follow-through — WP05 and WP06

The integrator dispatched both packages. The reliability owner's implementation
is complete and source-stable for the final coordinated build. The integrator's
current validation receipt remains the authority for final acceptance; the
preflight results below are narrower evidence from development.

### Published state and truthful completion

`terra_sandbox::publication` now owns same-directory private publication,
exclusive-create versus replace behavior, typed failure stages, relative-path
handling and checked Unix directory synchronization. Domain saves, native JSON
saves, network host saves and private resume identities, regional exports and
backup links, managed metadata and retained diagnostic reports use it. New
directories use private creation and acknowledge every newly introduced ancestor
entry; existing directories retain their permissions. Initial regional database
creation uses the same checked exclusive-create boundary.

Recovery builders flush their bounded payload tree before publishing the final
index or restore receipt. `RecoveryPublicationError::published()` carries the
final-marker stage through backend and launcher wrappers without parsing prose.
A published backend survives failure of a later native wrapper. That retained
backend is still distinguished from a completed native backup/profile. An inner
temporary database alone is not mistaken for a completed outer recovery tree.

Existing convenience recovery APIs retain their `Result<_, String>` signatures;
additive `_with_sync` entry points expose the structural stage and actual host
synchronization boundary. Direct domain/store/network error enums add a
publication variant. Native final-save handling emits an error exit and keeps
dirty state on uncertain publication. The actual network Welcome callback
retains readable private resume bytes but refuses connected-state admission
until identity publication succeeds.

Peer review found and closed one additional ancestor gap: backup/import and
managed recovery created parents before the final publisher could observe them.
The common checked-directory helper now records missing ancestors, exclusively
creates them, flushes their entries and removes only its own empty directories
on failure. Actual managed backup/restore tests inject failure at that boundary.
The closing sweep applied the same rule to new-world folder claims and telemetry
parents. A new-world profile that is already published remains visible on final
uncertainty, while a failure before its publication cannot return a world path.

### Cooperative managed ownership

`ManagedSessionLease` owns a nonblocking OS lock on `world-session.lock` from
before initialization through final save and metadata publication. The runtime
drops it before returning to the launcher. The stable marker is never removed
to unlock it, and its containing directory is re-synchronized on every
acquisition, including after a previous uncertain marker creation. The separate
`world-started.json` witness still protects interrupted first-session continuity.
Normal unlocked markers are reusable; malformed, aliased or concurrently locked
markers are rejected with recoverable explanations.

Screenshot, political-export, backup and retained-report paths reserve the
profile, initialization witness and session lease, including aliases and future
ancestor conflicts. Actual subprocess tests cover contention without mutation,
independent worlds, ownership while final metadata is saved, clean release,
abrupt holder termination and repeated reopening. No distributed or
nonparticipating-writer exclusion is asserted.

### Focused regression evidence and remaining integration gates

- The first shared-publication preflight recorded **6 passing tests** in
  `v0.22-preflight/publication.log`. A seventh ancestor-directory test was added
  during peer-review closure and belongs to fresh final acceptance.
- `v0.22-preflight/bevy-lib.log` records **140 passing tests**, including the
  actual Welcome identity failure callback, pre-marker recovery failure and
  uncertain final backup/restore markers with verifiable retained output.
- `v0.22-preflight/uncertain-backup.log` records the actual SQLite backup
  post-publication fault regression passing. It independently opens and verifies
  the retained complete checkpoint and rejects a blind no-clobber retry.
- Native regressions exercise actual Save-and-exit, the full managed recovery
  nesting matrix, newly created backups/worlds parents, new-world directory and
  profile acknowledgement, real lease-holder subprocesses and output aliases.
  `v0.22-preflight/demo-confirmation.log` records **226 passing tests**, including
  these integrated scenarios, with no failures or ignored tests. Final release
  acceptance remains the integrator's separate frozen-input run.
- Independent package-admission review closed three concrete corrections:
  real hard-link rejection, bounded reading of both terminal receipt aliases and
  strict present integer failure/ignored counters. The corrected Python tooling
  preflight records **46 passing tests**. Details are in
  [delivery cross-review](delivery-cross-review.md).

The independent player-target source review checked displayed consume identity,
principal/restore revision revalidation, queued body transitions and explicit
owned-ID Stop/Rest commands. Domain Release was confirmed to retain the active
body; it does not imply an avatar handoff. No additional demonstrated target
ownership defect remained in that inspected path. Actual rendered/native and
cross-platform acceptance remain the integrator's separate gates.

The scoped public documentation is updated in `REGIONAL_PERSISTENCE.md`,
`RECOVERY_BUNDLES.md` and `OPERATIONS.md`. Linux fault injection verifies the
program's responses to injected errors. It does not qualify Windows ACLs,
directory persistence, physical power-loss behavior, offsite recovery or a
production RPO/RTO.
