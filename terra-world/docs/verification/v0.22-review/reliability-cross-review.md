# Independent source review: publication and recovery

Reviewer: world/framework specialist, 29 October 2026. Reviewed the current
0.22 implementation while the reliability specialist continued the managed
lease work. This is **source inspection**, not a test-execution receipt or a
physical power-loss qualification. No reliability-owned implementation files
were changed by this reviewer.

## Finding requiring closure

### RC01 — callers can hide newly created parent directories from synchronization

**Severity: P1; sent to the reliability specialist and integrator.**

`terra-sandbox/src/publication.rs::prepare_parent` correctly records absent
ancestors before creating them, then synchronizes each new directory and its
parent before publishing a file. However, the following callers create those
directories before invoking the shared publisher:

- `terra-persistence/src/operations.rs::RegionStore::backup_with_options_and_sync`
  calls `fs::create_dir_all(parent)` before allocating its snapshot directory.
- `RegionArchive::import_new` in the same file creates the destination parent
  before staging its database and calling the backup path.
- `terra-demo/src/launcher/recovery.rs::managed_parent` calls
  `model::private_directory`, which creates the managed `backups` or `worlds`
  hierarchy with `create_dir_all`.

The later publisher sees an existing parent and cannot discover that this call
created its directory entry. A backup's file link and its immediate containing
directory are synchronized, but a newly created ancestor's own entry can remain
unconfirmed. For example, the first launcher backup synchronizes the new backup
tree and `user_data/backups`, while the new `backups` entry in `user_data` has no
corresponding checked flush. This matters specifically when the operation must
create its parent hierarchy; existing-parent paths do not have this defect.

Recommended bounded fix: expose or reuse a checked directory-creation helper
that records missing ancestors before creating them, synchronizes every newly
created directory and its parent, and propagates failures. Use it before
allocating backup/import temporary directories and when creating managed
launcher parents. Do not compensate by claiming that the final file's directory
flush establishes all ancestor durability. Add focused new-parent coverage at
the helper or adapter boundary, including a failing parent flush before final
publication. Runtime validation remains assigned to the integrator's scheduled
test runs.

## Contracts that held under source inspection

| Area | Evidence and assessment |
|---|---|
| Publication stage | `PublicationError::{NotPublished, Published}` and `.published()` preserve a structural distinction. File write, file sync and namespace publication precede the `Published` classification; final cleanup and parent flush failures preserve visible destination bytes. |
| Owned temporary cleanup | The cleanup guard is acquired only after exclusive temporary-file creation succeeds. The implementation does not acquire ownership of a preexisting colliding path. Replace and create-new policies use separate rename and hard-link operations. |
| Prior bytes and privacy | Save adapters retain validation and byte budgets before publication. Newly created staging files use Unix mode 0600. Relative basenames resolve their containing directory to `.`. No new money, items, ownership transitions or simulation clock changes are introduced by the shared publisher. |
| Database snapshot consistency | Backup still uses the bounded SQLite backup API into an isolated snapshot, validates that snapshot's full committed domain and pinned descriptor, consolidates its journal, then publishes the snapshot. The receipt uses evidence read from the copy. It does not switch to raw copying of an active WAL database. |
| Inner recovery completion | `create_recovery_bundle_with_sync` and `restore_recovery_bundle_with_sync` validate payloads, synchronize their bounded trees, publish a final marker and propagate `RecoveryPublicationError.published`. Cleanup is limited to owned trees that have not published their final marker. |
| Nested completion and cleanup | The launcher sets its retained-output flag after a successful backend bundle/restore and also propagates an inner published error before returning. Later wrapper-record/profile errors do not recursively delete the already published backend output. Error text gives the retained path and does not claim the attempt completed successfully. |
| Original state conservation | Recovery restores to a new destination. Copied generation, full archive hash, catalog, political/source identity and local metadata remain checked; the retained original bundle is revalidated before publishing the restore receipt. Domain and host saves continue to serialize complete validated snapshots without mutating the live domain. |
| Platform scope | The shared module explicitly narrows non-Unix directory durability claims. Quiescent/cooperative path assumptions remain stated, rather than claiming protection against an administrator replacing path ancestors during an operation. |

The backend recovery API requires its destination's parent to already exist;
its direct `new_private_directory` plus tree/parent synchronization is therefore
not itself the RC01 missing-ancestor case. The gap is in callers that create a
parent hierarchy beforehand.

## Acceptance evidence inspected, pending execution

These tests were read in source; this reviewer has not run Cargo:

- Shared publication fault matrix: before-write, before-file-sync,
  before-publication and post-link cleanup failures; replace/create-new
  directory-sync failures; collision ownership; relative basenames; permissive
  umask; bounded tree synchronization.
- Persistence's `uncertain_backup_publication_preserves_a_verifiable_complete_checkpoint`
  reopens the retained database and rejects a silent retry over it.
- Backend recovery's
  `uncertain_final_markers_keep_complete_backup_and_restored_tree_for_verification`
  retains and independently verifies both bundle and restored archive.
- Launcher's
  `uncertain_nested_recovery_publication_retains_verifiable_outputs_without_success`
  injects faults at both backend and wrapper markers. It checks exact restored
  archive equality and distinguishes a complete inner bundle from a completed
  native launcher backup/profile.

The managed lease was still being edited and is outside this review. Compilation,
actual fault-test outcomes, native save/exit behavior and supported-platform
execution must be reported separately. RC01 closure should be appended here
after the implementation is inspected and its scheduled verification completes.

## RC01 source closure

The reliability specialist accepted and corrected RC01 during this increment.
Reinspection confirms that shared `create_private_directories` records the
missing ancestor chain before creating it, creates new directories with Unix
mode 0700, and checks synchronization of every new directory and its containing
parent before payload publication. If admission fails, it removes only empty
directories exclusively created by that call; it does not recursively delete
another writer's content. The save publisher, backup/import staging setup and
launcher private-directory adapter now use this shared boundary.

The new helper regression observes both new directories and the existing
ancestor in the synchronization calls. The native
`new_managed_backup_and_world_parent_sync_failures_cannot_acknowledge_completion`
regression injects failure at the first `backups` parent entry and at a fresh
restore user-data parent, checks no completed output, verifies the original
profile/backup and exercises a later successful separate attempt. Nested
final-marker injection remains targeted to marker presence, so parent creation
does not accidentally replace the intended post-publication test case.

**Source-review disposition: RC01 closed; no remaining blocking source finding
in the reviewed publication/recovery scope.** These additional regressions
still require the integrator's scheduled execution. The lease remains outside
this review, and no Windows or physical power-loss result is implied.
