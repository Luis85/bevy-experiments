# Independent review: managed metadata maintenance

Reviewer: world/framework specialist. Reviewed 30 October 2026 after the
reliability owner identified WP05–06 as stable. This is an independent source
and test-design review; the canonical release receipt supplies actual execution.
No files owned by the reliability specialist were edited by this reviewer.

## Scope and result

Inspected `launcher/model.rs`, `launcher/recovery.rs` and
`launcher/recovery_tests.rs`, including the existing stable session-lock helper,
profile/path validators, publication wrappers, recovery final markers and
retained-output cleanup. **No blocking correctness defect was found in the
selected maintenance implementation.** Its qualification remains cooperative
same-world locking on participating current writers, not arbitrary external
filesystem mutation or proof of physical power-loss behavior.

| Check | Observed implementation | Assessment |
|---|---|---|
| Shared ownership | `ManagedMaintenanceLease` retains the file returned by the same `acquire_managed_lease` used for managed play. | Maintenance and play contend on one stable OS-locked file. There is no second PID-file convention. |
| First-launch distinction | Maintenance does not call `prepare_managed_launch`, publish a start witness or create progress. | Metadata work cannot accidentally consume fresh-world admission. The new JSON fixture explicitly checks witness/progress absence and subsequent real first launch. |
| Stale selection | Rename and backup reject a changed complete profile before acquiring the lock, then reread and compare the complete profile while holding it. | The first read avoids unnecessary coordination metadata for already stale UI. The second read closes the normal participating-writer race before mutation/copy. |
| Guard lifetime | Named `_lease` locals remain in the rename/backup function scope through publication and result construction. | The guard survives profile replacement, backend bundle publication, launcher marker publication and error cleanup. It is released by normal return or process death. |
| Busy backup | Lease acquisition precedes managed backup-parent creation and exclusive output-directory claim. | Active play or maintenance refuses before a backup output is allocated. The finite contention fixture checks the backup root remains absent. |
| Snapshot coherence | The held guard covers profile reread, backend committed snapshot and launcher wrapper creation. The profile is checked again after copying. | Participating native writers cannot combine a different profile revision with the copied world. Existing backend online SQLite backup remains separately available and unchanged. |
| Value/output conservation | Typed backend publication status propagates to `published`; a complete inner bundle sets it before optional wrapper work. Cleanup removes only the exclusively claimed unpublished output directory. | Later wrapper or synchronization failure retains independently verifiable published recovery artifacts and does not report completion. No original world/backup is overwritten. |
| Path/alias protection | Rename requires an exact managed `world.json`, rejects linked components and nonprivate/hard-linked profile files. Backup-parent admission protects database, profile, export, witness and stable lease paths. | Existing alias and owned-output checks remain in place. Administrative replacement of filesystem ancestors is explicitly outside the cooperative guarantee. |
| Error and retry | Lock contention names active session or maintenance and asks to close/wait. A retained unlocked marker does not mean ownership remains. | The user can retry after clean or abrupt holder exit without unlinking a lock or deleting an initialization witness. |

## Meaningful regression design

The new `managed_maintenance_coordinates_real_sessions_and_other_maintenance`
fixture creates actual child processes holding either a managed session or a
maintenance guard. It covers both clean exit and abrupt termination. During
ownership, rename, backup and a new session reject promptly; profile, database
and witness bytes remain unchanged, and no backup root is allocated. After
release, rename succeeds and the existing full regional backup/restore path
compares the recovered archive with the original fixture.

`json_world_maintenance_does_not_admit_first_launch_or_create_progress` checks
the independent initialization contract for a never-started JSON world. Rename
changes its title without setting checkpoint state, touching progress or
publishing a witness. The following real session still receives fresh admission.

Existing nested recovery fault injection now also tries to reacquire maintenance
at backend and launcher final-marker boundaries. Those assertions establish
ownership at the actual publication callbacks, while existing byte/recovery
checks distinguish a completed inner bundle from a completed native wrapper.
The rename preservation fixture admits exactly one new stable lock marker and
the selected profile's intended title edit; retained source, progress and backup
content remain compared exactly.

## Boundaries and follow-up

No claim is made here that tests have executed, that Windows locks or filesystem
durability have been qualified, or that an old binary/direct SDK writer joins
the protocol. Those conditions need their own evidence. Existing policies also
exclude arbitrary administrative ancestor/path replacement and unsupported
network-filesystem semantics. This implementation preserves those limits rather
than broadening them by implication.

Actual pass/fail, source/binary identity and any integration corrections belong
in the current [validation record](../../VALIDATION.md). No additional blocker
or new product policy was required by this source review.
